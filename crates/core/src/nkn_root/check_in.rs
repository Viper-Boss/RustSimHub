// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit original CheckInMachine adapter. No scheduler, default enablement,
//! permanent capability, second pending database or chain confirmation claim.
use super::*;
use crate::nkn_statistics::{Error as SignerError, SubscriptionSigner};
use rustsimhub_usage_stats::{
    check_in::{AttemptBudget, CheckInStore, CheckInTransport, StatisticsLease},
    protocol::CheckInIntent,
    query::TransportError,
    store::{FileStore, StoreError, StoredState},
    transaction::PreparedSubscription,
};

pub(super) fn signer_error(e: SignerError) -> PortError {
    match e {
        SignerError::Deadline => PortError::Deadline,
        SignerError::Busy => PortError::Unavailable,
        SignerError::Protocol | SignerError::Store => PortError::Protocol,
        SignerError::Denied => PortError::Denied,
    }
}
fn error(e: PortError) -> TransportError {
    match e {
        PortError::Deadline => TransportError::Timeout,
        PortError::Denied | PortError::Protocol => TransportError::Rejected,
        PortError::Offline => TransportError::Offline,
        PortError::Unavailable | PortError::Io => TransportError::Unavailable,
    }
}
pub(super) struct Startup {
    pub(super) signer: Arc<SubscriptionSigner>,
    pub(super) store: Arc<FileStore>,
    pub(super) budget: AttemptBudget,
}
impl Startup {
    pub(super) fn check(&self) -> Result<()> {
        self.signer
            .check_current(&self.store, &self.budget)
            .map_err(signer_error)?;
        Ok(())
    }
}

/// The Machine and adapter use the SAME locked FileStore and statistics identity.
/// This wrapper creates no additional state, identity or persistence format.
#[derive(Clone)]
pub struct SharedStatisticsStore(pub Arc<FileStore>);
impl CheckInStore for SharedStatisticsStore {
    fn public_key(&self) -> rustsimhub_usage_stats::protocol::StatsPublicKey {
        self.0.identity().public_key()
    }
    fn load(&self) -> std::result::Result<StoredState, StoreError> {
        self.0.load()
    }
    fn save(&self, state: &StoredState) -> std::result::Result<(), StoreError> {
        self.0.save(state)
    }
}

