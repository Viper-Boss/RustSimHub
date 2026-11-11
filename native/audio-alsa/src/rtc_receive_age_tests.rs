// SPDX-License-Identifier: AGPL-3.0-only
//! Actual UDP/DTLS/SRTP + Opus receive path, with deterministic age boundaries.
//! Only the private local monotonic clock is controlled, never authorization.
use super::*;
use rustsimhub_media::{RemoteAuthority, SAMPLES};

struct Pair {
    a: UdpSocket,
    b: UdpSocket,
    sender: Session,
    receiver: Session,
    _sender_authority: RemoteAuthority,
    receiver_authority: RemoteAuthority,
    network_at: Instant,
}
impl Pair {
    fn new() -> Self {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ip = "127.0.0.1".parse().unwrap();
        let (mut sender, offer, pending) = Session::offer(&a, ip).unwrap();
        let (mut receiver, answer) = Session::answer(&b, ip, &offer).unwrap();
        sender.accept_answer(&a, pending, &answer).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while sender.verified_peer().is_none() || receiver.verified_peer().is_none() {
            assert!(Instant::now() < deadline, "real DTLS handshake deadline");
            sender.service(&a, 0).unwrap();
            receiver.service(&b, 0).unwrap();
            // Handshake pacing only; the tested age never depends on this wait.
            std::thread::yield_now();
        }
        fn authorize(session: &mut Session) -> RemoteAuthority {
            let boot = "s".repeat(64);
            let mut authority = RemoteAuthority::new(boot.clone()).unwrap();
            let now = rustsimhub_media::local::now_ms();
            authority.update(&boot, now + 1000, now).unwrap();
            session
                .authorize(authority.permit(), session.expected_peer, now)
                .unwrap();
            authority
        }
        let sender_authority = authorize(&mut sender);
        let receiver_authority = authorize(&mut receiver);
        Self {
            a,
            b,
            sender,
            receiver,
            _sender_authority: sender_authority,
            receiver_authority,
            network_at: Instant::now(),
        }
    }

    fn deliver(
        &mut self,
        sequence: u64,
        before_ms: i64,
        after_ms: i64,
        revoke_after_decode: bool,
    ) -> (Result<(), Error>, usize) {
        self.deliver_payload(sequence, before_ms, after_ms, revoke_after_decode, None)
    }
    fn deliver_payload(
        &mut self,
        sequence: u64,
        before_ms: i64,
        after_ms: i64,
        revoke_after_decode: bool,
        payload: Option<Vec<u8>>,
    ) -> (Result<(), Error>, usize) {
        let now = rustsimhub_media::local::now_ms();
        let frame = Frame {
            samples: std::array::from_fn(|i| if i % 16 < 8 { 1024 } else { -1024 }),
            sequence,
            at_ms: now,
        };
        if let Some(payload) = payload {
            let mid = self.sender.mid.unwrap();
            let writer = self.sender.rtc.writer(mid).unwrap();
            let pt = writer
                .payload_params()
                .find(|p| p.spec().codec == str0m::format::Codec::Opus)
                .unwrap()
                .pt();
            writer
                .write(
                    pt,
                    Instant::now(),
                    MediaTime::new(sequence * u64::from(RTP_STEP), Frequency::FORTY_EIGHT_KHZ),
                    payload,
                )
                .unwrap();
            self.sender.drain(&self.a, Instant::now(), now).unwrap();
        } else {
            self.sender.send(&self.a, &frame, now).unwrap();
        }
        // The real RTP pacer releases the encoded SRTP packet on its timer.
        let tick = self.sender.timeout.max(Instant::now());
        self.sender.rtc.handle_input(Input::Timeout(tick)).unwrap();
        self.sender.drain(&self.a, tick, now).unwrap();
        self.network_at += Duration::from_millis(100);
        let network_at = self.network_at;
        let mut reads = 0;
        let authority = &mut self.receiver_authority;
        let mut clock = || {
            reads += 1;
            if reads == 2 && revoke_after_decode {
                authority.revoke();
            }
            let age = if reads == 1 { before_ms } else { after_ms };
            if age < 0 {
                network_at - Duration::from_millis(age.unsigned_abs())
            } else {
                network_at + Duration::from_millis(age as u64)
            }
        };
        let socket = &self.b;
        let mut transmit = |_: SocketAddr, destination: SocketAddr, bytes: &[u8]| {
            socket
                .send_to(bytes, destination)
                .map(|_| ())
                .map_err(|_| Error::Transport)
        };
        let mut bytes = [0; MAX_DATAGRAM + 1];
        let mut received = false;
        for _ in 0..16 {
            let (length, source) = match socket.recv_from(&mut bytes) {
                Ok(v) => v,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("synthetic UDP receive: {e}"),
            };
            received = true;
            self.receiver
                .rtc
                .handle_input(Input::Receive(
                    network_at,
                    Receive {
                        proto: Protocol::Udp,
                        source,
                        destination: self.receiver.address,
                        contents: (&bytes[..length]).try_into().unwrap(),
                    },
                ))
                .unwrap();
            if let Err(e) = self.receiver.drain_to_with_clock(&mut transmit, &mut clock) {
                return (Err(e), reads);
            }
        }
        assert!(received, "real sender emitted an encrypted datagram");
        // Deterministically release the real audio reorder buffer; no age sleep.
        self.receiver
            .rtc
            .handle_input(Input::Timeout(network_at + Duration::from_millis(45)))
            .unwrap();
        let result = self.receiver.drain_to_with_clock(&mut transmit, &mut clock);
        (result, reads)
    }
}

