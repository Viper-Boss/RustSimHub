// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

fn properties(ssid: &[u8]) -> Properties {
    HashMap::from([
        (
            "Ssid".into(),
            OwnedValue::try_from(Value::from(ssid.to_vec())).unwrap(),
        ),
        ("LastSeen".into(), 10i32.into()),
        ("Mode".into(), 2u32.into()),
        ("Strength".into(), 80u8.into()),
        ("Frequency".into(), 2412u32.into()),
        ("Flags".into(), 1u32.into()),
        ("WpaFlags".into(), 0u32.into()),
        ("RsnFlags".into(), 0x100u32.into()),
    ])
}
#[test]
fn retained_ap_objects_are_not_fresh_scan_results() {
    for seen in [-1, 8, 12] {
        let mut p = properties(b"Old");
        p.insert("LastSeen".into(), seen.into());
        assert!(choice(&p, 10_999, 11_500).unwrap().is_none());
    }
    // CLOCK_BOOTTIME seconds cannot distinguish two observations in the same second.
    assert!(choice(&properties(b"Current"), 10_999, 11_500)
        .unwrap()
        .is_some());
}
#[test]
fn unknown_binary_hidden_oversized_and_control_ssids_are_omitted() {
    for ssid in [vec![], vec![0xff], vec![b'a'; 33], b"line\nbreak".to_vec()] {
        assert!(choice(&properties(&ssid), 10_000, 10_500)
            .unwrap()
            .is_none());
    }
    for mode in [0u32, 1, 3] {
        let mut p = properties(b"Not infrastructure");
        p.insert("Mode".into(), mode.into());
        assert!(choice(&p, 10_000, 10_500).unwrap().is_none());
    }
}
#[test]
fn malformed_property_types_do_not_turn_into_display_defaults() {
    for (name, value) in [
        ("LastSeen", string("10")),
        ("Strength", 101u8.into()),
        ("RsnFlags", true.into()),
    ] {
        let mut p = properties(b"Malformed");
        p.insert(name.into(), value);
        assert!(choice(&p, 10_000, 10_500).is_err());
    }
    let mut p = properties(b"Missing");
    p.remove("LastSeen");
    assert!(choice(&p, 10_000, 10_500).is_err());
}
#[test]
fn security_is_display_only_and_keeps_unsupported_networks_separate() {
    let mut p = properties(b"Network");
    assert!(choice(&p, 10_000, 10_500).unwrap().unwrap().security == Security::Wpa2Personal);
    p.insert("Flags".into(), 0u32.into());
    p.insert("RsnFlags".into(), 0u32.into());
    assert!(choice(&p, 10_000, 10_500).unwrap().unwrap().security == Security::Open);
    p.insert("Flags".into(), 1u32.into());
    p.insert("WpaFlags".into(), 0x100u32.into());
    assert!(choice(&p, 10_000, 10_500).unwrap().unwrap().security == Security::Unsupported);
    p.insert("RsnFlags".into(), 0x400u32.into());
    assert!(choice(&p, 10_000, 10_500).unwrap().unwrap().security == Security::Unsupported);
}
#[test]
fn duplicate_display_choices_use_strongest_signal_without_a_bssid() {
    let row = choice(&properties(b"Same"), 10_000, 10_500)
        .unwrap()
        .unwrap();
    let mut strong = row.clone();
    strong.strength = 90;
    let mut other_security = row.clone();
    other_security.security = Security::Open;
    let result = snapshot(vec![row, strong, other_security], 0, 10_000, 10_500, 10_400);
    assert!(result.valid());
    assert_eq!(result.networks.len(), 2);
    assert_eq!(result.networks[0].strength, 90);
    let value = serde_json::to_value(&result).unwrap();
    let object = value["networks"][0].as_object().unwrap();
    assert_eq!(object.len(), 4);
    assert!(!object.contains_key("bssid"));
    assert!(!object.contains_key("path"));
}
#[test]
fn scan_snapshot_is_bounded_and_checks_timestamps_and_duplicates() {
    let rows = (0..40)
        .map(|i| Choice {
            ssid: format!("AP-{i:02}"),
            strength: 80,
            frequency_mhz: 2412,
            security: Security::Wpa2Personal,
        })
        .collect();
    let mut result = snapshot(rows, 2, 10_000, 10_500, 10_400);
    assert!(result.valid());
    assert_eq!(result.networks.len(), 32);
    assert!(result.truncated);
    assert_eq!(result.networks[0].ssid, "AP-00");
    result.last_scan_uptime_ms = 9_999;
    assert!(!result.valid());
    result.last_scan_uptime_ms = 10_400;
    result.completed_uptime_ms = 18_001;
    assert!(!result.valid());
    result.completed_uptime_ms = 10_500;
    result.omitted = 129;
    assert!(!result.valid());
    result.omitted = 2;
    result.networks[1] = result.networks[0].clone();
    assert!(!result.valid());
}
