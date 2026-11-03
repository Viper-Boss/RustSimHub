// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(all(test, target_os = "linux"))]
//! Explicit isolated UID harness. Reuse the actual binary modules verbatim;
//! no substitute normal dispatcher, verifier, router or media authority.
use crate::{nkn_business, nkn_host};
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{
    nkn_session::{MembershipKey, NknSession},
    Auth, Token,
};
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_recovery_session::{Action, Context, Purpose, Session};
use rustsimhub_storage::Store;
use std::{
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};
struct Fixture {
    auth: Arc<Mutex<Auth>>,
    owner: Token,
    client: [u8; 32],
    _store: Store,
    _dirs: Vec<tempfile::TempDir>,
}
fn fixture() -> Fixture {
    fixture_mode(false)
}
fn fixture_mode(recovery: bool) -> Fixture {
    let d = tempfile::tempdir().unwrap();
    let i = tempfile::tempdir().unwrap();
    for p in [d.path(), i.path()] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = Store::open(d.path()).unwrap();
    let mut auth = Auth::open(i.path(), store.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[71; 32]);
    let challenge = auth
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&challenge.id, &owner.sign(&challenge.transcript).to_bytes())
        .unwrap();
    let (store, mut auth) = if recovery {
        drop(auth);
        drop(store);
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        for p in [first.path(), second.path()] {
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        drop(Store::migrate_recovery_grants(d.path(), first.path()).unwrap());
        Store::migrate_recovery_execution(d.path(), second.path()).unwrap();
        let store = Store::open_recovery_execution(d.path()).unwrap();
        let auth = Auth::open(i.path(), store.handle()).unwrap();
        (store, auth)
    } else {
        (store, auth)
    };
    let challenge = auth
        .login_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    let owner = auth
        .login_finish(&challenge.id, &owner.sign(&challenge.transcript).to_bytes())
        .unwrap();
    let key = SigningKey::from_bytes(&[72; 32]);
    let client = key.verifying_key().to_bytes();
    auth.open_pairing(owner.expose()).unwrap();
    let challenge = auth
        .pairing_challenge(client, Kind::Client, &[Scope::StatusRead])
        .unwrap();
    let id = auth
        .pairing_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
        .unwrap();
    auth.approve(
        owner.expose(),
        &id,
        client,
        Kind::Client,
        &[Scope::StatusRead],
    )
    .unwrap();
    Fixture {
        auth: Arc::new(Mutex::new(auth)),
        owner,
        client,
        _store: store,
        _dirs: vec![d, i],
    }
}
fn root() -> PathBuf {
    PathBuf::from(
        std::env::var_os("RUSTSIMHUB_UID_FIXTURE_DIR").expect("explicit isolated runner required"),
    )
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()
        .unwrap()
}
fn membership(secret: u8) -> MembershipKey {
    MembershipKey::from_secret([5; 16], zeroize::Zeroizing::new([secret; 32]), 1).unwrap()
}
fn nonce() -> [u8; 32] {
    let mut value = [0; 32];
    getrandom::fill(&mut value).unwrap();
    value
}
#[test]
#[ignore = "explicit network-disabled real-UID fixture; use runner --nkn"]
fn isolated_nkn_core() {
    runtime().block_on(async {
        assert_eq!(rustix::process::geteuid().as_raw(), 1001);
        let f = fixture();
        let root = root();
        let sources = nkn_host::NknSources::new(1003, 1004, 1005, 1006, 1002).unwrap();
        assert!(nkn_host::NknSources::new(1003, 1004, 1005, 1002, 1002).is_err());
        let listener = UnixListener::bind(root.join("core.sock")).unwrap();
        fs::set_permissions(root.join("core.sock"), fs::Permissions::from_mode(0o660)).unwrap();
        fs::write(
            root.join("public.json"),
            serde_json::to_vec(
                &serde_json::json!({"pin":f.auth.lock().unwrap().public_key(),"generation":1}),
            )
            .unwrap(),
        )
        .unwrap();
        fs::set_permissions(root.join("public.json"), fs::Permissions::from_mode(0o640)).unwrap();
        let key = Arc::new(membership(7));
        println!("NKN_READY");
        for uid in [0, 1001, 1003, 1004, 1005, 1006, 1007] {
            let (s, _) = listener.accept().await.unwrap();
            assert_eq!(s.peer_cred().unwrap().uid(), uid);
            assert!(sources.accept(s).is_err());
            println!("UID_CASE denied_{uid}");
        }
        for case in [
            "query",
            "wrong_pin",
            "wrong_membership",
            "old_cipher",
            "old_token",
            "tamper",
            "disconnect",
            "revoked",
        ] {
            let (s, _) = listener.accept().await.unwrap();
            assert_eq!(s.peer_cred().unwrap().uid(), 1002);
            let accepted = sources.accept(s).unwrap();
            let ports = nkn_business::CorePorts::new(f.auth.clone());
            let key = key.clone();
            let worker = tokio::spawn(nkn_host::serve_owned(
                accepted,
                ports,
                key,
                Instant::now() + Duration::from_secs(30),
            ));
            if case == "revoked" {
                tokio::task::spawn_blocking(|| {
                    let mut byte = [0];
                    std::io::stdin().read_exact(&mut byte).unwrap();
                    assert_eq!(byte, [b'R']);
                })
                .await
                .unwrap();
                f.auth
                    .lock()
                    .unwrap()
                    .revoke(f.owner.expose(), f.client)
                    .unwrap();
                println!("NKN_REVOKED");
            }
            let result = worker.await.unwrap();
            if case == "query" {
                if let Err(e) = result {
                    panic!("actual normal Core query failed: {e:?}");
                }
            } else {
                assert!(result.is_err(), "invalid source reached normal Core");
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                while sources.is_busy() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            println!("UID_CASE {case}");
        }
    });
}
async fn send(stream: &mut UnixStream, kind: u8, wire: &[u8]) {
    assert!(wire.len() <= 2048);
    stream.write_u32((wire.len() + 2) as u32).await.unwrap();
    stream.write_all(&[1, kind]).await.unwrap();
    stream.write_all(wire).await.unwrap();
}
async fn receive(stream: &mut UnixStream, kind: u8) -> Vec<u8> {
    let size = stream.read_u32().await.unwrap();
    assert!((3..=2050).contains(&size));
    let mut bytes = vec![0; size as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    assert_eq!(bytes[..2], [1, kind]);
    bytes.split_off(2)
}
async fn eof(stream: &mut UnixStream) {
    let mut byte = [0];
    match tokio::time::timeout(Duration::from_secs(4), stream.read(&mut byte))
        .await
        .unwrap()
    {
        Ok(0) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
            ) => {}
        other => panic!("rejected source returned unexpected bytes: {other:?}"),
    }
}
#[test]
#[ignore = "explicit network-disabled real-UID fixture; use runner --nkn"]
fn isolated_nkn_client() {
    runtime().block_on(async {
        let sdk_combined = std::env::var_os("RSH_COMBINED_SDK_CLIENT").is_some();
        assert_eq!(
            rustix::process::geteuid().as_raw(),
            if sdk_combined { 1007 } else { 1002 }
        );
        let case = std::env::var("RUSTSIMHUB_NKN_CASE").unwrap();
        let mut stream = UnixStream::connect(root().join("core.sock")).await.unwrap();
        if case == "disconnect" {
            return;
        }
        let public: serde_json::Value =
            serde_json::from_slice(&fs::read(root().join("public.json")).unwrap()).unwrap();
        let pin: [u8; 32] = serde_json::from_value(public["pin"].clone()).unwrap();
        let pin = if case == "wrong_pin" {
            SigningKey::from_bytes(&[75; 32]).verifying_key().to_bytes()
        } else {
            pin
        };
        let key = SigningKey::from_bytes(&[72; 32]);
        let context = Context::new(
            Purpose::NknControl,
            pin,
            key.verifying_key().to_bytes(),
            1,
            nonce(),
        )
        .unwrap();
        let origin = Instant::now();
        let now = || origin.elapsed().as_millis() as u64;
        let mut peer = Session::client(context, 0, 30_000).unwrap();
        send(&mut stream, 1, &peer.write_client_hello(now()).unwrap()).await;
        let two = receive(&mut stream, 1).await;
        if case == "wrong_pin" {
            assert!(peer.read_handshake(&two, now()).is_err());
            return;
        }
        peer.read_handshake(&two, now()).unwrap();
        send(&mut stream, 1, &peer.write_handshake(now()).unwrap()).await;
        peer.open_binding(&receive(&mut stream, 1).await, now())
            .unwrap();
        let signature = key.sign(&peer.binding_message(now()).unwrap()).to_bytes();
        send(
            &mut stream,
            1,
            &peer.seal_binding(&signature, now()).unwrap(),
        )
        .await;
        peer.open_binding_ack(&receive(&mut stream, 1).await, now())
            .unwrap();
        let root = SigningKey::from_bytes(&[73; 32]);
        let root_public = root.verifying_key().to_bytes();
        let member = membership(if case == "wrong_membership" { 8 } else { 7 });
        let proof = member.proof(&root_public).unwrap();
        let message = NknSession::membership_message(
            &peer.verified_peer_binding_message(now()).unwrap(),
            member.network_id(),
            &root_public,
            &proof,
        )
        .unwrap();
        let mut body = vec![1];
        body.extend_from_slice(&root_public);
        body.extend_from_slice(&proof);
        body.extend_from_slice(&root.sign(&message).to_bytes());
        send(
            &mut stream,
            1,
            &peer
                .seal_request([1; 16], Action::NormalCore, &body, now())
                .unwrap(),
        )
        .await;
        if case == "wrong_membership" {
            eof(&mut stream).await;
            return;
        }
        let reply = peer
            .open_response(&receive(&mut stream, 1).await, now())
            .unwrap();
        let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
        assert_eq!(reply["auth_protocol"], 2);
        let mut token = reply["token"].as_str().unwrap().to_owned();
        assert_eq!(token.len(), 64);
        let old = PathBuf::from("/tmp/nkn-secure-peer");
        if case == "query" {
            fs::write(old.join("old-token"), &token).unwrap();
        }
        if case == "old_token" {
            token = fs::read_to_string(old.join("old-token")).unwrap();
        }
        if case == "revoked" {
            println!("NKN_BOUND_REVOKE");
            tokio::task::spawn_blocking(|| {
                let mut byte = [0];
                std::io::stdin().read_exact(&mut byte).unwrap();
                assert_eq!(byte, [b'R']);
            })
            .await
            .unwrap();
        }
        let id = "00000000-0000-4000-8000-000000000001";
        let body = serde_json::to_vec(
            &serde_json::json!({"protocol":2,"id":id,"command":{"op":"health","token":token}}),
        )
        .unwrap();
        let mut wire = peer
            .seal_request([2; 16], Action::NormalCore, &body, now())
            .unwrap();
        if case == "query" {
            fs::write(old.join("old-cipher"), &wire).unwrap();
        }
        if case == "old_cipher" {
            wire = fs::read(old.join("old-cipher")).unwrap();
        }
        if case == "tamper" {
            wire[0] ^= 1;
        }
        let attempt = async {
            stream.write_u32((wire.len() + 2) as u32).await?;
            stream.write_all(&[1, 2]).await?;
            stream.write_all(&wire).await
        }
        .await;
        if let Err(error) = attempt {
            assert!(
                case != "query"
                    && matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                    )
            );
        }
        if case == "query" {
            let wire = receive(&mut stream, 2).await;
            let body = peer.open_response(&wire, now()).unwrap();
            let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(response["protocol"], 2);
            assert_eq!(response["id"], id);
            assert_eq!(response["result"]["ok"], true);
            assert_eq!(
                response["result"]["mode"],
                if sdk_combined {
                    "authenticated_saved_wifi_trial"
                } else {
                    "authenticated_read_only"
                }
            );
            send(&mut stream, 3, &[]).await;
            eof(&mut stream).await;
        } else {
            eof(&mut stream).await;
        }
    });
}

