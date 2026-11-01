// SPDX-License-Identifier: AGPL-3.0-only
//! Rebuild real Auth/Store/sender around one unchanged synthetic MM instance.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_policy::Principal;
use rustsimhub_wire::AuthCommand;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

fn login(
    h: &Handle,
    identity: &std::path::Path,
    first: bool,
) -> (rustsimhub_auth::Auth, String, Principal) {
    let mut auth = rustsimhub_auth::Auth::new(
        rustsimhub_auth::Identity::open(identity).unwrap(),
        h.clone(),
    )
    .unwrap();
    let owner_key = SigningKey::from_bytes(&[71; 32]);
    let sender_key = SigningKey::from_bytes(&[72; 32]);
    if first {
        let c = auth
            .bootstrap_challenge(owner_key.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &owner_key.sign(&c.transcript).to_bytes())
            .unwrap();
        let owner = h
            .principal(owner_key.verifying_key().to_bytes())
            .unwrap()
            .unwrap();
        h.approve_client(
            owner,
            sender_key.verifying_key().to_bytes(),
            rustsimhub_policy::Kind::Client,
            rustsimhub_policy::mask(&[rustsimhub_policy::Scope::SmsSend]).unwrap(),
        )
        .unwrap();
    }
    let c = auth
        .login_challenge(sender_key.verifying_key().to_bytes())
        .unwrap();
    let token = auth
        .login_finish(&c.id, &sender_key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned();
    let principal = h
        .principal(sender_key.verifying_key().to_bytes())
        .unwrap()
        .unwrap();
    (auth, token, principal)
}

#[tokio::test]
async fn restarted_sender_queries_original_unknown_and_positive_proof_without_repeating_create_or_send(
) {
    let (server, client, sim) = peers().await;
    server
        .object_server()
        .remove::<FakeMessaging, _>("/org/freedesktop/ModemManager1/Modem/0")
        .await
        .unwrap();
    let creates = Arc::new(AtomicUsize::new(0));
    let sends = Arc::new(AtomicUsize::new(0));
    let state = Arc::new(AtomicU32::new(1));
    server
        .object_server()
        .at(
            "/org/freedesktop/ModemManager1/Modem/0",
            FakeOutgoingMessaging {
                creates: creates.clone(),
                mode: 6,
                sim,
                revoke: None,
            },
        )
        .await
        .unwrap();
    server
        .object_server()
        .at(
            path(9000),
            FakeOutgoingSms {
                sends: sends.clone(),
                state: state.clone(),
                mode: 6,
            },
        )
        .await
        .unwrap();
    let d = directory();
    let identity = directory();
    let mut operation_id = None;
    let mut prior_token = None;
    for generation in 0..2 {
        let store = rustsimhub_storage::Store::open(d.path()).unwrap();
        let h = store.handle();
        let (auth, token, principal) = login(&h, identity.path(), generation == 0);
        let line = h
            .observe_line(LineObservation {
                backend: "synthetic-sender".into(),
                fingerprint: Some([7; 32]),
                status: LineStatus::Ready,
            })
            .unwrap()
            .line;
        let (_fence, guard) = tokio::sync::watch::channel(watch::Notice::default());
        let binding = Binding {
            connection: client.clone(),
            context: rustsimhub_storage::SendContext {
                line,
                backend: "synthetic-sender".into(),
                owner: ":1.20".into(),
                modem: "/org/freedesktop/ModemManager1/Modem/0".into(),
                fingerprint: [7; 32],
            },
            iccid: "8901234567890123456".into(),
            sim: "/org/freedesktop/ModemManager1/SIM/0".try_into().unwrap(),
            guard,
            fence: 0,
            seen: Instant::now(),
        };
        let (_publisher, bindings) = tokio::sync::watch::channel(Some(binding));
        let (controller, worker) =
            crate::sender::channel(Arc::new(Mutex::new(auth)), h.clone(), bindings);
        let exercise = async {
            if generation == 0 {
                let prepared = controller
                    .request(AuthCommand::PrepareSms {
                        token: token.clone(),
                        line_epoch: line.0,
                        idempotency_key: "synthetic-restart".into(),
                        recipient: "+12025550123".into(),
                        text: "synthetic outgoing".into(),
                    })
                    .await
                    .unwrap();
                let id = prepared["id"].as_str().unwrap().to_owned();
                let result = controller
                    .request(AuthCommand::SubmitSms {
                        token: token.clone(),
                        operation_id: id.clone(),
                    })
                    .await
                    .unwrap();
                assert_eq!(result["state"], "unknown");
                operation_id = Some(id);
                prior_token = Some(token.clone());
            } else {
                // Merely starting the new worker never pumps old jobs.
                tokio::task::yield_now().await;
                assert_eq!(creates.load(Ordering::SeqCst), 1);
                assert_eq!(sends.load(Ordering::SeqCst), 1);
                let id = operation_id.clone().unwrap();
                assert!(controller
                    .request(AuthCommand::SubmitSms {
                        token: prior_token.clone().unwrap(),
                        operation_id: id.clone()
                    })
                    .await
                    .is_err());
                let result = controller
                    .request(AuthCommand::SubmitSms {
                        token: token.clone(),
                        operation_id: id.clone(),
                    })
                    .await
                    .unwrap();
                assert_eq!(result["state"], "unknown");
                let unconfirmed = controller
                    .request(AuthCommand::SmsOperation {
                        token: token.clone(),
                        operation_id: id.clone(),
                    })
                    .await
                    .unwrap();
                assert_eq!(unconfirmed["state"], "unknown");
                state.store(5, Ordering::SeqCst);
                for _ in 0..3 {
                    let result = controller
                        .request(AuthCommand::SmsOperation {
                            token: token.clone(),
                            operation_id: id.clone(),
                        })
                        .await
                        .unwrap();
                    assert_eq!(result["id"], id);
                    assert_eq!(result["state"], "submitted");
                    let result = controller
                        .request(AuthCommand::SubmitSms {
                            token: token.clone(),
                            operation_id: id.clone(),
                        })
                        .await
                        .unwrap();
                    assert_eq!(result["state"], "submitted");
                }
                let mut cursor = h.cursor().unwrap();
                cursor.seq = "0".into();
                let events = h.events(cursor, 32).unwrap().events;
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| e.operation_id.as_ref() == Some(&id) && e.kind == "submitted")
                        .count(),
                    1
                );
                assert_eq!(
                    h.sender_operation(id, principal.clone()).unwrap().state,
                    rustsimhub_domain::SmsState::Submitted
                );
            }
            assert_eq!(creates.load(Ordering::SeqCst), 1);
            assert_eq!(sends.load(Ordering::SeqCst), 1);
            drop(controller);
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(worker.run(), exercise)
        })
        .await
        .unwrap();
        // End this Store and Auth before constructing the next generation.
        drop(store);
    }
    server.close().await.unwrap();
}
