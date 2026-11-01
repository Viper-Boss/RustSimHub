// SPDX-License-Identifier: AGPL-3.0-only
//! Core-only accounting adapter. Does not dispatch any backend or parse wire.
use super::*;
use rustsimhub_storage::{
    RecoveryExecutionCompletion, RecoveryExecutionProgress, RecoveryExecutionReceipt,
};
impl Auth {
    pub fn record_recovery_trial_progress_from_trusted_core(
        &self,
        trial: &RecoveryTrialAuthority,
        progress: RecoveryExecutionProgress,
    ) -> Result<RecoveryExecutionReceipt> {
        self.check_recovery_trial(trial)?;
        let begin = trial.begin_claim();
        let input = self
            .trusted_execution(begin.request())?
            .with_deadline_limit(trial.deadline());
        let result = self.store.record_recovery_progress_from_trusted_core(
            input,
            begin.storage_claim(),
            progress,
        )?;
        self.check_recovery_trial(trial)?;
        Ok(result)
    }
    pub fn record_recovery_trial_saving_from_trusted_core(
        &self,
        trial: &RecoveryTrialAuthority,
        control: &RecoveryTrialControl,
    ) -> Result<()> {
        self.check_recovery_trial_control(trial, control, RecoveryAction::ConfirmWifi)?;
        let (b, c) = (trial.begin_claim(), control.claim());
        self.store.record_recovery_saving_from_trusted_core(
            self.trusted_execution(b.request())?
                .with_deadline_limit(trial.deadline()),
            b.storage_claim(),
            self.trusted_execution(c.request())?
                .with_deadline_limit(trial.deadline()),
            c.storage_claim(),
        )?;
        self.check_recovery_trial_control(trial, control, RecoveryAction::ConfirmWifi)
    }
    /// Trusted backend accounting, never a remote command. A failed authority
    /// gate must still allow recording actual rollback/Unknown after cleanup.
    pub fn finish_recovery_trial_from_trusted_core(
        &self,
        trial: &RecoveryTrialAuthority,
        control: Option<&RecoveryTrialControl>,
        completion: RecoveryExecutionCompletion,
    ) -> Result<RecoveryExecutionReceipt> {
        let b = trial.begin_claim();
        if b.request().capability().execution_binding().auth_instance() != self.instance
            || control.is_some_and(|c| !c.belongs_to(trial))
        {
            return Err(Error::Forbidden);
        }
        if completion == RecoveryExecutionCompletion::Saved
            && control.is_none_or(|c| c.action() != RecoveryAction::ConfirmWifi)
        {
            return Err(Error::InvalidInput);
        }
        trial.finish_from_trusted_core(control, completion)
    }
}
