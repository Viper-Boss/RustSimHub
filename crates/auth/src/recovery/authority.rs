// SPDX-License-Identifier: AGPL-3.0-only
//! One bounded recovery connection in the existing Auth. Not a wire or crypto protocol.
use super::{Auth, Error, RecoveryGrant, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fmt,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const MAX_ENVELOPE: usize = 2048;
const MAX_BODY: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryAction {
    ReadStatus,
    BeginWifi,
    QueryOperation,
    ConfirmWifi,
    CancelWifi,
    Disconnect,
}

/// Allocated only by trusted Core integration after accepting its finite IPC peer.
/// It proves neither SO_PEERCRED nor a password by itself. No wire deserialization.
pub struct RecoveryConnection {
    instance: String,
    nonce: [u8; 32],
}
impl fmt::Debug for RecoveryConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryConnection(..)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryProofBinding {
    instance: String,
    device_key: [u8; 32],
    connection: [u8; 32],
    grant: RecoveryGrant,
}
impl RecoveryProofBinding {
    pub fn auth_instance(&self) -> &str {
        &self.instance
    }
    pub fn device_key(&self) -> &[u8; 32] {
        &self.device_key
    }
    pub fn connection_nonce(&self) -> &[u8; 32] {
        &self.connection
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

/// Return ONLY after the approved A adapter has authenticated and decrypted the
/// request against every context binding, including realm/key/generation/device/
/// boot/connection. Never implement this by decoding an "authenticated" field.
/// No production implementation or cryptographic profile is supplied here.
pub trait RecoveryRequestVerifierV1: Send {
    fn verify_request(
        &mut self,
        context: &RecoveryProofContext,
        envelope: &[u8],
    ) -> Result<VerifiedRecoveryRequest>;
}

/// Trusted adapter output, not Deserialize/Serialize and never a wire DTO.
/// The session identifier is bounded opaque data, not a proposed nonce format.
pub struct VerifiedRecoveryRequest {
    binding: RecoveryProofBinding,
    session_id: Vec<u8>,
    request_id: [u8; 16],
    action: RecoveryAction,
    body: Zeroizing<Vec<u8>>,
    valid_for: Duration,
}
impl VerifiedRecoveryRequest {
    pub fn from_verified_adapter(
        binding: RecoveryProofBinding,
        session_id: Vec<u8>,
        request_id: [u8; 16],
        action: RecoveryAction,
        body: Zeroizing<Vec<u8>>,
        valid_for: Duration,
    ) -> Result<Self> {
        if !(16..=64).contains(&session_id.len())
            || session_id.iter().all(|b| *b == 0)
            || request_id == [0; 16]
            || body.len() > MAX_BODY
            || valid_for.is_zero()
            || valid_for > Duration::from_secs(300)
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            binding,
            session_id,
            request_id,
            action,
            body,
            valid_for,
        })
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
    session: Option<Session>,
}
#[derive(Default)]
pub(crate) struct State {
    verifier: Option<Box<dyn RecoveryRequestVerifierV1>>,
    connection: Option<Connection>,
}

/// Auth-created request capability; private fields, no Clone, no Deserialize and
/// no conversion to Principal, normal bearer or Scope. Recheck at each await gate.
/// This is NOT the durable operation receipt or exactly-once execution ledger.
pub struct RecoveryCapability {
    binding: RecoveryProofBinding,
    session_id: Vec<u8>,
    request_id: [u8; 16],
    action: RecoveryAction,
    digest: [u8; 32],
    deadline: Instant,
}
impl fmt::Debug for RecoveryCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCapability(..)")
    }
}
impl RecoveryCapability {
    pub fn action(&self) -> RecoveryAction {
        self.action
    }
    pub fn request_id(&self) -> &[u8; 16] {
        &self.request_id
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
    /// Reserve through the same checked capability used by the future Wi-Fi
    /// gate. No wire caller can substitute an arbitrary digest/grant/request.
    pub fn reserve_recovery_mutation(
        &self,
        capability: &RecoveryCapability,
        body: &[u8],
        trial_id: Option<String>,
    ) -> Result<rustsimhub_storage::RecoveryReservation> {
        self.check_recovery_capability(capability, capability.action, body)?;
        let mutation = mutation(capability.action)?;
        Ok(self.store.reserve_recovery_request(
            capability.binding.grant.clone(),
            request_uuid(capability.request_id),
            mutation,
            capability.digest,
            trial_id,
        )?)
    }
    pub fn claim_recovery_mutation(
        &self,
        capability: &RecoveryCapability,
        body: &[u8],
        receipt: rustsimhub_storage::RecoveryReceipt,
    ) -> Result<rustsimhub_storage::RecoveryExecutionPermit> {
        self.check_recovery_capability(capability, capability.action, body)?;
        let grant = &capability.binding.grant;
        if receipt.realm != grant.realm
            || receipt.grant_id != grant.id
            || receipt.generation != grant.generation
            || receipt.request_id != request_uuid(capability.request_id)
            || receipt.body_digest != capability.digest
            || receipt.action != mutation(capability.action)?
        {
            return Err(Error::Forbidden);
        }
        Ok(self.store.claim_recovery_request(grant.clone(), receipt)?)
    }
    pub fn owner_recovery_receipt(
        &mut self,
        token: &str,
        receipt: rustsimhub_storage::RecoveryReceipt,
    ) -> Result<rustsimhub_storage::RecoveryReceipt> {
        let owner = self.recovery_owner(token)?;
        Ok(self.store.owner_recovery_request(owner, receipt)?)
    }
    /// Trusted configuration, never HTTP/BLE payload. Any change invalidates the
    /// old connection/capabilities even if it installs the same adapter again.
    pub fn set_recovery_verifier(&mut self, verifier: Option<Box<dyn RecoveryRequestVerifierV1>>) {
        self.recovery.connection = None;
        self.recovery.verifier = verifier;
    }
    pub fn open_recovery_connection(&mut self) -> Result<RecoveryConnection> {
        self.rate_limit(Instant::now())?;
        if self.recovery.verifier.is_none() {
            return Err(Error::Forbidden);
        }
        if !self.store.recovery_enabled()? {
            return Err(Error::Forbidden);
        }
        if self.recovery.connection.is_some() {
            return Err(Error::Busy);
        }
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| Error::Entropy)?;
        if nonce == [0; 32] {
            return Err(Error::Entropy);
        }
        self.recovery.connection = Some(Connection {
            nonce,
            session: None,
        });
        Ok(RecoveryConnection {
            instance: self.instance.clone(),
            nonce,
        })
    }
    fn recovery_connection(&self, connection: &RecoveryConnection) -> Result<&Connection> {
        if connection.instance != self.instance {
            return Err(Error::Forbidden);
        }
        self.recovery
            .connection
            .as_ref()
            .filter(|c| c.nonce == connection.nonce)
            .ok_or(Error::Forbidden)
    }
    /// Trusted IPC disconnect/release only. Revocation is immediate and does not
    /// wait for remote acknowledgment. GATT/crypto cleanup remains its own duty.
    pub fn close_recovery_connection(&mut self, connection: &RecoveryConnection) -> Result<()> {
        self.recovery_connection(connection)?;
        self.recovery.connection = None;
        Ok(())
    }
    pub fn authorize_recovery(
        &mut self,
        connection: &RecoveryConnection,
        expected: RecoveryGrant,
        envelope: &[u8],
    ) -> Result<AuthorizedRecoveryRequest> {
        let started = Instant::now();
        self.rate_limit(started)?;
        if envelope.is_empty() || envelope.len() > MAX_ENVELOPE {
            return Err(Error::InvalidInput);
        }
        let current = self.recovery_connection(connection)?;
        let grant = self.store.check_recovery_grant(expected)?;
        let binding = RecoveryProofBinding {
            instance: self.instance.clone(),
            device_key: self.public_key(),
            connection: connection.nonce,
            grant,
        };
        // An expired/exhausted session remains a tombstone until disconnect. No
        // silent re-key/session-ID change can reset its budget on this connection.
        if let Some(session) = &current.session {
            if session.binding != binding {
                return Err(Error::Forbidden);
            }
            if started >= session.deadline {
                return Err(Error::Expired);
            }
            if session.requests.len() >= usize::from(binding.grant.limits.request_budget) {
                return Err(Error::Busy);
            }
        }
        let context = RecoveryProofContext {
            binding: binding.clone(),
        };
        let verified = self
            .recovery
            .verifier
            .as_mut()
            .ok_or(Error::Forbidden)?
            .verify_request(&context, envelope)?;
        if verified.binding != binding {
            return Err(Error::Forbidden);
        }
        // Recheck after potentially expensive proof verification. This also
        // catches another Store handle revoking the grant in the meantime.
        self.store.check_recovery_grant(binding.grant.clone())?;
        let now = Instant::now();
        let request_deadline = started
            + verified.valid_for.min(Duration::from_millis(u64::from(
                binding.grant.limits.session_ms,
            )));
        if now >= request_deadline {
            return Err(Error::Expired);
        }
        let current = self.recovery.connection.as_mut().ok_or(Error::Forbidden)?;
        if current.nonce != connection.nonce {
            return Err(Error::Forbidden);
        }
        if let Some(session) = &current.session {
            if session.id != verified.session_id {
                return Err(Error::Forbidden);
            }
            if session.requests.contains(&verified.request_id) {
                return Err(Error::Replay);
            }
            if now >= session.deadline {
                return Err(Error::Expired);
            }
        } else {
            current.session = Some(Session {
                binding: binding.clone(),
                id: verified.session_id.clone(),
                deadline: request_deadline,
                requests: HashSet::new(),
            });
        }
        let session = current.session.as_mut().ok_or(Error::Forbidden)?;
        session.requests.insert(verified.request_id);
        let capability = RecoveryCapability {
            binding,
            session_id: verified.session_id,
            request_id: verified.request_id,
            action: verified.action,
            digest: digest(verified.action, &verified.body),
            deadline: session.deadline.min(request_deadline),
        };
        Ok(AuthorizedRecoveryRequest {
            capability,
            body: verified.body,
        })
    }
    pub fn check_recovery_capability(
        &self,
        capability: &RecoveryCapability,
        action: RecoveryAction,
        body: &[u8],
    ) -> Result<()> {
        if capability.binding.instance != self.instance
            || capability.binding.device_key != self.public_key()
            || capability.action != action
            || body.len() > MAX_BODY
            || capability.digest != digest(action, body)
            || self.recovery.verifier.is_none()
        {
            return Err(Error::Forbidden);
        }
        let now = Instant::now();
        if now >= capability.deadline {
            return Err(Error::Expired);
        }
        let session = self
            .recovery
            .connection
            .as_ref()
            .filter(|c| c.nonce == capability.binding.connection)
            .and_then(|c| c.session.as_ref())
            .ok_or(Error::Forbidden)?;
        if session.binding != capability.binding
            || session.id != capability.session_id
            || !session.requests.contains(&capability.request_id)
        {
            return Err(Error::Forbidden);
        }
        if now >= session.deadline {
            return Err(Error::Expired);
        }
        self.store
            .check_recovery_grant(capability.binding.grant.clone())?;
        if Instant::now() >= capability.deadline || Instant::now() >= session.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
}

fn mutation(action: RecoveryAction) -> Result<rustsimhub_storage::RecoveryMutation> {
    use rustsimhub_storage::RecoveryMutation as M;
    match action {
        RecoveryAction::BeginWifi => Ok(M::BeginWifi),
        RecoveryAction::ConfirmWifi => Ok(M::ConfirmWifi),
        RecoveryAction::CancelWifi => Ok(M::CancelWifi),
        _ => Err(Error::Forbidden),
    }
}
fn request_uuid(id: [u8; 16]) -> String {
    let hex = format!("{:032x}", u128::from_be_bytes(id));
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

#[cfg(test)]
mod tests;
