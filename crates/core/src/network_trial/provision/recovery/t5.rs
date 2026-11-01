// SPDX-License-Identifier: AGPL-3.0-only
//! Frozen T5 application results from the original verified Core dispatcher.
//! No encryption/session framing, listener, status probe or retry is installed.
use super::*;
use rustsimhub_storage::RecoveryExecutionAction;

const QUERY_DEADLINE: Duration = Duration::from_secs(3);

/// Output only. Callers cannot construct an authoritative result from payload
/// fields, an internal RecoveryReply, or a deserialized backend success flag.
#[derive(Serialize)]
#[serde(untagged)]
pub enum RecoveryResponseV1 {
    Operation(OperationV1),
    Lookup(LookupV1),
}
#[derive(Serialize)]
pub struct OperationV1 {
    contract: u8,
    source: &'static str,
    request_id: String,
    action: &'static str,
    trial_id: Option<String>,
    state: State,
    authoritative: bool,
    settings_saved: Option<bool>,
    rollback_verified: Option<bool>,
    expires_in_ms: Option<u32>,
}
#[derive(Serialize)]
pub struct LookupV1 {
    contract: u8,
    source: &'static str,
    target_request_id: String,
    lookup: &'static str,
    operation: Option<OperationV1>,
}
/// Finite error only; no raw error strings, authenticated bodies or frames.
#[derive(Debug, Serialize)]
pub struct RecoveryErrorV1 {
    contract: u8,
    code: &'static str,
}
impl RecoveryErrorV1 {
    fn deadline() -> Self {
        Self {
            contract: 1,
            code: "deadline",
        }
    }
}
impl From<Error> for RecoveryErrorV1 {
    fn from(error: Error) -> Self {
        Self {
            contract: 1,
            code: match error {
                Error::Busy => "busy",
                Error::InvalidInput => "invalid_input",
                Error::Forbidden | Error::Replay => "permission_denied",
                Error::Expired => "deadline",
                Error::Io | Error::Storage => "unknown",
                Error::Entropy | Error::CorruptIdentity => "unavailable",
            },
        }
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
fn stored_action(action: RecoveryExecutionAction) -> &'static str {
    match action {
        RecoveryExecutionAction::BeginWifi => "begin_wifi",
        RecoveryExecutionAction::ConfirmWifi => "confirm_wifi",
        RecoveryExecutionAction::CancelWifi => "cancel_wifi",
    }
}
fn milliseconds(remaining: Duration) -> u32 {
    remaining.min(DEADLINE).as_millis() as u32
}
impl OperationV1 {
    fn unknown(request_id: String, action: &'static str, trial_id: Option<String>) -> Self {
        Self {
            contract: 1,
            source: "core",
            request_id,
            action,
            trial_id,
            state: State::Unknown,
            authoritative: false,
            settings_saved: None,
            rollback_verified: None,
            expires_in_ms: None,
        }
    }
    fn terminal(&mut self, state: RecoveryExecutionState) {
        match state {
            RecoveryExecutionState::Saved => {
                self.state = State::Saved;
                self.authoritative = true;
                self.settings_saved = Some(true);
                self.expires_in_ms = Some(0);
            }
            RecoveryExecutionState::RolledBack => {
                self.state = State::RolledBack;
                self.authoritative = true;
                self.settings_saved = Some(false);
                self.rollback_verified = Some(true);
                self.expires_in_ms = Some(0);
            }
            _ => {}
        }
    }
}
/// Opt-in local application entry. The accepted RecoveryChannel must stay
/// alive. A's secure adapter must encrypt/route this typed output; this API
/// neither starts a listener nor makes a frontend provider available.
pub async fn recovery_v1(
    auth: Arc<Mutex<Auth>>,
    request: AuthorizedRecoveryRequest,
) -> std::result::Result<RecoveryResponseV1, RecoveryErrorV1> {
    if !enabled() {
        return Err(Error::Forbidden.into());
    }
    execute(auth, request, |trial| NmProvision {
        recovery: Some(trial),
        ..NmProvision::default()
    })
    .await
}
pub(super) async fn execute<B: ProvisionBackend + Send + 'static>(
    auth: Arc<Mutex<Auth>>,
    request: AuthorizedRecoveryRequest,
    factory: impl FnOnce(Arc<CoreRecoveryTrial>) -> B,
) -> std::result::Result<RecoveryResponseV1, RecoveryErrorV1> {
    // Only this private, actually checked capability selects the read timeout.
    // A write timeout must never cancel dispatch and then replay a new write.
    let query = request.capability.action() == RecoveryAction::QueryOperation;
    let work = async {
        let a = auth.clone();
        let decoded = with_auth(a, move |a| recovery_body::decode_v1(a, request)).await?;
        dispatch_v1(auth, decoded, factory).await
    };
    if query {
        timeout(QUERY_DEADLINE, work)
            .await
            .map_err(|_| RecoveryErrorV1::deadline())?
            .map_err(Into::into)
    } else {
        work.await.map_err(Into::into)
    }
}
async fn dispatch_v1<B: ProvisionBackend + Send + 'static>(
    auth: Arc<Mutex<Auth>>,
    decoded: DecodedRecoveryRequest,
    factory: impl FnOnce(Arc<CoreRecoveryTrial>) -> B,
) -> Result<RecoveryResponseV1> {
    let (request_id, action, target) = match &decoded {
        DecodedRecoveryRequest::Write(w) => {
            (w.request_id().to_owned(), action_name(w.action())?, None)
        }
        DecodedRecoveryRequest::Lookup(q) => {
            (String::new(), "", Some(q.target_request_id().to_owned()))
        }
    };
    let reply = dispatch(auth.clone(), decoded, factory).await?;
    let operation = match reply {
        RecoveryReply::Lookup(receipt) => {
            let target_request_id = target.ok_or(Error::InvalidInput)?;
            let operation = match receipt {
                Some(r) => Some(from_receipt(auth, r).await),
                None => None,
            };
            return Ok(RecoveryResponseV1::Lookup(LookupV1 {
                contract: 1,
                source: "core",
                target_request_id,
                lookup: if operation.is_some() {
                    "found"
                } else {
                    "not_found"
                },
                operation,
            }));
        }
        RecoveryReply::Existing(r) => from_receipt(auth, r).await,
        RecoveryReply::Started {
            request_id,
            trial_id,
            remaining,
        } => {
            let mut op = OperationV1::unknown(request_id, action, Some(trial_id));
            // This result came from a newly spawned original actor, not a
            // historical Claimed row. It does not claim readiness or saving.
            if !remaining.is_zero() {
                op.state = State::Starting;
                op.authoritative = true;
                op.settings_saved = Some(false);
                op.expires_in_ms = Some(milliseconds(remaining));
            }
            op
        }
        RecoveryReply::Completed {
            request_id,
            trial_id,
            state,
        } => {
            let mut op = OperationV1::unknown(request_id, action, Some(trial_id));
            // The original actor replies only after backend completion AND
            // durable terminal accounting. Unknown never becomes success.
            op.terminal(state);
            op
        }
    };
    if operation.request_id != request_id {
        return Err(Error::InvalidInput);
    }
    Ok(RecoveryResponseV1::Operation(operation))
}
pub(super) async fn from_receipt(
    auth: Arc<Mutex<Auth>>,
    receipt: RecoveryExecutionReceipt,
) -> OperationV1 {
    let mut op = OperationV1::unknown(
        receipt.request_id().to_owned(),
        stored_action(receipt.action()),
        receipt.trial_id().map(str::to_owned),
    );
    let state = receipt.state();
    if (state == RecoveryExecutionState::Saved && receipt.settings_saved() == Some(true))
        || (state == RecoveryExecutionState::RolledBack
            && receipt.settings_saved() == Some(false)
            && receipt.rollback_verified() == Some(true))
    {
        op.terminal(state);
        return op;
    }
    let expected = match state {
        RecoveryExecutionState::Starting => State::Starting,
        RecoveryExecutionState::Connecting => State::Connecting,
        RecoveryExecutionState::Ready => State::Ready,
        RecoveryExecutionState::Saving => State::Saving,
        // A reservation/claim is not a running/Ready actor. Rejected has no
        // T5 equivalent. Unknown cannot be upgraded even by a late live task.
        _ => return op,
    };
    let live = (|| {
        let active = ACTIVE.try_lock().ok()?;
        let session = active.as_ref()?;
        let TrialOwner::Recovery(trial) = &session.owner else {
            return None;
        };
        if !trial.authority().matches_receipt(&receipt)
            || *session.state.try_lock().ok()? != expected
            || session.deadline <= Instant::now()
        {
            return None;
        }
        Some((
            trial.clone(),
            session.deadline.min(trial.authority().deadline()),
        ))
    })();
    if let Some((trial, deadline)) = live {
        // This also rejects a different Auth instance, a dead original session
        // or revoked authority. Reconnection can read history, not take over.
        if with_auth(auth, move |a| a.check_recovery_trial(trial.authority()))
            .await
            .is_ok()
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                op.state = expected;
                op.authoritative = true;
                op.settings_saved = Some(false);
                op.expires_in_ms = Some(milliseconds(remaining));
            }
        }
    }
    op
}
