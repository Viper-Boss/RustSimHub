// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use fs2::FileExt;
use rusqlite::Connection;
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

fn uid() -> u32 {
    rustix::process::geteuid().as_raw()
}
fn put(root: &Path, path: &str, bytes: &[u8], mode: u32) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn fixture(mode: Mode, outbox: bool) -> TempDir {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), BASE_PATH, &text(BASE), 0o644);
    if mode != Mode::Device {
        for material in forwarding() {
            put(
                root.path(),
                material.path,
                material.expected.as_ref().unwrap(),
                material.mode,
            );
        }
    }
    if mode == Mode::SetupAp {
        for material in setup_ap() {
            put(
                root.path(),
                material.path,
                material
                    .expected
                    .as_deref()
                    .unwrap_or(b"synthetic executable, provenance intentionally unverified"),
                material.mode,
            );
        }
    }
    let state = root.path().join(STATE);
    fs::create_dir_all(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    // Real main Store startup/close leaves a clean WAL-header schema-6 DB.
    drop(rustsimhub_storage::Store::open(&state).unwrap());
    if outbox {
        let directory = state.join("forwarding");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        drop(
            rustsimhub_forwarder::Queue::open(
                &directory,
                rustsimhub_forwarder::Binding {
                    device_key: [1; 32],
                    realm: "00000000-0000-4000-8000-000000000001".into(),
                    owner_key: [2; 32],
                    owner_generation: 1,
                },
            )
            .unwrap(),
        );
    }
    root
}
fn snapshot(root: &Path) -> MaintenanceSnapshot {
    MaintenanceSnapshot::observe_at(root, uid(), uid()).unwrap()
}
fn tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(path: &Path, result: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk(&entry.path(), result);
            } else {
                result.push((entry.path(), fs::read(entry.path()).unwrap()));
            }
        }
    }
    let mut result = vec![];
    walk(root, &mut result);
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}

#[test]
fn real_store_and_queue_are_observed_without_recovery_or_sidecar_creation() {
    let root = fixture(Mode::SetupAp, true);
    let before = tree(root.path());
    let state = root.path().join(STATE);
    let header = fs::read(state.join("store.sqlite3")).unwrap();
    assert_eq!(&header[18..20], &[2, 2]);
    let observed = snapshot(root.path());
    let report = serde_json::to_value(observed.report()).unwrap();
    assert_eq!(report["business_schema"], 6);
    assert_eq!(report["outbox"]["format"], 3);
    assert_eq!(report["root_mode"], "setup_ap");
    assert_eq!(report["authorization_verified"], false);
    assert_eq!(report["effective_systemd_verified"], false);
    assert_eq!(report["signed_programs_verified"], false);
    assert_eq!(report["root_material"].as_array().unwrap().len(), 18);
    assert!(observed.root_contract().is_some());
    observed.revalidate().unwrap();
    drop(observed);
    assert_eq!(tree(root.path()), before);
}

#[test]
fn three_recognized_disk_profiles_and_absent_or_empty_outbox_are_explicit() {
    for mode in [Mode::Device, Mode::Forwarding, Mode::SetupAp] {
        let root = fixture(mode, false);
        let observed = snapshot(root.path());
        assert!(observed.report().outbox.is_none());
        assert_eq!(observed.root_mode, mode);
        drop(observed);
        let directory = root.path().join(STATE).join("forwarding");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(snapshot(root.path()).report().outbox.is_none());
    }
}

#[test]
fn actual_main_writer_blocks_observation_and_snapshot_blocks_the_real_main_writer() {
    let root = fixture(Mode::Forwarding, true);
    let state = root.path().join(STATE);
    let writer = rustsimhub_storage::Store::open(&state).unwrap();
    assert!(matches!(
        MaintenanceSnapshot::observe_at(root.path(), uid(), uid()),
        Err(Error::Busy)
    ));
    drop(writer);
    let observed = snapshot(root.path());
    assert!(rustsimhub_storage::Store::open(&state).is_err());
    let lock = private_file(&state.join("forwarding/.writer.lock"), uid(), 0o600, 0).unwrap();
    assert!(lock.try_lock_exclusive().is_err());
    drop(observed);
    assert!(rustsimhub_storage::Store::open(&state).is_ok());
    assert!(lock.try_lock_exclusive().is_ok());
}

