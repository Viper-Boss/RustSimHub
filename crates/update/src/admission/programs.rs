// SPDX-License-Identifier: AGPL-3.0-only
//! Metadata from the exact signed executable bytes, never caller-supplied JSON.
use super::{MaintenanceSnapshot, ProgramMetadata, Result};
use crate::{manifest_hash, store::Store, verify, verify_payload, Error};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const LIMIT: usize = 16384;

/// Construction verifies the complete signed application group first. Fields
/// stay private so no caller can replace observed metadata after verification.
pub struct SignedPrograms {
    manifest_sha256: String,
    source_sha256: String,
    core: String,
    worker: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    source_archive_sha256: String,
    development_fixtures: bool,
    component: Option<String>,
    target_arch: Option<String>,
    worker: Option<bool>,
}

impl SignedPrograms {
    /// The public key must come from the maintenance caller's installed trust,
    /// not a package or browser. This verifies package provenance, not trust
    /// selection, effective systemd state, selected-slot identity or epoch floor.
    pub fn inspect(
        manifest: &[u8],
        signature: &[u8],
        public_key: &[u8; 32],
        payloads: &[Vec<u8>],
        schema: u32,
    ) -> Result<Self> {
        Self::inspect_with(manifest, signature, public_key, payloads, schema, |b| {
            capture(b, 986)
        })
    }

    fn inspect_with(
        manifest: &[u8],
        signature: &[u8],
        public_key: &[u8; 32],
        payloads: &[Vec<u8>],
        schema: u32,
        mut read_metadata: impl FnMut(&[u8]) -> Result<Vec<u8>>,
    ) -> Result<Self> {
        let verified = verify(manifest, signature, public_key, schema, 0)?;
        verify_payload(&verified, payloads)?;
        let mut observe = |index: usize, worker: bool| -> Result<String> {
            let bytes = read_metadata(&payloads[index])?;
            if bytes.is_empty() || bytes.len() > LIMIT {
                return Err(Error::Invalid);
            }
            let claims: Claims = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
            if claims.source_archive_sha256 != verified.source_sha256
                || claims.development_fixtures
                || claims
                    .target_arch
                    .as_deref()
                    .is_some_and(|a| a != "aarch64")
                || (worker && claims.worker != Some(true))
                || (!worker
                    && (claims.worker.is_some()
                        || claims.component.as_deref().is_some_and(|v| v != "core")))
            {
                return Err(Error::Integrity);
            }
            String::from_utf8(bytes).map_err(|_| Error::Invalid)
        };
        let core = observe(0, false)?;
        let worker = if verified.format == 2 {
            Some(observe(4, true)?)
        } else {
            None
        };
        Ok(Self {
            manifest_sha256: manifest_hash(manifest),
            source_sha256: verified.source_sha256,
            core,
            worker,
        })
    }

    fn metadata(&self) -> ProgramMetadata<'_> {
        ProgramMetadata {
            core: &self.core,
            worker: self.worker.as_deref(),
        }
    }
}

/// Borrows both the data fence and verified program identities. The report is
/// deliberately not a mutation capability; effective units and selection/floor
/// still have to be bound by the updater action before any slot mutation.
pub struct SignedVersionCheck<'a> {
    snapshot: &'a MaintenanceSnapshot,
    roles: [&'a SignedPrograms; 3],
    report: rustsimhub_core_launcher::UpgradeReport,
}

#[derive(Serialize)]
pub struct SignedVersionReport<'a> {
    format: u16,
    authorization_verified: bool,
    signed_programs_verified: bool,
    installed_trust_verified: bool,
    slot_identity_verified: bool,
    effective_systemd_verified: bool,
    manifest_sha256: [&'a str; 3],
    source_sha256: [&'a str; 3],
    comparison: &'a rustsimhub_core_launcher::UpgradeReport,
}

impl MaintenanceSnapshot {
    pub fn compare_signed<'a>(
        &'a self,
        selected: &'a SignedPrograms,
        candidate: &'a SignedPrograms,
        fallback: &'a SignedPrograms,
    ) -> Result<SignedVersionCheck<'a>> {
        let check = self.compare_versions(
            selected.metadata(),
            candidate.metadata(),
            fallback.metadata(),
        )?;
        Ok(SignedVersionCheck {
            snapshot: self,
            roles: [selected, candidate, fallback],
            report: check.report,
        })
    }
}

impl SignedVersionCheck<'_> {
    pub fn revalidate(&self) -> Result<()> {
        self.snapshot.revalidate()
    }
    pub fn exit_code(&self) -> i32 {
        self.report.exit_code()
    }
    pub fn report(&self) -> SignedVersionReport<'_> {
        SignedVersionReport {
            format: 1,
            authorization_verified: false,
            signed_programs_verified: true,
            installed_trust_verified: false,
            slot_identity_verified: false,
            effective_systemd_verified: false,
            manifest_sha256: self.roles.map(|p| p.manifest_sha256.as_str()),
            source_sha256: self.roles.map(|p| p.source_sha256.as_str()),
            comparison: &self.report,
        }
    }
}

