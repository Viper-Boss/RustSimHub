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
            tokio::time::sleep(Duration::from_secs(300)).await;
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
            schema_max: if matches!(
                fs::read_to_string("/fixture/scenario").unwrap().trim(),
                "default" | "email"
            ) {
                6
            } else {
                9
            },
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
        profile: match fs::read_to_string("/fixture/scenario").unwrap().trim() {
            "email" => Profile::Email,
            "email-recovery" => Profile::EmailRecovery,
            _ => Profile::RecoveryHealth,
        },
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
        TargetId::ProfileConfig=>serde_json::to_vec(&serde_json::json!({"format":1,"profile":expected.profile.as_str(),"source_archive_sha256":source,"core_sha256":expected.core_sha256})).unwrap(),
        TargetId::CoreDropin=>include_str!("../../../../../firmware/system/product-profiles/50-product-profile.conf").replace("@CORE_MEMORY@","192M").replace("@CORE_TASKS@","64").into_bytes(),
        TargetId::Launcher=>fs::read("/fixture/programs/rustsimhub-core-launcher").unwrap(),
        // Actual production provider bytes are installed but remain OFF.
        TargetId::NknRoot=>fs::read("/fixture/programs/nkn-root").unwrap(),
        TargetId::NknSdk=>fs::read("/fixture/programs/nkn-sdk-engine").unwrap(),
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
    for spec in expected.profile.targets() {
        let raw = blob(spec.id);
        entries.push(MaterialEntry {
            id: spec.id,
            bytes: raw.len() as u64,
            sha256: manifest_hash(&raw),
        });
        put(Path::new(INBOX).join(spec.id.payload_name()), &raw, 0o600);
    }
    let roles = expected.profile.required_roles().to_vec();
    let manifest = ProductManifestV1 {
        format: 1,
        kind: "product-maintenance".into(),
        board: BOARD.into(),
        arch: "aarch64-linux".into(),
        bindings: expected,
        roles,
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
#[ignore = "private UID985 Owner login to inspect actual synthetic-MM reconciliation"]
fn inspect_business_readiness() {
    isolated(985);
    fn request(command: serde_json::Value) -> serde_json::Value {
        use std::os::unix::net::UnixStream;
        let mut stream = UnixStream::connect("/run/rustsimhub/core/core.sock").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let raw = serde_json::to_vec(
            &serde_json::json!({"protocol":2,"id":"private-readiness","command":command}),
        )
        .unwrap();
        stream.write_all(&(raw.len() as u32).to_be_bytes()).unwrap();
        stream.write_all(&raw).unwrap();
        let mut header = [0; 4];
        stream.read_exact(&mut header).unwrap();
        let size = u32::from_be_bytes(header) as usize;
        assert!(size <= 65536);
        let mut body = vec![0; size];
        stream.read_exact(&mut body).unwrap();
        let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            response["result"].get("error").is_none(),
            "actual fixture Owner request refused"
        );
        response["result"].clone()
    }
    let key = SigningKey::from_bytes(&[73; 32]);
    let raw =
        request(serde_json::json!({"op":"challenge","public_key":key.verifying_key().to_bytes()}));
    let challenge: rustsimhub_auth::Challenge = serde_json::from_value(raw).unwrap();
    let token = request(
        serde_json::json!({"op":"authenticate","challenge_id":challenge.id,"signature":key.sign(&challenge.transcript).to_bytes().to_vec()}),
    );
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let line = request(serde_json::json!({"op":"line_status","token":token["token"]}));
        let metrics = &line["observer"];
        if metrics["subscriptions"] == true
            && metrics["reconciliations"].as_u64().is_some_and(|n| n > 0)
            && metrics["storage_errors"] == 0
        {
            eprintln!("actual private observer metrics: {metrics}");
            break;
        }
        assert!(
            Instant::now() < until,
            "actual MM reconciliation unavailable: {metrics}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn actual_core_ready() {
    use std::os::unix::net::UnixStream;
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let observed = (|| -> std::result::Result<bool, Box<dyn std::error::Error>> {
            let mut stream = UnixStream::connect(rustsimhub_wire::update_health::SOCKET)?;
            stream.set_read_timeout(Some(Duration::from_millis(250)))?;
            stream.set_write_timeout(Some(Duration::from_millis(250)))?;
            let mut nonce = [0u8; 32];
            getrandom::fill(&mut nonce)?;
            let request = serde_json::to_vec(&rustsimhub_wire::update_health::Request {
                protocol: 1,
                step: rustsimhub_wire::update_health::Step::Observe,
                challenge: nonce.iter().map(|b| format!("{b:02x}")).collect(),
                connection: None,
            })?;
            stream.write_all(&(request.len() as u32).to_be_bytes())?;
            stream.write_all(&request)?;
            let mut header = [0; 4];
            stream.read_exact(&mut header)?;
            let length = u32::from_be_bytes(header) as usize;
            if length == 0 || length > rustsimhub_wire::update_health::MAX_FRAME {
                return Ok(false);
            }
            let mut body = vec![0; length];
            stream.read_exact(&mut body)?;
            let snapshot = rustsimhub_wire::update_health::parse_snapshot(&body)?;
            Ok(snapshot.business_ready)
            // Closing releases this separate read lease. It is never a Root
            // confirmation capability and never advances the candidate ledger.
        })();
        if matches!(observed, Ok(true)) {
            std::thread::sleep(Duration::from_millis(100));
            return;
        }
        assert!(
            Instant::now() < until,
            "actual Core did not produce readiness: {observed:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn preflight_refusals() {
    let data = fs::read(Path::new(DATA).join("store.sqlite3")).unwrap();
    let state = fs::read(Path::new(APP).join("state.json")).unwrap();
    let refused = |name| {
        assert!(dispatch(&["product-prepare".into()]).is_err(), "{name}");
        assert_eq!(
            fs::read(Path::new(DATA).join("store.sqlite3")).unwrap(),
            data
        );
        assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), state);
        eprintln!("actual production preflight rejection: {name}");
    };
    for (name, path) in [
        ("signature", Path::new(CANDIDATE).join("manifest.sig")),
        (
            "program bytes",
            Path::new(CANDIDATE).join("rustsimhub-core"),
        ),
        ("installed trust", Path::new(TRUST).to_path_buf()),
    ] {
        let bytes = fs::read(&path).unwrap();
        let mut changed = bytes.clone();
        *changed.last_mut().unwrap() ^= 1;
        fs::write(&path, changed).unwrap();
        refused(name);
        fs::write(&path, bytes).unwrap();
    }
    let path = Path::new("/etc/group");
    let bytes = fs::read_to_string(path).unwrap();
    fs::write(
        path,
        bytes.replace("rustsimhub-nkn:x:1201:", "rustsimhub-nkn:x:1201:root"),
    )
    .unwrap();
    refused("actual NSS supplementary alias");
    fs::write(path, bytes).unwrap();
    for (name, path) in [
        (
            "maintenance competitor",
            Path::new(APP).join(".update.lock"),
        ),
        (
            "business writer competitor",
            Path::new(DATA).join(".writer.lock"),
        ),
    ] {
        let held = File::open(path).unwrap();
        fs2::FileExt::try_lock_exclusive(&held).unwrap();
        refused(name);
    }
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
    preflight_refusals();
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
    inspect_actual_business();
    actual_core_ready();
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
    let stopped = dispatch(&["product-inspect".into()]).unwrap();
    assert_eq!(
        stopped["data_admission"]["comparison"]["actual_data"]["business_schema"],
        9
    );
    schema9_backup();
    put(
        "/fixture/production-chain-passed",
        b"actual-core-launcher-auth-store-migration-health\n",
        0o600,
    );
}

