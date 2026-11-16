// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit, bounded check-in steps. No scheduler, SDK, network fallback or signer.
//! A durable pending record is always query-only after restart. The adapter must
//! prepare the real transaction without submitting it and honor the I/O budget.
use crate::{
    protocol::{
        CheckInIntent, Clock, EnabledPolicy, StatsPublicKey, Window, QUERY_TIMEOUT_SECS,
        RENEW_INTERVAL_SECS, RETRY_AFTER_SECS,
    },
    query::TransportError,
    store::{FileStore, LastSuccess, PendingTransaction, StoreError, StoredState},
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const MAX_REPLY: usize = 2048;
const MAX_PENDING_AGE: u64 = RENEW_INTERVAL_SECS;

/// No resource is acquired until an explicit enabled step actually needs it.
/// Only this module can mint a lease. Adapters release this statistics owner's
/// reference, never stop the shared engine or another owner's remote session.
pub struct StatisticsLease([u8; 16]);
impl StatisticsLease {
    pub fn id(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone)]
pub struct Cancellation(Arc<Control>);
struct Control {
    state: AtomicU64,
    commit: Mutex<()>,
}
impl Cancellation {
    /// Local fence, serialized with a confirmation commit. No lock is held
    /// during a transport call. The driving step/set_policy persists the
    /// disable generation; cancelling does no network/FS/Root operation itself.
    pub fn cancel(&self) {
        let _commit = self.0.commit.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self
            .0
            .state
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| {
                Some(if v >= u64::MAX - 2 {
                    u64::MAX - 1
                } else {
                    (v / 2 + 1) * 2
                })
            });
    }
}

