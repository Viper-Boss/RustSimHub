// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::forwarding::tests::{Fixture, CHANGE, REQUEST};
use rustsimhub_forwarder::{Failure, Fence};
use std::os::unix::fs::PermissionsExt;
use tokio::net::UnixListener;

fn token(value: &Value) -> Secret {
    Secret::new(value["lease"].as_str().unwrap().into())
}
async fn root(f: &Fixture, command: wire::Command) -> Value {
    f.control
        .private_request(f.auth.clone(), Route::Root, command)
        .await
        .unwrap()
}
async fn worker(f: &Fixture, command: wire::Command) -> Result<Value> {
    f.control
        .private_request(f.auth.clone(), Route::Worker, command)
        .await
}

#[tokio::test]
async fn root_grant_is_disabled_by_default_and_does_not_create_an_outbox() {
    let f = Fixture::new();
    let v = root(&f, wire::Command::State {}).await;
    assert_eq!(v["enabled"], false);
    assert_eq!(v["process_state_verified"], false);
    assert!(f
        .control
        .private_request(f.auth.clone(), Route::Root, wire::Command::Grant {})
        .await
        .is_err());
    assert!(!f.state.path().join("forwarding").exists());
    for route in [Route::Worker, Route::Browser] {
        assert!(f
            .control
            .private_request(f.auth.clone(), route, wire::Command::Grant {})
            .await
            .is_err());
        assert!(f
            .control
            .private_request(f.auth.clone(), route, wire::Command::State {})
            .await
            .is_err());
    }
}

#[tokio::test]
async fn real_auth_queue_grant_claim_staged_fence_and_receipt_are_single_use() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    f.incoming("worker new message");
    let lease = root(&f, wire::Command::Grant {}).await;
    assert!(f
        .control
        .private_request(f.auth.clone(), Route::Root, wire::Command::Grant {})
        .await
        .is_err());
    let claim: Claim = serde_json::from_value(
        worker(
            &f,
            wire::Command::Acquire {
                lease: token(&lease),
            },
        )
        .await
        .unwrap()["claim"]
            .take(),
    )
    .unwrap();
    assert!(worker(
        &f,
        wire::Command::Acquire {
            lease: token(&lease)
        }
    )
    .await
    .is_err());
    assert!(worker(
        &f,
        wire::Command::Finish {
            lease: token(&lease),
            attempt_id: claim.attempt_id.clone(),
            outcome: DeliveryOutcome::Confirmed { status: 200 }
        }
    )
    .await
    .is_err());
    for stage in [Stage::BeforeConnect, Stage::BeforeTls, Stage::BeforeSubmit] {
        assert_eq!(
            worker(
                &f,
                wire::Command::Check {
                    lease: token(&lease),
                    attempt_id: claim.attempt_id.clone(),
                    claim_sha256: wire::claim_sha256(&claim).unwrap(),
                    stage
                }
            )
            .await
            .unwrap()["allowed"],
            true
        );
    }
    assert_eq!(
        worker(
            &f,
            wire::Command::Finish {
                lease: token(&lease),
                attempt_id: claim.attempt_id.clone(),
                outcome: DeliveryOutcome::Confirmed { status: 200 }
            }
        )
        .await
        .unwrap()["state"],
        "confirmed"
    );
    assert!(worker(
        &f,
        wire::Command::Finish {
            lease: token(&lease),
            attempt_id: claim.attempt_id.clone(),
            outcome: DeliveryOutcome::Confirmed { status: 200 }
        }
    )
    .await
    .is_err());
    assert!(worker(
        &f,
        wire::Command::Acquire {
            lease: token(&lease)
        }
    )
    .await
    .unwrap()["claim"]
        .is_null());
}

#[tokio::test]
async fn owner_disable_revoke_expiry_and_new_configuration_never_redeliver_lost_claim() {
    for mode in ["disable", "revoke", "expire", "change"] {
        let f = Fixture::new();
        f.request(f.configure(0, REQUEST, true)).await;
        f.incoming(mode);
        let lease = root(&f, wire::Command::Grant {}).await;
        worker(
            &f,
            wire::Command::Acquire {
                lease: token(&lease),
            },
        )
        .await
        .unwrap();
        match mode {
            "disable" => {
                f.request(f.configure(1, CHANGE, false)).await;
            }
            "change" => {
                f.request(f.configure(1, CHANGE, true)).await;
            }
            "revoke" => {
                root(&f, wire::Command::Revoke {}).await;
            }
            _ => {
                f.control
                    .0
                    .lock()
                    .unwrap()
                    .worker
                    .as_mut()
                    .unwrap()
                    .deadline = Instant::now() - Duration::from_secs(1);
            }
        }
        assert!(worker(
            &f,
            wire::Command::Acquire {
                lease: token(&lease)
            }
        )
        .await
        .is_err());
        assert_eq!(
            f.control
                .0
                .lock()
                .unwrap()
                .queue
                .as_ref()
                .unwrap()
                .status()
                .unwrap()
                .unknown,
            1
        );
        if mode != "disable" {
            let new = root(&f, wire::Command::Grant {}).await;
            assert!(worker(&f, wire::Command::Acquire { lease: token(&new) })
                .await
                .unwrap()["claim"]
                .is_null());
        }
    }
}

