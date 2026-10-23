// SPDX-License-Identifier: AGPL-3.0-only
//! Device-wide logical deletion. Modem storage and message provenance remain intact.
use super::*;
use rustsimhub_policy::{allows, Principal, Scope};

#[derive(Debug, Serialize)]
pub struct SmsVisibility {
    pub sms_id: String,
    pub deleted: bool,
}

fn owner(connection: &Connection, principal: &Principal) -> Result<()> {
    super::auth::checked_owner(connection, principal)?;
    if !allows(principal.scopes, Scope::SmsRead) {
        return Err(Error::Forbidden);
    }
    Ok(())
}

impl Handle {
    pub fn set_sms_deleted(
        &self,
        principal: Principal,
        id: String,
        deleted: bool,
    ) -> Result<SmsVisibility> {
        self.call(move |db| {
            if Uuid::parse_str(&id).is_err() {
                return Err(Error::InvalidInput);
            }
            let tx = db.connection.transaction()?;
            owner(&tx, &principal)?;
            let line: i64 = tx
                .query_row("SELECT line_epoch FROM inbox WHERE id=?", [&id], |r| {
                    r.get(0)
                })
                .optional()?
                .ok_or(Error::NotFound)?;
            let changed = if deleted {
                tx.execute(
                    "INSERT OR IGNORE INTO inbox_tombstones(sms_id,deleted_by) VALUES(?,?)",
                    params![id, principal.id],
                )?
            } else {
                tx.execute("DELETE FROM inbox_tombstones WHERE sms_id=?", [&id])?
            };
            if changed != 0 {
                tx.execute(
                    "INSERT INTO inbox_events(sms_id,line_epoch,kind) VALUES(?,?,?)",
                    params![
                        id,
                        line,
                        if deleted {
                            "sms_deleted"
                        } else {
                            "sms_restored"
                        }
                    ],
                )?;
            }
            tx.commit()?;
            Ok(SmsVisibility {
                sms_id: id,
                deleted,
            })
        })
    }

    pub fn deleted_inbox(
        &self,
        principal: Principal,
        after: Option<InboxCursor>,
        limit: usize,
    ) -> Result<InboxPage> {
        self.call(move |db| {
            owner(&db.connection, &principal)?;
            let (epoch, mut seq) = super::inbox::position(&db.connection, after.as_ref(), "inbox", limit)?;
            let mut statement = db.connection.prepare(
                "SELECT id,line_epoch,length(data)>0,duplicate_candidate,i.seq,sender,timestamp,substr(body,1,96) FROM inbox i JOIN inbox_tombstones t ON t.sms_id=i.id WHERE i.seq>? ORDER BY i.seq LIMIT ?",
            )?;
            let rows = statement.query_map(params![seq, limit as i64], |r| {
                Ok((super::inbox::list_summary(r)?, r.get::<_, i64>(4)?))
            })?;
            let mut messages = Vec::new();
            for row in rows {
                let (message, next) = row?;
                messages.push(message);
                seq = next;
            }
            Ok(InboxPage { messages, cursor: InboxCursor { db_epoch: epoch, seq: seq.to_string() } })
        })
    }