#[cfg(feature = "product-providers-candidate")]
#[tokio::test]
#[ignore = "explicit private cgroup/netns SDK product fixture only"]
async fn isolated_product_core() {
    use rustsimhub_wire::{AuthCommand, NknOwner};
    assert_eq!(rustix::process::geteuid().as_raw(), 1001);
    let f = fixture_mode(true);
    let controller = crate::nkn_product::Controller::new(f.auth.clone());
    let instance = f.auth.lock().unwrap().observer_instance();
    let pin = f.auth.lock().unwrap().public_key();
    fs::write(
        "/tmp/owned-core-public-pin",
        pin.iter().map(|n| format!("{n:02x}")).collect::<String>(),
    )
    .unwrap();
    fs::write(
        "/tmp/sdk-counterpart/public.json",
        serde_json::to_vec(&serde_json::json!({"pin":pin})).unwrap(),
    )
    .unwrap();
    let change = |owner, enabled, revision: u64| AuthCommand::SetNknEnabled {
        token: f.owner.expose().to_owned(),
        owner,
        enabled,
        expected_revision: revision.to_string(),
        expected_core_instance: instance.clone(),
        request_id: format!("{revision:08x}-1111-4111-8111-111111111111"),
    };
    let wait = Instant::now() + Duration::from_secs(10);
    while !std::path::Path::new("/run/rustsimhub-nkn-root/root.sock").exists() {
        assert!(Instant::now() < wait);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let ports = nkn_business::CorePorts {
        auth: f.auth.clone(),
        metrics: None,
        sender: None,
        calls: None,
        providers: crate::authenticated::Providers {
            nkn: Some(controller.clone()),
            ..Default::default()
        },
    };
    for (revision, owner) in [
        NknOwner::ManualEngine,
        NknOwner::Statistics,
        NknOwner::Remote,
    ]
    .into_iter()
    .enumerate()
    {
        let result = controller
            .request(f.auth.clone(), change(owner, true, revision as u64))
            .await
            .unwrap();
        assert_eq!(result["state"], "accepted");
    }
    let driver = tokio::spawn(crate::nkn_product::run(
        Some(controller.clone()),
        ports.auth.clone(),
        None,
        None,
        None,
        ports.providers,
    ));
    let ready = Instant::now() + Duration::from_secs(10);
    while !std::path::Path::new("/run/rustsimhub/nkn/core.sock").exists() {
        assert!(Instant::now() < ready);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let report = controller
        .request(
            f.auth.clone(),
            AuthCommand::NknStatus {
                token: f.owner.expose().to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(report["provider"]["status"]["source"], "engine");
    assert_eq!(report["provider"]["status"]["service"], "running");
    assert_eq!(
        report["provider"]["owners"],
        serde_json::json!({"manual":true,"remote":true,"statistics":true})
    );
    fs::write(
        "/tmp/product-status.json",
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    let grant = f
        .auth
        .lock()
        .unwrap()
        .grant_recovery_key(
            f.owner.expose(),
            SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes(),
            40000,
            0,
            "22222222-2222-4222-8222-222222222222".into(),
        )
        .unwrap();
    fs::write(
        "/run/rustsimhub/recovery/public.json",
        serde_json::to_vec(&serde_json::json!({"pin":pin,"generation":grant.generation})).unwrap(),
    )
    .unwrap();
    let audit = tempfile::tempdir().unwrap();
    fs::set_permissions(audit.path(), fs::Permissions::from_mode(0o700)).unwrap();
    rustsimhub_core::network_trial::provision::enable(audit.path()).unwrap();
    crate::recovery_listener::activate();
    let recovery = tokio::spawn({
        let auth = f.auth.clone();
        async move { crate::recovery_listener::run(auth).await.map_err(|_| ()) }
    });
    let recovery_ready = Instant::now() + Duration::from_secs(3);
    while !std::path::Path::new("/run/rustsimhub/recovery/core.sock").exists() {
        assert!(Instant::now() < recovery_ready);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let contention_auth = f.auth.clone();
    let contention = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(15);
        while !std::path::Path::new("/tmp/product-hold-auth").exists() {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        let _auth = contention_auth.lock().unwrap();
        fs::write("/tmp/product-auth-held", "1").unwrap();
        let end = Instant::now() + Duration::from_millis(600);
        while !crate::nkn_host::AUTH_WAITING.load(std::sync::atomic::Ordering::Acquire) {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(100));
        fs::write("/tmp/product-auth-wait-verified", "1").unwrap();
    });
    fs::write("/tmp/product-ready", "1").unwrap();
    let done = Instant::now() + Duration::from_secs(20);
    while !std::path::Path::new("/tmp/product-client-done").exists() {
        assert!(Instant::now() < done);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    contention.join().unwrap();
    assert!(std::path::Path::new("/tmp/product-auth-wait-verified").exists());
    assert_eq!(
        controller
            .request(f.auth.clone(), change(NknOwner::Remote, false, 3))
            .await
            .unwrap()["state"],
        "accepted"
    );
    assert!(controller.root().source().is_err());
    let shared = controller
        .request(
            f.auth.clone(),
            AuthCommand::NknStatus {
                token: f.owner.expose().to_owned(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        shared["provider"]["owners"],
        serde_json::json!({"manual":true,"remote":false,"statistics":true})
    );
    assert_eq!(shared["provider"]["supervisor_state"], "ready");
    for (revision, owner) in [(4, NknOwner::Statistics), (5, NknOwner::ManualEngine)] {
        assert_eq!(
            controller
                .request(f.auth.clone(), change(owner, false, revision))
                .await
                .unwrap()["state"],
            "accepted"
        );
    }
    let stopped = Instant::now() + Duration::from_secs(6);
    loop {
        if let Ok(state) = controller
            .request(
                f.auth.clone(),
                AuthCommand::NknStatus {
                    token: f.owner.expose().to_owned(),
                },
            )
            .await
        {
            if state["provider"]["supervisor_state"] == "off" {
                assert_eq!(state["provider"]["original_process_exit_verified"], true);
                break;
            }
        }
        assert!(Instant::now() < stopped);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!std::path::Path::new(
        "/sys/fs/cgroup/system.slice/rustsimhub-nkn-root.service/engine"
    )
    .exists());
    recovery.abort();
    let _ = recovery.await;
    driver.abort();
    let _ = driver.await;
}
