// SPDX-License-Identifier: AGPL-3.0-only
//! Test a station connection and return to the same protected AP before asking to save.
//! This module never saves a profile, releases isolation or enables cellular data.
use crate::{transaction::Gate, Error, Result};
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::timeout};
use zeroize::Zeroizing;

// No Debug/Serialize: station credentials are confined to the trusted local worker.
pub struct StationCredentials {
    pub(crate) ssid: String,
    pub(crate) passphrase: Zeroizing<String>,
    pub(crate) uuid: String,
}
impl StationCredentials {
    pub fn new(ssid: String, passphrase: Zeroizing<String>) -> Result<Self> {
        // Same WPA2-personal input contract as Core's existing provisioner.
        if ssid.is_empty()
            || ssid.len() > 32
            || ssid.chars().any(char::is_control)
            || !((8..=63).contains(&passphrase.len())
                && passphrase.bytes().all(|b| (0x20..=0x7e).contains(&b))
                || passphrase.len() == 64 && passphrase.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(Error::Invalid);
        }
        let generated = crate::Credentials::generate()?;
        Ok(Self {
            ssid,
            passphrase,
            uuid: generated.uuid().into(),
        })
    }
}
pub trait StationNetwork {
    fn begin(
        &mut self,
        credentials: &StationCredentials,
    ) -> impl Future<Output = Result<()>> + Send;
    fn usable(&self, credentials: &StationCredentials)
        -> impl Future<Output = Result<bool>> + Send;
    fn restore_ap(
        &mut self,
        credentials: &StationCredentials,
    ) -> impl Future<Output = Result<()>> + Send;
}
/// Must verify the fixed AP isolation including the DHCP-client return path.
/// The normal AP-only guard intentionally does not implement this capability successfully.
pub trait TrialGuard {
    fn verify_trial(&self) -> impl Future<Output = Result<()>> + Send;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrialOutcome {
    /// DHCP/local station connectivity worked and the exact AP was restored. Not an Internet test.
    UsableAndRestored,
    FailedAndRestored,
    Unknown,
}
async fn cancelled(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}
/// Caller owns the live AP transaction and its Core network lock throughout this operation.
/// Supply the actual Root lease/Owner gate and enough time before the original AP deadline.
/// A success is only a test result: a later explicit confirmation must precede persistence.
pub async fn run<N: StationNetwork, G: TrialGuard, A: Gate>(
    network: &mut N,
    guard: &G,
    gate: &A,
    credentials: &StationCredentials,
    window: Duration,
    mut stop: watch::Receiver<bool>,
) -> Result<TrialOutcome> {
    if window.is_zero() || window > Duration::from_secs(40) {
        return Err(Error::Invalid);
    }
    if *stop.borrow() || stop.has_changed().is_err() {
        return Err(Error::Denied);
    }
    timeout(Duration::from_secs(3), gate.check())
        .await
        .map_err(|_| Error::Busy)??;
    timeout(Duration::from_secs(4), guard.verify_trial())
        .await
        .map_err(|_| Error::Busy)??;
    // After begin is entered even an error/timeout may have applied a mutation.
    let usable = {
        let work = async {
            gate.check().await?;
            guard.verify_trial().await?;
            network.begin(credentials).await?;
            loop {
                gate.check().await?;
                guard.verify_trial().await?;
                if network.usable(credentials).await? {
                    gate.check().await?;
                    return Ok::<(), Error>(());
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        };
        tokio::select! {
            biased;
            _ = cancelled(&mut stop) => false,
            result = timeout(window, work) => matches!(result, Ok(Ok(()))),
        }
    };
    // Cleanup remains awaited after cancellation; loss of user authority never stops exact cleanup.
    let restored = timeout(Duration::from_secs(8), async {
        network.restore_ap(credentials).await?;
        guard.verify_trial().await
    })
    .await
    .is_ok_and(|r| r.is_ok());
    let confirmed = usable
        && restored
        && !*stop.borrow()
        && stop.has_changed().is_ok()
        && timeout(Duration::from_secs(3), gate.check())
            .await
            .is_ok_and(|r| r.is_ok());
    Ok(if !restored {
        TrialOutcome::Unknown
    } else if confirmed {
        TrialOutcome::UsableAndRestored
    } else {
        TrialOutcome::FailedAndRestored
    })
}

#[cfg(test)]
mod tests;
