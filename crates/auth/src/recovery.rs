// SPDX-License-Identifier: AGPL-3.0-only
//! Owner management only. No handshake, recovery bearer or ordinary-role minting.
use super::{Auth, Error, Result};
use rustsimhub_policy::{Principal, Scope};
pub use rustsimhub_storage::{RecoveryAudit, RecoveryGrant, RecoveryKey, RecoveryLimits};

/// Implemented by the approved security adapter, never by request JSON.
/// There is deliberately no default implementation or permissive production profile.
/// Key validation alone is NOT proof of a recovery session or permission to use Wi-Fi.
pub trait RecoveryKeyVerifierV1 {
    fn validate_recovery_key(&self, key: &RecoveryKey) -> Result<()>;
}

impl Auth {
    fn recovery_owner(&mut self, token: &str) -> Result<Principal> {
        let owner = self.recent_owner(token)?;
        if self.authorize(token, Scope::NetworkManage)? != owner {
            return Err(Error::Forbidden);
        }
        Ok(owner)
    }
    /// Trusted Core management entry. Not currently exposed on HTTP/BLE/IPC.
    /// Only the explicit schema-7 offline Store accepts the resulting write.
    pub fn grant_wifi_recovery(
        &mut self,
        token: &str,
        key: RecoveryKey,
        limits: RecoveryLimits,
        expected_generation: u64,
        verifier: Option<&dyn RecoveryKeyVerifierV1>,
    ) -> Result<RecoveryGrant> {
        let owner = self.recovery_owner(token)?;
        key.validate().map_err(|_| Error::InvalidInput)?;
        limits.validate().map_err(|_| Error::InvalidInput)?;
        verifier
            .ok_or(Error::Forbidden)?
            .validate_recovery_key(&key)?;
        // A slow validator must not turn an old interactive login into a fresh one.
        if self.recovery_owner(token)? != owner {
            return Err(Error::Forbidden);
        }
        Ok(self
            .store
            .grant_recovery(owner, key, limits, expected_generation)?)
    }
    pub fn revoke_wifi_recovery(
        &mut self,
        token: &str,
        id: String,
        expected_generation: u64,
    ) -> Result<RecoveryGrant> {
        let owner = self.recovery_owner(token)?;
        Ok(self.store.revoke_recovery(owner, id, expected_generation)?)
    }
    pub fn wifi_recovery_grants(&mut self, token: &str) -> Result<Vec<RecoveryGrant>> {
        let owner = self.recovery_owner(token)?;
        Ok(self.store.recovery_grants(owner)?)
    }
    pub fn wifi_recovery_audit(&mut self, token: &str) -> Result<Vec<RecoveryAudit>> {
        let owner = self.recovery_owner(token)?;
        Ok(self.store.recovery_audit(owner)?)
    }
}

#[cfg(test)]
mod tests;
