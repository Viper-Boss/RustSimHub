// SPDX-License-Identifier: AGPL-3.0-only
//! One explicit Root maintenance trial. Live-health confirmation is separate.
use super::{systemd::runtime_capture, web_upload, *};
use crate::{runtime_observation::RuntimeObservation, store::inspection::ReadOnlyState};
use fs2::FileExt;
use rustsimhub_wire::{UpdateRunPhase, UpdateRunRecord, UPDATE_RUN_LIMIT};
use std::os::unix::fs::PermissionsExt;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const APP_GROUP: &str = "rustsimhub-app.target";
const JOURNAL: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/web-update";
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
    Confirmed,
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
    unit_property(unit, "ActiveState")
}

fn unit_property(unit: &str, property: &str) -> Result<String> {
    let raw = systemd(&["--system", "show", "--value", "-p", property, unit])?;
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
    fn confirm(&self) -> Result<bool>;
    fn recover(&self) -> Result<()>;
    fn restore_timers(&self) -> Result<()>;
}

fn execute(trial: &mut impl Trial) -> Result<TrialResult> {
    let mut touched_application = false;
    let mut phase = "standby";
    let attempt = (|| {
        trial.standby()?;
        // A timed-out stop may already have changed the group.
        touched_application = true;
        phase = "stop";
        trial.stop()?;
        phase = "admit";
        trial.admit()?;
        phase = "start";
        trial.start()?;
        phase = "observe";
        trial.observe(true)?;
        phase = "confirm";
        trial.confirm()
    })();
    let result = match attempt {
        Ok(true) => Ok(TrialResult::Confirmed),
        Ok(false) => Ok(TrialResult::AwaitingHealthConfirmation),
        Err(error) if touched_application => {
            eprintln!("web application trial failed at {phase}: {error:?}");
            trial.recover().map(|()| TrialResult::RejectedAndRestored)
        }
        Err(error) => Err(error),
    };
    let timers = trial.restore_timers();
    result.and_then(|result| timers.map(|()| result))
}

struct SystemTrial<'a> {
    request: &'a str,
    manifest: &'a str,
    baseline: String,
    auto_confirm: bool,
    active_timers: [bool; 2],
}

impl Trial for SystemTrial<'_> {
    fn standby(&mut self) -> Result<()> {
        for ((timer, _), active) in TIMERS.into_iter().zip(self.active_timers) {
            if active {
                systemd(&["--system", "stop", timer])?;
            }
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
        // Wait for the actual writers and selector, not only the target's
        // stop job. Propagated PartOf jobs may still be in flight.
        let mut units = vec![
            APP_GROUP,
            "rustsimhub-app-select.service",
            "rustsimhub-core.service",
            "rustsimhub-edge.service",
            "rustsimhub-media.service",
        ];
        for optional in [
            "rustsimhub-forwarder.service",
            "rustsimhub-setup-ap.service",
            "rustsimhub-setup-edge.service",
        ] {
            match unit_property(optional, "LoadState")?.as_str() {
                "loaded" => units.push(optional),
                "not-found" => {}
                _ => return Err(Error::Denied),
            }
        }
        let mut stop = vec!["--system", "stop"];
        stop.extend(units.iter().copied());
        systemd(&stop)?;
        for unit in units {
            if !matches!(unit_state(unit)?.as_str(), "inactive" | "failed") {
                return Err(Error::Busy);
            }
        }
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
            &self.baseline
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

    fn confirm(&self) -> Result<bool> {
        if !self.auto_confirm {
            return Ok(false);
        }
        match crate::runtime_observation::confirmation::confirm_web_running_trial(
            self.request,
            self.manifest,
        ) {
            Ok(_) => Ok(true),
            Err(error) => {
                // An acknowledgement lost after publication is distinguishable
                // by the original ledger. Never re-run confirmation or revert it.
                let state = ReadOnlyState::observe()?;
                let (selected, _, attempted, pending) = state.runtime_version()?;
                if !attempted && !pending && selected == self.manifest {
                    Ok(true)
                } else {
                    Err(error)
                }
            }
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
            check_idle()?;
            self.stop()?;
            web_upload::reject_web_trial(self.request, self.manifest)?;
        }
        self.start()?;
        self.observe(false)?;
        if self.auto_confirm {
            discard_failed_confirmation()?;
        }
        Ok(())
    }

    fn restore_timers(&self) -> Result<()> {
        let mut last = Ok(());
        for ((timer, _), active) in TIMERS.into_iter().zip(self.active_timers) {
            if active {
                if let Err(error) = systemd(&["--system", "start", timer]) {
                    last = Err(error);
                }
            }
        }
        last
    }
}

fn discard_failed_confirmation() -> Result<()> {
    let dir = Path::new(super::running::WEB_SCRATCH);
    let meta = denied_io(fs::symlink_metadata(dir))?;
    if !meta.is_dir() || meta.uid() != 0 || meta.mode() & 0o7777 != 0o700 {
        return Err(Error::Denied);
    }
    // Only after the intact original version has been re-observed. This is
    // uncommitted scratch, never the updater ledger's recoverable state.next.
    let path = dir.join("confirmation.next");
    if Observation::capture(&path)?.stamp.is_some() {
        private_file(&path, 0, 0o600, 8192)?;
        denied_io(fs::remove_file(path))?;
        denied_io(File::open(dir).and_then(|dir| dir.sync_all()))?;
    }
    Ok(())
}

fn prepare<'a>(request: &'a str, manifest: &'a str) -> Result<SystemTrial<'a>> {
    if !rustsimhub_wire::UpdateUpload::valid_reference(request, manifest) {
        return Err(Error::Invalid);
    }
    let auto_confirm = Observation::capture(Path::new("/").join(WEB_HEALTH_DROP).as_path())?
        .stamp
        .is_some();
    if auto_confirm {
        super::running::Root::observe_web_ap()?;
    }
    check_idle()?;
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
    let mut active_timers = [false; 2];
    for ((timer, _), active) in TIMERS.into_iter().zip(&mut active_timers) {
        *active = match unit_state(timer)?.as_str() {
            "active" => true,
            "inactive" => false,
            _ => return Err(Error::Busy),
        };
    }
    Ok(SystemTrial {
        request,
        manifest,
        baseline,
        auto_confirm,
        active_timers,
    })
}

pub fn run_web_trial(request: &str, manifest: &str) -> Result<TrialResult> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    let _lock = runner_lock()?;
    execute(&mut prepare(request, manifest)?)
}

