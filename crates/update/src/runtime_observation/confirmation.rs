// SPDX-License-Identifier: AGPL-3.0-only
//! A consumed, live Root capability, never a serialized health token.
use super::*;
use crate::product_admission::SignedProductPrograms;
use crate::{
    admission::{running, SignedPrograms},
    store::inspection::confirmation::Plan,
};
use rustsimhub_wire::update_health as wire;
use std::os::unix::net::UnixStream;
mod io;
// The web updater journals fixed stage names and error kinds only.
fn observed<T>(web: bool, label: &str, result: Result<T>) -> Result<T> {
    if let (true, Err(error)) = (web || cfg!(test), &result) {
        eprintln!("running candidate {label}: {error:?}");
    }
    result
}

struct Core {
    stream: UnixStream,
    snapshot: wire::Snapshot,
    socket: (u64, u64),
    until: Instant,
    pid: u32,
}
impl Core {
    fn connect(runtime: &RuntimeObservation) -> Result<Self> {
        let socket = running::socket()?;
        let fd = rustix::net::socket_with(
            rustix::net::AddressFamily::UNIX,
            rustix::net::SocketType::STREAM,
            rustix::net::SocketFlags::CLOEXEC | rustix::net::SocketFlags::NONBLOCK,
            None,
        )
        .map_err(|_| Error::Io)?;
        let addr = rustix::net::SocketAddrUnix::new(wire::SOCKET).map_err(|_| Error::Invalid)?;
        rustix::net::connect(&fd, &addr).map_err(|error| match error {
            rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => Error::Denied,
            rustix::io::Errno::AGAIN | rustix::io::Errno::CONNREFUSED => Error::Busy,
            _ => Error::Io,
        })?;
        let stream = UnixStream::from(fd);
        stream.set_nonblocking(false).map_err(|_| Error::Io)?;
        let until = runtime.started + MAX_AGE;
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| Error::Io)?;
        let challenge = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let pid = runtime
            .processes
            .iter()
            .find(|p| p.index == 0)
            .ok_or(Error::Denied)?
            .pid;
        let cred = rustix::net::sockopt::socket_peercred(&stream).map_err(|_| Error::Denied)?;
        if cred.uid.as_raw() != 986 || cred.pid.as_raw_pid() as u32 != pid {
            return Err(Error::Denied);
        }
        let request = wire::Request {
            protocol: wire::VERSION,
            step: wire::Step::Observe,
            challenge: challenge.clone(),
            connection: None,
        };
        let mut stream = stream;
        io::write(
            &mut stream,
            &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
            until,
        )?;
        let snapshot = wire::parse_snapshot(&io::read(&mut stream, until, false)?)
            .map_err(|_| Error::Invalid)?;
        if snapshot.challenge != challenge
            || !snapshot.business_ready
            || snapshot.source_sha256.as_deref() != Some(&runtime.slot.manifest.source_sha256)
        {
            return Err(Error::Integrity);
        }
        let core = Self {
            stream,
            snapshot,
            socket,
            until,
            pid,
        };
        core.check(runtime)?;
        Ok(core)
    }
    fn check(&self, runtime: &RuntimeObservation) -> Result<()> {
        runtime.fresh()?;
        let cred =
            rustix::net::sockopt::socket_peercred(&self.stream).map_err(|_| Error::Denied)?;
        if cred.uid.as_raw() != 986
            || cred.pid.as_raw_pid() as u32 != self.pid
            || running::socket()? != self.socket
        {
            return Err(Error::Integrity);
        }
        // Detect pending EOF/unexpected extra data. A closed producer or a
        // replay queued behind a snapshot cannot survive the final gate.
        let mut byte = [0; 1];
        match rustix::net::recv(
            &self.stream,
            &mut byte,
            rustix::net::RecvFlags::PEEK | rustix::net::RecvFlags::DONTWAIT,
        ) {
            Err(rustix::io::Errno::AGAIN) => (),
            _ => return Err(Error::Integrity),
        }
        match (self.snapshot.forwarding.policy, runtime.forwarder) {
            (wire::Policy::Off, WorkerState::Off) | (wire::Policy::On, WorkerState::On) => Ok(()),
            _ => Err(Error::Integrity),
        }
    }
    fn step(&mut self, step: wire::Step) -> Result<Vec<u8>> {
        let request = wire::Request {
            protocol: wire::VERSION,
            step,
            challenge: self.snapshot.challenge.clone(),
            connection: Some(self.snapshot.connection.clone()),
        };
        io::write(
            &mut self.stream,
            &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
            self.until,
        )?;
        io::read(&mut self.stream, self.until, false)
    }
    fn recheck(&mut self) -> Result<()> {
        let bytes = self.step(wire::Step::Recheck)?;
        if wire::parse_snapshot(&bytes).map_err(|_| Error::Invalid)? != self.snapshot {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Released {
            protocol: u16,
            released: bool,
        }
        let ack: Released =
            serde_json::from_slice(&self.step(wire::Step::Finish)?).map_err(|_| Error::Invalid)?;
        if ack.protocol != wire::VERSION || !ack.released {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}
/// Owns the retained signed/process/cgroup/Root/data and same-connection guards.
/// No Clone, Deserialize, supplied PID/path/hash/time or detached re-entry.
pub struct RunningCandidate {
    runtime: RuntimeObservation,
    root: running::Root,
    data: running::Data,
    core: Core,
    plan: Plan,
    web: Option<crate::admission::WebHealthReceipt>,
    invalid: Cell<bool>,
}
#[derive(Serialize)]
pub struct Confirmation {
    format: u16,
    result: &'static str,
    manifest_sha256: String,
    epoch: u64,
    floor_epoch: u64,
}
enum Programs {
    Legacy(SignedPrograms, SignedPrograms),
    Product(Box<(SignedProductPrograms, SignedProductPrograms)>),
}
impl RunningCandidate {
    pub fn observe() -> Result<Self> {
        let candidate = Self::collect()?;
        candidate.revalidate()?;
        Ok(candidate)
    }
    fn collect() -> Result<Self> {
        let runtime = observed(false, "runtime sample", RuntimeObservation::collect())?;
        let plan = runtime.ledger.confirmation_plan()?;
        let root = observed(false, "Root", running::Root::observe_for(&runtime))?;
        // Exactly one metadata path, before taking the Core SQL/Auth lease.
        let programs = if root.product_health() {
            Programs::Product(Box::new((
                SignedProductPrograms::running_health(runtime.slot.verified())?,
                SignedProductPrograms::running_health(plan.verified_baseline())?,
            )))
        } else {
            Self::legacy_programs(&runtime.slot, &plan)?
        };
        Self::with_live_core(runtime, plan, root, programs, None)
    }

    fn collect_web(request: &str, manifest: &str) -> Result<Self> {
        if !rustsimhub_wire::UpdateUpload::valid_reference(request, manifest) {
            return Err(Error::Invalid);
        }
        let (runtime, (plan, root, programs, receipt)) = observed(
            true,
            "prepare web trial and sample runtime",
            RuntimeObservation::collect_after_preflight(|ledger, slot| {
                let plan = observed(true, "verify fallback", ledger.confirmation_plan())?;
                let receipt = crate::admission::web_health_receipt(request, manifest)?;
                if slot.hash != manifest
                    || slot.manifest.epoch != receipt.epoch
                    || crate::manifest_hash(&plan.verified_baseline().group().0)
                        != receipt.baseline_sha256
                    || receipt.business_schema != 6
                    || receipt.authorization_binding.is_none()
                {
                    return Err(Error::Denied);
                }
                let root = observed(true, "Root", running::Root::observe_web_ap())?;
                let programs = observed(
                    true,
                    "inspect signed metadata",
                    Self::legacy_programs(slot, &plan),
                )?;
                Ok((plan, root, programs, receipt))
            }),
        )?;
        Self::with_live_core(runtime, plan, root, programs, Some(receipt))
    }

    fn legacy_programs(slot: &Slot, plan: &Plan) -> Result<Programs> {
        Ok(Programs::Legacy(
            SignedPrograms::running_verified(slot.verified())?,
            SignedPrograms::running_verified(plan.verified_baseline())?,
        ))
    }

    fn with_live_core(
        runtime: RuntimeObservation,
        plan: Plan,
        root: running::Root,
        programs: Programs,
        web: Option<crate::admission::WebHealthReceipt>,
    ) -> Result<Self> {
        let web_trial = web.is_some();
        observed(web_trial, "runtime before Core", runtime.revalidate())?;
        let core = observed(web_trial, "Core connection", Core::connect(&runtime))?;
        let data = observed(
            web_trial,
            "data binding",
            running::Data::observe_for(&core.snapshot, &root),
        )?;
        match programs {
            Programs::Product(programs) => {
                let (candidate, baseline) = *programs;
                root.compare_product(&core.snapshot, &candidate, &baseline)?;
            }
            Programs::Legacy(candidate, baseline) => {
                if let Some(receipt) = &web {
                    web_matches_health(receipt, &core.snapshot)?;
                    running::compatible_web_ap(&core.snapshot, &candidate, &baseline)?;
                } else {
                    running::compatible(&core.snapshot, &candidate, &baseline)?;
                }
            }
        }
        Ok(Self {
            runtime,
            root,
            data,
            core,
            plan,
            web,
            invalid: Cell::new(false),
        })
    }
    pub fn revalidate(&self) -> Result<()> {
        if self.invalid.get() {
            return Err(Error::Busy);
        }
        let result = (|| {
            self.runtime.revalidate()?;
            self.plan.revalidate(&self.runtime.ledger)?;
            self.root.revalidate()?;
            self.data.revalidate()?;
            self.core.check(&self.runtime)?;
            web_binding_current(self.web.as_ref())?;
            self.runtime.fresh()
        })();
        if result.is_err() {
            self.invalid.set(true);
        }
        result
    }
    /// Consumed on success OR any failure. No health capability escapes as JSON.
    pub fn confirm(self) -> Result<Confirmation> {
        let web_trial = self.web.is_some();
        observed(web_trial, "before ledger lock", self.revalidate())?;
        let mut store = observed(
            web_trial,
            "acquire ledger lock",
            self.runtime.ledger.exclusive(&self.plan),
        )?;
        observed(web_trial, "after ledger lock", self.revalidate())?;
        let Self {
            runtime,
            root,
            data,
            mut core,
            plan,
            web,
            ..
        } = self;
        // Prepare/fsync first. The sole final Core Recheck is inside the gate,
        // after all preparatory IO, while the original SQL/Auth locks are held.
        let publication = store.confirm_live(&plan, root.scratch(), || {
            observed(web_trial, "final Core snapshot", core.recheck())?;
            observed(web_trial, "final runtime", runtime.revalidate())?;
            plan.revalidate(&runtime.ledger)?;
            root.revalidate()?;
            data.revalidate()?;
            core.check(&runtime)?;
            web_binding_current(web.as_ref())?;
            observed(web_trial, "final freshness", runtime.fresh())
        });
        observed(web_trial, "publish health confirmation", publication)?;
        // An error after intent publication is uncertain; query the original
        // ledger instead of silently retrying or claiming nothing was written.
        observed(web_trial, "finish Core connection", core.finish())?;
        Ok(Confirmation {
            format: 1,
            result: "confirmed",
            manifest_sha256: runtime.slot.hash.clone(),
            epoch: runtime.slot.manifest.epoch,
            floor_epoch: store.floor(),
        })
    }
}
/// The exact CLI is root-gated before this call; no writable recovery dispatch.
pub fn confirm_running_candidate() -> Result<Confirmation> {
    // confirm starts with the same complete revalidation that public observe
    // performs before returning. No caller-visible capability escapes here,
    // so do it once at the action boundary, not twice in adjacent calls.
    RunningCandidate::collect()?.confirm()
}
fn web_matches_health(
    receipt: &crate::admission::WebHealthReceipt,
    snapshot: &wire::Snapshot,
) -> Result<()> {
    if receipt.authorization_binding.as_deref() != Some(&snapshot.authorization_binding)
        || receipt.business_schema != snapshot.data_schema
    {
        return Err(Error::Denied);
    }
    Ok(())
}

fn web_binding_current(web: Option<&crate::admission::WebHealthReceipt>) -> Result<()> {
    if let Some(record) = web {
        if *record
            != crate::admission::web_health_receipt(&record.request_id, &record.manifest_sha256)?
        {
            return Err(Error::Busy);
        }
    }
    Ok(())
}

pub fn confirm_web_running_trial(request: &str, manifest: &str) -> Result<Confirmation> {
    RunningCandidate::collect_web(request, manifest)?.confirm()
}

#[cfg(test)]
mod tests;