#[test]
#[ignore = "private Root selection-generation guard with real file replacement"]
fn selection_generation_rejects_later_equal_byte_inode() {
    isolated(0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("rustsimhub-core");
    let bytes = b"retained signed payload fixture";
    put(&path, bytes, 0o755);
    let held = HeldFile::capture(&path, 1024).unwrap();
    let entry = Entry {
        name: "rustsimhub-core".into(),
        bytes: bytes.len() as u64,
        sha256: manifest_hash(bytes),
    };
    fs::set_permissions(&path, fs::Permissions::from_mode(0o555)).unwrap();
    let selected = held.selected_generation(&entry).unwrap();
    selected.check().unwrap();
    let replacement = directory.path().join("next");
    put(&replacement, bytes, 0o555);
    fs::rename(replacement, &path).unwrap();
    assert!(selected.check().is_err());
    assert!(held.selected_generation(&entry).is_err());
}

#[test]
#[ignore = "private Root factory input generation; subsequent actions use formal CLI"]
fn prepare_packages() {
    isolated(0);
    crate::product_native_fixture::enable();
    install_base();
    let key = SigningKey::from_bytes(&[71; 32]);
    put(TRUST, &key.verifying_key().to_bytes(), 0o444);
    let (app, _, payloads) = applications(&key);
    package(&key, &app, &payloads);
}

fn inspect_actual_business() {
    assert!(Command::new("/fixture/setpriv")
        .args([
            "--bounding-set=-all",
            "--no-new-privs",
            "--reuid=985",
            "--regid=985",
            "--groups=987"
        ])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "admission::product::tests::inspect_business_readiness",
            "--ignored",
            "--nocapture",
            "--test-threads=1"
        ])
        .status()
        .unwrap()
        .success());
}
fn default_factory() {
    isolated(0);
    crate::product_native_fixture::enable();
    install_base();
    let key = SigningKey::from_bytes(&[71; 32]);
    put(TRUST, &key.verifying_key().to_bytes(), 0o444);
    applications(&key);
    put(
        "/usr/libexec/rustsimhub/rustsimhub-core-launcher",
        &fs::read("/fixture/programs/rustsimhub-core-launcher").unwrap(),
        0o555,
    );
}
#[test]
#[ignore = "private factory only for the unchanged formal ARM boot-select path"]
fn prepare_default() {
    default_factory();
}
#[test]
#[ignore = "isolated default production Core/launcher/Auth/Store, no product activation"]
fn default_chain() {
    default_factory();
    let key = SigningKey::from_bytes(&[71; 32]);
    {
        let mut store = Store::open(Path::new(APP), key.verifying_key().to_bytes(), 6).unwrap();
        store
            .select_runtime(
                Path::new("/run/rustsimhub-app"),
                Path::new("/opt/rustsimhub/versions"),
            )
            .unwrap();
    }
    assert!(services::activate_business().is_ok());
    inspect_actual_business();
    assert!(!Path::new("/etc/rustsimhub/product-profile.json").exists());
    assert!(!Path::new(rustsimhub_wire::update_health::SOCKET).exists());
    let bytes = fs::read(Path::new(DATA).join("store.sqlite3")).unwrap();
    assert_eq!(u32::from_be_bytes(bytes[60..64].try_into().unwrap()), 6);
    let ledger = crate::store::inspection::ReadOnlyState::observe()
        .unwrap()
        .report()
        .unwrap();
    assert_eq!(ledger["state"]["floor"], 1);
    assert_eq!(ledger["state"]["attempted"], false);
    assert!(ledger["state"]["candidate"].is_null());
    put(
        "/fixture/default-chain-passed",
        b"unchanged-core-auth-store-no-migration\n",
        0o600,
    );
}

