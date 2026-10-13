// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Audio primitives, with no network listener, filesystem recording or hardware discovery.
mod authorization;
pub use authorization::{Authority, Context, Permit, RemoteAuthority, Ticket};
#[cfg(target_os = "linux")]
pub mod local;
pub mod protocol;
mod pump;
pub use pump::{Direction, IoError, Pcm, Pump, Stats};
pub const RATE: u32 = 16_000;
pub const SAMPLES: usize = 320;
pub const FRAME_BYTES: usize = SAMPLES * 2;
pub const FRAME_MS: u64 = 20;
pub const MAX_AGE_MS: u64 = 60;
pub const QUEUE_FRAMES: usize = 4;
pub fn build_source_sha256() -> Option<&'static str> {
    option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256").filter(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    Denied,
    Expired,
    Busy,
    Entropy,
    InvalidFrame,
    Device,
    XrunLimit,
}
// Do not derive Debug: neither samples nor bearer credentials belong in logs.
pub struct Frame {
    pub samples: [i16; SAMPLES],
    pub sequence: u64,
    pub at_ms: u64,
}
impl Frame {
    pub fn from_le(bytes: &[u8], sequence: u64, at_ms: u64) -> Result<Self, Error> {
        if bytes.len() != FRAME_BYTES {
            return Err(Error::InvalidFrame);
        }
        let mut samples = [0; SAMPLES];
        for (s, b) in samples.iter_mut().zip(bytes.as_chunks::<2>().0) {
            *s = i16::from_le_bytes([b[0], b[1]]);
        }
        Ok(Self {
            samples,
            sequence,
            at_ms,
        })
    }
}
pub(crate) struct Queue {
    slots: [Option<Frame>; QUEUE_FRAMES],
    first: usize,
    len: usize,
    last_sequence: Option<u64>,
}
impl Queue {
    fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            first: 0,
            len: 0,
            last_sequence: None,
        }
    }
    fn push(&mut self, frame: Frame, now: u64) -> Result<bool, Error> {
        if frame.at_ms > now
            || now - frame.at_ms >= MAX_AGE_MS
            || self.last_sequence.is_some_and(|seq| frame.sequence <= seq)
        {
            return Err(Error::InvalidFrame);
        }
        self.last_sequence = Some(frame.sequence);
        let dropped = self.len == QUEUE_FRAMES;
        if dropped {
            self.pop();
        }
        self.slots[(self.first + self.len) % QUEUE_FRAMES] = Some(frame);
        self.len += 1;
        Ok(dropped)
    }
    fn pop(&mut self) -> Option<Frame> {
        if self.len == 0 {
            return None;
        }
        let frame = self.slots[self.first].take();
        self.first = (self.first + 1) % QUEUE_FRAMES;
        self.len -= 1;
        frame
    }
    fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = None;
        }
        self.len = 0;
    }
}
