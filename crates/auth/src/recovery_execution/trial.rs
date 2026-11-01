// SPDX-License-Identifier: AGPL-3.0-only
//! Derived original-trial authority, never a fabricated normal Principal.
use super::*;
use std::time::{Duration, Instant};
const MAX_TRIAL: Duration = Duration::from_secs(40);
/// Consumes one actual Begin claim before the first external effect. Cannot be
/// cloned, deserialized, constructed publicly or moved to a fresh connection.
pub struct RecoveryTrialAuthority {
    begin: RecoveryExecutionClaim,
    deadline: Instant,
    recorder: rustsimhub_storage::Handle,
}
impl RecoveryTrialAuthority {
    /// Accounting with the original Store and private claims, independent of
    /// Auth mutex availability. Never grants authority or clears Unknown.
    pub fn finish_from_trusted_core(
        &self,
        control: Option<&RecoveryTrialControl>,
        completion: rustsimhub_storage::RecoveryExecutionCompletion,
    ) -> Result<rustsimhub_storage::RecoveryExecutionReceipt> {
        if control.is_some_and(|c| !c.belongs_to(self)) {
            return Err(Error::Forbidden);
        }
        if completion == rustsimhub_storage::RecoveryExecutionCompletion::Saved
            && control.is_none_or(|c| c.action() != RecoveryAction::ConfirmWifi)
        {
            return Err(Error::InvalidInput);
        }
        Ok(self.recorder.finish_recovery_trial_from_trusted_core(
            self.begin.storage_claim(),
            control.map(|c| c.claim.storage_claim()),
            completion,
        )?)
    }
    pub(super) fn begin_claim(&self) -> &RecoveryExecutionClaim {
        &self.begin
    }
    pub fn trial_id(&self) -> &str {
        self.begin
            .receipt()
            .trial_id()
            .expect("claimed Begin has trial")
    }
    pub fn request_id(&self) -> &str {
        self.begin.request().request_id()
    }
    pub fn grant(&self) -> &RecoveryGrant {
        self.begin.request().grant()
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.deadline = Instant::now();
    }
}
/// Separately verified Confirm/Cancel claim for exactly this original trial,
/// grant AND live Auth/connection/session. No normal-auth or wire conversion.
pub struct RecoveryTrialControl {
    claim: RecoveryExecutionClaim,
    begin_request: String,
}
impl RecoveryTrialControl {
    pub(super) fn claim(&self) -> &RecoveryExecutionClaim {
        &self.claim
    }
    pub(super) fn belongs_to(&self, trial: &RecoveryTrialAuthority) -> bool {
        self.begin_request == trial.request_id()
            && self.claim.request().target_trial() == Some(trial.trial_id())
            && self
                .claim
                .request()
                .capability()
                .same_execution_session(trial.begin.request().capability())
    }
    pub fn action(&self) -> RecoveryAction {
        self.claim.request().action()
    }
    pub fn request_id(&self) -> &str {
        self.claim.request().request_id()
    }
}
impl Auth {
    /// Check the original session/target BEFORE reserving a control request.
    /// A rejected new connection must not strand another durable control row.
    pub fn check_recovery_trial_request(
        &self,
        trial: &RecoveryTrialAuthority,
        request: &RecoveryExecutionRequest,
    ) -> Result<()> {
        if !matches!(
            request.action(),
            RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi
        ) {
            return Err(Error::InvalidInput);
        }
        self.check_recovery_trial(trial)?;
        self.check_recovery_execution(request)?;
        if request.target_trial() != Some(trial.trial_id())
            || !request
                .capability()
                .same_execution_session(trial.begin.request().capability())
        {
            return Err(Error::Forbidden);
        }
        self.check_recovery_trial(trial)
    }
    pub fn begin_recovery_trial(
        &self,
        begin: RecoveryExecutionClaim,
    ) -> Result<RecoveryTrialAuthority> {
        if begin.request().action() != RecoveryAction::BeginWifi {
            return Err(Error::InvalidInput);
        }
        self.check_recovery_execution_start(&begin)?;
        let limit =
            Duration::from_millis(u64::from(begin.request().grant().max_trial_ms)).min(MAX_TRIAL);
        let deadline =
            (Instant::now() + limit).min(begin.request().capability().execution_deadline());
        if Instant::now() >= deadline {
            return Err(Error::Expired);
        }
        Ok(RecoveryTrialAuthority {
            begin,
            deadline,
            recorder: self.store.clone(),
        })
    }
    /// Background gate for the original bounded trial, not a Confirm capability.
    pub fn check_recovery_trial(&self, trial: &RecoveryTrialAuthority) -> Result<()> {
        if Instant::now() >= trial.deadline {
            return Err(Error::Expired);
        }
        self.check_recovery_execution_claim(&trial.begin)?;
        if Instant::now() >= trial.deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
    /// Core validates the exact authenticated body's semantic target before
    /// preparation. This adds private same-session/original-trial binding, not
    /// proof that the network actor is Ready or a backend save has succeeded.
    pub fn bind_recovery_trial_control(
        &self,
        trial: &RecoveryTrialAuthority,
        claim: RecoveryExecutionClaim,
    ) -> Result<RecoveryTrialControl> {
        self.check_recovery_trial_request(trial, claim.request())?;
        self.check_recovery_execution_claim(&claim)?;
        if claim.request().target_trial() != Some(trial.trial_id())
            || !claim
                .request()
                .capability()
                .same_execution_session(trial.begin.request().capability())
        {
            return Err(Error::Forbidden);
        }
        self.check_recovery_trial(trial)?;
        Ok(RecoveryTrialControl {
            claim,
            begin_request: trial.request_id().into(),
        })
    }
    /// Use BOTH before and after commit, with ConfirmWifi explicitly. A Begin
    /// or Cancel claim cannot stand in for confirmation; no deadline extension.
    pub fn check_recovery_trial_control(
        &self,
        trial: &RecoveryTrialAuthority,
        control: &RecoveryTrialControl,
        expected: RecoveryAction,
    ) -> Result<()> {
        if !matches!(
            expected,
            RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi
        ) || control.action() != expected
            || control.begin_request != trial.request_id()
            || control.claim.request().target_trial() != Some(trial.trial_id())
            || !control
                .claim
                .request()
                .capability()
                .same_execution_session(trial.begin.request().capability())
        {
            return Err(Error::Forbidden);
        }
        self.check_recovery_trial(trial)?;
        self.check_recovery_execution_claim(&control.claim)?;
        self.check_recovery_trial(trial)
    }
}
