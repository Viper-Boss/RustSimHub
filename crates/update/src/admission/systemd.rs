// SPDX-License-Identifier: AGPL-3.0-only
//! The loaded Core unit must be exactly the observed files: no pending reload
//! and no drop-in from a directory the disk snapshot does not cover.
use super::{programs::Reap, MaintenanceSnapshot, Result};
use crate::Error;
use rustsimhub_core_launcher::Mode;
use std::{
    collections::BTreeMap,
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const PROPERTIES: [&str; 4] = [
    "LoadState",
    "FragmentPath",
    "DropInPaths",
    "NeedDaemonReload",
];
const FRAGMENT: &str = "/etc/systemd/system/rustsimhub-core.service";
const DROP_INS: &str = "/etc/systemd/system/rustsimhub-core.service.d/";
const LIMIT: u64 = 16384;

impl MaintenanceSnapshot {
    pub fn verify_effective_systemd(&self) -> Result<()> {
        if self.product_qualified {
            return effective_product(self);
        }
        effective(&show()?, self.root_mode)?;
        self.revalidate()
    }
}

fn effective(output: &[u8], mode: Mode) -> Result<()> {
    effective_qualified(output, mode, false)
}
pub(super) fn effective_product(snapshot: &MaintenanceSnapshot) -> Result<()> {
    let present = std::fs::symlink_metadata(
        "/etc/systemd/system/rustsimhub-core.service.d/50-product-profile.conf",
    )
    .is_ok();
    effective_qualified(&show()?, snapshot.root_mode, present)?;
    snapshot.revalidate()
}
fn effective_qualified(output: &[u8], mode: Mode, product: bool) -> Result<()> {
    let text = std::str::from_utf8(output).map_err(|_| Error::Integrity)?;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or(Error::Integrity)?;
        if !PROPERTIES.contains(&key) || values.insert(key, value).is_some() {
            return Err(Error::Integrity);
        }
    }
    if values.len() != PROPERTIES.len() {
        return Err(Error::Integrity);
    }
    let expected: &[&str] = match mode {
        Mode::Device => &[],
        Mode::Forwarding => &["30-forwarding.conf"],
        Mode::SetupAp => &["30-forwarding.conf", "50-setup-ap.conf"],
    };
    let mut expected = expected.to_vec();
    if product {
        if std::fs::symlink_metadata(
            "/etc/systemd/system/rustsimhub-core.service.d/55-product-health.conf",
        )
        .is_ok()
        {
            expected.push("55-product-health.conf");
        }
        expected.push("50-product-profile.conf");
        expected.sort_unstable();
    }
    let mut drop_ins = values["DropInPaths"].split(' ').collect::<Vec<_>>();
    drop_ins.retain(|p| !p.is_empty());
    drop_ins.sort_unstable();
    if values["LoadState"] != "loaded"
        || values["FragmentPath"] != FRAGMENT
        || values["NeedDaemonReload"] != "no"
        || drop_ins.len() != expected.len()
        || drop_ins
            .iter()
            .zip(expected)
            .any(|(path, name)| path.strip_prefix(DROP_INS) != Some(name))
    {
        return Err(Error::Integrity);
    }
    Ok(())
}

/// Fixed program and arguments; no caller-supplied unit, path or property.
fn show() -> Result<Vec<u8>> {
    let mut command = Command::new("/usr/bin/systemctl");
    command
        .args(["show", "--all", "--no-pager"])
        .args(PROPERTIES.iter().flat_map(|p| ["-p", p]))
        .arg("rustsimhub-core.service")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = Reap(command.spawn().map_err(|_| Error::Denied)?);
    // A few hundred bytes never fill the pipe; a stuck manager times out.
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.0.try_wait().map_err(|_| Error::Io)? {
            break status;
        }
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        return Err(Error::Integrity);
    }
    let mut output = Vec::new();
    child
        .0
        .stdout
        .take()
        .ok_or(Error::Io)?
        .take(LIMIT + 1)
        .read_to_end(&mut output)
        .map_err(|_| Error::Io)?;
    if output.len() as u64 > LIMIT {
        return Err(Error::Integrity);
    }
    Ok(output)
}

// Runtime observation does not instantiate MaintenanceSnapshot or acquire its
// stopped-Core/data write guard. Only this fixed read-only process transport is
// reused; maintenance admission semantics above remain unchanged.
impl MaintenanceSnapshot {
    pub(crate) fn runtime_unit(index: usize) -> Result<Vec<u8>> {
        const UNITS: [&str; 4] = [
            "rustsimhub-core.service",
            "rustsimhub-edge.service",
            "rustsimhub-media.service",
            "rustsimhub-forwarder.service",
        ];
        let unit = UNITS.get(index).ok_or(Error::Invalid)?;
        let mut command = Command::new("/usr/bin/systemctl");
        command
            .args(["--system", "show", "--all", "--no-pager"])
            .args(
                crate::runtime_observation::units::PROPERTIES
                    .iter()
                    .flat_map(|p| ["-p", p]),
            )
            .arg(unit)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        runtime_capture(command, Duration::from_millis(250))
    }
    #[cfg(test)]
    pub(crate) fn runtime_test_capture(command: Command, budget: Duration) -> Result<Vec<u8>> {
        runtime_capture(command, budget)
    }
}

