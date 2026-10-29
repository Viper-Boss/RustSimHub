// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! D-21 proposal evidence: exercises current public APIs, not a proposed authorizer.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{Auth, Token};
use rustsimhub_policy::{Kind, Principal, Scope, ALL_SCOPES};
use rustsimhub_storage::Store;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture {
    root: PathBuf,
    identity: PathBuf,
    state: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let base = PathBuf::from(std::env::var_os("RSH_AUTHORIZER_CASE_ROOT").unwrap());
        let root = base.join(format!("case-{}", NEXT.fetch_add(1, Ordering::Relaxed)));
        let identity = root.join("identity");
        let state = root.join("state");
        for path in [&root, &identity, &state] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            root,
            identity,
            state,
        }
    }
    fn dir(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    fn open(&self) -> (Store, Auth) {
        let store = Store::open(&self.state).unwrap();
        let auth = Auth::open(&self.identity, store.handle()).unwrap();
        (store, auth)
    }
}
fn login(auth: &mut Auth, key: &SigningKey) -> Token {
    let proof = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.login_finish(&proof.id, &key.sign(&proof.transcript).to_bytes())
        .unwrap()
}
fn owner(auth: &mut Auth, key: &SigningKey) -> Token {
    let proof = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&proof.id, &key.sign(&proof.transcript).to_bytes())
        .unwrap();
    login(auth, key)
}
fn approve(auth: &mut Auth, token: &Token, key: &SigningKey, kind: Kind) -> Principal {
    auth.open_pairing(token.expose()).unwrap();
    let public = key.verifying_key().to_bytes();
    let scopes = [Scope::NetworkManage];
    let proof = auth.pairing_challenge(public, kind, &scopes).unwrap();
    let id = auth
        .pairing_finish(&proof.id, &key.sign(&proof.transcript).to_bytes())
        .unwrap();
    auth.approve(token.expose(), &id, public, kind, &scopes)
        .unwrap()
}

#[test]
fn independent_recovery_key_has_no_login_owner_or_bootstrap_authority() {
    let f = Fixture::new();
    let (store, mut auth) = f.open();
    let owner_key = SigningKey::from_bytes(&[31; 32]);
    let token = owner(&mut auth, &owner_key);
    let before = store.handle().principals().unwrap();
    let independent = SigningKey::from_bytes(&[32; 32]);
    let public = independent.verifying_key().to_bytes();
    assert!(auth.login_challenge(public).is_err());
    assert!(auth.recovery_challenge(public).is_err());
    // A valid independent signature is still unable to replace the existing owner.
    let c = auth.bootstrap_challenge(public).unwrap();
    assert!(auth
        .bootstrap_finish(&c.id, &independent.sign(&c.transcript).to_bytes())
        .is_err());
    assert!(auth.current_principal(public).is_err());
    assert_eq!(store.handle().principals().unwrap(), before);
    assert_eq!(auth.recent_owner(token.expose()).unwrap(), before[0]);
}

#[test]
fn normal_network_client_and_nas_cannot_be_used_as_recent_owner() {
    let f = Fixture::new();
    let (store, mut auth) = f.open();
    let owner_key = SigningKey::from_bytes(&[33; 32]);
    let token = owner(&mut auth, &owner_key);
    let client_key = SigningKey::from_bytes(&[34; 32]);
    let client = approve(&mut auth, &token, &client_key, Kind::Client);
    let client_token = login(&mut auth, &client_key);
    // Existing broad network delegation really works; it is not a wifi.recovery grant.
    assert_eq!(auth.network_manager(client_token.expose()).unwrap(), client);
    assert!(auth.recent_owner(client_token.expose()).is_err());
    assert!(auth.open_pairing(client_token.expose()).is_err());
    assert!(auth
        .revoke(client_token.expose(), owner_key.verifying_key().to_bytes())
        .is_err());
    let nas_key = SigningKey::from_bytes(&[35; 32]);
    let nas = approve(&mut auth, &token, &nas_key, Kind::Nas);
    let nas_token = login(&mut auth, &nas_key);
    assert_eq!(
        auth.authorize(nas_token.expose(), Scope::NetworkManage)
            .unwrap(),
        nas
    );
    assert!(auth.network_manager(nas_token.expose()).is_err());
    assert!(auth.recent_owner(nas_token.expose()).is_err());
    assert_eq!(store.handle().principals().unwrap().len(), 3);
    assert_eq!(
        store
            .handle()
            .principals()
            .unwrap()
            .iter()
            .filter(|p| p.kind == Kind::Owner)
            .count(),
        1
    );
}