    pub fn deleted_sms_detail(&self, principal: Principal, id: String) -> Result<SmsDetail> {
        self.call(move |db| {
            owner(&db.connection, &principal)?;
            if Uuid::parse_str(&id).is_err() { return Err(Error::InvalidInput); }
            db.connection.query_row(
                "SELECT id,line_epoch,length(data)>0,duplicate_candidate,sender,timestamp,body,data FROM inbox i JOIN inbox_tombstones t ON t.sms_id=i.id WHERE i.id=?",
                [id],
                |r| Ok(SmsDetail { summary: super::inbox::summary(r)?, sender: r.get(4)?, timestamp: r.get(5)?, text: r.get(6)?, data: r.get(7)? }),
            ).optional()?.ok_or(Error::NotFound)
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, Principal, String) {
        let directory = super::super::tests::directory();
        let store = Store::open(directory.path()).unwrap();
        let h = store.handle();
        let owner = h.bootstrap_owner([91; 32]).unwrap();
        let line = h
            .observe_line(LineObservation {
                backend: "fixture".into(),
                fingerprint: Some([1; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        let sms = h.receive_sms(message(line)).unwrap();
        (directory, store, owner, sms.id)
    }
    fn message(line: LineEpoch) -> IncomingSms {
        IncomingSms {
            line,
            source: "synthetic-source".into(),
            sender: "synthetic".into(),
            timestamp: "synthetic".into(),
            text: "测试🙂".into(),
            data: vec![],
        }
    }
    #[test]
    fn delete_restore_are_idempotent_persistent_and_observer_cannot_undo_them() {
        let (directory, store, owner, id) = fixture();
        let h = store.handle();
        let line = h.line_epoch().unwrap();
        let before = h.inbox_events(None, 32).unwrap().events.len();
        for _ in 0..2 {
            assert!(
                h.set_sms_deleted(owner.clone(), id.clone(), true)
                    .unwrap()
                    .deleted
            );
        }
        assert!(h.inbox(None, 32).unwrap().messages.is_empty());
        assert!(matches!(h.sms_detail(id.clone()), Err(Error::NotFound)));
        assert!(h.indicator_snapshot().unwrap().sms.is_empty());
        assert_eq!(
            h.deleted_sms_detail(owner.clone(), id.clone())
                .unwrap()
                .text,
            "测试🙂"
        );
        assert_eq!(
            h.deleted_inbox(owner.clone(), None, 32).unwrap().messages[0].id,
            id
        );
        assert_eq!(h.receive_sms(message(line)).unwrap().id, id);
        assert!(h.inbox(None, 32).unwrap().messages.is_empty());
        assert_eq!(h.inbox_events(None, 32).unwrap().events.len(), before + 1);
        drop(store);
        let store = Store::open(directory.path()).unwrap();
        let h = store.handle();
        assert!(h.inbox(None, 32).unwrap().messages.is_empty());
        for _ in 0..2 {
            assert!(
                !h.set_sms_deleted(owner.clone(), id.clone(), false)
                    .unwrap()
                    .deleted
            );
        }
        assert_eq!(h.inbox(None, 32).unwrap().messages[0].id, id);
        let kinds: Vec<_> = h
            .inbox_events(None, 32)
            .unwrap()
            .events
            .into_iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(kinds.iter().filter(|k| *k == "sms_deleted").count(), 1);
        assert_eq!(kinds.iter().filter(|k| *k == "sms_restored").count(), 1);
        assert!(h
            .deleted_inbox(owner, None, 32)
            .unwrap()
            .messages
            .is_empty());
    }
    #[test]
    fn client_even_with_all_scopes_and_stale_owner_cannot_modify_or_read_trash() {
        let (_directory, store, owner, id) = fixture();
        let h = store.handle();
        let client = h
            .approve_client(
                owner.clone(),
                [92; 32],
                rustsimhub_policy::Kind::Client,
                rustsimhub_policy::ALL_SCOPES,
            )
            .unwrap();
        assert!(matches!(
            h.set_sms_deleted(client.clone(), id.clone(), true),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            h.deleted_inbox(client.clone(), None, 32),
            Err(Error::Forbidden)
        ));
        assert!(matches!(
            h.deleted_sms_detail(client, id.clone()),
            Err(Error::Forbidden)
        ));
        h.call(|db| {
            db.connection.execute(
                "UPDATE principals SET generation=generation+1 WHERE kind='owner'",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            h.set_sms_deleted(owner, id.clone(), true),
            Err(Error::Forbidden)
        ));
        assert_eq!(h.inbox(None, 32).unwrap().messages[0].id, id);
    }
    #[test]
    fn failed_event_write_rolls_back_both_delete_and_restore() {
        let (_directory, store, owner, id) = fixture();
        let h = store.handle();
        let before = h.inbox_events(None, 32).unwrap().events.len();
        h.call(|db| { db.connection.execute_batch("CREATE TEMP TRIGGER fail_visibility BEFORE INSERT ON main.inbox_events WHEN NEW.kind IN ('sms_deleted','sms_restored') BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?; Ok(()) }).unwrap();
        assert!(h.set_sms_deleted(owner.clone(), id.clone(), true).is_err());
        assert!(h.sms_detail(id.clone()).is_ok());
        assert!(h
            .deleted_inbox(owner.clone(), None, 32)
            .unwrap()
            .messages
            .is_empty());
        assert_eq!(h.inbox_events(None, 32).unwrap().events.len(), before);
        h.call(|db| {
            db.connection
                .execute_batch("DROP TRIGGER fail_visibility")?;
            Ok(())
        })
        .unwrap();
        h.set_sms_deleted(owner.clone(), id.clone(), true).unwrap();
        h.call(|db| { db.connection.execute_batch("CREATE TEMP TRIGGER fail_visibility BEFORE INSERT ON main.inbox_events BEGIN SELECT RAISE(ABORT,'synthetic'); END;")?; Ok(()) }).unwrap();
        assert!(h.set_sms_deleted(owner.clone(), id.clone(), false).is_err());
        assert!(matches!(h.sms_detail(id.clone()), Err(Error::NotFound)));
        assert_eq!(h.deleted_inbox(owner, None, 32).unwrap().messages[0].id, id);
    }
    #[test]
    fn v5_migration_preserves_records_epoch_cursor_and_autoincrement_high_water() {
        let (directory, store, owner, id) = fixture();
        let old_cursor = store.handle().inbox_events(None, 32).unwrap().cursor;
        drop(store);
        let connection = Connection::open(directory.path().join("store.sqlite3")).unwrap();
        connection
            .execute_batch(
                "DROP TABLE inbox_tombstones; ALTER TABLE inbox_events RENAME TO inbox_events_new;",
            )
            .unwrap();
        let old_events = include_str!("inbox.sql")
            .split_once("CREATE TABLE inbox_events")
            .unwrap()
            .1
            .split_once("PRAGMA user_version")
            .unwrap()
            .0;
        connection
            .execute_batch(&format!("CREATE TABLE inbox_events{old_events}"))
            .unwrap();
        connection.execute_batch("INSERT INTO inbox_events SELECT * FROM inbox_events_new; DROP TABLE inbox_events_new; UPDATE sqlite_sequence SET seq=100 WHERE name='inbox_events'; PRAGMA user_version=5;").unwrap();
        assert!(connection
            .execute(
                "INSERT INTO inbox_events(sms_id,line_epoch,kind) VALUES(?,1,'sms_deleted')",
                [&id]
            )
            .is_err());
        drop(connection);
        let store = Store::open(directory.path()).unwrap();
        let h = store.handle();
        assert_eq!(
            h.inbox_events(None, 32).unwrap().cursor.db_epoch,
            old_cursor.db_epoch
        );
        assert!(h
            .inbox_events(Some(old_cursor.clone()), 32)
            .unwrap()
            .events
            .is_empty());
        assert_eq!(h.sms_detail(id.clone()).unwrap().text, "测试🙂");
        h.set_sms_deleted(owner, id, true).unwrap();
        let events = h.inbox_events(Some(old_cursor), 32).unwrap().events;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].seq, "101");
    }
}