#[test]
#[ignore = "private stopped schema9 fixture; real consistent backup reused without a migration request"]
fn schema9_backup() {
    isolated(0);
    let root = Root::production().unwrap();
    let transaction = "f".repeat(64);
    root.ensure_dir(&format!("{BACKUPS}/{transaction}"), 0o700)
        .unwrap();
    let snapshot = MaintenanceSnapshot::observe_qualified(Path::new("/"), 0, 986, true).unwrap();
    assert_eq!(snapshot.business.version.format, 9);
    let mut held = vec![];
    backup(&snapshot, &root, &transaction, &mut held, &mut None).unwrap();
    // This fresh plan has no recovery-child request with this ID; its existing
    // backup is the actual current9 snapshot, not a stale pre-migration file.
    backup(&snapshot, &root, &transaction, &mut held, &mut None).unwrap();
    for file in held {
        file.check().unwrap();
    }
    snapshot.revalidate().unwrap();
}

#[cfg(feature = "email-production-candidate")]
#[test]
#[ignore = "private real SMTP5/format3 original writer and retained migration chain"]
fn email_chain() {
    email_chain_impl(false);
}

#[cfg(feature = "email-production-candidate")]
#[test]
#[ignore = "private EmailRecovery real email4 then recovery9, interrupted retained writers and explicit recovery"]
fn email_recovery_chain() {
    email_chain_impl(true);
}