#[test]
fn revocation_restart_and_reapproval_do_not_resurrect_old_session_or_proof() {
    let f = Fixture::new();
    let (store, mut auth) = f.open();
    let owner_key = SigningKey::from_bytes(&[36; 32]);
    let token = owner(&mut auth, &owner_key);
    let key = SigningKey::from_bytes(&[37; 32]);
    let first = approve(&mut auth, &token, &key, Kind::Client);
    let old_token = login(&mut auth, &key);
    let pending = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.revoke(token.expose(), key.verifying_key().to_bytes())
        .unwrap();
    assert!(auth.network_manager(old_token.expose()).is_err());
    assert!(auth
        .login_finish(&pending.id, &key.sign(&pending.transcript).to_bytes())
        .is_err());
    let inactive = store
        .handle()
        .principal(key.verifying_key().to_bytes())
        .unwrap()
        .unwrap();
    assert!(!inactive.active);
    assert!(inactive.generation > first.generation);
    let realm = auth.durable_realm().unwrap();
    drop(auth);
    drop(store);
    let (store, mut auth) = f.open();
    assert_eq!(auth.durable_realm().unwrap(), realm);
    assert!(auth.network_manager(old_token.expose()).is_err());
    assert!(auth
        .login_challenge(key.verifying_key().to_bytes())
        .is_err());
    assert_eq!(
        store
            .handle()
            .principal(key.verifying_key().to_bytes())
            .unwrap()
            .unwrap(),
        inactive
    );
    let owner_token = login(&mut auth, &owner_key);
    let second = approve(&mut auth, &owner_token, &key, Kind::Client);
    assert_eq!(second.id, first.id);
    assert!(second.generation > inactive.generation);
    assert!(auth.network_manager(old_token.expose()).is_err());
    assert!(auth
        .login_finish(&pending.id, &key.sign(&pending.transcript).to_bytes())
        .is_err());
    let new_token = login(&mut auth, &key);
    assert_eq!(auth.network_manager(new_token.expose()).unwrap(), second);
}

#[test]
fn business_restore_keeps_device_pin_but_rotates_realm_and_deactivates_grants_source() {
    let f = Fixture::new();
    let (store, mut auth) = f.open();
    let owner_key = SigningKey::from_bytes(&[38; 32]);
    let token = owner(&mut auth, &owner_key);
    let key = SigningKey::from_bytes(&[39; 32]);
    let client = approve(&mut auth, &token, &key, Kind::Client);
    let client_token = login(&mut auth, &key);
    let pin = auth.public_key();
    let realm = auth.durable_realm().unwrap();
    let before = store.handle().principals().unwrap();
    let backup = f.dir("backup");
    store.handle().backup(&backup).unwrap();
    drop(auth);
    drop(store);
    let restored = Store::restore(&backup, f.dir("restored")).unwrap();
    let mut auth = Auth::open(&f.identity, restored.handle()).unwrap();
    assert_eq!(auth.public_key(), pin);
    assert_ne!(auth.durable_realm().unwrap(), realm);
    assert!(auth.recent_owner(token.expose()).is_err());
    assert!(auth.network_manager(client_token.expose()).is_err());
    assert!(auth
        .login_challenge(key.verifying_key().to_bytes())
        .is_err());
    let after = restored.handle().principals().unwrap();
    assert_eq!(after.len(), before.len());
    for p in &after {
        let old = before.iter().find(|old| old.id == p.id).unwrap();
        assert_eq!(p.public_key, old.public_key);
        assert!(!p.active);
        assert!(p.generation > old.generation);
    }
    // Offline recovery is an owner-key maintenance path, not recovery-key delegation.
    assert!(auth.recovery_challenge(client.public_key).is_err());
    let c = auth
        .recovery_challenge(owner_key.verifying_key().to_bytes())
        .unwrap();
    let recovered = auth
        .recovery_finish(&c.id, &owner_key.sign(&c.transcript).to_bytes())
        .unwrap();
    assert_eq!(recovered.kind, Kind::Owner);
    assert_eq!(
        restored
            .handle()
            .principals()
            .unwrap()
            .iter()
            .filter(|p| p.active)
            .count(),
        1
    );
    assert!(auth.network_manager(client_token.expose()).is_err());
}

