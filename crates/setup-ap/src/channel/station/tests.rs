// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
pub(crate) fn request() -> Request {
    Request {
        format: 1,
        binding: super::super::tests::binding(),
        trial_id: "00000000-0000-4000-8000-000000000004".into(),
        owner_key_sha256: "e".repeat(64),
        owner_generation: 1,
        requested_uptime_ms: 2000,
        deadline_uptime_ms: 42000,
        operation: Operation::Test {
            ssid: "synthetic SSID".into(),
            passphrase: Secret::new(Zeroizing::new("synthetic-password".into())),
        },
    }
}
#[test]
fn station_request_is_bounded_and_intent_and_status_never_contain_credentials() {
    let r = request();
    let bytes = Zeroizing::new(serde_json::to_vec(&r).unwrap());
    let parsed = Request::parse(&bytes, &r.binding, 2000).unwrap();
    let intent = parsed.intent().unwrap();
    let text = serde_json::to_string(&intent).unwrap();
    assert!(!text.contains("synthetic") && !text.contains("passphrase") && !text.contains("ssid"));
    assert!(Request::parse(&bytes, &r.binding, 18000).is_err());
    assert!(Request::parse(&bytes, &r.binding, 42000).is_err());
    let mut wrong = r.binding.clone();
    wrong.nonce = "f".repeat(64);
    assert!(Request::parse(&bytes, &wrong, 2000).is_err());
    let mut r = request();
    r.deadline_uptime_ms = 42001;
    assert!(r.validate(&r.binding, 2000).is_err());
    let mut r = request();
    r.binding.deadline_uptime_ms = 60000;
    assert!(r.validate(&r.binding, 2000).is_err());
}
#[test]
fn station_wire_rejects_extra_settings_commands_weak_credentials_and_future_timestamps() {
    let r = request();
    let initial = serde_json::to_value(&r).unwrap();
    for (key, value) in [
        ("settings", serde_json::json!({})),
        ("device", serde_json::json!("wwan0")),
        ("command", serde_json::json!("arbitrary")),
    ] {
        let mut v = initial.clone();
        v[key] = value;
        assert!(Request::parse(&serde_json::to_vec(&v).unwrap(), &r.binding, 2000).is_err());
    }
    for value in ["short", "invalid\npassword"] {
        let mut v = initial.clone();
        v["operation"]["passphrase"] = value.into();
        assert!(Request::parse(&serde_json::to_vec(&v).unwrap(), &r.binding, 2000).is_err());
    }
    assert!(Request::parse(&serde_json::to_vec(&r).unwrap(), &r.binding, 1999).is_err());
}
#[test]
fn station_cancel_requires_exact_original_ap_owner_and_trial() {
    let mut r = request();
    let intent = r.intent().unwrap();
    r.operation = Operation::Cancel;
    assert!(r.validate(&r.binding, 30000).is_ok() && r.cancels(&intent));
    r.owner_generation += 1;
    assert!(!r.cancels(&intent));
    r.owner_generation -= 1;
    r.owner_key_sha256 = "f".repeat(64);
    assert!(!r.cancels(&intent));
}
#[test]
fn station_success_requires_https_and_fresh_active_ap_receipt() {
    let intent = request().intent().unwrap();
    let mut s = Status {
        format: 1,
        intent,
        phase: Phase::Ready,
        sample_uptime_ms: 3000,
        ap_https_verified: true,
    };
    assert!(s.validate(3000).is_ok());
    s.ap_https_verified = false;
    assert!(s.validate(3000).is_err());
    s.phase = Phase::Testing;
    assert!(s.validate(3000).is_ok());
    s.ap_https_verified = true;
    assert!(s.validate(3000).is_err());
    s.phase = Phase::Ready;
    assert!(s.validate(18001).is_err());
    s.sample_uptime_ms = 301000;
    assert!(s.validate(301000).is_err());
}
