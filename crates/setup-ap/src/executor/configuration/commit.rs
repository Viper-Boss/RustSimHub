// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{
    channel::commit::{self as protocol, Intent, Phase, Request, Status},
    station_commit::{self, CommitAudit, CommitOutcome},
};
pub(super) enum Completion {
    Saved,
    Restored,
    Unknown,
}
pub(crate) fn status(intent: Intent, phase: Phase) -> Result<Status> {
    let boot = channel::boot_id()?;
    let time = channel::uptime_ms()?;
    let execution = if boot == intent.trial.binding.boot_id {
        time
    } else {
        intent.requested_uptime_ms
    };
    let saved = phase == Phase::Saved;
    let s = Status {
        format: 1,
        intent,
        phase,
        execution_uptime_ms: execution,
        observation_boot_id: boot.clone(),
        sample_uptime_ms: time,
        settings_saved: saved,
        ap_removed: saved,
        checkpoints_cleared: saved,
        guard_released: saved,
        ap_https_verified: phase == Phase::RestoredAp,
    };
    s.validate(time, &boot)?;
    Ok(s)
}
struct Audit<'a> {
    worker: &'a Worker,
    intent: Intent,
}
impl CommitAudit for Audit<'_> {
    fn consume(&self) -> Result<()> {
        self.worker.journal.consume_commit(&self.intent)?;
        self.worker.output.publish(
            Slot::StationCommitStatus,
            &status(self.intent.clone(), Phase::Saving)?,
        )
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn execute<N: Network + CommitProvider, G: Guard + TrialGuard, R: Ready + Sync>(
    worker: &Worker,
    network: &mut N,
    guard: &mut G,
    readiness: &R,
    gate: &LeaseGate<'_, R>,
    ap: &Credentials,
    q: Request,
    stopped: &watch::Receiver<bool>,
) -> Result<Completion> {
    let intent = q.intent()?;
    // A consumed confirmation can only yield its existing proof. Never run it again.
    if worker.journal.commit_consumed(&intent)? {
        if let Some(mut done) = worker.journal.commit_result(&intent)? {
            done.refresh()?;
            worker.output.publish(Slot::StationCommitStatus, &done)?;
            return Ok(match done.phase {
                Phase::Saved => Completion::Saved,
                Phase::RestoredAp => Completion::Restored,
                _ => Completion::Unknown,
            });
        }
        worker.publish_commit(status(intent, Phase::Unknown)?)?;
        return Ok(Completion::Unknown);
    }
    let intent = worker
        .journal
        .admit_commit(&q, &q.trial.binding, channel::uptime_ms()?)?;
    let credentials = q.credentials()?;
    let remaining = q
        .deadline_uptime_ms
        .checked_sub(channel::uptime_ms()?)
        .ok_or(Error::Denied)?;
    let (cancel, rx) = watch::channel(false);
    let outcome = {
        let mut session = network.commit_session(ap)?;
        let audit = Audit {
            worker,
            intent: intent.clone(),
        };
        let work = station_commit::run(
            &mut session,
            guard,
            gate,
            &audit,
            &credentials,
            Duration::from_millis(remaining),
            rx,
        );
        tokio::pin!(work);
        let mut poll = tokio::time::interval(Duration::from_millis(500));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                result=&mut work=>break result,
                _=poll.tick()=>{
                    if *stopped.borrow() || stopped.has_changed().is_err()
                        || !timeout(Duration::from_secs(4),gate.check()).await.is_ok_and(|v|v.is_ok()) {cancel.send_replace(true);}
                    if let Ok(Some(r))=super::request(worker,&intent.trial.binding) {
                        if r.cancels(&intent.trial) {cancel.send_replace(true);}
                    }
                    // Actual run checks the live Root lease as well. Output failure
                    // requests cancellation but never drops cleanup responsibility.
                    if worker.journal.commit_consumed(&intent).unwrap_or(false) {
                        let publication=status(intent.clone(),Phase::Saving).and_then(|s|worker.output.publish(Slot::StationCommitStatus,&s));
                        let parent=receipt(&intent.trial.binding,State::Starting,gate.sequence.load(Ordering::SeqCst)).and_then(|s|publish(&worker.output,&s));
                        if publication.is_err() || parent.is_err() {cancel.send_replace(true);}
                    }
                }
            }
        }
    };
    // Err is possible only before mutation; do not turn a failed admission into
    // a confirmed save. Any consumed intent without proof stays Unknown.
    let outcome = match outcome {
        Ok(v) => v,
        Err(e) => {
            if !worker.journal.commit_consumed(&intent)? {
                return Err(e);
            }
            CommitOutcome::Unknown
        }
    };
    let phase = match outcome {
        CommitOutcome::Saved => Phase::Saved,
        CommitOutcome::RestoredAp
            if timeout(Duration::from_secs(3), readiness.ap_check())
                .await
                .is_ok_and(|r| r.is_ok()) =>
        {
            Phase::RestoredAp
        }
        _ => Phase::Unknown,
    };
    let done = match status(intent.clone(), phase) {
        Ok(s) => s,
        Err(_) => return Ok(Completion::Unknown),
    };
    // After the engine may have committed, every write failure must retain the
    // candidate/isolation state; it must never fall through legacy AP rollback.
    if worker.journal.finish_commit(&done).is_err() {
        return Ok(Completion::Unknown);
    }
    if worker
        .output
        .publish(Slot::StationCommitStatus, &done)
        .is_err()
    {
        return Ok(Completion::Unknown);
    }
    Ok(match phase {
        protocol::Phase::Saved => Completion::Saved,
        protocol::Phase::RestoredAp => Completion::Restored,
        _ => Completion::Unknown,
    })
}