// A clone retains the EXACT deadline and original cancellation/context. It
// cannot mint, extend or re-enable an attempt. Production ports may retain this
// read-only fence while the original Machine owns the attempt and lease.
#[derive(Clone)]
pub struct AttemptBudget {
    deadline: Instant,
    cancellation: Cancellation,
    context: u64,
}
impl AttemptBudget {
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn max_reply_bytes(&self) -> usize {
        MAX_REPLY
    }
    pub fn cancelled(&self) -> bool {
        self.cancellation.0.state.load(Ordering::SeqCst) != self.context
    }
    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

pub trait CheckInStore {
    fn public_key(&self) -> StatsPublicKey;
    fn load(&self) -> Result<StoredState, StoreError>;
    fn save(&self, state: &StoredState) -> Result<(), StoreError>;
}
impl CheckInStore for FileStore {
    fn public_key(&self) -> StatsPublicKey {
        self.identity().public_key()
    }
    fn load(&self) -> Result<StoredState, StoreError> {
        FileStore::load(self)
    }
    fn save(&self, state: &StoredState) -> Result<(), StoreError> {
        FileStore::save(self, state)
    }
}

pub trait CheckInTransport {
    /// All methods are explicit, use this exact reference and must honor the
    /// same monotonic deadline/cancellation during I/O, not only on return.
    /// Partial acquire failure must be cleaned up by release_statistics too.
    fn acquire_statistics(
        &mut self,
        lease: &StatisticsLease,
        budget: &AttemptBudget,
    ) -> Result<(), TransportError>;
    fn release_statistics(&mut self, lease: StatisticsLease);
    /// Pure transaction preparation, NOT a network submission. A real SDK
    /// adapter supplies the real transaction digest, never an intent hash
    /// masquerading as a chain transaction. No signer/seed export is added here.
    fn prepare(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        budget: &AttemptBudget,
    ) -> Result<[u8; 32], TransportError>;
    /// Submit the already prepared EXACT transaction once. Ok means accepted
    /// by the adapter, never chain-confirmed. Every error remains ambiguous.
    fn submit(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        budget: &AttemptBudget,
    ) -> Result<(), TransportError>;
    /// Query ONLY this original digest and intent. A reply is a bounded strict
    /// JSON {intent,digest,status}; SDK/chain evidence authenticity belongs to
    /// the approved adapter, not to the fact that JSON passed local parsing.
    fn query(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        budget: &AttemptBudget,
    ) -> Result<Vec<u8>, TransportError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Store(StoreError),
    Clock,
    ClockRollback,
    Protocol,
    Exhausted,
    Poisoned,
    InvalidBudget,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statistics check-in: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    Prepare,
    Transport,
    InvalidReply,
    Deadline,
    ExpiredUnresolved,
    AlreadyAttempted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Outcome {
    Disabled,
    Due { not_before: u64 },
    Prepared { digest: [u8; 32] },
    Pending { digest: [u8; 32] },
    Confirmed { period: u64 },
    Failed { reason: Failure },
    Unknown { digest: [u8; 32], reason: Failure },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    intent: CheckInIntent,
    digest: [u8; 32],
    status: Verdict,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Verdict {
    Pending,
    Confirmed,
    Rejected,
}
#[derive(Clone, Copy)]
struct Ready {
    intent: CheckInIntent,
    digest: [u8; 32],
    context: u64,
    deadline: Instant,
}
struct FixedClock(u64);
impl Clock for FixedClock {
    fn now_secs(&self) -> Result<u64, crate::protocol::ClockError> {
        Ok(self.0)
    }
}

pub struct CheckInMachine<S: CheckInStore, T: CheckInTransport, C: Clock> {
    store: S,
    transport: T,
    clock: C,
    policy: EnabledPolicy,
    cancellation: Cancellation,
    active: bool,
    poisoned: bool,
    lease: Option<StatisticsLease>,
    ready: [Option<Ready>; 3],
    next_action: [u64; 3],
    last_clock: Option<u64>,
    budget: Duration,
}
impl<S: CheckInStore, T: CheckInTransport, C: Clock> CheckInMachine<S, T, C> {
    /// Construction reads neither clock nor store and acquires no resource.
    /// The supplied store must have been explicitly opened by its owner.
    pub fn new(store: S, transport: T, clock: C) -> Self {
        Self {
            store,
            transport,
            clock,
            policy: EnabledPolicy::disabled(),
            cancellation: Cancellation(Arc::new(Control {
                state: AtomicU64::new(0),
                commit: Mutex::new(()),
            })),
            active: false,
            poisoned: false,
            lease: None,
            ready: [None; 3],
            next_action: [0; 3],
            last_clock: None,
            budget: Duration::from_secs(QUERY_TIMEOUT_SECS),
        }
    }
    /// May only shorten the fixed ten-second maximum; no background work.
    pub fn with_budget(mut self, budget: Duration) -> Result<Self, Error> {
        if budget.is_zero() || budget > Duration::from_secs(QUERY_TIMEOUT_SECS) {
            return Err(Error::InvalidBudget);
        }
        self.budget = budget;
        Ok(self)
    }
    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }
    fn release(&mut self) {
        if let Some(lease) = self.lease.take() {
            self.transport.release_statistics(lease);
        }
    }
    fn load(&mut self) -> Result<StoredState, Error> {
        self.store
            .load()
            .and_then(|s| {
                s.validate()?;
                Ok(s)
            })
            .map_err(|e| {
                self.poisoned = true;
                Error::Store(e)
            })
    }
    fn save(&mut self, state: &StoredState) -> Result<(), Error> {
        self.store.save(state).map_err(|e| {
            self.poisoned = true;
            self.ready = [None; 3];
            Error::Store(e)
        })
    }
    fn flush_disabled(&mut self) -> Result<Outcome, Error> {
        self.policy = EnabledPolicy::disabled();
        self.ready = [None; 3];
        self.release();
        if self.poisoned {
            self.active = false;
            return Err(Error::Poisoned);
        }
        if self.active {
            // Never clear a pending digest or cached result on disable.
            let mut state = self.load()?;
            state.disabled_generation =
                state.disabled_generation.checked_add(1).ok_or_else(|| {
                    self.poisoned = true;
                    Error::Exhausted
                })?;
            self.save(&state)?;
            self.active = false;
        }
        Ok(Outcome::Disabled)
    }
    pub fn set_policy(&mut self, policy: EnabledPolicy) -> Result<(), Error> {
        if !policy.is_enabled() {
            self.cancellation.cancel();
            self.flush_disabled()?;
            return Ok(());
        }
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let context = self.cancellation.0.state.load(Ordering::SeqCst);
        if context & 1 != 0 {
            return Ok(());
        }
        if self.active {
            self.flush_disabled()?;
        }
        // A racing cancellation while loading must not be overwritten by a
        // stale grant. A fresh explicit policy call is needed to enable again.
        self.load()?;
        if context >= u64::MAX - 2 {
            return Err(Error::Exhausted);
        }
        if self
            .cancellation
            .0
            .state
            .compare_exchange(context, context + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            self.flush_disabled()?;
            return Ok(());
        }
        self.active = true;
        self.policy = policy;
        self.ready = [None; 3];
        Ok(())
    }
    fn now(&mut self) -> Result<u64, Error> {
        let now = self.clock.now_secs().map_err(|_| Error::Clock)?;
        if self.last_clock.is_some_and(|last| now < last) {
            self.ready = [None; 3];
            return Err(Error::ClockRollback);
        }
        self.last_clock = Some(now);
        Ok(now)
    }
    fn valid_return(&mut self, budget: &AttemptBudget, started: u64) -> Result<bool, Error> {
        let now = self.now()?;
        Ok(!budget.expired()
            && now
                .checked_sub(started)
                .is_some_and(|elapsed| Duration::from_secs(elapsed) < self.budget))
    }
    fn acquire(&mut self, budget: &AttemptBudget) -> Result<(), TransportError> {
        if budget.cancelled() || budget.expired() {
            return Err(TransportError::Timeout);
        }
        if self.lease.is_some() {
            return Ok(());
        }
        let mut id = [0; 16];
        getrandom::fill(&mut id).map_err(|_| TransportError::Unavailable)?;
        self.lease = Some(StatisticsLease(id));
        let result = self
            .transport
            .acquire_statistics(self.lease.as_ref().expect("lease just assigned"), budget);
        if result.is_err() {
            self.release();
        }
        result
    }
    pub fn advance(&mut self, window: Window) -> Result<Outcome, Error> {
        let context = self.cancellation.0.state.load(Ordering::SeqCst);
        if context & 1 == 0 {
            return self.flush_disabled();
        }
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let budget = AttemptBudget {
            deadline: Instant::now()
                .checked_add(self.budget)
                .ok_or(Error::InvalidBudget)?,
            cancellation: self.cancellation.clone(),
            context,
        };
        let result = self.step(window, &budget);
        if budget.cancelled() {
            return self.flush_disabled();
        }
        if result.is_err() {
            self.ready = [None; 3];
            self.release();
        }
        result
    }
    fn step(&mut self, window: Window, budget: &AttemptBudget) -> Result<Outcome, Error> {
        let now = self.now()?;
        let mut state = self.load()?;
        let index = window.index();
        let saved = &state.windows[index];
        if !self.valid_return(budget, now)? || budget.cancelled() {
            return Ok(match &saved.pending {
                Some(pending) => Outcome::Unknown {
                    digest: pending.digest,
                    reason: Failure::Deadline,
                },
                None => Outcome::Failed {
                    reason: Failure::Deadline,
                },
            });
        }
        if saved
            .period
            .is_some_and(|period| now / RENEW_INTERVAL_SECS < period)
            || saved.pending.as_ref().is_some_and(|p| now < p.prepared_at)
            || saved
                .last_success
                .as_ref()
                .is_some_and(|s| now < s.confirmed_at)
        {
            return Err(Error::ClockRollback);
        }
        if let Some(pending) = saved.pending.clone() {
            let intent = self
                .policy
                .check_in_intent(
                    window,
                    self.store.public_key(),
                    &FixedClock(pending.prepared_at),
                )
                .map_err(|_| Error::Protocol)?
                .ok_or(Error::Protocol)?;
            if Some(intent.renewal_period()) != saved.period {
                return Err(Error::Protocol);
            }
            // Take before invoking foreign code: panic/lost response never
            // leaves another submit-capable ticket in this process.
            if let Some(ready) = self.ready[index].take() {
                if ready.context == budget.context
                    && ready.digest == pending.digest
                    && ready.intent == intent
                    && intent.renewal_period() == now / RENEW_INTERVAL_SECS
                    && Instant::now() < ready.deadline
                {
                    if !self.valid_return(budget, now)? || budget.cancelled() {
                        return Ok(Outcome::Unknown {
                            digest: pending.digest,
                            reason: Failure::Deadline,
                        });
                    }
                    self.next_action[index] = now.saturating_add(RETRY_AFTER_SECS);
                    if self.acquire(budget).is_err() {
                        return Ok(Outcome::Unknown {
                            digest: pending.digest,
                            reason: Failure::Transport,
                        });
                    }
                    if budget.cancelled() || !self.valid_return(budget, now)? {
                        return Ok(Outcome::Unknown {
                            digest: pending.digest,
                            reason: Failure::Deadline,
                        });
                    }
                    let result = self.transport.submit(
                        self.lease.as_ref().expect("acquired"),
                        &intent,
                        pending.digest,
                        budget,
                    );
                    let timely = self.valid_return(budget, now)?;
                    return Ok(if timely && result.is_ok() {
                        Outcome::Pending {
                            digest: pending.digest,
                        }
                    } else {
                        Outcome::Unknown {
                            digest: pending.digest,
                            reason: if timely {
                                Failure::Transport
                            } else {
                                Failure::Deadline
                            },
                        }
                    });
                }
            }
            if now
                < self.next_action[index].max(pending.prepared_at.saturating_add(RETRY_AFTER_SECS))
            {
                return Ok(Outcome::Due {
                    not_before: self.next_action[index]
                        .max(pending.prepared_at.saturating_add(RETRY_AFTER_SECS)),
                });
            }
            self.next_action[index] = now.saturating_add(RETRY_AFTER_SECS);
            if self.acquire(budget).is_err() {
                return Ok(Outcome::Unknown {
                    digest: pending.digest,
                    reason: Failure::Transport,
                });
            }
            if budget.cancelled() || !self.valid_return(budget, now)? {
                return Ok(Outcome::Unknown {
                    digest: pending.digest,
                    reason: Failure::Deadline,
                });
            }
            let reply = self.transport.query(
                self.lease.as_ref().expect("acquired"),
                &intent,
                pending.digest,
                budget,
            );
            if !self.valid_return(budget, now)? {
                return Ok(Outcome::Unknown {
                    digest: pending.digest,
                    reason: Failure::Deadline,
                });
            }
            if budget.cancelled() {
                return Ok(Outcome::Disabled);
            }
            let Ok(bytes) = reply else {
                return Ok(Outcome::Unknown {
                    digest: pending.digest,
                    reason: Failure::Transport,
                });
            };
            let response = (bytes.len() <= MAX_REPLY)
                .then(|| serde_json::from_slice::<Reply>(&bytes).ok())
                .flatten();
            let Some(response) =
                response.filter(|r| r.digest == pending.digest && r.intent == intent)
            else {
                return Ok(Outcome::Unknown {
                    digest: pending.digest,
                    reason: Failure::InvalidReply,
                });
            };
            match response.status {
                Verdict::Pending => Ok(
                    if now.saturating_sub(pending.prepared_at) >= MAX_PENDING_AGE {
                        Outcome::Unknown {
                            digest: pending.digest,
                            reason: Failure::ExpiredUnresolved,
                        }
                    } else {
                        Outcome::Pending {
                            digest: pending.digest,
                        }
                    },
                ),
                Verdict::Confirmed | Verdict::Rejected => {
                    let confirmed = matches!(response.status, Verdict::Confirmed);
                    state.windows[index].pending = None;
                    if confirmed {
                        state.windows[index].last_success = Some(LastSuccess {
                            period: intent.renewal_period(),
                            confirmed_at: self.last_clock.ok_or(Error::Clock)?,
                        });
                    }
                    // Linearize adoption with cancellation. A cancel which
                    // returned before this lock prevents all result writes.
                    let control = self.cancellation.0.clone();
                    let _commit = control.commit.lock().map_err(|_| Error::Poisoned)?;
                    if budget.cancelled() || budget.expired() {
                        return Ok(Outcome::Disabled);
                    }
                    self.save(&state)?;
                    drop(_commit);
                    if !self.valid_return(budget, now)? {
                        return Ok(Outcome::Unknown {
                            digest: pending.digest,
                            reason: Failure::Deadline,
                        });
                    }
                    Ok(if confirmed {
                        Outcome::Confirmed {
                            period: intent.renewal_period(),
                        }
                    } else {
                        Outcome::Failed {
                            reason: Failure::AlreadyAttempted,
                        }
                    })
                }
            }
        } else {
            if saved.period == Some(now / RENEW_INTERVAL_SECS) {
                return Ok(
                    if saved
                        .last_success
                        .as_ref()
                        .is_some_and(|s| Some(s.period) == saved.period)
                    {
                        Outcome::Confirmed {
                            period: now / RENEW_INTERVAL_SECS,
                        }
                    } else {
                        Outcome::Failed {
                            reason: Failure::AlreadyAttempted,
                        }
                    },
                );
            }
            if now < self.next_action[index] {
                return Ok(Outcome::Due {
                    not_before: self.next_action[index],
                });
            }
            // All windows share one signing identity and its chain nonce.
            // A prepared or ambiguously submitted transaction must be resolved
            // by its original digest before any window prepares another write.
            // Period expiry, disable/re-enable and restart do not free a nonce.
            if state.windows.iter().any(|window| window.pending.is_some()) {
                return Ok(Outcome::Due {
                    not_before: now.saturating_add(RETRY_AFTER_SECS),
                });
            }
            self.next_action[index] = now.saturating_add(RETRY_AFTER_SECS);
            let intent = self
                .policy
                .check_in_intent(window, self.store.public_key(), &FixedClock(now))
                .map_err(|_| Error::Protocol)?
                .ok_or(Error::Protocol)?;
            if self.acquire(budget).is_err() {
                return Ok(Outcome::Failed {
                    reason: Failure::Prepare,
                });
            }
            if budget.cancelled() || !self.valid_return(budget, now)? {
                return Ok(Outcome::Failed {
                    reason: Failure::Deadline,
                });
            }
            let digest =
                self.transport
                    .prepare(self.lease.as_ref().expect("acquired"), &intent, budget);
            if !self.valid_return(budget, now)? || budget.cancelled() {
                return Ok(Outcome::Failed {
                    reason: Failure::Deadline,
                });
            }
            let Ok(digest) = digest else {
                return Ok(Outcome::Failed {
                    reason: Failure::Prepare,
                });
            };
            if digest == [0; 32] {
                return Ok(Outcome::Failed {
                    reason: Failure::Prepare,
                });
            }
            state.windows[index].period = Some(intent.renewal_period());
            state.windows[index].pending = Some(PendingTransaction {
                digest,
                prepared_at: now,
            });
            self.save(&state)?;
            if budget.cancelled() || !self.valid_return(budget, now)? {
                return Ok(Outcome::Unknown {
                    digest,
                    reason: Failure::Deadline,
                });
            }
            self.ready[index] = Some(Ready {
                intent,
                digest,
                context: budget.context,
                deadline: budget.deadline,
            });
            Ok(Outcome::Prepared { digest })
        }
    }
}
impl<S: CheckInStore, T: CheckInTransport, C: Clock> Drop for CheckInMachine<S, T, C> {
    fn drop(&mut self) {
        self.release();
    }
}
