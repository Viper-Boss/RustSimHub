// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_recovery as recovery;

fn sealed(f: &Fixture) -> (PathBuf, PathBuf, [u8; 32]) {
    let pin = Identity::open(&f.identity).unwrap().public_key();
    let keys = f.dir("recovery-keys");
    recovery::keygen(&keys).unwrap();
    let archive = f.dir("encrypted-backup");
    let blob = archive.join("identity.age");
    recovery::backup(&f.identity, &keys.join("recipient.txt"), &blob).unwrap();
    (keys, blob, pin)
}
fn encrypt_wrong_version(f: &Fixture, keys: &Path, pin: [u8; 32]) -> PathBuf {
    let recipient: age::x25519::Recipient = fs::read_to_string(keys.join("recipient.txt"))
        .unwrap()
        .parse()
        .unwrap();
    let mut plain = b"RSHIBK02".to_vec();
    plain.extend(pin);
    plain.extend(fs::read(f.identity.join("identity.seed")).unwrap());
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let mut output = encryptor.wrap_output(Vec::new()).unwrap();
    output.write_all(&plain).unwrap();
    let path = f.root.join("unsupported-inner-version.age");
    write(&path, &output.finish().unwrap());
    path
}

#[test]
fn identity_ciphertext_key_pin_and_inner_version_matrix_writes_no_plaintext() {
    let f = Fixture::new();
    let (keys, blob, pin) = sealed(&f);
    let wrong = f.dir("wrong-keys");
    recovery::keygen(&wrong).unwrap();
    let raw = fs::read(&blob).unwrap();
    let source_before = manifest(&f.identity);
    let version = encrypt_wrong_version(&f, &keys, pin);
    let truncated = f.root.join("truncated.age");
    write(&truncated, &raw[..raw.len() / 2]);
    let corrupt = f.root.join("corrupt.age");
    let mut changed = raw.clone();
    *changed.last_mut().unwrap() ^= 1;
    write(&corrupt, &changed);
    let oversized = f.root.join("oversized.age");
    write(&oversized, &vec![0; 8193]);
    let cases = [
        (wrong.join("recovery.agekey"), blob.clone(), pin),
        (keys.join("recovery.agekey"), blob.clone(), [0; 32]),
        (keys.join("recovery.agekey"), version, pin),
        (keys.join("recovery.agekey"), truncated, pin),
        (keys.join("recovery.agekey"), corrupt, pin),
        (keys.join("recovery.agekey"), oversized, pin),
        (keys.join("absent.agekey"), blob.clone(), pin),
        (keys.join("recovery.agekey"), f.root.join("absent.age"), pin),
    ];
    for (i, (key, input, expected)) in cases.into_iter().enumerate() {
        let target = f.dir(&format!("rejected-{i}"));
        assert!(recovery::restore(&key, &input, expected, &target).is_err());
        assert!(
            fs::read_dir(target).unwrap().next().is_none(),
            "rejected identity wrote plaintext"
        );
    }
    let empty = f.dir("missing-identity");
    let output = f.root.join("must-not-exist.age");
    assert!(recovery::backup(&empty, &keys.join("recipient.txt"), &output).is_err());
    assert!(!output.exists());
    assert!(fs::read_dir(empty).unwrap().next().is_none());
    let target = f.dir("valid-restored");
    recovery::restore(&keys.join("recovery.agekey"), &blob, pin, &target).unwrap();
    assert_eq!(Identity::load(&target).unwrap().public_key(), pin);
    assert!(hash(&target.join("identity.seed")) == hash(&f.identity.join("identity.seed")));
    assert!(manifest(&f.identity) == source_before);
}

#[test]
fn identity_restore_repetition_links_and_private_modes_do_not_overwrite() {
    let f = Fixture::new();
    let (keys, blob, pin) = sealed(&f);
    let target = f.dir("restored");
    recovery::restore(&keys.join("recovery.agekey"), &blob, pin, &target).unwrap();
    let before = manifest(&target);
    let encrypted = hash(&blob);
    for _ in 0..2 {
        assert!(recovery::restore(&keys.join("recovery.agekey"), &blob, pin, &target).is_err());
        assert!(manifest(&target) == before);
    }
    assert!(recovery::backup(&f.identity, &keys.join("recipient.txt"), &blob).is_err());
    assert!(hash(&blob) == encrypted);
    let linked_key = f.root.join("linked-key");
    symlink(keys.join("recovery.agekey"), &linked_key).unwrap();
    let link_input = f.root.join("linked.age");
    symlink(&blob, &link_input).unwrap();
    let link_target = f.root.join("linked-target");
    symlink(&target, &link_target).unwrap();
    assert!(recovery::restore(&linked_key, &blob, pin, &f.dir("bad-key-link")).is_err());
    assert!(
        recovery::restore(
            &keys.join("recovery.agekey"),
            &link_input,
            pin,
            &f.dir("bad-input-link")
        )
        .is_err()
    );
    assert!(recovery::restore(&keys.join("recovery.agekey"), &blob, pin, &link_target).is_err());
    assert!(
        recovery::restore(
            &keys.join("recovery.agekey"),
            &blob,
            pin,
            Path::new("relative-fixture")
        )
        .is_err()
    );
    let public = f.dir("public-destination");
    fs::set_permissions(&public, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(recovery::restore(&keys.join("recovery.agekey"), &blob, pin, &public).is_err());
    fs::set_permissions(
        keys.join("recovery.agekey"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let empty = f.dir("bad-key-mode");
    assert!(recovery::restore(&keys.join("recovery.agekey"), &blob, pin, &empty).is_err());
    assert!(fs::read_dir(empty).unwrap().next().is_none());
    fs::set_permissions(
        keys.join("recovery.agekey"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let hard = f.root.join("hard-key");
    fs::hard_link(keys.join("recovery.agekey"), &hard).unwrap();
    assert!(recovery::restore(&hard, &blob, pin, &f.dir("bad-hard-key")).is_err());
    assert!(manifest(&target) == before);
    assert!(hash(&blob) == encrypted);
}

#[test]
fn mixed_device_and_missing_identity_fail_at_the_actual_auth_boundary() {
    let a = Fixture::new();
    let (store, _, _, pin) = a.open();
    let b = Fixture::new();
    let (_other, _, _, other_pin) = b.open();
    assert_ne!(pin, other_pin);
    let backup = a.backup(&store, "backup");
    let destination = a.dir("restored");
    let restored = Store::restore(&backup, &destination).unwrap();
    let before = logical(&destination.join("store.sqlite3"));
    let id_a = manifest(&a.identity);
    let id_b = manifest(&b.identity);
    assert!(Auth::open(&b.identity, restored.handle()).is_err());
    assert!(logical(&destination.join("store.sqlite3")) == before);
    let missing = a.dir("missing-identity");
    assert!(Auth::open(&missing, restored.handle()).is_err());
    assert!(fs::read_dir(missing).unwrap().next().is_none());
    assert!(logical(&destination.join("store.sqlite3")) == before);
    assert_eq!(
        Auth::open(&a.identity, restored.handle())
            .unwrap()
            .public_key(),
        pin
    );
    assert!(manifest(&a.identity) == id_a);
    assert!(manifest(&b.identity) == id_b);
}