impl Controller {
    /// Read the installed, Core-bound, confirmation-capable policy. No Root
    /// launch or network connection is made during readiness checks.
    pub fn configured_statistics_key(
        &self,
    ) -> Result<rustsimhub_usage_stats::protocol::StatsPublicKey> {
        use std::os::unix::fs::FileTypeExt;
        let root_socket = std::path::Path::new("/run/rustsimhub-nkn-root/root.sock");
        if !root_socket
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_socket() && m.uid() == 0)
        {
            return Err(PortError::Unavailable);
        }
        let auth = self.auth.try_lock().map_err(|_| PortError::Unavailable)?;
        let details = policy_details(auth.public_key())?;
        if !details.confirms_check_ins {
            return Err(PortError::Unavailable);
        }
        details.transactions.ok_or(PortError::Unavailable)
    }

    // Only this original Machine lease can own the reporting consumer. Public
    // enable/disable Statistics independently owns the aggregate-query consumer.
    fn acquire_check_in(&self, scope: Arc<Startup>, lease: &StatisticsLease) -> Result<()> {
        scope.check()?;
        if self.closed.load(Ordering::Acquire) || !scope.signer.shares_auth(&self.auth) {
            return Err(PortError::Denied);
        }
        let mut state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        if state.as_ref().is_some_and(|r| {
            r.supervisor.snapshot().phase == rustsimhub_nkn_agent::Phase::Disabled
                && r.supervisor.snapshot().references == 0
        }) {
            *state = None;
        }
        if let Some(run) = state.as_mut() {
            if run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready
                || run.check_in.is_some()
                || run.source.shared.transaction_key != Some(scope.store.identity().public_key())
            {
                return Err(PortError::Unavailable);
            }
            run.source.shared.authority()?;
            if run.references[2].is_none() {
                let next = run
                    .statistics_epoch
                    .checked_add(1)
                    .ok_or(PortError::Denied)?;
                let reference = run
                    .supervisor
                    .acquire(rustsimhub_nkn_agent::Owner::Statistics)
                    .map_err(|_| PortError::Denied)?;
                run.statistics_epoch = next;
                run.references[2] = Some(reference);
            }
            run.check_in = Some(*lease.id());
            return Ok(());
        }
        // This startup belongs only to this attempt. A retained, already running
        // scope is never restarted or given a shorter authority by this path.
        let (root, engine, policy, proof) =
            authorized_ports(self.auth.clone(), scope.signer.owner_token())?;
        if proof.0.transaction_key != Some(scope.store.identity().public_key()) {
            proof.invalidate();
            return Err(PortError::Unavailable);
        }
        proof
            .0
            .host_closed
            .set(self.closed.clone())
            .map_err(|_| PortError::Denied)?;
        *proof.0.startup.lock().map_err(|_| PortError::Unavailable)? = Some(scope.clone());
        scope.check()?;
        let remaining = scope
            .budget
            .deadline()
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(10));
        let limits = rustsimhub_nkn_agent::Limits::bounded(
            remaining,
            Duration::from_secs(5),
            Duration::from_secs(5),
            Duration::from_secs(30),
            1,
        )
        .map_err(|_| PortError::Deadline)?;
        let source = Arc::new(proof.source());
        let mut supervisor =
            rustsimhub_nkn_agent::Supervisor::new(root, engine, rustsimhub_nkn_agent::SystemClock)
                .map_err(|_| PortError::Unavailable)?
                .with_limits(limits);
        let reference = supervisor
            .acquire(rustsimhub_nkn_agent::Owner::Statistics)
            .map_err(|_| PortError::Denied)?;
        let cancel = supervisor.arm().map_err(|_| PortError::Denied)?;
        *state = Some(Run {
            supervisor,
            source,
            references: [None, None, Some(reference)],
            remote: None,
            statistics_epoch: 1,
            statistics_query: false,
            check_in: Some(*lease.id()),
        });
        let run = state.as_mut().ok_or(PortError::Unavailable)?;
        let result = (|| {
            run.supervisor
                .begin(cancel, policy, proof)
                .map_err(|_| PortError::Protocol)?;
            loop {
                scope.check()?;
                let snapshot = run.supervisor.snapshot();
                match snapshot.phase {
                    rustsimhub_nkn_agent::Phase::Ready => return Ok(()),
                    rustsimhub_nkn_agent::Phase::Starting => {}
                    _ => return Err(PortError::Unavailable),
                }
                run.supervisor.drive().map_err(|_| PortError::Protocol)?;
                if run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        })();
        // Startup fence only governs this owned startup, never subsequent shared
        // engine work. Keep original Root authority/deadline unchanged.
        *run.source
            .shared
            .startup
            .lock()
            .map_err(|_| PortError::Unavailable)? = None;
        if result.is_err() {
            run.check_in = None;
            if let Some(reference) = run.references[2].take() {
                let _ = run.supervisor.release(reference);
            }
            run.statistics_epoch = run.statistics_epoch.saturating_add(1);
            // No fresh authority/launch. Original Core drive reconciles only the
            // retained scope after this purely local exact-reference release.
        }
        result
    }
    fn release_check_in(&self, id: &[u8; 16]) {
        // No Auth is required to discard a resource after logout/expiry. The
        // exact nonserializable Machine lease is the local ownership ticket.
        let mut state = self.run.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = state.as_mut() {
            if run.check_in.as_ref() != Some(id) {
                return;
            }
            run.check_in = None;
            if !run.statistics_query {
                if let Some(reference) = run.references[2].take() {
                    let _ = run.supervisor.release(reference);
                }
                run.statistics_epoch = run.statistics_epoch.saturating_add(1);
            }
        }
    }
    fn check_ticket(&self, id: &[u8; 16]) -> Result<()> {
        let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        let run = state.as_ref().ok_or(PortError::Unavailable)?;
        if self.closed.load(Ordering::Acquire)
            || run.check_in.as_ref() != Some(id)
            || run.references[2].is_none()
            || !run.source.shared.local_live()
            || run.supervisor.snapshot().phase != rustsimhub_nkn_agent::Phase::Ready
        {
            return Err(PortError::Denied);
        }
        run.source.shared.authority()
    }
}