#[cfg(feature = "email-production-candidate")]
pub(super) fn after_email_boundary(snapshot: &MaintenanceSnapshot) {
    if !Path::new("/fixture/stop-after-email").exists() {
        return;
    }
    isolated(0);
    assert_eq!(
        fs::read_to_string("/fixture/scenario").unwrap().trim(),
        "email-recovery"
    );
    snapshot.revalidate().unwrap();
    assert_eq!(snapshot.business.version.format, 6);
    assert_eq!(snapshot.outbox.as_ref().unwrap().version.format, 4);
    put(
        "/fixture/email-boundary-held",
        b"same-original-snapshot-writers-held",
        0o600,
    );
    rustix::process::kill_process(rustix::process::getpid(), rustix::process::Signal::STOP)
        .unwrap();
}

#[cfg(feature = "email-production-candidate")]
#[test]
#[ignore = "private deterministic interrupted product apply child; no new migration implementation"]
fn email_recovery_apply_child() {
    isolated(0);
    crate::product_native_fixture::enable();
    dispatch(&["product-apply".into()]).unwrap();
}

#[cfg(not(feature = "email-production-candidate"))]
#[test]
#[ignore = "private real default-updater feature refusal against signed SMTP5 EmailRecovery inputs"]
fn email_feature_rejection() {
    isolated(0);
    crate::product_native_fixture::enable();
    assert!(matches!(
        dispatch(&["product-prepare".into()]),
        Err(Error::Denied)
    ));
}

