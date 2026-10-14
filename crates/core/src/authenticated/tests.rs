// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};

#[tokio::test]
async fn graceful_shutdown_closes_store_and_restarts_on_same_socket() {
    let d = directory();
    let i = directory();
    let r = directory();
    let socket = r.path().join("core.sock");
    for _ in 0..2 {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let server = serve_until(
            socket.to_str().unwrap(),
            rustix::process::geteuid().as_raw(),
            d.path().to_str().unwrap(),
            i.path().to_str().unwrap(),
            async {
                let _ = rx.await;
            },
        );
        let client = async {
            timeout(Duration::from_secs(5), async {
                while !socket.exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            let mut s = UnixStream::connect(&socket).await.unwrap();
            let req = rustsimhub_wire::AuthRequest {
                protocol: 2,
                id: "shutdown".into(),
                command: AuthCommand::Identity {},
            };
            write_frame(&mut s, &serde_json::to_vec(&req).unwrap())
                .await
                .unwrap();
            let response: Response =
                serde_json::from_slice(&read_frame(&mut s).await.unwrap()).unwrap();
            assert!(response.result["public_key"].is_array());
            tx.send(()).unwrap();
        };
        let (result, _) = tokio::join!(server, client);
        result.unwrap();
        assert!(!socket.exists());
        let store = Store::open(d.path()).unwrap();
        assert!(store.handle().identity_public_key().unwrap().is_some());
        drop(store);
    }
}
fn directory() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}
async fn round_trip(command: AuthCommand, auth: Arc<Mutex<Auth>>) -> Value {
    round_trip_setup(command, auth, None).await
}
async fn round_trip_setup(
    command: AuthCommand,
    auth: Arc<Mutex<Auth>>,
    setup: Option<Arc<Mutex<rustsimhub_auth::LocalSetup>>>,
) -> Value {
    let (mut sender, mut receiver) = UnixStream::pair().unwrap();
    let task = tokio::spawn(async move {
        handle(
            &mut receiver,
            auth,
            None,
            None,
            None,
            Providers {
                setup,
                ..Providers::default()
            },
        )
        .await
        .unwrap();
    });
    let req = rustsimhub_wire::AuthRequest {
        protocol: 2,
        id: "test".into(),
        command,
    };
    write_frame(&mut sender, &serde_json::to_vec(&req).unwrap())
        .await
        .unwrap();
    let response: Response =
        serde_json::from_slice(&read_frame(&mut sender).await.unwrap()).unwrap();
    task.await.unwrap();
    response.result
}
#[tokio::test]
async fn setup_ipc_is_opt_in_private_code_bound_signed_and_cannot_replace_owner() {
    let d = directory();
    let i = directory();
    let config = directory();
    let store = Store::open(d.path()).unwrap();
    let auth = Arc::new(Mutex::new(Auth::open(i.path(), store.handle()).unwrap()));
    let key = SigningKey::from_bytes(&[72; 32]);
    let public = key.verifying_key().to_bytes();
    let code = "0b".repeat(32);
    let request = || AuthCommand::SetupChallenge {
        setup_code: code.clone(),
        public_key: public,
    };
    assert!(round_trip(request(), auth.clone())
        .await
        .get("error")
        .is_some());
    assert_eq!(
        round_trip(AuthCommand::Identity {}, auth.clone()).await["owner_setup_available"],
        false
    );
    let path = config.path().join("setup.code");
    rustsimhub_auth::write_private_new(&path, &[11; 32]).unwrap();
    let setup = Some(Arc::new(Mutex::new(
        rustsimhub_auth::LocalSetup::load(&path).unwrap(),
    )));
    let bad = round_trip_setup(
        AuthCommand::SetupChallenge {
            setup_code: "00".repeat(32),
            public_key: public,
        },
        auth.clone(),
        setup.clone(),
    )
    .await;
    assert!(bad.get("error").is_some());
    let c: rustsimhub_auth::Challenge =
        serde_json::from_value(round_trip_setup(request(), auth.clone(), setup.clone()).await)
            .unwrap();
    let signature = key.sign(&c.transcript).to_bytes().to_vec();
    let finish = || AuthCommand::SetupFinish {
        setup_code: code.clone(),
        challenge_id: c.id.clone(),
        signature: signature.clone(),
    };
    assert_eq!(
        round_trip_setup(finish(), auth.clone(), setup.clone()).await["owner_initialized"],
        true
    );
    assert!(round_trip_setup(finish(), auth.clone(), setup.clone())
        .await
        .get("error")
        .is_some());
    assert!(round_trip_setup(request(), auth.clone(), setup.clone())
        .await
        .get("error")
        .is_some());
    assert_eq!(
        round_trip_setup(AuthCommand::Identity {}, auth.clone(), setup).await
            ["owner_setup_available"],
        false
    );
    let c: rustsimhub_auth::Challenge = serde_json::from_value(
        round_trip(AuthCommand::Challenge { public_key: public }, auth.clone()).await,
    )
    .unwrap();
    let result = round_trip(
        AuthCommand::Authenticate {
            challenge_id: c.id,
            signature: key.sign(&c.transcript).to_bytes().to_vec(),
        },
        auth,
    )
    .await;
    assert!(result["token"].as_str().is_some_and(|v| v.len() == 64));
}
#[tokio::test]
async fn ipc_requires_proof_and_checks_revocation_even_from_same_uid() {
    let d = directory();
    let i = directory();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::new(Identity::open(i.path()).unwrap(), s.handle()).unwrap();
    let k = SigningKey::from_bytes(&[41; 32]);
    let key = k.verifying_key().to_bytes();
    let c = a.bootstrap_challenge(key).unwrap();
    a.bootstrap_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap();
    let auth = Arc::new(Mutex::new(a));
    assert_eq!(
        round_trip(
            AuthCommand::Health {
                token: "0".repeat(64)
            },
            auth.clone()
        )
        .await["error"],
        "denied"
    );
    let c = round_trip(AuthCommand::Challenge { public_key: key }, auth.clone()).await;
    let transcript: Vec<u8> = serde_json::from_value(c["transcript"].clone()).unwrap();
    let id = c["id"].as_str().unwrap().to_owned();
    let proof = k.sign(&transcript).to_bytes().to_vec();
    let result = round_trip(
        AuthCommand::Authenticate {
            challenge_id: id.clone(),
            signature: proof.clone(),
        },
        auth.clone(),
    )
    .await;
    let token = result["token"].as_str().unwrap().to_owned();
    assert_eq!(
        round_trip(
            AuthCommand::Health {
                token: token.clone()
            },
            auth.clone()
        )
        .await["ok"],
        true
    );
    // Full pairing and revoke flow over framed IPC; UID alone is never business authority.
    use rustsimhub_policy::Kind;
    round_trip(
        AuthCommand::OpenPairing {
            token: token.clone(),
        },
        auth.clone(),
    )
    .await;
    let client = SigningKey::from_bytes(&[42; 32]);
    let client_key = client.verifying_key().to_bytes();
    let scopes = vec![Scope::StatusRead];
    let c = round_trip(
        AuthCommand::PairChallenge {
            public_key: client_key,
            kind: Kind::Nas,
            scopes: scopes.clone(),
        },
        auth.clone(),
    )
    .await;
    let transcript: Vec<u8> = serde_json::from_value(c["transcript"].clone()).unwrap();
    let client_proof = client.sign(&transcript).to_bytes().to_vec();
    let pending = round_trip(
        AuthCommand::PairProof {
            challenge_id: c["id"].as_str().unwrap().into(),
            signature: client_proof,
        },
        auth.clone(),
    )
    .await;
    let approved = round_trip(
        AuthCommand::Approve {
            token: token.clone(),
            pairing_id: pending["pairing_id"].as_str().unwrap().into(),
            public_key: client_key,
            kind: Kind::Nas,
            scopes,
        },
        auth.clone(),
    )
    .await;
    assert_eq!(approved["kind"], "nas");
    let c = round_trip(
        AuthCommand::Challenge {
            public_key: client_key,
        },
        auth.clone(),
    )
    .await;
    let transcript: Vec<u8> = serde_json::from_value(c["transcript"].clone()).unwrap();
    let client_session = round_trip(
        AuthCommand::Authenticate {
            challenge_id: c["id"].as_str().unwrap().into(),
            signature: client.sign(&transcript).to_bytes().to_vec(),
        },
        auth.clone(),
    )
    .await;
    let client_token = client_session["token"].as_str().unwrap().to_owned();
    assert_eq!(
        round_trip(
            AuthCommand::Health {
                token: client_token.clone()
            },
            auth.clone()
        )
        .await["ok"],
        true
    );
    assert_eq!(
        round_trip(
            AuthCommand::OpenPairing {
                token: client_token.clone()
            },
            auth.clone()
        )
        .await["error"],
        "denied"
    );
    assert_eq!(
        round_trip(
            AuthCommand::Revoke {
                token: token.clone(),
                public_key: client_key
            },
            auth.clone()
        )
        .await["ok"],
        true
    );
    assert_eq!(
        round_trip(
            AuthCommand::Health {
                token: client_token
            },
            auth.clone()
        )
        .await["error"],
        "denied"
    );
    assert_eq!(
        round_trip(
            AuthCommand::Authenticate {
                challenge_id: id,
                signature: proof
            },
            auth.clone()
        )
        .await["error"],
        "denied"
    );
    // A new daemon instance cannot use the same token, even with identical UID and DB.
    let new = Arc::new(Mutex::new(
        Auth::new(Identity::open(i.path()).unwrap(), s.handle()).unwrap(),
    ));
    assert_eq!(
        round_trip(AuthCommand::Health { token }, new).await["error"],
        "denied"
    );
}
#[tokio::test]
async fn authenticated_endpoint_rejects_legacy_protocol_without_upgrade() {
    let d = directory();
    let i = directory();
    let s = Store::open(d.path()).unwrap();
    let a = Arc::new(Mutex::new(
        Auth::new(Identity::open(i.path()).unwrap(), s.handle()).unwrap(),
    ));
    let (mut tx, mut rx) = UnixStream::pair().unwrap();
    write_frame(&mut tx, br#"{"protocol":1,"id":"a","command":"health"}"#)
        .await
        .unwrap();
    assert!(handle(&mut rx, a, None, None, None, Providers::default())
        .await
        .is_err());
}
