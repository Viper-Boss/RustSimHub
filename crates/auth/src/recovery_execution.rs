// SPDX-License-Identifier: AGPL-3.0-only
//! Local persistent dedupe commitment. No new secure-session primitive, wire
//! API or permission to call the network backend without its existing gates.
use crate::{
    recovery_capability::{AuthorizedRecoveryRequest, RecoveryAction, RecoveryCapability},
    Auth, Error, Result,
};
use hmac::{Hmac, Mac};
use rustsimhub_storage::RecoveryGrant;
use sha2::Sha256;
const REQUEST_DOMAIN: &[u8] = b"RustSimHub.RecoveryExecution.Request.v2\0";
mod ledger;
pub use ledger::{RecoveryExecutionAdmission, RecoveryExecutionClaim, RecoveryExecutionPending};
mod trial;
pub use trial::{RecoveryTrialAuthority, RecoveryTrialControl};
mod observations;
/// Auth-minted preparation, not a durable execution permit. The verified body
/// stays in the original Zeroizing allocation; no Debug/Clone/Serde constructor.
pub struct RecoveryExecutionRequest {
    authorized: AuthorizedRecoveryRequest,
    request_id: String,
    trial: Option<String>,
    fingerprint: [u8; 32],
}
impl RecoveryExecutionRequest {
    pub fn action(&self) -> RecoveryAction {
        self.authorized.capability.action()
    }
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    pub fn target_trial(&self) -> Option<&str> {
        self.trial.as_deref()
    }
    pub fn fingerprint(&self) -> &[u8; 32] {
        &self.fingerprint
    }
    pub fn grant(&self) -> &RecoveryGrant {
        self.authorized.capability.execution_binding().grant()
    }
    pub fn capability(&self) -> &RecoveryCapability {
        &self.authorized.capability
    }
    /// Sensitive authenticated bytes for the trusted Core parser, never logs or
    /// a SQLite receipt. This does not validate Wi-Fi parameters/target meaning.
    pub fn body(&self) -> &[u8] {
        &self.authorized.body
    }
}
fn action_name(action: RecoveryAction) -> Result<&'static str> {
    match action {
        RecoveryAction::BeginWifi => Ok("begin_wifi"),
        RecoveryAction::ConfirmWifi => Ok("confirm_wifi"),
        RecoveryAction::CancelWifi => Ok("cancel_wifi"),
        _ => Err(Error::InvalidInput),
    }
}
fn check_target(action: RecoveryAction, trial: Option<&str>) -> Result<()> {
    action_name(action)?;
    match (action, trial) {
        (RecoveryAction::BeginWifi, None) => Ok(()),
        (RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi, Some(t))
            if t.len() == 64
                && t.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Ok(())
        }
        _ => Err(Error::InvalidInput),
    }
}
fn part(mac: &mut Hmac<Sha256>, bytes: &[u8]) -> Result<()> {
    let length = u32::try_from(bytes.len()).map_err(|_| Error::InvalidInput)?;
    mac.update(&length.to_le_bytes());
    mac.update(bytes);
    Ok(())
}
fn fingerprint(
    key: &[u8; 32],
    grant: &RecoveryGrant,
    request: [u8; 16],
    action: RecoveryAction,
    trial: Option<&str>,
    body: &[u8],
) -> Result<[u8; 32]> {
    check_target(action, trial)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| Error::InvalidInput)?;
    mac.update(REQUEST_DOMAIN);
    part(&mut mac, grant.realm.as_bytes())?;
    mac.update(&grant.device_public_key);
    part(&mut mac, grant.owner_id.as_bytes())?;
    mac.update(&grant.owner_generation.to_le_bytes());
    part(&mut mac, grant.id.as_bytes())?;
    mac.update(&grant.independent_public_key);
    mac.update(&grant.generation.to_le_bytes());
    mac.update(&request);
    part(&mut mac, action_name(action)?.as_bytes())?;
    mac.update(&grant.max_trial_ms.to_le_bytes());
    match trial {
        None => mac.update(&[0]),
        Some(trial) => {
            mac.update(&[1]);
            part(&mut mac, trial.as_bytes())?;
        }
    }
    part(&mut mac, body)?;
    Ok(mac.finalize().into_bytes().into())
}
impl Auth {
    /// Authenticated ReadStatus only; no claim, write permission or promise
    /// that a later Begin will pass its independent current gates.
    pub fn recovery_execution_available(
        &self,
        request: &AuthorizedRecoveryRequest,
    ) -> Result<bool> {
        self.check_recovery_capability(
            &request.capability,
            RecoveryAction::ReadStatus,
            &request.body,
        )?;
        let available = self.store.recovery_execution_available_from_trusted_auth(
            request.capability.execution_binding().grant().clone(),
            self.recovery_execution_fence(&request.capability)?,
        )?;
        self.check_recovery_capability(
            &request.capability,
            RecoveryAction::ReadStatus,
            &request.body,
        )?;
        Ok(available)
    }
    /// Bind the actual capability/action/body and local dedupe context. Core
    /// supplies a validated semantic target from that same body; this method
    /// does not parse a protocol, dispatch Wi-Fi, or make a one-time claim.
    pub fn prepare_recovery_execution(
        &self,
        request: AuthorizedRecoveryRequest,
        target_trial: Option<String>,
    ) -> Result<RecoveryExecutionRequest> {
        let cap = &request.capability;
        let action = cap.action();
        check_target(action, target_trial.as_deref())?;
        self.check_recovery_capability(cap, action, &request.body)?;
        self.store.check_recovery_execution_format()?;
        let binding = cap.execution_binding();
        let key = self.identity.recovery_execution_key()?;
        let fingerprint = fingerprint(
            &key,
            binding.grant(),
            *cap.request_id(),
            action,
            target_trial.as_deref(),
            &request.body,
        )?;
        self.check_recovery_capability(cap, action, &request.body)?;
        let request_id = uuid::Uuid::from_bytes(*cap.request_id()).to_string();
        Ok(RecoveryExecutionRequest {
            authorized: request,
            request_id,
            trial: target_trial,
            fingerprint,
        })
    }
    pub fn check_recovery_execution(&self, request: &RecoveryExecutionRequest) -> Result<()> {
        self.store.check_recovery_execution_format()?;
        self.check_recovery_capability(request.capability(), request.action(), request.body())
    }
}
#[cfg(all(test, unix))]
mod tests;
