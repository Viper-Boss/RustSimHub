#![forbid(unsafe_code)]
#![cfg(any(target_os = "linux", windows))]
use rustsimhub_usage_stats::{
    protocol::DISCLOSURE_VERSION,
    store::{
        participation::{Actor, Binding, Change, Error, Ledger, Snapshot},
        StoreError,
    },
};
use tempfile::TempDir;

fn binding() -> Binding {
    Binding {
        core_public_key: [3; 32],
        realm: "aaaaaaaa-1111-4111-8111-111111111111".into(),
    }
}
fn actor() -> Actor {
    Actor {
        public_key: [4; 32],
        generation: 1,
    }
}
fn change(n: u64, expected: u64, enabled: bool) -> Change {
    Change {
        request_id: format!("{n:08x}-1111-4111-8111-111111111111"),
        core_instance: "b".repeat(64),
        expected_generation: expected,
        enabled,
        disclosure_version: enabled.then_some(DISCLOSURE_VERSION),
    }
}

#[test]
fn absence_reads_without_creation_or_identity_and_invalid_consent_never_creates() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    assert!(Ledger::open_if_present(&path, binding()).unwrap().is_none());
    assert!(!path.exists());
    let mut edit = change(1, 0, true);
    edit.disclosure_version = None;
    assert!(matches!(
        Ledger::create(&path, binding(), actor(), edit),
        Err(Error::InvalidInput)
    ));
    assert!(!path.exists());
    let (_ledger, _) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    let raw = std::fs::read_to_string(path.join("state.json")).unwrap();
    assert!(!raw.contains("seed"));
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 2);
}
#[test]
fn consent_and_exact_receipt_survive_restart_and_disable_keeps_history() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    let (ledger, first) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    assert!(ledger.authorized_for(&actor()).unwrap());
    assert_eq!(first.generation, 1);
    drop(ledger);
    let ledger = Ledger::open_if_present(&path, binding()).unwrap().unwrap();
    assert!(ledger.snapshot().unwrap().enabled);
    assert_eq!(
        ledger
            .receipt(&actor(), &first.change.request_id)
            .unwrap()
            .unwrap()
            .generation,
        1
    );
    let off = ledger.change(actor(), change(2, 1, false)).unwrap();
    assert_eq!(off.generation, 2);
    let snap = ledger.snapshot().unwrap();
    assert_eq!(snap.disabled_generation, 1);
    assert!(!snap.enabled);
    assert!(ledger
        .receipt(&actor(), &first.change.request_id)
        .unwrap()
        .is_some());
    drop(ledger);
    let ledger = Ledger::open_if_present(&path, binding()).unwrap().unwrap();
    assert_eq!(ledger.snapshot().unwrap(), snap);
}
#[test]
fn duplicate_is_idempotent_conflicting_id_and_stale_generation_cannot_reenable() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    let (ledger, first) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    assert_eq!(
        ledger
            .change(actor(), change(1, 0, true))
            .unwrap()
            .generation,
        first.generation
    );
    ledger.change(actor(), change(2, 1, false)).unwrap();
    assert_eq!(
        ledger
            .change(actor(), change(1, 0, true))
            .unwrap()
            .generation,
        1
    );
    assert!(
        !ledger.snapshot().unwrap().enabled,
        "old receipt replay must not apply ON again"
    );
    let mut bad = change(1, 0, false);
    bad.core_instance = "c".repeat(64);
    assert!(matches!(ledger.change(actor(), bad), Err(Error::Conflict)));
    assert!(matches!(
        ledger.change(actor(), change(3, 0, true)),
        Err(Error::Conflict)
    ));
}
#[test]
fn owner_generation_and_realm_or_core_replacement_cannot_inherit_consent() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    let (ledger, _) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    let mut current = actor();
    current.generation = 2;
    assert!(!ledger.authorized_for(&current).unwrap());
    assert!(ledger
        .receipt(&current, &change(1, 0, true).request_id)
        .unwrap()
        .is_none());
    assert!(matches!(
        ledger.change(current, change(1, 0, true)),
        Err(Error::Conflict)
    ));
    drop(ledger);
    let mut other = binding();
    other.realm = "dddddddd-1111-4111-8111-111111111111".into();
    assert!(matches!(
        Ledger::open_if_present(&path, other),
        Err(Error::WrongBinding)
    ));
    let mut other = binding();
    other.core_public_key = [8; 32];
    assert!(matches!(
        Ledger::open_if_present(&path, other),
        Err(Error::WrongBinding)
    ));
}
#[test]
fn writer_lock_and_corruption_fail_closed_without_repair() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    let (ledger, _) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    assert!(matches!(
        Ledger::open_if_present(&path, binding()),
        Err(Error::Store(StoreError::Busy))
    ));
    drop(ledger);
    std::fs::write(path.join("state.json"), b"{bad").unwrap();
    assert!(Ledger::open_if_present(&path, binding()).is_err());
    assert!(Ledger::create(&path, binding(), actor(), change(2, 0, false)).is_err());
    assert_eq!(std::fs::read(path.join("state.json")).unwrap(), b"{bad");
}

