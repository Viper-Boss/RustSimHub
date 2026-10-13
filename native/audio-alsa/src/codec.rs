// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed 20ms mono speech. Codec state belongs to exactly one media authorization.
use rustsimhub_media::{Error, Frame, Permit, MAX_AGE_MS, RATE, SAMPLES};
pub const MAX_PACKET: usize = 400;
pub const RTP_RATE: u32 = 48_000;
pub const RTP_STEP: u32 = 960;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecError {
    Authorization(Error),
    Packet,
    Codec,
}
// Never Debug or Serialize: encoded speech is also private conversation data.
pub struct Packet {
    bytes: [u8; MAX_PACKET],
    len: usize,
}
impl Packet {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}
impl Drop for Packet {
    fn drop(&mut self) {
        self.bytes.fill(0);
        self.len = 0;
    }
}
pub struct Codec {
    encoder: opus::Encoder,
    decoder: opus::Decoder,
    permit: Permit,
    closed: bool,
    last_receive: Option<u64>,
}
impl Codec {
    pub fn new(permit: Permit, now: u64) -> Result<Self, CodecError> {
        permit.check(now).map_err(CodecError::Authorization)?;
        let mut encoder = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)
            .map_err(|_| CodecError::Codec)?;
        encoder
            .set_bitrate(opus::Bitrate::Bits(24_000))
            .map_err(|_| CodecError::Codec)?;
        encoder.set_complexity(3).map_err(|_| CodecError::Codec)?;
        encoder.set_vbr(false).map_err(|_| CodecError::Codec)?;
        encoder.set_dtx(false).map_err(|_| CodecError::Codec)?;
        let decoder =
            opus::Decoder::new(RATE, opus::Channels::Mono).map_err(|_| CodecError::Codec)?;
        Ok(Self {
            encoder,
            decoder,
            permit,
            closed: false,
            last_receive: None,
        })
    }
    fn gate(&mut self, now: u64) -> Result<(), CodecError> {
        if self.closed {
            return Err(CodecError::Authorization(Error::Denied));
        }
        if let Err(e) = self.permit.check(now) {
            self.close();
            return Err(CodecError::Authorization(e));
        }
        Ok(())
    }
    pub fn encode(&mut self, frame: &Frame, now: u64) -> Result<Packet, CodecError> {
        self.gate(now)?;
        if frame.at_ms > now || now - frame.at_ms >= MAX_AGE_MS {
            return Err(CodecError::Packet);
        }
        let mut packet = Packet {
            bytes: [0; MAX_PACKET],
            len: 0,
        };
        packet.len = self
            .encoder
            .encode(&frame.samples, &mut packet.bytes)
            .map_err(|_| CodecError::Codec)?;
        if packet.len == 0
            || packet.len > MAX_PACKET
            || opus::packet::get_nb_samples(packet.bytes(), RATE).map_err(|_| CodecError::Packet)?
                != SAMPLES
        {
            return Err(CodecError::Packet);
        }
        self.gate(now)?;
        Ok(packet)
    }
    /// Call only for an authenticated/decrypted peer packet with trusted local receive time.
    /// Extended sequence is supplied by the transport, not a JSON assertion.
    pub fn decode(
        &mut self,
        packet: &[u8],
        sequence: u64,
        at_ms: u64,
        now: u64,
    ) -> Result<Frame, CodecError> {
        self.gate(now)?;
        if packet.is_empty()
            || packet.len() > MAX_PACKET
            || at_ms > now
            || now - at_ms >= MAX_AGE_MS
            || self.last_receive.is_some_and(|s| sequence <= s)
        {
            return Err(CodecError::Packet);
        }
        if opus::packet::get_nb_samples(packet, RATE).map_err(|_| CodecError::Packet)? != SAMPLES {
            return Err(CodecError::Packet);
        }
        let mut frame = Frame {
            samples: [0; SAMPLES],
            sequence,
            at_ms,
        };
        if self
            .decoder
            .decode(packet, &mut frame.samples, false)
            .map_err(|_| CodecError::Codec)?
            != SAMPLES
        {
            frame.samples.fill(0);
            return Err(CodecError::Packet);
        }
        if let Err(e) = self.gate(now) {
            frame.samples.fill(0);
            return Err(e);
        }
        self.last_receive = Some(sequence);
        Ok(frame)
    }
    pub fn close(&mut self) {
        self.closed = true;
        let _ = self.encoder.reset_state();
        let _ = self.decoder.reset_state();
        self.last_receive = None;
    }
}
impl Drop for Codec {
    fn drop(&mut self) {
        self.close();
    }
}

