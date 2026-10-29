// SPDX-License-Identifier: AGPL-3.0-only
//! Durable private rescan consumer. The public Owner API remains Core's responsibility.
use super::*;
use crate::{
    ap_rescan::{self, RescanOutcome, RescanProvider},
    channel::rescan as protocol,
};

pub(super) fn request(worker: &Worker, binding: &Binding) -> Result<Option<protocol::Request>> {
    let Some(raw) = worker.input.read(Slot::RescanRequest)? else {
        return Ok(None);
    };
    let Ok(request) = serde_json::from_slice::<protocol::Request>(&raw) else {
        return Ok(None);
    };
    let now = channel::uptime_ms()?;
    // Keep structurally valid stale requests visible so they receive an explicit non-execution
    // decision. validate() remains mandatory before hardware admission.
    if request.intent().is_err()
        || request.binding != *binding
        || request.requested_uptime_ms > now
        || now >= binding.deadline_uptime_ms
    {
        return Ok(None);
    }
    Ok(Some(request))
}
pub(super) fn status(
    intent: protocol::Intent,
    phase: protocol::Phase,
    snapshot: Option<crate::nm::scan::Snapshot>,
) -> Result<protocol::Status> {
    let t = channel::uptime_ms()?;
    let boot = channel::boot_id()?;
    let finished = if phase == protocol::Phase::Scanning {
        None
    } else if boot == intent.binding.boot_id && t >= intent.requested_uptime_ms {
        Some(t)
    } else {
        None
    };
    let status = protocol::Status {
        format: 1,
        intent,
        phase,
        sample_boot_id: boot,
        sample_uptime_ms: t,
        finished_uptime_ms: finished,
        ap_https_verified: matches!(phase, protocol::Phase::Ready | protocol::Phase::Failed),
        snapshot,
        refusal: None,
    };
    status.validate(t, &status.sample_boot_id)?;
    Ok(status)
}
pub(super) fn publish_status(worker: &Worker, status: &protocol::Status) -> Result<()> {
    status.validate(channel::uptime_ms()?, &worker.boot)?;
    worker.output.publish(Slot::RescanStatus, status)
}
pub(super) fn capability(
    worker: &Worker,
    binding: &Binding,
    blocked: Option<protocol::Refusal>,
) -> Result<()> {
    let now = channel::uptime_ms()?;
    let admission = worker.journal.rescan_admission(binding, now)?;
    let refusal = blocked
        .or_else(|| {
            (binding.deadline_uptime_ms.saturating_sub(now) < 60_000)
                .then_some(protocol::Refusal::WindowClosing)
        })
        .or(admission.refusal);
    let c = protocol::Capability {
        format: 1,
        binding: binding.clone(),
        sample_uptime_ms: now,
        available: refusal.is_none(),
        max_per_parent: protocol::MAX_PER_PARENT,
        cooldown_ms: protocol::COOLDOWN_MS,
        remaining: admission.remaining,
        refusal,
        retry_after_uptime_ms: if refusal == Some(protocol::Refusal::Cooldown) {
            admission.retry_after_uptime_ms
        } else {
            None
        },
    };
    protocol::Capability::parse(
        &serde_json::to_vec(&c).map_err(|_| Error::Invalid)?,
        binding,
        c.sample_uptime_ms,
        &worker.boot,
    )?;
    worker.output.publish(Slot::RescanCapability, &c)
}
pub(in crate::executor) fn cached(
    worker: &Worker,
    intent: &protocol::Intent,
) -> Result<Option<protocol::Status>> {
    if !worker.journal.rescan_consumed(intent)? {
        return Ok(None);
    }
    let mut saved = match worker.journal.rescan_result(intent)? {
        Some(s) => s,
        None => {
            let unknown = status(intent.clone(), protocol::Phase::Unknown, None)?;
            worker.journal.finish_rescan(&unknown)?;
            unknown
        }
    };
    saved.refresh()?;
    publish_snapshot(worker, &saved)?;
    publish_status(worker, &saved)?;
    Ok(Some(saved))
}
fn publish_snapshot(worker: &Worker, status: &protocol::Status) -> Result<()> {
    let Some(snapshot) = status.snapshot.as_ref() else {
        return Ok(());
    };
    let binding = &status.intent.binding;
    let now = channel::uptime_ms()?;
    if binding.boot_id != worker.boot
        || binding.realm != worker.realm
        || binding.device_key_sha256 != worker.key
        || binding.material_sha256 != worker.material
        || now >= binding.deadline_uptime_ms
        || worker.journal.pending()?.as_ref() != Some(binding)
    {
        return Ok(());
    }
    let proof = channel::scan::Proof {
        format: 1,
        binding: binding.clone(),
        snapshot: snapshot.clone(),
    };
    proof.validate(binding, now)?;
    if let Some(raw) = worker.output.read(Slot::ScanSnapshot)? {
        let current: channel::scan::Proof =
            serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
        if current.binding == *binding {
            current.validate(binding, now)?;
            // Re-querying an older completed operation must not roll the list backwards.
            if current.snapshot.completed_uptime_ms >= snapshot.completed_uptime_ms {
                return Ok(());
            }
        }
    }
    worker.output.publish(Slot::ScanSnapshot, &proof)
}
fn refuse(
    worker: &Worker,
    intent: protocol::Intent,
    reason: protocol::Refusal,
) -> Result<protocol::Status> {
    let mut s = status(intent, protocol::Phase::Unknown, None)?;
    s.phase = protocol::Phase::Refused;
    s.refusal = Some(reason);
    s.validate(channel::uptime_ms()?, &worker.boot)?;
    worker.journal.refuse_rescan(&s)?;
    publish_status(worker, &s)?;
    Ok(s)
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle<N: Network + RescanProvider, G: Guard + TrialGuard, R: Ready + Sync>(
    worker: &Worker,
    network: &mut N,
    guard: &G,
    readiness: &R,
    gate: &LeaseGate<'_, R>,
    ap: &Credentials,
    last: Option<&station::Status>,
    barrier: Option<u64>,
    stopped: &watch::Receiver<bool>,
) -> Result<Option<protocol::Status>> {
    let Some(r) = request(worker, &gate.binding)? else {
        return Ok(None);
    };
    let intent = r.intent()?;
    if let Some(s) = cached(worker, &intent)? {
        return Ok(Some(s));
    }
    gate.check().await?;
    let now = channel::uptime_ms()?;
    // Missing terminal evidence from an accepted operation is not a mere refusal to a new
    // request. rescan_admission() fails closed and the caller retains the parent Unknown.
    let admission = worker
        .journal
        .rescan_admission(&gate.binding, r.requested_uptime_ms)?;
    let reason = if r.action == protocol::Action::Cancel {
        Some(protocol::Refusal::Cancelled)
    } else if gate.binding.deadline_uptime_ms.saturating_sub(now) < 60_000 {
        Some(protocol::Refusal::WindowClosing)
    } else if r.validate(&gate.binding, now).is_err() {
        Some(protocol::Refusal::Stale)
    } else if barrier.is_some_and(|t| r.requested_uptime_ms <= t) {
        Some(protocol::Refusal::Superseded)
    } else if conflicts(worker, &gate.binding, last)? {
        Some(protocol::Refusal::Conflict)
    } else {
        admission.refusal
    };
    if let Some(reason) = reason {
        return Ok(Some(refuse(worker, intent, reason)?));
    }
    capability(worker, &gate.binding, Some(protocol::Refusal::Conflict))?;
    execute(worker, network, guard, readiness, gate, ap, r, stopped)
        .await
        .map(Some)
}
pub(super) fn conflicts(
    worker: &Worker,
    binding: &Binding,
    last: Option<&station::Status>,
) -> Result<bool> {
    if worker.journal.commit_pending(binding)?.is_some() {
        return Ok(true);
    }
    let current = super::request(worker, binding)?;
    if let Some(r) = current.as_ref() {
        if matches!(r.operation, Operation::Test { .. })
            && !worker.journal.trial_consumed(&r.intent()?)?
        {
            return Ok(true);
        }
    }
    if let Some(last) = last {
        if matches!(
            last.phase,
            station::Phase::Unknown | station::Phase::Testing
        ) {
            return Ok(true);
        }
        if last.phase == station::Phase::Ready
            && !current.as_ref().is_some_and(|r| r.cancels(&last.intent))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn execute<N: Network + RescanProvider, G: Guard + TrialGuard, R: Ready + Sync>(
    worker: &Worker,
    network: &mut N,
    guard: &G,
    readiness: &R,
    gate: &LeaseGate<'_, R>,
    ap: &Credentials,
    request: protocol::Request,
    stopped: &watch::Receiver<bool>,
) -> Result<protocol::Status> {
    request.validate(&request.binding, channel::uptime_ms()?)?;
    if request.action != protocol::Action::Scan || request.binding != gate.binding {
        return Err(Error::Denied);
    }
    let intent = request.intent()?;
    gate.check().await?;
    worker.journal.consume_rescan(&intent)?;
    // Anything after durable admission can be uncertain; the caller keeps the parent guard
    // if this function fails or returns Unknown. No fallback to parent rollback here.
    publish_status(
        worker,
        &status(intent.clone(), protocol::Phase::Scanning, None)?,
    )?;
    let remaining = intent
        .deadline_uptime_ms
        .checked_sub(channel::uptime_ms()?)
        .ok_or(Error::Denied)?;
    let window = Duration::from_millis(remaining.min(ap_rescan::MAX_WORK_SECONDS * 1000));
    let (cancel, rx) = watch::channel(false);
    let mut publication_failed = false;
    let mut cancelled = false;
    let outcome = {
        let mut session = network.rescan_session(ap)?;
        let work = ap_rescan::run(&mut session, guard, gate, window, rx);
        tokio::pin!(work);
        let mut poll = tokio::time::interval(Duration::from_millis(500));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                result=&mut work=>break result,
                _=poll.tick()=>{
                    if *stopped.borrow() || stopped.has_changed().is_err()
                        || !timeout(Duration::from_secs(4),gate.check()).await.is_ok_and(|v|v.is_ok()) {cancelled=true;cancel.send_replace(true);}
                    match self::request(worker,&intent.binding) {
                        Ok(Some(r)) if r.action==protocol::Action::Scan && r.intent().is_ok_and(|i|i==intent)=>(),
                        _=>{cancelled=true;cancel.send_replace(true);}
                    }
                    if !publication_failed {
                        let a=status(intent.clone(),protocol::Phase::Scanning,None).and_then(|s|publish_status(worker,&s));
                        let b=receipt(&intent.binding,State::Starting,gate.sequence.load(Ordering::SeqCst)).and_then(|s|publish(&worker.output,&s));
                        if a.is_err() || b.is_err() {publication_failed=true;cancel.send_replace(true);}
                    }
                }
            }
        }
    };
    let restored = matches!(
        &outcome,
        Ok(RescanOutcome::ScannedAndRestored(_) | RescanOutcome::FailedAndRestored)
    );
    let valid = !publication_failed
        && !*stopped.borrow()
        && stopped.has_changed().is_ok()
        && channel::uptime_ms()? <= intent.deadline_uptime_ms
        && timeout(Duration::from_secs(4), gate.check())
            .await
            .is_ok_and(|r| r.is_ok());
    let https = valid
        && restored
        && timeout(Duration::from_secs(3), readiness.ap_check())
            .await
            .is_ok_and(|r| r.is_ok())
        && timeout(Duration::from_secs(4), guard.verify_trial())
            .await
            .is_ok_and(|r| r.is_ok())
        && timeout(Duration::from_secs(4), gate.check())
            .await
            .is_ok_and(|r| r.is_ok())
        && channel::uptime_ms()? <= intent.deadline_uptime_ms;
    let (phase, snapshot) = if !https {
        (protocol::Phase::Unknown, None)
    } else {
        match outcome {
            Ok(RescanOutcome::ScannedAndRestored(s)) if !cancelled => {
                (protocol::Phase::Ready, Some(s))
            }
            _ => (protocol::Phase::Failed, None),
        }
    };
    let done = status(intent, phase, snapshot)?;
    worker.journal.finish_rescan(&done)?;
    // Only an HTTPS-verified completed scan replaces the display snapshot, including an empty
    // one. The durable result precedes publication; loss of either output is read-only replay.
    publish_snapshot(worker, &done)?;
    publish_status(worker, &done)?;
    Ok(done)
}
