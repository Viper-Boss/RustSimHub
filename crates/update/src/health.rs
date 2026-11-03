// SPDX-License-Identifier: AGPL-3.0-only
//! C26 offline correlation candidate. Consistency is never confirm authority.
//! Root must supply a live, authenticated producer and retain admission guards
//! before wiring any result to Store::confirm. This module cannot call confirm.
use crate::{hash_hex, hex_digest, verify, verify_payload, Error, Manifest};
use serde::Serialize;

pub const MAX_AGE_MS: u64 = 5_000;

/// Constructed only after checking the actual signed manifest AND payload bytes.
/// No Deserialize implementation: callers cannot assert verification in JSON.
pub struct ExpectedCandidate {
    manifest: Manifest,
    manifest_hash: String,
}
impl ExpectedCandidate {
    pub fn verify(
        manifest: &[u8],
        signature: &[u8],
        key: &[u8; 32],
        schema: u32,
        floor: u64,
        payloads: &[Vec<u8>],
    ) -> Result<Self, Error> {
        let parsed = verify(manifest, signature, key, schema, floor)?;
        verify_payload(&parsed, payloads)?;
        Ok(Self {
            manifest: parsed,
            manifest_hash: hash_hex(manifest),
        })
    }
    pub fn manifest_hash(&self) -> &str {
        &self.manifest_hash
    }
    /// Compare observations to an exact verified candidate and caller's live
    /// expected boot/root/data fence. Even a perfect match is UNAUTHENTICATED.
    pub fn correlate(
        &self,
        fence: &ExpectedFence,
        observation: &UntrustedObservation,
        now_ms: u64,
    ) -> Result<Correlation, Rejection> {
        if !hex_digest(&fence.boot)
            || !hex_digest(&fence.root_sha256)
            || !hex_digest(&fence.challenge)
            || fence.data_generation == 0
        {
            return Err(Rejection::Fence);
        }
        if observation.boot != fence.boot || observation.challenge != fence.challenge {
            return Err(Rejection::BootOrChallenge);
        }
        if observation.manifest_sha256 != self.manifest_hash
            || observation.source_sha256 != self.manifest.source_sha256
            || observation.epoch != self.manifest.epoch
        {
            return Err(Rejection::Candidate);
        }
        if observation.at_ms > now_ms
            || observation.until_ms <= now_ms
            || observation.until_ms <= observation.at_ms
            || observation.until_ms - observation.at_ms > MAX_AGE_MS
            || now_ms - observation.at_ms >= MAX_AGE_MS
        {
            return Err(Rejection::Freshness);
        }
        if observation.root_sha256 != fence.root_sha256
            || observation.data_generation != fence.data_generation
            || !observation.data_compatible
            || observation.data_schema < self.manifest.schema_min
            || observation.data_schema > self.manifest.schema_max
        {
            return Err(Rejection::RootOrData);
        }
        let mut expected = vec![
            ("rustsimhub-core", 986),
            ("rustsimhub-edge", 985),
            ("rustsimhub-audio-alsa", 984),
        ];
        if self.manifest.format == 2 {
            expected.push(("rustsimhub-forwarder", 983));
        }
        if observation.processes.len() != expected.len() {
            return Err(Rejection::Processes);
        }
        let mut pids = std::collections::BTreeSet::new();
        for (name, uid) in expected {
            let matching: Vec<_> = observation
                .processes
                .iter()
                .filter(|p| p.role == name)
                .collect();
            if matching.len() != 1 {
                return Err(Rejection::Processes);
            }
            let p = matching[0];
            let signed = self
                .manifest
                .files
                .iter()
                .find(|e| e.name == name)
                .ok_or(Rejection::Processes)?;
            if p.pid == 0
                || !pids.insert(p.pid)
                || p.start_ticks == 0
                || p.uid != uid
                || p.exe_sha256 != signed.sha256
                || !p.no_new_privileges
                || p.effective_capabilities != 0
            {
                return Err(Rejection::Processes);
            }
        }
        Ok(Correlation {
            candidate_consistent: true,
            authorization_verified: false,
            may_confirm: false,
            missing_trusted_producer: true,
        })
    }
}