#[test]
fn outbox_writer_prevents_the_whole_observation_and_partial_acquisition_is_released() {
    let root = fixture(Mode::SetupAp, true);
    let state = root.path().join(STATE);
    let lock = private_file(&state.join("forwarding/.writer.lock"), uid(), 0o600, 0).unwrap();
    lock.try_lock_exclusive().unwrap();
    assert!(matches!(
        MaintenanceSnapshot::observe_at(root.path(), uid(), uid()),
        Err(Error::Busy)
    ));
    assert!(rustsimhub_storage::Store::open(&state).is_ok());
}

#[test]
fn any_sidecar_or_restore_marker_requires_recovery_elsewhere_and_is_untouched() {
    for name in [
        "store.sqlite3-journal",
        "store.sqlite3-wal",
        "store.sqlite3-shm",
        ".restore-pending",
        "forwarding/outbox.sqlite3-journal",
        "forwarding/outbox.sqlite3-wal",
        "forwarding/outbox.sqlite3-shm",
    ] {
        let root = fixture(Mode::SetupAp, true);
        let path = root.path().join(STATE).join(name);
        fs::write(&path, b"").unwrap();
        let before = tree(root.path());
        assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
        assert_eq!(tree(root.path()), before);
        fs::remove_file(&path).unwrap();
        symlink(root.path().join("missing"), &path).unwrap();
        assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }
}

#[test]
fn incomplete_or_changed_root_materials_are_not_promoted_to_a_known_profile() {
    let root = fixture(Mode::SetupAp, true);
    fs::remove_file(root.path().join("etc/rustsimhub/setup-ap-contract.json")).unwrap();
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
    let root = fixture(Mode::Forwarding, true);
    put(root.path(), forwarding()[0].path, b"unreviewed unit", 0o644);
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
    let root = fixture(Mode::Device, true);
    put(root.path(), BASE_PATH, &text(BASE), 0o666);
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
}

#[test]
fn symlinks_hardlinks_wrong_owners_and_uninitialized_databases_are_refused() {
    let root = fixture(Mode::SetupAp, true);
    let path = root.path().join(STATE).join("store.sqlite3");
    let saved = path.with_extension("saved");
    fs::rename(&path, &saved).unwrap();
    symlink(&saved, &path).unwrap();
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(&saved, &path).unwrap();
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
    let root = fixture(Mode::Device, false);
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid().wrapping_add(1), uid()).is_err());
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid().wrapping_add(1)).is_err());
    let path = root.path().join(STATE).join("store.sqlite3");
    fs::write(&path, b"").unwrap();
    assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
}

#[test]
fn revalidation_detects_root_changes_and_outbox_creation_after_an_absent_observation() {
    let root = fixture(Mode::Device, false);
    let observed = snapshot(root.path());
    put(root.path(), BASE_PATH, b"changed unit", 0o644);
    assert!(observed.revalidate().is_err());
    drop(observed);
    let root = fixture(Mode::Device, false);
    let observed = snapshot(root.path());
    let path = root.path().join(STATE).join("forwarding");
    fs::create_dir(&path).unwrap();
    assert!(observed.revalidate().is_err());
}

#[test]
fn reports_version_facts_without_reading_or_echoing_business_payloads() {
    let root = fixture(Mode::Forwarding, true);
    let path = root.path().join(STATE).join("store.sqlite3");
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE synthetic_secret(body TEXT);INSERT INTO synthetic_secret VALUES('synthetic sensitive text');").unwrap();
    drop(db);
    let observed = snapshot(root.path());
    let output = serde_json::to_string(&observed.report()).unwrap();
    assert!(!output.contains("synthetic sensitive"));
    assert!(!output.contains("owner_key"));
    assert!(output.len() < 16384);
}

