// SPDX-License-Identifier: AGPL-3.0-only
//! One audio-only LAN session. SDP is private; never log it or encrypted media.
use crate::codec::{Codec, MAX_PACKET, RTP_STEP};
use rustsimhub_media::{Frame, Permit, MAX_AGE_MS, QUEUE_FRAMES};
use std::{
    collections::VecDeque,
    io::ErrorKind,
    net::{IpAddr, SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};
use str0m::{
    change::SdpOffer,
    media::{Direction, Frequency, MediaKind, MediaTime, Mid},
    net::{Protocol, Receive},
    Candidate, Event, IceConnectionState, Input, Output, Rtc,
};
const MAX_SDP: usize = 16_384;
const MAX_DATAGRAM: usize = 1500;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Sdp,
    Identity,
    Transport,
    Authorization,
    Codec,
    Bounds,
    Closed,
}
fn local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local(),
        _ => false,
    }
}
/// Additional product limits, before calling the library parser. No DNS or TURN yet.
fn validate_sdp(sdp: &str, peer_ip: IpAddr) -> Result<[u8; 32], Error> {
    if !local_ip(peer_ip)
        || sdp.is_empty()
        || sdp.len() > MAX_SDP
        || !sdp.is_ascii()
        || sdp.bytes().any(|b| b < 32 && !matches!(b, b'\r' | b'\n'))
    {
        return Err(Error::Sdp);
    }
    let mut fingerprint = None;
    let mut media = 0;
    let mut candidates = 0;
    let mut opus = false;
    let mut sendrecv = false;
    let mut mux = false;
    for (n, line) in sdp.lines().enumerate() {
        if n >= 160 || line.len() > 1024 {
            return Err(Error::Bounds);
        }
        if let Some(m) = line.strip_prefix("m=") {
            let p: Vec<_> = m.split_ascii_whitespace().collect();
            if p.len() < 4 || p[0] != "audio" || p[1] == "0" || p[2] != "UDP/TLS/RTP/SAVPF" {
                return Err(Error::Sdp);
            }
            media += 1;
        }
        if line == "a=sendrecv" {
            sendrecv = true;
        }
        if line == "a=rtcp-mux" {
            mux = true;
        }
        if line.starts_with("a=rtpmap:") && line.ends_with(" opus/48000/2") {
            opus = true;
        }
        if line.starts_with("a=simulcast:")
            || line.starts_with("a=rid:")
            || line.starts_with("a=crypto:")
        {
            return Err(Error::Sdp);
        }
        if let Some(f) = line.strip_prefix("a=fingerprint:") {
            let f: str0m::crypto::Fingerprint = f.parse().map_err(|_| Error::Sdp)?;
            if f.hash_func != "sha-256" || f.bytes.len() != 32 {
                return Err(Error::Sdp);
            }
            let key: [u8; 32] = f.bytes.as_slice().try_into().map_err(|_| Error::Sdp)?;
            if fingerprint.is_some_and(|old| old != key) {
                return Err(Error::Identity);
            }
            fingerprint = Some(key);
        }
        if let Some(c) = line.strip_prefix("a=candidate:") {
            let c: Vec<_> = c.split_ascii_whitespace().collect();
            candidates += 1;
            if candidates > 8
                || c.len() < 8
                || c[1] != "1"
                || !c[2].eq_ignore_ascii_case("udp")
                || c[4].parse::<IpAddr>().ok() != Some(peer_ip)
                || c[5].parse::<u16>().ok().filter(|p| *p >= 1024).is_none()
                || c[6] != "typ"
                || c[7] != "host"
            {
                return Err(Error::Sdp);
            }
        }
    }
    if media != 1 || candidates == 0 || !opus || !sendrecv || !mux {
        return Err(Error::Sdp);
    }
    fingerprint.ok_or(Error::Identity)
}
fn rtc(socket: &UdpSocket) -> Result<Rtc, Error> {
    let address = socket.local_addr().map_err(|_| Error::Transport)?;
    if !local_ip(address.ip()) || address.port() < 1024 {
        return Err(Error::Transport);
    }
    socket.set_nonblocking(true).map_err(|_| Error::Transport)?;
    let mut rtc = Rtc::builder()
        .set_crypto_provider(Arc::new(str0m::crypto::from_feature_flags()))
        .set_fingerprint_verification(true)
        .clear_codecs()
        .enable_opus(true, false)
        .set_reordering_size_audio(3)
        .set_reordering_timeout_audio(Some(Duration::from_millis(40)))
        .set_send_buffer_audio(4)
        .set_sctp_max_buffered_amount(0)
        .set_stats_interval(None)
        .set_mtu(1100..=1200)
        .build(Instant::now());
    rtc.add_local_candidate(Candidate::host(address, "udp").map_err(|_| Error::Transport)?)
        .ok_or(Error::Transport)?;
    Ok(rtc)
}
// No Debug or access to the underlying Rtc: callers cannot disable fingerprint verification.
pub struct Session {
    rtc: Rtc,
    address: SocketAddr,
    peer_ip: IpAddr,
    expected_peer: [u8; 32],
    connected: bool,
    closed: bool,
    mid: Option<Mid>,
    codec: Option<Codec>,
    permit: Option<Permit>,
    authorized_at: Option<Instant>,
    last_media_receive: Option<Instant>,
    incoming: VecDeque<Frame>,
    timeout: Instant,
    created: Instant,
    pub sent_frames: u64,
    pub received_frames: u64,
    pub dropped_frames: u64,
}
impl Session {
    pub fn offer(
        socket: &UdpSocket,
        peer_ip: IpAddr,
    ) -> Result<(Self, String, str0m::change::SdpPendingOffer), Error> {
        if !local_ip(peer_ip) {
            return Err(Error::Transport);
        }
        let mut rtc = rtc(socket)?;
        let mut changes = rtc.sdp_api();
        let mid = changes.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
        let (offer, pending) = changes.apply().ok_or(Error::Sdp)?;
        let mut session = Self::from_rtc(rtc, socket, peer_ip, [0; 32])?;
        session.mid = Some(mid);
        session.drain(socket, Instant::now(), rustsimhub_media::local::now_ms())?;
        Ok((session, offer.to_sdp_string(), pending))
    }
    pub fn accept_answer(
        &mut self,
        socket: &UdpSocket,
        pending: str0m::change::SdpPendingOffer,
        sdp: &str,
    ) -> Result<(), Error> {
        if self.connected || self.closed {
            return Err(Error::Closed);
        }
        self.expected_peer = validate_sdp(sdp, self.peer_ip)?;
        self.rtc
            .sdp_api()
            .accept_answer(
                pending,
                str0m::change::SdpAnswer::from_sdp_string(sdp).map_err(|_| Error::Sdp)?,
            )
            .map_err(|_| Error::Sdp)?;
        self.drain(socket, Instant::now(), rustsimhub_media::local::now_ms())
    }
    pub fn answer(socket: &UdpSocket, peer_ip: IpAddr, sdp: &str) -> Result<(Self, String), Error> {
        let expected_peer = validate_sdp(sdp, peer_ip)?;
        let offer = SdpOffer::from_sdp_string(sdp).map_err(|_| Error::Sdp)?;
        let mut rtc = rtc(socket)?;
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|_| Error::Sdp)?
            .to_sdp_string();
        let mut session = Self::from_rtc(rtc, socket, peer_ip, expected_peer)?;
        session.drain(socket, Instant::now(), rustsimhub_media::local::now_ms())?;
        Ok((session, answer))
    }
    fn from_rtc(
        rtc: Rtc,
        socket: &UdpSocket,
        peer_ip: IpAddr,
        expected_peer: [u8; 32],
    ) -> Result<Self, Error> {
        Ok(Self {
            rtc,
            address: socket.local_addr().map_err(|_| Error::Transport)?,
            peer_ip,
            expected_peer,
            connected: false,
            closed: false,
            mid: None,
            codec: None,
            permit: None,
            authorized_at: None,
            last_media_receive: None,
            incoming: VecDeque::with_capacity(QUEUE_FRAMES),
            timeout: Instant::now(),
            created: Instant::now(),
            sent_frames: 0,
            received_frames: 0,
            dropped_frames: 0,
        })
    }
    pub fn verified_peer(&self) -> Option<[u8; 32]> {
        (self.connected && !self.closed).then_some(self.expected_peer)
    }
    /// grant_peer MUST be read from the core ticket, never a client assertion.
    /// Handshake happens first; issue/redeem the short-lived ticket after verification.
    pub fn authorize(
        &mut self,
        permit: Permit,
        grant_peer: [u8; 32],
        _now: u64,
    ) -> Result<(), Error> {
        if self.verified_peer() != Some(grant_peer) || self.codec.is_some() {
            return Err(Error::Identity);
        }
        let codec = Codec::new(permit.clone(), rustsimhub_media::local::now_ms())
            .map_err(|_| Error::Authorization)?;
        self.permit = Some(permit);
        self.codec = Some(codec);
        self.authorized_at = Some(Instant::now());
        Ok(())
    }
    fn gate(&mut self, _now: u64) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        if self
            .permit
            .as_ref()
            .is_some_and(|p| p.check(rustsimhub_media::local::now_ms()).is_err())
        {
            self.close();
            return Err(Error::Authorization);
        }
        Ok(())
    }
    fn drain(&mut self, socket: &UdpSocket, _instant: Instant, _now: u64) -> Result<(), Error> {
        // Every mutation is followed by a full drain to Timeout. Hard cap prevents hostile spin.
        for _ in 0..128 {
            let now = rustsimhub_media::local::now_ms();
            match self.rtc.poll_output().map_err(|_| Error::Transport)? {
                Output::Timeout(t) => {
                    self.timeout = t;
                    return Ok(());
                }
                Output::Transmit(t) => {
                    if t.proto != Protocol::Udp
                        || t.source != self.address
                        || t.destination.ip() != self.peer_ip
                        || t.destination.port() < 1024
                        || t.contents.len() > MAX_DATAGRAM
                    {
                        return Err(Error::Bounds);
                    }
                    self.gate(now)?;
                    // Pending ICE/DTLS may transmit before authorization; no writer can send speech.
                    match socket.send_to(&t.contents, t.destination) {
                        Ok(n) if n == t.contents.len() => {}
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                        _ => return Err(Error::Transport),
                    }
                }
                Output::Event(event) => match event {
                    Event::Connected => {
                        let actual = self
                            .rtc
                            .direct_api()
                            .remote_dtls_fingerprint()
                            .cloned()
                            .ok_or(Error::Identity)?;
                        if actual.hash_func != "sha-256"
                            || actual.bytes.as_slice() != self.expected_peer
                        {
                            self.close();
                            return Err(Error::Identity);
                        }
                        self.connected = true;
                    }
                    Event::MediaAdded(m) => {
                        if m.kind != MediaKind::Audio
                            || m.direction != Direction::SendRecv
                            || m.simulcast.is_some()
                            || self.mid.is_some_and(|old| old != m.mid)
                        {
                            return Err(Error::Sdp);
                        }
                        self.mid = Some(m.mid);
                    }
                    Event::MediaData(m) => {
                        if !self.connected || self.codec.is_none() {
                            continue;
                        }
                        self.gate(now)?;
                        if Some(m.mid) != self.mid
                            || m.rid.is_some()
                            || m.params.spec().codec != str0m::format::Codec::Opus
                            || m.time.denom() != 48_000
                            || m.data.len() > MAX_PACKET
                        {
                            return Err(Error::Codec);
                        }
                        let age = Instant::now()
                            .checked_duration_since(m.network_time)
                            .ok_or(Error::Bounds)?
                            .as_millis();
                        if age >= u128::from(MAX_AGE_MS) {
                            self.dropped_frames += 1;
                            continue;
                        }
                        let seq = **m.seq_range.end();
                        let mut frame = self
                            .codec
                            .as_mut()
                            .ok_or(Error::Authorization)?
                            .decode(&m.data, seq, now.saturating_sub(age as u64), now)
                            .map_err(|_| Error::Codec)?;
                        if self.gate(rustsimhub_media::local::now_ms()).is_err() {
                            frame.samples.fill(0);
                            return Err(Error::Authorization);
                        }
                        if self.incoming.len() == QUEUE_FRAMES {
                            self.incoming.pop_front();
                            self.dropped_frames += 1;
                        }
                        self.incoming.push_back(frame);
                        self.last_media_receive = Some(Instant::now());
                        self.received_frames += 1;
                    }
                    Event::IceConnectionStateChange(IceConnectionState::Disconnected)
                    | Event::Closed => {
                        self.close();
                        return Err(Error::Closed);
                    }
                    Event::ChannelOpen(..) | Event::ChannelData(..) | Event::ChannelClose(..) => {
                        self.close();
                        return Err(Error::Sdp);
                    }
                    _ => {}
                },
            }
        }
        Err(Error::Bounds)
    }
    pub fn service(&mut self, socket: &UdpSocket, now: u64) -> Result<(), Error> {
        self.gate(now)?;
        if self.codec.is_some()
            && self
                .last_media_receive
                .or(self.authorized_at)
                .is_some_and(|at| at.elapsed() >= Duration::from_secs(1))
        {
            eprintln!(
                "rtc_closed reason=media_idle sent={} received={}",
                self.sent_frames, self.received_frames
            );
            self.close();
            return Err(Error::Closed);
        }
        if !self.connected && self.created.elapsed() > Duration::from_secs(15) {
            self.close();
            return Err(Error::Closed);
        }
        let mut buffer = [0u8; MAX_DATAGRAM + 1];
        for _ in 0..8 {
            match socket.recv_from(&mut buffer) {
                Ok((n, source)) => {
                    if n > MAX_DATAGRAM || source.ip() != self.peer_ip || source.port() < 1024 {
                        continue;
                    }
                    let instant = Instant::now();
                    let Ok(contents) = (&buffer[..n]).try_into() else {
                        continue;
                    };
                    self.rtc
                        .handle_input(Input::Receive(
                            instant,
                            Receive {
                                proto: Protocol::Udp,
                                source,
                                destination: self.address,
                                contents,
                            },
                        ))
                        .map_err(|_| Error::Transport)?;
                    self.drain(socket, instant, now)?;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => return Err(Error::Transport),
            }
        }
        let instant = Instant::now();
        if instant >= self.timeout {
            self.rtc
                .handle_input(Input::Timeout(instant))
                .map_err(|_| Error::Transport)?;
            self.drain(socket, instant, now)?;
        }
        Ok(())
    }
    pub fn send(&mut self, socket: &UdpSocket, frame: &Frame, now: u64) -> Result<(), Error> {
        self.gate(now)?;
        if !self.connected {
            return Err(Error::Identity);
        }
        let packet = self
            .codec
            .as_mut()
            .ok_or(Error::Authorization)?
            .encode(frame, now)
            .map_err(|_| Error::Codec)?;
        let actual_now = rustsimhub_media::local::now_ms();
        self.gate(actual_now)?;
        if frame.at_ms > actual_now || actual_now - frame.at_ms >= MAX_AGE_MS {
            return Err(Error::Bounds);
        }
        let mid = self.mid.ok_or(Error::Sdp)?;
        let writer = self.rtc.writer(mid).ok_or(Error::Sdp)?;
        let pt = writer
            .payload_params()
            .find(|p| p.spec().codec == str0m::format::Codec::Opus)
            .map(|p| p.pt())
            .ok_or(Error::Codec)?;
        let timestamp = frame
            .sequence
            .checked_mul(u64::from(RTP_STEP))
            .ok_or(Error::Bounds)?;
        let instant = Instant::now();
        writer
            .write(
                pt,
                instant,
                MediaTime::new(timestamp, Frequency::FORTY_EIGHT_KHZ),
                packet.bytes().to_vec(),
            )
            .map_err(|_| Error::Transport)?;
        self.sent_frames += 1;
        self.drain(socket, instant, now)
    }
    pub fn receive(&mut self, _now: u64) -> Result<Option<Frame>, Error> {
        let now = rustsimhub_media::local::now_ms();
        self.gate(now)?;
        while let Some(frame) = self.incoming.pop_front() {
            if frame.at_ms <= now && now - frame.at_ms < MAX_AGE_MS {
                return Ok(Some(frame));
            }
            self.dropped_frames += 1;
        }
        Ok(None)
    }
    pub fn close(&mut self) {
        self.closed = true;
        self.connected = false;
        self.incoming.clear();
        if let Some(mut codec) = self.codec.take() {
            codec.close();
        }
        self.permit = None;
        self.authorized_at = None;
        self.last_media_receive = None;
        self.rtc.disconnect();
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
/// Real loopback UDP, ICE, DTLS/SRTP and two Opus directions; synthetic frames only.
pub fn self_check() -> Result<serde_json::Value, Error> {
    let a = UdpSocket::bind("127.0.0.1:0").map_err(|_| Error::Transport)?;
    let b = UdpSocket::bind("127.0.0.1:0").map_err(|_| Error::Transport)?;
    let mut sender = rtc(&a)?;
    let mut change = sender.sdp_api();
    let mid = change.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
    let (offer, pending) = change.apply().ok_or(Error::Sdp)?;
    let sdp = offer.to_sdp_string();
    let (mut receiver, answer) =
        Session::answer(&b, a.local_addr().map_err(|_| Error::Transport)?.ip(), &sdp)?;
    let expected = validate_sdp(&answer, b.local_addr().map_err(|_| Error::Transport)?.ip())?;
    sender
        .sdp_api()
        .accept_answer(
            pending,
            str0m::change::SdpAnswer::from_sdp_string(&answer).map_err(|_| Error::Sdp)?,
        )
        .map_err(|_| Error::Sdp)?;
    let mut sender = Session::from_rtc(
        sender,
        &a,
        b.local_addr().map_err(|_| Error::Transport)?.ip(),
        expected,
    )?;
    sender.mid = Some(mid);
    sender.drain(&a, Instant::now(), 0)?;
    let started = Instant::now();
    while sender.verified_peer().is_none() || receiver.verified_peer().is_none() {
        if started.elapsed() > Duration::from_secs(5) {
            return Err(Error::Transport);
        }
        sender.service(&a, 0)?;
        receiver.service(&b, 0)?;
        std::thread::sleep(Duration::from_millis(1));
    }
    let boot = "s".repeat(64);
    let mut authority_a =
        rustsimhub_media::RemoteAuthority::new(boot.clone()).map_err(|_| Error::Authorization)?;
    let mut authority_b =
        rustsimhub_media::RemoteAuthority::new(boot.clone()).map_err(|_| Error::Authorization)?;
    let now = rustsimhub_media::local::now_ms();
    authority_a
        .update(&boot, now + 1000, now)
        .map_err(|_| Error::Authorization)?;
    authority_b
        .update(&boot, now + 1000, now)
        .map_err(|_| Error::Authorization)?;
    let permit_a = authority_a.permit();
    let permit_b = authority_b.permit();
    // A substituted ticket identity cannot unlock codec/PCM.
    if sender.authorize(permit_a.clone(), [0; 32], 0) != Err(Error::Identity) {
        return Err(Error::Identity);
    }
    sender.authorize(permit_a, sender.expected_peer, 0)?;
    receiver.authorize(permit_b, receiver.expected_peer, 0)?;
    let mut peak_a = 0u32;
    let mut peak_b = 0u32;
    for sequence in 0..50 {
        let now = rustsimhub_media::local::now_ms();
        authority_a
            .update(&boot, now + 1000, now)
            .map_err(|_| Error::Authorization)?;
        authority_b
            .update(&boot, now + 1000, now)
            .map_err(|_| Error::Authorization)?;
        let frame = Frame {
            samples: std::array::from_fn(|i| if i % 16 < 8 { 1024 } else { -1024 }),
            sequence,
            at_ms: now,
        };
        sender.send(&a, &frame, now)?;
        receiver.send(&b, &frame, now)?;
        for _ in 0..10 {
            sender.service(&a, 0)?;
            receiver.service(&b, 0)?;
            std::thread::sleep(Duration::from_millis(2));
        }
        while let Some(f) = sender.receive(0)? {
            peak_a = peak_a.max(
                f.samples
                    .iter()
                    .map(|s| i32::from(*s).unsigned_abs())
                    .max()
                    .unwrap_or(0),
            );
        }
        while let Some(f) = receiver.receive(0)? {
            peak_b = peak_b.max(
                f.samples
                    .iter()
                    .map(|s| i32::from(*s).unsigned_abs())
                    .max()
                    .unwrap_or(0),
            );
        }
    }
    let received_a = sender.received_frames;
    let received_b = receiver.received_frames;
    // Keep the authorization mirrors live while the authenticated peer stops speech packets.
    let timeout_end = Instant::now() + Duration::from_secs(2);
    let media_timeout_closed = loop {
        let now = rustsimhub_media::local::now_ms();
        authority_a
            .update(&boot, now + 1000, now)
            .map_err(|_| Error::Authorization)?;
        authority_b
            .update(&boot, now + 1000, now)
            .map_err(|_| Error::Authorization)?;
        match sender.service(&a, now) {
            Err(Error::Closed) => break true,
            Err(error) => return Err(error),
            Ok(()) => {}
        }
        if Instant::now() >= timeout_end {
            break false;
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    authority_a.revoke();
    authority_b.revoke();
    if !media_timeout_closed
        || receiver.service(&b, 0) != Err(Error::Authorization)
        || sender.verified_peer().is_some()
        || receiver.verified_peer().is_some()
    {
        return Err(Error::Authorization);
    }
    if received_a < 45 || received_b < 45 || peak_a == 0 || peak_b == 0 {
        return Err(Error::Codec);
    }
    Ok(
        serde_json::json!({"synthetic_only":true,"pcm_opened":false,"udp_loopback":true,"dtls_fingerprint_verified":true,"srtp":true,"sent_each":50,"received_a":received_a,"received_b":received_b,"decoded_peak_a":peak_a,"decoded_peak_b":peak_b,"wrong_grant_denied":true,"revocation_closed":true,"media_timeout_closed":media_timeout_closed,"elapsed_ms":started.elapsed().as_millis()}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_loopback_roundtrip_and_ticket_identity_revocation() {
        self_check().unwrap();
    }
    #[test]
    fn substituted_sdp_certificate_cannot_complete_dtls_or_unlock_media() {
        let a = UdpSocket::bind("127.0.0.1:0").unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ip = "127.0.0.1".parse().unwrap();
        let (mut sender, offer, pending) = Session::offer(&a, ip).unwrap();
        let changed = offer
            .lines()
            .map(|line| {
                if line.starts_with("a=fingerprint:") {
                    format!("a=fingerprint:sha-256 {}", ["00"; 32].join(":"))
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\r\n")
            + "\r\n";
        let (mut receiver, answer) = Session::answer(&b, ip, &changed).unwrap();
        sender.accept_answer(&a, pending, &answer).unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        let mut denied = false;
        while Instant::now() < end {
            if receiver
                .service(&b, rustsimhub_media::local::now_ms())
                .is_err()
            {
                denied = true;
                break;
            }
            if sender
                .service(&a, rustsimhub_media::local::now_ms())
                .is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(denied);
        assert!(receiver.verified_peer().is_none());
        assert!(receiver.codec.is_none());
        assert_eq!(receiver.received_frames, 0);
    }
    #[test]
    fn bounded_audio_only_sdp_rejects_unsafe_candidates_and_extra_media() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut r = rtc(&socket).unwrap();
        let mut c = r.sdp_api();
        c.add_media(MediaKind::Audio, Direction::SendRecv, None, None, None);
        let (s, _) = c.apply().unwrap();
        let s = s.to_sdp_string();
        let ip = "127.0.0.1".parse().unwrap();
        assert!(validate_sdp(&s, ip).is_ok());
        for bad in [
            s.replace("127.0.0.1", "8.8.8.8"),
            s.replace("m=audio", "m=video"),
            format!("{s}m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n"),
            s.replace("sha-256", "sha-1"),
            s.replace("a=rtcp-mux", "a=rtcp"),
            "a".repeat(MAX_SDP + 1),
        ] {
            assert!(validate_sdp(&bad, ip).is_err());
        }
    }
}
