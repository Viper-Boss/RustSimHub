// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::{
    future::{poll_fn, Future},
    pin::Pin,
    task::Poll,
};
use tokio::{io::AsyncWriteExt, time::timeout};

fn wire(kind: Kind, payload: &[u8]) -> Vec<u8> {
    let mut bytes = ((payload.len() + 2) as u32).to_be_bytes().to_vec();
    bytes.extend([VERSION, kind as u8]);
    bytes.extend(payload);
    bytes
}
#[tokio::test]
async fn actual_unix_pairs_round_trip_boundaries_and_keep_frames_separate() {
    for size in [0, 1, 19, 244, 1024, 2048] {
        let (a, b) = UnixStream::pair().unwrap();
        let mut writer = FramedIo::new(a);
        let mut reader = FramedIo::new(b);
        let first = Packet::new(Kind::Handshake, vec![0x5a; size]).unwrap();
        let second = Packet::new(Kind::EncryptedRequest, vec![0xa5; size]).unwrap();
        let send = async {
            writer.write(&first).await.unwrap();
            writer.write(&second).await.unwrap();
        };
        let recv = async {
            let one = reader.read().await.unwrap();
            assert_eq!(one.kind(), Kind::Handshake);
            assert_eq!(one.payload(), first.payload());
            let two = reader.read().await.unwrap();
            assert_eq!(two.kind(), Kind::EncryptedRequest);
            assert_eq!(two.payload(), second.payload());
            assert!(!reader.is_poisoned());
        };
        tokio::join!(send, recv);
        assert!(!writer.is_poisoned());
    }
    assert!(matches!(
        Packet::new(Kind::Handshake, vec![0; 2049]),
        Err(Error::InvalidFrame)
    ));
}
#[tokio::test]
async fn invalid_lengths_are_rejected_from_header_without_waiting_for_any_body() {
    for length in [0u32, 1, 2051, u32::MAX] {
        let (mut writer, stream) = UnixStream::pair().unwrap();
        writer.write_all(&length.to_be_bytes()).await.unwrap();
        let mut io = FramedIo::new(stream);
        assert!(matches!(
            timeout(Duration::from_secs(1), io.read()).await.unwrap(),
            Err(Error::InvalidFrame)
        ));
        assert!(io.is_poisoned());
        assert!(matches!(io.read().await, Err(Error::Closed)));
    }
}
#[tokio::test]
async fn unknown_version_kind_and_close_payload_fail_before_payload_read() {
    for (version, kind, length) in [
        (0, 1, 2050u32),
        (2, 1, 2050),
        (1, 0, 2050),
        (1, 4, 2050),
        (1, 255, 2050),
        (1, 3, 3),
    ] {
        let (mut writer, stream) = UnixStream::pair().unwrap();
        writer.write_all(&length.to_be_bytes()).await.unwrap();
        writer.write_all(&[version, kind]).await.unwrap();
        let mut io = FramedIo::new(stream);
        assert!(matches!(
            timeout(Duration::from_secs(1), io.read()).await.unwrap(),
            Err(Error::InvalidFrame)
        ));
        assert!(io.is_poisoned());
    }
    assert!(matches!(
        Packet::new(Kind::CloseTransport, vec![1]),
        Err(Error::InvalidFrame)
    ));
}
#[tokio::test]
async fn every_truncation_fences_stream_instead_of_resynchronizing_into_a_new_request() {
    let complete = wire(Kind::EncryptedRequest, b"synthetic");
    for end in 0..complete.len() {
        let (mut writer, stream) = UnixStream::pair().unwrap();
        writer.write_all(&complete[..end]).await.unwrap();
        writer.shutdown().await.unwrap();
        let mut io = FramedIo::new(stream);
        assert!(matches!(io.read().await, Err(Error::Io)));
        assert!(io.is_poisoned());
        let next = Packet::new(Kind::Handshake, vec![]).unwrap();
        assert_eq!(io.write(&next).await, Err(Error::Closed));
    }
}
#[tokio::test]
async fn arbitrary_socket_fragmentation_still_consumes_exactly_one_bounded_frame() {
    let data = vec![0x79; MAX_ENVELOPE];
    let bytes = wire(Kind::EncryptedRequest, &data);
    let (mut writer, stream) = UnixStream::pair().unwrap();
    let mut io = FramedIo::new(stream);
    let send = async {
        for part in bytes.chunks(7) {
            writer.write_all(part).await.unwrap();
            tokio::task::yield_now().await;
        }
    };
    let receive = async {
        let p = io.read().await.unwrap();
        assert_eq!(p.payload(), data);
    };
    tokio::join!(send, receive);
}
#[tokio::test]
async fn read_deadline_poison_is_permanent_and_never_silently_retries() {
    let (mut writer, stream) = UnixStream::pair().unwrap();
    writer.write_all(&[0, 0, 0, 12, VERSION, 2]).await.unwrap();
    let mut io = FramedIo::new(stream);
    assert!(matches!(
        io.read_until(Instant::now() + Duration::from_millis(25))
            .await,
        Err(Error::Deadline)
    ));
    writer.write_all(b"late bytes").await.unwrap();
    assert!(matches!(io.read().await, Err(Error::Closed)));
}
#[tokio::test]
async fn unpolled_read_and_write_futures_have_no_side_effects() {
    let (stream, peer) = UnixStream::pair().unwrap();
    let mut io = FramedIo::new(stream);
    drop(io.read());
    let packet = Packet::new(Kind::Handshake, b"synthetic".to_vec()).unwrap();
    drop(io.write(&packet));
    assert!(!io.is_poisoned());
    let mut byte = [0; 1];
    assert_eq!(
        peer.try_read(&mut byte).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    let mut remote = FramedIo::new(peer);
    let (sent, received) = tokio::join!(io.write(&packet), remote.read());
    sent.unwrap();
    assert_eq!(received.unwrap().payload(), packet.payload());
}
#[tokio::test]
async fn cancelled_polled_partial_read_cannot_accept_late_bytes() {
    let (mut writer, stream) = UnixStream::pair().unwrap();
    writer.write_all(&[0, 0]).await.unwrap();
    let mut io = FramedIo::new(stream);
    let mut future = Box::pin(io.read());
    poll_fn(|cx| {
        assert!(matches!(Pin::as_mut(&mut future).poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    drop(future);
    assert!(io.is_poisoned());
    writer.write_all(&[0, 2, VERSION, 3]).await.unwrap();
    assert!(matches!(io.read().await, Err(Error::Closed)));
}
#[tokio::test]
async fn cancelled_blocked_write_is_unknown_and_connection_is_not_reusable() {
    let (stream, _peer) = UnixStream::pair().unwrap();
    stream.writable().await.unwrap();
    let mut blocked = false;
    for _ in 0..1024 {
        match stream.try_write(&[0; 4096]) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                blocked = true;
                break;
            }
            Err(e) => panic!("synthetic socket fill: {e}"),
        }
    }
    assert!(blocked, "bounded synthetic send buffer did not fill");
    let mut io = FramedIo::new(stream);
    let packet = Packet::new(Kind::EncryptedRequest, vec![3; 2048]).unwrap();
    let mut future = Box::pin(io.write(&packet));
    poll_fn(|cx| {
        assert!(matches!(Pin::as_mut(&mut future).poll(cx), Poll::Pending));
        Poll::Ready(())
    })
    .await;
    drop(future);
    assert!(io.is_poisoned());
    assert_eq!(io.write(&packet).await, Err(Error::Closed));
}
#[tokio::test]
async fn write_deadline_and_peer_loss_keep_connection_fenced() {
    let (stream, peer) = UnixStream::pair().unwrap();
    drop(peer);
    let mut io = FramedIo::new(stream);
    let packet = Packet::new(Kind::Handshake, vec![1]).unwrap();
    assert_eq!(io.write(&packet).await, Err(Error::Io));
    assert!(io.is_poisoned());
    let (stream, _peer) = UnixStream::pair().unwrap();
    let mut io = FramedIo::new(stream);
    let past = Instant::now() - Duration::from_millis(1);
    assert!(matches!(
        io.write_until(&packet, past).await,
        Err(Error::Deadline)
    ));
    assert!(io.is_poisoned());
}
#[tokio::test]
async fn an_expired_read_deadline_rejects_even_an_entire_ready_frame() {
    let (mut sender, stream) = UnixStream::pair().unwrap();
    sender
        .write_all(&wire(Kind::Handshake, b"synthetic ready frame"))
        .await
        .unwrap();
    let mut io = FramedIo::new(stream);
    assert!(matches!(
        io.read_until(Instant::now() - Duration::from_millis(1))
            .await,
        Err(Error::Deadline)
    ));
    assert!(io.is_poisoned());
}
#[tokio::test]
async fn close_transport_is_terminal_on_both_sides() {
    let (a, b) = UnixStream::pair().unwrap();
    let mut a = FramedIo::new(a);
    let mut b = FramedIo::new(b);
    let close = Packet::new(Kind::CloseTransport, vec![]).unwrap();
    let (sent, received) = tokio::join!(a.write(&close), b.read());
    sent.unwrap();
    assert_eq!(received.unwrap().kind(), Kind::CloseTransport);
    assert!(a.is_poisoned());
    assert!(b.is_poisoned());
    assert!(matches!(b.read().await, Err(Error::Closed)));
}
#[test]
fn production_role_construction_rejects_root_core_and_all_aliases() {
    let core = rustix::process::geteuid().as_raw();
    let ids = [core + 1, core + 2, core + 3, core + 4];
    assert!(Roles::new(ids[0], ids[1], ids[2], ids[3]).is_ok());
    for slot in 0..4 {
        for forbidden in [0, core, u32::MAX] {
            let mut bad = ids;
            bad[slot] = forbidden;
            assert!(Roles::new(bad[0], bad[1], bad[2], bad[3]).is_err());
        }
        for other in 0..4 {
            if other != slot {
                let mut bad = ids;
                bad[slot] = ids[other];
                assert!(Roles::new(bad[0], bad[1], bad[2], bad[3]).is_err());
            }
        }
    }
}
#[tokio::test]
async fn actual_kernel_peer_is_rejected_before_any_rogue_frame_is_decoded() {
    let core = rustix::process::geteuid().as_raw();
    let roles = Roles::new(core + 1, core + 2, core + 3, core + 4).unwrap();
    let guard = SourceGuard::new(roles).unwrap();
    let (mut sender, receiver) = UnixStream::pair().unwrap();
    sender.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
    assert!(matches!(guard.accept(receiver), Err(Error::Denied)));
    // No pretend positive peer test: a distinct approved BLE UID needs an isolated Root/NSS fixture.
}
