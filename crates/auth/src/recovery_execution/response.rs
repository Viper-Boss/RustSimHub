// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only response fencing. Never an execution claim or a crypto implementation.
use super::*;
use crate::recovery_capability::RecoveryProofBinding;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

/// Minted from an actually verified request before dispatch. No Clone, Serde,
/// Debug or public fields. The original connection, session and Store persist.
pub struct RecoveryResponseScope {
    binding: RecoveryProofBinding,
    session: zeroize::Zeroizing<Vec<u8>>,
    request: [u8; 16],
    action: RecoveryAction,
    deadline: Instant,
    live: Arc<AtomicBool>,
    store: rustsimhub_storage::Handle,
}
impl RecoveryResponseScope {
    pub(crate) fn from_verified_parts(
        binding: RecoveryProofBinding,
        session: Vec<u8>,
        request: [u8; 16],
        action: RecoveryAction,
        deadline: Instant,
        live: Arc<AtomicBool>,
        store: rustsimhub_storage::Handle,
    ) -> Result<Self> {
        let scope = Self {
            binding,
            session: zeroize::Zeroizing::new(session),
            request,
            action,
            deadline,
            live,
            store,
        };
        scope.check_live()?;
        Ok(scope)
    }
    pub fn binding(&self) -> &RecoveryProofBinding {
        &self.binding
    }
    pub fn session_id(&self) -> &[u8] {
        &self.session
    }
    pub fn request_id(&self) -> &[u8; 16] {
        &self.request
    }
    pub fn action(&self) -> RecoveryAction {
        self.action
    }
    /// SQLite access: run off the async executor. This authorizes only the
    /// original response, not another command, a trial, or a new connection.
    pub fn check_current(&self) -> Result<()> {
        self.check_live()?;
        self.store
            .check_recovery_grant(self.binding.grant().clone())?;
        self.check_live()
    }
    fn check_live(&self) -> Result<()> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::Forbidden);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.deadline = Instant::now();
    }
}
