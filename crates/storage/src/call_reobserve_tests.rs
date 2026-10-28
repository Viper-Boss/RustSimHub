// SPDX-License-Identifier: AGPL-3.0-only
//! Reliable source identities only; never deduplicate by caller number.
use super::*;

fn context(h: &Handle, fingerprint: u8) -> SendContext {
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-call-reobserve".into(),
            fingerprint: Some([fingerprint; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    SendContext {
        line,
        backend: "synthetic-call-reobserve".into(),
        owner: ":1.20".into(),
        modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
        fingerprint: [fingerprint; 32],
    }
}
fn observed(source: &str, state: i32) -> CallObservation {
    CallObservation {
        source: source.into(),
        number: "+12025550123".into(),
        direction: 1,
        state,
        reason: 0,
    }
}
fn event_rows(h: &Handle) -> Vec<(String, String, String)> {
    let mut cursor = None;
    let mut out = vec![];
    for _ in 0..16 {
        let page = h.call_events(cursor, 32).unwrap();
        if page.events.is_empty() {
            return out;
        }
        out.extend(page.events.into_iter().map(|e| (e.seq, e.call_id, e.kind)));
        cursor = Some(page.cursor);
    }
    panic!("bounded synthetic call events did not end");
}

#[test]
fn prior_sim_source_collision_never_relabels_or_mutates_historical_call() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let first = context(&h, 81);
    let old = h
        .observe_call(first.clone(), observed("call/1#0", 4), || true)
        .unwrap();
    let second = context(&h, 82);
    assert_ne!(first.line, second.line);
    let before = event_rows(&h);
    assert!(matches!(
        h.observe_call(second.clone(), observed("call/1#0", 7), || true),
        Err(Error::StaleLine)
    ));
    let unchanged = h.call_detail(old.id.clone()).unwrap();
    assert_eq!(unchanged.line, first.line);
    assert_eq!(unchanged.modem_state, 4);
    assert_eq!(unchanged.availability, "reconciling");
    assert_eq!(event_rows(&h), before);
    let fresh = h
        .observe_call(second.clone(), observed("call/1#new-line", 7), || true)
        .unwrap();
    assert_ne!(fresh.id, old.id);
    assert_eq!(fresh.line, second.line);
    assert!(matches!(
        h.observe_call(first, observed("call/late#1", 7), || true),
        Err(Error::StaleLine)
    ));
    assert_eq!(h.calls(None, 32).unwrap().calls.len(), 2);
}

#[test]
fn repeated_and_concurrent_terminal_observations_keep_one_id_and_one_event() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = context(&h, 83);
    let mut jobs = vec![];
    for _ in 0..8 {
        let h = h.clone();
        let b = b.clone();
        jobs.push(std::thread::spawn(move || {
            h.observe_call(b, observed("call/terminal#7", 7), || true)
                .unwrap()
                .id
        }));
    }
    let ids: Vec<_> = jobs.into_iter().map(|j| j.join().unwrap()).collect();
    assert!(ids.iter().all(|id| id == &ids[0]));
    for _ in 0..16 {
        assert_eq!(
            h.observe_call(b.clone(), observed("call/terminal#7", 7), || true)
                .unwrap()
                .id,
            ids[0]
        );
    }
    assert_eq!(h.calls(None, 32).unwrap().calls.len(), 1);
    let events = event_rows(&h);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].2, "observed");
    let distinct = h
        .observe_call(b, observed("call/terminal#8", 7), || true)
        .unwrap();
    assert_ne!(distinct.id, ids[0]);
    assert_eq!(h.calls(None, 32).unwrap().calls.len(), 2);
}

#[test]
fn backend_loss_reopen_and_confirmed_reobservation_keep_terminal_id_and_history() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = context(&h, 84);
    let row = h
        .observe_call(b.clone(), observed("call/end#9", 7), || true)
        .unwrap();
    h.calls_unavailable(vec![row.id.clone()]).unwrap();
    h.calls_unavailable(vec![row.id.clone()]).unwrap();
    let before = event_rows(&h);
    assert_eq!(before.len(), 2);
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = context(&h, 84);
    assert_eq!(h.call_detail(row.id.clone()).unwrap().modem_state, 7);
    assert_eq!(
        h.call_detail(row.id.clone()).unwrap().availability,
        "reconciling"
    );
    assert_eq!(event_rows(&h), before);
    let again = h
        .observe_call(b.clone(), observed("call/end#9", 7), || true)
        .unwrap();
    assert_eq!(again.id, row.id);
    assert_eq!(event_rows(&h).len(), 3);
    for _ in 0..4 {
        assert_eq!(
            h.observe_call(b.clone(), observed("call/end#9", 7), || true)
                .unwrap()
                .id,
            row.id
        );
    }
    assert_eq!(event_rows(&h).len(), 3);
    let mut other = b;
    other.backend = "synthetic-other-mm-instance".into();
    other.line = h
        .observe_line(LineObservation {
            backend: other.backend.clone(),
            fingerprint: Some(other.fingerprint),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let different = h
        .observe_call(other, observed("call/end#9", 7), || true)
        .unwrap();
    assert_ne!(different.id, row.id);
}

#[test]
fn failed_call_event_write_rolls_back_new_record_and_terminal_update() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let b = context(&h, 85);
    let row = h
        .observe_call(b.clone(), observed("call/live#1", 3), || true)
        .unwrap();
    let before = event_rows(&h);
    h.call(|db|{db.connection.execute_batch("CREATE TEMP TRIGGER fail_call_observation BEFORE INSERT ON main.call_events BEGIN SELECT RAISE(ABORT,'synthetic call event failure'); END;")?;Ok(())}).unwrap();
    assert!(h
        .observe_call(b.clone(), observed("call/new#2", 7), || true)
        .is_err());
    assert!(h
        .observe_call(b.clone(), observed("call/live#1", 7), || true)
        .is_err());
    assert_eq!(h.calls(None, 32).unwrap().calls.len(), 1);
    assert_eq!(h.call_detail(row.id.clone()).unwrap().modem_state, 3);
    assert_eq!(event_rows(&h), before);
    h.call(|db| {
        db.connection
            .execute_batch("DROP TRIGGER fail_call_observation")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        h.observe_call(b, observed("call/live#1", 7), || true)
            .unwrap()
            .id,
        row.id
    );
    assert_eq!(event_rows(&h).len(), 2);
}
