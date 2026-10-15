// SPDX-License-Identifier: AGPL-3.0-only
//! Trusted local preparation only. Never binds a listener or replaces credentials.
use super::{create_tls, tls, Config, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, net::SocketAddr, os::unix::fs::DirBuilderExt, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    listen: SocketAddr,
    tls_name: String,
    certificate_sha256: String,
    pem_sha256: String,
    config_sha256: String,
}
fn hash(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("installation state unavailable"),
    }
}
fn expected(listen: SocketAddr, name: &str) -> Result<Config> {
    if name.is_empty()
        || name.len() > 253
        || name
            .bytes()
            .any(|b| !(b.is_ascii_alphanumeric() || b".:-".contains(&b)))
    {
        return Err("invalid TLS name");
    }
    use tokio_rustls::rustls::pki_types::ServerName;
    ServerName::try_from(name.to_owned()).map_err(|_| "invalid TLS name")?;
    let authority = if name.contains(':') {
        format!("[{name}]:{}", listen.port())
    } else {
        format!("{name}:{}", listen.port())
    };
    let config = Config {
        listen,
        origin: format!("https://{authority}"),
        core_socket: "/run/rustsimhub/core/core.sock".into(),
        core_uid: 986,
        tls_directory: "/var/lib/rustsimhub-edge/installation/tls".into(),
        legacy_compat: true,
    };
    config.authority()?;
    Ok(config)
}
fn complete(directory: &Path, request: &Config, name: &str) -> Result<Record> {
    fn exact_names(path: &Path, expected: &[&str]) -> Result<()> {
        let mut actual = std::fs::read_dir(path)
            .map_err(|_| "installation directory unavailable")?
            .map(|entry| {
                entry
                    .map(|e| e.file_name())
                    .map_err(|_| "installation entry unavailable")
            })
            .collect::<Result<Vec<_>>>()?;
        actual.sort();
        let mut names = expected
            .iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>();
        names.sort();
        if actual != names {
            return Err("unexpected installation contents");
        }
        Ok(())
    }
    exact_names(directory, &["config.json", "record.json", "tls"])?;
    exact_names(&directory.join("tls"), &["cert.der", "cert.pem", "key.der"])?;
    let record: Record = serde_json::from_slice(
        &rustsimhub_auth::read_private(&directory.join("record.json"), 4096)
            .map_err(|_| "incomplete installation record")?,
    )
    .map_err(|_| "invalid installation record")?;
    if record.format != 1 || record.listen != request.listen || record.tls_name != name {
        return Err("installation request differs; explicit migration required");
    }
    let bytes = rustsimhub_auth::read_private(&directory.join("config.json"), 4096)
        .map_err(|_| "private installation config unavailable")?;
    let config: Config =
        serde_json::from_slice(&bytes).map_err(|_| "invalid installation config")?;
    if config != *request || hash(&bytes) != record.config_sha256 {
        return Err("installation config changed");
    }
    let local_tls = directory.join("tls");
    let cert = rustsimhub_auth::read_private(&local_tls.join("cert.der"), 8192)
        .map_err(|_| "certificate unavailable")?;
    let pem = rustsimhub_auth::read_private(&local_tls.join("cert.pem"), 8192)
        .map_err(|_| "certificate export unavailable")?;
    if hash(&cert) != record.certificate_sha256 || hash(&pem) != record.pem_sha256 {
        return Err("certificate changed");
    }
    tls(&local_tls)?;
    Ok(record)
}
fn canonical_name(name: &str) -> String {
    name.parse::<std::net::IpAddr>()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| name.to_ascii_lowercase())
}
pub(super) fn check(listen: SocketAddr, name: &str) -> Result<serde_json::Value> {
    let name = canonical_name(name);
    let config = expected(listen, &name)?;
    Ok(
        serde_json::json!({"request_validated":true,"origin":config.origin,"listener_started":false}),
    )
}
pub(super) fn prepare(
    directory: &Path,
    listen: SocketAddr,
    name: &str,
) -> Result<serde_json::Value> {
    let canonical = canonical_name(name);
    let name = canonical.as_str();
    let config = expected(listen, name)?;
    let _lock = rustsimhub_auth::lock_private_directory(directory)
        .map_err(|_| "private installation directory unavailable or busy")?;
    let destination = directory.join("installation");
    let pending = directory.join("installation.pending");
    let record = if exists(&destination)? {
        if exists(&pending)? {
            return Err("conflicting installation pending state");
        }
        complete(&destination, &config, name)?
    } else {
        if !exists(&pending)? {
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&pending)
                .map_err(|_| "new pending directory required")?;
            let digest = create_tls(&pending.join("tls"), name)?;
            let bytes = serde_json::to_vec(&config).map_err(|_| "config encoding failed")?;
            rustsimhub_auth::write_private_new(&pending.join("config.json"), &bytes)
                .map_err(|_| "config write failed")?;
            let pem = rustsimhub_auth::read_private(&pending.join("tls/cert.pem"), 8192)
                .map_err(|_| "certificate export unavailable")?;
            let record = Record {
                format: 1,
                listen,
                tls_name: name.to_owned(),
                certificate_sha256: digest,
                pem_sha256: hash(&pem),
                config_sha256: hash(&bytes),
            };
            rustsimhub_auth::write_private_new(
                &pending.join("record.json"),
                &serde_json::to_vec(&record).map_err(|_| "record encoding failed")?,
            )
            .map_err(|_| "record write failed")?;
        }
        let record = complete(&pending, &config, name)?;
        rustsimhub_auth::publish_private_directory(&pending, &destination)
            .map_err(|_| "atomic installation publication failed")?;
        record
    };
    Ok(
        serde_json::json!({"installation_prepared":true,"certificate_sha256":record.certificate_sha256,"origin":config.origin,"self_signed":true,"listener_started":false,"credentials_disclosed":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    fn directory() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    fn listen() -> SocketAddr {
        "127.0.0.1:8448".parse().unwrap()
    }
    #[test]
    fn credentials_are_persistent_and_complete_pending_resumes() {
        let d = directory();
        assert!(prepare(d.path(), listen(), "bad_name.local").is_err());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
        let first = prepare(d.path(), listen(), "127.0.0.1").unwrap();
        let key = fs::read(d.path().join("installation/tls/key.der")).unwrap();
        assert_eq!(prepare(d.path(), listen(), "127.0.0.1").unwrap(), first);
        fs::rename(
            d.path().join("installation"),
            d.path().join("installation.pending"),
        )
        .unwrap();
        assert_eq!(prepare(d.path(), listen(), "127.0.0.1").unwrap(), first);
        assert_eq!(
            fs::read(d.path().join("installation/tls/key.der")).unwrap(),
            key
        );
        assert!(!d.path().join("installation.pending").exists());
        assert!(prepare(d.path(), "127.0.0.1:8449".parse().unwrap(), "127.0.0.1").is_err());
    }
    #[test]
    fn partial_conflicting_or_linked_state_is_never_repaired() {
        let d = directory();
        let pending = d.path().join("installation.pending");
        fs::create_dir(&pending).unwrap();
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(pending.join("partial"), b"preserve").unwrap();
        assert!(prepare(d.path(), listen(), "127.0.0.1").is_err());
        assert_eq!(fs::read(pending.join("partial")).unwrap(), b"preserve");
        let d = directory();
        let outside = directory();
        symlink(outside.path(), d.path().join("installation")).unwrap();
        assert!(prepare(d.path(), listen(), "127.0.0.1").is_err());
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
        let d = directory();
        prepare(d.path(), listen(), "127.0.0.1").unwrap();
        fs::create_dir(d.path().join("installation.pending")).unwrap();
        assert!(prepare(d.path(), listen(), "127.0.0.1").is_err());
    }
    #[test]
    fn bad_request_busy_parent_and_tampered_material_fail_closed() {
        let d = directory();
        assert!(prepare(d.path(), "0.0.0.0:8448".parse().unwrap(), "127.0.0.1").is_err());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
        let lock = rustsimhub_auth::lock_private_directory(d.path()).unwrap();
        assert!(prepare(d.path(), listen(), "127.0.0.1").is_err());
        drop(lock);
        prepare(d.path(), listen(), "127.0.0.1").unwrap();
        for file in ["config.json", "tls/cert.pem", "tls/cert.der", "tls/key.der"] {
            let path = d.path().join("installation").join(file);
            let original = fs::read(&path).unwrap();
            fs::write(&path, b"corrupt").unwrap();
            assert!(prepare(d.path(), listen(), "127.0.0.1").is_err());
            assert_eq!(fs::read(&path).unwrap(), b"corrupt");
            fs::write(path, original).unwrap();
        }
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(prepare(d.path(), listen(), "127.0.0.1").is_err());
    }
}
