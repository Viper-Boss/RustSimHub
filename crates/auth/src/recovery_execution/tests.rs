// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic A adapter only in cfg(test); actual Auth, Identity and Store9.
use super::*;
use crate::recovery_capability::{
    RecoveryConnection, RecoveryProofContext, RecoveryRequestVerifierV1, VerifiedRecoveryRequest,
};
use crate::Token;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_storage::Store;
use std::{fs, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};
use tempfile::TempDir;
use zeroize::Zeroizing;
mod ledger;
mod response;
mod trial;
fn dir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
const BODY: &[u8] = b"{\"ssid\":\"SYNTHETIC_NET\",\"password\":\"SYNTHETIC_PASSWORD\"}";
struct Fixture {
    a: Auth,
    s: Store,
    _d: TempDir,
    i: TempDir,
    token: Token,
    grant: RecoveryGrant,
}
fn login(a: &mut Auth) -> Token {
    let owner = SigningKey::from_bytes(&[61; 32]);
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap()
}
fn fixture(nine: bool) -> Fixture {
    let d = dir();
    let i = dir();
    let b = dir();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[61; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let token = login(&mut a);
    let key = SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes();
    let grant = a
        .grant_recovery_key(token.expose(), key, 40000, 0, id(1))
        .unwrap();
    if !nine {
        return Fixture {
            a,
            s,
            _d: d,
            i,
            token,
            grant,
        };
    }
    drop(a);
    drop(s);
    let backup = dir();
    Store::migrate_recovery_execution(d.path(), backup.path()).unwrap();
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let token = login(&mut a);
    Fixture {
        a,
        s,
        _d: d,
        i,
        token,
        grant,
    }
}
struct Adapter {
    action: RecoveryAction,
    body: Vec<u8>,
    ttl: Duration,
}
impl RecoveryRequestVerifierV1 for Adapter {
    fn verify_request(
        &self,
        c: &RecoveryProofContext,
        e: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        let request: [u8; 16] = e.try_into().map_err(|_| Error::InvalidInput)?;
        VerifiedRecoveryRequest::from_verified_adapter(
            c.binding().clone(),
            vec![8; 16],
            request,
            self.action,
            Zeroizing::new(self.body.clone()),
            self.ttl,
        )
    }
}
fn install(
    f: &mut Fixture,
    action: RecoveryAction,
    body: &[u8],
    ttl: Duration,
) -> RecoveryConnection {
    f.a.set_recovery_verifier(Some(Arc::new(Adapter {
        action,
        body: body.to_vec(),
        ttl,
    })))
    .unwrap();
    f.a.open_recovery_connection_from_trusted_transport([11; 32])
        .unwrap()
}
fn authorize(f: &mut Fixture, connection: &RecoveryConnection, n: u8) -> AuthorizedRecoveryRequest {
    let pending =
        f.a.prepare_recovery_verification(connection, f.grant.clone(), &[n; 16])
            .unwrap();
    f.a.finish_recovery_verification(connection, pending.verify())
        .unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    let (chunks, remainder) = s.as_bytes().as_chunks::<2>();
    assert!(remainder.is_empty());
    chunks
        .iter()
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
fn binding(v: &serde_json::Value) -> RecoveryGrant {
    RecoveryGrant {
        id: v["grant_id"].as_str().unwrap().into(),
        independent_public_key: [34; 32],
        device_public_key: [33; 32],
        realm: v["realm"].as_str().unwrap().into(),
        owner_id: v["owner_id"].as_str().unwrap().into(),
        owner_generation: v["owner_generation"].as_u64().unwrap(),
        generation: v["grant_generation"].as_u64().unwrap(),
        active: true,
        max_trial_ms: v["max_trial_ms"].as_u64().unwrap() as u32,
    }
}
#[test]
fn independent_std_reference_vectors_bind_exact_binary_transcript() {
    let v: serde_json::Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let key: [u8; 32] = unhex(v["derived_key_hex"].as_str().unwrap())
        .try_into()
        .unwrap();
    let g = binding(&v["binding"]);
    let request = uuid::Uuid::parse_str(v["request_id"].as_str().unwrap())
        .unwrap()
        .into_bytes();
    for c in v["cases"].as_array().unwrap() {
        let action = match c["action"].as_str().unwrap() {
            "begin_wifi" => RecoveryAction::BeginWifi,
            "confirm_wifi" => RecoveryAction::ConfirmWifi,
            "cancel_wifi" => RecoveryAction::CancelWifi,
            _ => unreachable!(),
        };
        let digest = fingerprint(
            &key,
            &g,
            request,
            action,
            c["target_trial"].as_str(),
            &unhex(c["body_hex"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(hex(&digest), c["fingerprint_hex"].as_str().unwrap());
        let mut reference = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        reference.update(&unhex(c["transcript_hex"].as_str().unwrap()));
        assert_eq!(
            digest.as_slice(),
            reference.finalize().into_bytes().as_slice()
        );
    }
}
#[test]
fn every_durable_binding_field_changes_the_commitment() {
    let v: serde_json::Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let g = binding(&v["binding"]);
    let key = [77; 32];
    let base = fingerprint(&key, &g, [1; 16], RecoveryAction::BeginWifi, None, BODY).unwrap();
    let mut variants = Vec::new();
    let mut c = g.clone();
    c.realm = id(99);
    variants.push(c);
    let mut c = g.clone();
    c.device_public_key[0] ^= 1;
    variants.push(c);
    let mut c = g.clone();
    c.independent_public_key[0] ^= 1;
    variants.push(c);
    let mut c = g.clone();
    c.owner_id = id(99);
    variants.push(c);
    let mut c = g.clone();
    c.owner_generation += 1;
    variants.push(c);
    let mut c = g.clone();
    c.id = id(99);
    variants.push(c);
    let mut c = g.clone();
    c.generation += 1;
    variants.push(c);
    let mut c = g.clone();
    c.max_trial_ms -= 1;
    variants.push(c);
    for changed in variants {
        assert_ne!(
            base,
            fingerprint(
                &key,
                &changed,
                [1; 16],
                RecoveryAction::BeginWifi,
                None,
                BODY
            )
            .unwrap()
        );
    }
    assert_ne!(
        base,
        fingerprint(&key, &g, [2; 16], RecoveryAction::BeginWifi, None, BODY).unwrap()
    );
    assert_ne!(
        base,
        fingerprint(
            &key,
            &g,
            [1; 16],
            RecoveryAction::BeginWifi,
            None,
            b"changed"
        )
        .unwrap()
    );
    assert_ne!(
        base,
        fingerprint(
            &[78; 32],
            &g,
            [1; 16],
            RecoveryAction::BeginWifi,
            None,
            BODY
        )
        .unwrap()
    );
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    let confirm = fingerprint(
        &key,
        &g,
        [1; 16],
        RecoveryAction::ConfirmWifi,
        Some(&a),
        BODY,
    )
    .unwrap();
    assert_ne!(
        confirm,
        fingerprint(
            &key,
            &g,
            [1; 16],
            RecoveryAction::ConfirmWifi,
            Some(&b),
            BODY
        )
        .unwrap()
    );
    assert_ne!(
        confirm,
        fingerprint(
            &key,
            &g,
            [1; 16],
            RecoveryAction::CancelWifi,
            Some(&a),
            BODY
        )
        .unwrap()
    );
}
#[test]
fn actual_auth_and_store9_mint_preparation_only_after_live_gate() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let prepared = f.a.prepare_recovery_execution(request, None).unwrap();
    assert_eq!(prepared.body(), BODY);
    assert_eq!(prepared.action(), RecoveryAction::BeginWifi);
    assert_eq!(prepared.target_trial(), None);
    assert_eq!(prepared.grant(), &f.grant);
    assert_eq!(
        prepared.request_id(),
        uuid::Uuid::from_bytes([1; 16]).to_string()
    );
    f.a.check_recovery_execution(&prepared).unwrap();
}
#[test]
fn schema7_never_silently_supplies_persistent_execution_fingerprints() {
    let mut f = fixture(false);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    assert!(matches!(
        f.a.prepare_recovery_execution(request, None),
        Err(Error::Storage)
    ));
}
#[test]
fn connection_nonces_change_but_fingerprint_is_stable_and_old_authority_stays_dead() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let first = f.a.prepare_recovery_execution(request, None).unwrap();
    drop(c);
    assert!(matches!(
        f.a.check_recovery_execution(&first),
        Err(Error::Forbidden)
    ));
    let c =
        f.a.open_recovery_connection_from_trusted_transport([12; 32])
            .unwrap();
    let request = authorize(&mut f, &c, 1);
    let second = f.a.prepare_recovery_execution(request, None).unwrap();
    assert_eq!(first.fingerprint(), second.fingerprint());
    f.a.check_recovery_execution(&second).unwrap();
}
#[test]
fn restart_with_same_private_identity_keeps_fingerprint_without_cross_auth_authority() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let first = f.a.prepare_recovery_execution(request, None).unwrap();
    drop(c);
    let Fixture {
        a,
        s,
        _d,
        i,
        token,
        grant,
    } = f;
    drop(a);
    drop(s);
    let s = Store::open_recovery_execution(_d.path()).unwrap();
    let a = Auth::open(i.path(), s.handle()).unwrap();
    let mut f = Fixture {
        a,
        s,
        _d,
        i,
        token,
        grant,
    };
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let second = f.a.prepare_recovery_execution(request, None).unwrap();
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert!(matches!(
        f.a.check_recovery_execution(&first),
        Err(Error::Forbidden)
    ));
}
#[test]
fn changed_authenticated_body_and_cross_auth_request_are_refused() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let mut request = authorize(&mut f, &c, 1);
    request.body = Zeroizing::new(b"changed".to_vec());
    assert!(matches!(
        f.a.prepare_recovery_execution(request, None),
        Err(Error::Forbidden)
    ));
    let request = authorize(&mut f, &c, 2);
    let other = fixture(true);
    assert!(matches!(
        other.a.prepare_recovery_execution(request, None),
        Err(Error::Forbidden)
    ));
}
#[test]
fn revoked_grant_and_replaced_verifier_invalidate_preparation() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let prepared = f.a.prepare_recovery_execution(request, None).unwrap();
    f.a.revoke_recovery_key(f.token.expose(), f.grant.independent_public_key, 1, id(2))
        .unwrap();
    assert!(matches!(
        f.a.check_recovery_execution(&prepared),
        Err(Error::Storage)
    ));
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let prepared = f.a.prepare_recovery_execution(request, None).unwrap();
    f.a.set_recovery_verifier(None).unwrap();
    assert!(matches!(
        f.a.check_recovery_execution(&prepared),
        Err(Error::Forbidden)
    ));
}
#[test]
fn confirmation_cancel_and_target_shape_are_explicit_and_bounded() {
    assert!(check_target(RecoveryAction::BeginWifi, Some(&"a".repeat(64))).is_err());
    assert!(check_target(RecoveryAction::ConfirmWifi, None).is_err());
    assert!(check_target(RecoveryAction::CancelWifi, Some(&"A".repeat(64))).is_err());
    assert!(check_target(RecoveryAction::CancelWifi, Some(&"a".repeat(65))).is_err());
    for action in [RecoveryAction::ConfirmWifi, RecoveryAction::CancelWifi] {
        let mut f = fixture(true);
        let body = format!("{{\"trial\":\"{}\"}}", "a".repeat(64));
        let c = install(&mut f, action, body.as_bytes(), Duration::from_secs(30));
        let request = authorize(&mut f, &c, 1);
        let prepared =
            f.a.prepare_recovery_execution(request, Some("a".repeat(64)))
                .unwrap();
        assert_eq!(prepared.action(), action);
        assert_eq!(prepared.target_trial(), Some("a".repeat(64).as_str()));
    }
}
#[test]
fn readonly_and_disconnect_actions_cannot_mint_write_preparations() {
    for action in [
        RecoveryAction::ReadStatus,
        RecoveryAction::QueryOperation,
        RecoveryAction::Disconnect,
    ] {
        let mut f = fixture(true);
        let c = install(&mut f, action, BODY, Duration::from_secs(30));
        let request = authorize(&mut f, &c, 1);
        assert!(matches!(
            f.a.prepare_recovery_execution(request, None),
            Err(Error::InvalidInput)
        ));
    }
}
#[test]
fn expiration_does_not_refresh_into_a_new_fingerprint_authority() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let mut prepared = f.a.prepare_recovery_execution(request, None).unwrap();
    prepared.authorized.capability.expire_execution_test();
    assert!(matches!(
        f.a.check_recovery_execution(&prepared),
        Err(Error::Expired)
    ));
}
#[test]
fn preparation_writes_neither_plaintext_nor_receipt_to_database_backup() {
    let mut f = fixture(true);
    let c = install(
        &mut f,
        RecoveryAction::BeginWifi,
        BODY,
        Duration::from_secs(30),
    );
    let request = authorize(&mut f, &c, 1);
    let before = dir();
    f.s.handle().backup(before.path()).unwrap();
    let data = fs::read(before.path().join("backup.sqlite3")).unwrap();
    let prepared = f.a.prepare_recovery_execution(request, None).unwrap();
    f.a.check_recovery_execution(&prepared).unwrap();
    let after = dir();
    f.s.handle().backup(after.path()).unwrap();
    let output = fs::read(after.path().join("backup.sqlite3")).unwrap();
    assert_eq!(data, output);
    assert!(!output
        .windows(b"SYNTHETIC_PASSWORD".len())
        .any(|w| w == b"SYNTHETIC_PASSWORD"));
}
