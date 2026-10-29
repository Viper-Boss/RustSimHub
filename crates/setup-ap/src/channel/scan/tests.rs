// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::nm::scan::{Choice, Security};
fn proof() -> Proof {
    Proof {
        format: 1,
        binding: Binding {
            request_id: "11111111-1111-4111-8111-111111111111".into(),
            boot_id: "22222222-2222-4222-8222-222222222222".into(),
            realm: "33333333-3333-4333-8333-333333333333".into(),
            device_key_sha256: "a".repeat(64),
            material_sha256: "b".repeat(64),
            core_instance: "c".repeat(64),
            nonce: "d".repeat(64),
            created_uptime_ms: 1000,
            deadline_uptime_ms: 20_000,
        },
        snapshot: Snapshot {
            requested_uptime_ms: 2000,
            completed_uptime_ms: 2100,
            last_scan_uptime_ms: 2050,
            networks: vec![],
            omitted: 0,
            truncated: false,
        },
    }
}
#[test]
fn scan_proof_cannot_cross_boot_device_round_or_deadline() {
    let p = proof();
    p.validate(&p.binding, 2200).unwrap();
    let mut wrong = p.binding.clone();
    wrong.nonce = "f".repeat(64);
    assert!(p.validate(&wrong, 2200).is_err());
    wrong = p.binding.clone();
    wrong.boot_id = "44444444-4444-4444-8444-444444444444".into();
    assert!(p.validate(&wrong, 2200).is_err());
    wrong = p.binding.clone();
    wrong.device_key_sha256 = "e".repeat(64);
    assert!(p.validate(&wrong, 2200).is_err());
    assert!(p.validate(&p.binding, 2099).is_err());
    assert!(p.validate(&p.binding, 20_000).is_err());
    let mut old = p.clone();
    old.snapshot.requested_uptime_ms = 999;
    assert!(old.validate(&p.binding, 2200).is_err());
    let mut value = serde_json::to_value(p).unwrap();
    value["command"] = serde_json::json!("scan_again");
    assert!(Proof::parse(&serde_json::to_vec(&value).unwrap(), &wrong, 2200).is_err());
}
#[test]
fn maximum_escaped_display_snapshot_fits_the_fixed_private_slot() {
    let mut p = proof();
    p.snapshot.networks = (0..32)
        .map(|i| Choice {
            ssid: format!("{i:02}{}", "\"\\".repeat(15)),
            strength: 90,
            frequency_mhz: 2412,
            security: Security::Wpa2Personal,
        })
        .collect();
    let bytes = serde_json::to_vec(&p).unwrap();
    assert!(bytes.len() <= MAX_BYTES);
    let back = Proof::parse(&bytes, &p.binding, 2200).unwrap();
    assert_eq!(back.snapshot.networks.len(), 32);
    let mut excessive = bytes;
    excessive.resize(MAX_BYTES + 1, b' ');
    assert!(Proof::parse(&excessive, &p.binding, 2200).is_err());
}
