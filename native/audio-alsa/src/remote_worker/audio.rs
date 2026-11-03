// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_media::{Direction, IoError, Pcm, Pump, Stats, QUEUE_FRAMES};
use std::{
    sync::{mpsc, Mutex},
    thread::{self, JoinHandle},
};
pub(super) type Factory =
    Arc<dyn Fn(&rustsimhub_media::Permit) -> Result<Box<dyn Pcm + Send>> + Send + Sync>;
struct Device {
    inner: Box<dyn Pcm + Send>,
    permit: rustsimhub_media::Permit,
    stop: Arc<AtomicBool>,
    closed: bool,
}
impl Device {
    fn gate(&self) -> std::result::Result<(), IoError> {
        if self.closed
            || self.stop.load(Ordering::Acquire)
            || self.permit.check(local::now_ms()).is_err()
        {
            return Err(IoError::Device);
        }
        Ok(())
    }
}
impl Pcm for Device {
    fn read(&mut self, samples: &mut [i16]) -> std::result::Result<usize, IoError> {
        self.gate()?;
        let result = self.inner.read(samples);
        if self.gate().is_err() {
            samples.fill(0);
            return Err(IoError::Device);
        }
        result
    }
    fn write(&mut self, samples: &[i16]) -> std::result::Result<usize, IoError> {
        self.gate()?;
        let result = self.inner.write(samples);
        self.gate()?;
        result
    }
    fn recover(&mut self, direction: Direction) -> std::result::Result<(), IoError> {
        self.gate()?;
        let result = self.inner.recover(direction);
        self.gate()?;
        result
    }
    fn close(&mut self) {
        if !self.closed {
            self.closed = true;
            self.inner.close();
        }
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        self.close();
    }
}
#[derive(Default)]
pub(super) struct Status {
    pub opened: bool,
    pub stats: Stats,
    pub failure: Option<Failure>,
}
pub(super) struct Audio {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<Status>>,
    pub capture: mpsc::Receiver<rustsimhub_media::Frame>,
    pub playback: mpsc::SyncSender<rustsimhub_media::Frame>,
    thread: Option<JoinHandle<()>>,
}
impl Audio {
    pub fn start(permit: rustsimhub_media::Permit, factory: Factory) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let status = Arc::new(Mutex::new(Status::default()));
        let (captured, capture) = mpsc::sync_channel(QUEUE_FRAMES);
        let (playback, received) = mpsc::sync_channel(QUEUE_FRAMES);
        let flag = stop.clone();
        let shared = status.clone();
        let thread = thread::Builder::new()
            .name("simhub-remote-pcm".into())
            .spawn(move || {
                let outcome =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                        if flag.load(Ordering::Acquire) {
                            return Err(Failure::Closed);
                        }
                        permit.check(local::now_ms()).map_err(|_| Failure::Denied)?;
                        let mut device = Device {
                            inner: factory(&permit)?,
                            permit: permit.clone(),
                            stop: flag.clone(),
                            closed: false,
                        };
                        if flag.load(Ordering::Acquire) || permit.check(local::now_ms()).is_err() {
                            device.close();
                            return Err(Failure::Denied);
                        }
                        let mut pump = Pump::new(device, permit.clone(), local::now_ms())
                            .map_err(|_| Failure::Denied)?;
                        shared.lock().map_err(|_| Failure::Unavailable)?.opened = true;
                        while !flag.load(Ordering::Acquire) {
                            permit.check(local::now_ms()).map_err(|_| Failure::Denied)?;
                            for _ in 0..QUEUE_FRAMES {
                                match received.try_recv() {
                                    Ok(frame) => {
                                        pump.enqueue(frame, local::now_ms())
                                            .map_err(|_| Failure::Denied)?;
                                    }
                                    Err(mpsc::TryRecvError::Empty) => break,
                                    Err(mpsc::TryRecvError::Disconnected) => {
                                        return Err(Failure::Closed)
                                    }
                                }
                            }
                            pump.step_with_clock(local::now_ms)
                                .map_err(|_| Failure::Denied)?;
                            // Fresh time and stop gate after external PCM returns. Never forward late speech.
                            permit.check(local::now_ms()).map_err(|_| Failure::Denied)?;
                            if flag.load(Ordering::Acquire) {
                                break;
                            }
                            if let Some(mut frame) =
                                pump.capture(local::now_ms()).map_err(|_| Failure::Denied)?
                            {
                                match captured.try_send(frame) {
                                    Ok(()) => {}
                                    Err(mpsc::TrySendError::Full(late))
                                    | Err(mpsc::TrySendError::Disconnected(late)) => {
                                        frame = late;
                                        frame.samples.fill(0);
                                    }
                                }
                            }
                            shared.lock().map_err(|_| Failure::Unavailable)?.stats = pump.stats();
                            thread::sleep(Duration::from_millis(2));
                        }
                        pump.close();
                        shared.lock().map_err(|_| Failure::Unavailable)?.stats = pump.stats();
                        Ok(())
                    }));
                if let Ok(mut s) = shared.lock() {
                    s.opened = false;
                    s.failure = match outcome {
                        Ok(Ok(())) => None,
                        Ok(Err(e)) => Some(e),
                        Err(_) => Some(Failure::Unavailable),
                    };
                }
                while let Ok(mut frame) = received.try_recv() {
                    frame.samples.fill(0);
                }
            })
            .map_err(|_| Failure::Unavailable)?;
        Ok(Self {
            stop,
            status,
            capture,
            playback,
            thread: Some(thread),
        })
    }
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}
impl Drop for Audio {
    fn drop(&mut self) {
        self.stop();
        // Never detach a late PCM owner or release the exclusive service slot early.
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        while let Ok(mut frame) = self.capture.try_recv() {
            frame.samples.fill(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    struct Late {
        writes: Arc<AtomicUsize>,
        closed: Arc<AtomicUsize>,
    }
    impl Pcm for Late {
        fn read(&mut self, samples: &mut [i16]) -> std::result::Result<usize, IoError> {
            // Deliberately broken external nonblocking backend. The worker must
            // recheck real time on return, rather than reuse Pump's start time.
            thread::sleep(Duration::from_millis(1150));
            samples.fill(123);
            Ok(samples.len())
        }
        fn write(&mut self, samples: &[i16]) -> std::result::Result<usize, IoError> {
            self.writes.fetch_add(1, Ordering::AcqRel);
            Ok(samples.len())
        }
        fn recover(&mut self, _: Direction) -> std::result::Result<(), IoError> {
            Ok(())
        }
        fn close(&mut self) {
            self.closed.fetch_add(1, Ordering::AcqRel);
        }
    }
    #[test]
    fn actual_expired_permit_after_external_read_cannot_reach_any_playback_write() {
        let now = local::now_ms();
        let mut authority = rustsimhub_media::RemoteAuthority::new("a".repeat(64)).unwrap();
        authority.update(&"a".repeat(64), now + 1000, now).unwrap();
        let permit = authority.permit();
        let writes = Arc::new(AtomicUsize::new(0));
        let closed = Arc::new(AtomicUsize::new(0));
        let device = Device {
            inner: Box::new(Late {
                writes: writes.clone(),
                closed: closed.clone(),
            }),
            permit: permit.clone(),
            stop: Arc::new(AtomicBool::new(false)),
            closed: false,
        };
        let mut pump = Pump::new(device, permit, now).unwrap();
        assert!(pump.step(now).is_err());
        assert_eq!(writes.load(Ordering::Acquire), 0);
        assert_eq!(closed.load(Ordering::Acquire), 1);
        drop(pump);
        assert_eq!(closed.load(Ordering::Acquire), 1);
    }
}
