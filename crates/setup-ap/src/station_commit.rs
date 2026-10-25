// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit final STA handoff. Not a public command or an implicit consequence of a trial.
//! The caller must authenticate the current Owner, match a successful trial and durably
//! consume its exact confirmation before this transaction may mutate NetworkManager.
use crate::{
    station_trial::{StationCredentials, StationNetwork, TrialGuard},
    transaction::{Gate, Guard},
    Error, Result,
};
use std::{future::Future, time::Duration};
use tokio::{sync::watch, time::timeout};

pub trait CommitNetwork: StationNetwork {
    fn persist(
        &mut self,
        credentials: &StationCredentials,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Removes only this session's checkpoints/AP and proves the saved STA remains active.
    /// Gate checks precede checkpoint destruction. A partial/lost reply is Unknown,
    /// never an invitation to replay a save or guess whether rollback is still possible.
    fn finish<A: Gate + Sync>(
        &mut self,
        credentials: &StationCredentials,
        gate: &A,
    ) -> impl Future<Output = Result<()>> + Send;
}
pub trait CommitProvider {
    fn commit_session<'a>(
        &'a mut self,
        ap: &'a crate::Credentials,
    ) -> Result<impl CommitNetwork + Send + 'a>;
}
/// A real implementation must sync an immutable, scoped confirmation intent;
/// an always-allow stub is only a test fixture, not product authorization.
pub trait CommitAudit {
    fn consume(&self) -> Result<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitOutcome {
    Saved,
    RestoredAp,
    Unknown,
}
async fn stopped(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() || rx.changed().await.is_err() {
            return;
        }
    }
}
/// Caller retains the parent AP/Core network lock. No task is spawned, no cellular
/// operation or Internet probe is performed, and cancellation still awaits cleanup.
pub async fn run<N: CommitNetwork, G: TrialGuard + Guard, A: Gate + Sync>(
    network: &mut N,
    guard: &mut G,
    gate: &A,
    audit: &impl CommitAudit,
    credentials: &StationCredentials,
    window: Duration,
    mut stop: watch::Receiver<bool>,
) -> Result<CommitOutcome> {
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
    audit.consume()?;
    let prepared = {
        let work = async {
            gate.check().await?;
            guard.verify_trial().await?;
            network.begin(credentials).await?;
            loop {
                gate.check().await?;
                guard.verify_trial().await?;
                if network.usable(credentials).await? {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            gate.check().await?;
            network.persist(credentials).await?;
            gate.check().await?;
            guard.verify_trial().await?;
            Ok::<(), Error>(())
        };
        tokio::select! {
            biased;
            _=stopped(&mut stop)=>false,
            result=timeout(window,work)=>matches!(result,Ok(Ok(()))),
        }
    };
    if !prepared || *stop.borrow() || stop.has_changed().is_err() {
        return Ok(
            if timeout(Duration::from_secs(8), async {
                network.restore_ap(credentials).await?;
                guard.verify_trial().await
            })
            .await
            .is_ok_and(|v| v.is_ok())
            {
                CommitOutcome::RestoredAp
            } else {
                CommitOutcome::Unknown
            },
        );
    }
    // Once finalization starts, a lost reply may mean the last rollback timer
    // was destroyed. Do not blindly rollback/reactivate or release isolation.
    // Await the finite finalizer even if the caller disappears during cleanup.
    let finished = timeout(Duration::from_secs(8), network.finish(credentials, gate))
        .await
        .is_ok_and(|v| v.is_ok());
    if !finished {
        return Ok(CommitOutcome::Unknown);
    }
    Ok(
        if timeout(Duration::from_secs(5), guard.release())
            .await
            .is_ok_and(|v| v.is_ok())
        {
            CommitOutcome::Saved
        } else {
            CommitOutcome::Unknown
        },
    )
}

#[cfg(test)]
mod tests;
