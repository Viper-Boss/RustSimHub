// SPDX-License-Identifier: AGPL-3.0-only
//! Actual candidate cipher adapter for the same Auth's private connection and
//! grant. It does not install itself, create a listener, migrate a Store or mint
//! Owner/session tokens. Verification never acquires Auth's global mutex.
use crate::recovery_capability::{
    RecoveryAction, RecoveryConnection, RecoveryProofBinding, RecoveryProofContext,
    RecoveryRequestVerifierV1, VerifiedRecoveryRequest,
};
use crate::{Auth, Error, RecoveryResponseScope, Result};
use rustsimhub_recovery_session::{Action, Context, CoreBinding, Purpose, Role, Session};
use rustsimhub_storage::RecoveryGrant;
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

struct Slot {
    binding: RecoveryProofBinding,
    cipher: Session,
    origin: Instant,
    deadline: Instant,
    live: Arc<AtomicBool>,
}
impl Slot {
    fn now(&self) -> Result<u64> {
        u64::try_from(self.origin.elapsed().as_millis()).map_err(|_| Error::Expired)
    }
}
#[derive(Default)]
struct State {
    owner: Option<String>,
    last_connection: Option<[u8; 32]>,
    slot: Option<Slot>,
}
#[derive(Default)]
pub struct SecureRecoverySessions {
    state: Mutex<State>,
}
/// A device-only canonical transcript produced by the actual Noise session.
/// No arbitrary-message constructor/serde/clone. Auth rechecks the live binding
/// and stored grant before signing; the device private key never leaves Auth.
pub struct DeviceBindingWork {
    binding: RecoveryProofBinding,
    message: Vec<u8>,
    deadline: Instant,
}
pub struct SecureHandshakePreparation {
    binding: RecoveryProofBinding,
    context: Context,
    live: Arc<AtomicBool>,
    prepared: Instant,
}
pub struct SecureSessionLifetime {
    sessions: Weak<SecureRecoverySessions>,
    connection: [u8; 32],
    live: Arc<AtomicBool>,
}
impl Drop for SecureSessionLifetime {
    fn drop(&mut self) {
        self.live.store(false, Ordering::Release);
        if let Some(sessions) = self.sessions.upgrade() {
            if let Ok(mut state) = sessions.state.try_lock() {
                if state
                    .slot
                    .as_ref()
                    .is_some_and(|slot| slot.binding.connection_nonce() == &self.connection)
                {
                    state.slot = None;
                }
            }
        }
    }
}