#[test]
fn decrypted_opus_fresh_at_59ms_keeps_original_frame_time() {
    let mut pair = Pair::new();
    let before = rustsimhub_media::local::now_ms();
    let (result, reads) = pair.deliver(0, 0, 59, false);
    result.unwrap();
    assert_eq!(reads, 2, "actual Opus decode completed");
    assert_eq!(pair.receiver.received_frames, 1);
    assert_eq!(pair.receiver.dropped_frames, 0);
    assert_eq!(
        pair.receiver.last_media_receive,
        Some(pair.network_at + Duration::from_millis(59))
    );
    let frame = pair.receiver.incoming.front().unwrap();
    assert!(frame.at_ms >= before && frame.at_ms <= rustsimhub_media::local::now_ms());
    assert!(frame.samples.iter().any(|s| *s != 0));
}

#[test]
fn decrypted_opus_expiring_after_decode_at_60ms_never_counts_as_received() {
    for before in [0, 59] {
        let mut pair = Pair::new();
        let (result, reads) = pair.deliver(0, before, 60, false);
        result.unwrap();
        assert_eq!(reads, 2, "negative control must complete actual decode");
        assert!(
            pair.receiver.incoming.is_empty(),
            "post-decode stale audio entered queue"
        );
        assert_eq!(pair.receiver.received_frames, 0);
        assert_eq!(pair.receiver.dropped_frames, 1);
        assert!(pair.receiver.last_media_receive.is_none());
        assert!(!pair.receiver.closed);
        let (result, reads) = pair.deliver(1, 0, 0, false);
        result.unwrap();
        assert_eq!(reads, 2);
        assert_eq!(pair.receiver.received_frames, 1);
        assert_eq!(pair.receiver.incoming.len(), 1);
        assert!(
            pair.receiver.receive(0).unwrap().is_some(),
            "later valid speech still delivered"
        );
    }
}

#[test]
fn pre_decode_exact_60ms_is_dropped_before_decoding() {
    let mut pair = Pair::new();
    let (result, reads) = pair.deliver(0, 60, 60, false);
    result.unwrap();
    assert_eq!(reads, 1);
    assert!(pair.receiver.incoming.is_empty());
    assert_eq!(pair.receiver.received_frames, 0);
    assert_eq!(pair.receiver.dropped_frames, 1);
}

#[test]
fn stale_decoded_frame_cannot_evict_existing_fresh_queue() {
    let mut pair = Pair::new();
    let now = rustsimhub_media::local::now_ms();
    for sequence in 100..100 + QUEUE_FRAMES as u64 {
        pair.receiver.incoming.push_back(Frame {
            samples: [1; SAMPLES],
            sequence,
            at_ms: now,
        });
    }
    let expected: Vec<_> = pair.receiver.incoming.iter().map(|f| f.sequence).collect();
    pair.deliver(0, 0, 60, false).0.unwrap();
    assert_eq!(
        pair.receiver
            .incoming
            .iter()
            .map(|f| f.sequence)
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(pair.receiver.received_frames, 0);
    assert_eq!(pair.receiver.dropped_frames, 1);
}

#[test]
fn dropped_decoded_frame_does_not_extend_actual_media_idle_deadline() {
    let mut pair = Pair::new();
    let last = Instant::now() - Duration::from_secs(1);
    pair.receiver.last_media_receive = Some(last);
    pair.deliver(0, 0, 60, false).0.unwrap();
    assert_eq!(pair.receiver.last_media_receive, Some(last));
    assert_eq!(pair.receiver.service(&pair.b, 0), Err(Error::Closed));
}

#[test]
fn revocation_at_decode_boundary_is_not_hidden_by_stale_drop() {
    let mut pair = Pair::new();
    let (result, reads) = pair.deliver(0, 0, 60, true);
    assert_eq!(reads, 2);
    assert_eq!(result, Err(Error::Authorization));
    assert!(pair.receiver.closed && pair.receiver.incoming.is_empty());
    assert_eq!(pair.receiver.received_frames, 0);
    assert_eq!(pair.receiver.dropped_frames, 0);
}

#[test]
fn missing_session_permit_cannot_hide_behind_pre_decode_stale_drop() {
    let mut pair = Pair::new();
    pair.receiver.permit = None;
    assert_eq!(pair.deliver(0, 60, 60, false).0, Err(Error::Authorization));
    assert!(pair.receiver.closed);
    assert_eq!(pair.receiver.dropped_frames, 0);
}

#[test]
fn future_network_time_and_clock_regression_remain_errors() {
    for (before, after) in [(-1, 0), (20, 19), (0, -1)] {
        let mut pair = Pair::new();
        assert_eq!(pair.deliver(0, before, after, false).0, Err(Error::Bounds));
        assert!(pair.receiver.incoming.is_empty());
        assert_eq!(pair.receiver.received_frames, 0);
        assert_eq!(pair.receiver.dropped_frames, 0);
    }
}

#[test]
fn authenticated_wrong_opus_duration_is_an_error_not_a_stale_drop() {
    let mut pair = Pair::new();
    let mut encoder =
        opus::Encoder::new(16_000, opus::Channels::Mono, opus::Application::Voip).unwrap();
    let mut bytes = [0; MAX_PACKET];
    let length = encoder.encode(&[0_i16; SAMPLES / 2], &mut bytes).unwrap();
    let (result, reads) = pair.deliver_payload(0, 0, 60, false, Some(bytes[..length].to_vec()));
    assert_eq!(result, Err(Error::Codec));
    assert_eq!(
        reads, 1,
        "wrong 10ms shape must fail before post-decode stale handling"
    );
    assert!(pair.receiver.incoming.is_empty());
    assert_eq!(pair.receiver.received_frames, 0);
    assert_eq!(pair.receiver.dropped_frames, 0);
}
