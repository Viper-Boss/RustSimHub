// SPDX-License-Identifier: AGPL-3.0-only
use super::super::rescan as rpc;
use super::*;
use crate::{
    ap_rescan::{RescanNetwork, RescanProvider},
    channel::rescan as protocol,
    nm::scan::Snapshot,
};
struct ScanSession(Shared);
impl RescanNetwork for ScanSession {
    async fn pause_ap(&mut self) -> Result<()> {
        let blocked = {
            let mut m = self.0.lock().unwrap();
            m.events.push("rescan-pause");
            m.ap = false;
            m.blocked
        };
        if blocked {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
    async fn scan(&mut self) -> Result<Snapshot> {
        self.0.lock().unwrap().events.push("rescan-scan");
        let t = channel::uptime_ms()?;
        Ok(Snapshot {
            requested_uptime_ms: t,
            completed_uptime_ms: t,
            last_scan_uptime_ms: t,
            networks: vec![],
            omitted: 0,
            truncated: false,
        })
    }
    async fn restore_ap(&mut self) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        m.events.push("rescan-restore");
        if m.restore_failed {
            return Err(Error::Changed);
        }
        m.ap = true;
        Ok(())
    }
}
impl RescanProvider for Wifi {
    fn rescan_session<'a>(
        &'a mut self,
        _: &'a Credentials,
    ) -> Result<impl RescanNetwork + Send + 'a> {
        Ok(ScanSession(self.0.clone()))
    }
}
fn request(f: &Fixture) -> protocol::Request {
    let mut r = protocol::tests::request();
    let t = channel::uptime_ms().unwrap();
    r.binding = f.hint.binding.clone();
    r.requested_uptime_ms = t;
    r.deadline_uptime_ms = t + 40000;
    r
}
fn read(f: &Fixture) -> protocol::Status {
    serde_json::from_slice(&f.worker.output.read(Slot::RescanStatus).unwrap().unwrap()).unwrap()
}
#[tokio::test]
async fn protected_rescan_runs_once_publishes_empty_snapshot_and_preserves_parent_lease() {
    let f = Fixture::new(15000);
    let req = request(&f);
    let intent = req.intent().unwrap();
    f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "rescan-restore").await;
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let result = read(&f);
        assert!(result.phase == protocol::Phase::Ready && result.ap_https_verified);
        assert!(result.intent == intent && result.snapshot.unwrap().networks.is_empty());
        let proof = channel::scan::Proof::parse(
            &f.worker.output.read(Slot::ScanSnapshot).unwrap().unwrap(),
            &req.binding,
            channel::uptime_ms().unwrap(),
        )
        .unwrap();
        assert!(proof.snapshot.networks.is_empty());
        assert!(f.worker.journal.pending().unwrap() == Some(req.binding.clone()));
        let c = protocol::Capability::parse(
            &f.worker
                .output
                .read(Slot::RescanCapability)
                .unwrap()
                .unwrap(),
            &req.binding,
            channel::uptime_ms().unwrap(),
            &f.worker.boot,
        )
        .unwrap();
        assert!(!c.available && c.refusal == Some(protocol::Refusal::Cooldown));
        assert_eq!(c.remaining, protocol::MAX_PER_PARENT - 1);
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Closed);
    assert_eq!(
        m.lock()
            .unwrap()
            .events
            .iter()
            .filter(|e| **e == "rescan-pause")
            .count(),
        1
    );
    assert_eq!(
        m.lock()
            .unwrap()
            .events
            .iter()
            .filter(|e| **e == "rescan-scan")
            .count(),
        1
    );
    assert!(f.worker.journal.pending().unwrap().is_none());
    // A completed historical query never republishes a snapshot into a closed/new parent.
    f.worker.output.clear(Slot::ScanSnapshot).unwrap();
    f.worker.input.publish(Slot::RescanQuery, &intent).unwrap();
    assert!(f.worker.query_completed_rescan().unwrap());
    assert!(read(&f).phase == protocol::Phase::Ready);
    assert!(f.worker.output.read(Slot::ScanSnapshot).unwrap().is_none());
}
#[tokio::test]
async fn rescan_restore_https_or_authority_loss_keeps_parent_unknown_and_isolated() {
    for failure in 0..3 {
        let f = Fixture::new(15000);
        let req = request(&f);
        f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
        let m = Arc::new(StdMutex::new(Model {
            restore_failed: failure == 0,
            https_lost_after_restore: failure == 1,
            revoke_during_return_https: failure == 2,
            ..Default::default()
        }));
        let (mut n, mut g, r) = setup(m.clone());
        let (_tx, rx) = watch::channel(false);
        let ap = Credentials::generate().unwrap();
        let result = timeout(
            Duration::from_secs(5),
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        assert!(result.state == State::Unknown);
        assert!(read(&f).phase == protocol::Phase::Unknown);
        assert!(f.worker.output.read(Slot::ScanSnapshot).unwrap().is_none());
        assert!(f.worker.journal.pending().unwrap().is_some());
        let model = m.lock().unwrap();
        assert!(!model.events.contains(&"release"));
        assert!(!model.events.contains(&"ap-rollback"));
    }
}
#[tokio::test]
async fn exact_cancel_awaits_ap_restore_and_records_failed_without_repeating_scan() {
    let f = Fixture::new(15000);
    let mut req = request(&f);
    f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model {
        blocked: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "rescan-pause").await;
        req.action = protocol::Action::Cancel;
        f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
        wait(&m, "rescan-restore").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(read(&f).phase == protocol::Phase::Failed);
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Closed);
    assert!(!m.lock().unwrap().events.contains(&"rescan-scan"));
}
#[tokio::test]
async fn stop_during_pause_waits_for_restore_and_does_not_release_uncertain_parent() {
    let f = Fixture::new(15000);
    let req = request(&f);
    f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
    let m = Arc::new(StdMutex::new(Model {
        blocked: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "rescan-pause").await;
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Unknown);
    let model = m.lock().unwrap();
    assert!(model.events.contains(&"rescan-restore"));
    assert!(!model.events.contains(&"release"));
}
#[test]
fn query_after_restart_only_records_unknown_and_cannot_clear_parent_or_reconsume() {
    let f = Fixture::new(15000);
    let req = request(&f);
    let intent = req.intent().unwrap();
    f.worker.journal.consume(&req.binding).unwrap();
    f.worker.journal.consume_rescan(&intent).unwrap();
    f.worker.input.publish(Slot::RescanQuery, &intent).unwrap();
    assert!(f.worker.query_completed_rescan().unwrap());
    assert!(read(&f).phase == protocol::Phase::Unknown);
    assert!(f.worker.recover().unwrap());
    assert!(f.worker.journal.pending().unwrap().is_some());
    assert!(f.worker.journal.consume_rescan(&intent).is_err());
    assert!(f
        .worker
        .journal
        .ensure_rescans_closed(&req.binding)
        .is_err());
    let first = read(&f).finished_uptime_ms;
    assert!(f.worker.query_completed_rescan().unwrap());
    assert_eq!(read(&f).finished_uptime_ms, first);
}
#[tokio::test]
async fn lost_initial_status_output_consumes_intent_but_never_touches_radio_or_releases_guard() {
    let f = Fixture::new(15000);
    let req = request(&f);
    let intent = req.intent().unwrap();
    f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
    let output = f._dirs[1].path();
    let link = output.join("rescan-status.json");
    std::os::unix::fs::symlink(output.join("missing-target"), &link).unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let result = execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        .await
        .unwrap()
        .unwrap();
    assert!(result.state == State::Unknown);
    assert!(f.worker.journal.rescan_consumed(&intent).unwrap());
    assert!(!m.lock().unwrap().events.contains(&"rescan-pause"));
    assert!(!m.lock().unwrap().events.contains(&"release"));
    fs::remove_file(link).unwrap();
    f.worker.input.publish(Slot::RescanQuery, &intent).unwrap();
    assert!(f.worker.query_completed_rescan().unwrap());
    assert!(read(&f).phase == protocol::Phase::Unknown);
}
#[test]
fn station_conflicts_require_an_exact_explicit_cancel() {
    let f = Fixture::new(15000);
    let station = f.request();
    let intent = station.intent().unwrap();
    f.worker.journal.consume(&station.binding).unwrap();
    f.worker
        .input
        .publish(Slot::StationRequest, &station)
        .unwrap();
    assert!(rpc::conflicts(&f.worker, &station.binding, None).unwrap());
    f.worker.journal.consume_trial(&intent).unwrap();
    let ready = station::Status {
        format: 1,
        intent: intent.clone(),
        phase: station::Phase::Ready,
        sample_uptime_ms: channel::uptime_ms().unwrap(),
        ap_https_verified: true,
    };
    assert!(rpc::conflicts(&f.worker, &station.binding, Some(&ready)).unwrap());
    let mut cancel = station;
    cancel.operation = Operation::Cancel;
    f.worker
        .input
        .publish(Slot::StationRequest, &cancel)
        .unwrap();
    assert!(!rpc::conflicts(&f.worker, &cancel.binding, Some(&ready)).unwrap());
}

#[tokio::test]
async fn request_seen_with_station_work_cannot_run_later_when_station_is_cancelled() {
    let f = Fixture::new(15000);
    let scan = request(&f);
    let mut station = f.request();
    f.worker.input.publish(Slot::RescanRequest, &scan).unwrap();
    f.worker
        .input
        .publish(Slot::StationRequest, &station)
        .unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "station-restore").await;
        station.operation = Operation::Cancel;
        f.worker
            .input
            .publish(Slot::StationRequest, &station)
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert!(!m.lock().unwrap().events.contains(&"rescan-pause"));
        let s = read(&f);
        assert!(
            s.phase == protocol::Phase::Refused && s.refusal == Some(protocol::Refusal::Conflict)
        );
        assert!(f
            .worker
            .journal
            .rescan_consumed(&scan.intent().unwrap())
            .unwrap());
        assert!(!f._dirs[2]
            .path()
            .join(format!("{}.intent", scan.intent().unwrap().key().unwrap()))
            .exists());
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Closed);
}

