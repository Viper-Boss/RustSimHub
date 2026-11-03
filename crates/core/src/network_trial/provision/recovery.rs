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
mod t5;
pub use t5::{recovery_v1, RecoveryErrorV1, RecoveryResponseV1};

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
    if matches!(
        request.capability.action(),
        RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi
    ) {
        return control_request(auth, request).await;
    }
    let a = auth.clone();
    let decoded = with_auth(a, move |a| recovery_body::decode_v1(a, request)).await?;
    let backend_auth = auth.clone();
    dispatch(auth, decoded, move |trial| NmProvision {
        recovery: Some(trial),
        recovery_auth: Some(backend_auth),
        ..NmProvision::default()
    })
    .await
}
async fn dispatch<B: ProvisionBackend + Send + 'static>(
    auth: Arc<Mutex<Auth>>,
    decoded: DecodedRecoveryRequest,
    factory: impl FnOnce(Arc<CoreRecoveryTrial>) -> B,
) -> Result<RecoveryReply> {
    match decoded {
        DecodedRecoveryRequest::Lookup(q) => {
            with_auth(auth, move |a| q.lookup(a).map(RecoveryReply::Lookup)).await
        }
        DecodedRecoveryRequest::Write(w) => match w.action() {
            RecoveryAction::BeginWifi => begin_with(auth, w, factory).await,
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
    control_recovery_inner(auth, write, AdmissionSchedule::default()).await
}
#[derive(Default)]
struct AdmissionSchedule {
    #[cfg(test)]
    held: Option<Box<dyn FnOnce() + Send + Sync>>,
}
impl AdmissionSchedule {
    fn held(self) {
        #[cfg(test)]
        if let Some(work) = self.held {
            work();
        }
    }
}
async fn control_recovery_inner(
    auth: Arc<Mutex<Auth>>,
    write: Box<CoreRecoveryWrite>,
    schedule: AdmissionSchedule,
) -> Result<RecoveryReply> {
    let action = write.action();
    let request_id = write.request_id().to_owned();
    let target = write.target_trial().map(str::to_owned);
    enqueue_control(
        auth,
        PendingInput::Decoded(write),
        action,
        request_id,
        target,
        schedule,
    )
    .await
}
enum PendingInput {
    Decoded(Box<CoreRecoveryWrite>),
    Authorized(Box<AuthorizedRecoveryRequest>),
}
pub(super) struct PendingControl {
    auth: Arc<Mutex<Auth>>,
    input: PendingInput,
    schedule: AdmissionSchedule,
    decision: oneshot::Sender<Result<Option<Box<RecoveryExecutionReceipt>>>>,
}
async fn control_request(
    auth: Arc<Mutex<Auth>>,
    request: AuthorizedRecoveryRequest,
) -> Result<RecoveryReply> {
    let action = request.capability.action();
    let request_id = uuid::Uuid::from_bytes(*request.capability.request_id()).to_string();
    enqueue_control(
        auth,
        PendingInput::Authorized(Box::new(request)),
        action,
        request_id,
        None,
        AdmissionSchedule::default(),
    )
    .await
}
async fn enqueue_control(
    auth: Arc<Mutex<Auth>>,
    input: PendingInput,
    action: RecoveryAction,
    request_id: String,
    target: Option<String>,
    schedule: AdmissionSchedule,
) -> Result<RecoveryReply> {
    if !matches!(
        action,
        RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi
    ) {
        return Err(Error::Forbidden);
    }
    let confirm = action == RecoveryAction::ConfirmWifi;
    let (tx, id) = {
        let active = ACTIVE.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_ref().ok_or(Error::Forbidden)?;
        if !matches!(&s.owner, TrialOwner::Recovery(_)) {
            return Err(Error::Forbidden);
        }
        let state = *s.state.lock().await;
        if target.as_ref().is_some_and(|id| id != &s.id)
            || state.terminal()
            || state == State::Saving
            || Instant::now() >= s.deadline
            || (confirm && state != State::Ready)
        {
            return Err(Error::Forbidden);
        }
        (s.control.clone(), s.id.clone())
    };
    // Capacity and actor ownership precede every control decode/reservation/
    // claim. No Auth lock spans a not-yet-enqueued control or a background tick.
    let slot = tx.try_reserve().map_err(|_| Error::Busy)?;
    let (decision, admitted) = oneshot::channel();
    let (reply, rx) = oneshot::channel();
    slot.send(Control {
        authority: ControlAuthority::RecoveryPending(Box::new(PendingControl {
            auth,
            input,
            schedule,
            decision,
        })),
        confirm,
        reply,
    });
    // This is a reply wait, not an operation deadline or a cancellation token.
    // The actor retains ownership after enqueue, including caller disconnect.
    let until = tokio::time::Instant::now() + Duration::from_secs(8);
    if let Some(receipt) = tokio::time::timeout_at(until, admitted)
        .await
        .map_err(|_| Error::Busy)?
        .map_err(|_| Error::Busy)??
    {
        return Ok(RecoveryReply::Existing(*receipt));
    }
    let state = tokio::time::timeout_at(until, rx)
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
pub(super) async fn check_native_trial(
    auth: Arc<Mutex<Auth>>,
    trial: Arc<CoreRecoveryTrial>,
) -> Result<()> {
    with_auth(auth, move |a| a.check_recovery_trial(trial.authority())).await
}
impl Gate for RecoveryContext {
    async fn check(&self) -> Result<()> {
        check_native_trial(self.auth.clone(), self.trial.clone()).await
    }
}
impl ProvisionGate for RecoveryContext {
    async fn admit_control(
        &self,
        authority: ControlAuthority,
        confirm: bool,
        ready: bool,
    ) -> Result<Option<ControlAuthority>> {
        let ControlAuthority::RecoveryPending(pending) = authority else {
            return Ok(Some(authority));
        };
        let PendingControl {
            auth,
            input,
            schedule,
            decision,
        } = *pending;
        if !Arc::ptr_eq(&auth, &self.auth) || confirm && !ready {
            let _ = decision.send(Err(Error::Forbidden));
            return Ok(None);
        }
        let trial = self.trial.clone();
        let admitted = with_auth(self.auth.clone(), move |a| {
            schedule.held();
            a.check_recovery_trial(trial.authority())?;
            let write = match input {
                PendingInput::Decoded(w) => w,
                PendingInput::Authorized(request) => match recovery_body::decode_v1(a, *request)? {
                    DecodedRecoveryRequest::Write(w) => w,
                    _ => return Err(Error::Forbidden),
                },
            };
            if write.action()
                != if confirm {
                    RecoveryAction::ConfirmWifi
                } else {
                    RecoveryAction::CancelWifi
                }
            {
                return Err(Error::Forbidden);
            }
            write.check_trial(a, &trial)?;
            match write.reserve(a)? {
                CoreRecoveryAdmission::Existing(receipt) => Ok(Err(receipt)),
                CoreRecoveryAdmission::New(p) => {
                    a.check_recovery_trial(trial.authority())?;
                    Ok(Ok(p.claim(a, None)?.bind_control(a, &trial)?))
                }
            }
        })
        .await;
        match admitted {
            Ok(Ok(control)) => {
                let _ = decision.send(Ok(None));
                Ok(Some(ControlAuthority::Recovery(Arc::new(control))))
            }
            Ok(Err(receipt)) => {
                let _ = decision.send(Ok(Some(receipt)));
                Ok(None)
            }
            Err(error) => {
                let _ = decision.send(Err(error));
                Ok(None)
            }
        }
    }
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
        tokio::task::spawn_blocking(move || {
            t.authority()
                .finish_from_trusted_core(c.as_deref(), completion)
                .map(|_| ())
        })
        .await
        .map_err(|_| Error::Busy)?
    }
    async fn retain_unknown(&self) {
        let _ = self.finish_trial(State::Unknown, None).await;
    }
}

#[cfg(test)]
mod tests;
