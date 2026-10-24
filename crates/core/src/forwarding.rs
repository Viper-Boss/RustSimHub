// SPDX-License-Identifier: AGPL-3.0-only
//! Core-owned, on-demand management. No polling, network, forwarding task or
//! process starts here. Root must independently prove the worker lifecycle.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_forwarder::{Binding, Policy, Queue};
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
fn denied<T>(result: rustsimhub_forwarder::Result<T>) -> Result<T> {
    result.map_err(|_| Error::Forbidden)
}

#[derive(Clone)]
pub(crate) struct Controller(Arc<Mutex<Management>>);
struct Management {
    // Parent is the already-private business state directory, not a browser path.
    directory: PathBuf,
    reader: Handle,
    queue: Option<Queue>,
    worker: Option<ipc::Session>,
}
impl Controller {
    /// Does not create files, start a task, read SMS or touch an interface.
    pub fn new(state: &Path, reader: Handle) -> Self {
        Self(Arc::new(Mutex::new(Management {
            directory: state.join("forwarding"),
            reader,
            queue: None,
            worker: None,
        })))
    }
    pub async fn request(&self, auth: Arc<Mutex<Auth>>, command: AuthCommand) -> Result<Value> {
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
                let view = match self.existing(b.clone())? {
                    Some(q) => denied(q.policy_view())?,
                    None => Policy::default().view(),
                };
                denied(b.check_current(auth))?;
                serde_json::to_value(view).map_err(|_| Error::Forbidden)?
            }
            AuthCommand::ForwardingConfigure {
                token,
                expected_generation,
                request_id,
                policy,
            } => {
                let b = denied(Binding::from_recent_owner(auth, &token))?;
                let policy: Policy =
                    serde_json::from_value(policy).map_err(|_| Error::InvalidInput)?;
                denied(policy.validate())?;
                // All semantic/Owner validation precedes filesystem creation.
                if self.existing(b.clone())?.is_none() && expected_generation != 0 {
                    return Err(Error::Forbidden);
                }
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
