// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed platform initialization. Never opens PCM or accepts a control name.
use crate::{probe, CString, Ctl, ElemId, ElemIface, ElemType, ElemValue, CTL, ROUTES};
use std::{io, time::Duration};

const STREAMS: [&str; 4] = ["pcm0p", "pcm1c", "pcm4p", "pcm4c"];
const READINESS_TIMEOUT: Duration = Duration::from_secs(20);
const READINESS_POLL: Duration = Duration::from_millis(100);

trait Readiness {
    fn status(&mut self, stream: &str) -> io::Result<String>;
    fn elapsed(&self) -> Duration;
    fn pause(&mut self, duration: Duration);
}

struct ProcReadiness(std::time::Instant);
impl Readiness for ProcReadiness {
    fn status(&mut self, stream: &str) -> io::Result<String> {
        std::fs::read_to_string(format!("/proc/asound/card0/{stream}/sub0/status"))
    }
    fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }
    fn pause(&mut self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

// Module loading may return before the sound card finishes registering. Only a
// missing fixed status file is retryable; missing never means an idle device.
// Check every stream on each pass so a missing file cannot conceal a busy one.
fn wait_closed(readiness: &mut impl Readiness) -> Result<(), String> {
    loop {
        if readiness.elapsed() >= READINESS_TIMEOUT {
            return Err("PCM readiness timed out".into());
        }
        let mut missing = false;
        for stream in STREAMS {
            match readiness.status(stream) {
                Ok(status) if status.trim() == "closed" => (),
                Ok(_) => return Err(format!("PCM is in use or status invalid: {stream}")),
                Err(error) if error.kind() == io::ErrorKind::NotFound => missing = true,
                Err(error) => {
                    return Err(format!(
                        "PCM status unavailable: {stream} {:?}",
                        error.kind()
                    ));
                }
            }
        }
        let remaining = READINESS_TIMEOUT.saturating_sub(readiness.elapsed());
        if remaining.is_zero() {
            return Err("PCM readiness timed out".into());
        }
        if !missing {
            return Ok(());
        }
        readiness.pause(READINESS_POLL.min(remaining));
    }
}

trait Controls {
    fn write(&mut self, index: usize, enabled: bool) -> Result<(), ()>;
    fn read(&mut self, index: usize) -> Result<bool, ()>;
}

fn restore(controls: &mut impl Controls, changed: &[usize]) -> bool {
    let mut restored = true;
    for &index in changed.iter().rev() {
        if controls.write(index, false).is_err() || controls.read(index) != Ok(false) {
            restored = false;
        }
    }
    restored
}

fn enable(controls: &mut impl Controls, before: [bool; 7]) -> Result<usize, String> {
    let mut changed = Vec::with_capacity(7);
    for (index, enabled) in before.into_iter().enumerate() {
        if enabled {
            continue;
        }
        // Include a failed write: its result can be uncertain.
        changed.push(index);
        if controls.write(index, true).is_err() {
            let rollback = restore(controls, &changed);
            return Err(format!(
                "route write index={index} rollback_complete={rollback}"
            ));
        }
    }
    for index in 0..7 {
        if controls.read(index) != Ok(true) {
            let rollback = restore(controls, &changed);
            return Err(format!(
                "route readback index={index} rollback_complete={rollback}"
            ));
        }
    }
    Ok(changed.len())
}

struct Alsa(Ctl);
impl Alsa {
    fn value(index: usize) -> Result<ElemValue, ()> {
        let mut id = ElemId::new(ElemIface::Mixer);
        id.set_name(&CString::new(ROUTES[index]).map_err(|_| ())?);
        let mut value = ElemValue::new(ElemType::Boolean).map_err(|_| ())?;
        value.set_id(&id);
        Ok(value)
    }
}
impl Controls for Alsa {
    fn write(&mut self, index: usize, enabled: bool) -> Result<(), ()> {
        let mut value = Self::value(index)?;
        value.set_boolean(0, enabled).ok_or(())?;
        self.0.elem_write(&value).map(|_| ()).map_err(|_| ())
    }
    fn read(&mut self, index: usize) -> Result<bool, ()> {
        let mut value = Self::value(index)?;
        self.0.elem_read(&mut value).map_err(|_| ())?;
        value.get_boolean(0).ok_or(())
    }
}

/// Root-only board startup command. PCM device access is unnecessary and should
/// remain denied in its systemd unit. Media still requires a live Core permit.
pub fn prepare_routes() -> Result<usize, String> {
    if !rustix::process::geteuid().is_root() {
        return Err("platform initialization requires Root".into());
    }
    let compatible = std::fs::read("/sys/firmware/devicetree/base/compatible")
        .map_err(|_| "board identity unavailable")?;
    if !compatible.split(|b| *b == 0).any(|v| v == b"thwc,ufi003") {
        return Err("unsupported board".into());
    }
    let mut readiness = ProcReadiness(std::time::Instant::now());
    wait_closed(&mut readiness)?;
    eprintln!(
        "platform_readiness ready_ms={}",
        readiness.elapsed().as_millis()
    );
    let before = probe().map_err(|e| format!("fixed audio profile unavailable: {e:?}"))?;
    let mut controls = Alsa(Ctl::new(CTL, false).map_err(|_| "control unavailable")?);
    enable(&mut controls, before.routes_on)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Card {
        elapsed: Duration,
        ready_at: [Duration; 4],
        fail: Option<(usize, io::ErrorKind)>,
        nonclosed: Option<(usize, &'static str)>,
        pauses: usize,
        slow_read: bool,
        oversleep: bool,
    }
    impl Readiness for Card {
        fn status(&mut self, stream: &str) -> io::Result<String> {
            let index = STREAMS.iter().position(|v| *v == stream).unwrap();
            if self.slow_read {
                self.elapsed += Duration::from_secs(6);
            }
            if let Some((failed, kind)) = self.fail {
                if index == failed {
                    return Err(io::Error::from(kind));
                }
            }
            if self.elapsed < self.ready_at[index] {
                return Err(io::Error::from(io::ErrorKind::NotFound));
            }
            if let Some((busy, value)) = self.nonclosed {
                if index == busy {
                    return Ok(value.into());
                }
            }
            Ok("closed\n".into())
        }
        fn elapsed(&self) -> Duration {
            self.elapsed
        }
        fn pause(&mut self, duration: Duration) {
            self.pauses += 1;
            self.elapsed += if self.oversleep {
                Duration::from_secs(21)
            } else {
                duration
            };
        }
    }

    #[test]
    fn closed_card_needs_no_delay() {
        let mut card = Card::default();
        assert_eq!(wait_closed(&mut card), Ok(()));
        assert_eq!(card.pauses, 0);
    }

    #[test]
    fn waits_for_every_fixed_stream_to_appear_closed() {
        let mut card = Card {
            ready_at: [0, 100, 700, 1500].map(Duration::from_millis),
            ..Card::default()
        };
        assert_eq!(wait_closed(&mut card), Ok(()));
        assert_eq!(card.elapsed, Duration::from_millis(1500));
        assert_eq!(card.pauses, 15);
    }

    #[test]
    fn missing_status_has_a_fixed_deadline_and_is_never_assumed_closed() {
        let mut card = Card {
            ready_at: [Duration::from_secs(21); 4],
            ..Card::default()
        };
        assert_eq!(
            wait_closed(&mut card),
            Err("PCM readiness timed out".into())
        );
        assert_eq!(card.elapsed, READINESS_TIMEOUT);
        assert_eq!(card.pauses, 200);
    }

    #[test]
    fn stream_appearing_busy_during_wait_fails_before_initialization() {
        let mut card = Card {
            ready_at: [Duration::from_millis(300); 4],
            nonclosed: Some((3, "RUNNING")),
            ..Card::default()
        };
        assert!(wait_closed(&mut card).unwrap_err().contains("pcm4c"));
        assert_eq!(card.elapsed, Duration::from_millis(300));
        assert_eq!(card.pauses, 3);
    }

    #[test]
    fn missing_earlier_stream_does_not_hide_busy_or_invalid_status() {
        for value in ["RUNNING", "", "closed\nRUNNING", "not closed"] {
            let mut card = Card {
                ready_at: [
                    Duration::from_secs(21),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                ],
                nonclosed: Some((3, value)),
                ..Card::default()
            };
            assert!(wait_closed(&mut card).unwrap_err().contains("pcm4c"));
            assert_eq!(card.pauses, 0);
        }
    }

    #[test]
    fn errors_other_than_not_found_fail_without_retry() {
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::InvalidData,
            io::ErrorKind::Interrupted,
        ] {
            let mut card = Card {
                ready_at: [
                    Duration::from_secs(21),
                    Duration::ZERO,
                    Duration::ZERO,
                    Duration::ZERO,
                ],
                fail: Some((2, kind)),
                ..Card::default()
            };
            assert!(wait_closed(&mut card).unwrap_err().contains("pcm4p"));
            assert_eq!(card.pauses, 0);
        }
    }

    #[test]
    fn elapsed_budget_is_not_extended_by_slow_reads_or_oversleep() {
        let mut slow = Card {
            slow_read: true,
            ..Card::default()
        };
        assert_eq!(
            wait_closed(&mut slow),
            Err("PCM readiness timed out".into())
        );
        assert_eq!(slow.pauses, 0);
        let mut late = Card {
            ready_at: [Duration::from_secs(1); 4],
            oversleep: true,
            ..Card::default()
        };
        assert_eq!(
            wait_closed(&mut late),
            Err("PCM readiness timed out".into())
        );
        assert_eq!(late.pauses, 1);
    }

    #[derive(Default)]
    struct Mock {
        state: [bool; 7],
        writes: Vec<(usize, bool)>,
        fail_write: Option<usize>,
        fail_read: Option<usize>,
        fail_restore: bool,
    }
    impl Controls for Mock {
        fn write(&mut self, i: usize, value: bool) -> Result<(), ()> {
            self.writes.push((i, value));
            if !value && self.fail_restore {
                return Err(());
            }
            self.state[i] = value;
            if value && self.fail_write == Some(i) {
                Err(())
            } else {
                Ok(())
            }
        }
        fn read(&mut self, i: usize) -> Result<bool, ()> {
            if self.fail_read == Some(i) && self.state[i] {
                Err(())
            } else {
                Ok(self.state[i])
            }
        }
    }
    #[test]
    fn enables_and_reads_back_exact_seven() {
        let mut c = Mock::default();
        assert_eq!(enable(&mut c, [false; 7]), Ok(7));
        assert_eq!(c.state, [true; 7]);
        assert_eq!(c.writes.len(), 7);
    }
    #[test]
    fn already_initialized_is_readonly() {
        let mut c = Mock {
            state: [true; 7],
            ..Mock::default()
        };
        assert_eq!(enable(&mut c, [true; 7]), Ok(0));
        assert!(c.writes.is_empty());
    }
    #[test]
    fn uncertain_write_restores_it_and_previous_changes_in_reverse() {
        let mut c = Mock {
            fail_write: Some(3),
            ..Mock::default()
        };
        assert!(enable(&mut c, [false; 7])
            .unwrap_err()
            .ends_with("rollback_complete=true"));
        assert_eq!(c.state, [false; 7]);
        assert_eq!(
            &c.writes[4..],
            &[(3, false), (2, false), (1, false), (0, false)]
        );
    }
    #[test]
    fn failed_readback_rolls_back_and_retains_preexisting_route() {
        let mut before = [false; 7];
        before[2] = true;
        let mut c = Mock {
            state: before,
            fail_read: Some(4),
            ..Mock::default()
        };
        assert!(enable(&mut c, before).is_err());
        assert_eq!(c.state, before);
        assert!(!c.writes.iter().any(|v| v.0 == 2));
    }
    #[test]
    fn rollback_failure_is_reported() {
        let mut c = Mock {
            fail_write: Some(1),
            fail_restore: true,
            ..Mock::default()
        };
        assert!(enable(&mut c, [false; 7])
            .unwrap_err()
            .ends_with("rollback_complete=false"));
    }
}
