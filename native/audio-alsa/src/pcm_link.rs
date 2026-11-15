// SPDX-License-Identifier: AGPL-3.0-only
//! Local raw-PCM transport framing, for a gateway that Edge terminates
//! (the AirSIM phone link). The peer is the local Edge service on a
//! media-owned Unix socket; it supplies the same single-use Core credentials
//! a WebRTC session would, and no audio moves before Core redeems them.
//!
//! Wire: one length-prefixed JSON hello, one accept byte, then mono 16 kHz
//! signed 16-bit little-endian frames of 20 ms in both directions.
use rustsimhub_media::{protocol::Credentials, Frame, FRAME_BYTES, SAMPLES};
use serde::Deserialize;

pub const PROTOCOL: u16 = 1;
pub const MAX_HELLO: usize = 2048;
pub const ACCEPTED: u8 = 1;
/// Uplink audio waiting to be played is bounded: Wi-Fi delivers in bursts,
/// and anything older than this is stale speech, not a backlog to replay.
pub const MAX_BACKLOG_FRAMES: usize = 4;

// No Debug: carries single-use Core credentials.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub protocol: u16,
    pub session: String,
    pub credentials: Credentials,
}

pub fn parse_hello(bytes: &[u8]) -> Option<Hello> {
    if bytes.is_empty() || bytes.len() > MAX_HELLO {
        return None;
    }
    let hello: Hello = serde_json::from_slice(bytes).ok()?;
    (hello.protocol == PROTOCOL && rustsimhub_media::worker::session_id(&hello.session))
        .then_some(hello)
}

pub fn encode(frame: &Frame) -> [u8; FRAME_BYTES] {
    let mut bytes = [0u8; FRAME_BYTES];
    for (chunk, sample) in bytes
        .as_chunks_mut::<2>()
        .0
        .iter_mut()
        .zip(frame.samples.iter())
    {
        chunk.copy_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Reassembles the byte stream into whole frames and numbers them.
#[derive(Default)]
pub struct Framer {
    pending: Vec<u8>,
    sequence: u64,
    pub dropped: u64,
}
impl Framer {
    /// Adds received bytes and returns the complete frames, oldest first.
    /// When more than [`MAX_BACKLOG_FRAMES`] are waiting, the oldest are
    /// dropped (and counted) so latency cannot grow.
    pub fn push(&mut self, bytes: &[u8], now_ms: u64) -> Vec<Frame> {
        self.pending.extend_from_slice(bytes);
        let whole = self.pending.len() / FRAME_BYTES;
        let skip = whole.saturating_sub(MAX_BACKLOG_FRAMES);
        self.dropped += skip as u64;
        let mut frames = Vec::with_capacity(whole - skip);
        for index in skip..whole {
            let at = index * FRAME_BYTES;
            self.sequence += 1;
            let mut samples = [0i16; SAMPLES];
            for (sample, chunk) in samples
                .iter_mut()
                .zip(self.pending[at..at + FRAME_BYTES].as_chunks::<2>().0)
            {
                *sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            }
            frames.push(Frame {
                samples,
                sequence: self.sequence,
                at_ms: now_ms,
            });
        }
        self.pending.drain(..whole * FRAME_BYTES);
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(session: &str, extra: &str) -> Vec<u8> {
        format!(
            r#"{{"protocol":1,"session":"{session}","credentials":{{"ticket":{:?},"peer_key":{:?},"boot":"b"}}{extra}}}"#,
            [7u8; 32], [9u8; 32]
        )
        .into_bytes()
    }

    #[test]
    fn hello_is_strict_and_bounded() {
        let good = parse_hello(&hello(&"a".repeat(32), "")).unwrap();
        assert_eq!(good.credentials.peer_key, [9; 32]);
        assert!(parse_hello(&hello("short", "")).is_none());
        assert!(
            parse_hello(&hello(&"A".repeat(32), "")).is_none(),
            "session ids are lowercase hex"
        );
        assert!(parse_hello(&hello(&"a".repeat(32), r#","role":"owner""#)).is_none());
        assert!(
            parse_hello(br#"{"protocol":2,"session":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#)
                .is_none()
        );
        assert!(parse_hello(b"").is_none());
        assert!(parse_hello(&vec![b' '; MAX_HELLO + 1]).is_none());
    }

    #[test]
    fn frames_round_trip_across_arbitrary_chunking() {
        let frame = Frame {
            samples: std::array::from_fn(|i| (i as i16 - 160) * 100),
            sequence: 1,
            at_ms: 0,
        };
        let bytes = encode(&frame);
        assert_eq!(
            &bytes[..4],
            &(-16000i16)
                .to_le_bytes()
                .iter()
                .chain(&(-15900i16).to_le_bytes())
                .copied()
                .collect::<Vec<_>>()[..]
        );
        let mut framer = Framer::default();
        assert!(framer.push(&bytes[..1], 10).is_empty());
        assert!(framer.push(&bytes[1..639], 11).is_empty());
        let out = framer.push(&[&bytes[639..], &bytes[..5]].concat(), 12);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].samples, frame.samples);
        assert_eq!((out[0].sequence, out[0].at_ms), (1, 12));
        let next = framer.push(&bytes[5..], 13);
        assert_eq!(
            (next.len(), next[0].sequence),
            (1, 2),
            "the split frame is completed in order"
        );
        assert_eq!(framer.dropped, 0);
    }

    #[test]
    fn a_burst_keeps_only_the_newest_frames() {
        let mut framer = Framer::default();
        let mut burst = Vec::new();
        for n in 0..7u8 {
            burst.extend(std::iter::repeat_n(n, FRAME_BYTES));
        }
        let out = framer.push(&burst, 5);
        assert_eq!(out.len(), MAX_BACKLOG_FRAMES);
        assert_eq!(framer.dropped, 3);
        assert_eq!(
            out[0].samples[0],
            i16::from_le_bytes([3, 3]),
            "the oldest three were dropped"
        );
        assert_eq!(out[3].samples[0], i16::from_le_bytes([6, 6]));
        assert!(out.windows(2).all(|w| w[1].sequence == w[0].sequence + 1));
    }
}
