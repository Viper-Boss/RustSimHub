// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::PermissionsExt;
struct Fixture {
    _base: tempfile::TempDir,
    parent: PathBuf,
    binding: PathBuf,
    edge: PathBuf,
    uid: u32,
    edge_uid: u32,
}
fn write(dir: &Path, name: &str, bytes: &[u8]) {
    fs::write(dir.join(name), bytes).unwrap();
    fs::set_permissions(dir.join(name), fs::Permissions::from_mode(0o600)).unwrap();
}
impl Fixture {
    fn new() -> Self {
        let base = tempfile::tempdir().unwrap();
        let uid = fs::metadata(base.path()).unwrap().uid();
        let edge_uid = if uid == 0 { 65534 } else { uid };
        let parent = base.path().join("root");
        let binding = base.path().join("binding");
        let edge = base.path().join("edge");
        for p in [&parent, &binding, &edge] {
            fs::DirBuilder::new().mode(0o700).create(p).unwrap();
        }
        fs::DirBuilder::new()
            .mode(0o700)
            .create(edge.join("tls"))
            .unwrap();
        let data = serde_json::json!({"format":1,"realm":"00000000-0000-4000-8000-000000000002","core_uid":986,"device_public_key":([7u8;32])});
        write(
            &binding,
            "binding.json",
            &serde_json::to_vec(&data).unwrap(),
        );
        let config = serde_json::json!({"listen":"127.0.0.1:8443","origin":"https://172.31.85.1:8443","core_socket":"/run/rustsimhub/core/core.sock","core_uid":986,"tls_directory":"/var/lib/rustsimhub-setup-edge/installation/tls","legacy_compat":false,"additional_listen_ips":["172.31.85.1"]});
        let cert = rcgen::generate_simple_self_signed(vec![crate::ADDRESS.into()]).unwrap();
        let config = serde_json::to_vec(&config).unwrap();
        let pem = cert.cert.pem();
        write(&edge, "config.json", &config);
        write(&edge.join("tls"), "cert.der", cert.cert.der());
        write(&edge.join("tls"), "cert.pem", pem.as_bytes());
        let record = serde_json::json!({"format":1,"listen":"127.0.0.1:8443","tls_name":crate::ADDRESS,"certificate_sha256":hash(cert.cert.der()),"pem_sha256":hash(pem.as_bytes()),"config_sha256":hash(&config)});
        write(&edge, "record.json", &serde_json::to_vec(&record).unwrap());
        if uid == 0 {
            for path in [
                &edge,
                &edge.join("tls"),
                &edge.join("config.json"),
                &edge.join("record.json"),
                &edge.join("tls/cert.der"),
                &edge.join("tls/cert.pem"),
            ] {
                rustix::fs::chown(
                    path,
                    Some(rustix::process::Uid::from_raw(edge_uid)),
                    Some(rustix::process::Gid::from_raw(edge_uid)),
                )
                .unwrap();
            }
        }
        Self {
            _base: base,
            parent,
            binding,
            edge,
            uid,
            edge_uid,
        }
    }
    fn prepare(&self) -> Result<serde_json::Value> {
        prepare_owned(
            &self.parent,
            &self.binding,
            &self.edge,
            self.edge_uid,
            self.uid,
        )
    }
    fn material(&self) -> PathBuf {
        self.parent.join("installation")
    }
}
#[test]
fn per_device_material_is_stable_bound_and_exported_only_to_a_new_private_directory() {
    let f = Fixture::new();
    let first = f.prepare().unwrap();
    let raw = fs::read(f.material().join("config.json")).unwrap();
    assert_eq!(first["credentials_disclosed"], false);
    assert_eq!(first["ap_started"], false);
    let second = f.prepare().unwrap();
    assert_eq!(first, second);
    assert_eq!(raw, fs::read(f.material().join("config.json")).unwrap());
    let private: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let secret = private["passphrase"].as_str().unwrap();
    assert_eq!(secret.len(), 48);
    assert!(!first.to_string().contains(secret));
    let output = f.parent.join("trusted-export");
    let summary = export_owned(&f.material(), &output, f.uid).unwrap();
    assert!(!summary.to_string().contains(secret));
    assert_eq!(summary["credentials_in_stdout"], false);
    let file = output.join("connection.json");
    let out: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(out["passphrase"], private["passphrase"]);
    assert_eq!(out["ssid"], private["ssid"]);
    assert_eq!(out["device_public_key"], "07".repeat(32));
    assert_eq!(
        out["certificate_sha256"],
        hash(&fs::read(output.join("cert.der")).unwrap())
    );
    assert_eq!(fs::metadata(&file).unwrap().mode() & 0o7777, 0o600);
    assert!(export_owned(&f.material(), &output, f.uid).is_err());
    let other = Fixture::new();
    other.prepare().unwrap();
    assert_ne!(raw, fs::read(other.material().join("config.json")).unwrap());
}
#[test]
fn changed_identity_config_or_certificate_is_preserved_and_never_regenerated() {
    for mode in ["binding", "edge", "certificate", "material"] {
        let f = Fixture::new();
        f.prepare().unwrap();
        let before = fs::read(f.material().join("config.json")).unwrap();
        let path = match mode {
            "binding" => f.binding.join("binding.json"),
            "edge" => f.edge.join("config.json"),
            "certificate" => f.edge.join("tls/cert.der"),
            _ => f.material().join("config.json"),
        };
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' ');
        fs::write(&path, &bytes).unwrap();
        assert!(f.prepare().is_err(), "{mode}");
        assert_eq!(bytes, fs::read(&path).unwrap());
        if mode != "material" {
            assert_eq!(before, fs::read(f.material().join("config.json")).unwrap());
        }
    }
}
#[test]
fn incomplete_pending_links_and_weak_metadata_are_not_replaced_or_accepted() {
    for mode in ["pending", "symlink", "weak"] {
        let f = Fixture::new();
        match mode {
            "pending" => {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(f.parent.join("installation.pending"))
                    .unwrap();
            }
            "symlink" => {
                std::os::unix::fs::symlink(
                    f._base.path().join("untouched"),
                    f.parent.join("installation"),
                )
                .unwrap();
            }
            _ => fs::set_permissions(
                f.binding.join("binding.json"),
                fs::Permissions::from_mode(0o644),
            )
            .unwrap(),
        }
        assert!(f.prepare().is_err());
        assert!(!f._base.path().join("untouched").exists());
    }
}
#[test]
fn complete_pending_publication_recovers_same_credentials_and_conflicts_fail_closed() {
    let f = Fixture::new();
    let first = f.prepare().unwrap();
    let before = fs::read(f.material().join("config.json")).unwrap();
    fs::rename(f.material(), f.parent.join("installation.pending")).unwrap();
    assert_eq!(first, f.prepare().unwrap());
    assert_eq!(before, fs::read(f.material().join("config.json")).unwrap());
    fs::DirBuilder::new()
        .mode(0o700)
        .create(f.parent.join("installation.pending"))
        .unwrap();
    assert!(f.prepare().is_err());
}
#[test]
fn a_coherent_but_wrong_edge_origin_never_becomes_importable_material() {
    let f = Fixture::new();
    let config = f.edge.join("config.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["origin"] = "https://192.168.1.2:8443".into();
    let bytes = serde_json::to_vec(&value).unwrap();
    fs::write(&config, &bytes).unwrap();
    let record = f.edge.join("record.json");
    let mut r: serde_json::Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
    r["config_sha256"] = hash(&bytes).into();
    fs::write(&record, serde_json::to_vec(&r).unwrap()).unwrap();
    assert!(f.prepare().is_err());
    assert!(!f.material().exists());
}
#[test]
fn export_refuses_changed_material_before_creating_a_secret_copy() {
    let f = Fixture::new();
    f.prepare().unwrap();
    let config = f.material().join("config.json");
    let mut bytes = fs::read(&config).unwrap();
    bytes.push(b' ');
    fs::write(&config, &bytes).unwrap();
    let output = f.parent.join("must-not-exist");
    assert!(export_owned(&f.material(), &output, f.uid).is_err());
    assert!(!output.exists());
}
