// SPDX-License-Identifier: AGPL-3.0-only
//! Actual local client/Permit/Pump, with only the Core socket peer and PCM I/O
//! replaced. There is no modem, ALSA, TURN, public socket or recording.
use super::*;
use crate::{Direction, Frame, IoError, Pcm, Pump, SAMPLES};
use std::{
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone, Copy, Debug)]
enum ReplyCase {
    Correct,
    WrongId,
    WrongProtocol,
    WrongBoot,
    MissingBoot,
    WrongType,
    PastLease,
    TooLongLease,
    MaximumLease,
    InvalidJson,
    DuplicateId,
    UnknownRole,
    ZeroLength,
    OverlongLength,
    Truncated,
    Disconnect,
    Timeout,
}

async fn setup(
    script: Vec<ReplyCase>,
) -> (
    Client,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    Arc<AtomicUsize>,
) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = directory.path().join("core.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        for (index, case) in std::iter::once(ReplyCase::Correct)
            .chain(script)
            .enumerate()
        {
            let length = stream.read_u32().await.unwrap() as usize;
            assert!((1..=protocol::MAX_FRAME).contains(&length));
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await.unwrap();
            let request = protocol::parse(&bytes).unwrap();
            assert_eq!(request.id, index as u64 + 1);
            if index == 0 {
                match request.command {
                    Command::Redeem { ticket, peer_key } => {
                        assert_eq!(ticket, [8; 32]);
                        assert_eq!(peer_key, [7; 32]);
                    }
                    _ => panic!("expected a redeem, no business command"),
                }
            } else {
                assert!(matches!(request.command, Command::Pulse {}));
            }
            observed.fetch_add(1, Ordering::SeqCst);
            match case {
                ReplyCase::Timeout => std::future::pending::<()>().await,
                ReplyCase::Disconnect => return,
                ReplyCase::ZeroLength => {
                    stream.write_u32(0).await.unwrap();
                    return;
                }
                ReplyCase::OverlongLength => {
                    stream
                        .write_u32(protocol::MAX_FRAME as u32 + 1)
                        .await
                        .unwrap();
                    return;
                }
                ReplyCase::Truncated => {
                    stream.write_u32(100).await.unwrap();
                    stream.write_all(b"{\"protocol\":1").await.unwrap();
                    return;
                }
                _ => {}
            }
            let now = now_ms();
            let mut reply = serde_json::json!({"protocol":protocol::PROTOCOL,"id":request.id,"boot":"a".repeat(64),"until_ms":now+900});
            match case {
                ReplyCase::WrongId => reply["id"] = (request.id + 1).into(),
                ReplyCase::WrongProtocol => reply["protocol"] = 2.into(),
                ReplyCase::WrongBoot => reply["boot"] = "b".repeat(64).into(),
                ReplyCase::MissingBoot => {
                    reply.as_object_mut().unwrap().remove("boot");
                }
                ReplyCase::WrongType => reply["until_ms"] = "900".into(),
                ReplyCase::PastLease => reply["until_ms"] = now.into(),
                ReplyCase::TooLongLease => reply["until_ms"] = (now + 10_000).into(),
                ReplyCase::MaximumLease => reply["until_ms"] = u64::MAX.into(),
                ReplyCase::UnknownRole => reply["role"] = "owner".into(),
                _ => {}
            }
            let bytes = match case {
                ReplyCase::InvalidJson => vec![0xff, 0xfe],
                ReplyCase::DuplicateId => format!(
                    "{{\"protocol\":1,\"id\":{},\"id\":{},\"boot\":\"{}\",\"until_ms\":{}}}",
                    request.id,
                    request.id,
                    "a".repeat(64),
                    now + 900
                )
                .into_bytes(),
                _ => serde_json::to_vec(&reply).unwrap(),
            };
            stream.write_u32(bytes.len() as u32).await.unwrap();
            stream.write_all(&bytes).await.unwrap();
        }
    });
    let client = Client::redeem(
        &socket,
        rustix::process::geteuid().as_raw(),
        Credentials {
            ticket: [8; 32],
            peer_key: [7; 32],
            boot: "a".repeat(64),
        },
    )
    .await
    .unwrap();
    (client, server, directory, count)
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
struct IoCounts {
    read: usize,
    write: usize,
    close: usize,
}
struct Device(Arc<Mutex<IoCounts>>);
impl Pcm for Device {
    fn read(&mut self, _: &mut [i16]) -> Result<usize, IoError> {
        self.0.lock().unwrap().read += 1;
        Err(IoError::Again)
    }
    fn write(&mut self, samples: &[i16]) -> Result<usize, IoError> {
        self.0.lock().unwrap().write += 1;
        Ok(samples.len())
    }
    fn recover(&mut self, _: Direction) -> Result<(), IoError> {
        panic!("revocation must not recover audio")
    }
    fn close(&mut self) {
        self.0.lock().unwrap().close += 1;
    }
}

#[tokio::test]
async fn cancelling_an_inflight_pulse_retires_audio_even_when_the_client_is_retained() {
    let (mut client, server, _directory, count) = setup(vec![ReplyCase::Timeout]).await;
    let permit = client.permit();
    let now = now_ms();
    let io = Arc::new(Mutex::new(IoCounts::default()));
    let mut pump = Pump::new(Device(io.clone()), permit.clone(), now).unwrap();
    assert!(timeout(Duration::from_millis(50), client.pulse())
        .await
        .is_err());
    assert_eq!(count.load(Ordering::SeqCst), 2); // Request crossed the socket.
    assert_eq!(permit.check(0), Err(Error::Denied));
    assert_eq!(pump.step(now_ms()), Err(Error::Denied));
    assert_eq!(
        *io.lock().unwrap(),
        IoCounts {
            close: 1,
            ..IoCounts::default()
        }
    );
    server.abort();
    assert!(server.await.unwrap_err().is_cancelled());
}

