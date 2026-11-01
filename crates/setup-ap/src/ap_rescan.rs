// SPDX-License-Identifier: AGPL-3.0-only
//! Temporarily pause the owned AP, scan, and restore that same AP without saving a profile.
//! The trusted caller must durably consume the exact request before entering this transaction.
//! This module does not expose an endpoint, publish HTTPS readiness, or release the parent guard.
use crate::{nm::scan::Snapshot, station_trial::TrialGuard, transaction::Gate, Error, Result};
use std::{future::Future, time::Duration};
use tokio::{
    sync::watch,
    time::{timeout, timeout_at, Instant},
};

pub const MAX_WORK_SECONDS: u64 = 20;
pub trait RescanNetwork {
    fn pause_ap(&mut self) -> impl Future<Output = Result<()>> + Send;
    fn scan(&mut self) -> impl Future<Output = Result<Snapshot>> + Send;
    fn restore_ap(&mut self) -> impl Future<Output = Result<()>> + Send;
}
pub trait RescanProvider {
    fn rescan_session<'a>(
        &'a mut self,
        ap: &'a crate::Credentials,
    ) -> Result<impl RescanNetwork + Send + 'a>;
}
pub enum RescanOutcome {
    /// Radio scan completed and the exact AP/guard were restored. Caller still checks HTTPS.
    ScannedAndRestored(Snapshot),
    FailedAndRestored,
    Unknown,
}
async fn stopped(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() || rx.changed().await.is_err() {
            return;
        }
    }
}
pub async fn run<N: RescanNetwork, G: TrialGuard, A: Gate>(
    network: &mut N,
    guard: &G,
    gate: &A,
    window: Duration,
    mut stop: watch::Receiver<bool>,
) -> Result<RescanOutcome> {
    if window.is_zero() || window > Duration::from_secs(MAX_WORK_SECONDS) {
        return Err(Error::Invalid);
    }
    let expires = Instant::now() + window;
    if *stop.borrow() || stop.has_changed().is_err() {
        return Err(Error::Denied);
    }
    timeout(Duration::from_secs(3), gate.check())
        .await
        .map_err(|_| Error::Busy)??;
    timeout(Duration::from_secs(4), guard.verify_trial())
        .await
        .map_err(|_| Error::Busy)??;
    if Instant::now() >= expires {
        return Err(Error::Busy);
    }
    let started = crate::channel::uptime_ms()?;
    // Once pause_ap is polled, a lost reply may already have stopped the AP. Cleanup is
    // always awaited, including cancellation, authority loss and a failed scan.
    let report = {
        let work = async {
            gate.check().await?;
            guard.verify_trial().await?;
            network.pause_ap().await?;
            gate.check().await?;
            guard.verify_trial().await?;
            let snapshot = network.scan().await?;
            if !snapshot.valid()
                || snapshot.requested_uptime_ms < started
                || snapshot.completed_uptime_ms > crate::channel::uptime_ms()?
            {
                return Err(Error::Invalid);
            }
            gate.check().await?;
            guard.verify_trial().await?;
            Ok::<_, Error>(snapshot)
        };
        tokio::select! {
            biased;
            _=stopped(&mut stop)=>None,
            result=timeout_at(expires,work)=>result.ok().and_then(Result::ok),
        }
    };
    // Restoring exactly what we paused is allowed after authority loss. Never retry the
    // scan, pause another connection, or drop isolation to make an unknown outcome look good.
    let restored = timeout(Duration::from_secs(8), async {
        network.restore_ap().await?;
        guard.verify_trial().await
    })
    .await
    .is_ok_and(|r| r.is_ok());
    if !restored {
        return Ok(RescanOutcome::Unknown);
    }
    let authorized = !*stop.borrow()
        && stop.has_changed().is_ok()
        && timeout(Duration::from_secs(3), gate.check())
            .await
            .is_ok_and(|r| r.is_ok());
    Ok(match report {
        Some(snapshot) if authorized => RescanOutcome::ScannedAndRestored(snapshot),
        _ => RescanOutcome::FailedAndRestored,
    })
}
#[cfg(test)]
mod tests;
