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
        assert_eq!(rustix::process::geteuid().as_raw(), 1002);
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
            assert_eq!(response["result"]["mode"], "authenticated_read_only");
            send(&mut stream, 3, &[]).await;
            eof(&mut stream).await;
        } else {
            eof(&mut stream).await;
        }
    });
}