#[test]
fn querying_an_old_success_never_replaces_a_newer_snapshot() {
    let f = Fixture::new(15000);
    let req = request(&f);
    let intent = req.intent().unwrap();
    f.worker.journal.consume(&req.binding).unwrap();
    f.worker.journal.consume_rescan(&intent).unwrap();
    let t = channel::uptime_ms().unwrap();
    let old = Snapshot {
        requested_uptime_ms: t,
        completed_uptime_ms: t,
        last_scan_uptime_ms: t,
        networks: vec![],
        omitted: 0,
        truncated: false,
    };
    let mut result = rpc::status(intent.clone(), protocol::Phase::Ready, Some(old)).unwrap();
    f.worker.journal.finish_rescan(&result).unwrap();
    // A separately completed later scan already owns the display slot.
    std::thread::sleep(Duration::from_millis(2));
    let later = channel::uptime_ms().unwrap();
    let newer = Snapshot {
        requested_uptime_ms: later,
        completed_uptime_ms: later,
        last_scan_uptime_ms: later,
        networks: vec![],
        omitted: 0,
        truncated: false,
    };
    f.worker
        .output
        .publish(
            Slot::ScanSnapshot,
            &channel::scan::Proof {
                format: 1,
                binding: req.binding.clone(),
                snapshot: newer,
            },
        )
        .unwrap();
    f.worker.input.publish(Slot::RescanQuery, &intent).unwrap();
    assert!(f.worker.query_completed_rescan().unwrap());
    let raw = f.worker.output.read(Slot::ScanSnapshot).unwrap().unwrap();
    let proof =
        channel::scan::Proof::parse(&raw, &req.binding, channel::uptime_ms().unwrap()).unwrap();
    assert_eq!(proof.snapshot.completed_uptime_ms, later);
    result.refresh().unwrap();
    assert_eq!(read(&f).finished_uptime_ms, result.finished_uptime_ms);
}

