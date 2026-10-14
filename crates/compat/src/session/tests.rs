// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use std::{fs, os::unix::fs::PermissionsExt};
const REALM: &str = "e6c33cf2-3697-4504-ab68-32f0ca6d37a0";
fn dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn principal(key: [u8; 32], kind: Kind) -> Principal {
    Principal {
        id: "synthetic-principal".into(),
        public_key: key,
        kind,
        scopes: rustsimhub_policy::ALL_SCOPES,
        generation: 1,
        active: true,
    }
}
struct Request {
    id: String,
    epoch: String,
    stamp: String,
    nonce: String,
    signature: String,
    proof: String,
    body: Vec<u8>,
}
impl Request {
    fn signed<'a>(&'a self, core_id: &'a str) -> SignedSession<'a> {
        SignedSession {
            core_id,
            device_id: &self.id,
            binding_epoch: &self.epoch,
            timestamp: &self.stamp,
            nonce: &self.nonce,
            signature: &self.signature,
            device_proof: &self.proof,
            body: &self.body,
        }
    }
    fn new(key: &SigningKey, device: [u8; 32], binding: &Binding, now: i64, nonce: &str) -> Self {
        let stamp = OffsetDateTime::from_unix_timestamp(now)
            .unwrap()
            .format(&Rfc3339)
            .unwrap();
        let body = serde_json::to_vec(&serde_json::json!({"core_id":binding.core_id})).unwrap();
        let canonical = signing_bytes("POST", "/v1/session", &stamp, nonce, &body).unwrap();
        Self {
            id: crate::device_id(&device),
            epoch: binding.binding_epoch.clone(),
            stamp,
            nonce: nonce.into(),
            signature: STANDARD.encode(key.sign(&canonical).to_bytes()),
            proof: STANDARD.encode(
                key.sign(&device_bound_bytes(device, &binding.binding_epoch, &canonical).unwrap())
                    .to_bytes(),
            ),
            body,
        }
    }
}
#[test]
fn signed_legacy_sessions_are_approved_device_bound_scope_limited_and_revocable() {
    let d = dir();
    let key = SigningKey::from_bytes(&[45; 32]);
    let nas = principal(key.verifying_key().to_bytes(), Kind::Nas);
    let owner = principal([7; 32], Kind::Owner);
    let mut authority = Authority::open(d.path(), [77; 32], REALM).unwrap();
    assert!(authority
        .bind(&nas, &nas, "nas-main", &[Scope::StatusRead])
        .is_err());
    assert!(authority
        .bind(
            &owner,
            &principal([8; 32], Kind::Client),
            "nas-main",
            &[Scope::StatusRead]
        )
        .is_err());
    assert!(authority
        .bind(&owner, &nas, "nas-main", &[Scope::SmsSend])
        .is_err());
    let binding = authority
        .bind(
            &owner,
            &nas,
            "nas-main",
            &[Scope::StatusRead, Scope::SmsRead],
        )
        .unwrap();
    let request = Request::new(&key, [77; 32], &binding, 1_800_000_000, "unique-nonce-1");
    let token = authority
        .issue(&nas, request.signed("nas-main"), 1_800_000_000)
        .unwrap();
    authority
        .authorize(&token.token, &nas, Scope::SmsRead)
        .unwrap();
    assert!(authority
        .authorize(&token.token, &nas, Scope::CallsRead)
        .is_err());
    assert!(authority
        .authorize(&token.token, &nas, Scope::SmsSend)
        .is_err());
    assert!(matches!(
        authority.issue(&nas, request.signed("nas-main"), 1_800_000_000),
        Err(Error::Replay)
    ));
    let mut revoked = nas.clone();
    revoked.active = false;
    assert!(authority
        .authorize(&token.token, &revoked, Scope::SmsRead)
        .is_err());
    let mut newer = nas.clone();
    newer.generation += 1;
    assert!(authority
        .authorize(&token.token, &newer, Scope::SmsRead)
        .is_err());
    authority.revoke(&owner, "nas-main").unwrap();
    assert!(authority
        .authorize(&token.token, &nas, Scope::SmsRead)
        .is_err());
    let next = authority
        .bind(&owner, &nas, "nas-main", &[Scope::StatusRead])
        .unwrap();
    assert_ne!(next.binding_epoch, binding.binding_epoch);
    assert!(authority
        .authorize(&token.token, &nas, Scope::StatusRead)
        .is_err());
    assert!(authority
        .issue(&nas, request.signed("nas-main"), 1_800_000_000)
        .is_err());
}
#[test]
fn restart_preserves_nonce_fences_but_not_sessions_or_foreign_realms() {
    let d = dir();
    let key = SigningKey::from_bytes(&[46; 32]);
    let nas = principal(key.verifying_key().to_bytes(), Kind::Nas);
    let owner = principal([7; 32], Kind::Owner);
    let now = 1_800_000_000;
    let mut a = Authority::open(d.path(), [77; 32], REALM).unwrap();
    let binding = a
        .bind(&owner, &nas, "nas-main", &[Scope::StatusRead])
        .unwrap();
    let request = Request::new(&key, [77; 32], &binding, now, "nonce-restart");
    let token = a.issue(&nas, request.signed("nas-main"), now).unwrap();
    drop(a);
    let mut a = Authority::open(d.path(), [77; 32], REALM).unwrap();
    assert!(a.subject(&token.token).is_err());
    assert!(matches!(
        a.issue(&nas, request.signed("nas-main"), now),
        Err(Error::Replay)
    ));
    let other = Request::new(&key, [77; 32], &binding, now - 1, "different-nonce");
    assert!(matches!(
        a.issue(&nas, other.signed("nas-main"), now - 1),
        Err(Error::Clock)
    ));
    drop(a);
    assert!(matches!(
        Authority::open(d.path(), [78; 32], REALM),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        Authority::open(d.path(), [77; 32], "8f7a40cf-bf32-4a6a-9789-24f76baedbdc"),
        Err(Error::Forbidden)
    ));
}
#[test]
fn legacy_signature_cannot_cross_devices_binding_epochs_or_modified_bodies() {
    let d = dir();
    let key = SigningKey::from_bytes(&[47; 32]);
    let nas = principal(key.verifying_key().to_bytes(), Kind::Nas);
    let owner = principal([7; 32], Kind::Owner);
    let now = 1_800_000_000;
    let mut a = Authority::open(d.path(), [77; 32], REALM).unwrap();
    let binding = a
        .bind(&owner, &nas, "nas-main", &[Scope::StatusRead])
        .unwrap();
    let mut wrong = Request::new(&key, [78; 32], &binding, now, "cross-device");
    wrong.id = crate::device_id(&[77; 32]);
    assert!(a.issue(&nas, wrong.signed("nas-main"), now).is_err());
    let mut wrong = Request::new(&key, [77; 32], &binding, now, "changed-body");
    wrong.body = b"{\"core_id\":\"nas-other\"}".to_vec();
    assert!(a.issue(&nas, wrong.signed("nas-main"), now).is_err());
    let wrong = Request::new(&key, [77; 32], &binding, now - 301, "old-request");
    assert!(matches!(
        a.issue(&nas, wrong.signed("nas-main"), now),
        Err(Error::Clock)
    ));
    let good = Request::new(
        &key,
        [77; 32],
        &binding,
        now,
        "same-nonce-after-bad-signature",
    );
    assert!(a.issue(&nas, good.signed("nas-main"), now).is_ok());
}
