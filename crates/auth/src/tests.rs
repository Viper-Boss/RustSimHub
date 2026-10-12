// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_storage::Store;
use std::{fs, path::Path};
use tempfile::TempDir;

#[test]
fn recovery_reenables_only_original_owner_after_fresh_possession_proof() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let o = SigningKey::from_bytes(&[51; 32]);
    let ckey = SigningKey::from_bytes(&[52; 32]);
    let t = owner(&mut a, &o);
    pair(&mut a, &t, &ckey, Kind::Nas, &[Scope::StatusRead]);
    let backup = directory();
    let target = directory();
    s.handle().backup(backup.path()).unwrap();
    a.revoke(t.expose(), ckey.verifying_key().to_bytes())
        .unwrap();
    let restored = Store::restore(backup.path(), target.path()).unwrap();
    let mut b = Auth::open(i.path(), restored.handle()).unwrap();
    assert!(b
        .recovery_challenge(ckey.verifying_key().to_bytes())
        .is_err());
    assert!(b
        .recovery_challenge(SigningKey::from_bytes(&[53; 32]).verifying_key().to_bytes())
        .is_err());
    let c = b.recovery_challenge(o.verifying_key().to_bytes()).unwrap();
    assert!(b.recovery_finish(&c.id, &sign(&c, &ckey)).is_err());
    assert!(b.recovery_finish(&c.id, &sign(&c, &o)).is_err());
    let c = b.recovery_challenge(o.verifying_key().to_bytes()).unwrap();
    let recovered = b.recovery_finish(&c.id, &sign(&c, &o)).unwrap();
    assert_eq!(recovered.kind, Kind::Owner);
    assert!(recovered.active);
    assert_eq!(recovered.generation, 3);
    assert!(b.recovery_finish(&c.id, &sign(&c, &o)).is_err());
    assert!(b.recovery_challenge(o.verifying_key().to_bytes()).is_err());
    assert!(b.login_challenge(ckey.verifying_key().to_bytes()).is_err());
    let renewed = login(&mut b, &o);
    assert_eq!(
        b.clients(renewed.expose())
            .unwrap()
            .iter()
            .filter(|p| p.active)
            .count(),
        1
    );
    assert!(b.authorize(t.expose(), Scope::StatusRead).is_err());
    pair(&mut b, &renewed, &ckey, Kind::Nas, &[Scope::StatusRead]);
    let reauthorized = login(&mut b, &ckey);
    assert!(b
        .authorize(reauthorized.expose(), Scope::StatusRead)
        .is_ok());
}
#[test]
fn explicit_reapproval_invalidates_old_sessions_and_rejects_stale_proofs() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let o = SigningKey::from_bytes(&[54; 32]);
    let k = SigningKey::from_bytes(&[55; 32]);
    let key = k.verifying_key().to_bytes();
    let t = owner(&mut a, &o);
    pair(&mut a, &t, &k, Kind::Nas, &[Scope::StatusRead]);
    let old = login(&mut a, &k);
    a.revoke(t.expose(), key).unwrap();
    a.open_pairing(t.expose()).unwrap();
    let c = a
        .pairing_challenge(key, Kind::Nas, &[Scope::StatusRead])
        .unwrap();
    let id = a.pairing_finish(&c.id, &sign(&c, &k)).unwrap();
    let p = a.authorize(t.expose(), Scope::ClientsManage).unwrap();
    s.handle().revoke_client(p, key).unwrap();
    assert!(a
        .approve(t.expose(), &id, key, Kind::Nas, &[Scope::StatusRead])
        .is_err());
    assert!(a.pending_pairings(t.expose()).unwrap().is_empty());
    pair(&mut a, &t, &k, Kind::Nas, &[Scope::StatusRead]);
    assert!(a.authorize(old.expose(), Scope::StatusRead).is_err());
    let new = login(&mut a, &k);
    assert!(a.authorize(new.expose(), Scope::StatusRead).is_ok());
    assert!(a.clients(new.expose()).is_err());
}
#[test]
fn client_checks_pin_purpose_key_and_scope_before_signing() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let kdir = directory();
    let k = ClientKey::create(kdir.path()).unwrap();
    let c = a.bootstrap_challenge(k.public_key()).unwrap();
    let pin = a.public_key();
    assert!(k
        .prove(
            &c,
            SigningKey::from_bytes(&[56; 32]).verifying_key().to_bytes(),
            ProofExpectation::Bootstrap
        )
        .is_err());
    assert!(k.prove(&c, pin, ProofExpectation::Login).is_err());
    assert!(ClientKey::create(directory().path())
        .unwrap()
        .prove(&c, pin, ProofExpectation::Bootstrap)
        .is_err());
    let proof = k.prove(&c, pin, ProofExpectation::Bootstrap).unwrap();
    a.bootstrap_finish(&c.id, &proof).unwrap();
    let c = a.login_challenge(k.public_key()).unwrap();
    let proof = k.prove(&c, pin, ProofExpectation::Login).unwrap();
    let t = a.login_finish(&c.id, &proof).unwrap();
    a.open_pairing(t.expose()).unwrap();
    let other = ClientKey::create(directory().path()).unwrap();
    let c = a
        .pairing_challenge(other.public_key(), Kind::Nas, &[Scope::StatusRead])
        .unwrap();
    assert!(other
        .prove(
            &c,
            pin,
            ProofExpectation::Pair {
                kind: Kind::Client,
                scopes: 1
            }
        )
        .is_err());
    assert!(other
        .prove(
            &c,
            pin,
            ProofExpectation::Pair {
                kind: Kind::Nas,
                scopes: 4
            }
        )
        .is_err());
    assert!(other
        .prove(
            &c,
            pin,
            ProofExpectation::Pair {
                kind: Kind::Nas,
                scopes: 1
            }
        )
        .is_ok());
    let mut c = c;
    c.id = "a".repeat(64);
    assert!(other
        .prove(
            &c,
            pin,
            ProofExpectation::Pair {
                kind: Kind::Nas,
                scopes: 1
            }
        )
        .is_err());
}
#[test]
fn client_key_missing_load_and_cross_role_seed_are_rejected() {
    let d = directory();
    assert!(ClientKey::load(d.path()).is_err());
    assert!(!d.path().join("client.seed").exists());
    let k = ClientKey::create(d.path()).unwrap();
    assert_eq!(
        ClientKey::load(d.path()).unwrap().public_key(),
        k.public_key()
    );
    let other = directory();
    Identity::open(other.path()).unwrap();
    fs::copy(
        other.path().join("identity.seed"),
        d.path().join("client.seed"),
    )
    .unwrap();
    assert!(matches!(
        ClientKey::load(d.path()),
        Err(Error::CorruptIdentity)
    ));
}
#[test]
fn bound_database_never_generates_a_replacement_missing_identity() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let a = Auth::open(i.path(), s.handle()).unwrap();
    let key = a.public_key();
    drop(a);
    fs::remove_file(i.path().join("identity.seed")).unwrap();
    assert!(Auth::open(i.path(), s.handle()).is_err());
    assert!(!i.path().join("identity.seed").exists());
    assert_eq!(s.handle().identity_public_key().unwrap(), Some(key));
}
#[test]
fn private_identity_backup_reloads_without_overwrite() {
    let i = directory();
    let identity = Identity::open(i.path()).unwrap();
    let backup = directory();
    identity.backup(backup.path()).unwrap();
    assert_eq!(
        Identity::load(backup.path()).unwrap().public_key(),
        identity.public_key()
    );
    assert!(identity.backup(backup.path()).is_err());
}
fn directory() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    d
}
fn sign(c: &Challenge, k: &SigningKey) -> Vec<u8> {
    k.sign(&c.transcript).to_bytes().to_vec()
}
fn start(identity: &Path, store: &Store) -> Auth {
    Auth::new(Identity::open(identity).unwrap(), store.handle()).unwrap()
}
fn owner(auth: &mut Auth, key: &SigningKey) -> Token {
    let c = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &sign(&c, key)).unwrap();
    login(auth, key)
}
fn login(auth: &mut Auth, key: &SigningKey) -> Token {
    let c = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.login_finish(&c.id, &sign(&c, key)).unwrap()
}
fn pair(
    auth: &mut Auth,
    owner: &Token,
    key: &SigningKey,
    kind: Kind,
    scopes: &[Scope],
) -> Principal {
    auth.open_pairing(owner.expose()).unwrap();
    let c = auth
        .pairing_challenge(key.verifying_key().to_bytes(), kind, scopes)
        .unwrap();
    let id = auth.pairing_finish(&c.id, &sign(&c, key)).unwrap();
    auth.approve(
        owner.expose(),
        &id,
        key.verifying_key().to_bytes(),
        kind,
        scopes,
    )
    .unwrap()
}