#[cfg(feature = "email-production-candidate")]
fn email_chain_impl(recovery: bool) {
    isolated(0);
    crate::product_native_fixture::enable();
    install_base();
    mkdir("/run/rustsimhub-forwarding", 0, 0, 0o700);
    mkdir(
        "/var/lib/rustsimhub/rustsimhub-v1/system/forwarding",
        0,
        0,
        0o700,
    );
    assert!(Command::new("/fixture/setpriv")
        .args([
            "--bounding-set=-all",
            "--clear-groups",
            "--no-new-privs",
            "--reuid=986",
            "--regid=986",
            "/fixture/legacy-outbox"
        ])
        .status()
        .unwrap()
        .success());
    let receipts = recovery.then(matrix_outbox_receipts);
    if let Some(receipts) = &receipts {
        assert_eq!(receipts.len(), 3);
        assert_eq!(
            receipts.iter().map(|r| r.1.as_str()).collect::<Vec<_>>(),
            ["confirmed", "unknown", "unknown"]
        );
        assert!(receipts.iter().all(|r| r.2.is_some()));
    }
    let key = SigningKey::from_bytes(&[71; 32]);
    put(TRUST, &key.verifying_key().to_bytes(), 0o444);
    let (app, signature, payloads) = applications(&key);
    {
        let mut store = Store::open(Path::new(APP), key.verifying_key().to_bytes(), 6).unwrap();
        store
            .select_runtime(
                Path::new("/run/rustsimhub-app"),
                Path::new("/opt/rustsimhub/versions"),
            )
            .unwrap();
    }
    package(&key, &app, &payloads);
    if recovery {
        preflight_refusals();
        email_recovery_signed_refusals(&key, &app, &signature, &payloads);
        let db = rusqlite::Connection::open(Path::new(DATA).join("store.sqlite3")).unwrap();
        let owner: (String, i64) = db
            .query_row(
                "SELECT id,generation FROM principals WHERE kind='owner'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // Explicit SQL revocation fixture, not an Auth RPC or real Owner.
        db.execute(
            "UPDATE principals SET active=0,generation=generation+1 WHERE id=?",
            [&owner.0],
        )
        .unwrap();
        drop(db);
        assert!(matches!(
            dispatch(&["product-prepare".into()]),
            Err(Error::Denied)
        ));
        assert!(!Path::new("/etc/rustsimhub/product-profile.json").exists());
        assert!(!Path::new(INSTALL).join("active.json").exists());
        let db = rusqlite::Connection::open(Path::new(DATA).join("store.sqlite3")).unwrap();
        db.execute(
            "UPDATE principals SET active=1,generation=? WHERE id=?",
            rusqlite::params![owner.1, &owner.0],
        )
        .unwrap();
        drop(db);
        eprintln!(
            "actual original binding refusal: revoked Owner SQL fixture before Root preparation"
        );
        let before = fs::read(Path::new(APP).join("state.json")).unwrap();
        assert!(Command::new("/fixture/no-email-tests")
            .args([
                "--exact",
                "admission::product::tests::email_feature_rejection",
                "--ignored",
                "--nocapture",
                "--test-threads=1"
            ])
            .status()
            .unwrap()
            .success());
        assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), before);
    }
    assert!(dispatch(&["product-prepare".into()]).is_ok());
    if recovery {
        interrupted_email_recovery();
    } else {
        assert!(dispatch(&["product-apply".into()]).is_ok());
    }
    let report = dispatch(&["product-inspect".into()]).unwrap();
    assert_eq!(
        report["data_admission"]["comparison"]["actual_data"]["business_schema"],
        if recovery { 9 } else { 6 }
    );
    {
        let snapshot =
            MaintenanceSnapshot::observe_qualified(Path::new("/"), 0, 986, true).unwrap();
        assert_eq!(
            snapshot.business.version.format,
            if recovery { 9 } else { 6 }
        );
        assert_eq!(snapshot.outbox.as_ref().unwrap().version.format, 4);
        let before = fs::read(Path::new(DATA).join("store.sqlite3")).unwrap();
        let backup = coordinator::UpgradeJournal::production(coordinator::UpgradeKind::Email4)
            .unwrap()
            .completed_email()
            .unwrap();
        backup.revalidate().unwrap();
        assert!(rustsimhub_storage::Store::open(DATA).is_err());
        assert_eq!(
            fs::read(Path::new(DATA).join("store.sqlite3")).unwrap(),
            before
        );
    }
    if recovery {
        assert_eq!(matrix_outbox_receipts(), *receipts.as_ref().unwrap());
        let committed =
            coordinator::UpgradeJournal::production(coordinator::UpgradeKind::Recovery9)
                .unwrap()
                .completed()
                .unwrap();
        committed.revalidate().unwrap();
        // Do not let this diagnostic mutex mask an unsupported-mode refusal.
        drop(committed);
        let root_profile = fs::read("/etc/rustsimhub/product-profile.json").unwrap();
        let ledger_before_rollback = fs::read(Path::new(APP).join("state.json")).unwrap();
        let data_before_rollback = fs::read(Path::new(DATA).join("store.sqlite3")).unwrap();
        assert!(
            matches!(dispatch(&["product-rollback".into()]), Err(Error::Denied)),
            "schema9 cannot revert Root to an unproven schema6 launch profile"
        );
        assert_eq!(
            fs::read("/etc/rustsimhub/product-profile.json").unwrap(),
            root_profile
        );
        assert_eq!(
            fs::read(Path::new(APP).join("state.json")).unwrap(),
            ledger_before_rollback
        );
        assert_eq!(
            fs::read(Path::new(DATA).join("store.sqlite3")).unwrap(),
            data_before_rollback
        );
        assert_eq!(matrix_outbox_receipts(), *receipts.as_ref().unwrap());
        assert!(
            matches!(dispatch(&["product-activate".into()]), Err(Error::Denied)),
            "SMTP5 automatic health activation remains unsupported"
        );
        // The existing explicit boot selector may run the supported mode, but
        // it does not gain a confirmation receipt or an automatic-health prefix.
        let mut store = Store::open(Path::new(APP), key.verifying_key().to_bytes(), 9).unwrap();
        store
            .select_runtime(
                Path::new("/run/rustsimhub-app"),
                Path::new("/opt/rustsimhub/versions"),
            )
            .unwrap();
        drop(store);
        assert!(services::activate_business().is_ok());
        inspect_actual_business();
        assert_eq!(matrix_outbox_receipts(), *receipts.as_ref().unwrap());
        assert!(!Path::new(rustsimhub_wire::update_health::SOCKET).exists());
        let before = fs::read(Path::new(APP).join("state.json")).unwrap();
        assert!(crate::runtime_observation::confirmation::RunningCandidate::observe().is_err());
        assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), before);
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
        assert_eq!(
            crate::product_recovery_child::inspect().unwrap()["actual_schema"],
            9
        );
        let ledger = crate::store::inspection::ReadOnlyState::observe()
            .unwrap()
            .report()
            .unwrap();
        assert_eq!(ledger["state"]["floor"], 2);
        assert_eq!(ledger["state"]["attempted"], true);
        assert!(ledger["state"]["candidate"].is_object());
        put(
            "/fixture/email-recovery-chain-passed",
            b"original-email4-then-recovery9-explicit-boot-without-auto-confirm",
            0o600,
        );
    } else {
        put(
            "/fixture/email-chain-passed",
            b"actual-retained-email4-and-reopen\n",
            0o600,
        );
    }
}

