// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit configurable Root AP worker. No public API, automatic service enable or save command.
use super::*;
use crate::{
    channel::station::{self, Intent, Operation, Request},
    station_trial::{self, StationProvider, TrialGuard, TrialOutcome},
};
use zeroize::Zeroizing;

async fn stop(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() || rx.changed().await.is_err() {
            return;
        }
    }
}
fn request(worker: &Worker, binding: &Binding) -> Result<Option<Request>> {
    let Some(raw) = worker.input.read(Slot::StationRequest)? else {
        return Ok(None);
    };
    let raw = Zeroizing::new(raw);
    match Request::parse(raw.as_slice(), binding, channel::uptime_ms()?) {
        Ok(r) => Ok(Some(r)),
        // Expired/wrong-session payloads do not become new hardware work.
        Err(_) => Ok(None),
    }
}
fn status(intent: Intent, phase: station::Phase) -> Result<station::Status> {
    let s = station::Status {
        format: 1,
        intent,
        phase,
        sample_uptime_ms: channel::uptime_ms()?,
        ap_https_verified: matches!(phase, station::Phase::Ready | station::Phase::Failed),
    };
    s.validate(s.sample_uptime_ms)?;
    Ok(s)
}
fn publish_trial(worker: &Worker, status: &station::Status) -> Result<()> {
    status.validate(channel::uptime_ms()?)?;
    worker.output.publish(Slot::StationStatus, status)
}

