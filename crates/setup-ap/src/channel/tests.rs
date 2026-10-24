// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
pub(super) fn binding() -> Binding {
    Binding {
        request_id: "00000000-0000-4000-8000-000000000001".into(),
        boot_id: "00000000-0000-4000-8000-000000000002".into(),
        realm: "00000000-0000-4000-8000-000000000003".into(),
        device_key_sha256: "a".repeat(64),
        material_sha256: "d".repeat(64),
        core_instance: "b".repeat(64),
        nonce: "c".repeat(64),
        created_uptime_ms: 1000,
        deadline_uptime_ms: 301000,
    }
}
pub(super) fn start() -> Hint {
    Hint {
        format: 1,
        binding: binding(),
        action: Action::Start,
        sequence: 1,
        valid_until_uptime_ms: 16000,
    }
}
#[test]
fn hints_are_typed_boot_bound_fresh_and_cannot_extend_or_retarget_a_lease() {
    let initial = start();
    let boot = initial.binding.boot_id.clone();
    let realm = initial.binding.realm.clone();
    let key = initial.binding.device_key_sha256.clone();
    let mut lease = RootLease::begin(
        initial.clone(),
        1000,
        &boot,
        &realm,
        &key,
        &initial.binding.material_sha256,
    )
    .unwrap();
    assert!(lease.check(initial.clone(), 1001, &boot).unwrap());
    assert!(Hint::parse(&serde_json::to_vec(&initial).unwrap(), 1000, &boot).is_ok());
    let mut next = initial.clone();
    next.action = Action::KeepAlive;
    next.sequence = 2;
    next.valid_until_uptime_ms = 17000;
    assert!(lease.check(next.clone(), 2000, &boot).unwrap());
    assert!(lease.check(initial.clone(), 2001, &boot).is_err());
    for mutation in 0..5 {
        let mut bad = next.clone();
        bad.sequence = 3;
        bad.valid_until_uptime_ms = 18000;
        match mutation {
            0 => bad.binding.nonce = "d".repeat(64),
            1 => bad.binding.core_instance = "e".repeat(64),
            2 => bad.binding.deadline_uptime_ms += 1,
            3 => bad.binding.realm = "00000000-0000-4000-8000-000000000004".into(),
            _ => bad.binding.device_key_sha256 = "f".repeat(64),
        }
        assert!(lease.check(bad, 3000, &boot).is_err());
    }
    assert!(lease.check(next.clone(), 17000, &boot).is_err());
    assert!(lease.check(next.clone(), 2000, "other").is_err());
    let mut same_seq = next.clone();
    same_seq.valid_until_uptime_ms = 18000;
    assert!(lease.check(same_seq, 3000, &boot).is_err());
    next.action = Action::Cancel;
    next.sequence = 3;
    next.valid_until_uptime_ms = 18000;
    assert!(!lease.check(next.clone(), 3000, &boot).unwrap());
    next.action = Action::KeepAlive;
    next.sequence = 4;
    assert!(!lease.check(next, 3001, &boot).unwrap());
    let mut v = serde_json::to_value(&initial).unwrap();
    v["command"] = "shell".into();
    assert!(Hint::parse(&serde_json::to_vec(&v).unwrap(), 1000, &boot).is_err());
    assert!(Hint::parse(&vec![b' '; MAX_BYTES + 1], 1000, &boot).is_err());
}
#[test]
fn root_status_cannot_close_another_operation_or_claim_unverified_cleanup() {
    let b = binding();
    let mut s = Status {
        format: 1,
        binding: b.clone(),
        state: State::Closed,
        sequence: 5,
        sample_uptime_ms: 3000,
        rollback_verified: true,
        guard_released: true,
    };
    assert!(s.validate(&b, 3000).is_ok());
    s.guard_released = false;
    assert!(s.validate(&b, 3000).is_err());
    s.state = State::Unknown;
    assert!(s.validate(&b, 3000).is_ok());
    s.guard_released = true;
    assert!(s.validate(&b, 3000).is_err());
    s.state = State::Closed;
    s.binding.nonce = "f".repeat(64);
    assert!(s.validate(&b, 3000).is_err());
    s.binding = b.clone();
    assert!(s.validate(&b, 19000).is_err());
}
#[test]
fn availability_requires_the_exact_realm_device_boot_and_prepared_tls() {
    let b = binding();
    let mut a = Availability {
        format: 1,
        boot_id: b.boot_id.clone(),
        realm: b.realm.clone(),
        device_key_sha256: b.device_key_sha256.clone(),
        material_sha256: "d".repeat(64),
        sample_uptime_ms: 1000,
        tls_origin_ready: true,
        gate_ready: true,
    };
    assert!(a
        .validate(1000, &b.boot_id, &b.realm, &b.device_key_sha256)
        .is_ok());
    a.tls_origin_ready = false;
    assert!(a
        .validate(1000, &b.boot_id, &b.realm, &b.device_key_sha256)
        .is_err());
    a.tls_origin_ready = true;
    assert!(a
        .validate(17000, &b.boot_id, &b.realm, &b.device_key_sha256)
        .is_err());
    assert!(a
        .validate(1000, &b.boot_id, "other", &b.device_key_sha256)
        .is_err());
}
