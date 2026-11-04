// SPDX-License-Identifier: AGPL-3.0-only
use super::{
    files::{Slot, Stamp},
    units::Unit,
    Result, WorkerState, ROLES,
};
use crate::{health::probe_process, Error};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Process {
    pub(super) index: usize,
    pub(super) pid: u32,
    start: u64,
    uid: u32,
    exe: Stamp,
    cgroups: String,
}
fn bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    let f = File::open(path).map_err(|_| Error::Io)?;
    let mut bytes = Vec::new();
    f.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() as u64 > max {
        return Err(Error::Invalid);
    }
    Ok(bytes)
}
pub(super) fn boot() -> Result<String> {
    parse_boot(&bounded(Path::new("/proc/sys/kernel/random/boot_id"), 37)?)
}
pub(super) fn parse_boot(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let uuid = text.strip_suffix('\n').ok_or(Error::Invalid)?;
    if uuid.len() != 36
        || !uuid.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
    {
        return Err(Error::Invalid);
    }
    Ok(uuid.into())
}

pub(super) fn collect(units: &[Unit], slot: &Slot) -> Result<(Vec<Process>, WorkerState)> {
    collect_using(units, slot, members, probe, || uid_processes(983))
}
pub(super) fn collect_using(
    units: &[Unit],
    slot: &Slot,
    mut read_members: impl FnMut(&Unit) -> Result<BTreeSet<u32>>,
    mut read_process: impl FnMut(usize, u32, u32, &Path, &str, &str) -> Result<Process>,
    read_workers: impl FnOnce() -> Result<BTreeSet<u32>>,
) -> Result<(Vec<Process>, WorkerState)> {
    if units.len() != 4 {
        return Err(Error::Invalid);
    }
    let mut processes = Vec::new();
    let mut pids = BTreeSet::new();
    for (index, unit) in units.iter().enumerate() {
        if unit.index != index {
            return Err(Error::Invalid);
        }
        let members = read_members(unit)?;
        if unit.active {
            if !pids.insert(unit.pid) || members != BTreeSet::from([unit.pid]) {
                return Err(Error::Integrity);
            }
            let (executable, digest) = slot.executable(ROLES[index].1)?;
            processes.push(read_process(
                index,
                unit.pid,
                ROLES[index].3,
                &executable,
                digest,
                &unit.cgroup,
            )?);
        } else if index != 3 || !members.is_empty() {
            return Err(Error::Integrity);
        }
    }
    // A stopped unit alone does not prove the worker's children/old generation
    // disappeared. Scan only bounded status/cgroup metadata, never cmdline,
    // environment, file descriptors, credentials or business data.
    let workers = read_workers()?;
    let expected = if units[3].active {
        BTreeSet::from([units[3].pid])
    } else {
        BTreeSet::new()
    };
    if workers != expected {
        return Err(Error::Integrity);
    }
    let state = if units[3].active {
        WorkerState::On
    } else if units[3].stopped {
        WorkerState::Off
    } else {
        WorkerState::Unknown
    };
    Ok((processes, state))
}

pub(super) fn probe(
    index: usize,
    pid: u32,
    uid: u32,
    executable: &Path,
    digest: &str,
    cgroup: &str,
) -> Result<Process> {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let exe = proc.join("exe");
    let before: Stamp = fs::metadata(&exe).map_err(|_| Error::Io)?.into();
    let signed: Stamp = fs::metadata(executable).map_err(|_| Error::Io)?.into();
    if before != signed {
        return Err(Error::Integrity);
    }
    let observation = probe_process(pid, ROLES.get(index).ok_or(Error::Invalid)?.1)?;
    if observation.uid != uid
        || !observation.no_new_privileges
        || observation.effective_capabilities != 0
        || observation.exe_sha256 != digest
        || observation.start_ticks == 0
    {
        return Err(Error::Denied);
    }
    // Also reject permitted/inheritable/ambient capability changes, not just Eff.
    let status = bounded(&proc.join("status"), 65536)?;
    check_security(&status, uid)?;
    let bytes = bounded(&proc.join("cgroup"), 8192)?;
    let cgroups = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid)?;
    check_cgroups(cgroups, cgroup)?;
    let again = probe_process(pid, ROLES[index].1)?;
    if again.start_ticks != observation.start_ticks
        || again.uid != observation.uid
        || again.exe_sha256 != observation.exe_sha256
        || !again.no_new_privileges
        || again.effective_capabilities != 0
        || Stamp::from(fs::metadata(&exe).map_err(|_| Error::Io)?) != before
        || Stamp::from(fs::metadata(executable).map_err(|_| Error::Io)?) != signed
        || bounded(&proc.join("cgroup"), 8192)? != bytes
    {
        return Err(Error::Integrity);
    }
    check_security(&bounded(&proc.join("status"), 65536)?, uid)?;
    Ok(Process {
        index,
        pid,
        uid,
        start: observation.start_ticks,
        exe: before,
        cgroups: cgroups.into(),
    })
}