#[test]
fn identity_survives_restart_and_complete_pending_creation() {
    let dir = directory();
    let first = Identity::open(dir.path()).unwrap().public_key();
    assert_eq!(Identity::open(dir.path()).unwrap().public_key(), first);
    fs::rename(
        dir.path().join("identity.seed"),
        dir.path().join("identity.pending"),
    )
    .unwrap();
    assert_eq!(Identity::open(dir.path()).unwrap().public_key(), first);
    assert!(!dir.path().join("identity.pending").exists());
    assert_ne!(
        Identity::open(directory().path()).unwrap().public_key(),
        first
    );
}
#[test]
fn corrupt_seed_is_preserved_and_never_regenerated() {
    let dir = directory();
    Identity::open(dir.path()).unwrap();
    let path = dir.path().join("identity.seed");
    fs::write(&path, b"interrupted seed").unwrap();
    assert!(matches!(
        Identity::open(dir.path()),
        Err(Error::CorruptIdentity)
    ));
    assert_eq!(fs::read(path).unwrap(), b"interrupted seed");
}
#[test]
fn partial_pending_seed_is_preserved_and_refused() {
    let dir = directory();
    let path = dir.path().join("identity.pending");
    fs::write(&path, b"incomplete").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(matches!(
        Identity::open(dir.path()),
        Err(Error::CorruptIdentity)
    ));
    assert!(!dir.path().join("identity.seed").exists());
    assert_eq!(fs::read(path).unwrap(), b"incomplete");
}
#[test]
fn signed_transcript_binds_device_client_scopes_purpose_and_instance() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let k = SigningKey::from_bytes(&[1; 32]);
    let c = a.bootstrap_challenge(k.verifying_key().to_bytes()).unwrap();
    key_valid(&a.public_key())
        .unwrap()
        .verify_strict(
            &c.transcript,
            &Signature::from_slice(&c.device_signature).unwrap(),
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&c.transcript).unwrap();
    assert_eq!(value["domain"], "RustSimHub.Auth.v1");
    assert_eq!(value["purpose"], "bootstrap");
    let mut tampered = c.transcript.clone();
    tampered.push(b' ');
    assert_eq!(
        a.bootstrap_finish(&c.id, &k.sign(&tampered).to_bytes()),
        Err(Error::Forbidden)
    );
    assert_eq!(a.bootstrap_finish(&c.id, &sign(&c, &k)), Err(Error::Replay));
    assert!(s
        .handle()
        .principal(k.verifying_key().to_bytes())
        .unwrap()
        .is_none());
}
#[test]
fn closed_window_unapproved_key_and_owner_escalation_are_denied() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let k = SigningKey::from_bytes(&[2; 32]);
    let t = owner(&mut a, &k);
    let client = SigningKey::from_bytes(&[3; 32]);
    let key = client.verifying_key().to_bytes();
    assert!(matches!(
        a.pairing_challenge(key, Kind::Nas, &[Scope::StatusRead]),
        Err(Error::Forbidden)
    ));
    a.open_pairing(t.expose()).unwrap();
    assert!(matches!(
        a.pairing_challenge(key, Kind::Owner, &[Scope::ClientsManage]),
        Err(Error::Forbidden)
    ));
    let c = a
        .pairing_challenge(key, Kind::Nas, &[Scope::StatusRead])
        .unwrap();
    let id = a.pairing_finish(&c.id, &sign(&c, &client)).unwrap();
    assert!(matches!(a.login_challenge(key), Err(Error::Forbidden)));
    assert_eq!(
        a.approve(t.expose(), &id, key, Kind::Nas, &[Scope::SmsSend]),
        Err(Error::Forbidden)
    );
    assert!(s.handle().principal(key).unwrap().is_none());
    assert_eq!(
        a.approve(t.expose(), &id, key, Kind::Nas, &[Scope::StatusRead]),
        Err(Error::Replay)
    );
}
#[test]
fn granted_nas_scope_does_not_make_nas_an_owner() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let o = SigningKey::from_bytes(&[4; 32]);
    let t = owner(&mut a, &o);
    let k = SigningKey::from_bytes(&[5; 32]);
    pair(
        &mut a,
        &t,
        &k,
        Kind::Nas,
        &[Scope::StatusRead, Scope::ClientsManage],
    );
    let token = login(&mut a, &k);
    assert!(a.authorize(token.expose(), Scope::StatusRead).is_ok());
    assert_eq!(
        a.authorize(token.expose(), Scope::SmsSend),
        Err(Error::Forbidden)
    );
    assert_eq!(a.open_pairing(token.expose()), Err(Error::Forbidden));
    assert_eq!(
        a.authorize(&"0".repeat(64), Scope::StatusRead),
        Err(Error::Forbidden)
    );
}
#[test]
fn revocation_invalidates_sessions_and_inflight_authentication() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let o = SigningKey::from_bytes(&[6; 32]);
    let t = owner(&mut a, &o);
    let k = SigningKey::from_bytes(&[7; 32]);
    let key = k.verifying_key().to_bytes();
    pair(&mut a, &t, &k, Kind::Client, &[Scope::StatusRead]);
    let token = login(&mut a, &k);
    let c = a.login_challenge(key).unwrap();
    // Durable revocation also invalidates sessions in a different Auth instance.
    let p = a.authorize(t.expose(), Scope::ClientsManage).unwrap();
    s.handle().revoke_client(p, key).unwrap();
    assert_eq!(
        a.authorize(token.expose(), Scope::StatusRead),
        Err(Error::Forbidden)
    );
    assert!(matches!(
        a.login_finish(&c.id, &sign(&c, &k)),
        Err(Error::Forbidden)
    ));
}
#[test]
fn restart_expires_volatile_state_but_preserves_local_authority() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let o = SigningKey::from_bytes(&[8; 32]);
    let t = owner(&mut a, &o);
    let c = a.login_challenge(o.verifying_key().to_bytes()).unwrap();
    a.open_pairing(t.expose()).unwrap();
    let public = a.public_key();
    drop(a);
    drop(s);
    let s = Store::open(d.path()).unwrap();
    let mut b = start(i.path(), &s);
    assert_eq!(b.public_key(), public);
    assert_eq!(
        b.authorize(t.expose(), Scope::StatusRead),
        Err(Error::Forbidden)
    );
    assert!(matches!(
        b.login_finish(&c.id, &sign(&c, &o)),
        Err(Error::Replay)
    ));
    assert!(b.window.is_none());
    let renewed = login(&mut b, &o);
    assert!(b.authorize(renewed.expose(), Scope::ClientsManage).is_ok());
    let second = b
        .bootstrap_challenge(SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes())
        .unwrap();
    assert!(matches!(
        b.bootstrap_finish(
            &second.id,
            &sign(&second, &SigningKey::from_bytes(&[9; 32]))
        ),
        Err(Error::Storage)
    ));
}
#[test]
fn monotonic_deadlines_and_fresh_owner_proof_are_enforced() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let o = SigningKey::from_bytes(&[10; 32]);
    let t = owner(&mut a, &o);
    a.sessions
        .get_mut(&token_digest(t.expose()).unwrap())
        .unwrap()
        .authenticated = Instant::now() - REAUTH_TTL;
    assert_eq!(a.open_pairing(t.expose()), Err(Error::Expired));
    a.sessions
        .get_mut(&token_digest(t.expose()).unwrap())
        .unwrap()
        .deadline = Instant::now();
    assert_eq!(
        a.authorize(t.expose(), Scope::StatusRead),
        Err(Error::Forbidden)
    );
    let c = a.login_challenge(o.verifying_key().to_bytes()).unwrap();
    a.challenges.get_mut(&c.id).unwrap().deadline = Instant::now();
    assert!(a.login_finish(&c.id, &sign(&c, &o)).is_err());
}
#[test]
fn backup_cannot_restore_revoked_authority_or_reopen_bootstrap() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    let backup = directory();
    let restored = directory();
    let o = SigningKey::from_bytes(&[11; 32]);
    let t = owner(&mut a, &o);
    let k = SigningKey::from_bytes(&[12; 32]);
    let key = k.verifying_key().to_bytes();
    pair(&mut a, &t, &k, Kind::Client, &[Scope::SmsSend]);
    s.handle().backup(backup.path()).unwrap();
    a.revoke(t.expose(), key).unwrap();
    let r = Store::restore(backup.path(), restored.path()).unwrap();
    let mut b = start(i.path(), &r);
    assert!(!r.handle().principal(key).unwrap().unwrap().active);
    assert!(matches!(b.login_challenge(key), Err(Error::Forbidden)));
    assert!(matches!(
        b.login_challenge(o.verifying_key().to_bytes()),
        Err(Error::Forbidden)
    ));
    assert!(r.handle().bootstrap_owner(key).is_err());
}
#[test]
fn weak_keys_and_unbounded_challenges_fail_closed() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let mut a = start(i.path(), &s);
    assert!(matches!(
        a.bootstrap_challenge([0; 32]),
        Err(Error::InvalidInput)
    ));
    let k = SigningKey::from_bytes(&[13; 32]).verifying_key().to_bytes();
    for _ in 0..31 {
        a.bootstrap_challenge(k).unwrap();
    }
    assert!(matches!(a.bootstrap_challenge(k), Err(Error::Busy)));
}
#[test]
fn mismatched_identity_never_rebinds_database() {
    let d = directory();
    let s = Store::open(d.path()).unwrap();
    let i = directory();
    let a = start(i.path(), &s);
    let key = a.public_key();
    assert!(matches!(
        Auth::new(Identity::open(directory().path()).unwrap(), s.handle()),
        Err(Error::Storage)
    ));
    assert_eq!(start(i.path(), &s).public_key(), key);
}
#[cfg(target_os = "linux")]
#[test]
fn permissive_symlink_and_hardlink_identity_files_are_rejected() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let d = directory();
    Identity::open(d.path()).unwrap();
    let path = d.path().join("identity.seed");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(Identity::open(d.path()), Err(Error::Forbidden)));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let other = directory();
    symlink(&path, other.path().join("identity.seed")).unwrap();
    assert!(matches!(
        Identity::open(other.path()),
        Err(Error::Forbidden)
    ));
    fs::hard_link(&path, other.path().join("linked")).unwrap();
    assert!(matches!(Identity::open(d.path()), Err(Error::Forbidden)));
}
