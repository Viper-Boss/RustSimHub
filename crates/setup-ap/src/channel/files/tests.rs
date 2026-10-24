// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::channel::tests::binding;
fn dir(mode: u32) -> (tempfile::TempDir, Directory) {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(mode)).unwrap();
    let m = fs::metadata(d.path()).unwrap();
    let v = Directory::open(d.path(), m.uid(), m.gid(), mode).unwrap();
    (d, v)
}
#[test]
fn fixed_slots_reject_links_weak_metadata_and_oversized_files() {
    let (d, v) = dir(0o2750);
    assert!(v.read(Slot::Hint).unwrap().is_none());
    v.publish(Slot::Hint, &super::super::tests::start())
        .unwrap();
    let path = d.path().join("hint.json");
    assert!(v.read(Slot::Hint).unwrap().is_some());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(v.read(Slot::Hint).is_err());
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(d.path().join("private"), &path).unwrap();
    assert!(v.read(Slot::Hint).is_err());
    assert!(v
        .publish(Slot::Hint, &super::super::tests::start())
        .is_err());
    assert!(!d.path().join("private").exists());
    fs::remove_file(&path).unwrap();
    fs::write(&path, vec![b'x'; MAX_BYTES + 1]).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    assert!(v.read(Slot::Hint).is_err());
    fs::remove_file(&path).unwrap();
    v.publish(Slot::Hint, &super::super::tests::start())
        .unwrap();
    fs::hard_link(&path, d.path().join("second")).unwrap();
    assert!(v.read(Slot::Hint).is_err());
}
#[test]
fn root_journal_persists_unknown_and_never_replays_consumed_operations() {
    let (d, v) = dir(0o700);
    let j = Journal::open(v.clone()).unwrap();
    let b = binding();
    j.consume(&b).unwrap();
    assert!(j.pending().unwrap().is_some());
    assert!(j.consume(&b).is_err());
    let unknown = Status {
        format: 1,
        binding: b.clone(),
        state: State::Unknown,
        sequence: 1,
        sample_uptime_ms: 2000,
        rollback_verified: false,
        guard_released: false,
    };
    assert!(j.finish_closed(&unknown, 2000).is_err());
    drop(j);
    let j = Journal::open(v.clone()).unwrap();
    assert!(j.pending().unwrap().is_some());
    let mut other = b.clone();
    other.request_id = "00000000-0000-4000-8000-000000000009".into();
    assert!(j.consume(&other).is_err());
    let closed = Status {
        state: State::Closed,
        rollback_verified: true,
        guard_released: true,
        ..unknown
    };
    j.finish_closed(&closed, 2000).unwrap();
    assert!(j.pending().unwrap().is_none());
    assert!(j.consume(&b).is_err());
    drop(j);
    assert!(d
        .path()
        .join(format!("{}.consumed", b.journal_key().unwrap()))
        .is_file());
    let j = Journal::open(v).unwrap();
    assert!(j.consume(&b).is_err());
    j.consume(&other).unwrap();
}
#[test]
fn station_intent_is_immutable_password_free_and_survives_restart_without_replay() {
    let (_d, dir) = dir(0o700);
    let j = Journal::open(dir.clone()).unwrap();
    let r = crate::channel::station::tests::request();
    let intent = r.intent().unwrap();
    assert!(j.consume_trial(&intent).is_err());
    j.consume(&r.binding).unwrap();
    j.consume_trial(&intent).unwrap();
    let raw = dir
        .read_name(&format!("{}.intent", intent.key().unwrap()), 0o600)
        .unwrap()
        .unwrap();
    let text = String::from_utf8(raw).unwrap();
    assert!(!text.contains("synthetic") && !text.contains("passphrase"));
    assert!(j.consume_trial(&intent).is_err());
    drop(j);
    let j = Journal::open(dir).unwrap();
    assert!(j.trial_consumed(&intent).unwrap());
    assert!(j.trial_result(&intent).unwrap().is_none());
    assert!(j.consume_trial(&intent).is_err());
    let mut changed = intent;
    changed.request_sha256 = "f".repeat(64);
    assert!(j.trial_consumed(&changed).is_err());
}
#[test]
fn station_terminal_receipt_cannot_be_overwritten_or_forge_https_verification() {
    let (_d, dir) = dir(0o700);
    let j = Journal::open(dir).unwrap();
    let mut r = crate::channel::station::tests::request();
    let t = crate::channel::uptime_ms().unwrap();
    r.binding.created_uptime_ms = t;
    r.binding.deadline_uptime_ms = t + 120000;
    r.requested_uptime_ms = t;
    r.deadline_uptime_ms = t + 40000;
    let intent = r.intent().unwrap();
    j.consume(&r.binding).unwrap();
    j.consume_trial(&intent).unwrap();
    let mut status = crate::channel::station::Status {
        format: 1,
        intent: intent.clone(),
        phase: crate::channel::station::Phase::Ready,
        sample_uptime_ms: t,
        ap_https_verified: false,
    };
    assert!(j.finish_trial(&status).is_err());
    status.ap_https_verified = true;
    j.finish_trial(&status).unwrap();
    assert!(j.finish_trial(&status).is_err());
    assert!(j.trial_result(&intent).unwrap().unwrap().ap_https_verified);
}
