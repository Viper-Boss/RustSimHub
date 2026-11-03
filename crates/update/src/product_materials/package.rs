// SPDX-License-Identifier: AGPL-3.0-only
//! Signed finite Root materials. A detached plan or package-supplied key is never trust.
use super::targets::{Profile, RoleId, TargetId};
use super::{Error, Result};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, Metadata},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
};

pub const SIGNATURE_DOMAIN: &[u8] = b"RustSimHub.ProductMaintenance.v1\0";
pub const MAX_MANIFEST: usize = 32 * 1024;
pub const MAX_FILE: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL: usize = 64 * 1024 * 1024;
const MAX_CONFIG: usize = 128 * 1024;

/// Expected facts come from the caller's live application/source/NSS admission.
/// Deserializing these facts does not create VerifiedProductMaterials or an install permit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedBindings {
    pub profile: Profile,
    pub source_commit: String,
    pub source_archive_sha256: String,
    pub application_manifest_sha256: String,
    pub core_sha256: String,
    pub approved_role_policy_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialEntry {
    pub id: TargetId,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductManifestV1 {
    pub format: u16,
    pub kind: String,
    pub board: String,
    pub arch: String,
    pub bindings: ExpectedBindings,
    pub roles: Vec<RoleId>,
    pub files: Vec<MaterialEntry>,
}
impl ProductManifestV1 {
    pub fn encode(&self) -> Result<Vec<u8>> {
        validate_manifest(self, &self.bindings)?;
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Invalid)?;
        if bytes.len() > MAX_MANIFEST {
            return Err(Error::Invalid);
        }
        Ok(bytes)
    }
}
fn hex(value: &str, n: usize) -> bool {
    value.len() == n
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn material_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn product_signing_bytes(manifest: &[u8]) -> Result<Vec<u8>> {
    if manifest.is_empty() || manifest.len() > MAX_MANIFEST {
        return Err(Error::Invalid);
    }
    let mut signed = SIGNATURE_DOMAIN.to_vec();
    signed.extend_from_slice(manifest);
    Ok(signed)
}
fn validate_manifest(m: &ProductManifestV1, expected: &ExpectedBindings) -> Result<()> {
    if m.format != 1
        || m.kind != "product-maintenance"
        || m.board != "ufi003-v02"
        || m.arch != "aarch64-linux"
    {
        return Err(Error::Invalid);
    }
    if !hex(&m.bindings.source_commit, 40)
        || [
            &m.bindings.source_archive_sha256,
            &m.bindings.application_manifest_sha256,
            &m.bindings.core_sha256,
            &m.bindings.approved_role_policy_sha256,
        ]
        .iter()
        .any(|s| !hex(s, 64))
    {
        return Err(Error::Invalid);
    }
    if &m.bindings != expected {
        return Err(Error::Denied);
    }
    if m.roles != m.bindings.profile.required_roles() {
        return Err(Error::Invalid);
    }
    let targets = m.bindings.profile.targets();
    if m.files.len() != targets.len() {
        return Err(Error::Integrity);
    }
    let mut total = 0u64;
    for (entry, target) in m.files.iter().zip(targets) {
        let limit = if target.executable {
            MAX_FILE
        } else {
            MAX_CONFIG
        };
        if entry.id != target.id
            || entry.bytes == 0
            || entry.bytes > limit as u64
            || !hex(&entry.sha256, 64)
        {
            return Err(Error::Integrity);
        }
        total = total.checked_add(entry.bytes).ok_or(Error::Invalid)?;
    }
    if total > MAX_TOTAL as u64 {
        return Err(Error::Invalid);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: u32,
    links: u64,
    bytes: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<Metadata> for Stamp {
    fn from(m: Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            uid: m.uid(),
            gid: m.gid(),
            mode: m.mode(),
            links: m.nlink(),
            bytes: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}
struct SourceFile {
    name: &'static str,
    file: File,
    stamp: Stamp,
    bytes: Vec<u8>,
}
fn safe_metadata(m: &Metadata, directory: bool) -> Result<()> {
    if (directory && !m.is_dir())
        || (!directory && (!m.is_file() || m.nlink() != 1))
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o022 != 0
    {
        return Err(Error::Denied);
    }
    Ok(())
}
fn opened(directory: &File, name: &str) -> Result<File> {
    let fd = rustix::fs::openat(
        directory,
        name,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|_| Error::Io)?;
    Ok(File::from(fd))
}
fn read_file(file: &File, limit: usize) -> Result<Vec<u8>> {
    let m = file.metadata().map_err(|_| Error::Io)?;
    safe_metadata(&m, false)?;
    if m.len() > limit as u64 {
        return Err(Error::Invalid);
    }
    let mut data = vec![0; m.len() as usize];
    let mut offset = 0;
    while offset < data.len() {
        let n = file
            .read_at(&mut data[offset..], offset as u64)
            .map_err(|_| Error::Io)?;
        if n == 0 {
            return Err(Error::Integrity);
        }
        offset += n;
    }
    let mut extra = [0];
    if file
        .read_at(&mut extra, data.len() as u64)
        .map_err(|_| Error::Io)?
        != 0
    {
        return Err(Error::Integrity);
    }
    if Stamp::from(m) != Stamp::from(file.metadata().map_err(|_| Error::Io)?) {
        return Err(Error::Integrity);
    }
    Ok(data)
}
fn source(directory: &File, name: &'static str, limit: usize) -> Result<SourceFile> {
    let file = opened(directory, name)?;
    let stamp = file.metadata().map_err(|_| Error::Io)?.into();
    let bytes = read_file(&file, limit)?;
    if stamp != file.metadata().map_err(|_| Error::Io)?.into() {
        return Err(Error::Integrity);
    }
    Ok(SourceFile {
        name,
        file,
        stamp,
        bytes,
    })
}
fn entries(directory: &File, expected: &BTreeSet<&str>) -> Result<()> {
    // The proc-fd alias names this retained descriptor, not an input path. No
    // untrusted path or package-selected directory is traversed here.
    let path = format!("/proc/self/fd/{}", directory.as_raw_fd());
    let mut count = 0;
    for entry in fs::read_dir(path).map_err(|_| Error::Io)? {
        count += 1;
        let name = entry.map_err(|_| Error::Io)?.file_name();
        if count > expected.len() || !name.to_str().is_some_and(|n| expected.contains(n)) {
            return Err(Error::Integrity);
        }
    }
    if count != expected.len() {
        return Err(Error::Integrity);
    }
    Ok(())
}

/// Detached finite plan metadata. It is deliberately output-only: neither
/// deserialization nor a caller-written JSON flag can produce an install permit.
#[derive(Serialize)]
pub struct ProductMaterialPlanV1 {
    format: u16,
    kind: &'static str,
    profile: Profile,
    bindings: ExpectedBindings,
    roles: Vec<RoleId>,
    files: Vec<PlannedMaterial>,
    manifest_sha256: String,
    required_steps: &'static [&'static str],
    authorization_verified: bool,
    root_baseline_verified: bool,
    nss_baseline_verified: bool,
    installed: bool,
    starts_services: bool,
}
#[derive(Serialize)]
struct PlannedMaterial {
    id: TargetId,
    bytes: u64,
    sha256: String,
    mode: u32,
    uid: u32,
    gid: u32,
    phase: super::targets::MaterialPhase,
}

/// Retains actual source descriptors and exact signed bytes. No Clone,
/// Deserialize, caller-supplied verified flag, or detached install authority.
pub struct VerifiedProductMaterials {
    directory: File,
    directory_stamp: Stamp,
    sources: Vec<SourceFile>,
    manifest: ProductManifestV1,
    manifest_sha256: String,
}
impl VerifiedProductMaterials {
    /// The caller opens its fixed inbox using its live Root directory guard and
    /// retains installed trust/application/data guards. This API takes no path,
    /// package key, shell, UID, destination or JSON permission argument.
    pub fn open(
        directory: &File,
        trusted_key: &[u8; 32],
        expected: &ExpectedBindings,
    ) -> Result<Self> {
        let metadata = directory.metadata().map_err(|_| Error::Io)?;
        safe_metadata(&metadata, true)?;
        let directory_stamp = metadata.into();
        let directory = directory.try_clone().map_err(|_| Error::Io)?;
        let manifest = source(&directory, "manifest.json", MAX_MANIFEST)?;
        let signature = source(&directory, "manifest.sig", 64)?;
        let key = VerifyingKey::from_bytes(trusted_key).map_err(|_| Error::Signature)?;
        let sig = Signature::from_slice(&signature.bytes).map_err(|_| Error::Signature)?;
        key.verify_strict(&product_signing_bytes(&manifest.bytes)?, &sig)
            .map_err(|_| Error::Signature)?;
        let parsed: ProductManifestV1 =
            serde_json::from_slice(&manifest.bytes).map_err(|_| Error::Invalid)?;
        validate_manifest(&parsed, expected)?;
        let manifest_sha256 = material_sha256(&manifest.bytes);
        let mut sources = vec![manifest, signature];
        for entry in &parsed.files {
            let file = source(&directory, entry.id.payload_name(), entry.bytes as usize)?;
            if file.bytes.len() as u64 != entry.bytes
                || material_sha256(&file.bytes) != entry.sha256
            {
                return Err(Error::Integrity);
            }
            validate_payload(entry.id, &file.bytes, &parsed.bindings)?;
            sources.push(file);
        }
        let verified = Self {
            directory,
            directory_stamp,
            sources,
            manifest: parsed,
            manifest_sha256,
        };
        verified.revalidate()?;
        Ok(verified)
    }
    pub fn revalidate(&self) -> Result<()> {
        if Stamp::from(self.directory.metadata().map_err(|_| Error::Io)?) != self.directory_stamp {
            return Err(Error::Integrity);
        }
        let expected = self.sources.iter().map(|s| s.name).collect::<BTreeSet<_>>();
        entries(&self.directory, &expected)?;
        for s in &self.sources {
            let named = opened(&self.directory, s.name)?;
            if Stamp::from(named.metadata().map_err(|_| Error::Io)?) != s.stamp
                || Stamp::from(s.file.metadata().map_err(|_| Error::Io)?) != s.stamp
                || read_file(&s.file, s.bytes.len())? != s.bytes
            {
                return Err(Error::Integrity);
            }
        }
        if Stamp::from(self.directory.metadata().map_err(|_| Error::Io)?) != self.directory_stamp {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    /// Exact verified source bytes, including the signature envelope, for a
    /// transaction-owned copy. This view cannot create a new verification claim.
    pub(super) fn source_files(&self) -> impl Iterator<Item = (&'static str, &[u8])> {
        self.sources
            .iter()
            .map(|source| (source.name, source.bytes.as_slice()))
    }
    pub(super) fn manifest(&self) -> &ProductManifestV1 {
        &self.manifest
    }
    pub(super) fn payload(&self, id: TargetId) -> Result<&[u8]> {
        self.sources
            .iter()
            .find(|s| s.name == id.payload_name())
            .map(|s| s.bytes.as_slice())
            .ok_or(Error::Invalid)
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub fn bindings(&self) -> &ExpectedBindings {
        &self.manifest.bindings
    }
    /// Must be called by prepare with IDs from the retained Root-approved NSS
    /// policy. Package payload numbers cannot approve their own allocation.
    pub fn validate_role_materials(&self, nkn_gid: u32, ble_gid: u32) -> Result<()> {
        if !self.manifest.bindings.profile.product() {
            return Ok(());
        }
        if nkn_gid == ble_gid
            || [nkn_gid, ble_gid]
                .iter()
                .any(|id| matches!(*id, 0 | 29 | 983..=987 | 65534 | u32::MAX))
        {
            return Err(Error::Denied);
        }
        let expected = tmpfiles(nkn_gid, ble_gid);
        if self.payload(TargetId::ProductTmpfiles)? != expected.as_bytes() {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub fn plan(&self) -> Result<ProductMaterialPlanV1> {
        self.revalidate()?;
        Ok(ProductMaterialPlanV1 {
            format: 1,
            kind: "product-material-plan",
            profile: self.manifest.bindings.profile,
            bindings: self.manifest.bindings.clone(),
            roles: self.manifest.roles.clone(),
            files: self
                .manifest
                .files
                .iter()
                .map(|entry| {
                    let spec = entry.id.spec();
                    PlannedMaterial {
                        id: entry.id,
                        bytes: entry.bytes,
                        sha256: entry.sha256.clone(),
                        mode: spec.mode,
                        uid: 0,
                        gid: 0,
                        phase: spec.phase,
                    }
                })
                .collect(),
            manifest_sha256: self.manifest_sha256.clone(),
            required_steps: &[
                "retain_live_maintenance_and_data_fences",
                "verify_actual_nss_and_root_baseline",
                "stop_and_verify_finite_services",
                "durably_backup_and_prepare_journal",
                "replace_fixed_targets_and_fsync",
                "switch_profile_then_launcher_last",
                "revalidate_and_inspect_or_recover_without_enabling_services",
            ],
            authorization_verified: false,
            root_baseline_verified: false,
            nss_baseline_verified: false,
            installed: false,
            starts_services: false,
        })
    }
    pub fn plan_metadata(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.plan()?).map_err(|_| Error::Invalid)
    }
}
fn tmpfiles(nkn_gid: u32, ble_gid: u32) -> String {
    include_str!("../../../../firmware/system/product-profiles/product.tmpfiles.in")
        .replace("@NKN_GID@", &nkn_gid.to_string())
        .replace("@BLE_GID@", &ble_gid.to_string())
}
fn tmpfile_gids(bytes: &[u8]) -> Result<(u32, u32)> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != 4 {
        return Err(Error::Invalid);
    }
    let parse = |line: &str, prefix: &str| -> Result<u32> {
        let number = line
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(" -"))
            .ok_or(Error::Invalid)?;
        let id = number.parse::<u32>().map_err(|_| Error::Invalid)?;
        if id.to_string() != number {
            return Err(Error::Invalid);
        }
        Ok(id)
    };
    let nkn = parse(lines[2], "d /run/rustsimhub/nkn 2750 rustsimhub-core ")?;
    let ble = parse(lines[3], "d /run/rustsimhub/recovery 2750 rustsimhub-core ")?;
    if tmpfiles(nkn, ble).as_bytes() != bytes {
        return Err(Error::Integrity);
    }
    Ok((nkn, ble))
}
fn elf(bytes: &[u8]) -> Result<()> {
    if bytes.len() < 64
        || &bytes[..7] != b"\x7fELF\x02\x01\x01"
        || !matches!(u16::from_le_bytes([bytes[16], bytes[17]]), 2 | 3)
        || u16::from_le_bytes([bytes[18], bytes[19]]) != 183
        || u16::from_le_bytes([bytes[52], bytes[53]]) != 64
    {
        return Err(Error::Invalid);
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InstalledProfile {
    format: u16,
    profile: Profile,
    source_archive_sha256: String,
    core_sha256: String,
}
fn validate_payload(id: TargetId, bytes: &[u8], binding: &ExpectedBindings) -> Result<()> {
    if id.spec().executable {
        return elf(bytes);
    }
    let matches = match id {
        TargetId::ProfileConfig => {
            let p: InstalledProfile = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
            p.format == 1
                && p.profile == binding.profile
                && p.source_archive_sha256 == binding.source_archive_sha256
                && p.core_sha256 == binding.core_sha256
        }
        TargetId::CoreDropin => {
            let expected = include_str!(
                "../../../../firmware/system/product-profiles/50-product-profile.conf"
            )
            .replace(
                "@CORE_MEMORY@",
                if binding.profile.product() {
                    "192M"
                } else {
                    "96M"
                },
            )
            .replace(
                "@CORE_TASKS@",
                if binding.profile.product() {
                    "64"
                } else {
                    "16"
                },
            );
            bytes == expected.as_bytes()
        }
        TargetId::NknUnit => {
            bytes
                == include_bytes!(
                    "../../../../firmware/system/product-profiles/rustsimhub-nkn-root.service"
                )
        }
        TargetId::BleUnit => {
            bytes
                == include_bytes!(
                    "../../../../firmware/system/product-profiles/rustsimhub-ble-recovery.service"
                )
        }
        TargetId::BlePolicy => {
            bytes
                == include_bytes!("../../../../firmware/system/product-profiles/ble-product.policy")
        }
        TargetId::ProductTmpfiles => {
            tmpfile_gids(bytes)?;
            true
        }
        _ => return Err(Error::Invalid),
    };
    if !matches {
        return Err(Error::Integrity);
    }
    Ok(())
}