/// Result of the opt-in maintenance admission. `exit_code` is the comparison
/// class (0 compatible and staged, 3 maintenance required, 4 rejected).
pub struct Admission {
    pub staged: Option<String>,
    pub exit_code: i32,
    pub report: serde_json::Value,
}

/// Stage only when the installed roles compare compatible. Roles and trust come
/// from the locked store, never from caller paths: with no candidate pending,
/// the confirmed group is both selected and fallback. The snapshot's data
/// fences (Core stopped) stay held until the store has published the candidate.
pub fn admit_stage(
    store: &mut Store,
    snapshot: &MaintenanceSnapshot,
    manifest: &[u8],
    signature: &[u8],
    payloads: &[Vec<u8>],
) -> Result<Admission> {
    let (key, schema) = store.trusted_key();
    let key = *key;
    admit_stage_with(
        store,
        snapshot,
        manifest,
        signature,
        payloads,
        |m, s, p| SignedPrograms::inspect(m, s, &key, p, schema),
        MaintenanceSnapshot::verify_effective_systemd,
    )
}

fn admit_stage_with(
    store: &mut Store,
    snapshot: &MaintenanceSnapshot,
    manifest: &[u8],
    signature: &[u8],
    payloads: &[Vec<u8>],
    mut inspect: impl FnMut(&[u8], &[u8], &[Vec<u8>]) -> Result<SignedPrograms>,
    effective: impl Fn(&MaintenanceSnapshot) -> Result<()>,
) -> Result<Admission> {
    effective(snapshot)?;
    let (baseline_manifest, baseline_signature, baseline_payloads) = store.admission_baseline()?;
    let baseline = inspect(&baseline_manifest, &baseline_signature, &baseline_payloads)?;
    let candidate = inspect(manifest, signature, payloads)?;
    let check = snapshot.compare_signed(&baseline, &candidate, &baseline)?;
    let exit_code = check.exit_code();
    let comparison = serde_json::to_value(check.report()).map_err(|_| Error::Invalid)?;
    let staged = if exit_code == 0 {
        // Re-read the loaded units and every fence immediately before mutation.
        effective(snapshot)?;
        check.revalidate()?;
        Some(store.stage(manifest, signature, payloads)?)
    } else {
        None
    };
    let report = serde_json::json!({"format":1,"authorization_verified":false,
        "installed_trust_verified":true,"slot_identity_verified":true,
        "effective_systemd_verified":true,"staged":staged.is_some(),
        "manifest_sha256":staged,"comparison":comparison});
    Ok(Admission {
        staged,
        exit_code,
        report,
    })
}

