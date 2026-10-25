// SPDX-License-Identifier: AGPL-3.0-only
//! Core-owned, on-demand management. Passive by default; explicit management
//! mode invokes finite Root jobs. No forwarding loop or startup poller in Core.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_forwarder::{Binding, Policy, PolicyEdit, Queue};
use rustsimhub_storage::Handle;
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

type Result<T> = rustsimhub_auth::Result<T>;
pub(crate) mod ipc;
mod quick;
mod service;
fn denied<T>(result: rustsimhub_forwarder::Result<T>) -> Result<T> {
    result.map_err(|_| Error::Forbidden)
}

#[derive(Clone)]
pub(crate) struct Controller(Arc<Mutex<Management>>, Option<Arc<service::Coordinator>>);
struct Management {
    // Parent is the already-private business state directory, not a browser path.
    directory: PathBuf,
    reader: Handle,
    queue: Option<Queue>,
    worker: Option<ipc::Session>,
    quick: Option<String>,
    quick_stop: Option<(String, bool)>,
}
impl Controller {
    /// Does not create files, start a task, read SMS or touch an interface.
    pub fn new(state: &Path, reader: Handle) -> Self {
        Self(
            Arc::new(Mutex::new(Management {
                directory: state.join("forwarding"),
                reader,
                queue: None,
                worker: None,
                quick: None,
                quick_stop: None,
            })),
            None,
        )
    }
    pub fn with_service_management(mut self) -> Self {
        self.1 = Some(Arc::new(service::Coordinator::production()));
        self
    }
    pub fn service_management(&self) -> bool {
        self.1.is_some()
    }
    #[cfg(feature = "dev-fixtures")]
    pub(crate) fn with_fixture_management(mut self, path: &Path) -> Result<Self> {
        self.1 = Some(Arc::new(service::Coordinator::fixture(path)?));
        Ok(self)
    }
    pub async fn request(&self, auth: Arc<Mutex<Auth>>, command: AuthCommand) -> Result<Value> {
        if let AuthCommand::ForwardingQuickTest {
            token,
            expected_generation,
            request_id,
            target_id,
        } = command
        {
            return quick::start(
                self,
                auth,
                token,
                expected_generation,
                request_id,
                target_id,
            )
            .await;
        }
        if let Some(coordinator) = self.1.as_ref() {
            if let Some(action) = service::Action::from_command(&command) {
                return coordinator.request(self, auth, command, action).await;
            }
        }
        self.passive_request(auth, command).await
    }
    async fn passive_request(&self, auth: Arc<Mutex<Auth>>, command: AuthCommand) -> Result<Value> {
        let state = self.0.clone();
        tokio::task::spawn_blocking(move || {
            // All control/queue operations use the SAME order. Never block the
            // async executor or wait on a mutex held by an HTTP request.
            let mut auth = auth.try_lock().map_err(|_| Error::Busy)?;
            let mut state = state.try_lock().map_err(|_| Error::Busy)?;
            state.dispatch(&mut auth, command)
        })
        .await
        .map_err(|_| Error::Forbidden)?
    }
}
impl Management {
    fn existing(&mut self, binding: Binding) -> Result<Option<&mut Queue>> {
        if self.queue.is_none() {
            match fs::symlink_metadata(&self.directory) {
                Ok(_) => self.queue = Some(denied(Queue::open(&self.directory, binding.clone()))?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(Error::Forbidden),
            }
        }
        if let Some(q) = self.queue.as_ref() {
            // Includes generations after Owner recovery. Never adopt old jobs.
            denied(q.require_binding(&binding))?;
        }
        Ok(self.queue.as_mut())
    }
    fn create(&mut self, binding: Binding) -> Result<&mut Queue> {
        if self.existing(binding.clone())?.is_none() {
            let parent = self.directory.parent().ok_or(Error::Forbidden)?;
            let m = fs::symlink_metadata(parent).map_err(|_| Error::Forbidden)?;
            if !m.is_dir()
                || m.uid() != rustix::process::geteuid().as_raw()
                || m.mode() & 0o777 != 0o700
            {
                return Err(Error::Forbidden);
            }
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&self.directory)
                .map_err(|_| Error::Forbidden)?;
            self.queue = Some(denied(Queue::open(&self.directory, binding))?);
        }
        self.queue.as_mut().ok_or(Error::Forbidden)
    }
    fn status(&mut self, binding: Binding) -> Result<Value> {
        let mut value = match self.existing(binding)? {
            Some(q) => {
                let mut v =
                    serde_json::to_value(denied(q.status())?).map_err(|_| Error::Forbidden)?;
                v["configured"] = true.into();
                v
            }
            None => json!({"configured":false,"enabled":false,"generation":0,
                "queued":0,"in_flight":0,"unknown":0,"process_state_verified":false}),
        };
        // Intent is deliberately separate from observed service/process state.
        value["available"] = true.into();
        value["configuration_api"] = true.into();
        value["retained_credentials_api"] = true.into();
        value["request_receipt_api"] = true.into();
        value["synthetic_test_api"] = true.into();
        value["quick_test_api"] = false.into();
        value["quick_test_active"] = self.quick.is_some().into();
        value["delivery_api"] = false.into();
        value["state"] = if value["enabled"] == true {
            "awaiting_service"
        } else {
            "disabled_unverified"
        }
        .into();
        Ok(value)
    }
    fn dispatch(&mut self, auth: &mut Auth, command: AuthCommand) -> Result<Value> {
        if self.quick.is_some()
            && matches!(
                command,
                AuthCommand::SetForwardingEnabled { .. }
                    | AuthCommand::ForwardingConfigure { .. }
                    | AuthCommand::ForwardingTest { .. }
                    | AuthCommand::ForwardingPrune { .. }
            )
        {
            return Err(Error::Busy);
        }
        Ok(match command {
            AuthCommand::SetForwardingEnabled {
                token,
                expected_generation,
                request_id,
                enabled,
            } => {
                let b = denied(Binding::from_recent_owner(auth, &token))?;
                let exists = self.existing(b.clone())?.is_some();
                if !exists && (enabled || expected_generation != 0) {
                    return Err(Error::InvalidInput);
                }
                let reader = self.reader.clone();
                let applied = denied(self.create(b.clone())?.set_enabled_owner(
                    auth,
                    &reader,
                    &token,
                    expected_generation,
                    &request_id,
                    enabled,
                ))?;
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|s| s.generation != applied)
                {
                    self.worker = None;
                }
                self.status(b)?
            }
            AuthCommand::ForwardingStatus { token } => {
                let b = denied(Binding::from_owner(auth, &token))?;
                let value = self.status(b.clone())?;
                denied(b.check_current(auth))?;
                value
            }
            AuthCommand::ForwardingRules { token } => {
                let b = denied(Binding::from_owner(auth, &token))?;
                let (view, generation) = match self.existing(b.clone())? {
                    Some(q) => (denied(q.policy_view())?, denied(q.status())?.generation),
                    None => (Policy::default().view(), 0),
                };
                denied(b.check_current(auth))?;
                let mut value = serde_json::to_value(view).map_err(|_| Error::Forbidden)?;
                value["generation"] = generation.into();
                value
            }
            AuthCommand::ForwardingRequest { token, request_id } => {
                let b = denied(Binding::from_owner(auth, &token))?;
                let (generation, applied) = match self.existing(b.clone())? {
                    Some(q) => (
                        denied(q.status())?.generation,
                        denied(q.configuration_receipt(&b, &request_id))?,
                    ),
                    None => (0, None),
                };
                denied(b.check_current(auth))?;
                json!({"request_id":request_id,"known":applied.is_some(),"applied_generation":applied,
                    "current_generation":generation,"not_found_is_not_proof":true})
            }
            AuthCommand::ForwardingTest {
                token,
                expected_generation,
                request_id,
                target_id,
            } => {
                let b = denied(Binding::from_recent_owner(auth, &token))?;
                let reader = self.reader.clone();
                let q = self.existing(b.clone())?.ok_or(Error::Forbidden)?;
                let receipt = denied(q.test_owner(
                    auth,
                    &reader,
                    &token,
                    expected_generation,
                    &request_id,
                    &target_id,
                ))?;
                denied(b.check_current(auth))?;
                json!({"request_id":request_id,"accepted":true,"synthetic":true,"receipt":receipt})
            }
            AuthCommand::ForwardingTestRequest { token, request_id } => {
                let b = denied(Binding::from_owner(auth, &token))?;
                let receipt = match self.existing(b.clone())? {
                    Some(q) => denied(q.test_receipt(&b, &request_id))?,
                    None => None,
                };
                denied(b.check_current(auth))?;
                json!({"request_id":request_id,"known":receipt.is_some(),"receipt":receipt,"not_found_is_not_proof":true,
                    "quick_active":self.quick.as_deref()==Some(request_id.as_str()),
                    "automatic_stop_verified":self.quick_stop.as_ref().filter(|(id,_)|id==&request_id).map(|(_,verified)|*verified)})
            }
            AuthCommand::ForwardingConfigure {
                token,
                expected_generation,
                request_id,
                policy,
            } => {
                let b = denied(Binding::from_recent_owner(auth, &token))?;
                let edit: PolicyEdit =
                    serde_json::from_value(policy).map_err(|_| Error::InvalidInput)?;
                // All semantic/Owner validation precedes filesystem creation.
                let policy = match self.existing(b.clone())? {
                    Some(q) => denied(q.resolve_policy_edit(&b, edit))?,
                    None => {
                        if expected_generation != 0 {
                            return Err(Error::Forbidden);
                        }
                        denied(edit.resolve(Policy::default()))?
                    }
                };
                let reader = self.reader.clone();
                let applied = denied(self.create(b.clone())?.configure_owner(
                    auth,
                    &reader,
                    &token,
                    expected_generation,
                    &request_id,
                    policy,
                ))?;
                // An exact lost-response retry must NOT revoke the already
                // current worker. An actual new generation always does.
                if self
                    .worker
                    .as_ref()
                    .is_some_and(|s| s.generation != applied)
                {
                    self.worker = None;
                }
                self.status(b)?
            }
            AuthCommand::ForwardingReceipts {
                token,
                after,
                limit,
            } => {
                let b = denied(Binding::from_owner(auth, &token))?;
                let after = sequence(&after)?;
                if !(1..=32).contains(&limit) {
                    return Err(Error::InvalidInput);
                }
                let page = match self.existing(b.clone())? {
                    Some(q) => denied(q.receipts(after, limit))?,
                    None => vec![],
                };
                denied(b.check_current(auth))?;
                json!({"receipts":page})
            }
            AuthCommand::ForwardingPrune {
                token,
                generation,
                through,
            } => {
                let b = denied(Binding::from_recent_owner(auth, &token))?;
                let through = sequence(&through)?;
                let q = self.existing(b.clone())?.ok_or(Error::Forbidden)?;
                let pruned = denied(q.prune_terminal(&b, generation, through))?;
                denied(b.check_current(auth))?;
                json!({"pruned":pruned,"unknown_preserved":true})
            }
            _ => return Err(Error::Forbidden),
        })
    }
}
fn sequence(s: &str) -> Result<i64> {
    s.parse::<i64>()
        .ok()
        .filter(|n| *n >= 0 && n.to_string() == s)
        .ok_or(Error::InvalidInput)
}

#[cfg(test)]
mod tests;
