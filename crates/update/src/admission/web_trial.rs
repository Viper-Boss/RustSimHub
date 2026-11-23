// SPDX-License-Identifier: AGPL-3.0-only
//! One explicit Root maintenance trial. Live-health confirmation is separate.
use super::{systemd::runtime_capture, web_upload, *};
use crate::{runtime_observation::RuntimeObservation, store::inspection::ReadOnlyState};
use fs2::FileExt;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const APP_GROUP: &str = "rustsimhub-app.target";
const TIMERS: [(&str, &str); 2] = [
    (
        "rustsimhub-usage-checkin.timer",
        "rustsimhub-usage-checkin.service",
    ),
    (
        "rustsimhub-sim-sms-storage.timer",
        "rustsimhub-sim-sms-storage.service",
    ),
];

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrialResult {
    AwaitingHealthConfirmation,
    RejectedAndRestored,
}

fn capture(program: &str, arguments: &[&str], budget: Duration) -> Result<Vec<u8>> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env_clear()
        .env("LC_ALL", "C")
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    runtime_capture(command, budget)
}

fn systemd(arguments: &[&str]) -> Result<Vec<u8>> {
    capture("/usr/bin/systemctl", arguments, Duration::from_secs(12))
}

fn unit_state(unit: &str) -> Result<String> {
    let raw = systemd(&["--system", "show", "--value", "-p", "ActiveState", unit])?;
    let state = std::str::from_utf8(&raw).map_err(|_| Error::Invalid)?;
    let state = state.strip_suffix('\n').ok_or(Error::Invalid)?;
    if state.is_empty() || state.contains('\n') {
        return Err(Error::Invalid);
    }
    Ok(state.to_owned())
}

fn calls_idle(output: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(output)
        .map_err(|_| Error::Invalid)?
        .trim();
    if matches!(text, "No calls were found" | "No calls were found.") {
        return Ok(());
    }
    let mut count = 0;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let path = fields
            .next()
            .and_then(|field| field.strip_prefix("/org/freedesktop/ModemManager1/Call/"));
        if !path.is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()))
            || !matches!(fields.next(), Some("incoming" | "outgoing"))
            || fields.next() != Some("(terminated)")
            || fields.next().is_some()
        {
            return Err(Error::Busy);
        }
        count += 1;
    }
    if !(1..=64).contains(&count) {
        return Err(Error::Busy);
    }
    Ok(())
}

fn check_idle() -> Result<()> {
    calls_idle(&capture(
        "/usr/bin/mmcli",
        &["--timeout=3", "-m", "any", "--voice-list-calls"],
        Duration::from_secs(4),
    )?)
}

fn runner_lock() -> Result<File> {
    let dir = Path::new("/run/rustsimhub-app");
    let stat = denied_io(fs::symlink_metadata(dir))?;
    if !stat.is_dir() || stat.uid() != 0 || stat.mode() & 0o7777 != 0o755 {
        return Err(Error::Denied);
    }
    let lock = denied_io(
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(dir.join(".web-trial.lock")),
    )?;
    let stat = denied_io(lock.metadata())?;
    if !stat.is_file()
        || stat.uid() != 0
        || stat.mode() & 0o7777 != 0o600
        || stat.nlink() != 1
        || stat.len() != 0
    {
        return Err(Error::Denied);
    }
    lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
    Ok(lock)
}

trait Trial {
    fn standby(&mut self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn admit(&self) -> Result<()>;
    fn start(&self) -> Result<()>;
    fn observe(&self, candidate: bool) -> Result<()>;
    fn recover(&self) -> Result<()>;
    fn restore_timers(&self) -> Result<()>;
}

fn execute(trial: &mut impl Trial) -> Result<TrialResult> {
    let mut touched_application = false;
    let attempt = (|| {
        trial.standby()?;
        // A timed-out stop may already have changed the group.
        touched_application = true;
        trial.stop()?;
        trial.admit()?;
        trial.start()?;
        trial.observe(true)
    })();
    let result = match attempt {
        Ok(()) => Ok(TrialResult::AwaitingHealthConfirmation),
        Err(_) if touched_application => trial.recover().map(|()| TrialResult::RejectedAndRestored),
        Err(error) => Err(error),
    };
    let timers = trial.restore_timers();
    result.and_then(|result| timers.map(|()| result))
}

struct SystemTrial<'a> {
    request: &'a str,
    manifest: &'a str,
    baseline: &'a str,
    active_timers: Vec<&'static str>,
}