pub(super) fn web_trial_stopped() -> Result<()> {
    for index in 0..4 {
        let output = MaintenanceSnapshot::runtime_unit(index)?;
        let properties = std::str::from_utf8(&output).map_err(|_| Error::Invalid)?;
        let mut active_state = None;
        for line in properties.lines() {
            if let Some(state) = line.strip_prefix("ActiveState=") {
                if active_state.replace(state).is_some() {
                    return Err(Error::Integrity);
                }
            }
        }
        if !matches!(active_state, Some("inactive" | "failed"))
            || !properties.lines().any(|line| line == "MainPID=0")
            || !properties.lines().any(|line| line == "NeedDaemonReload=no")
        {
            return Err(Error::Busy);
        }
    }
    Ok(())
}

pub(super) fn runtime_capture(mut command: Command, budget: Duration) -> Result<Vec<u8>> {
    let mut child = Reap(command.spawn().map_err(|_| Error::Denied)?);
    let mut pipe = child.0.stdout.take().ok_or(Error::Io)?;
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(|_| Error::Io)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK).map_err(|_| Error::Io)?;
    let deadline = Instant::now() + budget;
    let mut output = Vec::new();
    let mut eof = false;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        let mut chunk = [0; 1024];
        match pipe.read(&mut chunk) {
            Ok(0) => eof = true,
            Ok(n) => {
                if output.len() + n > 4096 {
                    return Err(Error::Invalid);
                }
                output.extend_from_slice(&chunk[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(Error::Io),
        }
        if let Some(status) = child.0.try_wait().map_err(|_| Error::Io)? {
            if !status.success() {
                return Err(Error::Integrity);
            }
            if eof {
                return Ok(output);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Dedicated explicit candidate profile. Never widens maintenance semantics.
pub(super) fn running_unit() -> Result<Vec<u8>> {
    let mut command = Command::new("/usr/bin/systemctl");
    command
        .args(["--system", "show", "--all", "--no-pager"])
        .args(PROPERTIES.iter().flat_map(|p| ["-p", p]))
        .arg("rustsimhub-core.service")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    runtime_capture(command, Duration::from_millis(250))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show(fragment: &str, drop_ins: &str, reload: &str) -> Vec<u8> {
        format!("LoadState=loaded\nFragmentPath={fragment}\nDropInPaths={drop_ins}\nNeedDaemonReload={reload}\n").into_bytes()
    }
    const FW: &str = "/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf";
    const AP: &str = "/etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf";

    #[test]
    fn each_root_mode_accepts_only_its_exact_loaded_drop_ins() {
        let both = format!("{FW} {AP}");
        effective(&show(FRAGMENT, "", "no"), Mode::Device).unwrap();
        effective(&show(FRAGMENT, FW, "no"), Mode::Forwarding).unwrap();
        effective(&show(FRAGMENT, &both, "no"), Mode::SetupAp).unwrap();
        for (drop_ins, mode) in [
            (FW, Mode::Device),
            ("", Mode::Forwarding),
            (both.as_str(), Mode::Forwarding),
            (FW, Mode::SetupAp),
        ] {
            assert_eq!(
                effective(&show(FRAGMENT, drop_ins, "no"), mode),
                Err(Error::Integrity)
            );
        }
    }

    #[test]
    fn uncovered_drop_ins_pending_reload_and_foreign_fragments_are_refused() {
        for (fragment, drop_ins, reload) in [
            (FRAGMENT, "/etc/systemd/system/service.d/10-any.conf", "no"),
            (
                FRAGMENT,
                "/run/systemd/system/rustsimhub-core.service.d/x.conf",
                "no",
            ),
            (
                FRAGMENT,
                &format!("{FW} /etc/systemd/system/rustsimhub-.service.d/x.conf"),
                "no",
            ),
            (FRAGMENT, "", "yes"),
            ("/usr/lib/systemd/system/rustsimhub-core.service", "", "no"),
            ("", "", "no"),
        ] {
            assert_eq!(
                effective(&show(fragment, drop_ins, reload), Mode::Device)
                    .or_else(|_| effective(&show(fragment, drop_ins, reload), Mode::Forwarding)),
                Err(Error::Integrity)
            );
        }
    }

    #[test]
    fn missing_duplicate_unknown_or_unloaded_properties_are_refused() {
        let good = String::from_utf8(show(FRAGMENT, "", "no")).unwrap();
        for bad in [
            good.replace("LoadState=loaded", "LoadState=not-found"),
            good.replace("NeedDaemonReload=no\n", ""),
            format!("{good}LoadState=loaded\n"),
            format!("{good}ExecStart=/bin/sh\n"),
            good.replace("DropInPaths=", "DropInPaths"),
            String::new(),
        ] {
            assert_eq!(
                effective(bad.as_bytes(), Mode::Device),
                Err(Error::Integrity)
            );
        }
        assert_eq!(effective(&[0xff], Mode::Device), Err(Error::Integrity));
    }
}
