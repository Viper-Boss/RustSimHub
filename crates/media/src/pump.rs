// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Error, Frame, Permit, Queue, FRAME_MS, MAX_AGE_MS, SAMPLES};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Capture,
    Playback,
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IoError {
    Again,
    Xrun,
    Device,
}
/// All implementations must use nonblocking I/O. Recovery may prepare, never drain/resume-loop.
pub trait Pcm {
    fn read(&mut self, samples: &mut [i16]) -> Result<usize, IoError>;
    fn write(&mut self, samples: &[i16]) -> Result<usize, IoError>;
    fn recover(&mut self, direction: Direction) -> Result<(), IoError>;
    fn close(&mut self);
}
#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub captured_frames: u64,
    pub played_frames: u64,
    pub silence_frames: u64,
    pub dropped_frames: u64,
    pub xruns: u64,
    pub capture_peak: u32,
    pub playback_peak: u32,
}
pub struct Pump<D: Pcm> {
    device: D,
    permit: Permit,
    capture: [i16; SAMPLES],
    capture_offset: usize,
    capture_at: u64,
    capture_seq: u64,
    captured: Queue,
    uplink: Queue,
    playing: Option<(Frame, usize, bool)>,
    next_play: u64,
    last_now: u64,
    xrun_since: u64,
    xrun_count: u8,
    closed: bool,
    stats: Stats,
}
impl<D: Pcm> Pump<D> {
    pub fn new(mut device: D, permit: Permit, now: u64) -> Result<Self, Error> {
        if let Err(e) = permit.check(now) {
            device.close();
            return Err(e);
        }
        Ok(Self {
            device,
            permit,
            capture: [0; SAMPLES],
            capture_offset: 0,
            capture_at: now,
            capture_seq: 0,
            captured: Queue::new(),
            uplink: Queue::new(),
            playing: None,
            next_play: now,
            last_now: now,
            xrun_since: now,
            xrun_count: 0,
            closed: false,
            stats: Stats::default(),
        })
    }
    fn gate(&mut self, now: u64) -> Result<(), Error> {
        if self.closed {
            return Err(Error::Denied);
        }
        if now < self.last_now {
            self.close();
            return Err(Error::Expired);
        }
        self.last_now = now;
        if let Err(e) = self.permit.check(now) {
            self.close();
            return Err(e);
        }
        Ok(())
    }
    pub fn enqueue(&mut self, frame: Frame, now: u64) -> Result<(), Error> {
        self.gate(now)?;
        if self.uplink.push(frame, now)? {
            self.stats.dropped_frames += 1;
        }
        Ok(())
    }
    pub fn capture(&mut self, now: u64) -> Result<Option<Frame>, Error> {
        self.gate(now)?;
        while let Some(frame) = self.captured.pop() {
            if now - frame.at_ms < MAX_AGE_MS {
                return Ok(Some(frame));
            }
            self.stats.dropped_frames += 1;
        }
        Ok(None)
    }
    /// One bounded read and write per step. Caller runs this on a media thread, not the core reactor.
    pub fn step(&mut self, now: u64) -> Result<(), Error> {
        self.gate(now)?;
        if self.capture_offset > 0 && now - self.capture_at >= MAX_AGE_MS {
            self.capture_offset = 0;
            self.capture.fill(0);
            self.stats.dropped_frames += 1;
        }
        if self.capture_offset == 0 {
            self.capture_at = now;
        }
        match self.device.read(&mut self.capture[self.capture_offset..]) {
            Ok(n) if n <= SAMPLES - self.capture_offset => {
                self.capture_offset += n;
                if self.capture_offset == SAMPLES {
                    self.stats.capture_peak = self.stats.capture_peak.max(peak(&self.capture));
                    let frame = Frame {
                        samples: self.capture,
                        sequence: self.capture_seq,
                        at_ms: self.capture_at,
                    };
                    self.capture_seq = self.capture_seq.checked_add(1).ok_or_else(|| {
                        self.close();
                        Error::Device
                    })?;
                    if self.captured.push(frame, now)? {
                        self.stats.dropped_frames += 1;
                    }
                    self.capture_offset = 0;
                    self.capture.fill(0);
                    self.stats.captured_frames += 1;
                }
            }
            Ok(_) => return self.fail(Error::Device),
            Err(IoError::Again) => {}
            Err(e) => self.handle_io(e, Direction::Capture, now)?,
        }
        self.gate(now)?;
        if self
            .playing
            .as_ref()
            .is_some_and(|(f, _, _)| now - f.at_ms >= MAX_AGE_MS)
        {
            self.playing = None;
            self.stats.dropped_frames += 1;
            if self.device.recover(Direction::Playback).is_err() {
                return self.fail(Error::Device);
            }
        }
        if self.playing.is_none() && now >= self.next_play {
            let mut ready = None;
            while let Some(frame) = self.uplink.pop() {
                if now - frame.at_ms < MAX_AGE_MS {
                    ready = Some(frame);
                    break;
                }
                self.stats.dropped_frames += 1;
            }
            let silence = ready.is_none();
            let frame = ready.unwrap_or(Frame {
                samples: [0; SAMPLES],
                sequence: 0,
                at_ms: now,
            });
            self.playing = Some((frame, 0, silence));
            // Do not burst queued silence after scheduler delays.
            self.next_play = now.saturating_add(FRAME_MS);
        }
        if let Some((frame, offset, silence)) = &mut self.playing {
            match self.device.write(&frame.samples[*offset..]) {
                Ok(n) if n <= SAMPLES - *offset => {
                    *offset += n;
                    if *offset == SAMPLES {
                        self.stats.playback_peak =
                            self.stats.playback_peak.max(peak(&frame.samples));
                        self.stats.played_frames += 1;
                        if *silence {
                            self.stats.silence_frames += 1;
                        }
                        self.playing = None;
                    }
                }
                Ok(_) => return self.fail(Error::Device),
                Err(IoError::Again) => {}
                Err(e) => self.handle_io(e, Direction::Playback, now)?,
            }
        }
        Ok(())
    }
    fn handle_io(&mut self, e: IoError, direction: Direction, now: u64) -> Result<(), Error> {
        if e != IoError::Xrun {
            return self.fail(Error::Device);
        }
        if now - self.xrun_since >= 1_000 {
            self.xrun_since = now;
            self.xrun_count = 0;
        }
        self.xrun_count += 1;
        self.stats.xruns += 1;
        if self.xrun_count > 3 {
            return self.fail(Error::XrunLimit);
        }
        // A discontinuity loses both partial frames and queues; never replay pre-xrun speech.
        self.capture.fill(0);
        self.capture_offset = 0;
        self.playing = None;
        self.captured.clear();
        self.uplink.clear();
        self.gate(now)?;
        if self.device.recover(direction).is_err() {
            return self.fail(Error::Device);
        }
        Ok(())
    }
    fn fail<T>(&mut self, error: Error) -> Result<T, Error> {
        self.close();
        Err(error)
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.capture.fill(0);
        self.capture_offset = 0;
        self.playing = None;
        self.captured.clear();
        self.uplink.clear();
        self.device.close();
    }
}
impl<D: Pcm> Drop for Pump<D> {
    fn drop(&mut self) {
        self.close();
    }
}
fn peak(samples: &[i16]) -> u32 {
    samples
        .iter()
        .map(|s| i32::from(*s).unsigned_abs())
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authorization::tests::permit;
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Record {
        written: Vec<i16>,
        reads: usize,
        recoveries: usize,
        closed: usize,
    }
    struct Fake {
        record: Arc<Mutex<Record>>,
        chunk: usize,
        xrun: bool,
        again: bool,
        invalid: bool,
    }
    impl Pcm for Fake {
        fn read(&mut self, out: &mut [i16]) -> Result<usize, IoError> {
            self.record.lock().unwrap().reads += 1;
            if self.invalid {
                return Ok(out.len() + 1);
            }
            if self.xrun {
                return Err(IoError::Xrun);
            }
            if self.again {
                return Err(IoError::Again);
            }
            let n = self.chunk.min(out.len());
            out[..n].fill(-32768);
            Ok(n)
        }
        fn write(&mut self, s: &[i16]) -> Result<usize, IoError> {
            if self.again {
                return Err(IoError::Again);
            }
            let n = self.chunk.min(s.len());
            self.record
                .lock()
                .unwrap()
                .written
                .extend_from_slice(&s[..n]);
            Ok(n)
        }
        fn recover(&mut self, _: Direction) -> Result<(), IoError> {
            self.record.lock().unwrap().recoveries += 1;
            Ok(())
        }
        fn close(&mut self) {
            self.record.lock().unwrap().closed += 1;
        }
    }
    fn pump(
        chunk: usize,
        xrun: bool,
        again: bool,
        invalid: bool,
    ) -> (crate::Authority, Pump<Fake>, Arc<Mutex<Record>>) {
        let (a, p) = permit();
        let record = Arc::new(Mutex::new(Record::default()));
        let d = Fake {
            record: record.clone(),
            chunk,
            xrun,
            again,
            invalid,
        };
        (a, Pump::new(d, p, 0).unwrap(), record)
    }
    fn frame(seq: u64, at: u64) -> Frame {
        Frame {
            samples: [123; SAMPLES],
            sequence: seq,
            at_ms: at,
        }
    }
    #[test]
    fn partial_io_assembles_exact_20ms_frames_without_gain() {
        let (_a, mut p, r) = pump(80, false, false, false);
        p.enqueue(frame(1, 0), 0).unwrap();
        for now in 0..4 {
            p.step(now).unwrap();
        }
        let captured = p.capture(4).unwrap().unwrap();
        assert_eq!(captured.samples, [-32768; SAMPLES]);
        assert_eq!(r.lock().unwrap().written, vec![123; SAMPLES]);
        assert_eq!(p.stats().capture_peak, 32768);
    }
    #[test]
    fn full_queue_discards_oldest_and_stale_frames() {
        let (_a, mut p, r) = pump(320, false, false, false);
        for seq in 0..6 {
            p.enqueue(frame(seq, 0), 0).unwrap();
        }
        assert_eq!(p.stats().dropped_frames, 2);
        p.step(60).unwrap();
        assert_eq!(r.lock().unwrap().written, vec![0; SAMPLES]);
        assert_eq!(p.stats().dropped_frames, 6);
        assert!(p.enqueue(frame(6, 0), 60).is_err());
        assert!(p.enqueue(frame(6, 70), 60).is_err());
    }
    #[test]
    fn duplicate_and_out_of_order_uplink_are_rejected() {
        let (_a, mut p, _) = pump(320, false, false, false);
        p.enqueue(frame(2, 0), 0).unwrap();
        assert!(p.enqueue(frame(2, 0), 0).is_err());
        assert!(p.enqueue(frame(1, 0), 0).is_err());
    }
    #[test]
    fn revoke_expiry_or_clock_rollback_closes_before_next_io() {
        for cause in 0..3 {
            let (mut a, mut p, r) = pump(320, false, false, false);
            p.step(20).unwrap();
            let reads = r.lock().unwrap().reads;
            let now = match cause {
                0 => {
                    a.revoke();
                    21
                }
                1 => 1_000,
                _ => 19,
            };
            assert!(p.step(now).is_err());
            assert_eq!(r.lock().unwrap().reads, reads);
            p.close();
            assert_eq!(r.lock().unwrap().closed, 1);
        }
    }
    #[test]
    fn again_and_silence_do_not_build_unbounded_backlog() {
        let (_a, mut p, r) = pump(320, false, true, false);
        for now in 0..100 {
            p.step(now).unwrap();
        }
        assert_eq!(r.lock().unwrap().reads, 100);
        assert!(p.capture(100).unwrap().is_none());
        assert!(p.stats().dropped_frames <= 2);
    }
    #[test]
    fn persistent_xrun_is_bounded_and_invalid_counts_close_device() {
        let (_a, mut p, r) = pump(320, true, false, false);
        for now in 0..3 {
            p.step(now).unwrap();
        }
        assert_eq!(p.step(3), Err(Error::XrunLimit));
        assert_eq!(r.lock().unwrap().recoveries, 3);
        let (_a, mut p, r) = pump(320, false, false, true);
        assert_eq!(p.step(0), Err(Error::Device));
        assert_eq!(r.lock().unwrap().closed, 1);
    }
    #[test]
    fn pcm_bytes_are_fixed_little_endian_and_short_frames_fail() {
        assert!(Frame::from_le(&[0; 639], 0, 0).is_err());
        let mut bytes = [0; 640];
        bytes[0] = 0;
        bytes[1] = 128;
        assert_eq!(Frame::from_le(&bytes, 0, 0).unwrap().samples[0], -32768);
    }
}
