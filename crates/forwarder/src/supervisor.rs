// SPDX-License-Identifier: AGPL-3.0-only
//! Finite Root actions. No management daemon, forwarding watcher or listener.
//! Only fixed production units/paths are exposed by the production CLI.
mod files;
mod network;
mod system;
use crate::{worker, worker_ipc as wire, Error, Result, Secret};
use files::*;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use system::System;
use tokio::time::sleep;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub core_socket: PathBuf,
    pub core_uid: u32,
    pub worker_uid: u32,
    pub worker_gid: u32,
    pub worker_group: u32,
    pub program: PathBuf,
    pub persistent: PathBuf,
    pub runtime: PathBuf,
    pub network_status: PathBuf,
    pub interface: String,
    pub ca_bundle: PathBuf,
    pub unit: String,
}
impl Context {
    #[cfg(feature = "dev-fixtures")]
    pub fn fixture(path: &Path) -> Result<Self> {
        Ok(serde_json::from_slice(&read(path, 8192)?)?)
    }
    pub fn production() -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Choice {
            interface: String,
        }
        let choice: Choice =
            serde_json::from_slice(&read(Path::new("/etc/rustsimhub/forwarder.json"), 1024)?)?;
        Ok(Self {
            core_socket: "/run/rustsimhub/core/core.sock".into(),
            core_uid: 986,
            worker_uid: 983,
            worker_gid: 983,
            worker_group: 987,
            program: "/opt/rustsimhub/selected/rustsimhub-forwarder".into(),
            persistent: "/var/lib/rustsimhub/rustsimhub-v1/system/forwarding".into(),
            runtime: "/run/rustsimhub-forwarding".into(),
            network_status: "/run/rustsimhub-network/status.json".into(),
            interface: choice.interface,
            ca_bundle: "/etc/ssl/certs/ca-certificates.crt".into(),
            unit: "rustsimhub-forwarder.service".into(),
        })
    }
    pub fn validate(&self) -> Result<()> {
        if rustix::process::geteuid().as_raw() != 0
            || rustix::process::getuid().as_raw() != 0
            || self.core_uid == 0
            || self.worker_uid == 0
            || self.core_uid == self.worker_uid
            || self.worker_gid == 0
            || self.worker_group == 0
            || [
                &self.core_socket,
                &self.program,
                &self.persistent,
                &self.runtime,
                &self.network_status,
                &self.ca_bundle,
            ]
            .iter()
            .any(|p| !p.is_absolute())
        {
            return Err(Error::Forbidden);
        }
        let production = self.unit == "rustsimhub-forwarder.service"
            && ["wlan0", "usb0"].contains(&self.interface.as_str());
        #[cfg(feature = "dev-fixtures")]
        let fixture = self
            .unit
            .strip_prefix("rustsimhub-forwarder-fixture-")
            .and_then(|s| s.strip_suffix(".service"))
            .is_some_and(crate::model::uuid)
            && self.interface == "lo";
        #[cfg(not(feature = "dev-fixtures"))]
        let fixture = false;
        if !production && !fixture {
            return Err(Error::Forbidden);
        }
        directory(&self.persistent)?;
        directory(&self.runtime)?;
        Ok(())
    }
    fn gate(&self) -> PathBuf {
        self.persistent.join("enabled.json")
    }
    fn proof(&self) -> PathBuf {
        self.runtime.join("worker.json")
    }
    async fn state(&self) -> Result<CoreState> {
        // A Type=exec Core may not have bound its socket when the boot unit
        // starts. Only this read/recovery query may repeat a transport loss;
        // Grant/Acquire/HTTP never inherit this exception.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        let v = loop {
            match self.request(|| wire::Command::State {}).await {
                Ok(value) => break value,
                Err(Error::Storage) if tokio::time::Instant::now() < deadline => {
                    sleep(Duration::from_millis(100)).await;
                }
                Err(error) => return Err(error),
            }
        };
        let state: CoreState = serde_json::from_value(v)?;
        if !hex(&state.core_instance)
            || state.enabled && (state.generation == 0 || state.binding_sha256.is_none())
            || state.lease_active != state.lease_sha256.is_some()
        {
            return Err(Error::Invalid);
        }
        Ok(state)
    }
    async fn retire(&self, digest: [u8; 32]) -> Result<()> {
        self.request(|| wire::Command::Retire {
            lease_sha256: digest,
        })
        .await?;
        Ok(())
    }
    async fn request(&self, command: impl Fn() -> wire::Command) -> Result<serde_json::Value> {
        for attempt in 0..4 {
            let result = wire::request(&self.core_socket, self.core_uid, command()).await;
            if !matches!(result, Err(Error::Busy)) || attempt == 3 {
                return result;
            }
            sleep(Duration::from_millis(50 << attempt)).await;
        }
        Err(Error::Busy)
    }
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
#[derive(Clone, Serialize, Deserialize)]
struct CoreState {
    enabled: bool,
    generation: u64,
    core_instance: String,
    binding_sha256: Option<[u8; 32]>,
    lease_active: bool,
    lease_sha256: Option<[u8; 32]>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Gate {
    format: u16,
    generation: u64,
    binding_sha256: [u8; 32],
}
impl Gate {
    fn from_state(state: &CoreState) -> Result<Self> {
        if !state.enabled {
            return Err(Error::Forbidden);
        }
        Ok(Self {
            format: 1,
            generation: state.generation,
            binding_sha256: state.binding_sha256.ok_or(Error::Forbidden)?,
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    format: u16,
    boot_id: String,
    core_instance: String,
    generation: u64,
    binding_sha256: [u8; 32],
    lease_sha256: [u8; 32],
    worker_pid: u32,
    worker_ticks: u64,
}
#[derive(Serialize)]
pub struct Report {
    format: u16,
    boot_id: String,
    core_instance: String,
    generation: u64,
    binding_sha256: Option<[u8; 32]>,
    sample_uptime_ms: u64,
    pub state: &'static str,
    pub process_state_verified: bool,
    worker_pid: Option<u32>,
}
fn boot() -> Result<String> {
    let s = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    if !crate::model::uuid(s.trim()) {
        return Err(Error::Invalid);
    }
    Ok(s.trim().into())
}
fn uptime() -> Result<u64> {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    Ok(u64::try_from(t.tv_sec)
        .map_err(|_| Error::Invalid)?
        .saturating_mul(1000)
        + u64::try_from(t.tv_nsec).map_err(|_| Error::Invalid)? / 1_000_000)
}
pub async fn action(context: Context, action: &str) -> Result<()> {
    context.validate()?;
    // ExecStopPost runs while a reconcile is waiting for the unit to stop.
    // Conditional retirement is atomic in Core and must not take that lock.
    if action == "retire" {
        if let Some(proof) = optional::<Proof>(&context.proof())? {
            context.retire(proof.lease_sha256).await?;
        }
        return Ok(());
    }
    let _lock = lock(&context.runtime.join("control.lock"))?;
    match action {
        "reconcile" => reconcile(&context).await,
        "status" => {
            let state = context.state().await?;
            let report = observe(&context, &state).await?;
            publish(&context.runtime, "status.json", &report, 0o640)?;
            Ok(())
        }
        _ => Err(Error::Invalid),
    }
}
async fn reconcile(c: &Context) -> Result<()> {
    let before = c.state().await?;
    let system = System::new(c);
    if !before.enabled {
        remove(&c.gate())?; // Durable before touching the service.
        system.stop().await?;
        remove(&c.runtime.join("settings.json"))?;
        if let Some(digest) = before.lease_sha256 {
            c.retire(digest).await?;
        }
        if let Some(proof) = optional::<Proof>(&c.proof())? {
            c.retire(proof.lease_sha256).await?;
        }
    } else {
        let gate = Gate::from_state(&before)?;
        let old = optional::<Gate>(&c.gate())?;
        // An exact repeated enable leaves a healthy current process alone.
        if old.as_ref() == Some(&gate) {
            if let Ok(report) = observe(c, &before).await {
                if report.state == "running" && report.process_state_verified {
                    publish(&c.runtime, "status.json", &report, 0o640)?;
                    return Ok(());
                }
            }
        }
        system.stop().await?;
        remove(&c.runtime.join("settings.json"))?;
        if let Some(digest) = before.lease_sha256 {
            c.retire(digest).await?;
        }
        if let Some(proof) = optional::<Proof>(&c.proof())? {
            c.retire(proof.lease_sha256).await?;
        }
        // Refuse a restart if an unknown process survived the stop.
        system.empty().await?;
        publish(&c.persistent, "enabled.json", &gate, 0o600)?;
        prepare_and_start(c, &before).await?;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    loop {
        let current = c.state().await?;
        if current.enabled != before.enabled
            || current.generation != before.generation
            || current.binding_sha256 != before.binding_sha256
            || current.core_instance != before.core_instance
        {
            return Err(Error::Forbidden);
        }
        if let Ok(report) = observe(c, &current).await {
            if report.process_state_verified {
                publish(&c.runtime, "status.json", &report, 0o640)?;
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Error::Forbidden);
        }
        sleep(Duration::from_millis(100)).await;
    }
}
async fn observe(c: &Context, state: &CoreState) -> Result<Report> {
    let mut report = Report {
        format: 1,
        boot_id: boot()?,
        core_instance: state.core_instance.clone(),
        generation: state.generation,
        binding_sha256: state.binding_sha256,
        sample_uptime_ms: uptime()?,
        state: if state.enabled {
            "awaiting_service"
        } else {
            "stop_unconfirmed"
        },
        process_state_verified: false,
        worker_pid: None,
    };
    if !state.enabled {
        if optional::<Gate>(&c.gate())?.is_none() && !state.lease_active {
            System::new(c).empty().await?;
            report.state = "disabled";
            report.process_state_verified = true;
        }
    } else if let Some(proof) = optional::<Proof>(&c.proof())? {
        if proof.format == 1
            && proof.boot_id == report.boot_id
            && proof.core_instance == state.core_instance
            && proof.generation == state.generation
            && Some(proof.binding_sha256) == state.binding_sha256
            && Some(proof.lease_sha256) == state.lease_sha256
            && optional::<Gate>(&c.gate())?.as_ref() == Some(&Gate::from_state(state)?)
        {
            System::new(c).running(&proof).await?;
            report.state = "running";
            report.process_state_verified = true;
            report.worker_pid = Some(proof.worker_pid);
        }
    }
    Ok(report)
}
async fn prepare_and_start(c: &Context, before: &CoreState) -> Result<()> {
    let route = network::route(c)?;
    system::program(&c.program)?;
    let grant = c.request(|| wire::Command::Grant {}).await?;
    let digest: [u8; 32] = serde_json::from_value(grant["lease_sha256"].clone())?;
    let result = async {
        if grant["generation"] != before.generation
            || grant["core_instance"] != before.core_instance
        {
            return Err(Error::Forbidden);
        }
        let settings = worker::Settings {
            core_socket: c.core_socket.clone(),
            core_uid: c.core_uid,
            lease: serde_json::from_value::<Secret>(grant["lease"].clone())?,
            route,
            ca_bundle: c.ca_bundle.clone(),
        };
        // Root-only volatile file. PID1 copies it to a private read-only
        // per-service credential; neither argv nor environment contains it.
        publish(&c.runtime, "settings.json", &settings, 0o600)?;
        let system = System::new(c);
        system.start().await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        let pid = loop {
            if let Ok(pid) = system.live_worker().await {
                break pid;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::Forbidden);
            }
            sleep(Duration::from_millis(50)).await;
        };
        let proof = Proof {
            format: 1,
            boot_id: boot()?,
            core_instance: before.core_instance.clone(),
            generation: before.generation,
            binding_sha256: before.binding_sha256.ok_or(Error::Forbidden)?,
            lease_sha256: digest,
            worker_pid: pid,
            worker_ticks: system::ticks(pid)?,
        };
        publish(&c.runtime, "worker.json", &proof, 0o640)?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = c.retire(digest).await;
        let _ = System::new(c).stop().await;
        let _ = remove(&c.runtime.join("settings.json"));
    }
    result
}