#[cfg(feature = "email-production-candidate")]
fn email_recovery_signed_refusals(
    key: &SigningKey,
    original: &[u8],
    signature: &[u8],
    payloads: &[Vec<u8>],
) {
    let database = fs::read(Path::new(DATA).join("store.sqlite3")).unwrap();
    let outbox = fs::read(Path::new(DATA).join("forwarding/outbox.sqlite3")).unwrap();
    let state = fs::read(Path::new(APP).join("state.json")).unwrap();
    for index in [0, 4] {
        let mut changed = payloads.to_vec();
        if index == 0 {
            changed[0].push(0);
        } else {
            changed[4] = fs::read("/fixture/worker4").unwrap();
        }
        let mut manifest: Manifest = serde_json::from_slice(original).unwrap();
        manifest.files[index].bytes = changed[index].len() as u64;
        manifest.files[index].sha256 = manifest_hash(&changed[index]);
        let raw = serde_json::to_vec(&manifest).unwrap();
        let signed = key.sign(&signing_bytes(&raw).unwrap()).to_bytes();
        // These are genuinely valid signatures and executable payload groups;
        // rejection must come from the actual compatibility/fallback contract.
        crate::verify_payload(&manifest, &changed).unwrap();
        put(Path::new(CANDIDATE).join("manifest.json"), &raw, 0o600);
        put(Path::new(CANDIDATE).join("manifest.sig"), &signed, 0o600);
        put(
            Path::new(CANDIDATE).join(crate::FORWARDING_FILES[index]),
            &changed[index],
            0o600,
        );
        package(key, &raw, &changed);
        assert!(dispatch(&["product-prepare".into()]).is_err());
        assert_eq!(
            fs::read(Path::new(DATA).join("store.sqlite3")).unwrap(),
            database
        );
        assert_eq!(
            fs::read(Path::new(DATA).join("forwarding/outbox.sqlite3")).unwrap(),
            outbox
        );
        assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), state);
        eprintln!(
            "actual signed EmailRecovery refusal: {}",
            if index == 0 {
                "different fallback Core"
            } else {
                "worker4 with SMTP5 Core"
            }
        );
        put(Path::new(CANDIDATE).join("manifest.json"), original, 0o600);
        put(Path::new(CANDIDATE).join("manifest.sig"), signature, 0o600);
        put(
            Path::new(CANDIDATE).join(crate::FORWARDING_FILES[index]),
            &payloads[index],
            0o600,
        );
        package(key, original, payloads);
    }
}