#[tokio::test]
async fn exact_owner_save_retry_preserves_live_lease_and_first_event_start() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    let lease = root(&f, wire::Command::Grant {}).await;
    f.incoming("arrived after root grant");
    assert_eq!(
        f.request(f.configure(0, REQUEST, true)).await["generation"],
        1
    );
    let v = worker(
        &f,
        wire::Command::Acquire {
            lease: token(&lease),
        },
    )
    .await
    .unwrap();
    assert!(v["claim"].is_object());
}

#[tokio::test]
async fn wrong_stage_sim_change_and_real_owner_revocation_retire_claim_without_success() {
    for mode in ["stage", "sim", "owner"] {
        let f = Fixture::new();
        f.request(f.configure(0, REQUEST, true)).await;
        f.incoming(mode);
        let lease = root(&f, wire::Command::Grant {}).await;
        let v = worker(
            &f,
            wire::Command::Acquire {
                lease: token(&lease),
            },
        )
        .await
        .unwrap();
        let claim: Claim = serde_json::from_value(v["claim"].clone()).unwrap();
        match mode {
            "sim" => {
                f.store
                    .handle()
                    .observe_line(rustsimhub_storage::LineObservation {
                        backend: "synthetic modem".into(),
                        fingerprint: Some([20; 32]),
                        status: rustsimhub_storage::LineStatus::Ready,
                    })
                    .unwrap();
            }
            "owner" => {
                let db = rusqlite::Connection::open(f.state.path().join("store.sqlite3")).unwrap();
                db.execute(
                    "UPDATE principals SET active=0,generation=generation+1 WHERE kind='owner'",
                    [],
                )
                .unwrap();
            }
            _ => {}
        }
        assert!(worker(
            &f,
            wire::Command::Check {
                lease: token(&lease),
                claim_sha256: wire::claim_sha256(&claim).unwrap(),
                attempt_id: claim.attempt_id,
                stage: if mode == "stage" {
                    Stage::BeforeSubmit
                } else {
                    Stage::BeforeConnect
                }
            }
        )
        .await
        .is_err());
        assert_eq!(
            f.control
                .0
                .lock()
                .unwrap()
                .queue
                .as_ref()
                .unwrap()
                .status()
                .unwrap()
                .unknown,
            1
        );
    }
}

#[tokio::test]
async fn lost_submit_permission_ack_is_unknown_even_if_worker_says_not_submitted() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    f.incoming("ambiguous permission ack");
    let lease = root(&f, wire::Command::Grant {}).await;
    let claim: Claim = serde_json::from_value(
        worker(
            &f,
            wire::Command::Acquire {
                lease: token(&lease),
            },
        )
        .await
        .unwrap()["claim"]
            .take(),
    )
    .unwrap();
    for stage in [Stage::BeforeConnect, Stage::BeforeTls, Stage::BeforeSubmit] {
        worker(
            &f,
            wire::Command::Check {
                lease: token(&lease),
                attempt_id: claim.attempt_id.clone(),
                claim_sha256: wire::claim_sha256(&claim).unwrap(),
                stage,
            },
        )
        .await
        .unwrap();
    }
    let v = worker(
        &f,
        wire::Command::Finish {
            lease: token(&lease),
            attempt_id: claim.attempt_id,
            outcome: DeliveryOutcome::NotSubmitted {
                reason: Failure::Denied,
            },
        },
    )
    .await
    .unwrap();
    assert_eq!(v["state"], "unknown");
}

