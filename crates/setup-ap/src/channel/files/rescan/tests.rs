// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
fn setup() -> (
    tempfile::TempDir,
    Directory,
    Journal,
    crate::channel::rescan::Request,
) {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let m = fs::metadata(d.path()).unwrap();
    let dir = Directory::open(d.path(), m.uid(), m.gid(), 0o700).unwrap();
    let j = Journal::open(dir.clone()).unwrap();
    let mut r = crate::channel::rescan::tests::request();
    let t = crate::channel::uptime_ms().unwrap();
    r.binding.boot_id = crate::channel::boot_id().unwrap();
    r.binding.created_uptime_ms = t.saturating_sub(300000);
    r.binding.deadline_uptime_ms = t + 120000;
    r.requested_uptime_ms = t;
    r.deadline_uptime_ms = t + 40000;
    j.consume(&r.binding).unwrap();
    (d, dir, j, r)
}
fn done(intent: Intent, phase: Phase) -> Status {
    let t = crate::channel::uptime_ms().unwrap();
    Status {
        format: 1,
        sample_boot_id: crate::channel::boot_id().unwrap(),
        sample_uptime_ms: t,
        finished_uptime_ms: Some(intent.requested_uptime_ms),
        intent,
        phase,
        ap_https_verified: phase == Phase::Failed,
        snapshot: None,
        refusal: None,
    }
}
#[test]
fn rescan_intent_is_immutable_and_restart_cannot_reconsume_it() {
    let (_d, dir, j, r) = setup();
    let intent = r.intent().unwrap();
    j.consume_rescan(&intent).unwrap();
    assert!(j.consume_rescan(&intent).is_err());
    drop(j);
    let j = Journal::open(dir).unwrap();
    assert!(j.rescan_consumed(&intent).unwrap());
    assert!(j.rescan_pending().unwrap() == Some(intent.clone()));
    let mut altered = intent.clone();
    altered.owner_generation += 1;
    assert!(j.rescan_consumed(&altered).is_err());
    j.finish_rescan(&done(intent.clone(), Phase::Unknown))
        .unwrap();
    assert!(j.finish_rescan(&done(intent, Phase::Failed)).is_err());
    assert!(j.ensure_rescans_closed(&r.binding).is_err());
}
#[test]
fn missing_active_marker_still_blocks_parent_cleanup_and_allows_only_unknown_receipt() {
    let (d, _dir, j, r) = setup();
    let intent = r.intent().unwrap();
    j.consume_rescan(&intent).unwrap();
    fs::remove_file(d.path().join("rescan-active.json")).unwrap();
    assert!(j.ensure_rescans_closed(&r.binding).is_err());
    assert!(j
        .finish_rescan(&done(intent.clone(), Phase::Failed))
        .is_err());
    j.finish_rescan(&done(intent, Phase::Unknown)).unwrap();
    assert!(j.ensure_rescans_closed(&r.binding).is_err());
    let closed = super::super::super::Status {
        format: 1,
        binding: r.binding.clone(),
        state: State::Closed,
        sequence: 1,
        sample_uptime_ms: crate::channel::uptime_ms().unwrap(),
        rollback_verified: true,
        guard_released: true,
        refusal: None,
    };
    assert!(j.finish_closed(&closed, closed.sample_uptime_ms).is_err());
    assert!(j.pending().unwrap().is_some());
}
#[test]
fn known_results_allow_parent_cleanup_but_not_replay_and_enforce_cooldown_and_count() {
    let (_d, _dir, j, mut r) = setup();
    let t = r.requested_uptime_ms;
    // Old completed metadata uses the original boot clock; query sampling is current.
    for i in 0..MAX_PER_PARENT {
        r.scan_id = format!("00000000-0000-4000-8000-{:012}", 100 + i);
        r.requested_uptime_ms = t.saturating_sub(240000) + (i as u64) * COOLDOWN_MS;
        r.deadline_uptime_ms = r.requested_uptime_ms + 40000;
        let intent = r.intent().unwrap();
        j.consume_rescan(&intent).unwrap();
        j.finish_rescan(&done(intent.clone(), Phase::Failed))
            .unwrap();
        assert!(j.rescan_pending().unwrap().is_none());
        assert!(j.consume_rescan(&intent).is_err());
        let mut early = r.clone();
        early.scan_id = "00000000-0000-4000-8000-000000000999".into();
        early.requested_uptime_ms += COOLDOWN_MS - 1;
        early.deadline_uptime_ms = early.requested_uptime_ms + 40000;
        assert!(j.consume_rescan(&early.intent().unwrap()).is_err());
    }
    r.scan_id = "00000000-0000-4000-8000-000000000999".into();
    r.requested_uptime_ms = t;
    r.deadline_uptime_ms = t + 40000;
    assert!(j.consume_rescan(&r.intent().unwrap()).is_err());
    j.ensure_rescans_closed(&r.binding).unwrap();
}
#[test]
fn rescan_files_refuse_symlink_and_overfull_journal() {
    let (d, _dir, j, r) = setup();
    let intent = r.intent().unwrap();
    let name = format!("{}.intent", intent.key().unwrap());
    std::os::unix::fs::symlink(d.path().join("target"), d.path().join(&name)).unwrap();
    assert!(j.consume_rescan(&intent).is_err());
    assert!(!d.path().join("target").exists());
    fs::remove_file(d.path().join(name)).unwrap();
    for i in 0..513 {
        fs::write(d.path().join(format!("filler-{i}")), b"x").unwrap();
    }
    assert!(j.consume_rescan(&intent).is_err());
    assert!(j.ensure_rescans_closed(&r.binding).is_err());
}