struct Attempt {
    id: [u8; 16],
    origin: AttemptBudget,
    nonce: Option<u64>,
    prepared: Option<PreparedSubscription>,
}
/// Host-owned adapter, constructed explicitly with current real Owner consent.
/// Construction performs no startup, identity creation, preflight or submission.
pub struct StatisticsTransport {
    root: Arc<Controller>,
    signer: Arc<SubscriptionSigner>,
    store: Arc<FileStore>,
    attempt: Option<Attempt>,
}
impl StatisticsTransport {
    /// One explicitly authorized finite invocation. Keep the original Machine
    /// across prepare->persist->submit; then drop/release its exact reference.
    /// A later invocation reconstructs from FileStore and is query-only if
    /// Pending exists. No timer, automatic retry or new consent is created.
    pub fn run_once(
        self,
        window: rustsimhub_usage_stats::protocol::Window,
    ) -> std::result::Result<
        rustsimhub_usage_stats::check_in::Outcome,
        rustsimhub_usage_stats::check_in::Error,
    > {
        use rustsimhub_usage_stats::{
            check_in::{CheckInMachine, Outcome},
            protocol::{DisclosureAcknowledgement, EnabledPolicy, OwnerConsent, SystemClock},
        };
        let store = SharedStatisticsStore(self.store.clone());
        let mut machine = CheckInMachine::new(store, self, SystemClock);
        // This is only Machine's local policy flag. Real consent, current Auth
        // and original finite Root authority are independently checked at every
        // transport boundary and through dispatch/read by the signer/observer.
        let policy = EnabledPolicy::from_owner_consent(
            OwnerConsent::Granted,
            DisclosureAcknowledgement::Current,
        )
        .map_err(|_| rustsimhub_usage_stats::check_in::Error::Protocol)?;
        machine.set_policy(policy)?;
        let outcome = machine.advance(window)?;
        if matches!(outcome, Outcome::Prepared { .. }) {
            machine.advance(window)
        } else {
            Ok(outcome)
        }
    }

