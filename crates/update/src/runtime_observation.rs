// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed Linux sources, retained read guards, no business-health authority.
//!
//! An observation cannot be supplied as JSON or converted into a confirm token.
//! ```compile_fail
//! let _: rustsimhub_update::runtime_observation::RuntimeObservation =
//!     serde_json::from_str(r#"{"health_verified":true}"#).unwrap();
//! ```
use crate::{store::inspection::ReadOnlyState, Error};
use serde::Serialize;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};
pub mod confirmation;
mod files;
mod linux;
#[cfg(test)]
mod product_native;
pub(crate) mod units;
use files::Slot;
use linux::Process;
use units::Unit;

type Result<T> = std::result::Result<T, Error>;
const MAX_AGE: Duration = Duration::from_secs(5);
const ROLES: [(&str, &str, &str, u32); 4] = [
    (
        "rustsimhub-core.service",
        "rustsimhub-core",
        "rustsimhub-core",
        986,
    ),
    (
        "rustsimhub-edge.service",
        "rustsimhub-edge",
        "rustsimhub-edge",
        985,
    ),
    (
        "rustsimhub-media.service",
        "rustsimhub-audio-alsa",
        "rustsimhub-media",
        984,
    ),
    (
        "rustsimhub-forwarder.service",
        "rustsimhub-forwarder",
        "rustsimhub-forwarder",
        983,
    ),
];

/// Private live sources. No Clone, Deserialize, PID/path/time arguments or token.
pub struct RuntimeObservation {
    ledger: ReadOnlyState,
    slot: Slot,
    boot: String,
    units: Vec<Unit>,
    processes: Vec<Process>,
    forwarder: WorkerState,
    started: Instant,
    invalid: Cell<bool>,
    sources: Box<dyn Sources>,
}

/// Observed worker presence, never the Core's desired/authorized switch state.
#[derive(Serialize, PartialEq, Eq, Debug, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum WorkerState {
    Off,
    On,
    Unknown,
}

/// Deliberately excludes Linux boot UUIDs, PIDs, cgroup paths and configuration.
/// Detached metadata is not a live guard and always disclaims authority.
#[derive(Serialize)]
pub struct RuntimeReport {
    format: u16,
    scope: &'static str,
    read_only: bool,
    authorization_verified: bool,
    health_verified: bool,
    may_confirm: bool,
    application_integrity_verified: bool,
    runtime_selection_verified: bool,
    process_bindings_verified: bool,
    linux_boot_stable: bool,
    core_authorization_boot_verified: bool,
    business_ready_verified: bool,
    data_generation_verified: bool,
    selection: &'static str,
    candidate_present: bool,
    epoch: u64,
    manifest_sha256: String,
    source_sha256: String,
    processes: Vec<RoleReport>,
    forwarder_worker: WorkerState,
    forwarder_policy: &'static str,
    forwarder_policy_relation_verified: bool,
    max_age_ms: u64,
}
#[derive(Serialize)]
struct RoleReport {
    role: &'static str,
    signed_executable_bound: bool,
}

