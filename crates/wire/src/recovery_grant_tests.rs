// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde_json::json;
#[test]
fn recovery_admin_is_strict_bounded_and_never_accepts_payload_authority() {
    let body = json!({"protocol":2,"id":"synthetic","command":{"op":"recovery_grant","token":"a".repeat(64),
        "public_key":vec![2;32],"max_trial_ms":40000,"expected_generation":"0","request_id":"00000000-0000-4000-8000-000000000001"}});
    assert!(parse_auth_request(&serde_json::to_vec(&body).unwrap()).is_ok());
    for (field, value) in [
        ("max_trial_ms", json!(0)),
        ("max_trial_ms", json!(40001)),
        ("expected_generation", json!(1)),
        ("expected_generation", json!("01")),
        ("expected_generation", json!("-1")),
        ("expected_generation", json!("9223372036854775807")),
        ("request_id", json!("not-uuid")),
        ("channel", json!("ble")),
        ("peer_proof", json!({"authenticated":true})),
        ("uid", json!(1000)),
    ] {
        let mut bad = body.clone();
        bad["command"][field] = value;
        assert!(
            parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "accepted {field}"
        );
    }
    for op in ["recovery_key", "recovery_request", "recovery_revoke"] {
        let mut bad = body.clone();
        bad["command"]["op"] = json!(op);
        assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
}