impl Trial for SystemTrial<'_> {
    fn standby(&mut self) -> Result<()> {
        for (timer, _) in TIMERS {
            match unit_state(timer)?.as_str() {
                "active" => self.active_timers.push(timer),
                "inactive" => (),
                _ => return Err(Error::Busy),
            }
        }
        for timer in &self.active_timers {
            systemd(&["--system", "stop", timer])?;
        }
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if TIMERS.iter().all(|(timer, service)| {
                unit_state(timer).is_ok_and(|state| state == "inactive")
                    && unit_state(service)
                        .is_ok_and(|state| matches!(state.as_str(), "inactive" | "failed"))
            }) {
                break;
            }
            if Instant::now() >= until {
                return Err(Error::Busy);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        check_idle()
    }

    fn stop(&self) -> Result<()> {
        systemd(&["--system", "stop", APP_GROUP])?;
        super::systemd::web_trial_stopped()
    }

    fn admit(&self) -> Result<()> {
        check_idle()?;
        let result = web_upload::admit_web_upload(self.request, self.manifest)?;
        if result.staged.as_deref() != Some(self.manifest) {
            return Err(Error::Denied);
        }
        Ok(())
    }

    fn start(&self) -> Result<()> {
        systemd(&["--system", "start", APP_GROUP]).map(|_| ())
    }

    fn observe(&self, candidate: bool) -> Result<()> {
        let expected = if candidate {
            self.manifest
        } else {
            self.baseline
        };
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            if RuntimeObservation::observe()
                .and_then(|runtime| runtime.selected_version(expected, candidate))
                == Ok(true)
            {
                return Ok(());
            }
            if Instant::now() >= until {
                return Err(Error::Busy);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn recover(&self) -> Result<()> {
        // The existing group OnFailure may already have restored the baseline.
        let state = ReadOnlyState::observe()?;
        let (selected, _, attempted, _) = state.runtime_version()?;
        let pending = state.pending_trial_hash()?.map(str::to_owned);
        if pending.as_deref().is_some_and(|hash| hash != self.manifest)
            || (selected != self.baseline && selected != self.manifest)
        {
            return Err(Error::Denied);
        }
        if pending.is_none() && (attempted || selected != self.baseline) {
            return Err(Error::Denied);
        }
        drop(state);
        if pending.is_some() {
            self.stop()?;
            web_upload::reject_web_trial(self.request, self.manifest)?;
        }
        self.start()?;
        self.observe(false)
    }

    fn restore_timers(&self) -> Result<()> {
        let mut last = Ok(());
        for timer in &self.active_timers {
            if let Err(error) = systemd(&["--system", "start", timer]) {
                last = Err(error);
            }
        }
        last
    }
}

pub fn run_web_trial(request: &str, manifest: &str) -> Result<TrialResult> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    if !rustsimhub_wire::UpdateUpload::valid_reference(request, manifest) {
        return Err(Error::Invalid);
    }
    let _lock = runner_lock()?;
    web_upload::check_web_upload(request, manifest)?;
    let state = ReadOnlyState::observe()?;
    let (baseline, _, attempted, pending) = state.runtime_version()?;
    if attempted || pending {
        return Err(Error::Busy);
    }
    let baseline = baseline.to_owned();
    drop(state);
    if !RuntimeObservation::observe()?.selected_version(&baseline, false)? {
        return Err(Error::Busy);
    }
    check_idle()?;
    execute(&mut SystemTrial {
        request,
        manifest,
        baseline: &baseline,
        active_timers: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Probe {
        fail: &'static str,
        log: RefCell<Vec<&'static str>>,
    }
    impl Probe {
        fn call(&self, name: &'static str) -> Result<()> {
            self.log.borrow_mut().push(name);
            if name == self.fail {
                Err(Error::Busy)
            } else {
                Ok(())
            }
        }
    }
    impl Trial for Probe {
        fn standby(&mut self) -> Result<()> {
            self.call("standby")
        }
        fn stop(&self) -> Result<()> {
            self.call("stop")
        }
        fn admit(&self) -> Result<()> {
            self.call("admit")
        }
        fn start(&self) -> Result<()> {
            self.call("start")
        }
        fn observe(&self, _: bool) -> Result<()> {
            self.call("observe")
        }
        fn recover(&self) -> Result<()> {
            self.call("restore baseline")
        }
        fn restore_timers(&self) -> Result<()> {
            self.call("restore timers")
        }
    }

    #[test]
    fn web_trial_restores_after_failed_start_and_never_confirms_on_process_state() {
        for (fail, expected, steps) in [
            (
                "",
                Ok(TrialResult::AwaitingHealthConfirmation),
                vec![
                    "standby",
                    "stop",
                    "admit",
                    "start",
                    "observe",
                    "restore timers",
                ],
            ),
            (
                "standby",
                Err(Error::Busy),
                vec!["standby", "restore timers"],
            ),
            (
                "start",
                Ok(TrialResult::RejectedAndRestored),
                vec![
                    "standby",
                    "stop",
                    "admit",
                    "start",
                    "restore baseline",
                    "restore timers",
                ],
            ),
        ] {
            let mut probe = Probe {
                fail,
                log: RefCell::new(vec![]),
            };
            assert_eq!(execute(&mut probe), expected);
            assert_eq!(probe.log.into_inner(), steps);
        }
    }

    #[test]
    fn web_trial_busy_or_unknown_calls_inhibit_install() {
        let prefix = "/org/freedesktop/ModemManager1/Call/1";
        assert_eq!(calls_idle(b"No calls were found"), Ok(()));
        assert_eq!(
            calls_idle(format!("{prefix} incoming (terminated)").as_bytes()),
            Ok(())
        );
        for state in ["active", "ringing-in", "unknown"] {
            assert_eq!(
                calls_idle(format!("{prefix} incoming ({state})").as_bytes()),
                Err(Error::Busy)
            );
        }
        assert_eq!(calls_idle(b""), Err(Error::Busy));
    }
}