trait Sources {
    fn boot(&self) -> Result<String>;
    fn units(&self) -> Result<Vec<Unit>>;
    fn processes(&self, units: &[Unit], slot: &Slot) -> Result<(Vec<Process>, WorkerState)>;
}
struct LinuxSources;
impl Sources for LinuxSources {
    fn boot(&self) -> Result<String> {
        linux::boot()
    }
    fn units(&self) -> Result<Vec<Unit>> {
        units::observe()
    }
    fn processes(&self, units: &[Unit], slot: &Slot) -> Result<(Vec<Process>, WorkerState)> {
        linux::collect(units, slot)
    }
}
impl RuntimeObservation {
    /// Borrows immutable signed application claims, not a live runtime verdict.
    /// The running-candidate collector retains this object and fully validates
    /// it before connecting to Core and again before any ledger operation.
    pub(crate) fn signed_product_binding(&self) -> Result<(&crate::Manifest, &str, &[u8; 32])> {
        self.fresh()?;
        Ok((
            &self.slot.manifest,
            &self.slot.hash,
            self.ledger.runtime_key()?,
        ))
    }
    /// Root-only and entirely fixed-path. No Store::open, recovery or writes.
    pub fn observe() -> Result<Self> {
        let observation = Self::collect()?;
        observation.revalidate()?;
        Ok(observation)
    }
    // Private collection never escapes directly. RunningCandidate performs the
    // complete revalidation after its Root and signed metadata are gathered,
    // before taking the Core lease. Public observation validates on return.
    fn collect() -> Result<Self> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        let started = Instant::now();
        let boot = linux::boot()?;
        let ledger = ReadOnlyState::observe()?;
        let slot = Slot::observe(&ledger)?;
        let sources: Box<dyn Sources> = Box::new(LinuxSources);
        #[cfg(test)]
        let sources: Box<dyn Sources> = if crate::product_native_fixture::enabled() {
            Box::new(product_native::NativePid1)
        } else {
            sources
        };
        Self::sample(ledger, slot, boot, started, sources)
    }
    #[cfg(test)]
    fn capture(
        ledger: ReadOnlyState,
        slot: Slot,
        boot: String,
        started: Instant,
        sources: Box<dyn Sources>,
    ) -> Result<Self> {
        let observation = Self::sample(ledger, slot, boot, started, sources)?;
        observation.revalidate()?;
        Ok(observation)
    }
    fn sample(
        ledger: ReadOnlyState,
        slot: Slot,
        boot: String,
        started: Instant,
        sources: Box<dyn Sources>,
    ) -> Result<Self> {
        let units = sources.units()?;
        let (processes, forwarder) = sources.processes(&units, &slot)?;
        Ok(Self {
            ledger,
            slot,
            boot,
            units,
            processes,
            forwarder,
            started,
            invalid: Cell::new(false),
            sources,
        })
    }

    /// Any failed check permanently invalidates this object. Successful checks
    /// never move the original monotonic deadline, including repeated reports.
    pub fn revalidate(&self) -> Result<()> {
        self.validate_with(|| {
            if self.sources.boot()? != self.boot {
                return Err(Error::Integrity);
            }
            self.ledger.revalidate()?;
            self.slot.revalidate(&self.ledger)?;
            let units = self.sources.units()?;
            if units != self.units {
                return Err(Error::Integrity);
            }
            let (processes, forwarder) = self.sources.processes(&units, &self.slot)?;
            if processes != self.processes
                || forwarder != self.forwarder
                || self.sources.boot()? != self.boot
                || self.sources.units()? != self.units
            {
                return Err(Error::Integrity);
            }
            self.slot.revalidate(&self.ledger)?;
            self.ledger.revalidate()
        })
    }
    fn validate_with(&self, check: impl FnOnce() -> Result<()>) -> Result<()> {
        if self.invalid.get() {
            return Err(Error::Busy);
        }
        let result = (|| {
            self.fresh()?;
            check()?;
            self.fresh()
        })();
        if result.is_err() {
            self.invalid.set(true);
        }
        result
    }
    fn fresh(&self) -> Result<()> {
        match Instant::now().checked_duration_since(self.started) {
            Some(age) if age < MAX_AGE => Ok(()),
            _ => Err(Error::Busy),
        }
    }

    pub(crate) fn selected_version(&self, expected: &str, trial: bool) -> Result<bool> {
        self.revalidate()?;
        Ok(self.slot.attempted == trial
            && self.slot.candidate_present == trial
            && self.slot.hash == expected)
    }

    pub fn report(&self) -> Result<RuntimeReport> {
        self.revalidate()?;
        Ok(self.metadata())
    }
    fn metadata(&self) -> RuntimeReport {
        RuntimeReport {
            format: 1,
            scope: "linux-runtime-sources",
            read_only: true,
            authorization_verified: false,
            health_verified: false,
            may_confirm: false,
            application_integrity_verified: true,
            runtime_selection_verified: true,
            process_bindings_verified: true,
            linux_boot_stable: true,
            core_authorization_boot_verified: false,
            business_ready_verified: false,
            data_generation_verified: false,
            selection: if self.slot.attempted {
                "attempted_candidate"
            } else {
                "confirmed"
            },
            candidate_present: self.slot.candidate_present,
            epoch: self.slot.manifest.epoch,
            manifest_sha256: self.slot.hash.clone(),
            source_sha256: self.slot.manifest.source_sha256.clone(),
            processes: self
                .processes
                .iter()
                .map(|p| RoleReport {
                    role: ROLES[p.index].1,
                    signed_executable_bound: true,
                })
                .collect(),
            forwarder_worker: self.forwarder,
            forwarder_policy: "unknown",
            forwarder_policy_relation_verified: false,
            max_age_ms: MAX_AGE.as_millis() as u64,
        }
    }
}
#[cfg(test)]
mod tests;
