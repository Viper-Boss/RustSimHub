// SPDX-License-Identifier: AGPL-3.0-only
//! NasAudioPortV1 offline candidate, consuming the actual Core-issued Permit.
//! The caller supplies a nonblocking PCM port and the same trusted boot clock
//! as the media provider. There is no NAS JSON-to-Permit conversion or hardware
//! discovery. Media ticket IPC is authorization only, never an audio transport.
use rustsimhub_media::{Error, Frame, Pcm, Permit, Pump, Stats, FRAME_BYTES, RATE};

#[derive(Clone, Copy)]
pub struct Format {
    pub rate: u32,
    pub channels: u8,
    pub little_endian_signed_16: bool,
    pub frame_bytes: usize,
}
impl Format {
    pub const PCM16_MONO: Self = Self {
        rate: RATE,
        channels: 1,
        little_endian_signed_16: true,
        frame_bytes: FRAME_BYTES,
    };
    fn valid(self) -> bool {
        self.rate == RATE
            && self.channels == 1
            && self.little_endian_signed_16
            && self.frame_bytes == FRAME_BYTES
    }
}
pub struct CapturedFrame {
    pub bytes: [u8; FRAME_BYTES],
    pub sequence: u64,
    pub at_ms: u64,
}
pub struct NasAudioPortV1<D: Pcm, C: FnMut() -> u64> {
    pump: Pump<D>,
    clock: C,
}
impl<D: Pcm, C: FnMut() -> u64> NasAudioPortV1<D, C> {
    /// Format must be explicitly known; unsupported legacy rates never silently
    /// resample. Both the Permit and clock originate in the real media provider.
    pub fn open(
        mut device: D,
        permit: Permit,
        format: Format,
        mut clock: C,
    ) -> Result<Self, Error> {
        if !format.valid() {
            device.close();
            return Err(Error::InvalidFrame);
        }
        let pump = Pump::new(device, permit, clock())?;
        Ok(Self { pump, clock })
    }
    pub fn push(&mut self, bytes: &[u8], sequence: u64, at_ms: u64) -> Result<(), Error> {
        let frame = Frame::from_le(bytes, sequence, at_ms)?;
        self.pump.enqueue(frame, (self.clock)())
    }
    pub fn pull(&mut self) -> Result<Option<CapturedFrame>, Error> {
        self.pump.capture((self.clock)()).map(|frame| {
            frame.map(|f| {
                let mut bytes = [0; FRAME_BYTES];
                for (chunk, sample) in bytes.as_chunks_mut::<2>().0.iter_mut().zip(f.samples) {
                    chunk.copy_from_slice(&sample.to_le_bytes());
                }
                CapturedFrame {
                    bytes,
                    sequence: f.sequence,
                    at_ms: f.at_ms,
                }
            })
        })
    }
    pub fn step(&mut self) -> Result<(), Error> {
        self.pump.step_with_clock(&mut self.clock)
    }
    pub fn stats(&self) -> Stats {
        self.pump.stats()
    }
    pub fn close(&mut self) {
        self.pump.close();
    }
}

#[cfg(test)]
mod tests;