/// Live inputs must come from Root; serialization would not retain its fence.
pub struct ExpectedFence {
    pub boot: String,
    pub root_sha256: String,
    pub data_generation: u64,
    pub challenge: String,
}
/// Deliberately labelled untrusted, even when supplied by a healthy service.
/// No claimed "healthy" field, no receipt signature accepted on faith.
pub struct UntrustedObservation {
    pub manifest_sha256: String,
    pub source_sha256: String,
    pub epoch: u64,
    pub boot: String,
    pub challenge: String,
    pub at_ms: u64,
    pub until_ms: u64,
    pub root_sha256: String,
    pub data_generation: u64,
    pub data_schema: u32,
    pub data_compatible: bool,
    pub processes: Vec<ProcessObservation>,
}
pub struct ProcessObservation {
    pub role: String,
    pub pid: u32,
    pub start_ticks: u64,
    pub uid: u32,
    pub exe_sha256: String,
    pub no_new_privileges: bool,
    pub effective_capabilities: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Rejection {
    Fence,
    BootOrChallenge,
    Candidate,
    Freshness,
    RootOrData,
    Processes,
}
#[derive(Serialize)]
pub struct Correlation {
    pub candidate_consistent: bool,
    pub authorization_verified: bool,
    pub may_confirm: bool,
    pub missing_trusted_producer: bool,
}

/// Read actual Linux process bytes/status. This is an observation, not an
/// authenticated health receipt. Caller-selected PIDs do not confer authority.
#[cfg(target_os = "linux")]
pub fn probe_process(pid: u32, role: &str) -> Result<ProcessObservation, Error> {
    use std::{fs::File, io::Read, os::unix::fs::MetadataExt};
    fn bounded(path: &str, max: u64) -> Result<Vec<u8>, Error> {
        let file = File::open(path).map_err(|_| Error::Io)?;
        let mut data = Vec::new();
        file.take(max + 1)
            .read_to_end(&mut data)
            .map_err(|_| Error::Io)?;
        if data.len() as u64 > max {
            return Err(Error::Invalid);
        }
        Ok(data)
    }
    fn start(pid: u32) -> Result<u64, Error> {
        let data = bounded(&format!("/proc/{pid}/stat"), 8192)?;
        let text = std::str::from_utf8(&data).map_err(|_| Error::Invalid)?;
        let after = text.rsplit_once(')').ok_or(Error::Invalid)?.1;
        after
            .split_whitespace()
            .nth(19)
            .ok_or(Error::Invalid)?
            .parse()
            .map_err(|_| Error::Invalid)
    }
    if pid == 0
        || !matches!(
            role,
            "rustsimhub-core"
                | "rustsimhub-edge"
                | "rustsimhub-audio-alsa"
                | "rustsimhub-forwarder"
        )
    {
        return Err(Error::Invalid);
    }
    let before = start(pid)?;
    let executable = format!("/proc/{pid}/exe");
    let file = File::open(&executable).map_err(|_| Error::Io)?;
    let metadata = file.metadata().map_err(|_| Error::Io)?;
    if !metadata.is_file() || metadata.len() > crate::MAX_PAYLOAD {
        return Err(Error::Invalid);
    }
    let mut data = Vec::new();
    (&file)
        .take(crate::MAX_PAYLOAD + 1)
        .read_to_end(&mut data)
        .map_err(|_| Error::Io)?;
    if data.len() as u64 != metadata.len() {
        return Err(Error::Integrity);
    }
    let status_bytes = bounded(&format!("/proc/{pid}/status"), 65536)?;
    let status = std::str::from_utf8(&status_bytes).map_err(|_| Error::Invalid)?;
    let field = |name: &str| -> Result<&str, Error> {
        status
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .map(str::trim)
            .ok_or(Error::Invalid)
    };
    let uids: Vec<u32> = field("Uid:")?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| Error::Invalid)?;
    if uids.len() != 4 || uids.iter().any(|u| *u != uids[0]) {
        return Err(Error::Denied);
    }
    let nnp = field("NoNewPrivs:")? == "1";
    let caps = u64::from_str_radix(field("CapEff:")?, 16).map_err(|_| Error::Invalid)?;
    let after_file = std::fs::metadata(&executable).map_err(|_| Error::Io)?;
    let same = |a: &std::fs::Metadata, b: &std::fs::Metadata| {
        a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.len() == b.len()
            && a.mtime() == b.mtime()
            && a.mtime_nsec() == b.mtime_nsec()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    };
    if before != start(pid)?
        || !same(&metadata, &file.metadata().map_err(|_| Error::Io)?)
        || !same(&metadata, &after_file)
    {
        return Err(Error::Integrity);
    }
    Ok(ProcessObservation {
        role: role.into(),
        pid,
        start_ticks: before,
        uid: uids[0],
        exe_sha256: hash_hex(&data),
        no_new_privileges: nnp,
        effective_capabilities: caps,
    })
}

#[cfg(test)]
mod tests;
