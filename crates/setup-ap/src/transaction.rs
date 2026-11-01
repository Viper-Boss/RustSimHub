// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Credentials, Error, Result, MAX_WINDOW_SECONDS};
use serde::Serialize;
use std::{future::Future, time::Duration};
use tokio::{
    sync::watch,
    time::{timeout, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Preflight,
    Starting,
    Ready,
    Closing,
    Closed,
    Unknown,
}
/// Why preflight refused before any network change. Finite on purpose: no NM text is relayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    WifiDisabled,
    CheckpointExists,
    ApUnsupported,
    SubnetConflict,
    StationConnected,
    StationSaved,
    GsmAutoconnect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    RolledBack,
    Unknown,
}

/// Caller must bind a current Owner or explicit first-setup window, ownership proof,
/// TLS/AP origin readiness and Core's exclusive network/voice coordination lease.
/// Returning Ok from an unauthenticated or static "always allow" gate is not a product integration.
pub trait Gate {
    fn check(&self) -> impl Future<Output = Result<()>> + Send;
    fn publish_scan(
        &self,
        _snapshot: crate::nm::scan::Snapshot,
    ) -> impl Future<Output = Result<()>> + Send {
        async { Err(Error::Denied) }
    }
}
/// The guard exists BEFORE shared-mode activation and stays while rollback is uncertain.
pub trait Guard {
    fn install(&mut self) -> impl Future<Output = Result<()>> + Send;
    fn verify(&self) -> impl Future<Output = Result<()>> + Send;
    fn release(&mut self) -> impl Future<Output = Result<()>> + Send;
}
pub trait Network {
    fn preflight(&mut self) -> impl Future<Output = Result<()>> + Send;
    fn scan_before_start(
        &mut self,
    ) -> impl Future<Output = Result<Option<crate::nm::scan::Snapshot>>> + Send {
        async { Ok(None) }
    }
    fn checkpoint(&mut self, seconds: u32) -> impl Future<Output = Result<()>> + Send;
    fn activate(&mut self, credentials: &Credentials) -> impl Future<Output = Result<()>> + Send;
    fn ready(&self, credentials: &Credentials) -> impl Future<Output = Result<bool>> + Send;
    fn rollback(&mut self, credentials: &Credentials) -> impl Future<Output = Result<()>> + Send;
    /// The specific reason the last preflight refused, if it was a known precondition.
    fn refusal(&self) -> Option<Refusal> {
        None
    }
}
/// Collect the catalogue before any AP/checkpoint/forwarding action, within the original lease.
pub(crate) async fn scan_before_start(network: &mut impl Network, gate: &impl Gate) -> Result<()> {
    timeout(Duration::from_secs(5), gate.check())
        .await
        .map_err(|_| Error::Busy)??;
    let report = timeout(
        Duration::from_secs(crate::nm::scan::MAX_SCAN_SECONDS + 1),
        network.scan_before_start(),
    )
    .await
    .map_err(|_| Error::Busy)??;
    timeout(Duration::from_secs(5), gate.check())
        .await
        .map_err(|_| Error::Busy)??;
    if let Some(report) = report {
        timeout(Duration::from_secs(2), gate.publish_scan(report))
            .await
            .map_err(|_| Error::Busy)??;
    }
    Ok(())
}
async fn stop(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
/// One finite AP session, with no automatic retry, persistence, lease extension or save path.
/// Callers must retain an Unknown result and the guard for later explicit reconciliation.
pub async fn run<N: Network, G: Guard, A: Gate>(
    network: &mut N,
    guard: &mut G,
    gate: &A,
    credentials: &Credentials,
    window: Duration,
    mut cancelled: watch::Receiver<bool>,
    status: watch::Sender<Phase>,
) -> Result<Outcome> {
    if window.is_zero() || window > Duration::from_secs(u64::from(MAX_WINDOW_SECONDS)) {
        return Err(Error::Invalid);
    }
    let expires = Instant::now() + window;
    timeout(Duration::from_secs(5), gate.check())
        .await
        .map_err(|_| Error::Busy)??;
    // A lost caller or already cancelled request must not reach the firewall or NM.
    if *cancelled.borrow() || cancelled.has_changed().is_err() {
        return Err(Error::Denied);
    }
    status.send_replace(Phase::Preflight);
    timeout(Duration::from_secs(5), network.preflight())
        .await
        .map_err(|_| Error::Busy)??;
    scan_before_start(network, gate).await?;
    timeout(Duration::from_secs(5), gate.check())
        .await
        .map_err(|_| Error::Busy)??;
    if Instant::now() >= expires || *cancelled.borrow() || cancelled.has_changed().is_err() {
        return Err(Error::Denied);
    }
    if !timeout(Duration::from_secs(5), guard.install())
        .await
        .is_ok_and(|v| v.is_ok())
    {
        status.send_replace(Phase::Unknown);
        return Ok(Outcome::Unknown);
    }
    let work = async {
        gate.check().await?;
        guard.verify().await?;
        network
            .checkpoint(
                u32::try_from(window.as_secs())
                    .map_err(|_| Error::Invalid)?
                    .saturating_add(30),
            )
            .await?;
        gate.check().await?;
        guard.verify().await?;
        status.send_replace(Phase::Starting);
        network.activate(credentials).await?;
        let mut seen_ready = false;
        loop {
            gate.check().await?;
            guard.verify().await?;
            if network.ready(credentials).await? {
                status.send_replace(Phase::Ready);
                seen_ready = true;
            } else if seen_ready {
                return Err(Error::Changed);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        #[allow(unreachable_code)]
        Ok::<(), Error>(())
    };
    tokio::select! {
        biased;
        _=stop(&mut cancelled)=>{},
        _=tokio::time::sleep_until(expires)=>{},
        _=work=>{},
    }
    status.send_replace(Phase::Closing);
    // Never remove forwarding isolation until the exact candidate is gone and rollback is proven.
    let restored = timeout(Duration::from_secs(10), network.rollback(credentials))
        .await
        .is_ok_and(|v| v.is_ok());
    let released = restored
        && timeout(Duration::from_secs(5), guard.release())
            .await
            .is_ok_and(|v| v.is_ok());
    let outcome = if released {
        Outcome::RolledBack
    } else {
        Outcome::Unknown
    };
    status.send_replace(if released {
        Phase::Closed
    } else {
        Phase::Unknown
    });
    Ok(outcome)
}
#[cfg(test)]
mod tests;
