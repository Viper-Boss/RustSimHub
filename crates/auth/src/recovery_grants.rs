// SPDX-License-Identifier: AGPL-3.0-only
//! Independent recovery-key administration. No session proof or capability minting here.
use super::*;
use rustsimhub_storage::{RecoveryChange, RecoveryGrant};

impl Auth {
    /// Recent interactive owner, valid independent Ed25519 key, exact CAS and request identity.
    /// Available only on an explicitly migrated schema-7 Store, never default production startup.
    pub fn grant_recovery_key(
        &mut self,
        token: &str,
        key: [u8; 32],
        max_trial_ms: u32,
        expected_generation: u64,
        request_id: String,
    ) -> Result<RecoveryGrant> {
        let owner = self.recent_owner(token)?;
        key_valid(&key)?;
        Ok(self.store.change_recovery_grant(
            owner,
            key,
            expected_generation,
            request_id,
            RecoveryChange::Grant { max_trial_ms },
        )?)
    }
    pub fn revoke_recovery_key(
        &mut self,
        token: &str,
        key: [u8; 32],
        expected_generation: u64,
        request_id: String,
    ) -> Result<RecoveryGrant> {
        let owner = self.recent_owner(token)?;
        Ok(self.store.change_recovery_grant(
            owner,
            key,
            expected_generation,
            request_id,
            RecoveryChange::Revoke,
        )?)
    }
    /// Raw administrative record; `active` is not a capability or proof of a live session.
    pub fn recovery_key(&mut self, token: &str, key: [u8; 32]) -> Result<Option<RecoveryGrant>> {
        let owner = self.recent_owner(token)?;
        Ok(self.store.recovery_grant(owner, key)?)
    }
    /// Query the original request after a lost reply. Never retries a mutation.
    pub fn recovery_key_request(
        &mut self,
        token: &str,
        request_id: String,
    ) -> Result<Option<RecoveryGrant>> {
        let owner = self.recent_owner(token)?;
        Ok(self.store.recovery_grant_request(owner, request_id)?)
    }
}

#[cfg(all(test, unix))]
mod tests;
