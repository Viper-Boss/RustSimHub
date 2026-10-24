// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit local material preparation/export; never starts a service or reads a TLS private key.
use super::*;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::PathBuf,
};
use zeroize::Zeroizing;
fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingFile {
    format: u16,
    realm: String,
    core_uid: u32,
    device_public_key: [u8; 32],
}
impl BindingFile {
    fn validate(&self) -> Result<()> {
        let realm = self.realm.len() == 36
            && self.realm.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                }
            });
        if self.format != 1
            || !realm
            || self.core_uid == 0
            || self.core_uid == u32::MAX
            || self.device_public_key == [0; 32]
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeConfig {
    listen: std::net::SocketAddr,
    origin: String,
    core_socket: PathBuf,
    core_uid: u32,
    tls_directory: PathBuf,
    #[serde(default)]
    legacy_compat: bool,
    #[serde(default)]
    media_worker_socket: Option<PathBuf>,
    #[serde(default)]
    additional_listen_ips: Vec<std::net::IpAddr>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EdgeRecord {
    format: u16,
    listen: std::net::SocketAddr,
    tls_name: String,
    certificate_sha256: String,
    pem_sha256: String,
    config_sha256: String,
    #[serde(default)]
    media_sha256: Option<String>,
}
struct Snapshot {
    cert: Vec<u8>,
    config_hash: String,
    cert_hash: String,
}
fn snapshot(path: &Path, uid: u32, binding: &BindingFile) -> Result<Snapshot> {
    if uid == 0 || uid == u32::MAX {
        return Err(Error::Denied);
    }
    let dir = Directory::open(path, uid, uid, 0o700)?;
    let raw = dir.read_name("config.json", 0o600)?.ok_or(Error::Denied)?;
    let record = dir.read_name("record.json", 0o600)?.ok_or(Error::Denied)?;
    let config: EdgeConfig = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
    let record: EdgeRecord = serde_json::from_slice(&record).map_err(|_| Error::Invalid)?;
    let ap: std::net::IpAddr = crate::ADDRESS.parse().map_err(|_| Error::Invalid)?;
    let loopback = std::net::SocketAddr::from(([127, 0, 0, 1], crate::HTTPS_PORT));
    if config.listen != loopback
        || config.origin != format!("https://{}:{}", crate::ADDRESS, crate::HTTPS_PORT)
        || config.core_uid != binding.core_uid
        || config.core_socket != Path::new("/run/rustsimhub/core/core.sock")
        || config.tls_directory != Path::new("/var/lib/rustsimhub-setup-edge/installation/tls")
        || config.legacy_compat
        || config.media_worker_socket.is_some()
        || config.additional_listen_ips != [ap]
        || record.format != 1
        || record.listen != loopback
        || record.tls_name != crate::ADDRESS
        || record.config_sha256 != hash(&raw)
        || record.media_sha256.is_some()
    {
        return Err(Error::Changed);
    }
    let tls = Directory::open(&path.join("tls"), uid, uid, 0o700)?;
    let cert = tls.read_name("cert.der", 0o600)?.ok_or(Error::Denied)?;
    let pem = tls.read_name("cert.pem", 0o600)?.ok_or(Error::Denied)?;
    if cert.is_empty()
        || record.certificate_sha256 != hash(&cert)
        || record.pem_sha256 != hash(&pem)
    {
        return Err(Error::Changed);
    }
    // Parse the certificate without network access; availability still requires the live pinned probe.
    let mut roots = tokio_rustls::rustls::RootCertStore::empty();
    roots
        .add(tokio_rustls::rustls::pki_types::CertificateDer::from(
            cert.clone(),
        ))
        .map_err(|_| Error::Invalid)?;
    Ok(Snapshot {
        config_hash: hash(&raw),
        cert_hash: hash(&cert),
        cert,
    })
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    binding_sha256: String,
    edge_config_sha256: String,
    certificate_sha256: String,
    material_sha256: String,
}
#[derive(Serialize)]
struct PrivateConfig<'a> {
    format: u16,
    realm: &'a str,
    core_uid: u32,
    device_public_key: [u8; 32],
    ssid: &'a str,
    passphrase: &'a str,
    uuid: &'a str,
}
fn lock(parent: &Path, uid: u32) -> Result<File> {
    Directory::open(parent, uid, uid, 0o700)?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(parent.join(".lock"))
        .map_err(|_| Error::Io)?;
    let m = f.metadata().map_err(|_| Error::Io)?;
    if !m.is_file()
        || m.uid() != uid
        || m.gid() != uid
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
    {
        return Err(Error::Denied);
    }
    f.try_lock_exclusive().map_err(|_| Error::Busy)?;
    Ok(f)
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Error::Io),
    }
}
fn verify(
    path: &Path,
    uid: u32,
    binding: &BindingFile,
    raw_binding: &[u8],
    edge: &Snapshot,
) -> Result<Material> {
    let dir = Directory::open(path, uid, uid, 0o700)?;
    let raw = dir.read_name("record.json", 0o600)?.ok_or(Error::Denied)?;
    let record: Record = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
    let material = Material::open(dir)?;
    if record.format != 1
        || !material.matches(&binding.realm, binding.core_uid, &binding.device_public_key)
        || record.binding_sha256 != hash(raw_binding)
        || record.edge_config_sha256 != edge.config_hash
        || record.certificate_sha256 != edge.cert_hash
        || record.material_sha256 != material.digest
    {
        return Err(Error::Changed);
    }
    let own = Directory::open(path, uid, uid, 0o700)?
        .read_name("cert.der", 0o600)?
        .ok_or(Error::Denied)?;
    if own != edge.cert {
        return Err(Error::Changed);
    }
    let mut names = fs::read_dir(path)
        .map_err(|_| Error::Io)?
        .map(|v| v.map(|e| e.file_name()).map_err(|_| Error::Io))
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    let mut expected = ["config.json", "cert.der", "record.json"].map(std::ffi::OsString::from);
    expected.sort();
    if names != expected {
        return Err(Error::Changed);
    }
    Ok(material)
}
pub(super) fn prepare(
    parent: &Path,
    binding: &Path,
    edge: &Path,
    edge_uid: u32,
) -> Result<serde_json::Value> {
    prepare_owned(parent, binding, edge, edge_uid, 0)
}
fn prepare_owned(
    parent: &Path,
    binding_dir: &Path,
    edge_dir: &Path,
    edge_uid: u32,
    owner: u32,
) -> Result<serde_json::Value> {
    prepare_with_binding_owner(parent, binding_dir, edge_dir, edge_uid, owner, owner)
}
pub(super) fn prepare_from_core(
    parent: &Path,
    core: &Path,
    edge: &Path,
    edge_uid: u32,
    core_uid: u32,
) -> Result<serde_json::Value> {
    if core_uid == 0 || core_uid == u32::MAX {
        return Err(Error::Denied);
    }
    prepare_with_binding_owner(parent, core, edge, edge_uid, 0, core_uid)
}
fn prepare_with_binding_owner(
    parent: &Path,
    binding_dir: &Path,
    edge_dir: &Path,
    edge_uid: u32,
    owner: u32,
    binding_owner: u32,
) -> Result<serde_json::Value> {
    let _lock = lock(parent, owner)?;
    let binding_dir = Directory::open(binding_dir, binding_owner, binding_owner, 0o700)?;
    let _binding_lock = binding_dir.lock_directory()?;
    let raw = binding_dir
        .read_name("binding.json", 0o600)?
        .ok_or(Error::Denied)?;
    let binding: BindingFile = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
    binding.validate()?;
    if binding_owner != owner && binding.core_uid != binding_owner {
        return Err(Error::Denied);
    }
    let edge = snapshot(edge_dir, edge_uid, &binding)?;
    let destination = parent.join("installation");
    let pending = parent.join("installation.pending");
    if exists(&destination)? && exists(&pending)? {
        return Err(Error::Changed);
    }
    if !exists(&destination)? {
        if !exists(&pending)? {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&pending)
                .map_err(|_| Error::Io)?;
            let dir = Directory::open(&pending, owner, owner, 0o700)?;
            let credentials = Credentials::generate()?;
            let config = PrivateConfig {
                format: 1,
                realm: &binding.realm,
                core_uid: binding.core_uid,
                device_public_key: binding.device_public_key,
                ssid: credentials.ssid(),
                passphrase: credentials.expose_passphrase_for_trusted_delivery(),
                uuid: credentials.uuid(),
            };
            let bytes = Zeroizing::new(serde_json::to_vec(&config).map_err(|_| Error::Invalid)?);
            dir.write_name("config.json", &bytes, 0o600, true)?;
            dir.write_name("cert.der", &edge.cert, 0o600, true)?;
            let material = Material::open(dir.clone())?;
            let record = Record {
                format: 1,
                binding_sha256: hash(&raw),
                edge_config_sha256: edge.config_hash.clone(),
                certificate_sha256: edge.cert_hash.clone(),
                material_sha256: material.digest,
            };
            dir.write_name(
                "record.json",
                &serde_json::to_vec(&record).map_err(|_| Error::Invalid)?,
                0o600,
                true,
            )?;
        }
        verify(&pending, owner, &binding, &raw, &edge)?;
        // Linux RENAME_NOREPLACE keeps a concurrent destination or link from being replaced.
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            &pending,
            rustix::fs::CWD,
            &destination,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|_| Error::Io)?;
        File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| Error::Io)?;
    }
    let material = verify(&destination, owner, &binding, &raw, &edge)?;
    Ok(
        serde_json::json!({"material_prepared":true,"material_sha256":material.digest,"certificate_sha256":edge.cert_hash,"credentials_disclosed":false,"listener_started":false,"ap_started":false}),
    )
}
#[derive(Serialize)]
struct TrustedConnection<'a> {
    format: u16,
    ssid: &'a str,
    passphrase: &'a str,
    origin: &'static str,
    device_public_key: String,
    device_key_sha256: String,
    certificate_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    setup_code: Option<&'a str>,
}
pub(super) fn export(material: &Path, output: &Path) -> Result<serde_json::Value> {
    export_owned(material, output, 0)
}
fn export_owned(material: &Path, output: &Path, owner: u32) -> Result<serde_json::Value> {
    export_with_code(material, output, owner, None)
}
pub(super) fn export_first_setup(
    material: &Path,
    core: &Path,
    core_uid: u32,
    output: &Path,
) -> Result<serde_json::Value> {
    if core_uid == 0 || core_uid == u32::MAX {
        return Err(Error::Denied);
    }
    export_first_owned(material, core, core_uid, output, 0)
}
fn export_first_owned(
    material: &Path,
    core: &Path,
    core_uid: u32,
    output: &Path,
    owner: u32,
) -> Result<serde_json::Value> {
    let source = Directory::open(core, core_uid, core_uid, 0o700)?;
    let _setup_lock = source.lock_directory()?;
    if exists(&core.join("setup.used"))? {
        return Err(Error::Denied);
    }
    let raw = source
        .read_name("binding.json", 0o600)?
        .ok_or(Error::Denied)?;
    let binding: BindingFile = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
    binding.validate()?;
    let prepared = Material::open(Directory::open(material, owner, owner, 0o700)?)?;
    if binding.core_uid != core_uid
        || !prepared.matches(&binding.realm, core_uid, &binding.device_public_key)
    {
        return Err(Error::Changed);
    }
    let code = Zeroizing::new(
        source
            .read_name("setup.code", 0o600)?
            .ok_or(Error::Denied)?,
    );
    if code.len() != 32 {
        return Err(Error::Invalid);
    }
    let encoded = Zeroizing::new(code.iter().map(|v| format!("{v:02x}")).collect::<String>());
    export_with_code(material, output, owner, Some(&encoded))
}
fn export_with_code(
    material: &Path,
    output: &Path,
    owner: u32,
    setup_code: Option<&str>,
) -> Result<serde_json::Value> {
    let source = Directory::open(material, owner, owner, 0o700)?;
    let material = Material::open(source.clone())?;
    let certificate = source.read_name("cert.der", 0o600)?.ok_or(Error::Denied)?;
    let record: Record = serde_json::from_slice(
        &source
            .read_name("record.json", 0o600)?
            .ok_or(Error::Denied)?,
    )
    .map_err(|_| Error::Invalid)?;
    if record.format != 1
        || record.material_sha256 != material.digest
        || record.certificate_sha256 != hash(&certificate)
    {
        return Err(Error::Changed);
    }
    let parent = output.parent().ok_or(Error::Invalid)?;
    let _lock = lock(parent, owner)?;
    if !output.is_absolute() {
        return Err(Error::Invalid);
    }
    fs::DirBuilder::new()
        .mode(0o700)
        .create(output)
        .map_err(|_| Error::Busy)?;
    let target = Directory::open(output, owner, owner, 0o700)?;
    let connection = TrustedConnection {
        format: 1,
        ssid: material.credentials.ssid(),
        passphrase: material
            .credentials
            .expose_passphrase_for_trusted_delivery(),
        origin: "https://172.31.85.1:8443",
        device_public_key: material.device_public_key_hex(),
        device_key_sha256: material.device_hash(),
        certificate_sha256: hash(&certificate),
        setup_code,
    };
    let bytes = Zeroizing::new(serde_json::to_vec(&connection).map_err(|_| Error::Invalid)?);
    target.write_name("connection.json", &bytes, 0o600, true)?;
    target.write_name("cert.der", &certificate, 0o600, true)?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)?;
    Ok(
        serde_json::json!({"trusted_export_written":true,"credentials_in_stdout":false,"first_setup_code_in_private_export":setup_code.is_some(),"certificate_sha256":hash(&certificate),"ap_started":false}),
    )
}
#[cfg(test)]
mod tests;
