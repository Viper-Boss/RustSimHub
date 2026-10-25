// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::channel::commit::{self, Phase};
fn request() -> commit::Request {
    let mut r = commit::tests::request();
    let t = crate::channel::uptime_ms().unwrap();
    r.trial.binding.boot_id = crate::channel::boot_id().unwrap();
    r.trial.binding.created_uptime_ms = t;
    r.trial.binding.deadline_uptime_ms = t + 120_000;
    r.trial.requested_uptime_ms = t;
    r.trial.deadline_uptime_ms = t + 40_000;
    r.requested_uptime_ms = t;
    r.deadline_uptime_ms = t + 40_000;
    r
}
fn ready(j: &Journal, r: &commit::Request) {
    j.consume(&r.trial.binding).unwrap();
    let intent = r.trial.intent().unwrap();
    j.consume_trial(&intent).unwrap();
    j.finish_trial(&crate::channel::station::Status {
        format: 1,
        intent,
        phase: crate::channel::station::Phase::Ready,
        sample_uptime_ms: crate::channel::uptime_ms().unwrap(),
        ap_https_verified: true,
    })
    .unwrap();
}
fn status(r: &commit::Request, phase: Phase) -> commit::Status {
    let mut s = commit::tests::status(r.intent().unwrap(), phase);
    s.execution_uptime_ms = crate::channel::uptime_ms().unwrap();
    s.sample_uptime_ms = s.execution_uptime_ms;
    s
}
#[test]
fn only_exact_successful_trial_admits_confirmation_and_durable_consumption_survives_restart() {
    let (_d, dir) = dir(0o700);
    let j = Journal::open(dir.clone()).unwrap();
    let r = request();
    assert!(j
        .admit_commit(&r, &r.trial.binding, r.requested_uptime_ms)
        .is_err());
    ready(&j, &r);
    let intent = j
        .admit_commit(&r, &r.trial.binding, r.requested_uptime_ms)
        .unwrap();
    j.consume_commit(&intent).unwrap();
    let bytes = dir
        .read_name(&format!("{}.intent", intent.key().unwrap()), 0o600)
        .unwrap()
        .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(!text.contains("synthetic") && !text.contains("ssid") && !text.contains("passphrase"));
    assert!(j.consume_commit(&intent).is_err());
    drop(j);
    let j = Journal::open(dir).unwrap();
    assert!(j.commit_consumed(&intent).unwrap());
    assert!(j.commit_result(&intent).unwrap().is_none());
    assert!(j.consume_commit(&intent).is_err());
    let mut wrong = intent;
    wrong.trial.owner_generation += 1;
    assert!(j.commit_consumed(&wrong).is_err());
}
#[test]
fn saved_receipt_clears_parent_without_forging_legacy_closed_and_cannot_be_overwritten() {
    let (_d, dir) = dir(0o700);
    let j = Journal::open(dir).unwrap();
    let r = request();
    ready(&j, &r);
    j.consume_commit(&r.intent().unwrap()).unwrap();
    let done = status(&r, Phase::Saved);
    j.finish_commit(&done).unwrap();
    assert!(j.pending().unwrap().is_none());
    assert!(j.commit_pending(&r.trial.binding).unwrap().is_none());
    assert!(j.committed(&r.trial.binding).unwrap().is_some());
    assert!(j.closed(&r.trial.binding).unwrap().is_none());
    assert!(j.finish_commit(&status(&r, Phase::Unknown)).is_err());
    assert!(j.consume_commit(&r.intent().unwrap()).is_err());
}
#[test]
fn restored_confirmation_keeps_parent_and_unknown_blocks_another_confirmation() {
    let (_d, dir) = dir(0o700);
    let j = Journal::open(dir).unwrap();
    let mut r = request();
    ready(&j, &r);
    j.consume_commit(&r.intent().unwrap()).unwrap();
    j.finish_commit(&status(&r, Phase::RestoredAp)).unwrap();
    assert!(j.pending().unwrap().is_some());
    assert!(j.commit_pending(&r.trial.binding).unwrap().is_none());
    r.commit_id = "00000000-0000-4000-8000-000000000006".into();
    j.consume_commit(&r.intent().unwrap()).unwrap();
    j.finish_commit(&status(&r, Phase::Unknown)).unwrap();
    assert!(j.commit_pending(&r.trial.binding).unwrap().is_some());
    r.commit_id = "00000000-0000-4000-8000-000000000007".into();
    assert!(j.consume_commit(&r.intent().unwrap()).is_err());
}
#[test]
fn interrupted_saved_receipt_completion_only_finishes_metadata_never_replays_consumed_work() {
    let (_d, dir) = dir(0o700);
    let j = Journal::open(dir.clone()).unwrap();
    let r = request();
    ready(&j, &r);
    let intent = r.intent().unwrap();
    j.consume_commit(&intent).unwrap();
    let done = status(&r, Phase::Saved);
    dir.write_name(
        &format!("{}.result", intent.key().unwrap()),
        &serde_json::to_vec(&done).unwrap(),
        0o600,
        true,
    )
    .unwrap();
    drop(j);
    let j = Journal::open(dir).unwrap();
    assert!(j.pending().unwrap().is_some());
    j.finish_commit(&j.commit_result(&intent).unwrap().unwrap())
        .unwrap();
    assert!(j.pending().unwrap().is_none());
    assert!(j.committed(&r.trial.binding).unwrap().is_some());
    assert!(j.consume_commit(&intent).is_err());
}
