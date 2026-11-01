// SPDX-License-Identifier: AGPL-3.0-only
//! Opt-in verified recovery dispatch into the ONE existing Core Wi-Fi actor.
use super::*;
use crate::recovery_body::{
    self, CoreRecoveryAdmission, CoreRecoveryTrial, CoreRecoveryWrite, DecodedRecoveryRequest,
};
use rustsimhub_auth::recovery_capability::{AuthorizedRecoveryRequest, RecoveryAction};
use rustsimhub_storage::{
    RecoveryExecutionCompletion as Completion, RecoveryExecutionProgress as Progress,
    RecoveryExecutionReceipt, RecoveryExecutionState,
};

/// Internal Core result, not the frozen T5 wire Operation DTO. No credentials.
pub enum RecoveryReply {
    Existing(RecoveryExecutionReceipt),
    Started {
        request_id: String,
        trial_id: String,
        remaining: Duration,
    },
    Completed {
        request_id: String,
        trial_id: String,
        state: RecoveryExecutionState,
    },
    Lookup(Option<RecoveryExecutionReceipt>),
}
async fn with_auth<T: Send + 'static>(
    auth: Arc<Mutex<Auth>>,
    work: impl FnOnce(&Auth) -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(move || {
        let a = auth.try_lock().map_err(|_| Error::Busy)?;
        work(&a)
    })
    .await
    .map_err(|_| Error::Busy)?
}
/// Caller must retain its actual accepted RecoveryChannel. Dropping that
/// channel fences the capabilities; no new listener/verifier is configured.
pub async fn recovery(
    auth: Arc<Mutex<Auth>>,
    request: AuthorizedRecoveryRequest,
) -> Result<RecoveryReply> {
    if !enabled() {
        return Err(Error::Forbidden);
    }
    let a = auth.clone();
    let decoded = with_auth(a, move |a| recovery_body::decode_v1(a, request)).await?;
    match decoded {
        DecodedRecoveryRequest::Lookup(q) => {
            with_auth(auth, move |a| q.lookup(a).map(RecoveryReply::Lookup)).await
        }
        DecodedRecoveryRequest::Write(w) => match w.action() {
            RecoveryAction::BeginWifi => {
                begin_with(auth, w, |trial| NmProvision {
                    recovery: Some(trial),
                    ..NmProvision::default()
                })
                .await
            }
            RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi => {
                control_recovery(auth, w).await
            }
            _ => Err(Error::Forbidden),
        },
    }
}
async fn begin_with<B: ProvisionBackend + Send + 'static>(
    auth: Arc<Mutex<Auth>>,
    write: Box<CoreRecoveryWrite>,
    factory: impl FnOnce(Arc<CoreRecoveryTrial>) -> B,
) -> Result<RecoveryReply> {
    let mut active = ACTIVE.try_lock().map_err(|_| Error::Busy)?;
    if let Some(old) = active.as_ref() {
        if !old.state.lock().await.terminal() {
            return Err(Error::Busy);
        }
    }
    // All original exclusion BEFORE a new durable reservation. A normal trial
    // or call cannot strand a new Reserved row merely by making Core busy.
    let permit = TRIALS.try_acquire().map_err(|_| Error::Busy)?;
    let network = crate::coordination::global()
        .network()
        .map_err(|_| Error::Busy)?;
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Entropy)?;
    let id: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let trial = with_auth(auth.clone(), move |a| match write.reserve(a)? {
        CoreRecoveryAdmission::Existing(r) => Ok(Err(r)),
        CoreRecoveryAdmission::New(p) => Ok(Ok(p.claim(a, Some(id))?.begin(a)?)),
    })
    .await?;
    let trial = match trial {
        Ok(t) => Arc::new(t),
        Err(r) => return Ok(RecoveryReply::Existing(*r)),
    };
    let id = trial.authority().trial_id().to_owned();
    let request_id = trial.authority().request_id().to_owned();
    let deadline = trial.authority().deadline();
    let credentials = Credentials {
        ssid: trial.credentials().ssid().into(),
        passphrase: Zeroizing::new(trial.credentials().passphrase().into()),
    };
    let state = Arc::new(tokio::sync::Mutex::new(State::Starting));
    let (tx, mut rx) = mpsc::channel(1);
    *active = Some(Session {
        id: id.clone(),
        key: request_id.clone(),
        owner: TrialOwner::Recovery(trial.clone()),
        state: state.clone(),
        control: tx,
        deadline,
    });
    let mut backend = factory(trial.clone());
    let gate = RecoveryContext { auth, trial };
    tokio::spawn(async move {
        run_trial(
            &mut backend,
            &gate,
            credentials,
            &id,
            &gate,
            &state,
            &mut rx,
        )
        .await;
        // The active slot retains metadata, never the finished trial's raw
        // authenticated body/SSID/password. Context/backend drop after drain.
        {
            let mut active = ACTIVE.lock().await;
            if let Some(s) = active.as_mut() {
                if matches!(&s.owner,TrialOwner::Recovery(t) if Arc::ptr_eq(t,&gate.trial)) {
                    s.owner = TrialOwner::RecoveryClosed;
                }
            }
        }
        drop(backend);
        drop(gate);
        drop(network);
        drop(permit);
    });
    Ok(RecoveryReply::Started {
        request_id,
        trial_id: active.as_ref().ok_or(Error::Io)?.id.clone(),
        remaining: deadline.saturating_duration_since(Instant::now()),
    })
}
async fn control_recovery(
    auth: Arc<Mutex<Auth>>,
    write: Box<CoreRecoveryWrite>,
) -> Result<RecoveryReply> {
    let action = write.action();
    let confirm = action == RecoveryAction::ConfirmWifi;
    let request_id = write.request_id().to_owned();
    let id = write.target_trial().ok_or(Error::InvalidInput)?.to_owned();
    let (tx, trial) = {
        let active = ACTIVE.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_ref().ok_or(Error::Forbidden)?;
        let TrialOwner::Recovery(trial) = &s.owner else {
            return Err(Error::Forbidden);
        };
        let state = *s.state.lock().await;
        if s.id != id
            || state.terminal()
            || state == State::Saving
            || Instant::now() >= s.deadline
            || (confirm && state != State::Ready)
        {
            return Err(Error::Forbidden);
        }
        (s.control.clone(), trial.clone())
    };
    // Reserve the one existing control slot BEFORE claiming. A busy channel
    // must not consume a private claim which cannot be dispatched to its actor.
    let slot = tx.try_reserve().map_err(|_| Error::Busy)?;
    let control = with_auth(auth.clone(), move |a| {
        a.check_recovery_trial(trial.authority())?;
        // Bind the session before durable control admission, not only after.
        write.check_trial(a, &trial)?;
        match write.reserve(a)? {
            CoreRecoveryAdmission::Existing(r) => Ok(Err(r)),
            CoreRecoveryAdmission::New(p) => Ok(Ok(p.claim(a, None)?.bind_control(a, &trial)?)),
        }
    })
    .await?;
    let control = match control {
        Ok(c) => Arc::new(c),
        Err(r) => return Ok(RecoveryReply::Existing(*r)),
    };
    let (reply, rx) = oneshot::channel();
    slot.send(Control {
        authority: ControlAuthority::Recovery(control),
        confirm,
        reply,
    });
    let state = timeout(Duration::from_secs(8), rx)
        .await
        .map_err(|_| Error::Busy)?
        .map_err(|_| Error::Busy)?;
    Ok(RecoveryReply::Completed {
        request_id,
        trial_id: id,
        state: match state {
            State::Starting => RecoveryExecutionState::Starting,
            State::Connecting => RecoveryExecutionState::Connecting,
            State::Ready => RecoveryExecutionState::Ready,
            State::Saving => RecoveryExecutionState::Saving,
            State::Saved => RecoveryExecutionState::Saved,
            State::RolledBack => RecoveryExecutionState::RolledBack,
            State::Unknown => RecoveryExecutionState::Unknown,
        },
    })
}
struct RecoveryContext {
    auth: Arc<Mutex<Auth>>,
    trial: Arc<CoreRecoveryTrial>,
}
impl Gate for RecoveryContext {
    async fn check(&self) -> Result<()> {
        let t = self.trial.clone();
        with_auth(self.auth.clone(), move |a| {
            a.check_recovery_trial(t.authority())
        })
        .await
    }
}
impl ProvisionGate for RecoveryContext {
    async fn check_control(&self, proof: &ControlAuthority, confirm: bool) -> Result<()> {
        let c = match proof {
            ControlAuthority::Recovery(c) => c.clone(),
            ControlAuthority::Shutdown if !confirm => return Ok(()),
            _ => return Err(Error::Forbidden),
        };
        let t = self.trial.clone();
        with_auth(self.auth.clone(), move |a| {
            a.check_recovery_trial_control(
                t.authority(),
                &c,
                if confirm {
                    RecoveryAction::ConfirmWifi
                } else {
                    RecoveryAction::CancelWifi
                },
            )
        })
        .await
    }
}
impl TrialAudit for RecoveryContext {
    fn limit(&self) -> Duration {
        self.trial.authority().remaining()
    }
    fn deadline(&self) -> Instant {
        self.trial.authority().deadline()
    }
    async fn progress(&self, state: State) -> Result<()> {
        let p = match state {
            State::Starting => Progress::Starting,
            State::Connecting => Progress::Connecting,
            State::Ready => Progress::Ready,
            _ => return Err(Error::InvalidInput),
        };
        let t = self.trial.clone();
        with_auth(self.auth.clone(), move |a| {
            a.record_recovery_trial_progress_from_trusted_core(t.authority(), p)
                .map(|_| ())
        })
        .await
    }
    async fn confirm_control(&self, _: &str, proof: &ControlAuthority) -> Result<()> {
        let ControlAuthority::Recovery(c) = proof else {
            return Err(Error::Forbidden);
        };
        let c = c.clone();
        let t = self.trial.clone();
        with_auth(self.auth.clone(), move |a| {
            a.record_recovery_trial_saving_from_trusted_core(t.authority(), &c)
        })
        .await
    }
    async fn finish_trial(&self, state: State, proof: Option<&ControlAuthority>) -> Result<()> {
        let completion = match state {
            State::Saved => Completion::Saved,
            State::RolledBack => Completion::RolledBack,
            _ => Completion::Unknown,
        };
        let c = match proof {
            Some(ControlAuthority::Recovery(c)) => Some(c.clone()),
            _ => None,
        };
        let t = self.trial.clone();
        with_auth(self.auth.clone(), move |a| {
            a.finish_recovery_trial_from_trusted_core(t.authority(), c.as_deref(), completion)
                .map(|_| ())
        })
        .await
    }
    async fn retain_unknown(&self) {
        let _ = self.finish_trial(State::Unknown, None).await;
    }
}

#[cfg(test)]
mod tests;
