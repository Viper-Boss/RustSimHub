// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
pub(crate) fn request() -> Request {
    Request {
        format: 1,
        binding: crate::channel::tests::binding(),
        scan_id: "00000000-0000-4000-8000-000000000010".into(),
        owner_key_sha256: "e".repeat(64),
        owner_generation: 1,
        requested_uptime_ms: 2000,
        deadline_uptime_ms: 42000,
        action: Action::Scan,
    }
}
fn failed(r: &Request) -> Status {
    Status {
        format: 1,
        intent: r.intent().unwrap(),
        phase: Phase::Failed,
        sample_boot_id: r.binding.boot_id.clone(),
        sample_uptime_ms: 3000,
        finished_uptime_ms: Some(3000),
        ap_https_verified: true,
        snapshot: None,
    }
}
#[test]
fn rescan_requests_are_exact_bounded_and_cancel_does_not_extend_the_body() {
    let r = request();
    let b = &r.binding;
    let intent = r.intent().unwrap();
    assert!(Request::parse(&serde_json::to_vec(&r).unwrap(), b, 2000).is_ok());
    assert!(r.validate(b, 2000 + HEARTBEAT_MS + 1).is_err());
    for mutation in 0..8 {
        let mut bad = r.clone();
        match mutation {
            0 => bad.format = 2,
            1 => bad.scan_id = "not-a-uuid".into(),
            2 => bad.owner_key_sha256 = "e".into(),
            3 => bad.owner_generation = 0,
            4 => bad.deadline_uptime_ms += 1,
            5 => bad.requested_uptime_ms = 999,
            6 => bad.binding.nonce = "f".repeat(64),
            _ => bad.requested_uptime_ms = 2001,
        }
        assert!(bad.validate(b, 2000).is_err());
    }
    let mut cancel = r.clone();
    cancel.action = Action::Cancel;
    assert!(cancel.cancels(&intent));
    assert!(cancel.validate(b, 60000).is_ok());
    cancel.owner_generation += 1;
    assert!(!cancel.cancels(&intent));
    let mut v = serde_json::to_value(&r).unwrap();
    v["interface"] = "wlan0".into();
    assert!(Request::parse(&serde_json::to_vec(&v).unwrap(), b, 2000).is_err());
    assert!(Request::parse(&vec![b' '; MAX_BYTES + 1], b, 2000).is_err());
}
#[test]
fn rescan_terminal_queries_refresh_sampling_but_never_completion_or_the_lease() {
    let r = request();
    let mut s = failed(&r);
    assert!(s.validate(3000, &r.binding.boot_id).is_ok());
    s.sample_uptime_ms = 100000;
    assert!(s.validate(100000, &r.binding.boot_id).is_ok());
    assert_eq!(s.finished_uptime_ms, Some(3000));
    s.finished_uptime_ms = Some(42001);
    assert!(s.validate(100000, &r.binding.boot_id).is_err());
    s.finished_uptime_ms = Some(3000);
    s.ap_https_verified = false;
    assert!(s.validate(100000, &r.binding.boot_id).is_err());
    s.phase = Phase::Unknown;
    assert!(s.validate(100000, &r.binding.boot_id).is_ok());
    s.sample_boot_id = "00000000-0000-4000-8000-000000000099".into();
    s.sample_uptime_ms = 20;
    assert!(s.validate(20, &s.sample_boot_id).is_ok());
    s.phase = Phase::Failed;
    s.ap_https_verified = true;
    assert!(s.validate(20, &s.sample_boot_id).is_ok());
    s.phase = Phase::Scanning;
    s.ap_https_verified = false;
    s.finished_uptime_ms = None;
    assert!(s.validate(20, &s.sample_boot_id).is_err());
    s.phase = Phase::Unknown;
    assert!(s.validate(20, &s.sample_boot_id).is_ok());
}
#[test]
fn rescan_ready_requires_a_new_valid_snapshot_and_fits_the_fixed_slot() {
    let r = request();
    let mut s = failed(&r);
    s.phase = Phase::Ready;
    assert!(s.validate(3000, &r.binding.boot_id).is_err());
    s.snapshot = Some(Snapshot {
        requested_uptime_ms: 2000,
        last_scan_uptime_ms: 2500,
        completed_uptime_ms: 3000,
        networks: vec![],
        omitted: 0,
        truncated: false,
    });
    assert!(s.validate(3000, &r.binding.boot_id).is_ok());
    let snapshot = s.snapshot.as_mut().unwrap();
    snapshot.networks = (0..32)
        .map(|i| crate::nm::scan::Choice {
            ssid: format!("{i:02}{}", "\\".repeat(30)),
            strength: 100,
            frequency_mhz: 2412,
            security: crate::nm::scan::Security::Wpa2Personal,
        })
        .collect();
    snapshot.truncated = true;
    snapshot.omitted = 128;
    let bytes = serde_json::to_vec(&s).unwrap();
    assert!(bytes.len() <= MAX_BYTES);
    assert!(Status::parse(&bytes, &r.intent().unwrap(), 3000, &r.binding.boot_id).is_ok());
    s.snapshot.as_mut().unwrap().requested_uptime_ms = 1999;
    assert!(s.validate(3000, &r.binding.boot_id).is_err());
}
#[test]
fn rescan_capability_is_fresh_exact_and_separate_from_parent_availability() {
    let r = request();
    let mut c = Capability {
        format: 1,
        binding: r.binding.clone(),
        sample_uptime_ms: 2000,
        available: true,
        max_per_parent: MAX_PER_PARENT,
        cooldown_ms: COOLDOWN_MS,
    };
    let bytes = serde_json::to_vec(&c).unwrap();
    assert!(Capability::parse(&bytes, &r.binding, 2000, &r.binding.boot_id).is_ok());
    assert!(Capability::parse(&bytes, &r.binding, 18000, &r.binding.boot_id).is_err());
    assert!(Capability::parse(&bytes, &r.binding, 2000, "wrong-boot").is_err());
    c.max_per_parent += 1;
    assert!(Capability::parse(
        &serde_json::to_vec(&c).unwrap(),
        &r.binding,
        2000,
        &r.binding.boot_id
    )
    .is_err());
}