#[test]
fn permission_changes_preserve_separate_identity_pending_transactions_and_cache() {
    use rustsimhub_usage_stats::{
        identity::StatisticsIdentity,
        protocol::Window,
        store::{FileStore, PendingTransaction},
    };
    let root = TempDir::new().unwrap();
    let state = root.path().join("statistics");
    let identity = StatisticsIdentity::from_seed([91; 32]);
    let key = identity.public_key();
    let statistics = FileStore::create(&state, identity).unwrap();
    let mut value = statistics.load().unwrap();
    value.disabled_generation = 7;
    value.windows[Window::Last24Hours.index()].period = Some(0);
    value.windows[0].pending = Some(PendingTransaction {
        digest: [6; 32],
        prepared_at: 1,
    });
    statistics.save(&value).unwrap();
    let original = std::fs::read(state.join("state.json")).unwrap();
    drop(statistics);
    let (permission, _) = Ledger::create(
        &root.path().join("permission"),
        binding(),
        actor(),
        change(1, 0, true),
    )
    .unwrap();
    permission.change(actor(), change(2, 1, false)).unwrap();
    drop(permission);
    assert_eq!(std::fs::read(state.join("state.json")).unwrap(), original);
    let statistics = FileStore::open(&state, key).unwrap();
    let loaded = statistics.load().unwrap();
    assert_eq!(loaded.disabled_generation, 7);
    assert_eq!(loaded.windows[0].pending.as_ref().unwrap().digest, [6; 32]);
}
#[test]
fn receipt_eviction_never_resets_generation_or_replays_old_change() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    let (ledger, _) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    for n in 2..=70 {
        ledger
            .change(actor(), change(n, n - 1, n % 2 != 0))
            .unwrap();
    }
    assert!(ledger
        .receipt(&actor(), &change(1, 0, true).request_id)
        .unwrap()
        .is_none());
    assert!(matches!(
        ledger.change(actor(), change(1, 0, true)),
        Err(Error::Conflict)
    ));
    assert_eq!(ledger.snapshot().unwrap().generation, 70);
}
#[test]
fn incomplete_directory_is_not_default_off_and_unknown_fields_are_preserved_as_errors() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    std::fs::create_dir(&path).unwrap();
    assert!(Ledger::open_if_present(&path, binding()).is_err());
    std::fs::remove_dir(&path).unwrap();
    let (ledger, _) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    drop(ledger);
    let original = std::fs::read(path.join("state.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_slice(&original).unwrap();
    v["future"] = true.into();
    let corrupted = serde_json::to_vec(&v).unwrap();
    std::fs::write(path.join("state.json"), &corrupted).unwrap();
    assert!(matches!(
        Ledger::open_if_present(&path, binding()),
        Err(Error::Corrupt)
    ));
    assert_eq!(std::fs::read(path.join("state.json")).unwrap(), corrupted);
    assert_eq!(Snapshot::absent().generation, 0);
}

#[cfg(target_os = "linux")]
#[test]
fn private_modes_symlink_and_hardlink_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = TempDir::new().unwrap();
    let path = root.path().join("permission");
    let alias = root.path().join("alias");
    let (ledger, _) = Ledger::create(&path, binding(), actor(), change(1, 0, true)).unwrap();
    drop(ledger);
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    symlink(&path, &alias).unwrap();
    assert!(Ledger::open_if_present(&alias, binding()).is_err());
    std::fs::hard_link(path.join("state.json"), root.path().join("linked")).unwrap();
    assert!(Ledger::open_if_present(&path, binding()).is_err());
    std::fs::remove_file(root.path().join("linked")).unwrap();
    std::fs::set_permissions(
        path.join("state.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(Ledger::open_if_present(&path, binding()).is_err());
}