    pub fn new(
        root: Arc<Controller>,
        signer: SubscriptionSigner,
        store: Arc<FileStore>,
    ) -> Result<Self> {
        if !signer.shares_auth(&root.auth) {
            return Err(PortError::Denied);
        }
        Ok(Self {
            root,
            signer: Arc::new(signer),
            store,
            attempt: None,
        })
    }
    fn check(&self, lease: &StatisticsLease, budget: &AttemptBudget) -> Result<()> {
        self.signer
            .check_current(&self.store, budget)
            .map_err(signer_error)?;
        let attempt = self
            .attempt
            .as_ref()
            .filter(|a| &a.id == lease.id())
            .ok_or(PortError::Denied)?;
        self.root.check_ticket(&attempt.id)
    }
}
impl Drop for StatisticsTransport {
    fn drop(&mut self) {
        if let Some(a) = self.attempt.take() {
            self.root.release_check_in(&a.id);
        }
    }
}
impl CheckInTransport for StatisticsTransport {
    fn acquire_statistics(
        &mut self,
        lease: &StatisticsLease,
        budget: &AttemptBudget,
    ) -> std::result::Result<(), TransportError> {
        if self.attempt.is_some() {
            return Err(TransportError::Rejected);
        }
        let scope = Arc::new(Startup {
            signer: self.signer.clone(),
            store: self.store.clone(),
            budget: budget.clone(),
        });
        self.root.acquire_check_in(scope, lease).map_err(error)?;
        self.attempt = Some(Attempt {
            id: *lease.id(),
            origin: budget.clone(),
            nonce: None,
            prepared: None,
        });
        self.check(lease, budget).map_err(error)?;
        // Restart Pending is query-only: acquire must not request a fresh nonce.
        if self
            .store
            .load()
            .map_err(|_| TransportError::Rejected)?
            .windows
            .iter()
            .all(|w| w.pending.is_none())
        {
            let result = self
                .root
                .statistics_transaction(
                    &self.signer,
                    &self.store,
                    lease,
                    TransactionCall::Preflight,
                    budget,
                )
                .map_err(error)?;
            let TransactionConclusion::Nonce(nonce) = result else {
                return Err(TransportError::Rejected);
            };
            self.attempt.as_mut().ok_or(TransportError::Rejected)?.nonce = Some(nonce);
        }
        Ok(())
    }
    fn release_statistics(&mut self, lease: StatisticsLease) {
        if self.attempt.as_ref().is_some_and(|a| &a.id == lease.id()) {
            self.attempt = None;
            self.root.release_check_in(lease.id());
        }
    }
    fn prepare(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        budget: &AttemptBudget,
    ) -> std::result::Result<[u8; 32], TransportError> {
        self.check(lease, budget).map_err(error)?;
        let attempt = self.attempt.as_mut().ok_or(TransportError::Rejected)?;
        if attempt.prepared.is_some()
            || self
                .store
                .load()
                .map_err(|_| TransportError::Rejected)?
                .windows
                .iter()
                .any(|w| w.pending.is_some())
        {
            return Err(TransportError::Rejected);
        }
        let nonce = attempt.nonce.take().ok_or(TransportError::Rejected)?;
        // Pure local signer: NO Root/RPC call in this method.
        let prepared = self
            .signer
            .prepare(&self.store, *intent, nonce, &attempt.origin)
            .map_err(signer_error)
            .map_err(error)?;
        let digest = prepared.digest();
        attempt.prepared = Some(prepared);
        Ok(digest)
    }
    fn submit(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        budget: &AttemptBudget,
    ) -> std::result::Result<(), TransportError> {
        self.check(lease, budget).map_err(error)?;
        let attempt = self.attempt.as_mut().ok_or(TransportError::Rejected)?;
        // Consume BEFORE I/O; all errors/Unknown stay query-only. No second send.
        let prepared = attempt.prepared.take().ok_or(TransportError::Rejected)?;
        if prepared.intent() != *intent || prepared.digest() != digest {
            return Err(TransportError::Rejected);
        }
        let result = self
            .root
            .statistics_transaction(
                &self.signer,
                &self.store,
                lease,
                TransactionCall::Submit(&prepared),
                &attempt.origin,
            )
            .map_err(error)?;
        match result {
            TransactionConclusion::Accepted => Ok(()),
            _ => Err(TransportError::Unavailable),
        }
    }
    fn query(
        &mut self,
        lease: &StatisticsLease,
        intent: &CheckInIntent,
        digest: [u8; 32],
        budget: &AttemptBudget,
    ) -> std::result::Result<Vec<u8>, TransportError> {
        self.check(lease, budget).map_err(error)?;
        // Original Pending and exact intent/digest are checked inside the real
        // observer as well. No fresh nonce, prepare or submit after restart.
        let result = self
            .root
            .statistics_transaction(
                &self.signer,
                &self.store,
                lease,
                TransactionCall::Query {
                    intent: *intent,
                    digest,
                },
                budget,
            )
            .map_err(error)?;
        if result != TransactionConclusion::Confirmed {
            return Err(TransportError::Unavailable);
        }
        serde_json::to_vec(&serde_json::json!({
            "intent": intent,
            "digest": digest,
            "status": "confirmed",
        }))
        .map_err(|_| TransportError::Rejected)
    }
}

#[cfg(test)]
mod tests;
