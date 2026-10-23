// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed platform initialization. Never opens PCM or accepts a control name.
use crate::{probe, CString, Ctl, ElemId, ElemIface, ElemType, ElemValue, CTL, ROUTES};

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
    for stream in ["pcm0p", "pcm1c", "pcm4p", "pcm4c"] {
        let path = format!("/proc/asound/card0/{stream}/sub0/status");
        if std::fs::read_to_string(path)
            .map_err(|_| "PCM status unavailable")?
            .trim()
            != "closed"
        {
            return Err("PCM is in use".into());
        }
    }
    let before = probe().map_err(|e| format!("fixed audio profile unavailable: {e:?}"))?;
    let mut controls = Alsa(Ctl::new(CTL, false).map_err(|_| "control unavailable")?);
    enable(&mut controls, before.routes_on)
}

#[cfg(test)]
mod tests {
    use super::*;
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
