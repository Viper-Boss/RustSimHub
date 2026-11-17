// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_nkn_agent::{Owner, PortError};
use rustsimhub_policy::{Kind, Principal, Scope};
use rustsimhub_wire::NknOwner;
use serde_json::json;
use std::collections::VecDeque;

pub(super) struct Inner {
    pub auth: Arc<Mutex<Auth>>,
    pub root: Arc<rustsimhub_core::nkn_root::Controller>,
    actions: Mutex<Actions>,
    membership: Mutex<Option<Arc<super::membership::InstalledMembership>>>,
    pub wake: tokio::sync::Notify,
    pub closed: std::sync::atomic::AtomicBool,
    usage_cache: Mutex<(String, rustsimhub_usage_stats::query::QueryCache)>,
    participation: super::participation::Service,
}
struct Actions {
    revision: u64,
    receipts: VecDeque<Receipt>,
}
struct Receipt {
    id: String,
    owner: [u8; 32],
    owner_generation: u64,
    realm: String,
    feature: NknOwner,
    enabled: bool,
    expected: u64,
    admitted: u64,
    state: &'static str,
}
struct Caller {
    principal: Principal,
    realm: String,
    instance: String,
}

struct UsageTransport<'a> {
    root: &'a rustsimhub_core::nkn_root::Controller,
    token: &'a str,
}
impl rustsimhub_usage_stats::query::QueryTransport for UsageTransport<'_> {
    fn query_page(
        &mut self,
        request: &rustsimhub_usage_stats::query::QueryRequest,
    ) -> Result<Vec<u8>, rustsimhub_usage_stats::query::TransportError> {
        use rustsimhub_usage_stats::{
            protocol::{Clock, SystemClock, CACHE_TTL_SECS},
            query::TransportError,
        };
        if request.page() != 0 {
            return Err(TransportError::Rejected);
        }
        let count = self
            .root
            .read_usage_count(self.token, request.window(), request.deadline())
            .map_err(|error| match error {
                PortError::Deadline => TransportError::Timeout,
                PortError::Denied => TransportError::Rejected,
                _ => TransportError::Unavailable,
            })?;
        let now = SystemClock
            .now_secs()
            .map_err(|_| TransportError::Unavailable)?;
        let expires = now
            .checked_add(CACHE_TTL_SECS)
            .ok_or(TransportError::Unavailable)?;
        serde_json::to_vec(&json!({"version":request.version(),"window":request.window(),"topic":request.topic(),"page":0,"pages":1,"count":count,"observed_at_secs":now,"expires_at_secs":expires})).map_err(|_|TransportError::Unavailable)
    }
}
impl Inner {
    fn query_usage(&self, token: &str, window: &str) -> rustsimhub_auth::Result<Value> {
        use rustsimhub_usage_stats::{
            protocol::{
                DisclosureAcknowledgement, EnabledPolicy, OwnerConsent, SystemClock, Window,
            },
            query::{QueryClient, QueryOutcome},
        };
        let caller = self.caller(token, false)?;
        let window = match window {
            "24h" => Window::Last24Hours,
            "30d" => Window::Last30Days,
            "90d" => Window::Last90Days,
            _ => return Err(Error::InvalidInput),
        };
        // Same lock as enable/disable: no late query result can cross a completed
        // statistics-off operation. Never hold Auth's mutex during network IO.
        let _actions = self.actions.try_lock().map_err(|_| Error::Busy)?;
        let current = self.caller(token, false)?;
        if current.principal != caller.principal
            || current.realm != caller.realm
            || current.instance != caller.instance
        {
            return Err(Error::Forbidden);
        }
        if !self.root.owners().map_err(error)?.statistics {
            return serde_json::to_value(QueryOutcome::Disabled).map_err(|_| Error::InvalidInput);
        }
        let mut cache = self.usage_cache.try_lock().map_err(|_| Error::Busy)?;
        if cache.0 != caller.realm {
            *cache = (caller.realm.clone(), Default::default());
        }
        // Consent here authorizes the explicit aggregate QUERY only. No
        // check-in manager, seed, subscription adapter or scheduler is created.
        let policy = EnabledPolicy::from_owner_consent(
            OwnerConsent::Granted,
            DisclosureAcknowledgement::Current,
        )
        .map_err(|_| Error::InvalidInput)?;
        let mut query = QueryClient::new(
            UsageTransport {
                root: &self.root,
                token,
            },
            SystemClock,
            cache.1.clone(),
        )
        .map_err(|_| Error::InvalidInput)?;
        let outcome = query.query_window(&policy, window);
        let current = self.caller(token, false)?;
        if current.principal != caller.principal
            || current.realm != caller.realm
            || current.instance != caller.instance
            || self.closed.load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(Error::Forbidden);
        }
        cache.1 = query.into_parts().2;
        serde_json::to_value(outcome).map_err(|_| Error::InvalidInput)
    }
}
fn error(e: PortError) -> Error {
    if e == PortError::Unavailable {
        Error::Busy
    } else {
        Error::Forbidden
    }
}
fn feature(owner: NknOwner) -> Owner {
    match owner {
        NknOwner::ManualEngine => Owner::ManualEngine,
        NknOwner::Remote => Owner::Remote,
        NknOwner::Statistics => Owner::Statistics,
    }
}
impl Receipt {
    fn same_owner(&self, caller: &Caller) -> bool {
        self.owner == caller.principal.public_key
            && self.owner_generation == caller.principal.generation
            && self.realm == caller.realm
    }
    fn view(&self, instance: &str) -> Value {
        json!({"request_id":self.id,"known":true,"state":self.state,"owner":self.feature,"enabled":self.enabled,"expected_revision":self.expected.to_string(),"admitted_revision":self.admitted.to_string(),"core_instance":instance,"not_found_is_not_proof":true})
    }
}
impl Inner {
    #[cfg(test)]
    pub fn new(auth: Arc<Mutex<Auth>>) -> Self {
        Self::with_usage_state(auth, None)
    }
    pub fn with_usage_state(auth: Arc<Mutex<Auth>>, state: Option<&std::path::Path>) -> Self {
        Self {
            root: Arc::new(rustsimhub_core::nkn_root::Controller::new(auth.clone())),
            auth,
            membership: Mutex::new(None),
            actions: Mutex::new(Actions {
                revision: 0,
                receipts: VecDeque::new(),
            }),
            wake: tokio::sync::Notify::new(),
            closed: std::sync::atomic::AtomicBool::new(false),
            usage_cache: Mutex::new((String::new(), Default::default())),
            participation: super::participation::Service::new(state),
        }
    }
    fn caller(&self, token: &str, recent: bool) -> rustsimhub_auth::Result<Caller> {
        let mut auth = self.auth.lock().map_err(|_| Error::Forbidden)?;
        auth.authorize(token, Scope::SystemManage)?;
        let principal = if recent {
            auth.recent_owner(token)?
        } else {
            auth.authorize(token, Scope::SystemManage)?
        };
        if principal.kind != Kind::Owner || !principal.active {
            return Err(Error::Forbidden);
        }
        Ok(Caller {
            principal,
            realm: auth.durable_realm()?,
            instance: auth.observer_instance(),
        })
    }
    fn advance(&self) -> Result<Option<rustsimhub_nkn_agent::Snapshot>, PortError> {
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(PortError::Denied);
        }
        let _actions = self
            .actions
            .try_lock()
            .map_err(|_| PortError::Unavailable)?;
        self.root.drive()
    }
    pub fn request(&self, command: AuthCommand) -> rustsimhub_auth::Result<Value> {
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Error::Forbidden);
        }
        match command {
            command @ (AuthCommand::UsageParticipationStatus { .. }
            | AuthCommand::UsageParticipationSet { .. }
            | AuthCommand::UsageParticipationRequest { .. }) => {
                let _actions = self.actions.try_lock().map_err(|_| Error::Busy)?;
                self.participation.request(&self.auth, command)
            }
            AuthCommand::UsageStatsQuery { token, window } => self.query_usage(&token, &window),
            AuthCommand::NknStatus { token } => {
                self.caller(&token, false)?;
                let actions = self.actions.lock().map_err(|_| Error::Forbidden)?;
                let status = self.root.read_status(&token).map_err(error)?;
                Ok(
                    json!({"available":true,"configuration_api":true,"remote_control_supported":true,"revision":actions.revision.to_string(),"provider":status}),
                )
            }
            AuthCommand::NknRequest {
                token,
                request_id,
                expected_core_instance,
            } => {
                let caller = self.caller(&token, false)?;
                let actions = self.actions.lock().map_err(|_| Error::Forbidden)?;
                if expected_core_instance != caller.instance {
                    return Ok(
                        json!({"request_id":request_id,"known":false,"core_instance":caller.instance,"context_changed":true,"not_found_is_not_proof":true}),
                    );
                }
                if let Some(receipt) = actions.receipts.iter().find(|r| r.id == request_id) {
                    if !receipt.same_owner(&caller) {
                        return Err(Error::Forbidden);
                    }
                    return Ok(receipt.view(&caller.instance));
                }
                Ok(
                    json!({"request_id":request_id,"known":false,"core_instance":caller.instance,"not_found_is_not_proof":true}),
                )
            }
            AuthCommand::SetNknEnabled {
                token,
                owner,
                enabled,
                expected_revision,
                expected_core_instance,
                request_id,
            } => {
                let caller = self.caller(&token, true)?;
                // Even another reference cannot turn a remote NKN token into Root launch
                // authority. This temporary check never replaces the ORIGINAL live proof.
                if enabled {
                    self.auth
                        .try_lock()
                        .map_err(|_| Error::Busy)?
                        .nkn_root_authority(&token)?
                        .invalidate();
                }
                if caller.instance != expected_core_instance {
                    return Err(Error::Forbidden);
                }
                let expected = expected_revision
                    .parse::<u64>()
                    .map_err(|_| Error::InvalidInput)?;
                if expected.to_string() != expected_revision {
                    return Err(Error::InvalidInput);
                }
                let mut actions = self.actions.lock().map_err(|_| Error::Forbidden)?;
                // The original descriptor-owning worker may have queued behind
                // one supervisor observation. Recheck the same caller after it.
                let current_caller = self.caller(&token, true)?;
                if current_caller.principal != caller.principal
                    || current_caller.realm != caller.realm
                    || current_caller.instance != caller.instance
                {
                    return Err(Error::Forbidden);
                }
                if let Some(old) = actions.receipts.iter().find(|r| r.id == request_id) {
                    if !old.same_owner(&caller)
                        || old.feature != owner
                        || old.enabled != enabled
                        || old.expected != expected
                    {
                        return Err(Error::Forbidden);
                    }
                    return Ok(old.view(&caller.instance));
                }
                if actions.revision != expected {
                    return Err(Error::Forbidden);
                }
                if self.closed.load(std::sync::atomic::Ordering::Acquire) {
                    return Err(Error::Forbidden);
                }
                let next = expected
                    .checked_add(1)
                    .filter(|n| *n < i64::MAX as u64)
                    .ok_or(Error::Busy)?;
                let intents = self.root.owners().map_err(error)?;
                let current = match owner {
                    NknOwner::ManualEngine => intents.manual,
                    NknOwner::Remote => intents.remote,
                    NknOwner::Statistics => intents.statistics,
                };
                // Load only on an explicit new Remote reference, after current local
                // Owner checks; never generate or disclose an installation secret.
                if owner == NknOwner::Remote && enabled && !current {
                    let pin = self.auth.try_lock().map_err(|_| Error::Busy)?.public_key();
                    let installed = super::membership::InstalledMembership::load(pin)?;
                    *self.membership.try_lock().map_err(|_| Error::Busy)? =
                        Some(Arc::new(installed));
                }
                // Register ambiguity BEFORE any Root IO. Evicted IDs cannot replay: their
                // old CAS revision remains behind the monotonically admitted revision.
                actions.revision = next;
                actions.receipts.push_back(Receipt {
                    id: request_id,
                    owner: caller.principal.public_key,
                    owner_generation: caller.principal.generation,
                    realm: caller.realm.clone(),
                    feature: owner,
                    enabled,
                    expected,
                    admitted: next,
                    state: "unknown",
                });
                while actions.receipts.len() > 64 {
                    actions.receipts.pop_front();
                }
                let accepted = if current == enabled {
                    true
                } else if enabled {
                    self.root.enable(&token, feature(owner)).is_ok()
                } else {
                    self.root.disable(&token, feature(owner)).is_ok()
                };
                if owner == NknOwner::Remote && (!enabled || !accepted) {
                    self.membership.try_lock().map_err(|_| Error::Busy)?.take();
                }
                let receipt = actions.receipts.back_mut().ok_or(Error::Forbidden)?;
                if accepted {
                    receipt.state = "accepted";
                }
                let result = receipt.view(&caller.instance);
                drop(actions);
                self.wake.notify_one();
                Ok(result)
            }
            _ => Err(Error::Forbidden),
        }
    }
}

