// SPDX-License-Identifier: AGPL-3.0-only
//! D-22 proposal evidence against the existing real listener, kernel and parser.
//! No proposed authenticator, trusted capability or IPC server is implemented here.
use crate::{authenticated, linux, socket_runtime::SocketRuntime};
use rustsimhub_storage::Store;
use rustsimhub_wire::{AuthCommand, AuthRequest, Response};
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command, time::Duration};
use tokio::{net::UnixStream, sync::oneshot, time::timeout};

fn dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
async fn ready(path: &Path) {
    timeout(Duration::from_secs(3), async {
        while !path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
fn request(command: AuthCommand) -> Vec<u8> {
    serde_json::to_vec(&AuthRequest {
        protocol: 2,
        id: "synthetic-d22".into(),
        command,
    })
    .unwrap()
}

#[tokio::test]
async fn real_child_peer_uid_gid_and_pid_come_from_kernel_not_json() {
    let d = dir();
    let path = d.path().join("proof.sock");
    let (listener, _runtime) = SocketRuntime::bind(&path).unwrap();
    // This child uses only a fixed local socket and synthetic input. No UID changes or shell.
    let mut child=Command::new("python3").args(["-c",
        "import socket,sys,struct; s=socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect(sys.argv[1]); b=b'{\"uid\":0,\"pid\":1,\"authenticated\":true,\"channel\":\"ble\"}'; s.sendall(struct.pack('>I',len(b))+b); s.close()"])
        .arg(&path).spawn().unwrap();
    let child_pid = child.id();
    let (mut stream, _) = timeout(Duration::from_secs(3), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let before = stream.peer_cred().unwrap();
    let body = timeout(Duration::from_secs(3), linux::read_frame(&mut stream))
        .await
        .unwrap()
        .unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["uid"], 0);
    assert_eq!(payload["pid"], 1);
    assert_eq!(before.uid(), rustix::process::geteuid().as_raw());
    assert_eq!(before.gid(), rustix::process::getegid().as_raw());
    assert_eq!(before.pid(), Some(child_pid as i32));
    assert_ne!(before.pid(), Some(std::process::id() as i32));
    assert_eq!(stream.peer_cred().unwrap().uid(), before.uid());
    assert!(child.wait().unwrap().success());
    assert!(rustsimhub_wire::parse_auth_request(&body).is_err());
}

#[tokio::test]
async fn two_distinct_connections_share_credentials_so_uid_pid_are_not_connection_ids() {
    let d = dir();
    let path = d.path().join("two.sock");
    let (listener, _runtime) = SocketRuntime::bind(&path).unwrap();
    let a = UnixStream::connect(&path).await.unwrap();
    let b = UnixStream::connect(&path).await.unwrap();
    let (one, _) = listener.accept().await.unwrap();
    let (two, _) = listener.accept().await.unwrap();
    let first = one.peer_cred().unwrap();
    let second = two.peer_cred().unwrap();
    assert_eq!(
        (first.uid(), first.gid(), first.pid()),
        (second.uid(), second.gid(), second.pid())
    );
    drop(a);
    drop(b);
    // An actual accepted connection therefore still needs a Core-generated nonce/epoch.
}

#[tokio::test]
async fn real_core_listener_rejects_wrong_configured_uid_before_public_identity_dispatch() {
    let d = dir();
    let state = dir();
    let identity = dir();
    let path = d.path().join("core.sock");
    let wrong_uid = rustix::process::geteuid().as_raw().checked_add(1).unwrap();
    let (stop, stopped) = oneshot::channel();
    let server = authenticated::serve_until(
        path.to_str().unwrap(),
        wrong_uid,
        state.path().to_str().unwrap(),
        identity.path().to_str().unwrap(),
        async {
            let _ = stopped.await;
        },
    );
    let client = async {
        ready(&path).await;
        let mut stream = UnixStream::connect(&path).await.unwrap();
        let _ = linux::write_frame(&mut stream, &request(AuthCommand::Identity {})).await;
        assert!(
            timeout(Duration::from_secs(3), linux::read_frame(&mut stream))
                .await
                .unwrap()
                .is_err()
        );
        let _ = stop.send(());
    };
    let (result, ()) = tokio::join!(server, client);
    result.unwrap();
    assert!(!path.exists());
    assert!(Store::open(state.path())
        .unwrap()
        .handle()
        .principals()
        .unwrap()
        .is_empty());
    // The actual client UID stayed unchanged; this does not claim a second-UID process was run.
}

#[tokio::test]
async fn real_core_matching_uid_still_needs_owner_proof_for_recovery_management() {
    let d = dir();
    let state = dir();
    let identity = dir();
    let path = d.path().join("core.sock");
    let uid = rustix::process::geteuid().as_raw();
    let (stop, stopped) = oneshot::channel();
    let server = authenticated::serve_until(
        path.to_str().unwrap(),
        uid,
        state.path().to_str().unwrap(),
        identity.path().to_str().unwrap(),
        async {
            let _ = stopped.await;
        },
    );
    let client = async {
        ready(&path).await;
        let mut stream = UnixStream::connect(&path).await.unwrap();
        linux::write_frame(&mut stream, &request(AuthCommand::Identity {}))
            .await
            .unwrap();
        let identity: Response =
            serde_json::from_slice(&linux::read_frame(&mut stream).await.unwrap()).unwrap();
        assert!(identity.result.get("error").is_none());
        drop(stream);
        let mut stream = UnixStream::connect(&path).await.unwrap();
        linux::write_frame(
            &mut stream,
            &request(AuthCommand::RecoveryGrant {
                token: "0".repeat(64),
                public_key: [2; 32],
                max_trial_ms: 40000,
                expected_generation: "0".into(),
                request_id: "00000000-0000-4000-8000-000000000001".into(),
            }),
        )
        .await
        .unwrap();
        let denied: Response =
            serde_json::from_slice(&linux::read_frame(&mut stream).await.unwrap()).unwrap();
        assert!(denied.result.get("error").is_some());
        let _ = stop.send(());
    };
    let (result, ()) = tokio::join!(server, client);
    result.unwrap();
    assert!(Store::open(state.path())
        .unwrap()
        .handle()
        .principals()
        .unwrap()
        .is_empty());
    assert!(!path.exists());
}

#[test]
fn general_auth_frame_budget_is_not_a_ble_recovery_envelope_budget() {
    let mut normal = request(AuthCommand::Identity {});
    normal.resize(2049, b' ');
    assert!(rustsimhub_wire::parse_auth_request(&normal).is_ok());
    assert!(normal.len() < rustsimhub_wire::MAX_FRAME);
    // Intentional current Auth behavior: the dedicated proposed BLE decoder must enforce 2048.
}