#[test]
fn unknown_runtime_and_vendor_core_overrides_are_not_ignored() {
    for relative in [
        "etc/systemd/system/rustsimhub-core.service.d/99-override.conf",
        "run/systemd/system/rustsimhub-core.service",
        "run/systemd/system/rustsimhub-core.service.d/override.conf",
        "usr/lib/systemd/system/rustsimhub-core.service.d/vendor.conf",
    ] {
        let root = fixture(Mode::SetupAp, true);
        put(root.path(), relative, b"synthetic unknown override", 0o644);
        assert!(MaintenanceSnapshot::observe_at(root.path(), uid(), uid()).is_err());
    }
}

#[test]
fn private_fixture_paths_with_spaces_and_query_bytes_do_not_become_sqlite_uri_options() {
    let original = fixture(Mode::Forwarding, true);
    let parent = tempfile::tempdir().unwrap();
    let moved = parent.path().join("root #?mode=rw");
    fs::rename(original.path(), &moved).unwrap();
    let before = tree(&moved);
    let observed = snapshot(&moved);
    observed.revalidate().unwrap();
    drop(observed);
    assert_eq!(tree(&moved), before);
}

fn programs(format: i64) -> (String, String) {
    let runtime = serde_json::json!({"format":1,"worker_protocol":4,"outbox":{"application_id":0x52534657,
        "read_format":format,"write_format":format,"migrate_from":(1..format).collect::<Vec<_>>()}});
    let core=serde_json::json!({"component":"core","target_arch":"aarch64","source_archive_sha256":"a".repeat(64),
        "database_schema":6,"development_fixtures":false,"service_launch":{"format":1,"modes":["device","forwarding_managed","setup_ap"]},
        "setup_ap_installation_contract":rustsimhub_core_launcher::installation_contract(),"forwarding_runtime_contract":runtime}).to_string();
    let worker=serde_json::json!({"worker":true,"protocol":4,"source_archive_sha256":"a".repeat(64),"development_fixtures":false,"forwarding_runtime_contract":runtime}).to_string();
    (core, worker)
}
fn pair(programs: &(String, String)) -> ProgramMetadata<'_> {
    ProgramMetadata {
        core: &programs.0,
        worker: Some(&programs.1),
    }
}

#[test]
fn live_version_check_borrows_actual_root_and_data_fences_without_accepting_fake_observations() {
    let root = fixture(Mode::SetupAp, true);
    let before = tree(root.path());
    let observed = snapshot(root.path());
    let programs = programs(3);
    let check = observed
        .compare_versions(pair(&programs), pair(&programs), pair(&programs))
        .unwrap();
    assert_eq!(check.report().exit_code(), 0);
    let report = serde_json::to_value(check.report()).unwrap();
    assert_eq!(report["root_mode"], "setup_ap");
    assert_eq!(report["actual_data"]["business_schema"], 6);
    assert_eq!(report["actual_data"]["outbox"]["format"], 3);
    assert_eq!(report["authorization_verified"], false);
    assert!(rustsimhub_storage::Store::open(root.path().join(STATE)).is_err());
    check.revalidate().unwrap();
    drop(check);
    drop(observed);
    assert_eq!(tree(root.path()), before);
}

#[test]
fn live_version_check_rejects_an_old_reader_after_projecting_the_actual_older_outbox() {
    let root = fixture(Mode::SetupAp, true);
    let path = root.path().join(STATE).join("forwarding/outbox.sqlite3");
    let db = Connection::open(path).unwrap();
    db.execute_batch("DROP TABLE history_batches;PRAGMA user_version=2;")
        .unwrap();
    drop(db);
    let before = tree(root.path());
    let observed = snapshot(root.path());
    let current = programs(3);
    let old = programs(2);
    let check = observed
        .compare_versions(pair(&current), pair(&current), pair(&old))
        .unwrap();
    let report = serde_json::to_value(check.report()).unwrap();
    assert_eq!(report["actual_data"]["outbox"]["format"], 2);
    assert_eq!(report["projected_data"]["outbox"]["format"], 3);
    assert_eq!(check.report().exit_code(), 4);
    assert_eq!(
        report["fallback_after_candidate"]["reasons"],
        serde_json::json!(["unsupported_outbox"])
    );
    check.revalidate().unwrap();
    drop(check);
    drop(observed);
    assert_eq!(tree(root.path()), before);
}
