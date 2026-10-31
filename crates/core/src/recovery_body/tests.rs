// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth/Identity/Store9/decoder/admission. Only A's external proof adapter
//! is synthetic. No kernel provenance, crypto interoperability or Wi-Fi claim.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{
    recovery_capability::{
        RecoveryConnection, RecoveryProofContext, RecoveryRequestVerifierV1,
        VerifiedRecoveryRequest,
    },
    Token,
};
use rustsimhub_storage::{
    RecoveryExecutionProgress as Progress, RecoveryExecutionState as State, RecoveryGrant, Store,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::TempDir;
const BODY: &[u8] = br#"{"ssid":"SYNTHETIC_NET","passphrase":"SYNTHETIC_PASSWORD"}"#;
type Plan = Arc<Mutex<(RecoveryAction, Vec<u8>)>>;
struct Adapter(Plan);
impl RecoveryRequestVerifierV1 for Adapter {
    fn verify_request(
        &self,
        c: &RecoveryProofContext,
        e: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        let (action, body) = self.0.lock().unwrap().clone();
        VerifiedRecoveryRequest::from_verified_adapter(
            c.binding().clone(),
            vec![8; 16],
            e.try_into().map_err(|_| Error::InvalidInput)?,
            action,
            Zeroizing::new(body),
            Duration::from_secs(120),
        )
    }
}
struct Fixture {
    a: Auth,
    s: Store,
    d: TempDir,
    i: TempDir,
    owner: Token,
    grant: RecoveryGrant,
    plan: Plan,
}
fn dir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn id(n: u8) -> String {
    uuid::Uuid::from_bytes([n; 16]).to_string()
}
fn fixture(nine: bool) -> Fixture {
    let (d, i, b) = (dir(), dir(), dir());
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[71; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let grant = a
        .grant_recovery_key(
            token.expose(),
            SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes(),
            40000,
            0,
            id(90),
        )
        .unwrap();
    let (s, mut a) = if nine {
        drop(a);
        drop(s);
        let b = dir();
        Store::migrate_recovery_execution(d.path(), b.path()).unwrap();
        let s = Store::open_recovery_execution(d.path()).unwrap();
        let a = Auth::open(i.path(), s.handle()).unwrap();
        (s, a)
    } else {
        (s, a)
    };
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let plan = Arc::new(Mutex::new((RecoveryAction::BeginWifi, BODY.to_vec())));
    a.set_recovery_verifier(Some(Arc::new(Adapter(plan.clone()))))
        .unwrap();
    Fixture {
        a,
        s,
        d,
        i,
        owner: token,
        grant,
        plan,
    }
}
fn connect(f: &mut Fixture) -> RecoveryConnection {
    f.a.open_recovery_connection_from_trusted_transport([11; 32])
        .unwrap()
}
fn request(
    f: &mut Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    body: &[u8],
) -> AuthorizedRecoveryRequest {
    *f.plan.lock().unwrap() = (action, body.to_vec());
    let p =
        f.a.prepare_recovery_verification(c, f.grant.clone(), &[n; 16])
            .unwrap();
    f.a.finish_recovery_verification(c, p.verify()).unwrap()
}
fn decode(
    f: &mut Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    body: &[u8],
) -> Result<DecodedRecoveryRequest> {
    let r = request(f, c, n, action, body);
    decode_v1(&f.a, r)
}
fn write(
    f: &mut Fixture,
    c: &RecoveryConnection,
    n: u8,
    action: RecoveryAction,
    body: &[u8],
) -> Box<CoreRecoveryWrite> {
    match decode(f, c, n, action, body).unwrap() {
        DecodedRecoveryRequest::Write(w) => w,
        _ => panic!("write expected"),
    }
}
fn pending(w: Box<CoreRecoveryWrite>, a: &Auth) -> CoreRecoveryPending {
    match w.reserve(a).unwrap() {
        CoreRecoveryAdmission::New(p) => p,
        _ => panic!("new private ticket expected"),
    }
}
fn begin(f: &mut Fixture, c: &RecoveryConnection) -> CoreRecoveryTrial {
    let w = write(f, c, 1, RecoveryAction::BeginWifi, BODY);
    pending(w, &f.a)
        .claim(&f.a, Some("a".repeat(64)))
        .unwrap()
        .begin(&f.a)
        .unwrap()
}
fn query(f: &mut Fixture, c: &RecoveryConnection, n: u8, target: &str) -> Box<CoreRecoveryLookup> {
    let body = format!("{{\"target_request_id\":\"{target}\"}}");
    match decode(f, c, n, RecoveryAction::QueryOperation, body.as_bytes()).unwrap() {
        DecodedRecoveryRequest::Lookup(q) => q,
        _ => panic!("query expected"),
    }
}
#[test]
fn actual_begin_credentials_and_trial_authority_remain_bound_through_admission() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    assert_eq!(w.action(), RecoveryAction::BeginWifi);
    assert_eq!(w.request_id(), id(1));
    assert_eq!(w.target_trial(), None);
    assert_eq!(w.credentials().unwrap().ssid(), "SYNTHETIC_NET");
    let claim = pending(w, &f.a).claim(&f.a, Some("a".repeat(64))).unwrap();
    assert_eq!(claim.action(), RecoveryAction::BeginWifi);
    assert_eq!(claim.request_id(), id(1));
    let t = claim.begin(&f.a).unwrap();
    assert_eq!(t.credentials().passphrase(), "SYNTHETIC_PASSWORD");
    assert_eq!(t.authority().trial_id(), "a".repeat(64));
    assert!(t.authority().remaining() <= Duration::from_secs(40));
    f.a.check_recovery_trial(t.authority()).unwrap();
}
#[test]
fn confirm_and_cancel_use_only_the_parsed_original_trial() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let t = begin(&mut f, &c);
    for p in [Progress::Starting, Progress::Connecting, Progress::Ready] {
        f.a.record_recovery_trial_progress_from_trusted_core(t.authority(), p)
            .unwrap();
    }
    for (n, action) in [
        (2, RecoveryAction::ConfirmWifi),
        (3, RecoveryAction::CancelWifi),
    ] {
        let body = format!("{{\"trial_id\":\"{}\"}}", t.authority().trial_id());
        let w = write(&mut f, &c, n, action, body.as_bytes());
        assert_eq!(w.target_trial(), Some(t.authority().trial_id()));
        assert!(w.credentials().is_none());
        let control = pending(w, &f.a)
            .claim(&f.a, None)
            .unwrap()
            .bind_control(&f.a, &t)
            .unwrap();
        f.a.check_recovery_trial_control(t.authority(), &control, action)
            .unwrap();
        let wrong = if n == 2 {
            RecoveryAction::CancelWifi
        } else {
            RecoveryAction::ConfirmWifi
        };
        assert!(f
            .a
            .check_recovery_trial_control(t.authority(), &control, wrong)
            .is_err());
    }
}
#[test]
fn lookup_uses_its_authenticated_target_and_is_byte_preserving() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    let p = pending(w, &f.a);
    let b1 = dir();
    let b2 = dir();
    f.s.handle().backup(b1.path()).unwrap();
    let q = query(&mut f, &c, 2, &id(1));
    assert_eq!(q.target_request_id(), id(1));
    assert_eq!(q.lookup(&f.a).unwrap().unwrap().state(), State::Reserved);
    assert!(query(&mut f, &c, 3, &id(99))
        .lookup(&f.a)
        .unwrap()
        .is_none());
    f.s.handle().backup(b2.path()).unwrap();
    assert_eq!(
        fs::read(b1.path().join("backup.sqlite3")).unwrap(),
        fs::read(b2.path().join("backup.sqlite3")).unwrap()
    );
    drop(p);
}
#[test]
fn duplicate_unknown_role_and_normal_authcommand_fields_are_rejected() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let bad = [
        br#"{"ssid":"a","ssid":"b","passphrase":"12345678"}"#.as_slice(),
        br#"{"ssid":"a","passphrase":"12345678","passphrase":"87654321"}"#,
        br#"{"ssid":"a","passphrase":"12345678","verified":true}"#,
        br#"{"ssid":"a","passphrase":"12345678","uid":982}"#,
        br#"{"ssid":"a","passphrase":"12345678","channel":"ble"}"#,
        br#"{"ssid":"a","passphrase":"12345678","token":"SYNTHETIC_TOKEN"}"#,
        br#"{"ssid":"a","passphrase":"12345678","idempotency_key":"synthetic"}"#,
        br#"{"ssid":"a","passphrase":"12345678","action":"begin_wifi"}"#,
    ];
    for (n, b) in bad.into_iter().enumerate() {
        assert!(matches!(
            decode(&mut f, &c, n as u8 + 1, RecoveryAction::BeginWifi, b),
            Err(Error::InvalidInput)
        ));
    }
    assert!(query(&mut f, &c, 99, &id(1))
        .lookup(&f.a)
        .unwrap()
        .is_none());
}
#[test]
fn malformed_json_types_utf8_truncation_and_trailing_values_fail_closed() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let bad = [
        b"null".as_slice(),
        b"[]",
        br#"{"ssid":1,"passphrase":"12345678"}"#,
        br#"{"ssid":"a","passphrase":true}"#,
        br#"{"ssid":"a","passphrase":"12345678""#,
        b"\xff\xfe",
        b"\0",
        br#"{"ssid":"a","passphrase":"12345678"} {}"#,
        br#"{"ssid":"a","passphrase":NaN}"#,
    ];
    for (n, b) in bad.into_iter().enumerate() {
        assert!(matches!(
            decode(&mut f, &c, n as u8 + 1, RecoveryAction::BeginWifi, b),
            Err(Error::InvalidInput)
        ));
    }
}
#[test]
fn wifi_grammar_reuses_the_normal_core_validation_with_private_decoded_strings() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let cases = [
        ("".into(), "12345678".into(), false),
        ("s".repeat(33), "12345678".into(), false),
        ("网".repeat(10), "12345678".into(), true),
        ("a\u{85}".into(), "12345678".into(), false),
        ("a".into(), "1234567".into(), false),
        ("a".into(), "g".repeat(64), false),
        ("a".into(), "a".repeat(64), true),
        ("a".into(), "password\n".into(), false),
    ];
    for (n, (ssid, passphrase, valid)) in cases.into_iter().enumerate() {
        let b =
            serde_json::to_vec(&serde_json::json!({"ssid":ssid,"passphrase":passphrase})).unwrap();
        let result = decode(&mut f, &c, n as u8 + 1, RecoveryAction::BeginWifi, &b);
        assert_eq!(result.is_ok(), valid);
    }
    let escaped = br#"{"ssid":"SYNTHETIC_\u004eET","passphrase":"SYNTHETIC_\u0050ASSWORD"}"#;
    let w = write(&mut f, &c, 99, RecoveryAction::BeginWifi, escaped);
    assert_eq!(w.credentials().unwrap().passphrase(), "SYNTHETIC_PASSWORD");
}
#[test]
fn controls_and_queries_have_closed_canonical_action_specific_shapes() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let bad = [
        (
            RecoveryAction::ConfirmWifi,
            format!("{{\"trial_id\":\"{}\"}}", "A".repeat(64)),
        ),
        (
            RecoveryAction::CancelWifi,
            format!("{{\"trial_id\":\"{}\"}}", "a".repeat(63)),
        ),
        (RecoveryAction::ConfirmWifi, "{\"trial_id\":1}".into()),
        (
            RecoveryAction::ConfirmWifi,
            format!(
                "{{\"trial_id\":\"{}\",\"trial_id\":\"{}\"}}",
                "a".repeat(64),
                "b".repeat(64)
            ),
        ),
        (
            RecoveryAction::QueryOperation,
            format!("{{\"target_request_id\":\"{}\"}}", id(171).to_uppercase()),
        ),
        (
            RecoveryAction::QueryOperation,
            "{\"target_request_id\":\"bad\"}".into(),
        ),
        (
            RecoveryAction::QueryOperation,
            format!("{{\"target_request_id\":\"{}\",\"verified\":true}}", id(1)),
        ),
        (
            RecoveryAction::ConfirmWifi,
            String::from_utf8(BODY.to_vec()).unwrap(),
        ),
        (
            RecoveryAction::QueryOperation,
            format!("{{\"trial_id\":\"{}\"}}", "a".repeat(64)),
        ),
    ];
    for (n, (a, b)) in bad.into_iter().enumerate() {
        assert!(matches!(
            decode(&mut f, &c, n as u8 + 1, a, b.as_bytes()),
            Err(Error::InvalidInput)
        ));
    }
}
#[test]
fn disconnected_revoked_and_foreign_auth_are_checked_before_parsing_or_reserving() {
    for mode in 0..3 {
        let mut f = fixture(true);
        let c = connect(&mut f);
        let r = request(&mut f, &c, 1, RecoveryAction::BeginWifi, b"invalid JSON");
        match mode {
            0 => {
                c.invalidate();
                assert!(matches!(decode_v1(&f.a, r), Err(Error::Forbidden)));
            }
            1 => {
                f.a.revoke_recovery_key(
                    f.owner.expose(),
                    f.grant.independent_public_key,
                    f.grant.generation,
                    id(91),
                )
                .unwrap();
                assert!(matches!(decode_v1(&f.a, r), Err(Error::Storage)));
            }
            _ => {
                let other = fixture(true);
                assert!(matches!(decode_v1(&other.a, r), Err(Error::Forbidden)));
            }
        }
    }
    let mut f = fixture(true);
    let c = connect(&mut f);
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    c.invalidate();
    assert!(matches!(w.reserve(&f.a), Err(Error::Forbidden)));
}
#[test]
fn changing_exact_authenticated_bytes_cannot_reuse_the_private_capability() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let mut r = request(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    r.body.push(b' ');
    assert!(matches!(decode_v1(&f.a, r), Err(Error::Forbidden)));
}
#[test]
fn reconnect_existing_reserved_is_readonly_and_raw_json_variants_conflict() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    let p = pending(w, &f.a);
    drop(c);
    let c = connect(&mut f);
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    let existing = w.reserve(&f.a).unwrap();
    assert!(matches!(existing, CoreRecoveryAdmission::Existing(_)));
    assert!(matches!(
        p.claim(&f.a, Some("a".repeat(64))),
        Err(Error::Forbidden)
    ));
    drop(c);
    let c = connect(&mut f);
    let b = [BODY, b" "].concat();
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, &b);
    assert!(matches!(w.reserve(&f.a), Err(Error::Storage)));
}
#[test]
fn typed_control_cannot_become_begin_and_begin_cannot_become_confirmation() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let t = begin(&mut f, &c);
    let b = format!("{{\"trial_id\":\"{}\"}}", t.authority().trial_id());
    let w = write(&mut f, &c, 2, RecoveryAction::ConfirmWifi, b.as_bytes());
    let claim = pending(w, &f.a).claim(&f.a, None).unwrap();
    assert!(matches!(claim.begin(&f.a), Err(Error::InvalidInput)));
    let mut other = fixture(true);
    let c2 = connect(&mut other);
    let w = write(&mut other, &c2, 1, RecoveryAction::BeginWifi, BODY);
    let claim = pending(w, &other.a)
        .claim(&other.a, Some("b".repeat(64)))
        .unwrap();
    assert!(matches!(
        claim.bind_control(&other.a, &t),
        Err(Error::InvalidInput)
    ));
}
#[test]
fn old_schema_and_nonbusiness_actions_never_gain_execution_admission() {
    let mut f = fixture(false);
    let c = connect(&mut f);
    assert!(matches!(
        decode(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY),
        Err(Error::Storage)
    ));
    assert!(matches!(
        query(&mut f, &c, 2, &id(99)).lookup(&f.a),
        Err(Error::Storage)
    ));
    let mut f = fixture(true);
    let c = connect(&mut f);
    for (n, a) in [
        (1, RecoveryAction::ReadStatus),
        (2, RecoveryAction::Disconnect),
    ] {
        assert!(matches!(
            decode(&mut f, &c, n, a, b"{}"),
            Err(Error::InvalidInput)
        ));
    }
}
#[test]
fn reopened_request_is_unknown_and_never_mints_a_second_private_ticket() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    drop(begin(&mut f, &c));
    drop(c);
    drop(f.a);
    drop(f.s);
    let s = Store::open_recovery_execution(f.d.path()).unwrap();
    let mut a = Auth::open(f.i.path(), s.handle()).unwrap();
    a.set_recovery_verifier(Some(Arc::new(Adapter(f.plan.clone()))))
        .unwrap();
    f.a = a;
    f.s = s;
    let c = connect(&mut f);
    assert_eq!(
        query(&mut f, &c, 2, &id(1))
            .lookup(&f.a)
            .unwrap()
            .unwrap()
            .state(),
        State::Unknown
    );
    let w = write(&mut f, &c, 1, RecoveryAction::BeginWifi, BODY);
    match w.reserve(&f.a).unwrap() {
        CoreRecoveryAdmission::Existing(r) => assert_eq!(r.state(), State::Unknown),
        _ => panic!("Unknown must not mint another ticket"),
    }
}
#[test]
fn actual_backup_contains_no_plaintext_decoded_wifi_credentials() {
    let mut f = fixture(true);
    let c = connect(&mut f);
    let t = begin(&mut f, &c);
    let b = dir();
    f.s.handle().backup(b.path()).unwrap();
    let bytes = fs::read(b.path().join("backup.sqlite3")).unwrap();
    for secret in [
        t.credentials().ssid().as_bytes(),
        t.credentials().passphrase().as_bytes(),
    ] {
        assert!(!bytes.windows(secret.len()).any(|w| w == secret));
    }
}