#[allow(clippy::too_many_arguments)]
async fn trial<N: Network + StationProvider, G: Guard + TrialGuard, R: Ready + Sync>(
    worker: &Worker,
    network: &mut N,
    guard: &G,
    readiness: &R,
    gate: &LeaseGate<'_, R>,
    ap: &Credentials,
    r: Request,
    stopped: &watch::Receiver<bool>,
) -> Result<station::Status> {
    let intent = r.intent()?;
    worker.journal.consume_trial(&intent)?;
    // A durable at-most-once intent precedes both acknowledgement and every station mutation.
    publish_trial(worker, &status(intent.clone(), station::Phase::Testing)?)?;
    let credentials = r.credentials()?;
    let window = Duration::from_millis(
        r.deadline_uptime_ms
            .checked_sub(channel::uptime_ms()?)
            .ok_or(Error::Denied)?,
    );
    let (cancel, rx) = watch::channel(false);
    let mut session = network.station_session(ap)?;
    let work = station_trial::run(&mut session, guard, gate, &credentials, window, rx);
    tokio::pin!(work);
    let mut poll = tokio::time::interval(Duration::from_millis(500));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut publication_failed = false;
    let outcome = loop {
        tokio::select! {
            biased;
            result=&mut work=>break result,
            _=poll.tick()=>{
                if *stopped.borrow() || stopped.has_changed().is_err()
                    || !timeout(Duration::from_secs(4),gate.check()).await.is_ok_and(|v|v.is_ok())
                { cancel.send_replace(true); }
                match request(worker,&intent.binding) {
                    Ok(Some(r)) if r.cancels(&intent)=>{ cancel.send_replace(true); }
                    Err(_)=>{ publication_failed=true;cancel.send_replace(true); }
                    _=>{}
                }
                if !publication_failed {
                    let a=status(intent.clone(),station::Phase::Testing).and_then(|s|publish_trial(worker,&s));
                    let b=receipt(&intent.binding,State::Starting,gate.sequence.load(Ordering::SeqCst)).and_then(|s|publish(&worker.output,&s));
                    if a.is_err() || b.is_err() { publication_failed=true;cancel.send_replace(true); }
                }
            }
        }
    };
    // Do not drop the trial on shutdown, lost lease, failed publication or cancellation.
    // station_trial::run has now completed its bounded exact AP restoration attempt.
    let valid = !publication_failed
        && !*stopped.borrow()
        && stopped.has_changed().is_ok()
        && timeout(Duration::from_secs(4), gate.check())
            .await
            .is_ok_and(|v| v.is_ok());
    let restored = matches!(
        outcome,
        Ok(TrialOutcome::UsableAndRestored | TrialOutcome::FailedAndRestored)
    );
    let https = valid
        && restored
        && timeout(Duration::from_secs(3), readiness.ap_check())
            .await
            .is_ok_and(|v| v.is_ok())
        && !*stopped.borrow()
        && stopped.has_changed().is_ok()
        && timeout(Duration::from_secs(4), guard.verify_trial())
            .await
            .is_ok_and(|v| v.is_ok())
        && timeout(Duration::from_secs(4), gate.check())
            .await
            .is_ok_and(|v| v.is_ok());
    let phase = if !https {
        station::Phase::Unknown
    } else if matches!(outcome, Ok(TrialOutcome::UsableAndRestored)) {
        station::Phase::Ready
    } else {
        station::Phase::Failed
    };
    let done = status(intent, phase)?;
    worker.journal.finish_trial(&done)?;
    publish_trial(worker, &done)?;
    Ok(done)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn execute<
    N: Network + StationProvider,
    G: Guard + TrialGuard,
    R: Ready + Sync,
>(
    worker: &Worker,
    network: &mut N,
    guard: &mut G,
    readiness: &R,
    ap: &Credentials,
    hint: Hint,
    mut stopped: watch::Receiver<bool>,
) -> Result<Status> {
    let lease = RootLease::begin(
        hint.clone(),
        channel::uptime_ms()?,
        &worker.boot,
        &worker.realm,
        &worker.key,
        &worker.material,
    )?;
    let gate = LeaseGate {
        input: &worker.input,
        readiness,
        lease: Mutex::new(lease),
        sequence: AtomicU64::new(1),
        boot: worker.boot.clone(),
    };
    if *stopped.borrow() || stopped.has_changed().is_err() {
        return Err(Error::Denied);
    }
    worker.journal.consume(&hint.binding)?;
    publish(&worker.output, &receipt(&hint.binding, State::Starting, 1)?)?;
    let mut guard_attempted = false;
    let remaining = hint
        .binding
        .deadline_uptime_ms
        .checked_sub(channel::uptime_ms()?)
        .ok_or(Error::Denied)?;
    let started = {
        let start = async {
            gate.check().await?;
            network.preflight().await?;
            gate.check().await?;
            guard_attempted = true;
            guard.install().await?;
            guard.verify_trial().await?;
            gate.check().await?;
            let seconds = u32::try_from(
                hint.binding
                    .deadline_uptime_ms
                    .saturating_sub(channel::uptime_ms()?)
                    / 1000,
            )
            .map_err(|_| Error::Invalid)?;
            network.checkpoint(seconds.saturating_add(30)).await?;
            gate.check().await?;
            guard.verify().await?;
            network.activate(ap).await
        };
        tokio::pin!(start);
        tokio::select! {
            biased;
            _=stop(&mut stopped)=>false,
            value=timeout(Duration::from_millis(remaining.min(20_000)),&mut start)=>matches!(value,Ok(Ok(()))),
        }
    };
    let _body:Result<()>=async {
    if started {
        let mut seen=false;let initial=Instant::now();let mut last:Option<station::Status>=None;let mut trials=0;
        loop {
            if *stopped.borrow() || stopped.has_changed().is_err() || channel::uptime_ms()?>=hint.binding.deadline_uptime_ms { break; }
            if !timeout(Duration::from_secs(4),gate.check()).await.is_ok_and(|v|v.is_ok())
                || !timeout(Duration::from_secs(4),guard.verify()).await.is_ok_and(|v|v.is_ok()) { break; }
            let up=timeout(Duration::from_secs(3),network.ready(ap)).await.is_ok_and(|v|v==Ok(true));
            let https=up && timeout(Duration::from_secs(3),readiness.ap_check()).await.is_ok_and(|v|v.is_ok());
            if !https {
                if seen || initial.elapsed()>=Duration::from_secs(10) { break; }
            } else {
                seen=true;
                if let Some(s)=last.as_mut() { s.sample_uptime_ms=channel::uptime_ms()?;if publish_trial(worker,s).is_err() { break; } }
                match request(worker,&hint.binding) {
                    Ok(Some(r)) if matches!(r.operation,Operation::Test{..})=>{
                        let intent=r.intent()?;
                        match worker.journal.trial_consumed(&intent) {
                            Ok(true)=>{
                                let saved=worker.journal.trial_result(&intent)?;
                                let mut saved=saved.unwrap_or(status(intent,station::Phase::Unknown)?);
                                saved.sample_uptime_ms=channel::uptime_ms()?;
                                if saved.phase==station::Phase::Unknown || publish_trial(worker,&saved).is_err() { break; }
                                last=Some(saved);
                            }
                            Ok(false) if trials<16=>{
                                trials+=1;
                                match trial(worker,network,guard,readiness,&gate,ap,r,&stopped).await {
                                    Ok(s) if s.phase!=station::Phase::Unknown=>{ last=Some(s); }
                                    _=>break,
                                }
                            }
                            _=>break,
                        }
                    }
                    Err(_)=>break,
                    _=>{}
                }
            }
            if publish(&worker.output,&receipt(&hint.binding,if https { State::Ready } else { State::Starting },gate.sequence.load(Ordering::SeqCst))?).is_err() { break; }
            tokio::select! { _=stop(&mut stopped)=>break, _=tokio::time::sleep(Duration::from_millis(500))=>{} }
        }
    }
    Ok(())
    }.await;
    // If guard installation was attempted, only proven full AP cleanup permits release.
    let restored = !guard_attempted
        || timeout(Duration::from_secs(10), network.rollback(ap))
            .await
            .is_ok_and(|v| v.is_ok());
    let released = restored
        && (!guard_attempted
            || timeout(Duration::from_secs(5), guard.release())
                .await
                .is_ok_and(|v| v.is_ok()));
    let done = receipt(
        &hint.binding,
        if released {
            State::Closed
        } else {
            State::Unknown
        },
        gate.sequence.load(Ordering::SeqCst),
    )?;
    if released {
        worker.journal.finish_closed(&done, done.sample_uptime_ms)?;
    }
    publish(&worker.output, &done)?;
    Ok(done)
}

#[cfg(test)]
mod tests;
