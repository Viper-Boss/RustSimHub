//! Bounded, synchronous, explicitly invoked read-only queries. Adapters must enforce the
//! byte limit and deadline while reading; this crate also checks both on return.
use crate::protocol::{
    Clock, EnabledPolicy, Window, CACHE_TTL_SECS, MIN_REFRESH_SECS, PROTOCOL_VERSION,
    QUERY_TIMEOUT_SECS,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{Duration, Instant};

pub const MAX_PAGE_BYTES: usize = 2048;
pub const MAX_PAGES: u16 = 16;
pub const MAX_COUNT: u64 = 1_000_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportError {
    Offline,
    Timeout,
    Rejected,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryError {
    Offline,
    Timeout,
    Rejected,
    Unavailable,
    ClockUnavailable,
    ClockRollback,
    Throttled,
    Oversized,
    InvalidResponse,
    WrongVersion,
    WrongTopic,
    WrongWindow,
    InvalidPage,
    InconsistentPages,
    ExpiredResponse,
    InvalidTime,
    CountOverflow,
    CountOutOfRange,
    InvalidCache,
}
impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "usage statistics query: {self:?}")
    }
}
impl std::error::Error for QueryError {}
impl From<TransportError> for QueryError {
    fn from(value: TransportError) -> Self {
        match value {
            TransportError::Offline => Self::Offline,
            TransportError::Timeout => Self::Timeout,
            TransportError::Rejected => Self::Rejected,
            TransportError::Unavailable => Self::Unavailable,
        }
    }
}

/// A finite request; no endpoint, executable, device path or key material.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct QueryRequest {
    version: u16,
    window: Window,
    topic: &'static str,
    page: u16,
    max_bytes: usize,
    timeout_secs: u64,
    #[serde(skip)]
    deadline: Instant,
}
impl QueryRequest {
    pub fn version(&self) -> u16 {
        self.version
    }
    pub fn window(&self) -> Window {
        self.window
    }
    pub fn topic(&self) -> &'static str {
        self.topic
    }
    pub fn page(&self) -> u16 {
        self.page
    }
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }
    /// The same process-local monotonic deadline is passed to every page.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}
