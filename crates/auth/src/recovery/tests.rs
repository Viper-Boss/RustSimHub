// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{token_digest, Identity, Token, REAUTH_TTL};
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_policy::{Kind, ALL_SCOPES};
use rustsimhub_storage::Store;
use std::{cell::Cell, time::Instant};
use tempfile::TempDir;

pub(super) struct TestKeyRegistry(pub(super) Cell<usize>);
impl RecoveryKeyVerifierV1 for TestKeyRegistry {
    fn validate_recovery_key(&self, key: &RecoveryKey) -> Result<()> {
        self.0.set(self.0.get() + 1);
        if key.profile != "test-only-v1" || key.public_key != vec![42; 33] {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
fn directory() -> TempDir {
    let d = TempDir::new().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    d
}
pub(super) fn key() -> RecoveryKey {
    RecoveryKey {
        profile: "test-only-v1".into(),
        public_key: vec![42; 33],
    }
}
pub(super) fn limits() -> RecoveryLimits {
    RecoveryLimits {
        session_ms: 120_000,
        request_budget: 64,
    }
}
fn login(auth: &mut Auth, k: &SigningKey) -> Token {
    let c = auth.login_challenge(k.verifying_key().to_bytes()).unwrap();
    auth.login_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap()
}
pub(super) fn fixture(recovery: bool) -> (TempDir, TempDir, Store, Auth, SigningKey, Token) {
    let dir = directory();
    let identity = directory();
    let store = if recovery {
        Store::open_recovery(dir.path()).unwrap()
    } else {
        Store::open(dir.path()).unwrap()
    };
    let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[31; 32]);
    let c = auth
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let token = login(&mut auth, &owner);
    (dir, identity, store, auth, owner, token)
}

#[test]
fn actual_recent_owner_can_manage_grants_without_minting_a_normal_principal() {
    let (_dir, _identity, store, mut auth, _owner, token) = fixture(true);
    let registry = TestKeyRegistry(Cell::new(0));
    let before = store.handle().principals().unwrap();
    let grant = auth
        .grant_wifi_recovery(token.expose(), key(), limits(), 0, Some(&registry))
        .unwrap();
    assert_eq!(registry.0.get(), 1);
    assert_eq!(
        auth.wifi_recovery_grants(token.expose()).unwrap(),
        vec![grant.clone()]
    );
    let revoked = auth
        .revoke_wifi_recovery(token.expose(), grant.id, grant.generation)
        .unwrap();
    assert!(!revoked.active);
    assert_eq!(revoked.generation, 2);
    assert_eq!(auth.wifi_recovery_audit(token.expose()).unwrap().len(), 2);
    assert_eq!(store.handle().principals().unwrap(), before);
    assert_eq!(ALL_SCOPES, 1023);
}

#[test]
fn missing_or_unknown_key_registry_never_enables_recovery() {
    let (_dir, _identity, store, mut auth, _owner, token) = fixture(true);
    let registry = TestKeyRegistry(Cell::new(0));
    assert!(matches!(
        auth.grant_wifi_recovery(token.expose(), key(), limits(), 0, None),
        Err(Error::Forbidden)
    ));
    let mut unknown = key();
    unknown.profile = "unreviewed-profile".into();
    assert!(matches!(
        auth.grant_wifi_recovery(token.expose(), unknown, limits(), 0, Some(&registry)),
        Err(Error::Forbidden)
    ));
    assert!(auth
        .wifi_recovery_grants(token.expose())
        .unwrap()
        .is_empty());
    assert_eq!(store.handle().principals().unwrap().len(), 1);
    assert!(matches!(
        auth.grant_wifi_recovery(&"f".repeat(64), key(), limits(), 0, Some(&registry)),
        Err(Error::Forbidden)
    ));
    assert_eq!(registry.0.get(), 1); // An invalid normal login never reaches the key adapter.
}

#[test]
fn all_scope_client_and_nas_bearers_are_not_recent_owners() {
    let (_dir, _identity, store, mut auth, _owner, token) = fixture(true);
    let registry = TestKeyRegistry(Cell::new(0));
    let owner = auth.recent_owner(token.expose()).unwrap();
    for (kind, byte) in [(Kind::Client, 32), (Kind::Nas, 33)] {
        let k = SigningKey::from_bytes(&[byte; 32]);
        store
            .handle()
            .approve_client(
                owner.clone(),
                k.verifying_key().to_bytes(),
                kind,
                ALL_SCOPES,
            )
            .unwrap();
        let client = login(&mut auth, &k);
        assert!(matches!(
            auth.grant_wifi_recovery(client.expose(), key(), limits(), 0, Some(&registry)),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            auth.wifi_recovery_grants(client.expose()),
            Err(Error::Forbidden)
        ));
    }
    assert_eq!(registry.0.get(), 0);
}

#[test]
fn expired_interactive_owner_freshness_is_not_extended_by_management_calls() {
    let (_dir, _identity, _store, mut auth, owner, token) = fixture(true);
    let registry = TestKeyRegistry(Cell::new(0));
    let digest = token_digest(token.expose()).unwrap();
    let old = Instant::now() - REAUTH_TTL;
    auth.sessions.get_mut(&digest).unwrap().authenticated = old;
    assert!(matches!(
        auth.grant_wifi_recovery(token.expose(), key(), limits(), 0, Some(&registry)),
        Err(Error::Expired)
    ));
    assert!(matches!(
        auth.wifi_recovery_grants(token.expose()),
        Err(Error::Expired)
    ));
    assert_eq!(auth.sessions[&digest].authenticated, old);
    assert_eq!(registry.0.get(), 0);
    let fresh = login(&mut auth, &owner);
    assert!(auth
        .grant_wifi_recovery(fresh.expose(), key(), limits(), 0, Some(&registry))
        .is_ok());
}

#[test]
fn normal_schema_six_auth_does_not_silently_migrate_when_management_is_called() {
    let (_dir, _identity, store, mut auth, _owner, token) = fixture(false);
    let registry = TestKeyRegistry(Cell::new(0));
    assert!(matches!(
        auth.grant_wifi_recovery(token.expose(), key(), limits(), 0, Some(&registry)),
        Err(Error::Storage)
    ));
    assert!(!store.handle().recovery_enabled().unwrap());
    assert!(auth.authorize(token.expose(), Scope::CallsControl).is_ok());
}

#[test]
fn ordinary_restart_keeps_grants_but_does_not_restore_old_owner_bearers() {
    let (dir, identity, store, mut auth, owner, token) = fixture(true);
    let registry = TestKeyRegistry(Cell::new(0));
    let grant = auth
        .grant_wifi_recovery(token.expose(), key(), limits(), 0, Some(&registry))
        .unwrap();
    drop(auth);
    drop(store);
    let store = Store::open_recovery(dir.path()).unwrap();
    let mut auth = Auth::new(Identity::open(identity.path()).unwrap(), store.handle()).unwrap();
    assert!(matches!(
        auth.wifi_recovery_grants(token.expose()),
        Err(Error::Forbidden)
    ));
    let token = login(&mut auth, &owner);
    assert_eq!(
        auth.wifi_recovery_grants(token.expose()).unwrap(),
        vec![grant]
    );
}
