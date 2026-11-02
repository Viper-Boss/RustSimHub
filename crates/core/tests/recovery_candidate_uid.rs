// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#![cfg(all(test, target_os = "linux"))]
//! Explicit private broker/three actual UIDs. Real D service, A candidate and
//! original Core Wi-Fi actor; only BlueZ/NM/MM and the radio peer are external.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{secure_session::SecureRecoverySessions, Auth, Token};
use rustsimhub_core::{
    recovery_channel::{RecoveryChannel, SecureApplicationStep},
    recovery_transport::{Roles, SourceGuard},
};
use rustsimhub_storage::{RecoveryGrant, Store};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::UnixListener;

#[path = "fixtures/recovery_candidate_backend.rs"]
mod backend;
#[path = "fixtures/recovery_candidate_peer.rs"]
mod peer;

const CORE_SOCKET: &str = rustsimhub_ble::service::CORE_SOCKET;
fn root() -> PathBuf {
    assert_eq!(
        std::env::var("RUSTSIMHUB_RECOVERY_CANDIDATE_FIXTURE").as_deref(),
        Ok("1")
    );
    PathBuf::from("/fixture")
}
fn case() -> String {
    std::env::var("RUSTSIMHUB_RECOVERY_CASE").unwrap()
}
async fn wait(name: &str) {
    tokio::time::timeout(Duration::from_secs(12), async {
        while !root().join(name).exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("private fixture marker timeout: {name}"));
}
fn write(name: &str, value: &serde_json::Value) {
    fs::write(root().join(name), serde_json::to_vec(value).unwrap()).unwrap();
}
fn role(uid: u32) {
    assert_eq!(rustix::process::getuid().as_raw(), uid);
    assert_eq!(rustix::process::geteuid().as_raw(), uid);
    let status = fs::read_to_string("/proc/self/status").unwrap();
    assert!(status
        .lines()
        .any(|line| line == "CapEff:\t0000000000000000"));
}
struct Fixture {
    auth: Arc<Mutex<Auth>>,
    owner: Token,
    grant: RecoveryGrant,
    sessions: Arc<SecureRecoverySessions>,
    _store: Store,
    _directories: Vec<tempfile::TempDir>,
}
fn private_dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn fixture() -> Fixture {
    let directories = (0..4).map(|_| private_dir()).collect::<Vec<_>>();
    let key = SigningKey::from_bytes(&[61; 32]);
    let store = Store::open(directories[0].path()).unwrap();
    let mut auth = Auth::open(directories[1].path(), store.handle()).unwrap();
    let c = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(auth);
    drop(store);
    let store =
        Store::migrate_recovery_grants(directories[0].path(), directories[2].path()).unwrap();
    let mut auth = Auth::open(directories[1].path(), store.handle()).unwrap();
    let c = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    let owner = auth
        .login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let grant = auth
        .grant_recovery_key(
            owner.expose(),
            SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes(),
            40_000,
            0,
            peer::uuid(1),
        )
        .unwrap();
    drop(auth);
    drop(store);
    Store::migrate_recovery_execution(directories[0].path(), directories[3].path()).unwrap();
    let store = Store::open_recovery_execution(directories[0].path()).unwrap();
    let mut auth = Auth::open(directories[1].path(), store.handle()).unwrap();
    let c = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    let owner = auth
        .login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    let sessions = Arc::new(SecureRecoverySessions::new());
    auth.set_recovery_verifier(Some(sessions.clone())).unwrap();
    Fixture {
        auth: Arc::new(Mutex::new(auth)),
        owner,
        grant,
        sessions,
        _store: store,
        _directories: directories,
    }
}
#[tokio::test]
#[ignore = "explicit network-none actual UID recovery candidate fixture only"]
async fn core_role() {
    role(1001);
    let f = fixture();
    let audit = private_dir();
    rustsimhub_core::network_trial::provision::enable(audit.path()).unwrap();
    if case() == "normal_usb_denied" {
        wait("backend/ready.json").await;
        let command = |token: String| rustsimhub_wire::AuthCommand::WifiBegin {
            token,
            idempotency_key: "synthetic-normal-no-usb".into(),
            ssid: "SYNTHETIC_NATIVE".into(),
            passphrase: "SYNTHETIC_NATIVE_PASSWORD".into(),
        };
        assert!(rustsimhub_core::network_trial::provision::authorized(
            f.auth.clone(),
            command("synthetic-invalid-token".into())
        )
        .await
        .is_err());
        let normal = rustsimhub_core::network_trial::provision::authorized(
            f.auth.clone(),
            command(f.owner.expose().into()),
        )
        .await
        .unwrap();
        let id = normal["trial_id"].as_str().unwrap().to_owned();
        tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                let status = rustsimhub_core::network_trial::provision::authorized(
                    f.auth.clone(),
                    rustsimhub_wire::AuthCommand::WifiStatus {
                        token: f.owner.expose().into(),
                        trial_id: id.clone(),
                    },
                )
                .await
                .unwrap();
                if status["state"] == "rolled_back" {
                    break;
                }
                assert_eq!(status["state"], "starting");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        write("core/normal_usb_denied", &serde_json::json!(true));
    }
    let network = if case() == "busy_network" {
        Some(rustsimhub_core::coordination::global().network().unwrap())
    } else {
        None
    };
    let telephony = if case() == "call_busy" {
        rustsimhub_core::coordination::global()
            .telephony(rustsimhub_domain::CallAction::Dial)
            .unwrap()
    } else {
        None
    };
    let listener = UnixListener::bind(CORE_SOCKET).unwrap();
    fs::set_permissions(CORE_SOCKET, fs::Permissions::from_mode(0o660)).unwrap();
    write(
        "core/public.json",
        &serde_json::json!({"pin":f.auth.lock().unwrap().public_key(),"generation":f.grant.generation}),
    );
    let guard = SourceGuard::new(Roles::new(1003, 1004, 1005, 1002).unwrap()).unwrap();
    // The installed D endpoint first probes real metadata/UID without a packet.
    let (probe, _) = listener.accept().await.unwrap();
    assert_eq!(probe.peer_cred().unwrap().uid(), 1002);
    let probe =
        RecoveryChannel::attach(&mut f.auth.lock().unwrap(), guard.accept(probe).unwrap()).unwrap();
    assert!(probe
        .establish_secure(f.auth.clone(), f.sessions.clone())
        .await
        .is_err());
    tokio::time::timeout(Duration::from_secs(2), async {
        while guard.is_busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let (stream, _) = listener.accept().await.unwrap();
    assert_eq!(stream.peer_cred().unwrap().uid(), 1002);
    let channel =
        RecoveryChannel::attach(&mut f.auth.lock().unwrap(), guard.accept(stream).unwrap())
            .unwrap();
    let auth = f.auth.clone();
    let grant = f.grant.clone();
    let owner = f.owner;
    let revoke = if case().starts_with("revoke_") {
        Some(tokio::spawn(async move {
            wait("control/revoke").await;
            auth.lock()
                .unwrap()
                .revoke_recovery_key(
                    owner.expose(),
                    grant.independent_public_key,
                    grant.generation,
                    peer::uuid(2),
                )
                .unwrap();
            write("core/revoked", &serde_json::json!(true));
        }))
    } else {
        None
    };
    let mut steps = 0;
    let mut secure = channel
        .establish_secure(f.auth.clone(), f.sessions.clone())
        .await;
    let established = secure.is_ok();
    if let Err(error) = &secure {
        eprintln!("candidate_handshake_error={error}");
    }
    let admission = if matches!(
        case().as_str(),
        "confirm" | "cancel" | "revoke_after_save" | "normal_usb_denied" | "rollback_unknown"
    ) {
        let auth = f.auth.clone();
        let path = f._directories[0].path().join("store.sqlite3");
        Some(tokio::spawn(async move {
            wait("backend/ready_held").await;
            let db = rusqlite::Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            tokio::time::timeout(Duration::from_secs(4), async move {
                loop {
                    let claimed: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM recovery_execution_receipts WHERE action IN ('confirm_wifi','cancel_wifi') AND state='claimed')", [], |r|r.get(0)).unwrap();
                    let admitted = claimed && auth.try_lock().map(|guard| {drop(guard);true}).unwrap_or(false);
                    if admitted { break; }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }).await.unwrap();
            write("control/release_ready", &serde_json::json!(true));
        }))
    } else {
        None
    };
    while let Ok(channel) = secure {
        match channel.dispatch(f.auth.clone()).await {
            Ok(SecureApplicationStep::Continue(channel)) => {
                steps += 1;
                secure = Ok(channel);
            }
            Ok(SecureApplicationStep::Closed) => break,
            Err(error) => {
                eprintln!("candidate_dispatch_error={error}");
                break;
            }
        }
    }
    assert_eq!(established, case() != "wrong_pin");
    drop(network);
    drop(telephony);
    if let Some(revoke) = revoke {
        revoke.abort();
        let _ = revoke.await;
    }
    if let Some(admission) = admission {
        admission.await.unwrap();
    }
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if !guard.is_busy() && rustsimhub_core::coordination::global().network().is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    // Read-only observation of this synthetic private test DB, never authority.
    let db = rusqlite::Connection::open_with_flags(
        f._directories[0].path().join("store.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db.prepare("SELECT action,state,settings_saved,rollback_verified FROM recovery_execution_receipts ORDER BY action").unwrap();
    let journal = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    if matches!(
        case().as_str(),
        "wrong_pin" | "tamper" | "busy_network" | "call_busy"
    ) {
        assert!(journal.is_empty());
    } else {
        assert!(journal.iter().any(|r| r.0 == "begin_wifi"));
        assert!(
            journal.iter().all(|r| r.1
                == if matches!(case().as_str(), "confirm" | "normal_usb_denied") {
                    "saved"
                } else if case() == "rollback_unknown" {
                    "unknown"
                } else {
                    "rolled_back"
                }),
            "private journal: {journal:?}"
        );
    }
    write(
        "core/done.json",
        &serde_json::json!({"uid":1001,"case":case(),"actual_auth_and_secure_session":true,"established":established,"application_replies":steps,"original_network_lock_released":true,"normal_usb_gate_checked":case()=="normal_usb_denied","journal":journal}),
    );
}
#[tokio::test]
#[ignore = "explicit network-none actual UID recovery candidate fixture only"]
async fn ble_role() {
    role(1002);
    wait("core/public.json").await;
    wait("backend/ready.json").await;
    let address = std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap();
    let connection = zbus::connection::Builder::address(address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let broker = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
    let owner = broker
        .get_name_owner("org.bluez".try_into().unwrap())
        .await
        .unwrap();
    let control = rustsimhub_ble::bluez_control::BluezControl::bind(
        connection,
        owner.as_str(),
        rustsimhub_ble::bluez_control::Adapter::Hci0,
    )
    .await
    .unwrap();
    let endpoint = rustsimhub_ble::service::endpoint_from_policy(
        br#"{"format":1,"core_uid":1001,"ble_uid":1002,"ble_gid":1002}"#,
    )
    .unwrap();
    let port = rustsimhub_ble::core_port::UnixSecurePort::connect(endpoint)
        .await
        .unwrap();
    let result =
        rustsimhub_ble::service::run(control, port, [54; 16], std::future::pending::<()>()).await;
    write(
        "ble/done.json",
        &serde_json::json!({"uid":1002,"case":case(),"actual_non_test_service":true,"actual_fixed_core_port":true,"closed":true,"clean_shutdown":result.is_ok()}),
    );
}
#[tokio::test]
#[ignore = "explicit network-none actual UID recovery candidate fixture only"]
async fn backend_role() {
    role(1003);
    backend::run().await;
}