pub(super) fn check_cgroups(text: &str, expected: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut controllers = BTreeSet::new();
    let mut matched = 0;
    for line in text.lines() {
        let parts = line.split(':').collect::<Vec<_>>();
        if parts.len() != 3
            || !ids.insert(parts[0])
            || parts[0].parse::<u32>().is_err()
            || !parts[2].starts_with('/')
            || parts[2].split('/').any(|s| s == ".." || s == ".")
        {
            return Err(Error::Invalid);
        }
        for name in parts[1].split(',') {
            if !controllers.insert(name) {
                return Err(Error::Invalid);
            }
        }
        if parts[2] != expected && parts[2] != "/" {
            return Err(Error::Integrity);
        }
        if parts[0] == "0" && parts[1].is_empty()
            || parts[1].split(',').any(|c| c == "name=systemd")
        {
            if parts[2] != expected {
                return Err(Error::Integrity);
            }
            matched += 1;
        }
    }
    // Reject hybrid/duplicate managers rather than guessing one membership.
    if matched != 1 {
        return Err(Error::Integrity);
    }
    Ok(())
}
fn members(unit: &Unit) -> Result<BTreeSet<u32>> {
    let base = if Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        Path::new("/sys/fs/cgroup")
    } else {
        Path::new("/sys/fs/cgroup/systemd")
    };
    read_members(&base.join(unit.cgroup.trim_start_matches('/')), unit.active)
}
pub(super) fn read_members(path: &Path, required: bool) -> Result<BTreeSet<u32>> {
    let mut stack = vec![path.to_path_buf()];
    let mut count = 0;
    let mut result = BTreeSet::new();
    while let Some(directory) = stack.pop() {
        let m = match fs::symlink_metadata(&directory) {
            Ok(m) => m,
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound && !required && directory == path =>
            {
                return Ok(result)
            }
            Err(_) => return Err(Error::Io),
        };
        if !m.is_dir() {
            return Err(Error::Denied);
        }
        count += 1;
        if count > 32 {
            return Err(Error::Invalid);
        }
        let data = bounded(&directory.join("cgroup.procs"), 4096)?;
        for line in std::str::from_utf8(&data)
            .map_err(|_| Error::Invalid)?
            .lines()
        {
            let pid = line.parse::<u32>().map_err(|_| Error::Invalid)?;
            if pid == 0 || !result.insert(pid) || result.len() > 128 {
                return Err(Error::Integrity);
            }
        }
        let mut entries = 0;
        for entry in fs::read_dir(&directory).map_err(|_| Error::Io)? {
            entries += 1;
            if entries > 256 {
                return Err(Error::Invalid);
            }
            let entry = entry.map_err(|_| Error::Io)?;
            let ty = entry.file_type().map_err(|_| Error::Io)?;
            if ty.is_symlink() {
                return Err(Error::Denied);
            }
            if ty.is_dir() {
                if stack.len() >= 32 {
                    return Err(Error::Invalid);
                }
                stack.push(entry.path());
            }
        }
    }
    Ok(result)
}
fn uid_processes(uid: u32) -> Result<BTreeSet<u32>> {
    let mut result = BTreeSet::new();
    let mut count = 0;
    for entry in fs::read_dir("/proc").map_err(|_| Error::Io)? {
        count += 1;
        if count > 8192 {
            return Err(Error::Invalid);
        }
        let entry = entry.map_err(|_| Error::Io)?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        // An unrelated disappearing PID may have been a worker. Incomplete
        // metadata fails closed; never retry silently within this observation.
        let bytes = bounded(&entry.path().join("status"), 65536)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid)?;
        let fields = text
            .lines()
            .filter_map(|s| s.strip_prefix("Uid:"))
            .collect::<Vec<_>>();
        if fields.len() != 1 {
            return Err(Error::Invalid);
        }
        let values = fields[0]
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| Error::Invalid)?;
        if values.len() != 4 {
            return Err(Error::Invalid);
        }
        if values.contains(&uid) {
            result.insert(pid);
        }
    }
    Ok(result)
}

pub(super) fn check_security(status: &[u8], uid: u32) -> Result<()> {
    let text = std::str::from_utf8(status).map_err(|_| Error::Invalid)?;
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if [
                "CapInh",
                "CapPrm",
                "CapEff",
                "CapAmb",
                "CapBnd",
                "NoNewPrivs",
                "Uid",
            ]
            .contains(&key)
            {
                if !seen.insert(key) {
                    return Err(Error::Invalid);
                }
                if key.starts_with("Cap")
                    && u64::from_str_radix(value.trim(), 16).map_err(|_| Error::Invalid)? != 0
                {
                    return Err(Error::Denied);
                }
                if key == "NoNewPrivs" && value.trim() != "1" {
                    return Err(Error::Denied);
                }
                if key == "Uid" {
                    let values = value.split_whitespace().collect::<Vec<_>>();
                    if values.len() != 4 || values.iter().any(|v| v.parse::<u32>() != Ok(uid)) {
                        return Err(Error::Denied);
                    }
                }
            }
        }
    }
    if seen.len() != 7 {
        return Err(Error::Invalid);
    }
    Ok(())
}

#[cfg(test)]
impl Process {
    pub(super) fn with_test_start(mut self, start: u64) -> Self {
        self.start = start;
        self
    }
    pub(super) fn synthetic(index: usize, pid: u32, executable: &Path) -> Self {
        Self {
            index,
            pid,
            uid: super::ROLES[index].3,
            start: 1,
            exe: fs::metadata(executable).unwrap().into(),
            cgroups: format!("0::/system.slice/{}\n", super::ROLES[index].0),
        }
    }
}