#[test]
fn existing_scope_mask_and_schema_cannot_store_an_extra_recovery_bit() {
    let scopes = [
        Scope::StatusRead,
        Scope::SmsRead,
        Scope::SmsSend,
        Scope::CallsRead,
        Scope::CallsControl,
        Scope::MediaUse,
        Scope::NetworkManage,
        Scope::ClientsManage,
        Scope::UpdateManage,
        Scope::SystemManage,
    ];
    assert_eq!(ALL_SCOPES, 1023);
    for (bit, scope) in scopes.into_iter().enumerate() {
        assert_eq!(scope as usize, bit);
    }
    assert!(serde_json::from_str::<Scope>("\"wifi.recovery\"").is_err());
    assert!(serde_json::from_str::<Kind>("\"recovery\"").is_err());
    let f = Fixture::new();
    let (store, mut auth) = f.open();
    let key = SigningKey::from_bytes(&[40; 32]);
    let token = owner(&mut auth, &key);
    let p = auth.recent_owner(token.expose()).unwrap();
    let independent = SigningKey::from_bytes(&[41; 32]).verifying_key().to_bytes();
    assert!(store
        .handle()
        .approve_client(p.clone(), independent, Kind::Client, 1 << 10)
        .is_err());
    assert!(store.handle().principal(independent).unwrap().is_none());
    drop(auth);
    drop(store);
    // Only this now-closed synthetic DB is opened for a constraint demonstration.
    let db = rusqlite::Connection::open(f.state.join("store.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert!(db
        .execute(
            "INSERT INTO principals VALUES ('synthetic-recovery',?1,'client',1024,1,1)",
            [independent.as_slice()]
        )
        .is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM principals", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    drop(db);
    let (store, _) = f.open();
    assert_eq!(store.handle().principals().unwrap(), vec![p]);
}

#[test]
fn payload_cannot_assert_ble_channel_peer_credentials_or_recovery_capability() {
    let body = json!({"protocol":2,"id":"synthetic-request","command":{
        "op":"wifi_confirm","token":"a".repeat(64),"trial_id":"b".repeat(64)}});
    let encoded = serde_json::to_vec(&body).unwrap();
    assert!(rustsimhub_wire::parse_auth_request(&encoded).is_ok());
    for (field, value) in [
        ("channel", json!("ble")),
        ("uid", json!(1000)),
        ("peer_proof", json!({"authenticated":true})),
        (
            "recovery_context",
            json!({"grant_generation":1,"authorized":true}),
        ),
    ] {
        let mut injected = body.clone();
        injected["command"][field] = value;
        assert!(
            rustsimhub_wire::parse_auth_request(&serde_json::to_vec(&injected).unwrap()).is_err()
        );
    }
    for op in ["recovery_grant", "recovery_authorize", "recover_owner"] {
        let mut injected = body.clone();
        injected["command"]["op"] = json!(op);
        assert!(
            rustsimhub_wire::parse_auth_request(&serde_json::to_vec(&injected).unwrap()).is_err()
        );
    }
}
