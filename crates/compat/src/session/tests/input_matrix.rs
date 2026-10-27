// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

#[test]
fn c07_expired_real_session_is_denied_without_extending_the_production_lifetime() {
    let d = dir();
    let key = SigningKey::from_bytes(&[51; 32]);
    let nas = principal(key.verifying_key().to_bytes(), Kind::Nas);
    let owner = principal([7; 32], Kind::Owner);
    let now = 1_800_000_000;
    let mut authority = Authority::open(d.path(), [77; 32], REALM).unwrap();
    let binding = authority
        .bind(&owner, &nas, "nas-matrix", &[Scope::SmsRead])
        .unwrap();
    let request = Request::new(&key, [77; 32], &binding, now, "c07-expiry");
    let issued = authority
        .issue(&nas, request.signed("nas-matrix"), now)
        .unwrap();
    assert_eq!(
        OffsetDateTime::parse(&issued.expires_at, &Rfc3339)
            .unwrap()
            .unix_timestamp(),
        now + 900
    );
    authority
        .authorize(&issued.token, &nas, Scope::SmsRead)
        .unwrap();
    // Only test-owned in-memory state is advanced; authorize/subject stay real.
    for session in authority.sessions.values_mut() {
        session.deadline = Instant::now() - Duration::from_secs(1);
    }
    let bytes = fs::read(d.path().join("legacy.sqlite")).unwrap();
    assert!(matches!(
        authority.subject(&issued.token),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        authority.authorize(&issued.token, &nas, Scope::SmsRead),
        Err(Error::Forbidden)
    ));
    assert_eq!(fs::read(d.path().join("legacy.sqlite")).unwrap(), bytes);
    let fresh_request = Request::new(&key, [77; 32], &binding, now, "c07-after-expiry");
    let fresh = authority
        .issue(&nas, fresh_request.signed("nas-matrix"), now)
        .unwrap();
    authority
        .authorize(&fresh.token, &nas, Scope::SmsRead)
        .unwrap();
    assert!(authority.subject(&issued.token).is_err());
}

#[test]
fn c07_wrong_nas_identity_token_scope_and_signed_time_matrix_fails_closed() {
    let d = dir();
    let key = SigningKey::from_bytes(&[52; 32]);
    let nas = principal(key.verifying_key().to_bytes(), Kind::Nas);
    let owner = principal([7; 32], Kind::Owner);
    let now = 1_800_000_000;
    let mut authority = Authority::open(d.path(), [77; 32], REALM).unwrap();
    let binding = authority
        .bind(&owner, &nas, "nas-matrix", &[Scope::SmsRead])
        .unwrap();
    let request = Request::new(&key, [77; 32], &binding, now, "c07-valid");
    let token = authority
        .issue(&nas, request.signed("nas-matrix"), now)
        .unwrap();
    let mut variants = vec![principal([99; 32], Kind::Nas)];
    let mut wrong = nas.clone();
    wrong.kind = Kind::Client;
    variants.push(wrong);
    let mut wrong = nas.clone();
    wrong.kind = Kind::Owner;
    variants.push(wrong);
    let mut wrong = nas.clone();
    wrong.active = false;
    variants.push(wrong);
    let mut wrong = nas.clone();
    wrong.generation += 1;
    variants.push(wrong);
    let mut wrong = nas.clone();
    wrong.scopes = 1 << (Scope::StatusRead as u16);
    variants.push(wrong);
    let before = fs::read(d.path().join("legacy.sqlite")).unwrap();
    for current in variants {
        assert!(matches!(
            authority.authorize(&token.token, &current, Scope::SmsRead),
            Err(Error::Forbidden)
        ));
    }
    for scope in [Scope::SmsSend, Scope::CallsControl, Scope::CallsRead] {
        assert!(matches!(
            authority.authorize(&token.token, &nas, scope),
            Err(Error::Forbidden)
        ));
    }
    for value in [
        String::new(),
        "f".repeat(64),
        "A".repeat(64),
        "a".repeat(65),
        "\0".into(),
    ] {
        assert!(matches!(authority.subject(&value), Err(Error::Forbidden)));
    }
    for delta in [-301, 301] {
        let request = Request::new(
            &key,
            [77; 32],
            &binding,
            now + delta,
            &format!("c07-skew-{delta}"),
        );
        assert!(matches!(
            authority.issue(&nas, request.signed("nas-matrix"), now),
            Err(Error::Clock)
        ));
    }
    assert_eq!(fs::read(d.path().join("legacy.sqlite")).unwrap(), before);
    for delta in [-300, 300] {
        let request = Request::new(
            &key,
            [77; 32],
            &binding,
            now + delta,
            &format!("c07-boundary-{delta}"),
        );
        assert!(authority
            .issue(&nas, request.signed("nas-matrix"), now)
            .is_ok());
    }
    authority.revoke(&owner, "nas-matrix").unwrap();
    let revoked = fs::read(d.path().join("legacy.sqlite")).unwrap();
    assert!(authority.subject(&token.token).is_err());
    assert_eq!(fs::read(d.path().join("legacy.sqlite")).unwrap(), revoked);
}

#[test]
fn c07_wrong_session_store_version_never_reinitializes_existing_bindings() {
    for pragma in ["PRAGMA user_version=2", "PRAGMA application_id=0"] {
        let d = dir();
        let nas = principal([51; 32], Kind::Nas);
        let owner = principal([7; 32], Kind::Owner);
        let mut authority = Authority::open(d.path(), [77; 32], REALM).unwrap();
        authority
            .bind(&owner, &nas, "nas-matrix", &[Scope::SmsRead])
            .unwrap();
        drop(authority);
        let path = d.path().join("legacy.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(pragma).unwrap();
        drop(db);
        let bytes = fs::read(&path).unwrap();
        assert!(matches!(
            Authority::open(d.path(), [77; 32], REALM),
            Err(Error::Storage)
        ));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
