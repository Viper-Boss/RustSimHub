// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::store::Store;
use ed25519_dalek::SigningKey;
use std::{collections::BTreeMap, os::unix::fs::PermissionsExt};

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    public: PathBuf,
    key: [u8; 32],
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("application state ?# space");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let public = directory.path().join("release.public");
        let key = SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes();
        fs::write(&public, key).unwrap();
        fs::set_permissions(&public, fs::Permissions::from_mode(0o644)).unwrap();
        let state = serde_json::json!({
            "format":1,"key_sha256":manifest_hash(&key),"floor":71,
            "confirmed":{"hash":"a".repeat(64),"epoch":71},
            "candidate":null,"attempted":false
        });
        fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
        fs::write(root.join(".update.lock"), b"").unwrap();
        for name in ["state.json", ".update.lock"] {
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o600)).unwrap();
        }
        Self {
            _directory: directory,
            root,
            public,
            key,
        }
    }

    fn observe(&self) -> Result<ReadOnlyState> {
        ReadOnlyState::at(&self.root, &self.public)
    }

    fn inventory(&self) -> BTreeMap<PathBuf, (u32, u64, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut BTreeMap<PathBuf, (u32, u64, Vec<u8>)>) {
            for path in fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
            {
                let m = fs::symlink_metadata(&path).unwrap();
                let bytes = if m.file_type().is_symlink() {
                    fs::read_link(&path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else if m.is_file() {
                    fs::read(&path).unwrap()
                } else {
                    vec![]
                };
                out.insert(path.clone(), (m.mode(), m.nlink(), bytes));
                if m.is_dir() {
                    walk(&path, out);
                }
            }
        }
        let mut result = BTreeMap::new();
        walk(self._directory.path(), &mut result);
        result
    }
}

#[test]
fn existing_ledger_is_reported_without_creating_versions_or_touching_files() {
    let f = Fixture::new();
    let before = f.inventory();
    let observation = f.observe().unwrap();
    let report = observation.report().unwrap();
    assert_eq!(report["scope"], "application-ledger");
    assert_eq!(report["read_only"], true);
    assert_eq!(report["trust_binding_verified"], true);
    for claim in [
        "authorization_verified",
        "health_verified",
        "application_integrity_verified",
        "runtime_selection_verified",
    ] {
        assert_eq!(report[claim], false);
    }
    assert_eq!(report["state"]["floor"], 71);
    assert_eq!(report["state"]["confirmed"]["epoch"], 71);
    assert!(!f.root.join("versions").exists());
    drop(observation);
    assert_eq!(before, f.inventory());
}

#[test]
fn immutable_installed_public_trust_is_accepted_without_chmod() {
    let f = Fixture::new();
    fs::set_permissions(&f.public, fs::Permissions::from_mode(0o444)).unwrap();
    let before = f.inventory();
    assert_eq!(
        f.observe().unwrap().report().unwrap()["trust_binding_verified"],
        true
    );
    assert_eq!(before, f.inventory());
}

#[test]
fn valid_invalid_and_linked_pending_states_are_never_recovered_or_deleted() {
    for pending in ["valid", "malformed", "dangling-link"] {
        let f = Fixture::new();
        let next = f.root.join("state.next");
        match pending {
            "valid" => {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(f.root.join("state.json")).unwrap()).unwrap();
                value["floor"] = 72.into();
                fs::write(&next, serde_json::to_vec(&value).unwrap()).unwrap();
                fs::set_permissions(&next, fs::Permissions::from_mode(0o600)).unwrap();
            }
            "malformed" => fs::write(&next, b"not a ledger").unwrap(),
            _ => std::os::unix::fs::symlink("missing", &next).unwrap(),
        }
        let before = f.inventory();
        assert!(matches!(f.observe(), Err(Error::Busy)));
        assert_eq!(before, f.inventory());
    }
}

