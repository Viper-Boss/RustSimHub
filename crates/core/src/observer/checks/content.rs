// SPDX-License-Identifier: AGPL-3.0-only
//! Actual MM property decoder -> real inbox, never a second PDU decoder.
use super::*;

fn properties(text: &str, data: Vec<u8>) -> HashMap<String, OwnedValue> {
    HashMap::from([
        ("State".into(), OwnedValue::from(3u32)),
        ("PduType".into(), OwnedValue::from(1u32)),
        (
            "Number".into(),
            OwnedValue::from(zbus::zvariant::Str::from("synthetic")),
        ),
        (
            "Timestamp".into(),
            OwnedValue::from(zbus::zvariant::Str::from("synthetic-time")),
        ),
        (
            "Text".into(),
            OwnedValue::from(zbus::zvariant::Str::from(text)),
        ),
        (
            "Data".into(),
            zbus::zvariant::Value::from(data).try_into().unwrap(),
        ),
    ])
}

#[test]
fn complete_text_and_binary_use_byte_limits_and_retain_exact_content_in_real_store() {
    let d = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-content".into(),
            fingerprint: Some([21; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let samples = vec![
        ("中文\n🙂e\u{301}\u{200d}👩\u{200d}💻".into(), vec![]),
        ("🙂".repeat(1024), vec![]),
        ("界".repeat(1365) + "x", vec![]),
        ("".into(), vec![]),
        ("".into(), (0..4096).map(|n| (n % 256) as u8).collect()),
    ];
    for (n, (text, data)) in samples.into_iter().enumerate() {
        let value = decode_message(
            &properties(&text, data.clone()),
            format!("{}#1", path(n)),
            line,
        )
        .unwrap()
        .unwrap();
        assert_eq!(value.text, text);
        assert_eq!(value.data, data);
        let saved = h.receive_sms(value).unwrap();
        assert_eq!(saved.binary, !data.is_empty());
        let detail = h.sms_detail(saved.id).unwrap();
        assert_eq!(detail.text, text);
        assert_eq!(detail.data, data);
    }
    assert_eq!(message_count(&h), 5);
}

#[test]
fn incomplete_and_non_received_objects_never_turn_into_confirmed_partial_text() {
    let d = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-parts".into(),
            fingerprint: Some([22; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let mut source = Sources::default();
    let mut partial = properties("synthetic first part", vec![]);
    for state in [0, 1, 2, 4, 5, u32::MAX] {
        partial.insert("State".into(), OwnedValue::from(state));
        assert!(decode_message(&partial, path(1).to_string(), line)
            .unwrap()
            .is_none());
    }
    partial.insert("State".into(), OwnedValue::from(3u32));
    for kind in [0, 2, 3, 33, u32::MAX] {
        partial.insert("PduType".into(), OwnedValue::from(kind));
        assert!(decode_message(&partial, path(1).to_string(), line)
            .unwrap()
            .is_none());
    }
    assert_eq!(message_count(&h), 0);
    for kind in [1u32, 32] {
        let mut complete = properties("synthetic full assembled message", vec![]);
        complete.insert("PduType".into(), OwnedValue::from(kind));
        let mut value = decode_message(&complete, path(kind as usize).to_string(), line)
            .unwrap()
            .unwrap();
        source.bind(&mut value).unwrap();
        h.receive_sms(value).unwrap();
    }
    assert_eq!(message_count(&h), 2);
}

#[test]
fn malformed_properties_and_oversize_payloads_fail_without_partial_inbox_writes() {
    let d = directory();
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let h = store.handle();
    let line = h
        .observe_line(LineObservation {
            backend: "synthetic-invalid".into(),
            fingerprint: Some([23; 32]),
            status: LineStatus::Ready,
        })
        .unwrap()
        .line;
    let before = h.inbox_event_head().unwrap();
    for field in ["State", "PduType", "Number", "Timestamp", "Text", "Data"] {
        let mut missing = properties("synthetic", vec![]);
        missing.remove(field);
        assert!(
            decode_message(&missing, path(0).to_string(), line).is_err(),
            "missing {field}"
        );
        let mut wrong = properties("synthetic", vec![]);
        wrong.insert(field.into(), OwnedValue::from(false));
        assert!(
            decode_message(&wrong, path(0).to_string(), line).is_err(),
            "wrong type {field}"
        );
    }
    for (text, data) in [
        ("🙂".repeat(1024) + "x", vec![]),
        (String::new(), vec![0; 4097]),
        ("synthetic".into(), vec![1]),
    ] {
        assert!(decode_message(&properties(&text, data), path(0).to_string(), line).is_err());
    }
    for (field, length) in [("Number", 257), ("Timestamp", 129)] {
        let mut value = properties("synthetic", vec![]);
        value.insert(
            field.into(),
            OwnedValue::from(zbus::zvariant::Str::from("x".repeat(length))),
        );
        assert!(decode_message(&value, path(0).to_string(), line).is_err());
    }
    assert_eq!(message_count(&h), 0);
    assert_eq!(h.inbox_event_head().unwrap().seq, before.seq);
    assert_eq!(h.inbox_event_head().unwrap().db_epoch, before.db_epoch);
}
