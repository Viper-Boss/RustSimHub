// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::Signer;
use std::os::unix::fs::PermissionsExt;
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    trust: PathBuf,
    scratch: PathBuf,
    key: [u8; 32],
    hash: String,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("app");
        let scratch = temp.path().join("scratch");
        for p in [&root, &scratch] {
            fs::create_dir(p).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let (signing, mut m, payloads) = crate::tests::fixture();
        let key = signing.verifying_key().to_bytes();
        m.schema_min = 6;
        m.schema_max = 6;
        m.epoch = 1;
        let mut store = Store::open(&root, key, 6).unwrap();
        let bytes = serde_json::to_vec(&m).unwrap();
        store
            .install_factory(
                &bytes,
                &signing
                    .sign(&crate::signing_bytes(&bytes).unwrap())
                    .to_bytes(),
                &payloads,
            )
            .unwrap();
        m.epoch = 2;
        let bytes = serde_json::to_vec(&m).unwrap();
        let hash = store
            .stage(
                &bytes,
                &signing
                    .sign(&crate::signing_bytes(&bytes).unwrap())
                    .to_bytes(),
                &payloads,
            )
            .unwrap();
        store.choose().unwrap();
        drop(store);
        let trust = temp.path().join("release.public");
        fs::write(&trust, key).unwrap();
        fs::set_permissions(&trust, fs::Permissions::from_mode(0o444)).unwrap();
        Self {
            _temp: temp,
            root,
            trust,
            scratch,
            key,
            hash,
        }
    }
    fn read(&self) -> ReadOnlyState {
        ReadOnlyState::runtime_test_at(&self.root, &self.trust).unwrap()
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.root.join("state.json")).unwrap()
    }
}
#[test]
fn same_lock_upgrade_confirms_original_ledger_once_without_new_epoch() {
    let f = Fixture::new();
    let ledger = f.read();
    let plan = ledger.confirmation_plan().unwrap();
    let before = f.bytes();
    let mut store = ledger.exclusive(&plan).unwrap();
    store
        .confirm_live(&plan, &f.scratch, || plan.revalidate(&ledger))
        .unwrap();
    assert_ne!(before, f.bytes());
    assert_eq!(store.floor(), 2);
    assert_eq!(store.status()["confirmed"], f.hash);
    assert_eq!(
        store.confirm_live(&plan, &f.scratch, || Ok(())).err(),
        Some(Error::Denied)
    );
    drop(store);
    drop(ledger);
    let state = f.read().report().unwrap();
    assert_eq!(state["state"]["floor"], 2);
    assert_eq!(state["state"]["attempted"], false);
    assert!(state["state"]["candidate"].is_null());
    assert!(!f.root.join("state.next").exists());
    assert!(!f.scratch.join("confirmation.next").exists());
}
#[test]
fn failed_final_gate_is_never_an_autorecoverable_confirm() {
    let f = Fixture::new();
    let ledger = f.read();
    let plan = ledger.confirmation_plan().unwrap();
    let old = f.bytes();
    let mut store = ledger.exclusive(&plan).unwrap();
    assert_eq!(
        store
            .confirm_live(&plan, &f.scratch, || Err(Error::Integrity))
            .err(),
        Some(Error::Integrity)
    );
    assert_eq!(f.bytes(), old);
    assert!(!f.root.join("state.next").exists());
    assert!(f.scratch.join("confirmation.next").exists());
    drop(store);
    drop(ledger);
    let store = Store::open(&f.root, f.key, 6).unwrap();
    assert_eq!(f.bytes(), old);
    assert_eq!(store.status()["candidate_attempted"], true);
    drop(store);
    let ledger = f.read();
    let plan = ledger.confirmation_plan().unwrap();
    let mut store = ledger.exclusive(&plan).unwrap();
    assert!(store.confirm_live(&plan, &f.scratch, || Ok(())).is_err());
    assert_eq!(f.bytes(), old);
}
#[test]
fn competing_shared_lock_and_substituted_lock_or_state_cannot_upgrade_stale_snapshot() {
    let f = Fixture::new();
    let first = f.read();
    let plan = first.confirmation_plan().unwrap();
    let second = f.read();
    let old = f.bytes();
    assert_eq!(first.exclusive(&plan).err(), Some(Error::Busy));
    assert_eq!(old, f.bytes());
    drop(second);
    drop(first);
    let ledger = f.read();
    let plan = ledger.confirmation_plan().unwrap();
    fs::rename(f.root.join(".update.lock"), f.root.join(".old-lock")).unwrap();
    fs::write(f.root.join(".update.lock"), b"").unwrap();
    assert!(ledger.exclusive(&plan).is_err());
    assert_eq!(old, f.bytes());
}
#[test]
fn modified_fallback_and_floor_or_unattempted_selection_are_refused() {
    let f = Fixture::new();
    let ledger = f.read();
    let plan = ledger.confirmation_plan().unwrap();
    let baseline = ledger.state.confirmed.as_ref().unwrap().hash.clone();
    fs::write(
        f.root
            .join("versions")
            .join(baseline)
            .join("rustsimhub-core"),
        b"changed",
    )
    .unwrap();
    assert!(plan.revalidate(&ledger).is_err());
    drop(ledger);
    let mut state: serde_json::Value = serde_json::from_slice(&f.bytes()).unwrap();
    state["floor"] = 3.into();
    fs::write(
        f.root.join("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    assert!(f.read().confirmation_plan().is_err());
    state["floor"] = 2.into();
    state["attempted"] = false.into();
    fs::write(
        f.root.join("state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
    assert!(f.read().confirmation_plan().is_err());
}
#[test]
fn partial_scratch_or_unsafe_scratch_is_not_cleaned_or_promoted() {
    let f = Fixture::new();
    fs::write(f.scratch.join("confirmation.next"), b"partial crash").unwrap();
    let ledger = f.read();
    let plan = ledger.confirmation_plan().unwrap();
    let mut store = ledger.exclusive(&plan).unwrap();
    let old = f.bytes();
    assert!(store.confirm_live(&plan, &f.scratch, || Ok(())).is_err());
    assert_eq!(f.bytes(), old);
    assert_eq!(
        fs::read(f.scratch.join("confirmation.next")).unwrap(),
        b"partial crash"
    );
}
