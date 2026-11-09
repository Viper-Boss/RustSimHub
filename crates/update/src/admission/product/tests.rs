// SPDX-License-Identifier: AGPL-3.0-only
//! Disposable container only: real production Core/launcher/worker, signatures,
//! installer, original Store migrations/Auth and health; synthetic MM/PID1.
use super::*;
use crate::{manifest_hash, signing_bytes, Entry, Manifest, BOARD};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
};
const DATA: &str = "/var/lib/rustsimhub/rustsimhub-v1/core/state";
const IDENTITY: &str = "/var/lib/rustsimhub/rustsimhub-v1/core/identity";
fn isolated(uid: u32) {
    assert_eq!(rustix::process::geteuid().as_raw(), uid);
    assert_eq!(
        fs::read("/fixture/WEB4_ISOLATED").unwrap(),
        b"private-production-wiring-only\n"
    );
    assert_eq!(
        std::env::var("RSH_WEB4_PRIVATE_CONTAINER").as_deref(),
        Ok("yes")
    );
}
fn mkdir(path: impl AsRef<Path>, uid: u32, gid: u32, mode: u32) {
    fs::create_dir_all(path.as_ref()).unwrap();
    fs::set_permissions(path.as_ref(), fs::Permissions::from_mode(mode)).unwrap();
    rustix::fs::chown(
        path.as_ref(),
        Some(rustix::process::Uid::from_raw(uid)),
        Some(rustix::process::Gid::from_raw(gid)),
    )
    .unwrap();
}
fn put(path: impl AsRef<Path>, bytes: &[u8], mode: u32) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn child(name: &str, uid: u32) -> std::process::Child {
    Command::new("/fixture/setpriv")
        .args(["--bounding-set=-all", "--clear-groups", "--no-new-privs"])
        .arg(format!("--reuid={uid}"))
        .arg(format!("--regid={uid}"))
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            name,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .spawn()
        .unwrap()
}
#[test]
#[ignore = "private UID986 setup child using original Auth bootstrap and Store"]
fn prepare_owner() {
    isolated(986);
    let store = rustsimhub_storage::Store::open(DATA).unwrap();
    let mut auth = rustsimhub_auth::Auth::open(IDENTITY, store.handle()).unwrap();
    let key = SigningKey::from_bytes(&[73; 32]);
    let challenge = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
        .unwrap();
    drop(auth);
    drop(store);
}
struct Modem;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem")]
impl Modem {
    #[zbus(property)]
    fn sim(&self) -> zbus::zvariant::OwnedObjectPath {
        "/org/freedesktop/ModemManager1/SIM/0".try_into().unwrap()
    }
    #[zbus(property)]
    fn state(&self) -> i32 {
        8
    }
    #[zbus(property)]
    fn unlock_required(&self) -> u32 {
        1
    }
}
struct Sim;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Sim")]
impl Sim {
    #[zbus(property)]
    fn sim_identifier(&self) -> &str {
        "8900000000000000001"
    }
}
struct Messaging;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Messaging")]
impl Messaging {
    fn list(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
        vec![]
    }
}
struct Voice;
#[zbus::interface(name = "org.freedesktop.ModemManager1.Modem.Voice")]
impl Voice {
    fn list_calls(&self) -> Vec<zbus::zvariant::OwnedObjectPath> {
        vec![]
    }
}
#[test]
#[ignore = "private synthetic MM on an owned offline D-Bus, never the host bus"]
fn private_mm_bus() {
    isolated(0);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let _connection =
                zbus::connection::Builder::address("unix:path=/run/dbus/system_bus_socket")
                    .unwrap()
                    .name("org.freedesktop.ModemManager1")
                    .unwrap()
                    .serve_at("/org/freedesktop/ModemManager1", zbus::fdo::ObjectManager)
                    .unwrap()
                    .serve_at("/org/freedesktop/ModemManager1/Modem/0", Modem)
                    .unwrap()
                    .serve_at("/org/freedesktop/ModemManager1/Modem/0", Messaging)
                    .unwrap()
                    .serve_at("/org/freedesktop/ModemManager1/Modem/0", Voice)
                    .unwrap()
                    .serve_at("/org/freedesktop/ModemManager1/SIM/0", Sim)
                    .unwrap()
                    .build()
                    .await
                    .unwrap();
            put("/fixture/mm-ready", b"ready", 0o600);
            tokio::time::sleep(Duration::from_secs(120)).await;
        });
}
fn install_base() {
    mkdir("/run/rustsimhub", 0, 0, 0o755);
    mkdir("/run/rustsimhub/core", 986, 987, 0o2750);
    mkdir("/run/rustsimhub/media", 984, 987, 0o2750);
    mkdir("/run/rustsimhub-app", 0, 0, 0o755);
    for name in [
        "state", "identity", "archive", "access", "compat", "journal", "setup",
    ] {
        mkdir(
            Path::new("/var/lib/rustsimhub/rustsimhub-v1/core").join(name),
            986,
            986,
            0o700,
        );
    }
    let physical = fs::metadata("/var/lib/rustsimhub/rustsimhub-v1/core").unwrap();
    let alias = fs::symlink_metadata("/var/lib/rustsimhub-core").unwrap();
    assert!(alias.is_dir());
    use std::os::unix::fs::MetadataExt;
    assert_eq!(
        (physical.dev(), physical.ino()),
        (alias.dev(), alias.ino()),
        "actual production state bind is required"
    );
    symlink("/run/rustsimhub-app/selected", "/opt/rustsimhub/selected").unwrap();
    put(Path::new("/").join(BASE_PATH), BASE.as_bytes(), 0o644);
    for material in forwarding() {
        put(
            Path::new("/").join(material.path),
            &material.expected.unwrap(),
            material.mode,
        );
    }
    assert!(child("admission::product::tests::prepare_owner", 986)
        .wait()
        .unwrap()
        .success());
}
fn applications(key: &SigningKey) -> (Vec<u8>, Vec<u8>, Vec<Vec<u8>>) {
    let payloads = crate::FORWARDING_FILES
        .iter()
        .map(|name| fs::read(Path::new("/fixture/programs").join(name)).unwrap())
        .collect::<Vec<_>>();
    let source = rustsimhub_wire::build_source_sha256().unwrap();
    let mut store = Store::open(Path::new(APP), key.verifying_key().to_bytes(), 6).unwrap();
    let mut result = None;
    for epoch in [1, 2] {
        let manifest = Manifest {
            format: 2,
            kind: "application".into(),
            board: BOARD.into(),
            arch: "aarch64-linux".into(),
            epoch,
            schema_min: 6,
            schema_max: 9,
            source_sha256: source.into(),
            development_fixtures: false,
            files: crate::FORWARDING_FILES
                .iter()
                .zip(&payloads)
                .map(|(name, p)| Entry {
                    name: (*name).into(),
                    bytes: p.len() as u64,
                    sha256: manifest_hash(p),
                })
                .collect(),
        };
        let raw = serde_json::to_vec(&manifest).unwrap();
        let signature = key.sign(&signing_bytes(&raw).unwrap()).to_bytes().to_vec();
        if epoch == 1 {
            store.install_factory(&raw, &signature, &payloads).unwrap();
        } else {
            mkdir(CANDIDATE, 0, 0, 0o700);
            put(Path::new(CANDIDATE).join("manifest.json"), &raw, 0o600);
            put(Path::new(CANDIDATE).join("manifest.sig"), &signature, 0o600);
            for (name, p) in crate::FORWARDING_FILES.iter().zip(&payloads) {
                put(Path::new(CANDIDATE).join(name), p, 0o600);
            }
            result = Some((raw, signature, payloads.clone()));
        }
    }
    result.unwrap()
}
fn package(key: &SigningKey, application: &[u8], payloads: &[Vec<u8>]) {
    use materials::{MaterialEntry, TargetId};
    let source = rustsimhub_wire::build_source_sha256().unwrap();
    let expected = ExpectedBindings {
        profile: Profile::RecoveryHealth,
        source_commit: fs::read_to_string("/fixture/source-commit")
            .unwrap()
            .trim()
            .into(),
        source_archive_sha256: source.into(),
        application_manifest_sha256: manifest_hash(application),
        core_sha256: manifest_hash(&payloads[0]),
        approved_role_policy_sha256: manifest_hash(
            &fs::read("/etc/rustsimhub/product-roles.json").unwrap(),
        ),
    };
    let blob = |id: TargetId| -> Vec<u8> {
        match id {
        TargetId::ProfileConfig=>serde_json::to_vec(&serde_json::json!({"format":1,"profile":"product-recovery-health-v1","source_archive_sha256":source,"core_sha256":expected.core_sha256})).unwrap(),
        TargetId::CoreDropin=>include_str!("../../../../../firmware/system/product-profiles/50-product-profile.conf").replace("@CORE_MEMORY@","192M").replace("@CORE_TASKS@","64").into_bytes(),
        TargetId::Launcher=>fs::read("/fixture/programs/rustsimhub-core-launcher").unwrap(),
        // Actual production provider bytes are installed but remain OFF.
        TargetId::NknRoot|TargetId::NknSdk=>fs::read("/fixture/idle").unwrap(),
        TargetId::BleHost=>fs::read("/fixture/programs/rustsimhub-ble-recovery").unwrap(),
        TargetId::NknUnit=>include_bytes!("../../../../../firmware/system/product-profiles/rustsimhub-nkn-root.service").to_vec(),
        TargetId::ProductTmpfiles=>include_str!("../../../../../firmware/system/product-profiles/product.tmpfiles.in").replace("@NKN_GID@","1201").replace("@BLE_GID@","1202").into_bytes(),
        TargetId::BleUnit=>include_bytes!("../../../../../firmware/system/product-profiles/rustsimhub-ble-recovery.service").to_vec(),
        TargetId::BlePolicy=>include_bytes!("../../../../../firmware/system/product-profiles/ble-product.policy").to_vec(),
        TargetId::HealthDropin=>include_bytes!("../../../../../firmware/system/product-profiles/55-product-health.conf").to_vec(),
        TargetId::HealthTmpfiles=>include_bytes!("../../../../../firmware/system/update-health/tmpfiles.conf").to_vec(),
        TargetId::HealthService=>include_bytes!("../../../../../firmware/system/update-health/rustsimhub-update-health.service").to_vec(),
        TargetId::HealthTimer=>include_bytes!("../../../../../firmware/system/update-health/rustsimhub-update-health.timer").to_vec(),
    }
    };
    mkdir(INBOX, 0, 0, 0o700);
    let mut entries = vec![];
    for spec in Profile::RecoveryHealth.targets() {
        let raw = blob(spec.id);
        entries.push(MaterialEntry {
            id: spec.id,
            bytes: raw.len() as u64,
            sha256: manifest_hash(&raw),
        });
        put(Path::new(INBOX).join(spec.id.payload_name()), &raw, 0o600);
    }
    let manifest = ProductManifestV1 {
        format: 1,
        kind: "product-maintenance".into(),
        board: BOARD.into(),
        arch: "aarch64-linux".into(),
        bindings: expected,
        roles: Profile::RecoveryHealth.required_roles().to_vec(),
        files: entries,
    };
    let raw = manifest.encode().unwrap();
    let signature = key
        .sign(&materials::product_signing_bytes(&raw).unwrap())
        .to_bytes();
    put(Path::new(INBOX).join("manifest.json"), &raw, 0o600);
    put(Path::new(INBOX).join("manifest.sig"), &signature, 0o600);
}
#[test]
#[ignore = "disposable Root-only production wiring E2E; native arch, MM/PID1/cgroup adapters"]
fn production_chain() {
    isolated(0);
    crate::product_native_fixture::enable();
    install_base();
    let key = SigningKey::from_bytes(&[71; 32]);
    put(TRUST, &key.verifying_key().to_bytes(), 0o444);
    let (app, _, payloads) = applications(&key);
    package(&key, &app, &payloads);
    for command in ["product-prepare", "product-apply", "product-inspect"] {
        let result = dispatch(&[command.into()]);
        eprintln!("{command}: {result:?}");
        assert!(result.is_ok());
    }
    let before = fs::read(Path::new(APP).join("state.json")).unwrap();
    let report = dispatch(&["product-activate".into()]);
    eprintln!("product-activate: {report:?}");
    assert!(report.is_ok());
    assert_ne!(fs::read(Path::new(APP).join("state.json")).unwrap(), before);
    assert!(dispatch(&["product-activate".into()]).is_err());
    // Actual Core performs MM reconciliation; no always-ready source adapter.
    std::thread::sleep(Duration::from_secs(3));
    let cap = crate::runtime_observation::confirmation::RunningCandidate::observe().unwrap();
    let result = cap.confirm().unwrap();
    eprintln!("confirmed: {}", serde_json::to_string(&result).unwrap());
    assert!(crate::runtime_observation::confirmation::RunningCandidate::observe().is_err());
    let ledger = crate::store::inspection::ReadOnlyState::observe()
        .unwrap()
        .report()
        .unwrap();
    assert_eq!(ledger["state"]["floor"], 2);
    assert_eq!(ledger["state"]["attempted"], false);
    assert!(ledger["state"]["candidate"].is_null());
    assert!(Command::new("/usr/bin/systemctl")
        .args([
            "--system",
            "stop",
            "rustsimhub-core.service",
            "rustsimhub-edge.service",
            "rustsimhub-media.service"
        ])
        .status()
        .unwrap()
        .success());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        crate::product_recovery_child::inspect().unwrap()["actual_schema"],
        9
    );
    put(
        "/fixture/production-chain-passed",
        b"actual-core-launcher-auth-store-migration-health\n",
        0o600,
    );
}
