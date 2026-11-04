// SPDX-License-Identifier: AGPL-3.0-only
//! Same-byte signed program probes and a borrowed live maintenance observation.
//! No report, caller stdout JSON or metadata number is migration authority.
use crate::{
    admission::{MaintenanceSnapshot, SignedPrograms},
    manifest_hash, verify, verify_payload, Error,
};
use serde::Serialize;
use std::{
    fs::File,
    io::{Read, Write},
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProductRuntimeModeV1 {
    #[serde(rename = "legacy-v1")]
    Legacy,
    #[serde(rename = "product-v1")]
    Product,
    #[serde(rename = "product-email-v1")]
    Email,
    #[serde(rename = "product-recovery-v1")]
    Recovery,
    #[serde(rename = "product-email-recovery-v1")]
    EmailRecovery,
    #[serde(rename = "product-recovery-health-v1")]
    RecoveryHealth,
}
impl ProductRuntimeModeV1 {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy-v1",
            Self::Product => "product-v1",
            Self::Email => "product-email-v1",
            Self::Recovery => "product-recovery-v1",
            Self::EmailRecovery => "product-email-recovery-v1",
            Self::RecoveryHealth => "product-recovery-health-v1",
        }
    }
}
enum OriginalVerification {
    Checked {
        _original: SignedPrograms,
    },
    #[cfg(test)]
    SyntheticTransport,
}
/// No Deserialize/Clone. Metadata was read from sealed, signature-checked bytes.
pub struct SignedProductPrograms {
    _base: OriginalVerification,
    manifest_sha256: String,
    source_sha256: String,
    core_sha256: String,
    worker_sha256: String,
    mode: ProductRuntimeModeV1,
    core: String,
    worker: String,
    _manifest: Vec<u8>,
    _signature: Vec<u8>,
    _payloads: Vec<Vec<u8>>,
}
struct SignedInput<'a> {
    manifest: &'a [u8],
    signature: &'a [u8],
    key: &'a [u8; 32],
    payloads: &'a [Vec<u8>],
}
impl SignedProductPrograms {
    pub fn inspect(
        manifest: &[u8],
        signature: &[u8],
        trusted_key: &[u8; 32],
        payloads: &[Vec<u8>],
        schema: u32,
        mode: ProductRuntimeModeV1,
    ) -> Result<Self, Error> {
        // Retain the original SignedPrograms verification/provenance path.
        let base = SignedPrograms::inspect(manifest, signature, trusted_key, payloads, schema)?;
        Self::finish(
            OriginalVerification::Checked { _original: base },
            SignedInput {
                manifest,
                signature,
                key: trusted_key,
                payloads,
            },
            schema,
            mode,
            |bytes, args| capture(bytes, args, 986),
        )
    }
    pub(crate) fn running_health(
        group: &crate::store::SignedGroup,
        key: &[u8; 32],
    ) -> Result<Self, Error> {
        let base = SignedPrograms::running_inspect(group, key)?;
        base.supports_running_health()?;
        Self::finish(
            OriginalVerification::Checked { _original: base },
            SignedInput {
                manifest: &group.0,
                signature: &group.1,
                key,
                payloads: &group.2,
            },
            9,
            ProductRuntimeModeV1::RecoveryHealth,
            |bytes, args| capture_for(bytes, args, 986, Duration::from_millis(250)),
        )
    }
    fn finish(
        base: OriginalVerification,
        input: SignedInput<'_>,
        schema: u32,
        mode: ProductRuntimeModeV1,
        mut probe: impl FnMut(&[u8], &[&str]) -> Result<Vec<u8>, Error>,
    ) -> Result<Self, Error> {
        let SignedInput {
            manifest,
            signature,
            key,
            payloads,
        } = input;
        let parsed = verify(manifest, signature, key, schema, 0)?;
        verify_payload(&parsed, payloads)?;
        if parsed.format != 2 {
            return Err(Error::Denied);
        }
        let core = probe(
            &payloads[0],
            &["--product-runtime-compatibility", mode.as_str()],
        )?;
        let worker = probe(&payloads[4], &["--build-info"])?;
        if core.len() > 16384 || worker.len() > 16384 {
            return Err(Error::Invalid);
        }
        let value = Self {
            _base: base,
            manifest_sha256: manifest_hash(manifest),
            source_sha256: parsed.source_sha256,
            core_sha256: manifest_hash(&payloads[0]),
            worker_sha256: manifest_hash(&payloads[4]),
            mode,
            core: String::from_utf8(core).map_err(|_| Error::Invalid)?,
            worker: String::from_utf8(worker).map_err(|_| Error::Invalid)?,
            _manifest: manifest.into(),
            _signature: signature.into(),
            _payloads: payloads.into(),
        };
        // Use the strict explicit protocol parser, then separately pin sources.
        let core: serde_json::Value =
            serde_json::from_str(&value.core).map_err(|_| Error::Invalid)?;
        let worker: serde_json::Value =
            serde_json::from_str(&value.worker).map_err(|_| Error::Invalid)?;
        let write_schema = core["business"]["write"].as_u64().ok_or(Error::Invalid)?;
        if write_schema < u64::from(parsed.schema_min)
            || write_schema > u64::from(parsed.schema_max)
        {
            return Err(Error::Schema);
        }
        if core["source_archive_sha256"] != value.source_sha256
            || worker["source_archive_sha256"] != value.source_sha256
            || core["target_arch"] != target_arch()
        {
            return Err(Error::Integrity);
        }
        let request = serde_json::json!({"format":2,"target_arch":target_arch(),"root_mode":"setup_ap",
            "root_contract":rustsimhub_core_launcher::installation_contract().to_string(),
            "data":{"business_schema":core["business"]["write"],"outbox":null},
            "selected":value.role(),"candidate":value.role(),"fallback":value.role()});
        if rustsimhub_core_launcher::check_upgrade(
            &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
        )
        .map_err(|_| Error::Invalid)?
        .exit_code()
            != 0
        {
            return Err(Error::Denied);
        }
        Ok(value)
    }
    pub(crate) fn role(&self) -> serde_json::Value {
        serde_json::json!({"mode":self.mode,
        "core":self.core,"worker":self.worker,"core_sha256":self.core_sha256,"worker_sha256":self.worker_sha256})
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn mode(&self) -> ProductRuntimeModeV1 {
        self.mode
    }
}

/// Keeps the REAL caller's maintenance observation alive. This borrows it and
/// never opens a second business writer or reconstructs a guard from JSON.
pub struct ProductDataAdmission<'a> {
    snapshot: &'a MaintenanceSnapshot,
    roles: [&'a SignedProductPrograms; 3],
    comparison: rustsimhub_core_launcher::UpgradeReport,
}
impl<'a> ProductDataAdmission<'a> {
    pub fn compare(
        snapshot: &'a MaintenanceSnapshot,
        selected: &'a SignedProductPrograms,
        candidate: &'a SignedProductPrograms,
        fallback: &'a SignedProductPrograms,
    ) -> Result<Self, Error> {
        snapshot.revalidate()?;
        // This value is generated by the held original guard, not accepted
        // from the caller. Its DB application_id/quick_check are already real.
        let observed = serde_json::to_value(snapshot.report()).map_err(|_| Error::Invalid)?;
        let request = serde_json::json!({"format":2,"target_arch":target_arch(),
            "root_mode":observed["root_mode"],"root_contract":snapshot.root_contract(),
            "data":{"business_schema":observed["business_schema"],"outbox":observed["outbox"]},
            "selected":selected.role(),"candidate":candidate.role(),"fallback":fallback.role()});
        let comparison = rustsimhub_core_launcher::check_upgrade(
            &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
        )
        .map_err(|_| Error::Invalid)?;
        snapshot.revalidate()?;
        Ok(Self {
            snapshot,
            roles: [selected, candidate, fallback],
            comparison,
        })
    }
    pub fn revalidate(&self) -> Result<(), Error> {
        self.snapshot.revalidate()
    }
    pub fn exit_code(&self) -> i32 {
        self.comparison.exit_code()
    }
    pub fn report(&self) -> Result<serde_json::Value, Error> {
        self.revalidate()?;
        Ok(
            serde_json::json!({"format":1,"kind":"product-data-admission",
            "read_only":true,"authorization_verified":false,"may_install":false,"may_migrate":false,"may_confirm":false,
            "signed_programs_verified":true,"mode_metadata_verified":true,"installed_trust_verified":false,
            "slot_identity_verified":false,"effective_systemd_verified":false,
            "manifest_sha256":self.roles.map(|r|r.manifest_sha256.as_str()),
            "source_sha256":self.roles.map(|r|r.source_sha256.as_str()),"comparison":self.comparison}),
        )
    }
}
struct Reap(std::process::Child);
impl Drop for Reap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn capture(bytes: &[u8], args: &[&str], uid: u32) -> Result<Vec<u8>, Error> {
    capture_for(bytes, args, uid, Duration::from_secs(3))
}
fn capture_for(bytes: &[u8], args: &[&str], uid: u32, budget: Duration) -> Result<Vec<u8>, Error> {
    use rustix::fs::{fchmod, fcntl_add_seals, memfd_create, MemfdFlags, Mode, SealFlags};
    let change =
        rustix::process::geteuid().as_raw() != uid || rustix::process::getegid().as_raw() != uid;
    if change {
        use rustix::thread::{capabilities, CapabilitySet};
        if !capabilities(None)
            .map_err(|_| Error::Denied)?
            .effective
            .contains(CapabilitySet::SETUID | CapabilitySet::SETGID | CapabilitySet::KILL)
        {
            return Err(Error::Denied);
        }
    }
    rustix::thread::set_no_new_privs(true).map_err(|_| Error::Denied)?;
    let mut file = File::from(
        memfd_create(
            c"rustsimhub-product-metadata",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )
        .map_err(|_| Error::Io)?,
    );
    file.write_all(bytes).map_err(|_| Error::Io)?;
    fchmod(&file, Mode::from_raw_mode(0o555)).map_err(|_| Error::Denied)?;
    fcntl_add_seals(
        &file,
        SealFlags::SEAL | SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK,
    )
    .map_err(|_| Error::Denied)?;
    let mut command = Command::new("/proc/self/fd/0");
    command
        .arg0("rustsimhub-product-metadata")
        .args(args)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::from(file))
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if change {
        command.uid(uid).gid(uid);
    }
    let mut child = Reap(command.spawn().map_err(|_| Error::Denied)?);
    let mut pipe = child.0.stdout.take().ok_or(Error::Io)?;
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(|_| Error::Io)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK).map_err(|_| Error::Io)?;
    let until = Instant::now() + budget;
    let mut output = Vec::new();
    let mut eof = false;
    loop {
        let mut part = [0; 1024];
        match pipe.read(&mut part) {
            Ok(0) => eof = true,
            Ok(n) => {
                if output.len() + n > 16384 {
                    return Err(Error::Invalid);
                }
                output.extend_from_slice(&part[..n]);
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
        if Instant::now() >= until {
            return Err(Error::Busy);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[cfg(test)]
mod tests;

pub mod coordinator;

// Production ARM policy. Native isolation adapters are compiled only in tests.
pub(crate) fn target_arch() -> &'static str {
    #[cfg(test)]
    if crate::product_native_fixture::enabled() {
        return std::env::consts::ARCH;
    }
    "aarch64"
}