#[tokio::test]
async fn a_successful_actual_pulse_keeps_permission_until_the_client_is_dropped() {
    let (mut client, server, _directory, count) = setup(vec![ReplyCase::Correct]).await;
    let permit = client.permit();
    let not_polled = client.pulse();
    drop(not_polled); // No request or observation existed to retire.
    permit.check(now_ms()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    client.pulse().await.unwrap();
    permit.check(now_ms()).unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
    server.await.unwrap();
    drop(client);
    assert_eq!(permit.check(0), Err(Error::Denied));
}

#[tokio::test]
async fn exhausted_local_request_counter_revokes_existing_audio_before_returning_error() {
    let (mut client, server, _directory, count) = setup(vec![]).await;
    let permit = client.permit();
    let now = now_ms();
    permit.check(now).unwrap();
    let io = Arc::new(Mutex::new(IoCounts::default()));
    let mut pump = Pump::new(Device(io.clone()), permit.clone(), now).unwrap();
    pump.step(now).unwrap();
    let before = *io.lock().unwrap();
    client.id = u64::MAX; // Private test access, never a production input/hook.
    assert_eq!(client.pulse().await, Err(Error::Expired));
    // Use zero solely to distinguish immediate revocation from natural expiry.
    assert_eq!(permit.check(0), Err(Error::Denied));
    assert_eq!(pump.step(now_ms()), Err(Error::Denied));
    let after = *io.lock().unwrap();
    assert_eq!(
        (after.read, after.write, after.close),
        (before.read, before.write, 1)
    );
    assert_eq!(count.load(Ordering::SeqCst), 1);
    server.await.unwrap();
}

#[tokio::test]
async fn malformed_lost_or_stale_actual_socket_receipts_revoke_pump_without_retries() {
    for case in [
        ReplyCase::WrongId,
        ReplyCase::WrongProtocol,
        ReplyCase::WrongBoot,
        ReplyCase::MissingBoot,
        ReplyCase::WrongType,
        ReplyCase::PastLease,
        ReplyCase::TooLongLease,
        ReplyCase::MaximumLease,
        ReplyCase::InvalidJson,
        ReplyCase::DuplicateId,
        ReplyCase::UnknownRole,
        ReplyCase::ZeroLength,
        ReplyCase::OverlongLength,
        ReplyCase::Truncated,
        ReplyCase::Disconnect,
        ReplyCase::Timeout,
    ] {
        let (mut client, server, _directory, count) = setup(vec![case]).await;
        let permit = client.permit();
        let now = now_ms();
        let io = Arc::new(Mutex::new(IoCounts::default()));
        let mut pump = Pump::new(Device(io.clone()), permit.clone(), now).unwrap();
        pump.enqueue(
            Frame {
                samples: [0; SAMPLES],
                sequence: 1,
                at_ms: now,
            },
            now,
        )
        .unwrap();
        pump.step(now).unwrap();
        let before = *io.lock().unwrap();
        assert!(client.pulse().await.is_err(), "{case:?}");
        assert_eq!(permit.check(0), Err(Error::Denied), "{case:?}");
        assert_eq!(pump.step(now_ms()), Err(Error::Denied), "{case:?}");
        let after = *io.lock().unwrap();
        assert_eq!(
            (after.read, after.write, after.close),
            (before.read, before.write, 1),
            "{case:?}"
        );
        assert_eq!(count.load(Ordering::SeqCst), 2, "{case:?}");
        if matches!(case, ReplyCase::Timeout) {
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
        } else {
            server.await.unwrap();
        }
    }
}

#[tokio::test]
async fn later_correct_socket_reply_cannot_resurrect_a_revoked_mirror_or_queued_pcm() {
    let (mut client, server, _directory, count) =
        setup(vec![ReplyCase::WrongBoot, ReplyCase::Correct]).await;
    let permit = client.permit();
    let now = now_ms();
    let io = Arc::new(Mutex::new(IoCounts::default()));
    let mut pump = Pump::new(Device(io.clone()), permit.clone(), now).unwrap();
    assert!(client.pulse().await.is_err());
    assert_eq!(pump.step(now_ms()), Err(Error::Denied));
    assert!(client.pulse().await.is_err());
    assert_eq!(permit.check(0), Err(Error::Denied));
    assert_eq!(
        pump.enqueue(
            Frame {
                samples: [0; SAMPLES],
                sequence: 1,
                at_ms: now
            },
            now
        ),
        Err(Error::Denied)
    );
    assert_eq!(
        *io.lock().unwrap(),
        IoCounts {
            close: 1,
            ..IoCounts::default()
        }
    );
    assert_eq!(count.load(Ordering::SeqCst), 3);
    server.await.unwrap();
}

#[tokio::test]
async fn wrong_real_socket_identity_or_parent_permissions_never_submit_a_ticket() {
    use std::os::unix::net::UnixListener;
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.path().join("core.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let credentials = || Credentials {
        ticket: [8; 32],
        peer_key: [7; 32],
        boot: "a".repeat(64),
    };
    assert!(Client::redeem(&path, u32::MAX, credentials())
        .await
        .is_err());
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        Client::redeem(&path, rustix::process::geteuid().as_raw(), credentials())
            .await
            .is_err()
    );
    assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
}
