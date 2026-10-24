// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit opt-in routing on the existing Core socket. Root may grant/revoke
//! a finite worker capability; only the DISTINCT configured UID may use it.
//! Neither that UID nor a browser may configure policy through this protocol.
use super::*;
use rustsimhub_forwarder::{worker_ipc as wire, Claim, DeliveryOutcome, Secret, Stage, State};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use tokio::{net::UnixStream, time::timeout};

const LEASE: Duration = Duration::from_secs(30);
#[cfg(feature = "dev-fixtures")]
pub(crate) mod fixture;
pub(super) struct Session {
    token_digest: [u8; 32],
    binding: Binding,
    pub generation: u64,
    deadline: Instant,
    claim: Option<Claim>,
    next_stage: u8,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Route {
    Browser,
    Root,
    Worker,
}
#[derive(Clone, Copy)]
pub(crate) struct Access {
    edge: u32,
    worker: u32,
}
impl Access {
    pub fn new(edge: u32, worker: u32) -> std::result::Result<Self, &'static str> {
        let core = rustix::process::geteuid().as_raw();
        if core == 0 || edge == 0 || worker == 0 || edge == worker || core == edge || core == worker
        {
            return Err("forwarding requires separate non-root Core, Edge and worker UIDs");
        }
        Ok(Self { edge, worker })
    }
    pub fn route(&self, stream: &UnixStream) -> Option<Route> {
        self.uid(stream.peer_cred().ok()?.uid())
    }
    fn uid(&self, uid: u32) -> Option<Route> {
        if uid == 0 {
            Some(Route::Root)
        } else if uid == self.worker {
            Some(Route::Worker)
        } else if uid == self.edge {
            Some(Route::Browser)
        } else {
            None
        }
    }
}
impl Controller {
    async fn private_request(
        &self,
        auth: Arc<Mutex<Auth>>,
        route: Route,
        command: wire::Command,
    ) -> Result<Value> {
        if route == Route::Browser || command.root_only() != (route == Route::Root) {
            return Err(Error::Forbidden);
        }
        let state = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let a = auth.try_lock().map_err(|_| Error::Busy)?;
            let mut s = state.try_lock().map_err(|_| Error::Busy)?;
            s.private(&a, command)
        })
        .await
        .map_err(|_| Error::Forbidden)?
    }
}
pub(crate) async fn handle(
    stream: &mut UnixStream,
    auth: Arc<Mutex<Auth>>,
    controller: Controller,
    route: Route,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let bytes = timeout(Duration::from_secs(5), wire::read_frame(stream)).await??;
    let request = wire::parse(&bytes)?;
    let result = match controller
        .private_request(auth, route, request.command)
        .await
    {
        Ok(value) => value,
        Err(Error::Busy) => json!({"error":"retry_later"}), // Rejected before admission/mutation.
        Err(_) => json!({"error":"denied"}),
    };
    let bytes = serde_json::to_vec(&wire::Response {
        protocol: wire::PROTOCOL,
        id: request.id,
        result,
    })?;
    timeout(Duration::from_secs(5), wire::write_frame(stream, &bytes)).await??;
    Ok(())
}
pub(crate) async fn cli(
    socket: &str,
    core_uid: u32,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((wire::MAX_FRAME + 1) as u64)
        .read_to_end(&mut bytes)?;
    let request = wire::parse(&bytes)?;
    let result = wire::request(Path::new(socket), core_uid, request.command).await?;
    println!(
        "{}",
        serde_json::to_string(&wire::Response {
            protocol: wire::PROTOCOL,
            id: request.id,
            result
        })?
    );
    Ok(())
}
impl Management {
    fn root_open(&mut self, auth: &Auth) -> Result<bool> {
        if self.queue.is_none() {
            match fs::symlink_metadata(&self.directory) {
                Ok(_) => self.queue = Some(denied(Queue::open_current(&self.directory, auth))?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(_) => return Err(Error::Forbidden),
            }
        }
        denied(
            self.queue
                .as_ref()
                .ok_or(Error::Forbidden)?
                .current_binding(auth),
        )?;
        Ok(true)
    }
    fn retire(&mut self) {
        if let Some(session) = self.worker.take() {
            if let (Some(q), Some(claim)) = (self.queue.as_mut(), session.claim) {
                // Safe even after Owner revocation. This is ONLY an ambiguity
                // marker; never a submit, retry, success or change of owner.
                let _ = q.finish(&session.binding, &claim, State::Unknown);
            }
        }
    }
    fn validate(&mut self, auth: &Auth, lease: &Secret) -> Result<()> {
        let digest: [u8; 32] = Sha256::digest(lease.expose().as_bytes()).into();
        let session = self.worker.as_ref().ok_or(Error::Forbidden)?;
        if digest != session.token_digest {
            return Err(Error::Forbidden);
        }
        let result = (|| {
            if Instant::now() >= session.deadline {
                return Err(Error::Forbidden);
            }
            denied(session.binding.check_current(auth))?;
            let q = self.queue.as_ref().ok_or(Error::Forbidden)?;
            denied(q.require_binding(&session.binding))?;
            let status = denied(q.status())?;
            if !status.enabled || status.generation != session.generation {
                return Err(Error::Forbidden);
            }
            Ok(())
        })();
        if result.is_err() {
            self.retire();
        }
        result
    }
    fn private(&mut self, auth: &Auth, command: wire::Command) -> Result<Value> {
        if self
            .worker
            .as_ref()
            .is_some_and(|s| Instant::now() >= s.deadline)
        {
            self.retire();
        }
        Ok(match command {
            wire::Command::State {} => {
                if !self.root_open(auth)? {
                    json!({"configured":false,"enabled":false,"generation":0,"lease_active":false,"process_state_verified":false,
                        "core_instance":auth.observer_instance(),"binding_sha256":null,"lease_sha256":null})
                } else {
                    let mut v = serde_json::to_value(denied(
                        self.queue.as_ref().ok_or(Error::Forbidden)?.status(),
                    )?)
                    .map_err(|_| Error::Forbidden)?;
                    v["configured"] = true.into();
                    v["lease_active"] = self.worker.is_some().into();
                    v["core_instance"] = auth.observer_instance().into();
                    v["binding_sha256"] = serde_json::to_value(binding_sha256(&denied(
                        self.queue
                            .as_ref()
                            .ok_or(Error::Forbidden)?
                            .current_binding(auth),
                    )?)?)
                    .map_err(|_| Error::Forbidden)?;
                    v["lease_sha256"] =
                        serde_json::to_value(self.worker.as_ref().map(|s| s.token_digest))
                            .map_err(|_| Error::Forbidden)?;
                    v
                }
            }
            wire::Command::Grant {} => {
                if !self.root_open(auth)? || self.worker.is_some() {
                    return Err(Error::Forbidden);
                }
                let q = self.queue.as_ref().ok_or(Error::Forbidden)?;
                let status = denied(q.status())?;
                if !status.enabled {
                    return Err(Error::Forbidden);
                }
                let binding = denied(q.current_binding(auth))?;
                let mut random = [0; 32];
                getrandom::fill(&mut random).map_err(|_| Error::Entropy)?;
                let token = Secret::new(random.iter().map(|b| format!("{b:02x}")).collect());
                let token_digest = Sha256::digest(token.expose().as_bytes()).into();
                self.worker = Some(Session {
                    token_digest,
                    binding,
                    generation: status.generation,
                    deadline: Instant::now() + LEASE,
                    claim: None,
                    next_stage: 0,
                });
                json!({"lease":token,"generation":status.generation,"expires_in_seconds":30,
                    "core_instance":auth.observer_instance(),"lease_sha256":token_digest,"process_state_verified":false})
            }
            wire::Command::Revoke {} => {
                self.retire();
                json!({"revoked":true,"process_state_verified":false})
            }
            wire::Command::Retire { lease_sha256 } => {
                let matching = self
                    .worker
                    .as_ref()
                    .is_some_and(|s| s.token_digest == lease_sha256);
                if matching {
                    self.retire();
                }
                json!({"revoked":matching,"process_state_verified":false})
            }
            wire::Command::Acquire { lease } => {
                self.validate(auth, &lease)?;
                let s = self.worker.as_mut().ok_or(Error::Forbidden)?;
                if s.claim.is_some() {
                    return Err(Error::Forbidden); // A possibly lost claim is NOT retry-later.
                }
                let q = self.queue.as_mut().ok_or(Error::Forbidden)?;
                denied(q.capture_current(auth, &self.reader, s.generation))?;
                s.claim = denied(q.claim_current(auth, &self.reader, s.generation))?;
                s.next_stage = 0;
                s.deadline = Instant::now() + LEASE;
                let digest = denied(s.claim.as_ref().map(wire::claim_sha256).transpose())?;
                json!({"claim":s.claim,"claim_sha256":digest})
            }
            wire::Command::Check {
                lease,
                attempt_id,
                claim_sha256,
                stage,
            } => {
                self.validate(auth, &lease)?;
                let result = (|| {
                    let s = self.worker.as_mut().ok_or(Error::Forbidden)?;
                    let c = s.claim.as_ref().ok_or(Error::Forbidden)?;
                    let expected = match s.next_stage {
                        0 => Stage::BeforeConnect,
                        1 => Stage::BeforeTls,
                        2 => Stage::BeforeSubmit,
                        _ => return Err(Error::Forbidden),
                    };
                    if attempt_id != c.attempt_id
                        || stage != expected
                        || denied(wire::claim_sha256(c))? != claim_sha256
                    {
                        return Err(Error::Forbidden);
                    }
                    denied(
                        self.queue
                            .as_ref()
                            .ok_or(Error::Forbidden)?
                            .check_current_claim(auth, &self.reader, c),
                    )?;
                    s.next_stage += 1;
                    s.deadline = Instant::now() + LEASE;
                    Ok(json!({"allowed":true}))
                })();
                // A failed live fence terminates this capability and marks
                // ambiguity. No failure ever hands the request out a second time.
                if result.is_err() {
                    self.retire();
                }
                result?
            }
            wire::Command::Finish {
                lease,
                attempt_id,
                outcome,
            } => {
                self.validate(auth, &lease)?;
                let s = self.worker.as_ref().ok_or(Error::Forbidden)?;
                let c = s.claim.as_ref().ok_or(Error::Forbidden)?;
                if c.attempt_id != attempt_id {
                    return Err(Error::Forbidden);
                }
                if matches!(outcome, DeliveryOutcome::Confirmed { .. }) && s.next_stage != 3 {
                    return Err(Error::Forbidden);
                }
                let state = if s.next_stage == 3
                    && matches!(outcome, DeliveryOutcome::NotSubmitted { .. })
                {
                    // Submit permission may have been delivered before an IPC
                    // ACK disappeared. No false safe-to-retry assertion.
                    State::Unknown
                } else {
                    outcome.queue_state()
                };
                let result = denied(
                    self.queue
                        .as_mut()
                        .ok_or(Error::Forbidden)?
                        .finish(&s.binding, c, state),
                );
                if result.is_err() {
                    self.retire();
                    return Err(Error::Forbidden);
                }
                let s = self.worker.as_mut().ok_or(Error::Forbidden)?;
                s.claim = None;
                s.next_stage = 0;
                s.deadline = Instant::now() + LEASE;
                json!({"accepted":true,"state":state})
            }
        })
    }
}

fn binding_sha256(binding: &Binding) -> Result<[u8; 32]> {
    Ok(Sha256::digest(serde_json::to_vec(binding).map_err(|_| Error::Forbidden)?).into())
}

#[cfg(test)]
mod tests;