fn journal_directory(path: &Path, uid: u32) -> Result<()> {
    let meta = denied_io(fs::symlink_metadata(path))?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o7777 != 0o755 {
        return Err(Error::Denied);
    }
    Ok(())
}

fn read_run(path: &Path, uid: u32) -> Result<Option<UpdateRunRecord>> {
    journal_directory(path, uid)?;
    let file = path.join("current.json");
    if Observation::capture(&file)?.stamp.is_none() {
        return Ok(None);
    }
    let handle = private_file(&file, uid, 0o644, UPDATE_RUN_LIMIT as u64)?;
    let mut bytes = Vec::new();
    denied_io(
        handle
            .take(UPDATE_RUN_LIMIT as u64 + 1)
            .read_to_end(&mut bytes),
    )?;
    let run: UpdateRunRecord = serde_json::from_slice(&bytes).map_err(|_| Error::Integrity)?;
    if bytes.len() > UPDATE_RUN_LIMIT || !run.valid() {
        return Err(Error::Integrity);
    }
    Ok(Some(run))
}

fn write_run(path: &Path, uid: u32, run: &UpdateRunRecord) -> Result<()> {
    if !run.valid() {
        return Err(Error::Invalid);
    }
    let bytes = serde_json::to_vec(run).map_err(|_| Error::Invalid)?;
    publish_journal(path, uid, "current", &bytes)
}

fn publish_journal(path: &Path, uid: u32, name: &str, bytes: &[u8]) -> Result<()> {
    journal_directory(path, uid)?;
    let next = path.join(format!(".{name}.next"));
    if let Some(stamp) = Observation::capture(&next)?.stamp {
        let mode = stamp.mode & 0o7777;
        if ![0o600, 0o644].contains(&mode) {
            return Err(Error::Denied);
        }
        // An unrenamed journal has never authorized maintenance or replaced the
        // previous record. Only discard this exact private scratch entry.
        private_file(&next, uid, mode, UPDATE_RUN_LIMIT as u64)?;
        denied_io(fs::remove_file(&next))?;
    }
    crate::store::write_new(&next, bytes, false)?;
    let file = private_file(&next, uid, 0o600, UPDATE_RUN_LIMIT as u64)?;
    denied_io(file.set_permissions(fs::Permissions::from_mode(0o644)))?;
    denied_io(file.sync_all())?;
    denied_io(fs::rename(next, path.join(format!("{name}.json"))))?;
    denied_io(File::open(path).and_then(|dir| dir.sync_all()))
}

