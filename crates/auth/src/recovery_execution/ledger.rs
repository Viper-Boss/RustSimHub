// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_storage::{
    RecoveryExecutionAction, RecoveryExecutionReceipt, RecoveryExecutionReservation,
    StorageRecoveryExecutionClaim, StorageRecoveryExecutionReservation, TrustedRecoveryExecution,
};

/// Only the freshly inserted case has a private one-use pending ticket.
/// Existing is readonly, including a Reserved record from an earlier session.
pub enum RecoveryExecutionAdmission {
    Reserved(Box<RecoveryExecutionPending>),
    Existing(Box<RecoveryExecutionReceipt>),
}
impl RecoveryExecutionAdmission {
    pub fn receipt(&self) -> &RecoveryExecutionReceipt {
        match self {
            Self::Reserved(p) => p.durable.receipt(),
            Self::Existing(r) => r,
        }
    }
}
pub struct RecoveryExecutionPending {
    request: RecoveryExecutionRequest,
    durable: StorageRecoveryExecutionReservation,
}
/// Same-Auth request plus the one committed Storage claim. No public
/// constructor/Clone/Debug/Serde or conversion to an Owner/Client Principal.
/// This is a one-time dispatch token, not permanent permission across awaits.
pub struct RecoveryExecutionClaim {
    request: RecoveryExecutionRequest,
    durable: StorageRecoveryExecutionClaim,
}
impl RecoveryExecutionClaim {
    pub(super) fn storage_claim(&self) -> &StorageRecoveryExecutionClaim {
        &self.durable
    }
    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.request.authorized.capability.expire_execution_test();
    }
    pub fn request(&self) -> &RecoveryExecutionRequest {
        &self.request
    }
    pub fn receipt(&self) -> &RecoveryExecutionReceipt {
        self.durable.receipt()
    }
}
impl Auth {
    pub(super) fn trusted_execution(
        &self,
        request: &RecoveryExecutionRequest,
    ) -> Result<TrustedRecoveryExecution> {
        self.check_recovery_execution(request)?;
        let action = match request.action() {
            RecoveryAction::BeginWifi => RecoveryExecutionAction::BeginWifi,
            RecoveryAction::ConfirmWifi => RecoveryExecutionAction::ConfirmWifi,
            RecoveryAction::CancelWifi => RecoveryExecutionAction::CancelWifi,
            _ => return Err(Error::InvalidInput),
        };
        Ok(TrustedRecoveryExecution::from_trusted_auth(
            request.grant().clone(),
            request.request_id().into(),
            action,
            *request.fingerprint(),
            request.target_trial().map(str::to_owned),
            self.recovery_execution_fence(request.capability())?,
        )?)
    }
    /// Reserve/Existing are snapshots, never a dispatch permit. Identical
    /// requests may be observed again; conflict does not create a new UUID.
    pub fn reserve_recovery_execution(
        &self,
        request: RecoveryExecutionRequest,
    ) -> Result<RecoveryExecutionAdmission> {
        let input = self.trusted_execution(&request)?;
        let result = self
            .store
            .reserve_recovery_execution_from_trusted_auth(input)?;
        self.check_recovery_execution(&request)?;
        Ok(match result {
            RecoveryExecutionReservation::Reserved(durable) => {
                RecoveryExecutionAdmission::Reserved(Box::new(RecoveryExecutionPending {
                    request,
                    durable,
                }))
            }
            RecoveryExecutionReservation::Existing(receipt) => {
                RecoveryExecutionAdmission::Existing(Box::new(receipt))
            }
        })
    }
    /// Core allocates a Begin id from the ORIGINAL Wi-Fi engine before effects;
    /// Confirm/Cancel use the already validated target and take no new id.
    /// If authority disappears after commit, no token is returned and the
    /// durable claimed record remains conservative, never automatically retried.
    pub fn claim_recovery_execution(
        &self,
        pending: Box<RecoveryExecutionPending>,
        new_trial: Option<String>,
    ) -> Result<RecoveryExecutionClaim> {
        let RecoveryExecutionPending {
            request,
            durable: reservation,
        } = *pending;
        self.check_recovery_execution(&request)?;
        let durable = self
            .store
            .claim_recovery_execution_from_trusted_auth(reservation, new_trial)?;
        self.check_recovery_execution(&request)?;
        Ok(RecoveryExecutionClaim { request, durable })
    }
    /// Recheck at every future business await/save gate; no Auth mutex is
    /// acquired by the writer. Terminal/Unknown records are never authority.
    pub fn check_recovery_execution_claim(&self, claim: &RecoveryExecutionClaim) -> Result<()> {
        let input = self.trusted_execution(&claim.request)?;
        self.store
            .check_recovery_execution_claim_from_trusted_auth(input, &claim.durable)?;
        self.check_recovery_execution(&claim.request)
    }
    pub(super) fn check_recovery_execution_start(
        &self,
        claim: &RecoveryExecutionClaim,
    ) -> Result<()> {
        let input = self.trusted_execution(&claim.request)?;
        self.store
            .check_recovery_execution_start_from_trusted_auth(input, &claim.durable)?;
        self.check_recovery_execution(&claim.request)
    }
    /// Read-only Query capability. Core must derive the requested UUID from
    /// this same authenticated body; NotFound never authorizes retransmission.
    pub fn lookup_recovery_execution(
        &self,
        query: &AuthorizedRecoveryRequest,
        request_id: String,
    ) -> Result<Option<RecoveryExecutionReceipt>> {
        if query.capability.action() != RecoveryAction::QueryOperation {
            return Err(Error::InvalidInput);
        }
        self.check_recovery_capability(
            &query.capability,
            RecoveryAction::QueryOperation,
            &query.body,
        )?;
        let result = self.store.lookup_recovery_execution_from_trusted_auth(
            query.capability.execution_binding().grant().clone(),
            request_id,
            self.recovery_execution_fence(&query.capability)?,
        )?;
        self.check_recovery_capability(
            &query.capability,
            RecoveryAction::QueryOperation,
            &query.body,
        )?;
        Ok(result)
    }
}
