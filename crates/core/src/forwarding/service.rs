// SPDX-License-Identifier: AGPL-3.0-only
//! Finite Owner-requested PID1 jobs; no shell, arbitrary units or background loop.
use super::*;
use futures_lite::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{io::Read, os::unix::fs::OpenOptionsExt, time::Duration};
use tokio::{sync::Mutex as AsyncMutex, time::timeout};
use zbus::zvariant::OwnedObjectPath;

const NAME: &str = "org.freedesktop.systemd1";
const PATH: &str = "/org/freedesktop/systemd1";
const MANAGER: &str = "org.freedesktop.systemd1.Manager";
const RECONCILE: &str = "rustsimhub-forwarder-reconcile.service";
const STATUS: &str = "rustsimhub-forwarder-status.service";
const REPORT: &str = "/run/rustsimhub-forwarding/status.json";
const BUDGET: Duration = Duration::from_secs(8);
#[derive(Clone, Copy)]
enum Completion {
    Done,
    Failed,
}

#[derive(Clone, Copy)]
pub(super) enum Action {
    Reconcile,
    Status,
}
impl Action {
    pub fn from_command(command: &AuthCommand) -> Option<Self> {
        match command {
            AuthCommand::SetForwardingEnabled { .. } | AuthCommand::ForwardingConfigure { .. } => {
                Some(Self::Reconcile)
            }
            AuthCommand::ForwardingStatus { .. } => Some(Self::Status),
            _ => None,
        }
    }
    fn token<'a>(&self, command: &'a AuthCommand) -> &'a str {
        match command {
            AuthCommand::SetForwardingEnabled { token, .. }
            | AuthCommand::ForwardingConfigure { token, .. }
            | AuthCommand::ForwardingStatus { token } => token,
            _ => unreachable!("internal action must match a forwarding command"),
        }
    }
}
pub(super) struct Coordinator {
    slot: AsyncMutex<()>,
    units: [String; 2],
    report: PathBuf,
    #[cfg(test)]
    test: Option<TestContext>,
}
#[cfg(test)]
struct TestContext {
    connection: zbus::Connection,
    report: PathBuf,
    uid: u32,
    budget: Duration,
}
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    binding: Binding,
    core: String,
    generation: u64,
    enabled: bool,
    configured: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    format: u16,
    boot_id: String,
    core_instance: String,
    generation: u64,
    binding_sha256: Option<[u8; 32]>,
    sample_uptime_ms: u64,
    state: String,
    process_state_verified: bool,
    worker_pid: Option<u32>,
}
impl Snapshot {
    fn new(auth: &mut Auth, state: &mut Management, token: &str) -> Result<(Self, Value)> {
        let binding = denied(Binding::from_owner(auth, token))?;
        let value = state.status(binding.clone())?;
        let snap = Self {
            binding,
            core: auth.observer_instance().to_owned(),
            generation: value["generation"].as_u64().ok_or(Error::Forbidden)?,
            enabled: value["enabled"].as_bool().ok_or(Error::Forbidden)?,
            configured: value["configured"].as_bool().ok_or(Error::Forbidden)?,
        };
        Ok((snap, value))
    }
    fn digest(&self) -> Result<Option<[u8; 32]>> {
        if !self.configured {
            return Ok(None);
        }
        Ok(Some(
            Sha256::digest(serde_json::to_vec(&self.binding).map_err(|_| Error::Forbidden)?).into(),
        ))
    }
}
impl Coordinator {
    pub fn production() -> Self {
        Self {
            slot: AsyncMutex::new(()),
            units: [RECONCILE.into(), STATUS.into()],
            report: REPORT.into(),
            #[cfg(test)]
            test: None,
        }
    }
    #[cfg(feature = "dev-fixtures")]
    pub(super) fn fixture(path: &Path) -> Result<Self> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Settings {
            reconcile: String,
            status: String,
            report: PathBuf,
        }
        let settings: Settings = read_owned(path, 0)?;
        let stem = settings
            .reconcile
            .strip_suffix("-reconcile.service")
            .ok_or(Error::Forbidden)?;
        let id = stem
            .strip_prefix("rustsimhub-forwarder-fixture-")
            .ok_or(Error::Forbidden)?;
        if id.len() != 36
            || !id.bytes().enumerate().all(|(i, c)| {
                if [8, 13, 18, 23].contains(&i) {
                    c == b'-'
                } else {
                    c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
                }
            })
            || settings.status != format!("{stem}-status.service")
            || settings
                .report
                .file_name()
                .is_none_or(|p| p != "status.json")
            || settings
                .report
                .parent()
                .and_then(Path::file_name)
                .is_none_or(|p| p != "runtime")
            || settings.report.parent().and_then(Path::parent) != path.parent()
            || path.parent().and_then(Path::parent) != Some(Path::new("/run"))
            || path
                .parent()
                .and_then(Path::file_name)
                .and_then(|p| p.to_str())
                .is_none_or(|p| !p.starts_with("rsh-fwd-root-"))
        {
            return Err(Error::Forbidden);
        }
        Ok(Self {
            slot: AsyncMutex::new(()),
            units: [settings.reconcile, settings.status],
            report: settings.report,
            #[cfg(test)]
            test: None,
        })
    }
    async fn connection(&self) -> Result<zbus::Connection> {
        #[cfg(test)]
        if let Some(test) = &self.test {
            return Ok(test.connection.clone());
        }
        // Explicit system endpoint: ignore DBUS_SYSTEM_BUS_ADDRESS/session env.
        zbus::connection::Builder::address("unix:path=/run/dbus/system_bus_socket")
            .map_err(|_| Error::Io)?
            .max_queued(32)
            .method_timeout(Duration::from_secs(2))
            .build()
            .await
            .map_err(|_| Error::Io)
    }
    pub async fn request(
        &self,
        controller: &Controller,
        auth: Arc<Mutex<Auth>>,
        command: AuthCommand,
        action: Action,
    ) -> Result<Value> {
        let token = action.token(&command).to_owned();
        let request_id = match &command {
            AuthCommand::SetForwardingEnabled { request_id, .. }
            | AuthCommand::ForwardingConfigure { request_id, .. } => Some(request_id.clone()),
            _ => None,
        };
        let _slot = self.slot.try_lock().map_err(|_| Error::Busy)?;
        let state = controller.0.clone();
        let a = auth.clone();
        let t = token.clone();
        let before = tokio::task::spawn_blocking(move || {
            let mut a = a.try_lock().map_err(|_| Error::Busy)?;
            let mut s = state.try_lock().map_err(|_| Error::Busy)?;
            // Actual Auth and semantic checks and durable intent all precede
            // even connecting to the system bus. Root IPC needs these locks.
            s.dispatch(&mut a, command)?;
            Snapshot::new(&mut a, &mut s, &t).map(|(snap, _)| snap)
        })
        .await
        .map_err(|_| Error::Forbidden)??;
        let start = uptime()?;
        let job = timeout(self.budget(), async {
            let connection = self.connection().await?;
            job(
                &connection,
                &self.units[match action {
                    Action::Reconcile => 0,
                    Action::Status => 1,
                }],
            )
            .await
        })
        .await;
        let observation = match job {
            Ok(Ok(Completion::Done)) => {
                let path = self.report_path().to_owned();
                let uid = self.report_uid();
                tokio::task::spawn_blocking(move || read_report(&path, uid))
                    .await
                    .ok()
                    .and_then(std::result::Result::ok)
            }
            _ => None,
        };
        let outcome = match job {
            Ok(Ok(Completion::Done)) => "completed",
            Ok(Ok(Completion::Failed)) => "failed",
            _ => "unconfirmed", // A lost StartUnit reply is never retried.
        };
        let state = controller.0.clone();
        tokio::task::spawn_blocking(move || {
            let mut a = auth.try_lock().map_err(|_| Error::Busy)?;
            let mut s = state.try_lock().map_err(|_| Error::Busy)?;
            let (after, mut value) = Snapshot::new(&mut a, &mut s, &token)?;
            if before != after {
                return Err(Error::Forbidden);
            }
            value["service_management_api"] = true.into();
            value["service_action"] = outcome.into();
            value["state"] = if after.enabled {
                "awaiting_service"
            } else {
                "stop_unconfirmed"
            }
            .into();
            if let Some(id) = request_id {
                value["request_id"] = id.into();
                value["configuration_applied"] = true.into();
            }
            if let Some(report) = observation {
                if verified(&after, &report, start, &boot()?, uptime()?)? {
                    value["state"] = report.state.into();
                    value["process_state_verified"] = true.into();
                    value["worker_pid"] = report.worker_pid.into();
                    value["delivery_api"] = after.enabled.into();
                }
            }
            Ok(value)
        })
        .await
        .map_err(|_| Error::Forbidden)?
    }
    fn report_path(&self) -> &Path {
        #[cfg(test)]
        if let Some(test) = &self.test {
            return &test.report;
        }
        &self.report
    }
    fn report_uid(&self) -> u32 {
        #[cfg(test)]
        if let Some(test) = &self.test {
            return test.uid;
        }
        0
    }
    fn budget(&self) -> Duration {
        #[cfg(test)]
        if let Some(test) = &self.test {
            return test.budget;
        }
        BUDGET
    }
}
async fn job(c: &zbus::Connection, unit: &str) -> Result<Completion> {
    let bus = zbus::fdo::DBusProxy::new(c).await.map_err(|_| Error::Io)?;
    let name = zbus::names::BusName::try_from(NAME).map_err(|_| Error::Io)?;
    let owner = bus
        .get_name_owner(name.clone())
        .await
        .map_err(|_| Error::Io)?;
    let manager = zbus::Proxy::new(c, owner.as_str(), PATH, MANAGER)
        .await
        .map_err(|_| Error::Io)?;
    // Subscribe before admission, including the signal stream, so very short
    // jobs cannot finish in a lost-signal gap. No interactive authorization.
    let mut signals = manager
        .receive_signal_with_args("JobRemoved", &[(2, unit)])
        .await
        .map_err(|_| Error::Io)?;
    manager
        .call::<_, _, ()>("Subscribe", &())
        .await
        .map_err(|_| Error::Io)?;
    let path: OwnedObjectPath = manager
        .call("StartUnit", &(unit, "fail"))
        .await
        .map_err(|_| Error::Io)?;
    let id = job_id(path.as_str()).ok_or(Error::Io)?;
    // Filtered and bounded. Ignore unrelated completions without accepting a
    // unit name or path supplied by the browser. Never retry StartUnit.
    for _ in 0..32 {
        let message = signals.next().await.ok_or(Error::Io)?;
        let (got_id, got_path, got_unit, result): (u32, OwnedObjectPath, String, String) =
            message.body().deserialize().map_err(|_| Error::Io)?;
        if got_path == path && got_id == id && got_unit == unit {
            if bus.get_name_owner(name).await.map_err(|_| Error::Io)? != owner {
                return Err(Error::Io);
            }
            return Ok(if result == "done" {
                Completion::Done
            } else {
                Completion::Failed
            });
        }
    }
    Err(Error::Io)
}
fn job_id(path: &str) -> Option<u32> {
    let id = path.strip_prefix("/org/freedesktop/systemd1/job/")?;
    let n = id.parse::<u32>().ok()?;
    (n > 0 && n.to_string() == id).then_some(n)
}
fn boot() -> Result<String> {
    let s = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| Error::Io)?;
    Ok(s.trim().to_owned())
}
fn uptime() -> Result<u64> {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    Ok(u64::try_from(t.tv_sec)
        .map_err(|_| Error::Io)?
        .saturating_mul(1000)
        + u64::try_from(t.tv_nsec).map_err(|_| Error::Io)? / 1_000_000)
}
fn read_report(path: &Path, uid: u32) -> Result<Report> {
    read_owned(path, uid)
}
fn read_owned<T: serde::de::DeserializeOwned>(path: &Path, uid: u32) -> Result<T> {
    let dir = fs::symlink_metadata(path.parent().ok_or(Error::Io)?).map_err(|_| Error::Io)?;
    if !path.is_absolute() || !dir.is_dir() || dir.uid() != uid || dir.mode() & 0o022 != 0 {
        return Err(Error::Forbidden);
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|_| Error::Io)?;
    let meta = file.metadata().map_err(|_| Error::Io)?;
    if !meta.is_file()
        || meta.uid() != uid
        || meta.mode() & 0o777 != 0o640
        || meta.nlink() != 1
        || meta.len() > 8192
    {
        return Err(Error::Forbidden);
    }
    let mut bytes = Vec::new();
    file.take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > 8192 {
        return Err(Error::Forbidden);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::Forbidden)
}
fn verified(s: &Snapshot, r: &Report, start: u64, boot: &str, now: u64) -> Result<bool> {
    Ok(r.format == 1
        && r.boot_id == boot
        && r.core_instance == s.core
        && r.generation == s.generation
        && r.binding_sha256 == s.digest()?
        && r.sample_uptime_ms >= start
        && r.sample_uptime_ms <= now
        && now.saturating_sub(r.sample_uptime_ms) <= 10_000
        && r.process_state_verified
        && if s.enabled {
            r.state == "running" && r.worker_pid.is_some_and(|p| p > 1)
        } else {
            r.state == "disabled" && r.worker_pid.is_none()
        })
}
#[cfg(test)]
mod tests;
