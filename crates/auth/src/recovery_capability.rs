// SPDX-License-Identifier: AGPL-3.0-only
//! Same-Auth capability gates, not a cryptographic implementation or wire API.
//! Adapted from PR #61's private-capability semantics to main's fixed grant format.
use super::{Auth, Error, Result};
use rustsimhub_storage::RecoveryGrant;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const MAX_BODY: usize = 1024;
const MAX_REQUESTS: usize = 128;
const SESSION_TTL: Duration = Duration::from_secs(300);
const PROOF_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    ReadStatus,
    BeginWifi,
    QueryOperation,
    ConfirmWifi,
    CancelWifi,
    Disconnect,
}
/// Trusted adapter context only. No Debug/Serde or public field constructors.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryProofBinding {
    instance: String,
    device: [u8; 32],
    connection: [u8; 32],
    transport: [u8; 32],
    verifier_generation: u64,
    grant: RecoveryGrant,
}
impl RecoveryProofBinding {
    pub fn auth_instance(&self) -> &str {
        &self.instance
    }
    pub fn device_key(&self) -> &[u8; 32] {
        &self.device
    }
    pub fn connection_nonce(&self) -> &[u8; 32] {
        &self.connection
    }
    pub fn transport_nonce(&self) -> &[u8; 32] {
        &self.transport
    }
    pub fn grant(&self) -> &RecoveryGrant {
        &self.grant
    }
}
pub struct RecoveryProofContext {
    binding: RecoveryProofBinding,
}
impl RecoveryProofContext {
    pub fn binding(&self) -> &RecoveryProofBinding {
        &self.binding
    }
}
/// Trusted A adapter injection only. No default production implementation.
/// Authenticate/decrypt every binding before constructing VerifiedRecoveryRequest.
/// Called WITHOUT borrowing Auth. Stateful implementations may use their own lock;
/// they must not acquire the global Auth lock or detach work past their return.
pub trait RecoveryRequestVerifierV1: Send + Sync {
    fn verify_request(
        &self,
        context: &RecoveryProofContext,
        envelope: &[u8],
    ) -> Result<VerifiedRecoveryRequest>;
}
pub struct VerifiedRecoveryRequest {
    binding: RecoveryProofBinding,
    session: Vec<u8>,
    request: [u8; 16],
    action: RecoveryAction,
    body: Zeroizing<Vec<u8>>,
    valid_for: Duration,
}
impl VerifiedRecoveryRequest {
    /// Trusted adapter output, never decode a payload's "authenticated" flag here.
    pub fn from_verified_adapter(
        binding: RecoveryProofBinding,
        session: Vec<u8>,
        request: [u8; 16],
        action: RecoveryAction,
        body: Zeroizing<Vec<u8>>,
        valid_for: Duration,
    ) -> Result<Self> {
        if !(16..=64).contains(&session.len())
            || session.iter().all(|v| *v == 0)
            || request == [0; 16]
            || body.len() > MAX_BODY
            || valid_for.is_zero()
            || valid_for > SESSION_TTL
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            binding,
            session,
            request,
            action,
            body,
            valid_for,
        })
    }
}
/// Private transport token, not evidence that SO_PEERCRED or crypto was checked.
pub struct RecoveryConnection {
    instance: String,
    nonce: [u8; 32],
    transport: [u8; 32],
    live: Arc<AtomicBool>,
}
impl RecoveryConnection {
    /// Trusted host cancellation/disconnect fence. No Auth mutex or cleanup claim.
    pub fn invalidate(&self) {
        self.live.store(false, Ordering::Release);
    }
    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }
}
impl Drop for RecoveryConnection {
    fn drop(&mut self) {
        self.invalidate();
    }
}
struct Session {
    binding: RecoveryProofBinding,
    id: Vec<u8>,
    deadline: Instant,
    requests: HashSet<[u8; 16]>,
}
struct Connection {
    nonce: [u8; 32],
    transport: [u8; 32],
    session: Option<Session>,
    attempts: usize,
    live: Arc<AtomicBool>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
    }
}
#[derive(Default)]
pub(crate) struct State {
    verifier: Option<Arc<dyn RecoveryRequestVerifierV1>>,
    generation: u64,
    connection: Option<Connection>,
    busy: Arc<AtomicBool>,
}
struct Permit(Arc<AtomicBool>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
/// Own this work outside the short Auth lock, e.g. in Core's bounded worker.
/// Dropping a waiter must not drop work still running on another thread.
pub struct RecoveryVerification {
    verifier: Arc<dyn RecoveryRequestVerifierV1>,
    context: RecoveryProofContext,
    started: Instant,
    envelope: Zeroizing<Vec<u8>>,
    _permit: Permit,
}
pub struct RecoveryVerificationResult {
    context: RecoveryProofContext,
    started: Instant,
    verified: Result<VerifiedRecoveryRequest>,
    // Last: output/context must finish dropping before another work item can start.
    _permit: Permit,
}
impl RecoveryVerification {
    /// May block; the caller owns execution/deadline/draining, never call under Auth lock.
    pub fn verify(self) -> RecoveryVerificationResult {
        let verified = if Instant::now() >= self.started + PROOF_TTL {
            Err(Error::Expired)
        } else {
            self.verifier.verify_request(&self.context, &self.envelope)
        };
        RecoveryVerificationResult {
            context: self.context,
            started: self.started,
            verified,
            _permit: self._permit,
        }
    }
}
/// Auth-minted, no Clone/Serde/token/Principal conversion. A gate, not a durable claim.
pub struct RecoveryCapability {
    binding: RecoveryProofBinding,
    session: Vec<u8>,
    request: [u8; 16],
    action: RecoveryAction,
    digest: [u8; 32],
    deadline: Instant,
}
impl RecoveryCapability {
    #[cfg(feature = "recovery-execution")]
    pub(crate) fn execution_deadline(&self) -> Instant {
        self.deadline
    }
    #[cfg(feature = "recovery-execution")]
    pub(crate) fn same_execution_session(&self, other: &Self) -> bool {
        self.binding == other.binding && self.session == other.session
    }
    #[cfg(feature = "recovery-execution")]
    pub(crate) fn execution_binding(&self) -> &RecoveryProofBinding {
        &self.binding
    }
    #[cfg(all(test, feature = "recovery-execution"))]
    pub(crate) fn expire_execution_test(&mut self) {
        self.deadline = Instant::now();
    }
    pub fn action(&self) -> RecoveryAction {
        self.action
    }
    pub fn request_id(&self) -> &[u8; 16] {
        &self.request
    }
}

#[cfg(feature = "recovery-execution")]
impl Auth {
    /// Only after the ordinary same-Auth capability/body check. Writer receives
    /// the existing live flag and original minimum deadline, never a closure
    /// that could acquire this Auth lock or recursively call Storage.
    pub(crate) fn recovery_execution_fence(
        &self,
        cap: &RecoveryCapability,
    ) -> Result<rustsimhub_storage::RecoveryExecutionFence> {
        let connection = self.recovery.connection.as_ref().ok_or(Error::Forbidden)?;
        let session = connection.session.as_ref().ok_or(Error::Forbidden)?;
        if cap.binding.instance != self.instance
            || cap.binding != session.binding
            || cap.session != session.id
            || !connection.live.load(Ordering::Acquire)
        {
            return Err(Error::Forbidden);
        }
        Ok(
            rustsimhub_storage::RecoveryExecutionFence::from_trusted_auth(
                connection.live.clone(),
                cap.deadline.min(session.deadline),
            ),
        )
    }
}
pub struct AuthorizedRecoveryRequest {
    pub capability: RecoveryCapability,
    pub body: Zeroizing<Vec<u8>>,
}
fn digest(action: RecoveryAction, body: &[u8]) -> [u8; 32] {
    let tag = match action {
        RecoveryAction::ReadStatus => 1,
        RecoveryAction::BeginWifi => 2,
        RecoveryAction::QueryOperation => 3,
        RecoveryAction::ConfirmWifi => 4,
        RecoveryAction::CancelWifi => 5,
        RecoveryAction::Disconnect => 6,
    };
    let mut h = Sha256::new();
    h.update(b"RustSimHub.Recovery.Capability.v1");
    h.update([tag]);
    h.update((body.len() as u64).to_be_bytes());
    h.update(body);
    h.finalize().into()
}
impl Auth {
    /// Trusted startup configuration, not a wire command. Replacement invalidates
    /// the connection, but keeps the single work permit occupied until real drain.
    pub fn set_recovery_verifier(
        &mut self,
        verifier: Option<Arc<dyn RecoveryRequestVerifierV1>>,
    ) -> Result<()> {
        let next = self.recovery.generation.checked_add(1).ok_or(Error::Busy)?;
        self.recovery.generation = next;
        self.recovery.connection = None;
        self.recovery.verifier = verifier;
        Ok(())
    }
    /// Trusted Core calls ONLY after its private accepted-peer check. This method
    /// does not check a UID, and must never be routed from an AuthCommand/payload.
    pub fn open_recovery_connection_from_trusted_transport(
        &mut self,
        transport: [u8; 32],
    ) -> Result<RecoveryConnection> {
        if self.recovery.verifier.is_none() || transport == [0; 32] {
            return Err(Error::Forbidden);
        }
        if self.recovery.busy.load(Ordering::Acquire) {
            return Err(Error::Busy);
        }
        if self
            .recovery
            .connection
            .as_ref()
            .is_some_and(|c| !c.live.load(Ordering::Acquire))
        {
            self.recovery.connection = None;
        }
        if self.recovery.connection.is_some() {
            return Err(Error::Busy);
        }
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| Error::Entropy)?;
        if nonce == [0; 32] {
            return Err(Error::Entropy);
        }
        let live = Arc::new(AtomicBool::new(true));
        self.recovery.connection = Some(Connection {
            nonce,
            transport,
            session: None,
            attempts: 0,
            live: live.clone(),
        });
        Ok(RecoveryConnection {
            instance: self.instance.clone(),
            nonce,
            transport,
            live,
        })
    }
    fn recovery_connection(&self, token: &RecoveryConnection) -> Result<&Connection> {
        if token.instance != self.instance {
            return Err(Error::Forbidden);
        }
        self.recovery
            .connection
            .as_ref()
            .filter(|c| {
                c.nonce == token.nonce
                    && c.transport == token.transport
                    && c.live.load(Ordering::Acquire)
            })
            .ok_or(Error::Forbidden)
    }
    pub fn close_recovery_connection(&mut self, token: &RecoveryConnection) -> Result<()> {
        self.recovery_connection(token)?;
        self.recovery.connection = None;
        Ok(())
    }
    /// Short-lock phase: current grant snapshot and one outstanding work item.
    pub fn prepare_recovery_verification(
        &mut self,
        token: &RecoveryConnection,
        expected: RecoveryGrant,
        envelope: &[u8],
    ) -> Result<RecoveryVerification> {
        let started = Instant::now();
        if envelope.is_empty() || envelope.len() > 2048 {
            return Err(Error::InvalidInput);
        }
        let current = self.recovery_connection(token)?;
        // Invalid proofs and abandoned work consume the same bounded budget.
        // A new session identifier cannot reset work on the current connection.
        if current.attempts >= MAX_REQUESTS {
            return Err(Error::Busy);
        }
        let verifier = self
            .recovery
            .verifier
            .as_ref()
            .ok_or(Error::Forbidden)?
            .clone();
        let binding = RecoveryProofBinding {
            instance: self.instance.clone(),
            device: self.public_key(),
            connection: token.nonce,
            transport: token.transport,
            verifier_generation: self.recovery.generation,
            grant: self.store.check_recovery_grant(expected)?,
        };
        if let Some(session) = &current.session {
            if session.binding != binding {
                return Err(Error::Forbidden);
            }
            if started >= session.deadline {
                return Err(Error::Expired);
            }
            if session.requests.len() >= MAX_REQUESTS {
                return Err(Error::Busy);
            }
        }
        self.recovery_connection(token)?;
        self.recovery
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Busy)?;
        let permit = Permit(self.recovery.busy.clone());
        self.recovery
            .connection
            .as_mut()
            .ok_or(Error::Forbidden)?
            .attempts += 1;
        Ok(RecoveryVerification {
            verifier,
            context: RecoveryProofContext { binding },
            started,
            envelope: Zeroizing::new(envelope.to_vec()),
            _permit: permit,
        })
    }
    /// Short-lock phase after external verification. Stale results cannot mint authority.
    pub fn finish_recovery_verification(
        &mut self,
        token: &RecoveryConnection,
        result: RecoveryVerificationResult,
    ) -> Result<AuthorizedRecoveryRequest> {
        self.recovery_connection(token)?;
        let binding = result.context.binding;
        if binding.instance != self.instance
            || binding.device != self.public_key()
            || binding.connection != token.nonce
            || binding.transport != token.transport
            || binding.verifier_generation != self.recovery.generation
            || self.recovery.verifier.is_none()
        {
            return Err(Error::Forbidden);
        }
        if Instant::now() >= result.started + PROOF_TTL {
            return Err(Error::Expired);
        }
        let verified = result.verified?;
        if verified.binding != binding {
            return Err(Error::Forbidden);
        }
        self.store.check_recovery_grant(binding.grant.clone())?;
        self.recovery_connection(token)?;
        let deadline = result.started + verified.valid_for;
        let now = Instant::now();
        if now >= deadline || now >= result.started + PROOF_TTL {
            return Err(Error::Expired);
        }
        let current = self.recovery.connection.as_mut().ok_or(Error::Forbidden)?;
        if let Some(session) = &current.session {
            if session.binding != binding || session.id != verified.session {
                return Err(Error::Forbidden);
            }
            if now >= session.deadline {
                return Err(Error::Expired);
            }
            if session.requests.contains(&verified.request) {
                return Err(Error::Replay);
            }
            if session.requests.len() >= MAX_REQUESTS {
                return Err(Error::Busy);
            }
        } else {
            current.session = Some(Session {
                binding: binding.clone(),
                id: verified.session.clone(),
                deadline,
                requests: HashSet::new(),
            });
        }
        let session = current.session.as_mut().ok_or(Error::Forbidden)?;
        session.requests.insert(verified.request);
        Ok(AuthorizedRecoveryRequest {
            capability: RecoveryCapability {
                binding,
                session: verified.session,
                request: verified.request,
                action: verified.action,
                digest: digest(verified.action, &verified.body),
                deadline: deadline.min(session.deadline),
            },
            body: verified.body,
        })
    }
    /// Recheck before each business await/save gate. Does not claim an operation once.
    pub fn check_recovery_capability(
        &self,
        cap: &RecoveryCapability,
        action: RecoveryAction,
        body: &[u8],
    ) -> Result<()> {
        if cap.binding.instance != self.instance
            || cap.binding.device != self.public_key()
            || cap.binding.verifier_generation != self.recovery.generation
            || self.recovery.verifier.is_none()
            || cap.action != action
            || body.len() > MAX_BODY
            || cap.digest != digest(action, body)
        {
            return Err(Error::Forbidden);
        }
        let connection = self
            .recovery
            .connection
            .as_ref()
            .filter(|c| {
                c.nonce == cap.binding.connection
                    && c.transport == cap.binding.transport
                    && c.live.load(Ordering::Acquire)
            })
            .ok_or(Error::Forbidden)?;
        let session = connection.session.as_ref().ok_or(Error::Forbidden)?;
        if session.binding != cap.binding
            || session.id != cap.session
            || !session.requests.contains(&cap.request)
        {
            return Err(Error::Forbidden);
        }
        if Instant::now() >= cap.deadline || Instant::now() >= session.deadline {
            return Err(Error::Expired);
        }
        self.store.check_recovery_grant(cap.binding.grant.clone())?;
        if !connection.live.load(Ordering::Acquire) {
            return Err(Error::Forbidden);
        }
        if Instant::now() >= cap.deadline || Instant::now() >= session.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests;
