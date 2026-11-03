// SPDX-License-Identifier: AGPL-3.0-only
//! One forced-relay RTC session. Reuses the existing RTC/Opus/Permit engine;
//! transport facts are never normal business or call-control authorization.
pub use crate::turn::{Error as TurnError, Registration};
use crate::{rtc, turn::Driver};
use rustsimhub_media::{local::now_ms, Frame, Permit, MAX_AGE_MS};
use std::{collections::VecDeque, net::SocketAddr, time::Duration};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Turn(TurnError),
    Rtc(rtc::Error),
    Unverified,
    Bounds,
    Closed,
}
impl From<TurnError> for Error {
    fn from(e: TurnError) -> Self {
        Self::Turn(e)
    }
}
impl From<rtc::Error> for Error {
    fn from(e: rtc::Error) -> Self {
        Self::Rtc(e)
    }
}
/// Process-local observed path/DTLS fact. No Deserialize, public constructor or
/// execution capability; a current Core Permit is independently mandatory.
pub struct VerifiedRemotePeer {
    fingerprint: [u8; 32],
    local: SocketAddr,
    peer: SocketAddr,
}
impl VerifiedRemotePeer {
    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }
    pub fn selected_path(&self) -> (SocketAddr, SocketAddr) {
        (self.local, self.peer)
    }
}
struct Packet {
    data: Vec<u8>,
    at: u64,
}
impl Drop for Packet {
    fn drop(&mut self) {
        self.data.fill(0);
    }
}
/// No underlying RTC/socket/driver access. No PCM open, Core ticket issuer,
/// identity store, normal lease renewal or LAN fallback is implemented here.
pub struct Session {
    rtc: rtc::Session,
    relay: Option<Driver>,
    pending: Option<str0m::change::SdpPendingOffer>,
    verified: Option<VerifiedRemotePeer>,
    outgoing: VecDeque<Packet>,
    closed: bool,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.fence();
    }
}
impl Session {
    pub async fn offer(registration: Registration) -> Result<(Self, String), Error> {
        let relay = Driver::allocate(registration).await?;
        let (rtc, sdp, pending) =
            rtc::Session::relay_offer(relay.allocation().address, relay.base())?;
        Ok((
            Self {
                rtc,
                relay: Some(relay),
                pending: Some(pending),
                verified: None,
                outgoing: VecDeque::new(),
                closed: false,
            },
            sdp,
        ))
    }
    /// expected_peer is supplied by the normal authenticated signaling host,
    /// never recovered from an unauthenticated SDP and treated as a pin itself.
    pub async fn answer(
        registration: Registration,
        sdp: &str,
        expected_peer: [u8; 32],
    ) -> Result<(Self, String), Error> {
        let peer = rtc::relay_signaling(sdp, expected_peer)?;
        let mut relay = Driver::allocate(registration).await?;
        relay.bind_peer(peer).await?;
        let (rtc, answer) = rtc::Session::relay_answer(
            relay.allocation().address,
            relay.base(),
            peer,
            expected_peer,
            sdp,
        )?;
        Ok((
            Self {
                rtc,
                relay: Some(relay),
                pending: None,
                verified: None,
                outgoing: VecDeque::new(),
                closed: false,
            },
            answer,
        ))
    }
    pub async fn accept_answer(&mut self, sdp: &str, expected_peer: [u8; 32]) -> Result<(), Error> {
        let result = self.accept_inner(sdp, expected_peer).await;
        if result.is_err() {
            self.fence();
        }
        result
    }
    async fn accept_inner(&mut self, sdp: &str, expected_peer: [u8; 32]) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Closed);
        }
        let pending = self.pending.take().ok_or(Error::Closed)?;
        let peer = self.rtc.relay_accept(pending, sdp, expected_peer)?;
        self.relay
            .as_mut()
            .ok_or(Error::Closed)?
            .bind_peer(peer)
            .await?;
        Ok(())
    }
    fn current_path(&self) -> Result<(SocketAddr, SocketAddr), Error> {
        if self.closed || self.pending.is_some() {
            return Err(Error::Closed);
        }
        let relay = self.relay.as_ref().ok_or(Error::Closed)?;
        relay.ready()?;
        Ok((
            relay.allocation().address,
            relay.peer().ok_or(Error::Unverified)?,
        ))
    }
    /// A positive transport snapshot only while the original actual allocation,
    /// selected candidate pair and actual peer DTLS certificate still match.
    pub fn verified_peer(&self) -> Option<&VerifiedRemotePeer> {
        let path = self.current_path().ok()?;
        let key = self.rtc.verified_peer()?;
        let verified = self.verified.as_ref()?;
        (self.rtc.selected_pair == Some(path)
            && path == (verified.local, verified.peer)
            && key == verified.fingerprint)
            .then_some(verified)
    }
    pub fn authorize(&mut self, permit: Permit, core_peer: [u8; 32]) -> Result<(), Error> {
        if self.verified_peer().map(VerifiedRemotePeer::fingerprint) != Some(core_peer) {
            self.fence();
            return Err(Error::Unverified);
        }
        if let Err(e) = self.rtc.authorize(permit, core_peer, now_ms()) {
            self.fence();
            return Err(e.into());
        }
        Ok(())
    }
    fn fence(&mut self) {
        self.closed = true;
        self.verified = None;
        self.outgoing.clear();
        self.rtc.close();
        if let Some(relay) = &self.relay {
            relay.fence();
        }
    }
    async fn flush(&mut self, captured_at: Option<u64>) -> Result<(), Error> {
        let (local, peer) = self.current_path()?;
        let queue = &mut self.outgoing;
        self.rtc.drain_to(|from, to, bytes| {
            if from != local
                || to != peer
                || bytes.len() > crate::turn::DATAGRAM_BYTES
                || queue.len() >= 8
            {
                return Err(rtc::Error::Bounds);
            }
            queue.push_back(Packet {
                data: bytes.to_vec(),
                at: captured_at.unwrap_or_else(now_ms),
            });
            Ok(())
        })?;
        while let Some(packet) = self.outgoing.pop_front() {
            let now = now_ms();
            if packet.at > now || now - packet.at >= MAX_AGE_MS {
                return Err(Error::Bounds);
            }
            self.rtc.relay_tick()?; // Original Permit gate before every write.
            let relay = self.relay.as_mut().ok_or(Error::Closed)?;
            let remaining = Duration::from_millis(MAX_AGE_MS - (now - packet.at));
            tokio::time::timeout(remaining, relay.send(&packet.data))
                .await
                .map_err(|_| Error::Bounds)??;
            self.rtc.relay_tick()?;
        }
        if let (Some(key), Some(actual)) = (self.rtc.verified_peer(), self.rtc.selected_pair) {
            if actual != (local, peer) {
                return Err(Error::Unverified);
            }
            if self.verified.is_none() {
                self.verified = Some(VerifiedRemotePeer {
                    fingerprint: key,
                    local,
                    peer,
                });
            }
        }
        Ok(())
    }
    pub async fn service(&mut self) -> Result<(), Error> {
        let result = self.service_inner().await;
        if result.is_err() {
            self.fence();
        }
        result
    }
    async fn service_inner(&mut self) -> Result<(), Error> {
        let (_, peer) = self.current_path()?;
        self.rtc.relay_tick()?;
        self.flush(None).await?;
        if let Some(data) = self
            .relay
            .as_mut()
            .ok_or(Error::Closed)?
            .step(Duration::from_millis(1))
            .await?
        {
            self.rtc.relay_input(peer, &data)?;
            self.flush(None).await?;
        }
        Ok(())
    }
    pub async fn send(&mut self, frame: &Frame) -> Result<(), Error> {
        let result = async {
            self.verified_peer().ok_or(Error::Unverified)?;
            self.rtc.enqueue_frame(frame, now_ms())?;
            self.flush(Some(frame.at_ms)).await
        }
        .await;
        if result.is_err() {
            self.fence();
        }
        result
    }
    pub fn receive(&mut self) -> Result<Option<Frame>, Error> {
        if self.verified_peer().is_none() {
            return Err(Error::Unverified);
        }
        match self.rtc.receive(now_ms()) {
            Ok(frame) => Ok(frame),
            Err(e) => {
                self.fence();
                Err(e.into())
            }
        }
    }
    pub fn counters(&self) -> (u64, u64, u64) {
        (
            self.rtc.sent_frames,
            self.rtc.received_frames,
            self.rtc.dropped_frames,
        )
    }
    /// Stops speech first. Deallocate/TLS close is best effort, never a
    /// release_verified result; cancelled close still drops/closes the driver.
    pub async fn close(mut self) {
        // Don't fence the network until the bounded deallocation attempt.
        self.closed = true;
        self.verified = None;
        self.outgoing.clear();
        self.rtc.close();
        if let Some(relay) = self.relay.take() {
            relay.close().await;
        }
    }
}
