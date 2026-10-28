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
        effective(&show()?, self.root_mode)?;
        self.revalidate()
    }
}

fn effective(output: &[u8], mode: Mode) -> Result<()> {
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
