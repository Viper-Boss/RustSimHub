// SPDX-License-Identifier: AGPL-3.0-only
//! D-02: multi-page replay and real SQLite failures, with synthetic data only.
use super::*;
fn cursor_key(c: InboxCursor) -> (String, String) {
    (c.db_epoch, c.seq)
}

fn ready(h: &Handle) -> LineEpoch {
    h.observe_line(LineObservation {
        backend: "synthetic-recovery".into(),
        fingerprint: Some([42; 32]),
        status: LineStatus::Ready,
    })
    .unwrap()
    .line
}
fn message(line: LineEpoch, n: usize) -> IncomingSms {
    IncomingSms {
        line,
        source: format!("/org/freedesktop/ModemManager1/SMS/{n}#{}", n + 1),
        sender: "synthetic".into(),
        timestamp: "synthetic-time".into(),
        text: format!("合成恢复消息 {n} 🙂"),
        data: vec![],
    }
}
fn visible(h: &Handle, limit: usize) -> Vec<String> {
    let mut cursor = None;
    let mut ids = Vec::new();
    for _ in 0..200 {
        let page = h.inbox(cursor.clone(), limit).unwrap();
        if page.messages.is_empty() {
            return ids;
        }
        if let Some(previous) = cursor.as_ref() {
            assert!(page.cursor.seq.parse::<u64>().unwrap() > previous.seq.parse::<u64>().unwrap());
        }
        ids.extend(page.messages.into_iter().map(|s| s.id));
        cursor = Some(page.cursor);
    }
    panic!("bounded synthetic pagination did not terminate");
}
fn events(h: &Handle, limit: usize) -> Vec<(String, Option<String>, String)> {
    let mut cursor = None;
    let mut rows = Vec::new();
    for _ in 0..200 {
        let page = h.inbox_events(cursor.clone(), limit).unwrap();
        if page.events.is_empty() {
            return rows;
        }
        rows.extend(page.events.into_iter().map(|e| (e.seq, e.sms_id, e.kind)));
        cursor = Some(page.cursor);
    }
    panic!("bounded synthetic event replay did not terminate");
}

#[test]
fn pagination_skips_tombstone_gaps_and_reopen_keeps_ids_and_event_sequences() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([43; 32]).unwrap();
    let line = ready(&h);
    let ids: Vec<_> = (0..70)
        .map(|n| h.receive_sms(message(line, n)).unwrap().id)
        .collect();
    for id in &ids[..34] {
        h.set_sms_deleted(owner.clone(), id.clone(), true).unwrap();
    }
    for n in (0..70).rev() {
        assert_eq!(h.receive_sms(message(line, n)).unwrap().id, ids[n]);
    }
    for size in [1, 16, 32] {
        assert_eq!(visible(&h, size), ids[34..]);
    }
    let original = events(&h, 32);
    assert_eq!(original.len(), 105); // one line change, 70 receives, 34 deletes
    assert_eq!(events(&h, 1), original);
    assert_eq!(events(&h, 16), original);
    let cursor = h.inbox_event_head().unwrap();
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    assert_eq!(
        cursor_key(h.inbox_event_head().unwrap()),
        cursor_key(cursor.clone())
    );
    assert_eq!(visible(&h, 32), ids[34..]);
    assert_eq!(events(&h, 16), original);
    h.set_sms_deleted(owner, ids[0].clone(), false).unwrap();
    let catchup = h.inbox_events(Some(cursor), 32).unwrap();
    assert_eq!(catchup.events.len(), 1);
    assert_eq!(catchup.events[0].sms_id.as_ref(), Some(&ids[0]));
    assert_eq!(catchup.events[0].kind, "sms_restored");
    let mut expected = vec![ids[0].clone()];
    expected.extend_from_slice(&ids[34..]);
    assert_eq!(visible(&h, 16), expected);
}