pub(crate) fn fixture_for_peer(
    peer_key: [u8; 32],
) -> Result<(rustsimhub_media::Authority, Permit), CodecError> {
    let mut authority = rustsimhub_media::Authority::new();
    let context = rustsimhub_media::Context {
        backend: "synthetic-codec-only".into(),
        boot: "a".repeat(64),
        line: 1,
        call: "c".repeat(36),
        call_revision: 1,
        lease_generation: 1,
        principal: "p".repeat(36),
        principal_generation: 1,
        peer_key,
        lease_until_ms: 30_000,
        active: true,
        media_scope: true,
        control_scope: true,
    };
    let ticket = authority
        .issue(context.clone(), 0)
        .map_err(CodecError::Authorization)?;
    let permit = authority
        .consume(ticket, &context, &context.peer_key, 0)
        .map_err(CodecError::Authorization)?;
    Ok((authority, permit))
}
fn fixture() -> Result<(rustsimhub_media::Authority, Permit), CodecError> {
    fixture_for_peer([7; 32])
}
/// Bounded synthetic diagnostic: never opens ALSA, never encodes a real conversation.
pub fn self_check() -> Result<serde_json::Value, CodecError> {
    let (_authority, permit) = fixture()?;
    let mut codec = Codec::new(permit, 0)?;
    let start = std::time::Instant::now();
    let mut bytes = 0;
    let mut peak = 0;
    for sequence in 0..500 {
        let frame = Frame {
            samples: std::array::from_fn(|i| if i % 16 < 8 { 2048 } else { -2048 }),
            sequence,
            at_ms: 0,
        };
        let packet = codec.encode(&frame, 0)?;
        bytes += packet.bytes().len();
        let decoded = codec.decode(packet.bytes(), sequence, 0, 0)?;
        peak = peak.max(
            decoded
                .samples
                .iter()
                .map(|s| i32::from(*s).unsigned_abs())
                .max()
                .unwrap_or(0),
        );
    }
    codec.close();
    Ok(
        serde_json::json!({"codec":"opus","library_version":opus::version(),"sample_rate":RATE,"channels":1,"frame_samples":SAMPLES,"rtp_rate":RTP_RATE,"rtp_step":RTP_STEP,"frames":500,"encoded_bytes":bytes,"decoded_peak":peak,"elapsed_us":start.elapsed().as_micros(),"hardware_mutations":false,"pcm_opened":false,"synthetic_only":true}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_opus_roundtrip_is_bounded_and_has_fixed_rtp_clock() {
        let v = self_check().unwrap();
        assert!(v["decoded_peak"].as_u64().unwrap() > 0);
        assert_eq!(v["frames"], 500);
        assert!(v["encoded_bytes"].as_u64().unwrap() <= 500 * MAX_PACKET as u64);
        assert_eq!(RTP_STEP as usize, SAMPLES * 3);
    }
    #[test]
    fn wrong_packet_duration_and_malformed_or_oversized_data_are_rejected() {
        let (_a, p) = fixture().unwrap();
        let mut c = Codec::new(p, 0).unwrap();
        for bad in [&[][..], &[0xff][..], &[0u8; MAX_PACKET + 1][..]] {
            assert!(c.decode(bad, 1, 0, 0).is_err());
        }
        let mut other =
            opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip).unwrap();
        let mut out = [0; MAX_PACKET];
        let n = other.encode(&[0; 160], &mut out).unwrap();
        assert!(c.decode(&out[..n], 1, 0, 0).is_err());
    }
    #[test]
    fn stale_future_replayed_and_revoked_frames_fail_before_codec_use() {
        let (mut a, p) = fixture().unwrap();
        let mut c = Codec::new(p, 0).unwrap();
        let mut frame = Frame {
            samples: [1024; SAMPLES],
            sequence: 1,
            at_ms: 0,
        };
        assert!(c.encode(&frame, 60).is_err());
        frame.at_ms = 1;
        assert!(c.encode(&frame, 0).is_err());
        frame.at_ms = 0;
        let packet = c.encode(&frame, 0).unwrap();
        c.decode(packet.bytes(), 1, 0, 0).unwrap();
        assert!(c.decode(packet.bytes(), 1, 0, 0).is_err());
        assert!(c.decode(packet.bytes(), 2, 0, 60).is_err());
        a.revoke();
        assert!(matches!(
            c.encode(&frame, 0),
            Err(CodecError::Authorization(_))
        ));
        assert!(c.decode(packet.bytes(), 2, 0, 0).is_err());
    }
}