// One owner of the original supervisor and original accepted-channel listener.
pub(super) async fn drive(controller: Controller, ports: crate::nkn_business::CorePorts) {
    struct Close(Arc<Inner>);
    impl Drop for Close {
        fn drop(&mut self) {
            self.0
                .closed
                .store(true, std::sync::atomic::Ordering::Release);
            self.0.root.shutdown();
            if let Ok(mut key) = self.0.membership.lock() {
                key.take();
            }
        }
    }
    let _close = Close(controller.inner.clone());
    let root = controller.root();
    let mut suppressed: Option<std::sync::Weak<super::membership::InstalledMembership>> = None;
    loop {
        controller.changed().await;
        loop {
            let host = controller.inner.clone();
            let snapshot = tokio::task::spawn_blocking(move || host.advance()).await;
            let phase = match snapshot {
                Ok(Ok(Some(s))) => s.phase,
                Ok(Err(PortError::Unavailable)) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                }
                _ => break,
            };
            if !matches!(
                phase,
                rustsimhub_nkn_agent::Phase::Starting
                    | rustsimhub_nkn_agent::Phase::Ready
                    | rustsimhub_nkn_agent::Phase::WaitingRetry
                    | rustsimhub_nkn_agent::Phase::Stopping
            ) {
                break;
            }
            let remote = root.owners().is_ok_and(|o| o.remote);
            if !remote {
                suppressed = None;
            }
            if phase == rustsimhub_nkn_agent::Phase::Ready && remote {
                let key = controller
                    .inner
                    .membership
                    .lock()
                    .ok()
                    .and_then(|k| k.clone());
                if let Some(key) = key {
                    let generation = Arc::downgrade(&key);
                    if suppressed
                        .as_ref()
                        .is_some_and(|old| old.ptr_eq(&generation))
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                        continue;
                    }
                    let mut listener = Box::pin(crate::nkn_listener::serve(
                        root.clone(),
                        ports.clone(),
                        key.key.clone(),
                    ));
                    // Fail closed on credential replacement; no reconnect/reopen loop.
                    let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
                    loop {
                        tokio::select! {biased;_=tick.tick()=>{let original=key.clone();let host=controller.inner.clone();if !tokio::task::spawn_blocking(move||original.current()&&matches!(host.advance(),Ok(_)|Err(PortError::Unavailable))).await.unwrap_or(false){break}},_=&mut listener=>break}
                    }
                    // A failed original listener needs OFF then a new explicit ON.
                    // OFF and ON can both complete between driver polls. Suppress
                    // this installed generation only; a new explicit ON installs
                    // another one. Weak keeps no failed credential alive.
                    suppressed = Some(generation);
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
}