pub trait QueryTransport {
    /// Return one bounded response. Implementors must honour timeout_secs and
    /// max_bytes during I/O. No concrete networking implementation is supplied.
    fn query_page(&mut self, request: &QueryRequest) -> Result<Vec<u8>, TransportError>;
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    version: u16,
    window: Window,
    topic: String,
    page: u16,
    pages: u16,
    count: u64,
    observed_at_secs: u64,
    expires_at_secs: u64,
}
impl Page {
    fn parse(bytes: &[u8], request: &QueryRequest, now: u64) -> Result<Self, QueryError> {
        if bytes.len() > MAX_PAGE_BYTES {
            return Err(QueryError::Oversized);
        }
        let value: Self = serde_json::from_slice(bytes).map_err(|_| QueryError::InvalidResponse)?;
        if value.version != PROTOCOL_VERSION {
            return Err(QueryError::WrongVersion);
        }
        if value.window != request.window {
            return Err(QueryError::WrongWindow);
        }
        if value.topic != request.topic {
            return Err(QueryError::WrongTopic);
        }
        if value.page != request.page
            || value.pages == 0
            || value.pages > MAX_PAGES
            || value.page >= value.pages
        {
            return Err(QueryError::InvalidPage);
        }
        if value.observed_at_secs > now
            || value.expires_at_secs <= value.observed_at_secs
            || value.expires_at_secs - value.observed_at_secs > CACHE_TTL_SECS
        {
            return Err(QueryError::InvalidTime);
        }
        if value.expires_at_secs <= now {
            return Err(QueryError::ExpiredResponse);
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuerySource {
    ReadOnlyTransport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheEntry {
    window: Window,
    count: u64,
    source: QuerySource,
    observed_at_secs: u64,
    succeeded_at_secs: u64,
    expires_at_secs: u64,
    freshness_revoked: bool,
}
impl CacheEntry {
    pub fn window(&self) -> Window {
        self.window
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn source(&self) -> QuerySource {
        self.source
    }
    pub fn observed_at_secs(&self) -> u64 {
        self.observed_at_secs
    }
    pub fn succeeded_at_secs(&self) -> u64 {
        self.succeeded_at_secs
    }
    pub fn expires_at_secs(&self) -> u64 {
        self.expires_at_secs
    }
    pub fn freshness(&self, now: u64) -> Freshness {
        if self.freshness_revoked || now < self.succeeded_at_secs {
            Freshness::ClockRollback
        } else if now >= self.expires_at_secs {
            Freshness::Expired
        } else {
            Freshness::WithinTtl
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    WithinTtl,
    Expired,
    ClockRollback,
}

/// Local view DTO. Cached counts always carry a reason and their original time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QueryOutcome {
    Disabled,
    Success {
        entry: CacheEntry,
    },
    Cached {
        entry: CacheEntry,
        freshness: Freshness,
        reason: QueryError,
    },
    Unknown {
        reason: QueryError,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryCache {
    high_water_secs: u64,
    entries: [Option<CacheEntry>; 3],
    last_attempts: [Option<u64>; 3],
}
impl QueryCache {
    pub fn entry(&self, window: Window) -> Option<&CacheEntry> {
        self.entries[window.index()].as_ref()
    }
    pub fn high_water_secs(&self) -> u64 {
        self.high_water_secs
    }
    pub(crate) fn validate(&self) -> Result<(), QueryError> {
        for window in Window::ALL {
            if let Some(entry) = self.entry(window) {
                if entry.window != window
                    || entry.count > MAX_COUNT
                    || entry.observed_at_secs > entry.succeeded_at_secs
                    || entry.expires_at_secs <= entry.succeeded_at_secs
                    || entry
                        .expires_at_secs
                        .checked_sub(entry.observed_at_secs)
                        .is_none_or(|ttl| ttl > CACHE_TTL_SECS)
                    || entry.succeeded_at_secs > self.high_water_secs
                {
                    return Err(QueryError::InvalidCache);
                }
            }
            if self.last_attempts[window.index()]
                .is_some_and(|attempt| attempt > self.high_water_secs)
            {
                return Err(QueryError::InvalidCache);
            }
        }
        Ok(())
    }
    fn observe(&mut self, now: u64) -> Result<(), QueryError> {
        if now < self.high_water_secs {
            for entry in self.entries.iter_mut().flatten() {
                entry.freshness_revoked = true;
            }
            return Err(QueryError::ClockRollback);
        }
        self.high_water_secs = now;
        Ok(())
    }
    fn fallback(&self, window: Window, now: Option<u64>, reason: QueryError) -> QueryOutcome {
        match self.entry(window) {
            Some(entry) => QueryOutcome::Cached {
                entry: entry.clone(),
                freshness: now.map_or(Freshness::ClockRollback, |now| entry.freshness(now)),
                reason,
            },
            None => QueryOutcome::Unknown { reason },
        }
    }
}

/// Explicit calls only: constructing this object creates no identity, files,
/// queue, timer, worker, or network activity. Persist cache through FileStore.
pub struct QueryClient<T, C> {
    transport: T,
    clock: C,
    cache: QueryCache,
}
impl<T: QueryTransport, C: Clock> QueryClient<T, C> {
    pub fn new(transport: T, clock: C, cache: QueryCache) -> Result<Self, QueryError> {
        cache.validate()?;
        Ok(Self {
            transport,
            clock,
            cache,
        })
    }
    pub fn cache(&self) -> &QueryCache {
        &self.cache
    }
    pub fn into_parts(self) -> (T, C, QueryCache) {
        (self.transport, self.clock, self.cache)
    }
    pub fn query_all(&mut self, policy: &EnabledPolicy) -> [QueryOutcome; 3] {
        Window::ALL.map(|window| self.query_window(policy, window))
    }
    pub fn query_window(&mut self, policy: &EnabledPolicy, window: Window) -> QueryOutcome {
        if !policy.is_enabled() {
            return QueryOutcome::Disabled;
        }
        let now = match self.clock.now_secs() {
            Ok(now) => now,
            Err(_) => {
                return self
                    .cache
                    .fallback(window, None, QueryError::ClockUnavailable)
            }
        };
        if let Err(error) = self.cache.observe(now) {
            return self.cache.fallback(window, Some(now), error);
        }
        if self.cache.last_attempts[window.index()]
            .is_some_and(|last| now.saturating_sub(last) < MIN_REFRESH_SECS)
        {
            return self
                .cache
                .fallback(window, Some(now), QueryError::Throttled);
        }
        self.cache.last_attempts[window.index()] = Some(now);
        match self.fetch(window, now) {
            Ok(entry) => {
                self.cache.entries[window.index()] = Some(entry.clone());
                QueryOutcome::Success { entry }
            }
            Err(error) => {
                // A high-water mark records a previous observation, not current
                // time. If the post-I/O clock read failed, it cannot prove TTL.
                let now =
                    (error != QueryError::ClockUnavailable).then_some(self.cache.high_water_secs);
                self.cache.fallback(window, now, error)
            }
        }
    }
    fn fetch(&mut self, window: Window, start: u64) -> Result<CacheEntry, QueryError> {
        let monotonic_deadline = Instant::now()
            .checked_add(Duration::from_secs(QUERY_TIMEOUT_SECS))
            .ok_or(QueryError::InvalidTime)?;
        let deadline = start
            .checked_add(QUERY_TIMEOUT_SECS)
            .ok_or(QueryError::InvalidTime)?;
        let mut total = 0u64;
        let mut expected = None;
        let mut last_now = start;
        for page in 0..MAX_PAGES {
            let monotonic_remaining = monotonic_deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
                .ok_or(QueryError::Timeout)?;
            let remaining = deadline
                .checked_sub(last_now)
                .filter(|remaining| *remaining > 0)
                .ok_or(QueryError::Timeout)?;
            let request = QueryRequest {
                version: PROTOCOL_VERSION,
                topic: window.topic(),
                window,
                page,
                max_bytes: MAX_PAGE_BYTES,
                timeout_secs: remaining.min(
                    monotonic_remaining.as_secs()
                        + u64::from(monotonic_remaining.subsec_nanos() > 0),
                ),
                deadline: monotonic_deadline,
            };
            let result = self.transport.query_page(&request);
            let now = self
                .clock
                .now_secs()
                .map_err(|_| QueryError::ClockUnavailable)?;
            self.cache.observe(now)?;
            if now >= deadline || Instant::now() >= monotonic_deadline {
                return Err(QueryError::Timeout);
            }
            last_now = now;
            let value = Page::parse(&result.map_err(QueryError::from)?, &request, now)?;
            let metadata = (value.pages, value.observed_at_secs, value.expires_at_secs);
            if expected.is_some_and(|expected| expected != metadata) {
                return Err(QueryError::InconsistentPages);
            }
            expected = Some(metadata);
            total = total
                .checked_add(value.count)
                .ok_or(QueryError::CountOverflow)?;
            if page + 1 == value.pages {
                if total > MAX_COUNT {
                    return Err(QueryError::CountOutOfRange);
                }
                return Ok(CacheEntry {
                    window,
                    count: total,
                    source: QuerySource::ReadOnlyTransport,
                    observed_at_secs: value.observed_at_secs,
                    succeeded_at_secs: now,
                    expires_at_secs: value.expires_at_secs,
                    freshness_revoked: false,
                });
            }
        }
        Err(QueryError::InvalidPage)
    }
}