fn maintain_backups(journal: &Path, confirmed_update: bool) -> Result<()> {
    use crate::store::{read_root_release_key, Store};
    use rustsimhub_wire::{UpdateBackupDeletion, UPDATE_DELETE_BACKUP};
    web_upload::checked_spool()?;
    let mut store = Store::open_stable(Path::new(web_upload::APP), read_root_release_key()?, 6)?;
    let runtime = Path::new("/run/rustsimhub-app");
    let versions = Path::new("/opt/rustsimhub/versions");
    let sampled = store.retention_status(runtime, versions)?;
    let request = Path::new(web_upload::WEB_INBOX).join(UPDATE_DELETE_BACKUP);
    if Observation::capture(&request)?.stamp.is_some() {
        let file = private_file(
            &request,
            web_upload::CORE_UID,
            0o600,
            UPDATE_RUN_LIMIT as u64,
        )?;
        let mut bytes = Vec::new();
        denied_io(
            file.take(UPDATE_RUN_LIMIT as u64 + 1)
                .read_to_end(&mut bytes),
        )?;
        let deletion: UpdateBackupDeletion =
            serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Invalid)?
            .as_secs();
        if deletion.current == sampled.current
            && sampled.backups.contains(&deletion.backup)
            && (now..=now.saturating_add(90)).contains(&deletion.expires_unix_secs)
        {
            store.remove_retained_application(
                runtime,
                versions,
                deletion.backup.epoch,
                &deletion.backup.manifest_sha256,
            )?;
        }
    }
    if confirmed_update || !sampled.backups.is_empty() {
        store.prune_unreferenced_applications(runtime, versions)?;
    }
    let current = store.retention_status(runtime, versions)?;
    if !current.valid() {
        return Err(Error::Integrity);
    }
    let bytes = serde_json::to_vec(&current).map_err(|_| Error::Invalid)?;
    // Idle timers should not keep rewriting an unchanged public snapshot.
    let path = journal.join("backups.json");
    if crate::store::read_checked(&path, UPDATE_RUN_LIMIT).is_ok_and(|previous| previous == bytes) {
        return Ok(());
    }
    publish_journal(journal, 0, "backups", &bytes)
}

fn reconciled_phase(
    run: &UpdateRunRecord,
    selected: &str,
    epoch: u64,
    pending: bool,
) -> Option<UpdateRunPhase> {
    if !pending && selected == run.manifest_sha256 && epoch == run.epoch {
        Some(UpdateRunPhase::Confirmed)
    } else {
        None
    }
}

fn recover_run(run: &UpdateRunRecord) -> Result<UpdateRunPhase> {
    let state = ReadOnlyState::observe()?;
    let (selected, epoch, _, pending) = state.runtime_version()?;
    let confirmed = reconciled_phase(run, selected, epoch, pending);
    drop(state);
    let trial = SystemTrial {
        request: &run.request_id,
        manifest: &run.manifest_sha256,
        baseline: run.baseline_sha256.clone(),
        auto_confirm: true,
        active_timers: run.active_timers,
    };
    let result = if let Some(phase) = confirmed {
        Ok(phase)
    } else {
        trial.recover().map(|()| UpdateRunPhase::Restored)
    };
    let timers = trial.restore_timers();
    result.and_then(|phase| timers.map(|()| phase))
}

fn write_completed_run(
    journal: &Path,
    uid: u32,
    run: &UpdateRunRecord,
    refresh_retention: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if run.phase == UpdateRunPhase::Confirmed {
        // Status polling ends on a terminal receipt. Publish the current
        // version first so that response cannot pair success with the old slot.
        // A failed refresh leaves the Running journal for ledger reconciliation.
        refresh_retention()?;
    }
    write_run(journal, uid, run)
}