fn sealed(bytes: &[u8]) -> Result<File> {
    use rustix::fs::{fchmod, fcntl_add_seals, memfd_create, MemfdFlags, Mode, SealFlags};
    let fd = memfd_create(
        c"rustsimhub-build-info",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .map_err(|_| Error::Denied)?;
    let mut file = File::from(fd);
    file.write_all(bytes).map_err(|_| Error::Io)?;
    fchmod(&file, Mode::from_raw_mode(0o555)).map_err(|_| Error::Denied)?;
    fcntl_add_seals(
        &file,
        SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL,
    )
    .map_err(|_| Error::Denied)?;
    Ok(file)
}

pub(super) struct Reap(pub(super) Child);
impl Drop for Reap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn capture(bytes: &[u8], uid: u32) -> Result<Vec<u8>> {
    let change_identity =
        rustix::process::geteuid().as_raw() != uid || rustix::process::getegid().as_raw() != uid;
    if change_identity {
        use rustix::thread::{capabilities, CapabilitySet};
        let effective = capabilities(None).map_err(|_| Error::Denied)?.effective;
        // Killing a child after dropping it to another UID needs CAP_KILL.
        // Refuse BEFORE spawn when timeout cleanup cannot be guaranteed.
        if !effective.contains(CapabilitySet::KILL | CapabilitySet::SETUID | CapabilitySet::SETGID)
        {
            return Err(Error::Denied);
        }
    }
    rustix::thread::set_no_new_privs(true).map_err(|_| Error::Denied)?;
    // FD 0 is mapped through Stdio rather than making arbitrary descriptors
    // inheritable. The kernel executes the sealed bytes, not a pathname that
    // can be replaced between signature verification and process creation.
    let file = sealed(bytes)?;
    let mut command = Command::new("/proc/self/fd/0");
    command
        .arg0("rustsimhub-build-info")
        .arg("--build-info")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::from(file))
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if change_identity {
        command.uid(uid).gid(uid);
    }
    let mut child = Reap(command.spawn().map_err(|_| Error::Denied)?);
    let mut output = child.0.stdout.take().ok_or(Error::Io)?;
    let flags = rustix::fs::fcntl_getfl(&output).map_err(|_| Error::Io)?;
    rustix::fs::fcntl_setfl(&output, flags | rustix::fs::OFlags::NONBLOCK)
        .map_err(|_| Error::Io)?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut result = Vec::new();
    loop {
        let mut part = [0; 4096];
        let mut eof = false;
        loop {
            match output.read(&mut part) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    if result.len() + n > LIMIT {
                        return Err(Error::Invalid);
                    }
                    result.extend_from_slice(&part[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error::Io),
            }
        }
        if let Some(status) = child.0.try_wait().map_err(|_| Error::Io)? {
            if !status.success() {
                return Err(Error::Integrity);
            }
            if eof {
                return Ok(result);
            }
        }
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests;

/// Explicit format3 maintenance, separate from staging or normal startup.
/// Installed trust and the locked confirmed slot produce all role facts; no
/// caller-supplied runtime JSON or report is accepted as a migration proof.
#[cfg(feature = "email-production-candidate")]
pub fn migrate_email4(
    store: &mut Store,
    mut snapshot: MaintenanceSnapshot,
    manifest: &[u8],
    signature: &[u8],
    payloads: &[Vec<u8>],
) -> Result<()> {
    use rustsimhub_forwarder::{
        supervisor::{migrate_email4_blocking, Context},
        RuntimeCompatibility,
    };
    use sha2::Digest;
    if rustix::process::getuid().as_raw() != 0
        || rustix::process::geteuid().as_raw() != 0
        || snapshot.root_mode == rustsimhub_core_launcher::Mode::Device
        || snapshot
            .outbox
            .as_ref()
            .is_none_or(|d| d.version.application_id != 0x52534657 || d.version.format != 3)
    {
        return Err(Error::Denied);
    }
    snapshot.verify_effective_systemd()?;
    let binding = snapshot
        .outbox
        .as_ref()
        .ok_or(Error::Denied)?
        .email_binding()?;
    snapshot.business.check_email_binding(&binding)?;
    let (key, schema) = store.trusted_key();
    let key = *key;
    let (old_manifest, old_signature, old_payloads) = store.admission_baseline()?;
    let baseline =
        SignedPrograms::inspect(&old_manifest, &old_signature, &key, &old_payloads, schema)?;
    let candidate = SignedPrograms::inspect(manifest, signature, &key, payloads, schema)?;
    #[derive(Deserialize)]
    struct RuntimeEnvelope {
        forwarding_runtime_contract: RuntimeCompatibility,
    }
    let contract = |p: &SignedPrograms| -> Result<RuntimeCompatibility> {
        let core: RuntimeEnvelope = serde_json::from_str(&p.core).map_err(|_| Error::Invalid)?;
        #[derive(Deserialize)]
        struct WorkerEnvelope {
            worker: bool,
            protocol: u16,
            source_archive_sha256: String,
            development_fixtures: bool,
            forwarding_runtime_contract: RuntimeCompatibility,
        }
        let worker: WorkerEnvelope =
            serde_json::from_str(p.worker.as_deref().ok_or(Error::Denied)?)
                .map_err(|_| Error::Invalid)?;
        let expected = rustsimhub_forwarder::runtime_compatibility();
        if core.forwarding_runtime_contract != expected
            || worker.forwarding_runtime_contract != expected
            || !worker.worker
            || worker.protocol != 5
            || worker.development_fixtures
            || worker.source_archive_sha256 != p.source_sha256
        {
            return Err(Error::Denied);
        }
        Ok(core.forwarding_runtime_contract)
    };
    let programs = [
        contract(&baseline)?,
        contract(&candidate)?,
        contract(&baseline)?,
    ];
    // Schema/Root/start-mode and every role are compared as well. Data3 may
    // require maintenance; reject any unrelated rejection before changing it.
    if snapshot
        .compare_signed(&baseline, &candidate, &baseline)?
        .exit_code()
        == 4
    {
        return Err(Error::Denied);
    }
    let digest = sha2::Sha256::digest(old_payloads.get(4).ok_or(Error::Denied)?);
    let digest: [u8; 32] = digest.into();
    snapshot.verify_effective_systemd()?;
    snapshot.revalidate()?;
    // Relinquish ONLY outbox's read transaction/flock. The original business
    // writer fence and installed Root observations remain held across child.
    drop(snapshot.outbox.take());
    migrate_email4_blocking(
        Context::production().map_err(|_| Error::Denied)?,
        programs,
        digest,
    )
    .map_err(|_| Error::Denied)?;
    // Reacquire the actual outbox fence, checking the resulting version. Only
    // its expected directory journal churn is resampled, no other observation.
    let directory = std::path::Path::new("/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding");
    let db = super::Database::observe(directory, "outbox.sqlite3", 986, false, 32 * 1024 * 1024)?;
    if db.version.application_id != 0x52534657 || db.version.format != 4 {
        return Err(Error::Integrity);
    }
    snapshot.outbox = Some(db);
    for observed in &mut snapshot.observations {
        if observed.path == directory {
            *observed = super::Observation::capture(directory)?;
        }
    }
    snapshot.verify_effective_systemd()?;
    Ok(())
}
