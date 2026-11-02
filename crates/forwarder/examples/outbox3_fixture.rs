// SPDX-License-Identifier: AGPL-3.0-only
//! Disposable source-format fixture using the ACTUAL default Queue writer.
//! Build only with dev-fixtures and without smtp-production-candidate.
#[cfg(all(feature = "dev-fixtures", not(feature = "smtp-production-candidate")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rustsimhub_domain::{InboxCursor, LineEpoch};
    use rustsimhub_forwarder::*;
    let directory = std::env::args()
        .nth(1)
        .ok_or("fixture directory required")?;
    let id = || uuid::Uuid::new_v4().to_string();
    let b = Binding {
        device_key: [1; 32],
        owner_key: [2; 32],
        owner_generation: 7,
        realm: id(),
    };
    let target = id();
    let rule = id();
    let mut q = Queue::open(std::path::Path::new(&directory), b.clone())?;
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
                    bearer: Some(Secret::new("SYNTHETIC_FIXTURE_ONLY".into())),
                },
            }],
            rules: vec![Rule {
                id: rule,
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
    )?;
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
                sender: "SYNTHETIC_FIXTURE_SENDER".into(),
                timestamp: "SYNTHETIC_FIXTURE_TIME".into(),
                body: "SYNTHETIC_FIXTURE_BODY 123456".into(),
                duplicate_candidate: false,
                binary: false,
            }),
        )?;
    }
    let claim = q.claim(&b, 1, LineEpoch(2))?.ok_or("claim")?;
    q.finish(&b, &claim, State::Unknown)?;
    let _inflight = q.claim(&b, 1, LineEpoch(2))?.ok_or("claim")?;
    assert_eq!(OUTBOX_CURRENT_FORMAT, 3);
    assert_eq!(FORWARDING_WORKER_PROTOCOL, 4);
    Ok(())
}
#[cfg(any(not(feature = "dev-fixtures"), feature = "smtp-production-candidate"))]
fn main() {
    eprintln!("source-format fixture requires dev-fixtures and the default format3 writer");
    std::process::exit(1);
}
