// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde_json::json;

#[test]
fn pcm_relay_messages_are_bounded_and_reject_identity_or_authority_fields() {
    let request = |command| {
        serde_json::to_vec(&json!({"protocol":AUTH_PROTOCOL,"id":"pcm-test","command":command}))
            .unwrap()
    };
    let open = json!({"op":"airsim_pcm_open","preface":"AIRSIMVWL1 1 nonce signature\n","peer":"192.168.1.2"});
    assert!(parse_auth_request(&request(open.clone())).is_ok());
    for (field, value) in [
        ("preface", json!("x".repeat(1025))),
        ("preface", json!("AIRSIMVWL1 1 nonce signature\r\n")),
        ("preface", json!("AIRSIMVWL1\n1 nonce signature\n")),
        ("peer", json!("not-an-address")),
        ("role", json!("owner")),
        ("token", json!("a".repeat(64))),
    ] {
        let mut changed = open.clone();
        changed[field] = value;
        assert!(parse_auth_request(&request(changed)).is_err(), "{field}");
    }
    let pulse = json!({"op":"airsim_pcm_pulse","ticket":"a".repeat(32),"close":false});
    assert!(parse_auth_request(&request(pulse.clone())).is_ok());
    for ticket in ["".to_owned(), "a".repeat(33), "z".repeat(32)] {
        let mut changed = pulse.clone();
        changed["ticket"] = json!(ticket);
        assert!(parse_auth_request(&request(changed)).is_err());
    }
}
