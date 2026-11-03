// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
fn key(value: u8) -> Vec<u8> {
    vec![value; 32]
}
fn prepare() -> serde_json::Value {
    serde_json::json!({"protocol":2,"id":1,"command":{"op":"prepare","session":"a".repeat(32),"relay_handle":"approved-relay","expected_peer":key(7),"offer":"synthetic-offer"}})
}
fn encoded(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}
#[test]
fn v2_is_explicit_bounded_and_cannot_take_over_v1_or_call_control() {
    let value = prepare();
    assert!(parse(&encoded(&value)).is_ok());
    for (field, new) in [
        ("protocol", serde_json::json!(1)),
        ("id", serde_json::json!(0)),
    ] {
        let mut bad = value.clone();
        bad[field] = new;
        assert!(parse(&encoded(&bad)).is_err());
    }
    for operation in [
        "stop_all",
        "dial",
        "answer",
        "renew_call",
        "authorize_owner",
        "prepare_call",
    ] {
        let mut bad = value.clone();
        bad["command"]["op"] = operation.into();
        assert!(parse(&encoded(&bad)).is_err());
    }
    assert!(crate::worker::parse(&encoded(&value)).is_err());
    assert!(parse(&vec![b' '; MAX_FRAME + 1]).is_err());
}
#[test]
fn request_objects_duplicates_and_credential_objects_are_not_positional_arrays() {
    for bytes in [
        b"[2,1,{}]".as_slice(),
        b"{\"protocol\":2,\"protocol\":2,\"id\":1,\"command\":{}}",
        b"null",
    ] {
        assert!(parse(bytes).is_err());
    }
    let request = serde_json::json!({"protocol":2,"id":2,"command":{"op":"authorize","session":"a".repeat(32),"credentials":{"boot":"a".repeat(64),"ticket":key(0),"peer_key":key(7)}}});
    assert!(parse(&encoded(&request)).is_ok());
    let mut bad = request;
    bad["command"]["credentials"] = serde_json::json!(["a".repeat(64), key(0), key(7)]);
    assert!(parse(&encoded(&bad)).is_err());
}
#[test]
fn no_url_endpoint_role_bool_scope_or_secret_can_be_added_to_a_prepare() {
    let value = prepare();
    for name in [
        "uid",
        "role",
        "owner",
        "verified",
        "authorized",
        "scope",
        "core_socket",
        "pcm_device",
        "server",
        "url",
        "username",
        "password",
        "ticket",
        "generation",
    ] {
        let mut bad = value.clone();
        bad["command"][name] = "untrusted".into();
        assert!(parse(&encoded(&bad)).is_err(), "{name}");
    }
    for handle in ["", "https://any", "../relay", "UPPER", "a/b", "a b"] {
        let mut bad = value.clone();
        bad["command"]["relay_handle"] = handle.into();
        assert!(parse(&encoded(&bad)).is_err());
    }
    let mut bad = value;
    bad["command"]["expected_peer"] = serde_json::json!(key(0));
    assert!(parse(&encoded(&bad)).is_err());
}
fn snapshot() -> Snapshot {
    Snapshot {
        session: "a".repeat(32),
        phase: Phase::Handshaking,
        peer_key: None,
        answer: Some("synthetic".into()),
        pcm_opened: false,
        counters: Counters::default(),
        failure: None,
        release: Release::NotRequested,
    }
}
#[test]
fn reply_id_version_session_and_positive_phase_shape_are_checked() {
    let good = serde_json::to_vec(&Reply {
        protocol: 2,
        id: 1,
        result: Ok(snapshot()),
    })
    .unwrap();
    assert!(parse_reply(&good, 1, &"a".repeat(32)).is_ok());
    let mut positional: serde_json::Value = serde_json::from_slice(&good).unwrap();
    positional["result"]["Ok"]["counters"] = serde_json::json!([0, 0, 0, 0, 0, 0]);
    assert!(parse_reply(&encoded(&positional), 1, &"a".repeat(32)).is_err());
    assert!(parse_reply(&good, 2, &"a".repeat(32)).is_err());
    assert!(parse_reply(&good, 1, &"b".repeat(32)).is_err());
    for phase in [Phase::Verified, Phase::Authorizing, Phase::Active] {
        let mut s = snapshot();
        s.phase = phase;
        let b = serde_json::to_vec(&Reply {
            protocol: 2,
            id: 1,
            result: Ok(s),
        })
        .unwrap();
        assert!(parse_reply(&b, 1, &"a".repeat(32)).is_err());
    }
    let mut s = snapshot();
    s.phase = Phase::Closed;
    s.pcm_opened = true;
    let b = serde_json::to_vec(&Reply {
        protocol: 2,
        id: 1,
        result: Ok(s),
    })
    .unwrap();
    assert!(parse_reply(&b, 1, &"a".repeat(32)).is_err());
    assert!(parse_reply(br#"[2,1,{"Err":"denied"}]"#, 1, &"a".repeat(32)).is_err());
}
#[cfg(target_os = "linux")]
mod local {
    use super::*;
    use std::{os::unix::fs::PermissionsExt, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::UnixListener,
    };
    async fn endpoint() -> (tempfile::TempDir, UnixListener, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("v2.sock");
        let listener = UnixListener::bind(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o660)).unwrap();
        (dir, listener, p)
    }
    fn command() -> Command {
        parse(&encoded(&prepare())).unwrap().command
    }
    async fn read(s: &mut tokio::net::UnixStream) -> Request {
        let n = s.read_u32().await.unwrap() as usize;
        let mut b = vec![0; n];
        s.read_exact(&mut b).await.unwrap();
        parse(&b).unwrap()
    }
    async fn reply(s: &mut tokio::net::UnixStream, id: u64) {
        let b = serde_json::to_vec(&Reply {
            protocol: 2,
            id,
            result: Ok(snapshot()),
        })
        .unwrap();
        s.write_u32(b.len() as u32).await.unwrap();
        s.write_all(&b).await.unwrap();
    }
    #[tokio::test]
    async fn actual_socket_requires_kernel_uid_and_keeps_one_connection_session_and_ids() {
        let (_dir, listener, path) = endpoint().await;
        let uid = rustix::process::geteuid().as_raw();
        assert!(Client::connect(&path, uid + 1, "a".repeat(32))
            .await
            .is_err());
        let mut client = Client::connect(&path, uid, "a".repeat(32)).await.unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            assert_eq!(read(&mut s).await.id, 1);
            reply(&mut s, 1).await;
            let r = read(&mut s).await;
            assert_eq!(r.id, 2);
            assert!(matches!(r.command, Command::Inspect { .. }));
            reply(&mut s, 2).await;
        });
        client.exchange(command()).await.unwrap();
        assert!(client.exchange(command()).await.is_err());
        assert!(client
            .exchange(Command::Inspect {
                session: "b".repeat(32)
            })
            .await
            .is_err());
        client
            .exchange(Command::Inspect {
                session: "a".repeat(32),
            })
            .await
            .unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn cancelled_partial_exchange_closes_actual_fd_and_cannot_replay_or_reconnect() {
        let (_dir, listener, path) = endpoint().await;
        let uid = rustix::process::geteuid().as_raw();
        let mut client = Client::connect(&path, uid, "a".repeat(32)).await.unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let _ = read(&mut s).await;
            tokio::time::timeout(Duration::from_secs(1), async {
                assert_eq!(
                    s.read_u8().await.unwrap_err().kind(),
                    std::io::ErrorKind::UnexpectedEof
                );
            })
            .await
            .unwrap();
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(30), client.exchange(command()))
                .await
                .is_err()
        );
        assert!(client.exchange(command()).await.is_err());
        server.await.unwrap();
    }
    #[tokio::test]
    async fn wrong_reply_id_permanently_fences_the_original_controller() {
        let (_dir, listener, path) = endpoint().await;
        let uid = rustix::process::geteuid().as_raw();
        let mut client = Client::connect(&path, uid, "a".repeat(32)).await.unwrap();
        let server = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let _ = read(&mut s).await;
            reply(&mut s, 2).await;
        });
        assert!(client.exchange(command()).await.is_err());
        assert!(client.exchange(command()).await.is_err());
        server.await.unwrap();
    }
}
