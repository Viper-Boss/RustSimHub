// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(target_os = "linux")]
#[allow(dead_code, unused_imports)]
#[path = "../src/product_materials/mod.rs"]
mod product_materials;
#[path = "product_materials_support/mod.rs"]
mod support;
use product_materials::*;
use std::{
    cell::{Cell, RefCell},
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    rc::Rc,
};
use support::{Bundle, FIXTURE_ROLE_POLICY};

struct Fence {
    path: PathBuf,
    digest: String,
    _lock: File,
    fail: Cell<bool>,
    phases: RefCell<Vec<FencePhase>>,
}
impl LiveInstallationFence for Fence {
    fn revalidate(&self, phase: FencePhase, _: &OwnedRootChanges<'_>) -> Result<()> {
        self.phases.borrow_mut().push(phase);
        if self.fail.get()
            || material_sha256(&fs::read(&self.path).map_err(|_| Error::Io)?) != self.digest
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
struct Fixture {
    root: tempfile::TempDir,
    bundle: Bundle,
    fence: Fence,
    services: Rc<IsolatedServiceState>,
}
impl Fixture {
    fn new(profile: Profile) -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let bundle = Bundle::new(profile);
        let froot = root.path();
        put(froot,"/etc/passwd",b"root:x:0:0:root:/root:/bin/sh\nrustsimhub-core:x:986:986::/nonexistent:/usr/sbin/nologin\nrustsimhub-edge:x:985:985::/nonexistent:/usr/sbin/nologin\nrustsimhub-media:x:984:984::/nonexistent:/usr/sbin/nologin\nrustsimhub-forwarder:x:983:983::/nonexistent:/usr/sbin/nologin\n",0o644);
        put(froot,"/etc/group",b"root:x:0:\nrustsimhub-core:x:986:\nrustsimhub-edge:x:985:\nrustsimhub-media:x:984:\nrustsimhub-forwarder:x:983:\naudio:x:29:rustsimhub-media\nrustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media,rustsimhub-forwarder\n",0o644);
        put(
            froot,
            "/etc/nsswitch.conf",
            b"passwd: files\ngroup: files\n",
            0o644,
        );
        put(
            froot,
            "/etc/rustsimhub/product-roles.json",
            FIXTURE_ROLE_POLICY,
            0o600,
        );
        ProductInstaller::prepare_isolated_accounts(froot).unwrap();
        for (name, bytes) in bundle.source_files() {
            put(
                froot,
                &format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox/{name}"),
                &bytes,
                0o600,
            );
        }
        for target in profile.targets() {
            put(
                froot,
                target.path,
                b"synthetic prior Root material",
                if target.executable { 0o755 } else { 0o644 },
            );
        }
        let data = actual(
            froot,
            "/var/lib/rustsimhub/rustsimhub-v1/core/state/store.sqlite3",
        );
        mkdirs(froot, data.parent().unwrap());
        let db = rusqlite::Connection::open(&data).unwrap();
        db.execute_batch("PRAGMA user_version=6;CREATE TABLE synthetic_private(value TEXT);INSERT INTO synthetic_private VALUES('fixture retained, never real business');").unwrap();
        drop(db);
        fs::set_permissions(&data, fs::Permissions::from_mode(0o600)).unwrap();
        put(
            froot,
            "/var/lib/rustsimhub/rustsimhub-v1/system/app/state.json",
            b"synthetic floor/candidate/attempted unchanged",
            0o600,
        );
        put(
            froot,
            "/var/lib/rustsimhub/rustsimhub-v1/system/recovery-budget",
            b"synthetic unused budget",
            0o600,
        );
        let writer = data.parent().unwrap().join(".writer.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(writer)
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        let fence = Fence {
            digest: material_sha256(&fs::read(&data).unwrap()),
            path: data,
            _lock: lock,
            fail: Cell::new(false),
            phases: RefCell::new(vec![]),
        };
        let services = Rc::new(IsolatedServiceState::default());
        services.off.set(true);
        Self {
            root,
            bundle,
            fence,
            services,
        }
    }
    fn open(&self, reopen: bool) -> Result<ProductInstaller<'_>> {
        ProductInstaller::prepare_isolated(
            self.root.path(),
            &self.bundle.public(),
            &self.bundle.bindings,
            &self.fence,
            self.services.clone(),
            reopen,
        )
    }
    fn path(&self, path: &str) -> PathBuf {
        actual(self.root.path(), path)
    }
    fn protected(&self) -> Vec<Vec<u8>> {
        [
            "/var/lib/rustsimhub/rustsimhub-v1/core/state/store.sqlite3",
            "/var/lib/rustsimhub/rustsimhub-v1/system/app/state.json",
            "/var/lib/rustsimhub/rustsimhub-v1/system/recovery-budget",
            "/etc/passwd",
            "/etc/group",
            "/etc/rustsimhub/product-roles.json",
        ]
        .iter()
        .map(|p| fs::read(self.path(p)).unwrap())
        .collect()
    }
}
fn actual(root: &Path, path: &str) -> PathBuf {
    root.join(path.trim_start_matches('/'))
}
fn mkdirs(root: &Path, parent: &Path) {
    fs::create_dir_all(parent).unwrap();
    let mut at = parent;
    while at != root {
        fs::set_permissions(at, fs::Permissions::from_mode(0o755)).unwrap();
        at = at.parent().unwrap();
    }
}
fn put(root: &Path, path: &str, bytes: &[u8], mode: u32) {
    let path = actual(root, path);
    mkdirs(root, path.parent().unwrap());
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn actual_install_idempotent_inspect_and_rollback_preserve_business_nss_and_default_off() {
    for profile in [
        Profile::Legacy,
        Profile::Product,
        Profile::Email,
        Profile::Recovery,
        Profile::EmailRecovery,
    ] {
        let f = Fixture::new(profile);
        let protected = f.protected();
        let mut installer = f.open(false).unwrap();
        assert_eq!(installer.inspect().unwrap().state, InstallState::Prepared);
        let plan = serde_json::to_value(installer.plan().unwrap()).unwrap();
        assert_eq!(plan["root_baseline_verified"], true);
        assert_eq!(plan["nss_baseline_verified"], true);
        assert_eq!(plan["authorization_verified"], false);
        assert_eq!(plan["nss_baseline_sha256"].as_str().unwrap().len(), 64);
        let report = installer.apply().unwrap();
        assert_eq!(report.state, InstallState::Applied);
        assert!(
            !report.starts_services
                && !report.may_confirm
                && !report.health_verified
                && !report.shared_cli_integrated
        );
        for spec in profile.targets() {
            let path = f.path(spec.path);
            let meta = fs::metadata(&path).unwrap();
            assert_eq!(meta.mode() & 0o7777, spec.mode);
            assert_eq!(meta.uid(), rustix::process::geteuid().as_raw());
            assert_eq!(
                fs::read(path).unwrap(),
                fs::read(f.bundle.path().join(spec.id.payload_name())).unwrap()
            );
        }
        assert_eq!(installer.apply().unwrap().state, InstallState::Applied);
        assert_eq!(f.protected(), protected);
        drop(installer);
        let mut installer = f.open(true).unwrap();
        assert_eq!(installer.inspect().unwrap().state, InstallState::Applied);
        assert_eq!(
            installer.recover(true).unwrap().state,
            InstallState::RolledBack
        );
        for spec in profile.targets() {
            assert_eq!(
                fs::read(f.path(spec.path)).unwrap(),
                b"synthetic prior Root material"
            );
        }
        assert_eq!(f.protected(), protected);
        assert!(f
            .services
            .events
            .borrow()
            .iter()
            .all(|e| matches!(*e, "stop_optional" | "daemon_reload")));
        for name in ["nkn.enabled", "ble.enabled"] {
            assert!(!f
                .path(&format!(
                    "/var/lib/rustsimhub/rustsimhub-v1/system/product/{name}"
                ))
                .exists());
        }
    }
}
#[test]
fn prepared_transaction_reopens_from_retained_signed_bytes_after_inbox_disappears() {
    let f = Fixture::new(Profile::Recovery);
    let protected = f.protected();
    drop(f.open(false).unwrap());
    fs::remove_dir_all(f.path("/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox")).unwrap();
    let mut installer = f.open(true).unwrap();
    assert_eq!(installer.apply().unwrap().state, InstallState::Applied);
    assert_eq!(f.protected(), protected);
}
#[test]
fn interruption_at_replace_journal_and_service_boundaries_resumes_or_restores_exact_old_bytes() {
    for point in [
        TestBoundary::BeforeReplace,
        TestBoundary::AfterReplace,
        TestBoundary::AfterJournal,
        TestBoundary::BeforeReload,
        TestBoundary::AfterReload,
    ] {
        for rollback in [false, true] {
            let f = Fixture::new(Profile::Recovery);
            let protected = f.protected();
            let mut installer = f.open(false).unwrap();
            let fired = Rc::new(Cell::new(false));
            let once = fired.clone();
            installer.fault_at(move |p, _| {
                if p == point && !once.replace(true) {
                    Err(Error::Io)
                } else {
                    Ok(())
                }
            });
            assert_eq!(installer.apply().err(), Some(Error::Io));
            assert!(fired.get());
            drop(installer);
            let mut installer = f.open(true).unwrap();
            assert_eq!(
                installer
                    .recover(rollback)
                    .unwrap_or_else(|error| panic!(
                        "recover {point:?} rollback={rollback}: {error:?}"
                    ))
                    .state,
                if rollback {
                    InstallState::RolledBack
                } else {
                    InstallState::Applied
                }
            );
            for spec in f.bundle.bindings.profile.targets() {
                let bytes = fs::read(f.path(spec.path)).unwrap();
                assert_eq!(
                    bytes,
                    if rollback {
                        b"synthetic prior Root material".to_vec()
                    } else {
                        fs::read(f.bundle.path().join(spec.id.payload_name())).unwrap()
                    }
                );
            }
            assert_eq!(f.protected(), protected);
        }
    }
}
#[test]
fn interrupted_rollback_is_idempotent_and_never_deletes_later_target_data() {
    for point in [TestBoundary::BeforeRestore, TestBoundary::AfterRestore] {
        let f = Fixture::new(Profile::Recovery);
        let mut installer = f.open(false).unwrap();
        installer.apply().unwrap();
        let fired = Rc::new(Cell::new(false));
        let once = fired.clone();
        installer.fault_at(move |p, _| {
            if p == point && !once.replace(true) {
                Err(Error::Io)
            } else {
                Ok(())
            }
        });
        assert!(installer.recover(true).is_err());
        drop(installer);
        let mut installer = f.open(true).unwrap();
        assert_eq!(
            installer.recover(true).unwrap().state,
            InstallState::RolledBack
        );
        assert!(fired.get());
    }
    let f = Fixture::new(Profile::Product);
    let mut installer = f.open(false).unwrap();
    installer.apply().unwrap();
    let changed = f.path(TargetId::NknSdk.spec().path);
    fs::remove_file(&changed).unwrap();
    fs::write(&changed, b"later administrator data must survive").unwrap();
    fs::set_permissions(&changed, fs::Permissions::from_mode(0o555)).unwrap();
    assert!(installer.recover(true).is_err());
    assert_eq!(
        fs::read(changed).unwrap(),
        b"later administrator data must survive"
    );
}
#[test]
fn fresh_absent_targets_remove_only_the_transactions_own_files_during_recovery() {
    let f = Fixture::new(Profile::Product);
    for spec in f.bundle.bindings.profile.targets() {
        fs::remove_file(f.path(spec.path)).unwrap();
    }
    let mut installer = f.open(false).unwrap();
    installer.apply().unwrap();
    assert_eq!(
        installer.recover(true).unwrap().state,
        InstallState::RolledBack
    );
    for spec in f.bundle.bindings.profile.targets() {
        assert!(!f.path(spec.path).exists());
    }
}
#[test]
fn changed_backup_or_lease_nss_services_and_journal_conflicts_remain_failures() {
    for case in ["backup", "lease", "nss", "service", "journal"] {
        let f = Fixture::new(Profile::Product);
        let protected = f.protected();
        let mut installer = f.open(false).unwrap();
        assert!(matches!(f.open(false), Err(Error::Busy)));
        match case {
            "backup" => {
                installer.apply().unwrap();
                let tx = installer.inspect().unwrap().transaction;
                fs::write(f.path(&format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-install/transactions/{tx}/before/launcher")),b"tamper").unwrap();
            }
            "lease" => f.fence.fail.set(true),
            "nss" => {
                let mut bytes = fs::read(f.path("/etc/group")).unwrap();
                bytes.extend_from_slice(b"foreign:x:1201:\n");
                fs::write(f.path("/etc/group"), bytes).unwrap();
            }
            "service" => f.services.off.set(false),
            _ => {
                let tx = installer.inspect().unwrap().transaction;
                fs::write(f.path(&format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-install/transactions/{tx}/journal.json")),b"{}").unwrap();
            }
        }
        assert!(
            if case == "backup" {
                installer.recover(true)
            } else {
                installer.apply()
            }
            .is_err(),
            "{case}"
        );
        if case != "nss" {
            assert_eq!(f.protected(), protected);
        }
    }
}
#[test]
fn wrong_trust_and_enabled_flags_fail_before_target_or_journal_creation() {
    let f = Fixture::new(Profile::Product);
    let protected = f.protected();
    let wrong = ed25519_dalek::SigningKey::from_bytes(&[94; 32])
        .verifying_key()
        .to_bytes();
    assert!(matches!(
        ProductInstaller::prepare_isolated(
            f.root.path(),
            &wrong,
            &f.bundle.bindings,
            &f.fence,
            f.services.clone(),
            false
        ),
        Err(Error::Signature)
    ));
    assert!(!f
        .path("/var/lib/rustsimhub/rustsimhub-v1/system/product-install")
        .exists());
    put(
        f.root.path(),
        "/var/lib/rustsimhub/rustsimhub-v1/system/product/ble.enabled",
        b"enabled externally",
        0o600,
    );
    assert!(matches!(f.open(false), Err(Error::Denied)));
    assert_eq!(f.protected(), protected);
}

#[test]
fn prepared_journal_before_pointer_publication_is_recoverable_only_with_unchanged_baseline() {
    for changed in [false, true] {
        let f = Fixture::new(Profile::Recovery);
        let protected = f.protected();
        drop(f.open(false).unwrap());
        fs::remove_file(
            f.path("/var/lib/rustsimhub/rustsimhub-v1/system/product-install/active.json"),
        )
        .unwrap();
        if changed {
            put(
                f.root.path(),
                TargetId::NknRoot.spec().path,
                b"later Root material",
                0o755,
            );
            assert!(matches!(f.open(false), Err(Error::Conflict)));
            assert_eq!(
                fs::read(f.path(TargetId::NknRoot.spec().path)).unwrap(),
                b"later Root material"
            );
        } else {
            let mut installer = f.open(false).unwrap();
            assert_eq!(installer.inspect().unwrap().state, InstallState::Prepared);
            assert_eq!(installer.apply().unwrap().state, InstallState::Applied);
        }
        assert_eq!(f.protected(), protected);
    }
}

#[test]
fn distinct_signed_upgrade_backs_up_installed_bytes_and_retains_prior_history() {
    let mut f = Fixture::new(Profile::Recovery);
    let protected = f.protected();
    let first = f.open(false).unwrap().apply().unwrap().transaction;
    let before = f.bundle.source_files();
    let mut upgrade = Bundle::new(Profile::Recovery);
    let mut launcher = fs::read(upgrade.path().join(TargetId::Launcher.payload_name())).unwrap();
    launcher.extend_from_slice(b"second signed launcher build");
    upgrade.replace_payload(TargetId::Launcher, &launcher);
    upgrade.resign();
    for (name, bytes) in upgrade.source_files() {
        put(
            f.root.path(),
            &format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox/{name}"),
            &bytes,
            0o600,
        );
    }
    f.bundle = upgrade;
    let mut installer = f.open(false).unwrap();
    let second = installer.apply().unwrap().transaction;
    assert_ne!(first, second);
    assert!(f.path(&format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-install/transactions/{first}/journal.json")).exists());
    assert_eq!(
        installer.recover(true).unwrap().state,
        InstallState::RolledBack
    );
    for spec in f.bundle.bindings.profile.targets() {
        let bytes = before
            .iter()
            .find(|(name, _)| name == spec.id.payload_name())
            .unwrap();
        assert_eq!(fs::read(f.path(spec.path)).unwrap(), bytes.1);
    }
    assert_eq!(f.protected(), protected);
}

#[test]
#[ignore = "private child entry: invoked only by the bounded SIGKILL parent test"]
fn private_install_crash_child() {
    let marker =
        PathBuf::from(std::env::var_os("PRODUCT_INSTALL_CRASH_MARKER").expect("private marker"));
    let point = std::env::var("PRODUCT_INSTALL_CRASH_POINT").unwrap();
    let f = Fixture::new(Profile::Recovery);
    let mut installer = f.open(false).unwrap();
    let record = serde_json::to_vec(&serde_json::json!({
        "root": f.root.path(), "bindings": f.bundle.bindings,
        "digest": f.fence.digest, "protected": f.protected(),
    }))
    .unwrap();
    installer.fault_at(move |boundary, _| {
        if format!("{boundary:?}") == point {
            fs::write(&marker, &record).unwrap();
            // Parent sends a real SIGKILL. No executor error handler, Drop,
            // unlock, rollback or mark_unknown path can run in this process.
            loop {
                std::thread::park();
            }
        }
        Ok(())
    });
    installer.apply().unwrap();
    panic!("requested crash boundary was not reached");
}

#[test]
fn actual_sigkill_releases_mutex_and_recovers_durable_rename_and_journal_evidence() {
    use std::{
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    struct Reap(Child);
    impl Drop for Reap {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for point in ["AfterReplace", "AfterJournal"] {
        for rollback in [false, true] {
            let private = tempfile::tempdir().unwrap();
            let marker = private.path().join("cut.json");
            let mut child = Reap(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "private_install_crash_child",
                        "--test-threads=1",
                    ])
                    .env("TMPDIR", private.path())
                    .env("PRODUCT_INSTALL_CRASH_MARKER", &marker)
                    .env("PRODUCT_INSTALL_CRASH_POINT", point)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now() + Duration::from_secs(15);
            while !marker.exists() {
                assert!(Instant::now() < deadline, "child cut timed out at {point}");
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited before cut"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let record: serde_json::Value = loop {
                if let Ok(value) = serde_json::from_slice(&fs::read(&marker).unwrap()) {
                    break value;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            };
            child.0.kill().unwrap();
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(child.0.wait().unwrap().signal(), Some(9));
            let root = PathBuf::from(record["root"].as_str().unwrap());
            assert!(root.starts_with(private.path()));
            let data = actual(
                &root,
                "/var/lib/rustsimhub/rustsimhub-v1/core/state/store.sqlite3",
            );
            let lock = OpenOptions::new()
                .read(true)
                .write(true)
                .open(data.parent().unwrap().join(".writer.lock"))
                .unwrap();
            fs2::FileExt::try_lock_exclusive(&lock).unwrap();
            let fence = Fence {
                path: data,
                digest: record["digest"].as_str().unwrap().into(),
                _lock: lock,
                fail: Cell::new(false),
                phases: RefCell::new(vec![]),
            };
            let bindings: ExpectedBindings =
                serde_json::from_value(record["bindings"].clone()).unwrap();
            let key = ed25519_dalek::SigningKey::from_bytes(&[93; 32])
                .verifying_key()
                .to_bytes();
            let services = Rc::new(IsolatedServiceState::default());
            services.off.set(true);
            let mut installer =
                ProductInstaller::prepare_isolated(&root, &key, &bindings, &fence, services, true)
                    .unwrap();
            assert_eq!(
                installer.recover(rollback).unwrap().state,
                if rollback {
                    InstallState::RolledBack
                } else {
                    InstallState::Applied
                }
            );
            let protected: Vec<Vec<u8>> =
                serde_json::from_value(record["protected"].clone()).unwrap();
            for (path, before) in [
                "/var/lib/rustsimhub/rustsimhub-v1/core/state/store.sqlite3",
                "/var/lib/rustsimhub/rustsimhub-v1/system/app/state.json",
                "/var/lib/rustsimhub/rustsimhub-v1/system/recovery-budget",
                "/etc/passwd",
                "/etc/group",
                "/etc/rustsimhub/product-roles.json",
            ]
            .iter()
            .zip(protected)
            {
                assert_eq!(fs::read(actual(&root, path)).unwrap(), before);
            }
        }
    }
}
