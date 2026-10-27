// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_forwarder::{
    Binding, Claim, Destination, Message, Policy, Queue, Rule, Secret, Share, State, Target,
};

fn uuid(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
struct Outbox {
    f: Fixture,
    store: Store,
    owner: ClientKey,
    pin: [u8; 32],
    binding: Binding,
    directory: PathBuf,
    context: SendContext,
    receipts: Vec<String>,
    lost: Claim,
}
impl Outbox {
    fn new() -> Self {
        let f = Fixture::new();
        let (store, owner, principal, pin) = f.open();
        let b = observe(&store, [4; 32]);
        let binding = Binding {
            device_key: pin,
            realm: store.handle().cursor().unwrap().db_epoch,
            owner_key: principal.public_key,
            owner_generation: principal.generation,
        };
        let directory = f.dir("outbox");
        let mut q = Queue::open(&directory, binding.clone()).unwrap();
        q.configure(
            &binding,
            0,
            &uuid(100),
            Policy {
                enabled: true,
                targets: vec![Target {
                    id: uuid(101),
                    destination: Destination::Webhook {
                        url: Secret::new("https://example.invalid/synthetic-no-transport".into()),
                        bearer: None,
                    },
                }],
                rules: vec![Rule {
                    id: uuid(102),
                    targets: vec![uuid(101)],
                    sender_equals: None,
                    body_contains: None,
                    line: None,
                    share: Share::Notice,
                    conditions: None,
                }],
            },
            store.handle().inbox_event_head().unwrap(),
        )
        .unwrap();
        for i in 1..=4 {
            let id = incoming(&store, b.line, i);
            let detail = store.handle().sms_detail(id).unwrap();
            let m = Message {
                id: detail.summary.id,
                line: detail.summary.line,
                sender: detail.sender,
                timestamp: detail.timestamp,
                body: detail.text,
                duplicate_candidate: detail.summary.duplicate_candidate.is_some(),
                binary: detail.summary.binary,
            };
            assert_eq!(
                q.ingest(
                    &binding,
                    1,
                    store.handle().inbox_event_head().unwrap(),
                    Some(&m)
                )
                .unwrap(),
                1
            );
        }
        let confirmed = q.claim(&binding, 1, b.line).unwrap().unwrap();
        q.finish(&binding, &confirmed, State::Confirmed).unwrap();
        let unknown = q.claim(&binding, 1, b.line).unwrap().unwrap();
        q.finish(&binding, &unknown, State::Unknown).unwrap();
        let lost = q.claim(&binding, 1, b.line).unwrap().unwrap();
        let receipts = q
            .receipts(0, 32)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        drop(q);
        for suffix in ["-wal", "-shm", "-journal"] {
            assert!(!directory.join(format!("outbox.sqlite3{suffix}")).exists());
        }
        Self {
            f,
            store,
            owner,
            pin,
            binding,
            directory,
            context: b,
            receipts,
            lost,
        }
    }
    fn closed_copy(&self, name: &str) -> PathBuf {
        let target = self.f.dir(name);
        fs::copy(
            self.directory.join("outbox.sqlite3"),
            target.join("outbox.sqlite3"),
        )
        .unwrap();
        fs::set_permissions(
            target.join("outbox.sqlite3"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fs::copy(
            self.directory.join(".writer.lock"),
            target.join(".writer.lock"),
        )
        .unwrap();
        fs::set_permissions(
            target.join(".writer.lock"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        target
    }
}

#[test]
fn closed_outbox_reopen_preserves_confirmed_unknown_and_attempt_ids() {
    let a = Outbox::new();
    let saved = a.closed_copy("closed-copy");
    let original = logical(&a.directory.join("outbox.sqlite3"));
    let db = readonly(&saved.join("outbox.sqlite3"));
    let attempt: String = db
        .query_row(
            "SELECT attempt_id FROM jobs WHERE id=?",
            [&a.lost.id],
            |r| r.get(0),
        )
        .unwrap();
    drop(db);
    let auth = Auth::open(&a.f.identity, a.store.handle()).unwrap();
    let mut q = Queue::open_current(&saved, &auth).unwrap();
    let receipts = q.receipts(0, 32).unwrap();
    assert_eq!(
        receipts.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
        a.receipts
    );
    assert_eq!(
        receipts.iter().map(|r| r.state).collect::<Vec<_>>(),
        [
            State::Confirmed,
            State::Unknown,
            State::Unknown,
            State::Queued
        ]
    );
    let db = readonly(&saved.join("outbox.sqlite3"));
    assert_eq!(
        db.query_row(
            "SELECT attempt_id FROM jobs WHERE id=?",
            [&a.lost.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        attempt
    );
    drop(db);
    assert!(q.finish(&a.binding, &a.lost, State::Confirmed).is_err());
    // Explicit test-only claim of the *original queued* fourth job. No transport
    // is constructed; neither of the Unknown jobs can be selected again.
    let only_queued = q.claim(&a.binding, 1, a.context.line).unwrap().unwrap();
    assert_eq!(only_queued.id, a.receipts[3]);
    q.finish(&a.binding, &only_queued, State::Unknown).unwrap();
    assert!(q.claim(&a.binding, 1, a.context.line).unwrap().is_none());
    drop(q);
    let mut q = Queue::open_current(&saved, &auth).unwrap();
    assert!(q.claim(&a.binding, 1, a.context.line).unwrap().is_none());
    assert_eq!(q.receipts(0, 32).unwrap().len(), 4);
    assert!(logical(&a.directory.join("outbox.sqlite3")) == original);
}

#[test]
fn restored_realm_rejects_both_matching_and_mixed_old_outbox_without_adoption() {
    let a = Outbox::new();
    let first = a.f.backup(&a.store, "batch-a");
    incoming(&a.store, a.context.line, 5);
    let second = a.f.backup(&a.store, "batch-b");
    let source_outbox = logical(&a.directory.join("outbox.sqlite3"));
    for (i, source) in [&first, &second].into_iter().enumerate() {
        let target = a.f.dir(&format!("restored-{i}"));
        let restored = Store::restore(source, &target).unwrap();
        recover_owner(&restored, &a.f.identity, &a.owner, a.pin);
        let auth = Auth::open(&a.f.identity, restored.handle()).unwrap();
        assert_ne!(auth.durable_realm().unwrap(), a.binding.realm);
        let outbox = a.closed_copy(&format!("old-outbox-{i}"));
        let bytes = hash(&outbox.join("outbox.sqlite3"));
        assert!(rustsimhub_forwarder::inspect_outbox(&outbox).is_ok());
        assert!(Queue::open_current(&outbox, &auth).is_err());
        assert!(hash(&outbox.join("outbox.sqlite3")) == bytes);
        assert_eq!(
            restored.handle().inbox(None, 16).unwrap().messages.len(),
            if i == 0 { 4 } else { 5 }
        );
        let db = readonly(&outbox.join("outbox.sqlite3"));
        assert_eq!(
            db.query_row(
                "SELECT COUNT(*) FROM jobs WHERE state='in_flight'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    assert!(logical(&a.directory.join("outbox.sqlite3")) == source_outbox);
}

#[test]
fn same_device_mixed_batches_have_no_product_wide_consistency_proof() {
    let a = Outbox::new();
    let batch_a = a.f.backup(&a.store, "batch-a");
    let old_outbox = a.closed_copy("outbox-a");
    incoming(&a.store, a.context.line, 5);
    let batch_b = a.f.backup(&a.store, "batch-b");
    let mixed = a.f.dir("mixed-input");
    fs::copy(batch_b.join("backup.sqlite3"), mixed.join("backup.sqlite3")).unwrap();
    fs::copy(
        old_outbox.join("outbox.sqlite3"),
        mixed.join("outbox.sqlite3"),
    )
    .unwrap();
    write(&mixed.join("synthetic-config.json"), b"{\"batch\":\"A\"}");
    let before = manifest(&mixed);
    let target = a.f.dir("restored-mix");
    // This is evidence of the CURRENT gap, not a test-only bundle validator:
    // Store::restore consumes only backup.sqlite3 and ignores the other domains.
    let restored = Store::restore(&mixed, &target).unwrap();
    assert_eq!(restored.handle().inbox(None, 16).unwrap().messages.len(), 5);
    assert_eq!(
        readonly(&batch_a.join("backup.sqlite3"))
            .query_row("SELECT COUNT(*) FROM inbox", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        Auth::open(&a.f.identity, restored.handle())
            .unwrap()
            .public_key(),
        a.pin
    );
    assert!(!target.join("outbox.sqlite3").exists());
    assert!(!target.join("synthetic-config.json").exists());
    assert!(rustsimhub_forwarder::inspect_outbox(&old_outbox).is_ok());
    assert!(manifest(&mixed) == before);
    assert!(!mixed.join("bundle-manifest.json").exists());
}