#[test]
fn failed_receive_never_advances_event_head_and_retry_after_reopen_is_once() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = ready(&h);
    let head = h.inbox_event_head().unwrap();
    h.call(|db| { db.connection.execute_batch("CREATE TEMP TRIGGER fail_receive BEFORE INSERT ON main.inbox_events WHEN NEW.kind='sms_received' BEGIN SELECT RAISE(ABORT,'synthetic event failure'); END;")?; Ok(()) }).unwrap();
    assert!(h.receive_sms(message(line, 0)).is_err());
    assert_eq!(
        cursor_key(h.inbox_event_head().unwrap()),
        cursor_key(head.clone())
    );
    assert!(visible(&h, 32).is_empty());
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    assert_eq!(
        cursor_key(h.inbox_event_head().unwrap()),
        cursor_key(head.clone())
    );
    assert_eq!(ready(&h), line);
    let id = h.receive_sms(message(line, 0)).unwrap().id;
    for _ in 0..8 {
        assert_eq!(h.receive_sms(message(line, 0)).unwrap().id, id);
    }
    assert_eq!(visible(&h, 1), vec![id.clone()]);
    let new = h.inbox_events(Some(head), 32).unwrap();
    assert_eq!(
        new.events
            .iter()
            .filter(|e| e.kind == "sms_received")
            .count(),
        1
    );
    assert_eq!(
        new.events
            .iter()
            .find(|e| e.kind == "sms_received")
            .unwrap()
            .sms_id
            .as_ref(),
        Some(&id)
    );
}

#[test]
fn failed_page_read_does_not_consume_cursor_or_change_persisted_events() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = ready(&h);
    let first = h.inbox_events(None, 1).unwrap().cursor;
    let id = h.receive_sms(message(line, 0)).unwrap().id;
    let head = h.inbox_event_head().unwrap();
    // A temporary view induces a real query/decode error without touching the durable table.
    h.call(|db| { db.connection.execute_batch("CREATE TEMP VIEW inbox_events AS SELECT 2 AS seq, NULL AS sms_id, 1 AS line_epoch, NULL AS kind;")?; Ok(()) }).unwrap();
    assert!(h.inbox_events(Some(first.clone()), 32).is_err());
    h.call(|db| {
        db.connection.execute_batch("DROP VIEW temp.inbox_events")?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        cursor_key(h.inbox_event_head().unwrap()),
        cursor_key(head.clone())
    );
    let page = h.inbox_events(Some(first.clone()), 32).unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].sms_id.as_ref(), Some(&id));
    // Lost reply / duplicated fetch: the exact same durable event is returned.
    assert_eq!(
        h.inbox_events(Some(first), 32).unwrap().events[0].seq,
        page.events[0].seq
    );
}

#[test]
fn malformed_or_foreign_cursors_never_skip_history_and_restore_rotates_realm() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = ready(&h);
    h.receive_sms(message(line, 0)).unwrap();
    let head = h.inbox_event_head().unwrap();
    for seq in [
        "",
        "-1",
        "1x",
        "18446744073709551616",
        "9999999999999999999",
    ] {
        let cursor = InboxCursor {
            db_epoch: head.db_epoch.clone(),
            seq: seq.into(),
        };
        assert!(matches!(
            h.inbox_events(Some(cursor.clone()), 32),
            Err(Error::ResyncRequired)
        ));
        assert!(matches!(
            h.inbox(Some(cursor), 32),
            Err(Error::ResyncRequired)
        ));
        assert_eq!(
            cursor_key(h.inbox_event_head().unwrap()),
            cursor_key(head.clone())
        );
    }
    for size in [0, 33, usize::MAX] {
        assert!(matches!(
            h.inbox_events(None, size),
            Err(Error::InvalidInput)
        ));
    }
    let backup = super::tests::directory();
    h.backup(backup.path()).unwrap();
    let restored = super::tests::directory();
    let recovered = Store::restore(backup.path(), restored.path()).unwrap();
    let recovered_h = recovered.handle();
    assert!(matches!(
        recovered_h.inbox_events(Some(head), 32),
        Err(Error::ResyncRequired)
    ));
    assert_eq!(visible(&recovered_h, 32), visible(&h, 32));
}
