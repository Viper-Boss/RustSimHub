// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> (tempfile::TempDir, Connection, Binding) {
    // Deliberately downgrade a synthetic HTTPS-only fixture's representation.
    // Separate process tests use the actual unmodified format3 executable.
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let id = || Uuid::new_v4().to_string();
    let b = Binding {
        device_key: [1; 32],
        owner_key: [2; 32],
        owner_generation: 7,
        realm: id(),
    };
    let target = id();
    let rule = id();
    let mut q = Queue::open(d.path(), b.clone()).unwrap();
    q.configure(
        &b,
        0,
        &id(),
        Policy {
            enabled: true,
            targets: vec![Target {
                id: target.clone(),
                destination: Destination::Webhook {
                    url: Secret::new("https://example.invalid/original".into()),
                    bearer: Some(Secret::new("SYNTHETIC_CREDENTIAL".into())),
                },
            }],
            rules: vec![Rule {
                id: rule.clone(),
                targets: vec![target],
                sender_equals: None,
                body_contains: None,
                line: None,
                share: Share::FullText,
                conditions: None,
            }],
        },
        InboxCursor {
            db_epoch: b.realm.clone(),
            seq: "10".into(),
        },
    )
    .unwrap();
    for seq in 11..=14 {
        q.ingest(
            &b,
            1,
            InboxCursor {
                db_epoch: b.realm.clone(),
                seq: seq.to_string(),
            },
            Some(&Message {
                id: id(),
                line: LineEpoch(2),
                sender: "SYNTHETIC_SENDER".into(),
                timestamp: "SYNTHETIC_TIME".into(),
                body: "SYNTHETIC_BODY 123456".into(),
                duplicate_candidate: false,
                binary: false,
            }),
        )
        .unwrap();
    }
    let claim = q.claim(&b, 1, LineEpoch(2)).unwrap().unwrap();
    q.finish(&b, &claim, State::Unknown).unwrap();
    let _inflight = q.claim(&b, 1, LineEpoch(2)).unwrap().unwrap();
    drop(q);
    let conn = Connection::open(d.path().join("outbox.sqlite3")).unwrap();
    conn.execute_batch(
        "UPDATE jobs SET request=json_extract(request,'$.request');PRAGMA user_version=3;",
    )
    .unwrap();
    (d, conn, b)
}
type JobRow = (
    i64,
    String,
    Option<String>,
    i64,
    i64,
    String,
    Option<String>,
    Option<String>,
);
fn dump(conn: &Connection) -> Vec<JobRow> {
    conn.prepare(
        "SELECT seq,id,attempt_id,generation,line,state,request,purpose FROM jobs ORDER BY seq",
    )
    .unwrap()
    .query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
            r.get(6)?,
            r.get(7)?,
        ))
    })
    .unwrap()
    .collect::<std::result::Result<_, _>>()
    .unwrap()
}
#[test]
fn conversion_preserves_original_identity_payload_cursor_dedupe_and_unknown() {
    let (d, mut conn, b) = fixture();
    let before = dump(&conn);
    let meta: (String, i64, i64, String) = conn
        .query_row(
            "SELECT binding,generation,cursor,policy FROM meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    let requests: Vec<(String, Vec<u8>, i64)> = conn
        .prepare("SELECT id,digest,generation FROM requests")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    convert(&mut conn, || Ok(())).unwrap();
    let after = dump(&conn);
    for (old, new) in before.iter().zip(after.iter()) {
        assert_eq!(
            (&old.0, &old.1, &old.2, &old.3, &old.4, &old.7),
            (&new.0, &new.1, &new.2, &new.3, &new.4, &new.7)
        );
        assert_eq!(
            new.5,
            if old.5 == "in_flight" {
                "unknown"
            } else {
                &old.5
            }
        );
        assert_eq!(
            new.6.as_ref().unwrap(),
            &format!(
                "{{\"transport\":\"https\",\"request\":{}}}",
                old.6.as_ref().unwrap()
            )
        );
    }
    assert_eq!(
        meta,
        conn.query_row(
            "SELECT binding,generation,cursor,policy FROM meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        )
        .unwrap()
    );
    let saved: Vec<(String, Vec<u8>, i64)> = conn
        .prepare("SELECT id,digest,generation FROM requests")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(requests, saved);
    drop(conn);
    let mut q = Queue::open(d.path(), b.clone()).unwrap();
    assert_eq!(q.status().unwrap().unknown, 2);
    assert_eq!(q.event_cursor().unwrap().seq, "14");
    let claim = q.claim(&b, 1, LineEpoch(2)).unwrap().unwrap();
    assert_eq!(claim.id, after[2].1);
    assert!(!claim.request.is_smtp());
}
#[test]
fn missing_or_lost_root_commit_ack_rolls_back_every_change() {
    let (_d, mut conn, _b) = fixture();
    let before = dump(&conn);
    assert_eq!(
        convert(&mut conn, || Err(Error::Forbidden)),
        Err(Error::Forbidden)
    );
    assert_eq!(dump(&conn), before);
    assert_eq!(
        conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        3
    );
    convert(&mut conn, || Ok(())).unwrap();
    assert!(convert(&mut conn, || Ok(())).is_err());
}
#[test]
fn malformed_or_mixed_request_and_unknown_schema_never_partially_migrate() {
    for mutation in [
        "UPDATE jobs SET request=json_set(request,'$.transport','smtp') WHERE seq=3",
        "CREATE TRIGGER injected AFTER UPDATE ON jobs BEGIN DELETE FROM requests;END",
        "UPDATE jobs SET request=' '||request WHERE seq=3",
        "UPDATE meta SET policy=json_set(policy,'$.extra',true)",
    ] {
        let (_d, mut conn, _b) = fixture();
        conn.execute_batch(mutation).unwrap();
        let before = dump(&conn);
        assert!(convert(&mut conn, || panic!(
            "Root must not be asked to commit invalid data"
        ))
        .is_err());
        assert_eq!(dump(&conn), before);
        assert_eq!(
            conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            3
        );
    }
}
#[test]
fn ordinary_candidate_open_refuses_format3_without_modifying_source_bytes() {
    let (d, conn, b) = fixture();
    drop(conn);
    let before = fs::read(d.path().join("outbox.sqlite3")).unwrap();
    assert!(matches!(Queue::open(d.path(), b), Err(Error::Forbidden)));
    assert_eq!(fs::read(d.path().join("outbox.sqlite3")).unwrap(), before);
}
#[test]
fn prepared_legacy_history_expires_but_terminal_receipts_keep_original_ids() {
    let (_d, mut conn, b) = fixture();
    let mut ids = vec![];
    for state in ["prepared", "submitted", "cancelled", "expired"] {
        let id = Uuid::new_v4().to_string();
        let confirmation = Uuid::new_v4().to_string();
        let plan = serde_json::json!({"preview":{"id":id,"confirmation_id":confirmation,"generation":1,"line":2,"range":{"after":null,"limit":1,"targets":[Uuid::new_v4().to_string()]},"cursor":{"db_epoch":b.realm,"seq":"14"},"scanned":0,"skipped_messages":0,"already_queued_or_recorded":0,"deliveries":[]},"digests":[]});
        conn.execute(
            "INSERT INTO history_batches VALUES(?,1,2,?,?)",
            params![id, state, plan.to_string()],
        )
        .unwrap();
        ids.push((id, state, confirmation));
    }
    convert(&mut conn, || Ok(())).unwrap();
    for (id, state, confirmation) in ids {
        let (actual, plan): (String, String) = conn
            .query_row(
                "SELECT state,plan FROM history_batches WHERE id=?",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let plan: serde_json::Value = serde_json::from_str(&plan).unwrap();
        assert_eq!(
            actual,
            if state == "prepared" {
                "expired"
            } else {
                state
            }
        );
        assert_eq!(plan["preview"]["id"], id);
        assert_eq!(plan["preview"]["confirmation_id"], confirmation);
        assert_eq!(plan["prepared_unix_seconds"], 0);
    }
}