fn map_crypto(_: rustsimhub_recovery_session::Error) -> Error {
    Error::Forbidden
}
fn core_binding(binding: &RecoveryProofBinding) -> Result<CoreBinding> {
    CoreBinding::new(
        Sha256::digest(binding.auth_instance().as_bytes()).into(),
        *binding.connection_nonce(),
        *binding.transport_nonce(),
    )
    .map_err(map_crypto)
}
impl Auth {
    /// Short Auth phase only. The expensive Noise construction is performed by
    /// begin_prepared after the caller has released its global Auth lock.
    pub fn prepare_secure_recovery_handshake(
        &self,
        token: &RecoveryConnection,
        grant: RecoveryGrant,
        nonce: [u8; 32],
    ) -> Result<SecureHandshakePreparation> {
        let binding = self.current_recovery_proof_binding(token, grant)?;
        let context = Context::new(
            Purpose::BleRecovery,
            *binding.device_key(),
            binding.grant().independent_public_key,
            binding.grant().generation,
            nonce,
        )
        .map_err(map_crypto)?;
        Ok(SecureHandshakePreparation {
            binding,
            context,
            live: token.secure_liveness(),
            prepared: Instant::now(),
        })
    }
    pub fn secure_recovery_grant(
        &self,
        token: &RecoveryConnection,
        key: [u8; 32],
        generation: u64,
    ) -> Result<RecoveryGrant> {
        // Reject unadmitted/retired/foreign source before the database lookup.
        self.check_secure_recovery_source(token)?;
        let grant = self
            .store
            .recovery_grant_for_trusted_verifier(key, generation)?;
        self.current_recovery_proof_binding(token, grant.clone())?;
        Ok(grant)
    }
    pub fn check_secure_recovery_context(
        &self,
        token: &RecoveryConnection,
        binding: &RecoveryProofBinding,
    ) -> Result<()> {
        let current = self.current_recovery_proof_binding(token, binding.grant().clone())?;
        if &current != binding {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    pub fn sign_secure_recovery_binding(
        &self,
        token: &RecoveryConnection,
        work: &DeviceBindingWork,
    ) -> Result<Vec<u8>> {
        self.check_secure_recovery_context(token, &work.binding)?;
        if Instant::now() >= work.deadline {
            return Err(Error::Expired);
        }
        Ok(self.identity.sign(&work.message))
    }
}
impl SecureRecoverySessions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn begin_prepared(
        &self,
        token: &RecoveryConnection,
        preparation: SecureHandshakePreparation,
    ) -> Result<()> {
        let SecureHandshakePreparation {
            binding,
            context,
            live,
            prepared: origin,
        } = preparation;
        if !token.matches_secure_binding(&binding)
            || Instant::now() >= origin + Duration::from_secs(5)
        {
            return Err(Error::Forbidden);
        }
        let mut state = self.state.try_lock().map_err(|_| Error::Busy)?;
        if state
            .owner
            .as_ref()
            .is_some_and(|owner| owner != binding.auth_instance())
            || state.last_connection == Some(*binding.connection_nonce())
        {
            return Err(Error::Forbidden);
        }
        // Only a newly admitted connection in the SAME real Auth can replace an
        // old slot. It does not reset a handshake on an old accepted descriptor.
        state.owner = Some(binding.auth_instance().to_owned());
        state.last_connection = Some(*binding.connection_nonce());
        state.slot = None;
        let deadline = origin + Duration::from_secs(300);
        let cipher =
            Session::device(context, core_binding(&binding)?, 0, 300_000).map_err(map_crypto)?;
        if !token.matches_secure_binding(&binding)
            || Instant::now() >= origin + Duration::from_secs(10)
        {
            return Err(Error::Expired);
        }
        state.slot = Some(Slot {
            binding,
            cipher,
            origin,
            deadline,
            live,
        });
        Ok(())
    }
    fn with_token<T>(
        &self,
        token: &RecoveryConnection,
        action: impl FnOnce(&mut Slot) -> Result<T>,
    ) -> Result<T> {
        let mut state = self.state.try_lock().map_err(|_| Error::Busy)?;
        let slot = state.slot.as_mut().ok_or(Error::Forbidden)?;
        if !token.matches_secure_binding(&slot.binding) {
            return Err(Error::Forbidden);
        }
        let result = action(slot).and_then(|output| {
            if token.is_live() {
                Ok(output)
            } else {
                Err(Error::Forbidden)
            }
        });
        if result.is_err() {
            state.slot = None;
            token.invalidate();
        }
        result
    }
    pub fn read_handshake(&self, token: &RecoveryConnection, wire: &[u8]) -> Result<()> {
        self.with_token(token, |slot| {
            slot.cipher
                .read_handshake(wire, slot.now()?)
                .map_err(map_crypto)
        })
    }
    pub fn write_handshake(&self, token: &RecoveryConnection) -> Result<Vec<u8>> {
        self.with_token(token, |slot| {
            slot.cipher.write_handshake(slot.now()?).map_err(map_crypto)
        })
    }
    pub fn device_signing_work(&self, token: &RecoveryConnection) -> Result<DeviceBindingWork> {
        self.with_token(token, |slot| {
            if slot.cipher.local_role() != Role::Device {
                return Err(Error::Forbidden);
            }
            Ok(DeviceBindingWork {
                binding: slot.binding.clone(),
                message: slot
                    .cipher
                    .binding_message(slot.now()?)
                    .map_err(map_crypto)?,
                deadline: (slot.origin + Duration::from_secs(10)).min(slot.deadline),
            })
        })
    }
    pub fn seal_device_binding(
        &self,
        token: &RecoveryConnection,
        work: DeviceBindingWork,
        signature: &[u8],
    ) -> Result<Vec<u8>> {
        self.with_token(token, |slot| {
            if slot.binding != work.binding
                || Instant::now() >= work.deadline
                || slot
                    .cipher
                    .binding_message(slot.now()?)
                    .map_err(map_crypto)?
                    != work.message
            {
                return Err(Error::Forbidden);
            }
            slot.cipher
                .seal_binding(signature, slot.now()?)
                .map_err(map_crypto)
        })
    }
    pub fn read_client_binding(&self, token: &RecoveryConnection, wire: &[u8]) -> Result<()> {
        self.with_token(token, |slot| {
            slot.cipher
                .open_binding(wire, slot.now()?)
                .map_err(map_crypto)
        })
    }
    pub fn write_binding_ack(&self, token: &RecoveryConnection) -> Result<Vec<u8>> {
        self.with_token(token, |slot| {
            slot.cipher
                .seal_binding_ack(slot.now()?)
                .map_err(map_crypto)
        })
    }
    pub fn binding(&self, token: &RecoveryConnection) -> Result<RecoveryProofBinding> {
        self.with_token(token, |slot| Ok(slot.binding.clone()))
    }
    pub fn lifetime(self: &Arc<Self>, token: &RecoveryConnection) -> Result<SecureSessionLifetime> {
        self.with_token(token, |slot| {
            Ok(SecureSessionLifetime {
                sessions: Arc::downgrade(self),
                connection: *slot.binding.connection_nonce(),
                live: slot.live.clone(),
            })
        })
    }
    pub fn retire(&self, token: &RecoveryConnection) -> Result<()> {
        self.with_token(token, |slot| {
            slot.cipher.close();
            Ok(())
        })?;
        token.invalidate();
        Ok(())
    }
    pub fn seal_response(
        &self,
        scope: &RecoveryResponseScope,
        plaintext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>> {
        scope.check_current()?;
        let mut state = self.state.try_lock().map_err(|_| Error::Busy)?;
        let slot = state.slot.as_mut().ok_or(Error::Forbidden)?;
        if !slot.live.load(Ordering::Acquire)
            || &slot.binding != scope.binding()
            || slot.cipher.session_id().map_err(map_crypto)?.as_slice() != scope.session_id()
        {
            return Err(Error::Forbidden);
        }
        let result = (|| {
            let wire = slot
                .cipher
                .seal_response(*scope.request_id(), plaintext, slot.now()?)
                .map_err(map_crypto)?;
            scope.check_current()?;
            Ok(Zeroizing::new(wire))
        })();
        if result.is_err() {
            state.slot = None;
        }
        result
    }
}
impl RecoveryRequestVerifierV1 for SecureRecoverySessions {
    fn verify_request(
        &self,
        context: &RecoveryProofContext,
        envelope: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        let mut state = self.state.try_lock().map_err(|_| Error::Busy)?;
        let slot = state.slot.as_mut().ok_or(Error::Forbidden)?;
        if !slot.live.load(Ordering::Acquire) || &slot.binding != context.binding() {
            return Err(Error::Forbidden);
        }
        let result = (|| {
            let request = slot
                .cipher
                .open_request(envelope, slot.now()?)
                .map_err(map_crypto)?;
            let action = match request.action {
                Action::ReadStatus => RecoveryAction::ReadStatus,
                Action::BeginWifi => RecoveryAction::BeginWifi,
                Action::QueryOperation => RecoveryAction::QueryOperation,
                Action::ConfirmWifi => RecoveryAction::ConfirmWifi,
                Action::CancelWifi => RecoveryAction::CancelWifi,
                Action::Disconnect => RecoveryAction::Disconnect,
                _ => return Err(Error::Forbidden),
            };
            let remaining = slot
                .deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Expired)?;
            if !slot.live.load(Ordering::Acquire) {
                return Err(Error::Forbidden);
            }
            VerifiedRecoveryRequest::from_verified_adapter(
                context.binding().clone(),
                slot.cipher.session_id().map_err(map_crypto)?.to_vec(),
                request.id,
                action,
                request.body,
                remaining,
            )
        })();
        if result.is_err() {
            state.slot = None;
        }
        result
    }
}
