// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

fn ready(h: &Handle, fingerprint: u8) -> LineEpoch {
    h.observe_line(LineObservation {
        backend: "synthetic-content".into(),
        fingerprint: Some([fingerprint; 32]),
        status: LineStatus::Ready,
    })
    .unwrap()
    .line
}
fn incoming(line: LineEpoch, source: &str, text: &str) -> IncomingSms {
    IncomingSms {
        line,
        source: source.into(),
        sender: "synthetic".into(),
        timestamp: "synthetic-time".into(),
        text: text.into(),
        data: vec![],
    }
}

#[test]
fn composed_and_decomposed_unicode_and_binary_survive_database_reopen_without_coalescing() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = ready(&h, 31);
    let inputs = ["é", "e\u{301}", "👩\u{200d}💻\n中文", ""];
    let mut rows = Vec::new();
    for (n, text) in inputs.iter().enumerate() {
        let saved = h
            .receive_sms(incoming(line, &format!("synthetic/{n}"), text))
            .unwrap();
        assert!(saved.duplicate_candidate.is_none());
        rows.push((saved.id, text.to_string(), vec![]));
    }
    let binary: Vec<u8> = (0..4096).map(|n| (n % 256) as u8).collect();
    let mut input = incoming(line, "synthetic/binary", "");
    input.data = binary.clone();
    let saved = h.receive_sms(input).unwrap();
    assert!(saved.binary);
    rows.push((saved.id, String::new(), binary));
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    for (id, text, data) in rows {
        let detail = h.sms_detail(id).unwrap();
        assert_eq!(detail.text, text);
        assert_eq!(detail.data, data);
    }
    assert_eq!(h.inbox(None, 32).unwrap().messages.len(), 5);
}

#[test]
fn real_store_byte_boundaries_reject_without_consuming_event_sequences() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = ready(&h, 32);
    let at_limit = "🙂".repeat(1024);
    let id = h
        .receive_sms(incoming(line, "synthetic/limit", &at_limit))
        .unwrap()
        .id;
    assert_eq!(h.sms_detail(id).unwrap().text.len(), 4096);
    let before = h.inbox_event_head().unwrap();
    let mut values = vec![incoming(line, "synthetic/long", &(at_limit + "x"))];
    let mut binary = incoming(line, "synthetic/long-binary", "");
    binary.data = vec![0; 4097];
    values.push(binary);
    let mut mixed = incoming(line, "synthetic/mixed", "nonempty");
    mixed.data = vec![1];
    values.push(mixed);
    for input in values {
        assert!(matches!(h.receive_sms(input), Err(Error::InvalidInput)));
        assert_eq!(h.inbox_event_head().unwrap().seq, before.seq);
    }
    assert_eq!(h.inbox(None, 32).unwrap().messages.len(), 1);
}

#[test]
fn duplicate_candidate_tombstone_restore_and_cross_boot_preserve_each_independent_id() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([33; 32]).unwrap();
    let line = ready(&h, 33);
    let old = h
        .receive_sms(incoming(
            line,
            "synthetic/old#1",
            "identical synthetic payload",
        ))
        .unwrap();
    let current = h
        .receive_sms(incoming(
            line,
            "synthetic/new#2",
            "identical synthetic payload",
        ))
        .unwrap();
    assert_ne!(old.id, current.id);
    assert_eq!(current.duplicate_candidate.as_ref(), Some(&old.id));
    h.set_sms_deleted(owner.clone(), current.id.clone(), true)
        .unwrap();
    let head = h.inbox_event_head().unwrap();
    for source in ["synthetic/old#1", "synthetic/new#2"] {
        h.receive_sms(incoming(line, source, "identical synthetic payload"))
            .unwrap();
    }
    assert_eq!(h.inbox_event_head().unwrap().seq, head.seq);
    assert_eq!(h.inbox(None, 32).unwrap().messages[0].id, old.id);
    assert_eq!(
        h.deleted_sms_detail(owner.clone(), current.id.clone())
            .unwrap()
            .summary
            .duplicate_candidate
            .as_ref(),
        Some(&old.id)
    );
    drop(store);
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    assert_eq!(ready(&h, 33), line);
    assert!(matches!(
        h.sms_detail(current.id.clone()),
        Err(Error::NotFound)
    ));
    h.set_sms_deleted(owner.clone(), current.id.clone(), false)
        .unwrap();
    assert_eq!(
        h.sms_detail(current.id.clone())
            .unwrap()
            .summary
            .duplicate_candidate
            .as_ref(),
        Some(&old.id)
    );
    assert_eq!(
        h.sms_detail(old.id.clone()).unwrap().text,
        "identical synthetic payload"
    );
    assert_eq!(
        h.receive_sms(incoming(
            line,
            "synthetic/new#2",
            "identical synthetic payload"
        ))
        .unwrap()
        .id,
        current.id
    );
    let new_line = ready(&h, 34);
    let other = h
        .receive_sms(incoming(
            new_line,
            "synthetic/old#1",
            "identical synthetic payload",
        ))
        .unwrap();
    assert!(other.duplicate_candidate.is_none());
    assert_ne!(other.id, old.id);
    assert_eq!(h.inbox(None, 32).unwrap().messages.len(), 3);
}

#[test]
fn restored_backup_keeps_duplicate_edges_and_tombstones_but_quarantines_old_owner() {
    let d = super::tests::directory();
    let store = Store::open(d.path()).unwrap();
    let h = store.handle();
    let owner = h.bootstrap_owner([35; 32]).unwrap();
    let line = ready(&h, 35);
    let old = h
        .receive_sms(incoming(line, "synthetic/a", "same"))
        .unwrap();
    let current = h
        .receive_sms(incoming(line, "synthetic/b", "same"))
        .unwrap();
    h.set_sms_deleted(owner.clone(), current.id.clone(), true)
        .unwrap();
    let backup = super::tests::directory();
    h.backup(backup.path()).unwrap();
    let target = super::tests::directory();
    let restored = Store::restore(backup.path(), target.path()).unwrap();
    let rh = restored.handle();
    assert!(matches!(
        rh.deleted_sms_detail(owner, current.id.clone()),
        Err(Error::Forbidden)
    ));
    let quarantined = rh.principal([35; 32]).unwrap().unwrap();
    assert!(!quarantined.active);
    // Trusted offline recovery fixture, not a browser or anonymous claim path.
    let recovered = rh.recover_owner(quarantined).unwrap();
    let detail = rh
        .deleted_sms_detail(recovered.clone(), current.id.clone())
        .unwrap();
    assert_eq!(detail.summary.duplicate_candidate.as_ref(), Some(&old.id));
    assert_eq!(rh.sms_detail(old.id.clone()).unwrap().text, "same");
    rh.set_sms_deleted(recovered, current.id.clone(), false)
        .unwrap();
    assert_eq!(
        rh.inbox(None, 32)
            .unwrap()
            .messages
            .into_iter()
            .map(|s| s.id)
            .collect::<Vec<_>>(),
        vec![old.id, current.id]
    );
}
