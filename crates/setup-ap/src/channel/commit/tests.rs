// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
pub(crate) fn request() -> Request {
    Request {
        format: 1,
        trial: station::tests::request(),
        commit_id: "00000000-0000-4000-8000-000000000005".into(),
        requested_uptime_ms: 3000,
        deadline_uptime_ms: 43000,
    }
}
#[test]
fn confirmation_requires_exact_trial_credentials_new_id_and_finite_parent_time() {
    let r = request();
    let bytes = Zeroizing::new(serde_json::to_vec(&r).unwrap());
    assert!(Request::parse(&bytes, &r.trial.binding, 3000).is_ok());
    let intent = r.intent().unwrap();
    assert!(intent.valid());
    let text = serde_json::to_string(&intent).unwrap();
    assert!(!text.contains("synthetic") && !text.contains("ssid") && !text.contains("passphrase"));
    assert_eq!(r.credentials().unwrap().uuid, r.commit_id);
    assert_eq!(r.credentials().unwrap().uuid, r.credentials().unwrap().uuid);
    assert!(Request::parse(&bytes, &r.trial.binding, 2999).is_err());
    assert!(Request::parse(&bytes, &r.trial.binding, 18001).is_err());
    let mut wrong = r.trial.binding.clone();
    wrong.nonce = "f".repeat(64);
    assert!(Request::parse(&bytes, &wrong, 3000).is_err());
    for mode in 0..4 {
        let mut r = request();
        if mode == 0 {
            r.commit_id = r.trial.trial_id.clone();
        }
        if mode == 1 {
            r.deadline_uptime_ms += 1;
        }
        if mode == 2 {
            r.trial.operation = station::Operation::Cancel;
        }
        if mode == 3 {
            r.trial.binding.deadline_uptime_ms = 82000;
        }
        assert!(r.validate(&r.trial.binding, 3000).is_err());
    }
}
#[test]
fn confirmation_rejects_extra_settings_paths_owner_and_commands() {
    let r = request();
    let original = serde_json::to_value(&r).unwrap();
    for key in ["settings", "path", "owner_generation", "command"] {
        let mut value = original.clone();
        value[key] = serde_json::json!("not allowed");
        assert!(
            Request::parse(&serde_json::to_vec(&value).unwrap(), &r.trial.binding, 3000).is_err()
        );
    }
}
pub(crate) fn status(intent: Intent, phase: Phase) -> Status {
    let saved = phase == Phase::Saved;
    Status {
        format: 1,
        observation_boot_id: intent.trial.binding.boot_id.clone(),
        intent,
        phase,
        execution_uptime_ms: 4000,
        sample_uptime_ms: 4000,
        settings_saved: saved,
        ap_removed: saved,
        checkpoints_cleared: saved,
        guard_released: saved,
        ap_https_verified: phase == Phase::RestoredAp,
    }
}
#[test]
fn saved_proof_requires_all_cleanup_flags_and_can_be_observed_after_boot_without_reexecution() {
    let s = status(request().intent().unwrap(), Phase::Saved);
    assert!(s.validate(4000, &s.observation_boot_id).is_ok());
    for field in [
        "settings_saved",
        "ap_removed",
        "checkpoints_cleared",
        "guard_released",
    ] {
        let mut value = serde_json::to_value(&s).unwrap();
        value[field] = false.into();
        let bad: Status = serde_json::from_value(value).unwrap();
        assert!(bad.validate(4000, &bad.observation_boot_id).is_err());
    }
    let mut old = s;
    old.observation_boot_id = "00000000-0000-4000-8000-000000000099".into();
    old.sample_uptime_ms = 10;
    assert!(old.validate(10, &old.observation_boot_id).is_ok());
    assert_eq!(old.execution_uptime_ms, 4000);
    old.phase = Phase::Saving;
    old.settings_saved = false;
    old.ap_removed = false;
    old.checkpoints_cleared = false;
    old.guard_released = false;
    assert!(old.validate(10, &old.observation_boot_id).is_err());
}
#[test]
fn intermediate_and_restored_proofs_cannot_claim_settings_saved_or_release_isolation() {
    for phase in [Phase::Saving, Phase::RestoredAp, Phase::Unknown] {
        let mut s = status(request().intent().unwrap(), phase);
        assert!(s.validate(4000, &s.observation_boot_id).is_ok());
        s.guard_released = true;
        assert!(s.validate(4000, &s.observation_boot_id).is_err());
    }
}