#[test]
fn missing_sources_and_unknown_ledger_fields_fail_without_initialization() {
    for name in [
        ".update.lock",
        "state.json",
        "release.public",
        "bad-json",
        "extra-field",
    ] {
        let f = Fixture::new();
        match name {
            "release.public" => fs::remove_file(&f.public).unwrap(),
            "bad-json" => fs::write(f.root.join("state.json"), b"{}").unwrap(),
            "extra-field" => {
                let path = f.root.join("state.json");
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value["untrusted_health"] = true.into();
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            _ => fs::remove_file(f.root.join(name)).unwrap(),
        }
        let before = f.inventory();
        assert!(f.observe().is_err());
        assert_eq!(before, f.inventory());
    }
}

#[test]
fn shared_observers_exclude_writers_and_release_the_lock_on_drop() {
    let f = Fixture::new();
    let first = f.observe().unwrap();
    let second = f.observe().unwrap();
    assert!(matches!(Store::open(&f.root, f.key, 6), Err(Error::Busy)));
    drop(first);
    assert!(matches!(Store::open(&f.root, f.key, 6), Err(Error::Busy)));
    drop(second);
    let writer = Store::open(&f.root, f.key, 6).unwrap();
    assert!(matches!(f.observe(), Err(Error::Busy)));
    drop(writer);
    assert!(f.observe().is_ok());
}

#[test]
fn inherited_read_descriptor_does_not_hold_or_unlock_a_later_writer() {
    let f = Fixture::new();
    let observation = f.observe().unwrap();
    let inherited = observation._lock.0.try_clone().unwrap();
    drop(observation);
    let writer = Store::open(&f.root, f.key, 6).unwrap();
    drop(inherited);
    assert!(matches!(Store::open(&f.root, f.key, 6), Err(Error::Busy)));
    drop(writer);
}

#[test]
fn changed_sources_and_substituted_locks_invalidate_a_held_observation() {
    for item in ["state", "trust", "lock", "next"] {
        let f = Fixture::new();
        let observation = f.observe().unwrap();
        match item {
            "state" => fs::write(f.root.join("state.json"), b"{}").unwrap(),
            "trust" => fs::write(&f.public, [0; 32]).unwrap(),
            "next" => fs::write(f.root.join("state.next"), b"pending").unwrap(),
            _ => {
                fs::rename(f.root.join(".update.lock"), f.root.join("old.lock")).unwrap();
                fs::write(f.root.join(".update.lock"), b"").unwrap();
            }
        }
        assert!(observation.report().is_err());
    }
}

#[test]
fn file_type_links_permissions_and_size_are_checked_before_reading() {
    for name in ["state.json", ".update.lock", "release.public"] {
        for case in ["symlink", "hardlink", "fifo", "writable", "oversize"] {
            let f = Fixture::new();
            let path = if name == "release.public" {
                f.public.clone()
            } else {
                f.root.join(name)
            };
            match case {
                "symlink" => {
                    let saved = path.with_extension("saved");
                    fs::rename(&path, &saved).unwrap();
                    std::os::unix::fs::symlink(&saved, &path).unwrap();
                }
                "hardlink" => fs::hard_link(&path, path.with_extension("link")).unwrap(),
                "fifo" => {
                    fs::remove_file(&path).unwrap();
                    rustix::fs::mknodat(
                        rustix::fs::CWD,
                        &path,
                        rustix::fs::FileType::Fifo,
                        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                        0,
                    )
                    .unwrap();
                }
                "writable" => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap()
                }
                _ if name == ".update.lock" => continue, // A lock carries no JSON payload.
                _ => {
                    fs::write(&path, vec![0; if name == "state.json" { 8193 } else { 33 }]).unwrap()
                }
            }
            let before = f.inventory();
            assert!(f.observe().is_err(), "{name}/{case}");
            assert_eq!(before, f.inventory());
        }
    }
}

#[test]
fn malformed_trust_binding_and_invalid_state_relations_are_not_accepted() {
    for case in ["key", "key-length", "attempted", "epoch", "hash"] {
        let f = Fixture::new();
        let path = f.root.join("state.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match case {
            "key" => fs::write(&f.public, [8; 32]).unwrap(),
            "key-length" => fs::write(&f.public, [8; 31]).unwrap(),
            "attempted" => value["attempted"] = true.into(),
            "epoch" => value["confirmed"]["epoch"] = 72.into(),
            _ => value["confirmed"]["hash"] = "not-a-hash".into(),
        }
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(f.observe().is_err());
    }
}
