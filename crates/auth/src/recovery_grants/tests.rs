// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_storage::Store;
use std::{fs, os::unix::fs::PermissionsExt};
use tempfile::TempDir;
fn dir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn login(a: &mut Auth, k: &SigningKey) -> Token {
    let c = a.login_challenge(k.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap()
}
fn fixture() -> (TempDir, TempDir, Store, Auth, SigningKey, Token) {
    let d = dir();
    let i = dir();
    let b = dir();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let k = SigningKey::from_bytes(&[51; 32]);
    let c = a.bootstrap_challenge(k.verifying_key().to_bytes()).unwrap();
    a.bootstrap_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let t = login(&mut a, &k);
    (d, i, s, a, k, t)
}

#[test]
fn grant_is_independent_and_recent_owner_is_required_for_all_management() {
    let (_d, _i, s, mut a, k, t) = fixture();
    let key = SigningKey::from_bytes(&[52; 32]).verifying_key().to_bytes();
    let before = s.handle().principals().unwrap();
    assert!(a
        .grant_recovery_key(&"0".repeat(64), key, 40000, 0, id(1))
        .is_err());
    assert!(a
        .grant_recovery_key(t.expose(), [0; 32], 40000, 0, id(1))
        .is_err());
    assert!(a
        .grant_recovery_key(t.expose(), k.verifying_key().to_bytes(), 40000, 0, id(1))
        .is_err());
    assert!(a
        .grant_recovery_key(t.expose(), key, 40001, 0, id(1))
        .is_err());
    let g = a
        .grant_recovery_key(t.expose(), key, 40000, 0, id(1))
        .unwrap();
    assert!(g.active);
    assert_eq!(a.recovery_key_request(t.expose(), id(1)).unwrap(), Some(g));
    assert!(a.login_challenge(key).is_err());
    assert!(a.recovery_challenge(key).is_err());
    assert_eq!(s.handle().principals().unwrap(), before);
    let digest = token_digest(t.expose()).unwrap();
    a.sessions.get_mut(&digest).unwrap().authenticated = Instant::now() - REAUTH_TTL;
    assert!(matches!(
        a.grant_recovery_key(t.expose(), key, 40000, 0, id(2)),
        Err(Error::Expired)
    ));
    assert!(matches!(
        a.revoke_recovery_key(t.expose(), key, 1, id(2)),
        Err(Error::Expired)
    ));
    assert!(matches!(
        a.recovery_key(t.expose(), key),
        Err(Error::Expired)
    ));
    assert!(matches!(
        a.recovery_key_request(t.expose(), id(1)),
        Err(Error::Expired)
    ));
}

#[test]
fn normal_clients_cannot_administer_recovery_and_recovery_key_cannot_become_client() {
    let (_d, _i, s, mut a, _k, t) = fixture();
    let client = SigningKey::from_bytes(&[53; 32]);
    let owner = a.recent_owner(t.expose()).unwrap();
    s.handle()
        .approve_client(
            owner,
            client.verifying_key().to_bytes(),
            Kind::Client,
            ALL_SCOPES,
        )
        .unwrap();
    let ct = login(&mut a, &client);
    let key = SigningKey::from_bytes(&[54; 32]).verifying_key().to_bytes();
    assert!(a.network_manager(ct.expose()).is_ok());
    assert!(a
        .grant_recovery_key(ct.expose(), key, 40000, 0, id(1))
        .is_err());
    assert!(a.revoke_recovery_key(ct.expose(), key, 1, id(1)).is_err());
    assert!(a.recovery_key(ct.expose(), key).is_err());
    let g = a
        .grant_recovery_key(t.expose(), key, 40000, 0, id(1))
        .unwrap();
    a.open_pairing(t.expose()).unwrap();
    let challenge = a
        .pairing_challenge(key, Kind::Client, &[Scope::NetworkManage])
        .unwrap();
    let recovery = SigningKey::from_bytes(&[54; 32]);
    let pending = a
        .pairing_finish(
            &challenge.id,
            &recovery.sign(&challenge.transcript).to_bytes(),
        )
        .unwrap();
    assert!(a
        .approve(
            t.expose(),
            &pending,
            key,
            Kind::Client,
            &[Scope::NetworkManage]
        )
        .is_err());
    let off = a
        .revoke_recovery_key(t.expose(), key, g.generation, id(2))
        .unwrap();
    assert!(!off.active);
    assert!(a
        .grant_recovery_key(
            t.expose(),
            client.verifying_key().to_bytes(),
            40000,
            0,
            id(3)
        )
        .is_err());
}

#[test]
fn reboot_and_real_restore_require_fresh_owner_and_never_turn_grants_into_principals() {
    let (d, i, s, mut a, k, t) = fixture();
    let key = SigningKey::from_bytes(&[55; 32]).verifying_key().to_bytes();
    let g = a
        .grant_recovery_key(t.expose(), key, 40000, 0, id(1))
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::open_recovery_grants(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    assert!(a.recovery_key(t.expose(), key).is_err());
    let next = login(&mut a, &k);
    assert_eq!(a.recovery_key(next.expose(), key).unwrap(), Some(g.clone()));
    let backup = dir();
    s.handle().backup(backup.path()).unwrap();
    drop(a);
    drop(s);
    let target = dir();
    let s = Store::restore_recovery_grants(backup.path(), target.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    assert_ne!(a.durable_realm().unwrap(), g.realm);
    assert!(a.recovery_key(next.expose(), key).is_err());
    assert!(a.recovery_challenge(key).is_err());
    let c = a.recovery_challenge(k.verifying_key().to_bytes()).unwrap();
    a.recovery_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap();
    let owner = login(&mut a, &k);
    assert!(a
        .recovery_key_request(owner.expose(), id(1))
        .unwrap()
        .is_none());
    let renewed = a
        .grant_recovery_key(owner.expose(), key, 40000, g.generation, id(2))
        .unwrap();
    assert!(renewed.generation > g.generation);
    assert_ne!(renewed.realm, g.realm);
    assert_eq!(s.handle().principals().unwrap().len(), 1);
    assert!(a.login_challenge(key).is_err());
}

#[test]
fn default_store_even_with_feature_cannot_create_recovery_grants() {
    let d = dir();
    let i = dir();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let k = SigningKey::from_bytes(&[56; 32]);
    let c = a.bootstrap_challenge(k.verifying_key().to_bytes()).unwrap();
    a.bootstrap_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap();
    let t = login(&mut a, &k);
    let key = SigningKey::from_bytes(&[57; 32]).verifying_key().to_bytes();
    assert!(a
        .grant_recovery_key(t.expose(), key, 40000, 0, id(1))
        .is_err());
    assert!(a.recovery_key(t.expose(), key).is_err());
    assert_eq!(s.handle().principals().unwrap().len(), 1);
}

#[test]
fn auth_startup_rejects_invalid_key_material_in_trusted_store_records() {
    let (_d, i, s, mut a, _k, t) = fixture();
    let owner = a.recent_owner(t.expose()).unwrap();
    // Trusted storage does not implement Ed25519; Auth must reject bad restored key material.
    s.handle()
        .change_recovery_grant(
            owner,
            [0; 32],
            0,
            id(1),
            RecoveryChange::Grant {
                max_trial_ms: 40000,
            },
        )
        .unwrap();
    drop(a);
    assert!(Auth::open(i.path(), s.handle()).is_err());
}