#[tokio::test]
async fn restart_discards_ephemeral_grant_reopens_only_current_owner_and_preserves_unknown() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    f.incoming("restart inflight");
    let old = root(&f, wire::Command::Grant {}).await;
    worker(&f, wire::Command::Acquire { lease: token(&old) })
        .await
        .unwrap();
    f.control.0.lock().unwrap().worker = None;
    f.control.0.lock().unwrap().queue = None;
    let next = Controller::new(f.state.path(), f.store.handle());
    assert!(next
        .private_request(
            f.auth.clone(),
            Route::Worker,
            wire::Command::Acquire { lease: token(&old) }
        )
        .await
        .is_err());
    let state = next
        .private_request(f.auth.clone(), Route::Root, wire::Command::State {})
        .await
        .unwrap();
    assert_eq!(state["enabled"], true);
    assert_eq!(state["unknown"], 1);
    assert_eq!(state["lease_active"], false);
    let db = rusqlite::Connection::open(f.state.path().join("store.sqlite3")).unwrap();
    db.execute(
        "UPDATE principals SET generation=generation+1 WHERE kind='owner'",
        [],
    )
    .unwrap();
    next.0.lock().unwrap().queue = None;
    assert!(next
        .private_request(f.auth.clone(), Route::Root, wire::Command::Grant {})
        .await
        .is_err());
}

#[tokio::test]
async fn actual_socket_worker_client_uses_peer_uid_and_each_live_fence() {
    let f = Fixture::new();
    f.request(f.configure(0, REQUEST, true)).await;
    f.incoming("socket payload");
    let lease = root(&f, wire::Command::Grant {}).await;
    let dir = tempfile::tempdir().unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("core.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let auth = f.auth.clone();
    let c = f.control.clone();
    let access = Access {
        edge: 65529,
        worker: rustix::process::geteuid().as_raw(),
    };
    let server = tokio::spawn(async move {
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let role = access.route(&stream).unwrap();
            assert!(role == Route::Worker);
            handle(&mut stream, auth.clone(), c.clone(), role)
                .await
                .unwrap();
        }
    });
    assert!(wire::request(
        &path,
        rustix::process::geteuid().as_raw(),
        wire::Command::Grant {}
    )
    .await
    .is_err());
    let mut client = wire::Client::new(
        path.clone(),
        rustix::process::geteuid().as_raw(),
        token(&lease),
    );
    let claim = client.acquire().await.unwrap().unwrap();
    for stage in [Stage::BeforeConnect, Stage::BeforeTls, Stage::BeforeSubmit] {
        client.check(stage, &claim).await.unwrap();
    }
    client
        .finish(
            &claim,
            DeliveryOutcome::Unknown {
                status: None,
                reason: Failure::Cancelled,
            },
        )
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(
        f.control
            .0
            .lock()
            .unwrap()
            .queue
            .as_ref()
            .unwrap()
            .status()
            .unwrap()
            .unknown,
        1
    );
}

#[tokio::test]
async fn actual_client_fence_hashes_the_payload_it_will_send_not_an_acquire_time_hint() {
    for field in ["url", "body", "bearer", "line", "generation"] {
        let f = Fixture::new();
        f.request(f.configure(0, REQUEST, true)).await;
        f.incoming(field);
        let lease = root(&f, wire::Command::Grant {}).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let auth = f.auth.clone();
        let c = f.control.clone();
        let access = Access {
            edge: 65529,
            worker: rustix::process::geteuid().as_raw(),
        };
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let route = access.route(&stream).unwrap();
                handle(&mut stream, auth.clone(), c.clone(), route)
                    .await
                    .unwrap();
            }
        });
        let mut client =
            wire::Client::new(path, rustix::process::geteuid().as_raw(), token(&lease));
        let mut claim = client.acquire().await.unwrap().unwrap();
        match field {
            "url" => claim.request.url = Secret::new("https://changed.invalid/other-target".into()),
            "body" => claim.request.body = Secret::new("{\"modified\":true}".into()),
            "bearer" => claim.request.bearer = Some(Secret::new("modified-bearer".into())),
            "line" => claim.line.0 += 1,
            _ => claim.generation += 1,
        }
        assert!(client.check(Stage::BeforeConnect, &claim).await.is_err());
        server.await.unwrap();
        assert_eq!(
            f.control
                .0
                .lock()
                .unwrap()
                .queue
                .as_ref()
                .unwrap()
                .status()
                .unwrap()
                .unknown,
            1
        );
    }
}

#[test]
fn routing_and_worker_protocol_reject_confused_roles_unbounded_frames_and_fields() {
    let a = Access {
        edge: 65529,
        worker: 65531,
    };
    assert!(a.uid(0) == Some(Route::Root));
    assert!(a.uid(65529) == Some(Route::Browser));
    assert!(a.uid(65531) == Some(Route::Worker));
    assert!(a.uid(65532).is_none());
    assert!(Access::new(65531, 65531).is_err());
    assert!(Access::new(0, 65531).is_err());
    for bytes in [vec![],vec![0;wire::MAX_FRAME+1],br#"{"protocol":4,"id":"11111111-1111-4111-8111-111111111111","command":{"op":"grant","owner":true}}"#.to_vec()] {
        assert!(wire::parse(&bytes).is_err());
    }
}
