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
    pub fn new(auth: Arc<Mutex<Auth>>) -> Self {
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
        }
    }
    fn caller(&self, token: &str, recent: bool) -> rustsimhub_auth::Result<Caller> {
        let mut auth = self.auth.try_lock().map_err(|_| Error::Busy)?;
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
    pub fn request(&self, command: AuthCommand) -> rustsimhub_auth::Result<Value> {
        if self.closed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(Error::Forbidden);
        }
        match command {
            AuthCommand::NknStatus { token } => {
                self.caller(&token, false)?;
                let actions = self.actions.try_lock().map_err(|_| Error::Busy)?;
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
                let actions = self.actions.try_lock().map_err(|_| Error::Busy)?;
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
                let mut actions = self.actions.try_lock().map_err(|_| Error::Busy)?;
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
    let mut suppressed = false;
    loop {
        controller.changed().await;
        loop {
            let host = root.clone();
            let snapshot = tokio::task::spawn_blocking(move || host.drive()).await;
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
                suppressed = false;
            }
            if !suppressed && phase == rustsimhub_nkn_agent::Phase::Ready && remote {
                let key = controller
                    .inner
                    .membership
                    .lock()
                    .ok()
                    .and_then(|k| k.clone());
                if let Some(key) = key {
                    let mut listener = Box::pin(crate::nkn_listener::serve(
                        root.clone(),
                        ports.clone(),
                        key.key.clone(),
                    ));
                    // Fail closed on credential replacement; no reconnect/reopen loop.
                    let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
                    loop {
                        tokio::select! {biased;_=tick.tick()=>{let original=key.clone();if !tokio::task::spawn_blocking(move||original.current()).await.unwrap_or(false){break}},_=&mut listener=>break}
                    }
                    // A failed original listener needs OFF then a new explicit ON.
                    suppressed = true;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
}