fn refusal(intent: Intent, reason: Refusal) -> Status {
    let mut s = done(intent, Phase::Refused);
    s.refusal = Some(reason);
    s
}
#[test]
fn refusal_is_one_atomic_decision_survives_restart_and_does_not_consume_scan_budget() {
    let (d, dir, j, r) = setup();
    let intent = r.intent().unwrap();
    let status = refusal(intent.clone(), Refusal::Conflict);
    j.refuse_rescan(&status).unwrap();
    assert!(j.rescan_consumed(&intent).unwrap());
    assert!(!d
        .path()
        .join(format!("{}.intent", intent.key().unwrap()))
        .exists());
    assert!(j.rescan_pending().unwrap().is_none());
    assert!(j.consume_rescan(&intent).is_err());
    assert!(j.refuse_rescan(&status).is_err());
    assert_eq!(
        j.rescan_admission(&r.binding, r.requested_uptime_ms)
            .unwrap()
            .remaining,
        8
    );
    j.ensure_rescans_closed(&r.binding).unwrap();
    drop(j);
    let j = Journal::open(dir).unwrap();
    let saved = j.rescan_result(&intent).unwrap().unwrap();
    assert!(saved.phase == Phase::Refused && saved.refusal == Some(Refusal::Conflict));
    let mut changed = intent;
    changed.owner_generation += 1;
    assert!(j.rescan_consumed(&changed).is_err());
}
#[test]
fn refusal_cannot_hide_accepted_unknown_or_replace_an_accepted_result() {
    let (_d, _dir, j, r) = setup();
    let intent = r.intent().unwrap();
    j.consume_rescan(&intent).unwrap();
    assert!(j
        .refuse_rescan(&refusal(intent.clone(), Refusal::Conflict))
        .is_err());
    assert!(j
        .finish_rescan(&refusal(intent.clone(), Refusal::Conflict))
        .is_err());
    assert!(j
        .rescan_admission(&r.binding, r.requested_uptime_ms)
        .is_err());
    j.finish_rescan(&done(intent, Phase::Unknown)).unwrap();
    assert!(j
        .rescan_admission(&r.binding, r.requested_uptime_ms)
        .is_err());
}
#[test]
fn admission_exposes_original_cooldown_and_exact_persisted_scan_limit() {
    let (_d, _dir, j, mut r) = setup();
    let t = r.requested_uptime_ms;
    r.requested_uptime_ms = t - 10000;
    r.deadline_uptime_ms = t + 30000;
    let i = r.intent().unwrap();
    j.consume_rescan(&i).unwrap();
    j.finish_rescan(&done(i, Phase::Failed)).unwrap();
    let a = j.rescan_admission(&r.binding, t).unwrap();
    assert!(a.refusal == Some(Refusal::Cooldown));
    assert_eq!(a.retry_after_uptime_ms, Some(t + 20000));
    assert_eq!(a.remaining, 7);
    let a = j.rescan_admission(&r.binding, t + 20000).unwrap();
    assert!(a.refusal.is_none());
    assert!(a.retry_after_uptime_ms.is_none());
    // Queries do not advance the stored start time or the next admission time.
    assert_eq!(
        j.rescan_admission(&r.binding, t)
            .unwrap()
            .retry_after_uptime_ms,
        Some(t + 20000)
    );
}
#[test]
fn refusal_storage_is_bounded_and_links_cannot_turn_refusal_into_permission() {
    let (d, _dir, j, mut r) = setup();
    for i in 0..MAX_REFUSALS_PER_PARENT {
        r.scan_id = format!("00000000-0000-4000-8000-{:012}", 100 + i);
        j.refuse_rescan(&refusal(r.intent().unwrap(), Refusal::Conflict))
            .unwrap();
    }
    assert!(
        j.rescan_admission(&r.binding, r.requested_uptime_ms)
            .unwrap()
            .refusal
            == Some(Refusal::Capacity)
    );
    r.scan_id = "00000000-0000-4000-8000-000000000999".into();
    let intent = r.intent().unwrap();
    assert!(j
        .refuse_rescan(&refusal(intent.clone(), Refusal::Conflict))
        .is_err());
    assert!(j.consume_rescan(&intent).is_err());
    let name = format!("{}.refusal", intent.key().unwrap());
    std::os::unix::fs::symlink(d.path().join("target"), d.path().join(name)).unwrap();
    assert!(j.rescan_consumed(&intent).is_err());
    assert!(!d.path().join("target").exists());
}
