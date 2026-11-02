// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(all(test, target_os = "linux"))]
//! Explicit real-UID fixture: actual Noise, Identity, Store, Auth and original
//! Core QueryOperation dispatch. No radio, external network or production keys.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{secure_session::SecureRecoverySessions, Auth, Token};
use rustsimhub_ble::{
    core_channel::CoreRecoveryPort,
    framing::{InputKind, OpaqueBytes},
    ports::{Epoch, Peer, SecurePortV1, SecureReply},
};
use rustsimhub_core::{
    recovery_channel::{RecoveryChannel, SecureApplicationStep},
    recovery_transport::{Roles, SourceGuard},
};
use rustsimhub_recovery_session::{Action, Context, Purpose, Session};
use rustsimhub_storage::{RecoveryGrant, Store};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncReadExt,
    net::{UnixListener, UnixStream},
};

const BODY: &[u8] = br#"{"target_request_id":"00000000-0000-4000-8000-000000000099"}"#;
struct Fixture {
    auth: Arc<Mutex<Auth>>,
    grant: RecoveryGrant,
    owner: Token,
    sessions: Arc<SecureRecoverySessions>,
    _store: Store,
    _dirs: Vec<tempfile::TempDir>,
}
fn private_dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn fixture() -> Fixture {
    // Reuse the original D fixture's explicit bootstrap/migration sequence;
    // replace its synthetic verifier and sealer with actual cryptography.
    let d = private_dir();
    let i = private_dir();
    let b = private_dir();
    let b9 = private_dir();
    let owner = SigningKey::from_bytes(&[61; 32]); // isolated test identity only
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let grant = a
        .grant_recovery_key(
            token.expose(),
            SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes(),
            40000,
            0,
            "00000000-0000-4000-8000-000000000001".into(),
        )
        .unwrap();
    drop(a);
    drop(s);
    Store::migrate_recovery_execution(d.path(), b9.path()).unwrap();
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let sessions = Arc::new(SecureRecoverySessions::new());
    a.set_recovery_verifier(Some(sessions.clone())).unwrap();
    Fixture {
        auth: Arc::new(Mutex::new(a)),
        grant,
        owner: token,
        sessions,
        _store: s,
        _dirs: vec![d, i, b, b9],
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

#[test]
fn isolated_secure_core() {
    runtime().block_on(async {
        assert_eq!(rustix::process::geteuid().as_raw(),1001);
        let root=root(); let f=fixture();
        let guard=SourceGuard::new(Roles::new(1003,1004,1005,1002).unwrap()).unwrap();
        let listener=UnixListener::bind(root.join("core.sock")).unwrap();
        fs::set_permissions(root.join("core.sock"),fs::Permissions::from_mode(0o660)).unwrap();
        // Public out-of-band fixture provisioning only, never accepted from wire.
        fs::write(root.join("public.json"),serde_json::to_vec(&serde_json::json!({"pin":f.auth.lock().unwrap().public_key(),"generation":f.grant.generation})).unwrap()).unwrap();
        fs::set_permissions(root.join("public.json"),fs::Permissions::from_mode(0o640)).unwrap();
        let audit=private_dir();
        rustsimhub_core::network_trial::provision::enable(audit.path()).unwrap();
        println!("SECURE_READY");
        for uid in [0,1001,1003,1004,1005,1006] {
            let (s,_)=listener.accept().await.unwrap();
            assert_eq!(s.peer_cred().unwrap().uid(),uid);
            assert!(guard.accept(s).is_err());
            println!("UID_CASE denied_{uid}");
        }
        for case in ["query", "wrong_pin", "tamper", "old_cipher", "disconnect", "revoked"] {
            let (s,_)=tokio::time::timeout(Duration::from_secs(10),listener.accept()).await.unwrap().unwrap();
            assert_eq!(s.peer_cred().unwrap().uid(),1002);
            let channel=RecoveryChannel::attach(&mut f.auth.lock().unwrap(),guard.accept(s).unwrap()).unwrap();
            let secure=channel.establish_secure(f.auth.clone(),f.sessions.clone()).await;
            if matches!(case,"wrong_pin"|"disconnect") { assert!(secure.is_err()); }
            else {
                let secure=secure.unwrap();
                if case=="revoked" {
                    f.auth.lock().unwrap().revoke_recovery_key(f.owner.expose(),f.grant.independent_public_key,f.grant.generation,"00000000-0000-4000-8000-000000000002".into()).unwrap();
                }
                let result=secure.dispatch(f.auth.clone()).await;
                if case=="query" { match result { Ok(SecureApplicationStep::Continue(_))=>{}, Err(e)=>panic!("actual Core query failed: {e}"), _=>panic!("query unexpectedly closed") } }
                else { assert!(result.is_err()); }
            }
            tokio::time::timeout(Duration::from_secs(2), async { while guard.is_busy() {tokio::task::yield_now().await;} }).await.unwrap();
            println!("UID_CASE {case}");
        }
    });
}

async fn exchange(
    port: &mut CoreRecoveryPort,
    kind: InputKind,
    wire: Vec<u8>,
) -> rustsimhub_ble::ports::SecureReply {
    port.receive(
        Epoch::new([1; 16], 1).unwrap(),
        Peer::from_backend_callback(":1.2", "/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap(),
        kind,
        OpaqueBytes::new(wire).unwrap(),
    )
    .await
    .unwrap()
}
fn auth_bytes(reply: SecureReply) -> Vec<u8> {
    match reply {
        SecureReply::Handshake(b) => b.into_vec(),
        _ => panic!("expected actual Core handshake reply"),
    }
}
async fn eof(s: &mut UnixStream) {
    let mut b = [0];
    match tokio::time::timeout(Duration::from_secs(3), s.read(&mut b))
        .await
        .unwrap()
    {
        Ok(0) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
            ) => {}
        other => panic!("expected rejection without application bytes, received {other:?}"),
    }
}
#[test]
fn isolated_secure_client() {
    runtime().block_on(async {
        assert_eq!(rustix::process::geteuid().as_raw(), 1002);
        let case = std::env::var("RUSTSIMHUB_SECURE_CASE").unwrap();
        let stream = UnixStream::connect(root().join("core.sock")).await.unwrap();
        if case == "disconnect" {
            return;
        }
        let native = stream.into_std().unwrap();
        let mut monitor = UnixStream::from_std(native.try_clone().unwrap()).unwrap();
        let mut port =
            CoreRecoveryPort::attach(UnixStream::from_std(native).unwrap(), 1001).unwrap();
        let public: serde_json::Value =
            serde_json::from_slice(&fs::read(root().join("public.json")).unwrap()).unwrap();
        let pin: [u8; 32] = serde_json::from_value(public["pin"].clone()).unwrap();
        let pin = if case == "wrong_pin" {
            SigningKey::from_bytes(&[63; 32]).verifying_key().to_bytes()
        } else {
            pin
        };
        let key = SigningKey::from_bytes(&[62; 32]);
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).unwrap();
        let context = Context::new(
            Purpose::BleRecovery,
            pin,
            key.verifying_key().to_bytes(),
            public["generation"].as_u64().unwrap(),
            nonce,
        )
        .unwrap();
        let origin = Instant::now();
        let now = || u64::try_from(origin.elapsed().as_millis()).unwrap();
        let mut session = Session::client(context, now(), 300_000).unwrap();
        let noise = auth_bytes(
            exchange(
                &mut port,
                InputKind::Auth,
                session.write_client_hello(now()).unwrap(),
            )
            .await,
        );
        if case == "wrong_pin" {
            assert!(session.read_handshake(&noise, now()).is_err());
            drop(port);
            return;
        }
        session.read_handshake(&noise, now()).unwrap();
        let binding = auth_bytes(
            exchange(
                &mut port,
                InputKind::Auth,
                session.write_handshake(now()).unwrap(),
            )
            .await,
        );
        session.open_binding(&binding, now()).unwrap();
        let signature = key
            .sign(&session.binding_message(now()).unwrap())
            .to_bytes();
        let ack = auth_bytes(
            exchange(
                &mut port,
                InputKind::Auth,
                session.seal_binding(&signature, now()).unwrap(),
            )
            .await,
        );
        session.open_binding_ack(&ack, now()).unwrap();
        let mut wire = session
            .seal_request([9; 16], Action::QueryOperation, BODY, now())
            .unwrap();
        let old = PathBuf::from("/tmp/recovery-secure-peer/old-cipher");
        if case == "query" {
            fs::write(&old, &wire).unwrap();
        }
        if case == "tamper" {
            wire[0] ^= 1;
        }
        if case == "old_cipher" {
            wire = fs::read(old).unwrap();
        }
        let result = port
            .receive(
                Epoch::new([1; 16], 1).unwrap(),
                Peer::from_backend_callback(":1.2", "/org/bluez/hci0/dev_00_11_22_33_44_55")
                    .unwrap(),
                InputKind::Command,
                OpaqueBytes::new(wire).unwrap(),
            )
            .await;
        if case == "query" {
            let wire = match result.unwrap() {
                SecureReply::Application(b) => b.into_vec(),
                _ => panic!("expected actual Core application reply"),
            };
            // Exact real Core response, not an echo or fabricated receipt.
            let body = session.open_response(&wire, now()).unwrap();
            let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["lookup"], "not_found");
            assert_eq!(value["operation"], serde_json::Value::Null);
            // Keep the source alive through Core's post-write scope check.
            // A client close races the EOF monitor and deliberately cancels it.
            eof(&mut monitor).await;
        } else {
            assert!(result.is_err());
            eof(&mut monitor).await;
        }
    });
}
