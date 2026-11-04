// SPDX-License-Identifier: AGPL-3.0-only
//! A consumed, live Root capability, never a serialized health token.
use super::*;
use crate::{
    admission::{running, SignedPrograms},
    store::inspection::confirmation::Plan,
};
use rustsimhub_wire::update_health as wire;
use std::os::unix::net::UnixStream;
mod io;

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
        rustix::net::connect(&fd, &addr).map_err(|_| Error::Busy)?;
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
impl RunningCandidate {
    pub fn observe() -> Result<Self> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        let runtime = RuntimeObservation::observe()?;
        let root = running::Root::observe_for(&runtime)?;
        let plan = runtime.ledger.confirmation_plan()?;
        let candidate = SignedPrograms::running_inspect(
            &runtime.slot.signed()?,
            runtime.ledger.runtime_key()?,
        )?;
        let baseline =
            SignedPrograms::running_inspect(plan.baseline(), runtime.ledger.runtime_key()?)?;
        runtime.revalidate()?;
        let core = Core::connect(&runtime)?;
        let data = running::Data::observe_for(&core.snapshot, &root)?;
        if root.product_health() {
            let candidate_mode = crate::product_admission::SignedProductPrograms::running_health(
                &runtime.slot.signed()?,
                runtime.ledger.runtime_key()?,
            )?;
            let fallback_mode = crate::product_admission::SignedProductPrograms::running_health(
                plan.baseline(),
                runtime.ledger.runtime_key()?,
            )?;
            root.compatible_product(&core.snapshot, &candidate_mode, &fallback_mode)?;
        } else {
            running::compatible(&core.snapshot, &candidate, &baseline)?;
        }
        let result = Self {
            runtime,
            root,
            data,
            core,
            plan,
            invalid: Cell::new(false),
        };
        result.revalidate()?;
        Ok(result)
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
            self.runtime.fresh()
        })();
        if result.is_err() {
            self.invalid.set(true);
        }
        result
    }
    /// Consumed on success OR any failure. No health capability escapes as JSON.
    pub fn confirm(self) -> Result<Confirmation> {
        self.revalidate()?;
        let mut store = self.runtime.ledger.exclusive(&self.plan)?;
        self.revalidate()?;
        let Self {
            runtime,
            root,
            data,
            mut core,
            plan,
            ..
        } = self;
        // Prepare/fsync first. The sole final Core Recheck is inside the gate,
        // after all preparatory IO, while the original SQL/Auth locks are held.
        store.confirm_live(&plan, root.scratch(), || {
            core.recheck()?;
            runtime.revalidate()?;
            plan.revalidate(&runtime.ledger)?;
            root.revalidate()?;
            data.revalidate()?;
            core.check(&runtime)?;
            runtime.fresh()
        })?;
        // An error after intent publication is uncertain; query the original
        // ledger instead of silently retrying or claiming nothing was written.
        core.finish()?;
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
    RunningCandidate::observe()?.confirm()
}
#[cfg(test)]
mod tests;