fn aged_fixture(age: u64) -> Fixture {
    let mut f = Fixture::new(15000);
    f.hint.binding.created_uptime_ms = f.hint.binding.created_uptime_ms.saturating_sub(age);
    f.worker.input.publish(Slot::Hint, &f.hint).unwrap();
    f
}
async fn refusal_ready(f: &Fixture, reason: protocol::Refusal) {
    timeout(Duration::from_secs(4), async {
        loop {
            if let Some(raw) = f.worker.output.read(Slot::RescanStatus).unwrap() {
                let s: protocol::Status = serde_json::from_slice(&raw).unwrap();
                if s.phase == protocol::Phase::Refused {
                    assert!(s.refusal == Some(reason));
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
fn history(f: &Fixture, offset: u64, id: usize) {
    let mut req = request(f);
    let t = channel::uptime_ms().unwrap();
    req.requested_uptime_ms = t - offset;
    req.deadline_uptime_ms = req.requested_uptime_ms + 40000;
    req.scan_id = format!("00000000-0000-4000-8000-{:012}", 100 + id);
    let intent = req.intent().unwrap();
    f.worker.journal.consume_rescan(&intent).unwrap();
    let s = protocol::Status {
        format: 1,
        intent,
        phase: protocol::Phase::Failed,
        sample_boot_id: f.worker.boot.clone(),
        sample_uptime_ms: t,
        finished_uptime_ms: Some(req.requested_uptime_ms + 1),
        ap_https_verified: true,
        snapshot: None,
        refusal: None,
    };
    f.worker.journal.finish_rescan(&s).unwrap();
}
#[tokio::test]
async fn stale_cancelled_or_closing_request_gets_a_durable_non_execution_receipt() {
    for reason in [
        protocol::Refusal::Stale,
        protocol::Refusal::Cancelled,
        protocol::Refusal::WindowClosing,
    ] {
        let mut f = aged_fixture(20000);
        if reason == protocol::Refusal::WindowClosing {
            f.hint.binding.deadline_uptime_ms = channel::uptime_ms().unwrap() + 50000;
            f.worker.input.publish(Slot::Hint, &f.hint).unwrap();
        }
        let mut req = request(&f);
        if reason == protocol::Refusal::Stale {
            req.requested_uptime_ms -= 16000;
            req.deadline_uptime_ms = req.requested_uptime_ms + 40000;
        }
        if reason == protocol::Refusal::Cancelled {
            req.action = protocol::Action::Cancel;
        }
        if reason == protocol::Refusal::WindowClosing {
            req.deadline_uptime_ms = req.requested_uptime_ms + 20000;
        }
        let intent = req.intent().unwrap();
        f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
        let m = Arc::new(StdMutex::new(Model::default()));
        let (mut n, mut g, r) = setup(m.clone());
        let (tx, rx) = watch::channel(false);
        let ap = Credentials::generate().unwrap();
        let monitor = async {
            refusal_ready(&f, reason).await;
            assert!(!read(&f).ap_https_verified);
            tx.send_replace(true);
        };
        let (result, _) = tokio::join!(
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
            monitor
        );
        assert!(result.unwrap().unwrap().state == State::Closed);
        assert!(!m.lock().unwrap().events.contains(&"rescan-pause"));
        f.worker.input.publish(Slot::RescanQuery, &intent).unwrap();
        assert!(f.worker.query_completed_rescan().unwrap());
        assert!(read(&f).phase == protocol::Phase::Refused && read(&f).refusal == Some(reason));
    }
}
#[tokio::test]
async fn cooldown_and_exhausted_budget_are_authoritative_without_any_radio_work() {
    for reason in [protocol::Refusal::Cooldown, protocol::Refusal::Limit] {
        let f = aged_fixture(300000);
        let m = Arc::new(StdMutex::new(Model::default()));
        let (mut n, mut g, r) = setup(m.clone());
        let (tx, rx) = watch::channel(false);
        let ap = Credentials::generate().unwrap();
        let monitor = async {
            wait(&m, "ap-activate").await;
            if reason == protocol::Refusal::Cooldown {
                history(&f, 10000, 0);
            } else {
                for i in 0..8 {
                    history(&f, 240000 - i as u64 * 30000, i);
                }
            }
            let req = request(&f);
            f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
            refusal_ready(&f, reason).await;
            let c = protocol::Capability::parse(
                &f.worker
                    .output
                    .read(Slot::RescanCapability)
                    .unwrap()
                    .unwrap(),
                &req.binding,
                channel::uptime_ms().unwrap(),
                &f.worker.boot,
            )
            .unwrap();
            assert!(!c.available && c.refusal == Some(reason));
            assert_eq!(
                c.remaining,
                if reason == protocol::Refusal::Limit {
                    0
                } else {
                    7
                }
            );
            tx.send_replace(true);
        };
        let (result, _) = tokio::join!(
            execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
            monitor
        );
        assert!(result.unwrap().unwrap().state == State::Closed);
        assert!(!m.lock().unwrap().events.contains(&"rescan-pause"));
    }
}
#[tokio::test]
async fn a_scan_posted_during_station_work_is_refused_after_station_cleanup() {
    let f = Fixture::new(15000);
    let mut station = f.request();
    f.worker
        .input
        .publish(Slot::StationRequest, &station)
        .unwrap();
    let m = Arc::new(StdMutex::new(Model {
        blocked: true,
        ..Default::default()
    }));
    let (mut n, mut g, r) = setup(m.clone());
    let (tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let monitor = async {
        wait(&m, "station-begin").await;
        let req = request(&f);
        f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
        station.operation = Operation::Cancel;
        f.worker
            .input
            .publish(Slot::StationRequest, &station)
            .unwrap();
        refusal_ready(&f, protocol::Refusal::Superseded).await;
        assert!(!m.lock().unwrap().events.contains(&"rescan-pause"));
        tx.send_replace(true);
    };
    let (result, _) = tokio::join!(
        execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx),
        monitor
    );
    assert!(result.unwrap().unwrap().state == State::Closed);
}

#[tokio::test]
async fn a_lost_refusal_reply_stays_durable_without_becoming_a_scan_after_query() {
    let f = Fixture::new(15000);
    let mut req = request(&f);
    req.action = protocol::Action::Cancel;
    let intent = req.intent().unwrap();
    f.worker.input.publish(Slot::RescanRequest, &req).unwrap();
    let output = f._dirs[1].path();
    let link = output.join("rescan-status.json");
    std::os::unix::fs::symlink(output.join("missing-target"), &link).unwrap();
    let m = Arc::new(StdMutex::new(Model::default()));
    let (mut n, mut g, r) = setup(m.clone());
    let (_tx, rx) = watch::channel(false);
    let ap = Credentials::generate().unwrap();
    let result = execute(&f.worker, &mut n, &mut g, &r, &ap, f.hint.clone(), rx)
        .await
        .unwrap()
        .unwrap();
    assert!(result.state == State::Unknown);
    assert!(!m.lock().unwrap().events.contains(&"rescan-pause"));
    let stored = f.worker.journal.rescan_result(&intent).unwrap().unwrap();
    assert!(
        stored.phase == protocol::Phase::Refused
            && stored.refusal == Some(protocol::Refusal::Cancelled)
    );
    fs::remove_file(link).unwrap();
    let before = m.lock().unwrap().events.clone();
    f.worker.input.publish(Slot::RescanQuery, &intent).unwrap();
    assert!(f.worker.query_completed_rescan().unwrap());
    assert!(read(&f).phase == protocol::Phase::Refused);
    assert_eq!(m.lock().unwrap().events, before);
    // A later Scan with the identical ID/body is already terminally refused.
    req.action = protocol::Action::Scan;
    assert!(f
        .worker
        .journal
        .consume_rescan(&req.intent().unwrap())
        .is_err());
}