#[cfg(feature = "email-production-candidate")]
fn interrupted_email_recovery() {
    use fs2::FileExt;
    put(
        "/fixture/stop-after-email",
        b"deterministic-test-boundary",
        0o600,
    );
    let mut process = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "admission::product::tests::email_recovery_apply_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        assert!(
            process.try_wait().unwrap().is_none(),
            "apply exited before deterministic boundary"
        );
        let stat = fs::read_to_string(format!("/proc/{}/stat", process.id())).unwrap();
        if Path::new("/fixture/email-boundary-held").exists()
            && stat.rsplit_once(") ").unwrap().1.starts_with('T')
        {
            break;
        }
        assert!(Instant::now() < deadline, "email boundary deadline");
        std::thread::yield_now();
    }
    for path in [
        Path::new(APP).join(".update.lock"),
        Path::new(DATA).join(".writer.lock"),
        Path::new(DATA).join("forwarding/.writer.lock"),
    ] {
        let file = File::open(path).unwrap();
        assert!(
            file.try_lock_exclusive().is_err(),
            "original writer was released at combined boundary"
        );
    }
    let email: serde_json::Value = serde_json::from_slice(
        &fs::read("/var/lib/rustsimhub/rustsimhub-v1/system/product-upgrade/email4/state.json")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(email["phase"], "committed");
    assert!(!Path::new(
        "/var/lib/rustsimhub/rustsimhub-v1/system/product-upgrade/recovery9/state.json"
    )
    .exists());
    process.kill().unwrap();
    assert!(!process.wait().unwrap().success());
    fs::remove_file("/fixture/stop-after-email").unwrap();
    let business = fs::read(Path::new(DATA).join("store.sqlite3")).unwrap();
    let outbox = fs::read(Path::new(DATA).join("forwarding/outbox.sqlite3")).unwrap();
    assert_eq!(u32::from_be_bytes(business[60..64].try_into().unwrap()), 6);
    assert_eq!(u32::from_be_bytes(outbox[60..64].try_into().unwrap()), 4);
    // Root is applied before the application slot is staged. The retained
    // transaction binds epoch2 while the only confirmed slot still names epoch1.
    let ledger_before = fs::read(Path::new(APP).join("state.json")).unwrap();
    let ledger: serde_json::Value = serde_json::from_slice(&ledger_before).unwrap();
    assert!(ledger["candidate"].is_null());
    let retained: ProductManifestV1 = serde_json::from_slice(
        &fs::read(source_directory(true).unwrap().join("manifest.json")).unwrap(),
    )
    .unwrap();
    let confirmed = ledger["confirmed"]["hash"].as_str().unwrap();
    assert_ne!(retained.bindings.application_manifest_sha256, confirmed);
    // Reopen is a NEW qualified maintenance operation over the original durable
    // journals. It is explicit, never a retry or a JSON-restored live capability.
    dispatch(&["product-inspect".into()]).expect("explicit interrupted inspect must reopen");
    assert_eq!(
        fs::read(Path::new(APP).join("state.json")).unwrap(),
        ledger_before
    );
    assert_eq!(
        fs::read(Path::new(DATA).join("store.sqlite3")).unwrap(),
        business
    );
    assert_eq!(
        fs::read(Path::new(DATA).join("forwarding/outbox.sqlite3")).unwrap(),
        outbox
    );
    assert!(dispatch(&["product-recover".into()]).is_ok());
}

#[cfg(feature = "email-production-candidate")]
fn matrix_outbox_receipts() -> Vec<(String, String, Option<String>)> {
    let db = rusqlite::Connection::open_with_flags(
        "file:/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding/outbox.sqlite3?immutable=1",
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .unwrap();
    let mut query = db
        .prepare("SELECT id,state,attempt_id FROM jobs ORDER BY seq")
        .unwrap();
    query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap()
}