/// The systemd timer consumes the one fixed inbox. Crash recovery always
/// resolves the previous journal before looking for a new request.
pub fn process_web_upload() -> Result<()> {
    let uid = rustix::process::geteuid().as_raw();
    if uid != 0 {
        return Err(Error::Denied);
    }
    let _lock = runner_lock()?;
    let journal = Path::new(JOURNAL);
    let previous = match read_run(journal, uid)? {
        Some(mut run) if run.phase == UpdateRunPhase::Running => {
            run.phase = recover_run(&run)?;
            return write_completed_run(journal, uid, &run, || maintain_backups(journal, true));
        }
        previous => previous,
    };

    let Some(upload) = web_upload::pending_upload()? else {
        return maintain_backups(journal, false);
    };
    if let Some(run) = previous {
        if run.request_id == upload.request_id {
            return if run.matches(&upload) {
                maintain_backups(journal, run.phase == UpdateRunPhase::Confirmed)
            } else {
                Err(Error::Denied)
            };
        }
    }
    let mut trial = prepare(&upload.request_id, &upload.manifest_sha256)?;
    if !trial.auto_confirm {
        return Err(Error::Denied);
    }
    let mut run = UpdateRunRecord {
        format: 1,
        request_id: upload.request_id.clone(),
        manifest_sha256: upload.manifest_sha256.clone(),
        epoch: upload.epoch,
        baseline_sha256: trial.baseline.clone(),
        active_timers: trial.active_timers,
        phase: UpdateRunPhase::Running,
    };
    write_run(journal, uid, &run)?;
    run.phase = match execute(&mut trial)? {
        TrialResult::Confirmed => UpdateRunPhase::Confirmed,
        TrialResult::RejectedAndRestored => UpdateRunPhase::Restored,
        TrialResult::AwaitingHealthConfirmation => return Err(Error::Integrity),
    };
    write_completed_run(journal, uid, &run, || maintain_backups(journal, true))
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
        fn confirm(&self) -> Result<bool> {
            self.call("confirm")?;
            Ok(self.fail == "success")
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
                    "confirm",
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
    fn web_trial_confirms_only_after_the_health_handshake() {
        for (fail, expected) in [
            ("success", TrialResult::Confirmed),
            ("confirm", TrialResult::RejectedAndRestored),
        ] {
            let mut probe = Probe {
                fail,
                log: RefCell::new(vec![]),
            };
            assert_eq!(execute(&mut probe), Ok(expected));
            assert_eq!(&probe.log.borrow()[4..6], ["observe", "confirm"]);
        }
    }

    #[test]
    fn web_trial_journal_recovers_only_the_exact_confirmed_request() {
        let directory = tempfile::tempdir().unwrap();
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let uid = rustix::process::geteuid().as_raw();
        let run = UpdateRunRecord {
            format: 1,
            request_id: "12345678-1234-4234-8234-123456789abc".into(),
            manifest_sha256: "a".repeat(64),
            epoch: 115,
            baseline_sha256: "b".repeat(64),
            active_timers: [true, false],
            phase: UpdateRunPhase::Running,
        };
        write_run(directory.path(), uid, &run).unwrap();
        assert_eq!(read_run(directory.path(), uid).unwrap(), Some(run));
        let mut run = read_run(directory.path(), uid).unwrap().unwrap();
        assert_eq!(
            reconciled_phase(&run, &run.manifest_sha256, 114, false),
            None
        );
        assert_eq!(
            reconciled_phase(&run, &run.manifest_sha256, 115, true),
            None
        );
        assert_eq!(
            reconciled_phase(&run, &run.baseline_sha256, 115, false),
            None
        );
        let next = directory.path().join(".current.next");
        crate::store::write_new(&next, b"interrupted journal publication", false).unwrap();
        assert_eq!(
            read_run(directory.path(), uid).unwrap().unwrap().phase,
            UpdateRunPhase::Running
        );
        run.phase = reconciled_phase(&run, &run.manifest_sha256, 115, false).unwrap();
        write_run(directory.path(), uid, &run).unwrap();
        assert_eq!(read_run(directory.path(), uid).unwrap(), Some(run));
        assert!(!next.exists());
    }

    #[test]
    fn confirmed_receipt_waits_for_the_current_version_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let journal = directory.path();
        fs::set_permissions(journal, fs::Permissions::from_mode(0o755)).unwrap();
        let uid = rustix::process::geteuid().as_raw();
        let mut run = UpdateRunRecord {
            format: 1,
            request_id: "12345678-1234-4234-8234-123456789abc".into(),
            manifest_sha256: "a".repeat(64),
            epoch: 128,
            baseline_sha256: "b".repeat(64),
            active_timers: [true, false],
            phase: UpdateRunPhase::Running,
        };
        write_run(journal, uid, &run).unwrap();
        run.phase = UpdateRunPhase::Confirmed;
        assert_eq!(
            write_completed_run(journal, uid, &run, || Err(Error::Io)),
            Err(Error::Io)
        );
        assert_eq!(
            read_run(journal, uid).unwrap().unwrap().phase,
            UpdateRunPhase::Running
        );
        write_completed_run(journal, uid, &run, || {
            assert_eq!(
                read_run(journal, uid)?.unwrap().phase,
                UpdateRunPhase::Running
            );
            publish_journal(journal, uid, "backups", b"new current version")
        })
        .unwrap();
        assert_eq!(
            fs::read(journal.join("backups.json")).unwrap(),
            b"new current version"
        );
        assert_eq!(read_run(journal, uid).unwrap(), Some(run));
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
