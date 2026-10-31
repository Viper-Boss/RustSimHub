#![forbid(unsafe_code)]
#![cfg(any(target_os = "linux", windows))]

use rustsimhub_usage_stats::identity::{StatisticsIdentity, IDENTITY_DOMAIN};
use rustsimhub_usage_stats::protocol::{StatsPublicKey, Window};
use rustsimhub_usage_stats::store::{
    FileStore, LastSuccess, PendingTransaction, StoreError, MAX_STORE_BYTES,
};
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn setup() -> (TempDir, std::path::PathBuf, StatsPublicKey) {
    let parent = TempDir::new().unwrap();
    let path = parent.path().join("statistics");
    let identity = StatisticsIdentity::from_seed([91; 32]);
    let public = identity.public_key();
    drop(FileStore::create(&path, identity).unwrap());
    (parent, path, public)
}

fn rewrite(path: &Path, mutate: impl FnOnce(&mut Value)) {
    let path = path.join("state.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    mutate(&mut value);
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

#[test]
fn dedicated_identity_is_deterministic_domain_separated_and_redacted() {
    let identity = StatisticsIdentity::from_seed([91; 32]);
    assert_eq!(
        identity.public_key(),
        StatisticsIdentity::from_seed([91; 32]).public_key()
    );
    assert_ne!(
        identity.public_key(),
        StatisticsIdentity::from_seed([92; 32]).public_key()
    );
    let plain = ed25519_dalek::SigningKey::from_bytes(&[91; 32])
        .verifying_key()
        .to_bytes();
    assert_ne!(identity.public_key().as_bytes(), &plain);
    let debug = format!("{identity:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("91, 91"));
    assert!(!debug.contains("5b5b5b5b"));
    assert!(IDENTITY_DOMAIN.starts_with("RustSimHub/usage-stats/"));
}

#[test]
fn operating_system_entropy_produces_distinct_identities() {
    let first = StatisticsIdentity::generate().unwrap();
    let second = StatisticsIdentity::generate().unwrap();
    assert_ne!(first.public_key(), second.public_key());
}

#[test]
fn real_file_store_survives_drop_and_reopen_with_all_reserved_fields() {
    let (_parent, path, public) = setup();
    let store = FileStore::open(&path, public).unwrap();
    let mut state = store.load().unwrap();
    state.disabled_generation = 7;
    for (index, window) in Window::ALL.into_iter().enumerate() {
        state.windows[index].period = Some(100);
        state.windows[index].pending = Some(PendingTransaction {
            digest: [index as u8 + 1; 32],
            prepared_at: 100 * window.duration_secs(),
        });
        state.windows[index].last_success = Some(LastSuccess {
            period: 99,
            confirmed_at: 100 * window.duration_secs() - 1,
        });
    }
    store.save(&state).unwrap();
    let expected = serde_json::to_value(&state).unwrap();
    drop(store);
    let reopened = FileStore::open(&path, public).unwrap();
    assert_eq!(
        serde_json::to_value(reopened.load().unwrap()).unwrap(),
        expected
    );
    assert_eq!(reopened.identity().public_key(), public);
    assert!(path.join("state.json").is_file());
}

#[test]
fn second_writer_is_rejected_until_first_handle_drops() {
    let (_parent, path, public) = setup();
    let first = FileStore::open(&path, public).unwrap();
    assert_eq!(
        FileStore::open(&path, public).unwrap_err(),
        StoreError::Busy
    );
    drop(first);
    FileStore::open(&path, public).unwrap();
}

#[test]
fn create_existing_store_never_replaces_its_identity_or_files() {
    let (_parent, path, public) = setup();
    let original = fs::read(path.join("state.json")).unwrap();
    assert_eq!(
        FileStore::create(&path, StatisticsIdentity::from_seed([92; 32])).unwrap_err(),
        StoreError::AlreadyExists
    );
    assert_eq!(fs::read(path.join("state.json")).unwrap(), original);
    FileStore::open(&path, public).unwrap();
}

#[test]
fn opening_missing_store_does_not_create_files_or_identity() {
    let parent = TempDir::new().unwrap();
    let path = parent.path().join("missing");
    assert!(FileStore::open(&path, StatisticsIdentity::from_seed([3; 32]).public_key()).is_err());
    assert!(!path.exists());
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn truncated_and_malformed_files_are_preserved_instead_of_rekeyed() {
    for bytes in [b"".as_slice(), b"{", b"{\"format_version\":1", b"not-json"] {
        let (_parent, path, public) = setup();
        fs::write(path.join("state.json"), bytes).unwrap();
        assert_eq!(
            FileStore::open(&path, public).unwrap_err(),
            StoreError::Corrupt
        );
        assert_eq!(fs::read(path.join("state.json")).unwrap(), bytes);
    }
}

#[test]
fn unsupported_version_is_preserved_and_rejected() {
    let (_parent, path, public) = setup();
    rewrite(&path, |value| value["format_version"] = 2.into());
    let original = fs::read(path.join("state.json")).unwrap();
    assert_eq!(
        FileStore::open(&path, public).unwrap_err(),
        StoreError::UnsupportedVersion
    );
    assert_eq!(fs::read(path.join("state.json")).unwrap(), original);
}

#[test]
fn different_product_and_identity_cannot_adopt_a_store() {
    let (_parent, path, public) = setup();
    let wrong = StatisticsIdentity::from_seed([5; 32]).public_key();
    assert_eq!(
        FileStore::open(&path, wrong).unwrap_err(),
        StoreError::WrongIdentity
    );
    rewrite(&path, |value| {
        value["product_domain"] = "nknguard/usage-stats/v1".into()
    });
    assert_eq!(
        FileStore::open(&path, public).unwrap_err(),
        StoreError::WrongProduct
    );
}

#[test]
fn substituted_private_seed_is_detected_even_with_original_public_key() {
    let (_parent, path, public) = setup();
    rewrite(&path, |value| {
        value["seed"] = serde_json::to_value([7_u8; 32]).unwrap()
    });
    assert_eq!(
        FileStore::open(&path, public).unwrap_err(),
        StoreError::WrongIdentity
    );
}

#[test]
fn extra_duplicate_and_unknown_nested_fields_are_rejected() {
    for nested in [false, true] {
        let (_parent, path, public) = setup();
        rewrite(&path, |value| {
            if nested {
                value["state"]["windows"][0]["unexpected"] = true.into();
            } else {
                value["unexpected"] = true.into();
            }
        });
        assert_eq!(
            FileStore::open(&path, public).unwrap_err(),
            StoreError::Corrupt
        );
    }
    let (_parent, path, public) = setup();
    let bytes = fs::read_to_string(path.join("state.json")).unwrap();
    let duplicate = bytes.replacen('{', "{\"format_version\":1,", 1);
    fs::write(path.join("state.json"), duplicate).unwrap();
    assert_eq!(
        FileStore::open(&path, public).unwrap_err(),
        StoreError::Corrupt
    );
}

#[test]
fn oversized_file_is_rejected_before_parsing_and_retained() {
    let (_parent, path, public) = setup();
    let bytes = vec![b' '; MAX_STORE_BYTES as usize + 1];
    fs::write(path.join("state.json"), &bytes).unwrap();
    assert_eq!(
        FileStore::open(&path, public).unwrap_err(),
        StoreError::TooLarge
    );
    assert_eq!(
        fs::metadata(path.join("state.json")).unwrap().len(),
        bytes.len() as u64
    );
}

#[test]
fn invalid_window_period_and_pending_state_never_replace_saved_bytes() {
    let (_parent, path, public) = setup();
    let store = FileStore::open(&path, public).unwrap();
    let original = fs::read(path.join("state.json")).unwrap();
    for case in 0..4 {
        let mut state = store.load().unwrap();
        match case {
            0 => state.windows.swap(0, 1),
            1 => state.windows[0].period = Some(u64::MAX),
            2 => {
                state.windows[0].pending = Some(PendingTransaction {
                    digest: [1; 32],
                    prepared_at: 0,
                })
            }
            3 => {
                state.windows[0].period = Some(1);
                state.windows[0].pending = Some(PendingTransaction {
                    digest: [0; 32],
                    prepared_at: 0,
                });
            }
            _ => unreachable!(),
        }
        assert_eq!(store.save(&state), Err(StoreError::InvalidState));
        assert_eq!(fs::read(path.join("state.json")).unwrap(), original);
    }
}

#[test]
fn corrupted_file_after_open_blocks_save_without_destroying_evidence() {
    let (_parent, path, public) = setup();
    let store = FileStore::open(&path, public).unwrap();
    let state = store.load().unwrap();
    fs::write(path.join("state.json"), b"interrupted").unwrap();
    assert_eq!(store.save(&state), Err(StoreError::Corrupt));
    assert_eq!(fs::read(path.join("state.json")).unwrap(), b"interrupted");
}

#[test]
fn abandoned_temporary_file_never_becomes_authoritative() {
    let (_parent, path, public) = setup();
    fs::write(path.join(".state-abandoned.tmp"), b"incomplete replacement").unwrap();
    let store = FileStore::open(&path, public).unwrap();
    let mut state = store.load().unwrap();
    state.disabled_generation = 9;
    store.save(&state).unwrap();
    assert_eq!(store.load().unwrap().disabled_generation, 9);
    assert_eq!(
        fs::read(path.join(".state-abandoned.tmp")).unwrap(),
        b"incomplete replacement"
    );
}

#[test]
fn a_real_filesystem_failure_returns_error_and_preserves_the_obstruction() {
    let (_parent, path, public) = setup();
    let store = FileStore::open(&path, public).unwrap();
    let state = store.load().unwrap();
    fs::remove_file(path.join("state.json")).unwrap();
    fs::create_dir(path.join("state.json")).unwrap();
    assert!(store.save(&state).is_err());
    assert!(path.join("state.json").is_dir());
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    #[test]
    fn private_modes_are_exact_and_insecure_directory_is_rejected() {
        let (_parent, path, public) = setup();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0o700);
        for name in ["state.json", "writer.lock"] {
            assert_eq!(
                fs::metadata(path.join(name)).unwrap().mode() & 0o7777,
                0o600
            );
        }
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        assert_eq!(
            FileStore::open(&path, public).unwrap_err(),
            StoreError::UnsafePermissions
        );
    }

    #[test]
    fn insecure_data_and_lock_modes_are_rejected() {
        for name in ["state.json", "writer.lock"] {
            let (_parent, path, public) = setup();
            fs::set_permissions(path.join(name), fs::Permissions::from_mode(0o640)).unwrap();
            assert_eq!(
                FileStore::open(&path, public).unwrap_err(),
                StoreError::UnsafePermissions
            );
        }
    }

    #[test]
    fn symlinked_directory_ancestor_data_and_lock_are_rejected() {
        for name in ["state.json", "writer.lock"] {
            let (parent, path, public) = setup();
            let outside = parent.path().join("outside");
            fs::rename(path.join(name), &outside).unwrap();
            symlink(&outside, path.join(name)).unwrap();
            assert!(FileStore::open(&path, public).is_err());
            assert!(outside.is_file());
        }
        let (parent, path, public) = setup();
        let linked = parent.path().join("linked");
        symlink(&path, &linked).unwrap();
        assert!(FileStore::open(&linked, public).is_err());
        let linked_parent = parent.path().join("linked-parent");
        symlink(parent.path(), &linked_parent).unwrap();
        assert!(FileStore::open(linked_parent.join("statistics"), public).is_err());
    }

    #[test]
    fn hardlinked_data_and_lock_are_rejected() {
        for name in ["state.json", "writer.lock"] {
            let (parent, path, public) = setup();
            fs::hard_link(path.join(name), parent.path().join("alias")).unwrap();
            assert_eq!(
                FileStore::open(&path, public).unwrap_err(),
                StoreError::UnsafePath
            );
        }
    }

    #[test]
    fn permissions_changed_after_open_block_save_and_preserve_original() {
        let (_parent, path, public) = setup();
        let store = FileStore::open(&path, public).unwrap();
        let state = store.load().unwrap();
        let original = fs::read(path.join("state.json")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).unwrap();
        assert_eq!(store.save(&state), Err(StoreError::UnsafePermissions));
        assert_eq!(fs::read(path.join("state.json")).unwrap(), original);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn replacing_the_lock_inode_is_detected_by_the_open_handle() {
        let (_parent, path, public) = setup();
        let store = FileStore::open(&path, public).unwrap();
        fs::rename(path.join("writer.lock"), path.join("old-lock")).unwrap();
        fs::write(path.join("writer.lock"), b"").unwrap();
        fs::set_permissions(path.join("writer.lock"), fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(store.load().unwrap_err(), StoreError::UnsafePath);
    }
}
