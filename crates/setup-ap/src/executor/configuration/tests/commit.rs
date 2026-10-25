// SPDX-License-Identifier: AGPL-3.0-only
//! Real Root loop, protected slots and durable journal; explicit network/TLS fixtures.
use super::*;
use crate::channel::commit::{self as protocol, Phase as SavePhase};
fn confirmation(f: &Fixture) -> protocol::Request {
    let trial = f.request();
    let t = channel::uptime_ms().unwrap();
    protocol::Request {
        format: 1,
        trial,
        commit_id: "10000000-0000-4000-8000-000000000004".into(),
        requested_uptime_ms: t,
        deadline_uptime_ms: t + 40_000,
    }
}
async fn trial_ready(f: &Fixture) {
    timeout(Duration::from_secs(4), async {
        loop {
            if let Some(bytes) = f.worker.output.read(Slot::StationStatus).unwrap() {
                let s: station::Status = serde_json::from_slice(&bytes).unwrap();
                if s.phase == station::Phase::Ready {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
fn done(f: &Fixture) -> protocol::Status {
    serde_json::from_slice(
        &f.worker
            .output
            .read(Slot::StationCommitStatus)
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
#[tokio::test]
async fn actual_root_confirmation_consumes_before_saving_and_never_rolls_back_committed_sta() {
    let f = Fixture::new(15000);
    let q = confirmation(&f);
    let intent = q.intent().unwrap();
    f.worker
        .input
        .publish(Slot::StationRequest, &q.trial)
        .unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        trial_ready(&f).await;
        f.worker
            .input
            .publish(Slot::StationCommitRequest, &q)
            .unwrap();
    };
    let (result, _) = tokio::join!(
        timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        ),
        monitor
    );
    assert!(result.unwrap().unwrap().is_none());
    let proof = done(&f);
    assert!(proof.phase == SavePhase::Saved && proof.guard_released && proof.settings_saved);
    assert!(f.worker.journal.pending().unwrap().is_none());
    assert!(f.worker.journal.commit_result(&intent).unwrap().is_some());
    assert!(f.worker.journal.closed(&f.hint.binding).unwrap().is_none());
    let before = m.lock().unwrap().events.clone();
    assert!(!before.contains(&"ap-rollback"));
    assert_eq!(before.iter().filter(|x| **x == "release").count(), 1);
    assert_eq!(
        m.lock().unwrap().commit_uuid.as_deref(),
        Some(q.commit_id.as_str())
    );
    assert!(f
        .worker
        .completed(&serde_json::to_vec(&f.hint).unwrap())
        .unwrap());
    assert_eq!(m.lock().unwrap().events, before);
    f.worker
        .input
        .publish(Slot::StationCommitQuery, &intent)
        .unwrap();
    assert!(f.worker.query_completed_commit().unwrap());
    assert_eq!(m.lock().unwrap().events, before);
    assert_eq!(done(&f).execution_uptime_ms, proof.execution_uptime_ms);
}
#[tokio::test]
async fn uncertain_finalization_retains_marker_and_guard_without_legacy_parent_rollback() {
    let f = Fixture::new(15000);
    let q = confirmation(&f);
    f.worker
        .input
        .publish(Slot::StationRequest, &q.trial)
        .unwrap();
    let m = Arc::new(StdMutex::new(Model {
        commit_unknown: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        trial_ready(&f).await;
        f.worker
            .input
            .publish(Slot::StationCommitRequest, &q)
            .unwrap();
    };
    let (result, _) = tokio::join!(
        timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        ),
        monitor
    );
    assert!(result.unwrap().unwrap().unwrap().state == State::Unknown);
    assert!(done(&f).phase == SavePhase::Unknown);
    assert!(f.worker.journal.pending().unwrap().is_some());
    assert!(f
        .worker
        .journal
        .commit_pending(&f.hint.binding)
        .unwrap()
        .is_some());
    let events = m.lock().unwrap().events.clone();
    assert!(!events.contains(&"release") && !events.contains(&"ap-rollback"));
    assert!(f.worker.recover().unwrap());
    assert!(done(&f).phase == SavePhase::Unknown);
    assert_eq!(m.lock().unwrap().events, events);
}
#[tokio::test]
async fn failed_save_or_stop_restores_ap_before_normal_parent_cleanup() {
    for stop in [false, true] {
        let f = Fixture::new(15000);
        let q = confirmation(&f);
        f.worker
            .input
            .publish(Slot::StationRequest, &q.trial)
            .unwrap();
        let m = Arc::new(StdMutex::new(Model {
            commit_fail_persist: !stop,
            commit_blocked: stop,
            ..Default::default()
        }));
        let (mut n, mut g, r) = setup(m.clone());
        let (tx, rx) = watch::channel(false);
        let ap = Credentials::generate().unwrap();
        let monitor = async {
            trial_ready(&f).await;
            f.worker
                .input
                .publish(Slot::StationCommitRequest, &q)
                .unwrap();
            wait(
                &m,
                if stop {
                    "commit-begin"
                } else {
                    "commit-restore"
                },
            )
            .await;
            if stop {
                assert!(f
                    .worker
                    .journal
                    .commit_consumed(&q.intent().unwrap())
                    .unwrap());
                assert!(done(&f).phase == SavePhase::Saving);
            }
            tx.send_replace(true);
        };
        let (result, _) = tokio::join!(
            timeout(
                Duration::from_secs(5),
                execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
            ),
            monitor
        );
        assert!(result.unwrap().unwrap().unwrap().state == State::Closed);
        assert!(done(&f).phase == SavePhase::RestoredAp);
        let events = m.lock().unwrap().events.clone();
        let restored = events.iter().position(|e| *e == "commit-restore").unwrap();
        let closed = events.iter().position(|e| *e == "ap-rollback").unwrap();
        assert!(restored < closed);
        assert!(!events.contains(&"commit-finish"));
        assert!(f.worker.journal.pending().unwrap().is_none());
    }
}
#[tokio::test]
async fn modified_original_credentials_have_no_confirmation_side_effects() {
    let f = Fixture::new(15000);
    let mut q = confirmation(&f);
    f.worker
        .input
        .publish(Slot::StationRequest, &q.trial)
        .unwrap();
    q.trial.operation = station::Operation::Test {
        ssid: "changed station".into(),
        passphrase: station::Secret::new(Zeroizing::new("changed-passphrase".into())),
    };
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        trial_ready(&f).await;
        f.worker
            .input
            .publish(Slot::StationCommitRequest, &q)
            .unwrap();
    };
    let (result, _) = tokio::join!(
        timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        ),
        monitor
    );
    assert!(result.unwrap().unwrap().unwrap().state == State::Closed);
    assert!(!m.lock().unwrap().events.contains(&"commit-begin"));
    assert!(!f
        .worker
        .journal
        .commit_consumed(&q.intent().unwrap())
        .unwrap());
}
