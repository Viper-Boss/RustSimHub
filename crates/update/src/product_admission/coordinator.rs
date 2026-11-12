// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed maintenance coordination, not an install permission or HTTP endpoint.
//! Window8 supplies the ACTUAL stopped-Core/slot/data ownership handoff. The
//! existing snapshot cannot be dropped/re-observed and called the same guard.
use super::*;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{MetadataExt, OpenOptionsExt},
            net::UnixStream,
        },
    },
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    Executing,
    Committed,
    Unknown,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    kind: String,
    manifest_sha256: [String; 3],
    phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email_source_sha256: Option<String>,
}
/// Caller-owned serialized records cannot construct this live lock holder.
#[derive(Clone, Copy)]
pub enum UpgradeKind {
    Email4,
    Recovery9,
}
impl UpgradeKind {
    fn name(self) -> &'static str {
        match self {
            Self::Email4 => "email4",
            Self::Recovery9 => "recovery9",
        }
    }
}
pub struct UpgradeJournal {
    kind: &'static str,
    directory: PathBuf,
    _lock: File,
    record: Option<Record>,
}
impl UpgradeJournal {
    pub fn production(kind: UpgradeKind) -> Result<Self, Error> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        Self::at(
            &Path::new("/var/lib/rustsimhub/rustsimhub-v1/system/product-upgrade")
                .join(kind.name()),
            kind.name(),
        )
    }
    fn at(directory: &Path, kind: &'static str) -> Result<Self, Error> {
        let m = fs::symlink_metadata(directory).map_err(|_| Error::Io)?;
        if !m.is_dir()
            || m.uid() != rustix::process::geteuid().as_raw()
            || m.mode() & 0o7777 != 0o700
        {
            return Err(Error::Denied);
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(directory.join(".lock"))
            .map_err(|_| Error::Io)?;
        let m = lock.metadata().map_err(|_| Error::Io)?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != rustix::process::geteuid().as_raw()
            || m.mode() & 0o7777 != 0o600
        {
            return Err(Error::Denied);
        }
        lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
        let state = directory.join("state.json");
        let record = match OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(&state)
        {
            Ok(f) => {
                let m = f.metadata().map_err(|_| Error::Io)?;
                if !m.is_file()
                    || m.nlink() != 1
                    || m.uid() != rustix::process::geteuid().as_raw()
                    || m.mode() & 0o7777 != 0o600
                    || m.len() > 8192
                {
                    return Err(Error::Denied);
                }
                let mut b = Vec::new();
                f.take(8193).read_to_end(&mut b).map_err(|_| Error::Io)?;
                let r: Record = serde_json::from_slice(&b).map_err(|_| Error::Invalid)?;
                if !matches!(
                    (r.format, r.kind.as_str(), r.email_source_sha256.as_deref()),
                    (1, _, None) | (2, "email4", Some(_))
                ) || r
                    .email_source_sha256
                    .as_deref()
                    .is_some_and(|s| !crate::hex_digest(s))
                    || r.kind != kind
                    || r.manifest_sha256.iter().any(|s| !crate::hex_digest(s))
                {
                    return Err(Error::Invalid);
                }
                Some(r)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Error::Io),
        };
        Ok(Self {
            kind,
            directory: directory.into(),
            _lock: lock,
            record,
        })
    }
    fn save(&mut self) -> Result<(), Error> {
        let bytes = serde_json::to_vec(self.record.as_ref().ok_or(Error::Invalid)?)
            .map_err(|_| Error::Invalid)?;
        let pending = self.directory.join("state.next");
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&pending)
            .map_err(|_| Error::Busy)?;
        f.write_all(&bytes)
            .and_then(|_| f.sync_all())
            .map_err(|_| Error::Io)?;
        fs::rename(&pending, self.directory.join("state.json")).map_err(|_| Error::Io)?;
        File::open(&self.directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| Error::Io)
    }
    fn begin(&mut self, kind: &str, roles: [&SignedProductPrograms; 3]) -> Result<(), Error> {
        if self.record.is_some() || kind != self.kind {
            return Err(Error::Busy);
        }
        self.record = Some(Record {
            format: 1,
            kind: kind.into(),
            manifest_sha256: roles.map(|r| r.manifest_sha256.clone()),
            phase: Phase::Prepared,
            email_source_sha256: None,
        });
        self.save()
    }
    fn phase(&mut self, phase: Phase) -> Result<(), Error> {
        self.record.as_mut().ok_or(Error::Invalid)?.phase = phase;
        self.save()
    }
    pub fn inspect(&self) -> serde_json::Value {
        let phase = self.record.as_ref().map(|r| r.phase);
        // An executing record after restart is ambiguous even if the DB number
        // now looks new. Only the original commit acknowledgement marks success.
        serde_json::json!({"format":1,"read_only":true,"state":match phase{None=>"not_started",Some(Phase::Prepared)=>"not_started",Some(Phase::Committed)=>"committed",_=>"unknown"},"may_retry_automatically":false,"may_restore_database":false,"authorization_verified":false})
    }
}
/// Trusted in-process adapter, implemented by Window8 over retained Root/slot
/// fences. It may release only the original business writer into the original
/// UID986 migration actor, and must revalidate stop/Root facts at Root's G ack.
/// No implementation from JSON is provided by this module.
pub trait RecoveryMigrationHandoff {
    fn revalidate_before_handoff(&self) -> Result<(), Error>;
    fn with_business_writer_handoff(
        &mut self,
        snapshot: MaintenanceSnapshot,
        work: impl FnOnce(&Self) -> Result<(), Error>,
    ) -> Result<(), Error>;
    fn revalidate_before_commit(&self) -> Result<(), Error>;
}
/// Reuse the actual original email child, backup and R/G/D protocol. This
/// consumes the old snapshot exactly as the old API does; callers must not
/// pretend they still hold it afterwards. Email precedes recovery when both
/// migrations are required, because the old email adapter requires base6.
#[cfg(feature = "email-production-candidate")]
pub fn migrate_email4(
    store: &mut crate::store::Store,
    snapshot: MaintenanceSnapshot,
    selected: &SignedProductPrograms,
    candidate: &SignedProductPrograms,
    fallback: &SignedProductPrograms,
    journal: &mut UpgradeJournal,
) -> Result<(), Error> {
    migrate_email4_retained(store, snapshot, selected, candidate, fallback, journal).map(|_| ())
}

/// Combined transactions retain the original live guard after the same
/// original adapter. No re-observation, alternate sender or relaxed baseline.
#[cfg(feature = "email-production-candidate")]
pub fn migrate_email4_retained(
    store: &mut crate::store::Store,
    snapshot: MaintenanceSnapshot,
    selected: &SignedProductPrograms,
    candidate: &SignedProductPrograms,
    fallback: &SignedProductPrograms,
    journal: &mut UpgradeJournal,
) -> Result<MaintenanceSnapshot, Error> {
    let checked = ProductDataAdmission::compare(&snapshot, selected, candidate, fallback)?;
    if checked.exit_code() == 4 {
        return Err(Error::Denied);
    }
    checked.revalidate()?;
    drop(checked);
    let source = snapshot.email_source_digest()?;
    journal.begin("email4", [selected, candidate, fallback])?;
    // Same original Root journal, explicit format2: bind the actual source
    // read under this snapshot's still-held outbox writer, before G/handoff.
    let record = journal.record.as_mut().ok_or(Error::Invalid)?;
    record.format = 2;
    record.email_source_sha256 = Some(source);
    journal.phase(Phase::Executing)?;
    let result = crate::admission::migrate_email4_retained(
        store,
        snapshot,
        &candidate._manifest,
        &candidate._signature,
        &candidate._payloads,
    );
    match result {
        Ok(snapshot) => {
            journal.phase(Phase::Committed)?;
            Ok(snapshot)
        }
        Err(error) => {
            journal.phase(Phase::Unknown)?;
            Err(error)
        }
    }
}
/// Executes only the signature-verified candidate's sealed Core bytes and fixed
/// child option. Original Store migration code lives in the candidate's child,
/// not a new Root database writer. A missing shared CLI fails, never simulates.
pub struct RecoveryUpgradePlan<'a> {
    roles: [&'a SignedProductPrograms; 3],
    snapshot: MaintenanceSnapshot,
    permit: crate::product_recovery_child::Permit,
}
impl RecoveryUpgradePlan<'_> {
    pub(crate) fn permit(&self) -> crate::product_recovery_child::Permit {
        self.permit.clone()
    }
}
/// Takes ownership of the original held snapshot. The opaque plan keeps that
/// very guard alive continuously until the restricted writer handoff.
pub fn prepare_recovery9<'a>(
    snapshot: MaintenanceSnapshot,
    selected: &'a SignedProductPrograms,
    candidate: &'a SignedProductPrograms,
    fallback: &'a SignedProductPrograms,
    request_id: &str,
) -> Result<RecoveryUpgradePlan<'a>, Error> {
    let checked = ProductDataAdmission::compare(&snapshot, selected, candidate, fallback)?;
    if checked.exit_code() == 4 {
        return Err(Error::Denied);
    }
    let projected = serde_json::to_value(&checked.comparison).map_err(|_| Error::Invalid)?;
    if projected["projected_data"]["business_schema"] != 9
        || !matches!(
            candidate.mode,
            ProductRuntimeModeV1::Recovery
                | ProductRuntimeModeV1::EmailRecovery
                | ProductRuntimeModeV1::RecoveryHealth
        )
        || !matches!(
            fallback.mode,
            ProductRuntimeModeV1::Recovery
                | ProductRuntimeModeV1::EmailRecovery
                | ProductRuntimeModeV1::RecoveryHealth
        )
    {
        return Err(Error::Denied);
    }
    let permit =
        crate::product_recovery_child::prepare_root(request_id).map_err(|_| Error::Denied)?;
    checked.revalidate()?;
    drop(checked);
    Ok(RecoveryUpgradePlan {
        roles: [selected, candidate, fallback],
        snapshot,
        permit,
    })
}
pub fn migrate_recovery9(
    plan: RecoveryUpgradePlan<'_>,
    journal: &mut UpgradeJournal,
    handoff: &mut impl RecoveryMigrationHandoff,
) -> Result<(), Error> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    plan.snapshot.revalidate()?;
    handoff.revalidate_before_handoff()?;
    let actual = crate::product_recovery_child::prepare_root(&plan.permit.request_id)
        .map_err(|_| Error::Denied)?;
    if actual.source_schema != plan.permit.source_schema
        || actual.source_sha256 != plan.permit.source_sha256
        || actual.binding_sha256 != plan.permit.binding_sha256
    {
        return Err(Error::Busy);
    }
    journal.begin("recovery9", plan.roles)?;
    journal.phase(Phase::Executing)?;
    let bytes = &plan.roles[1]._payloads[0];
    let result = handoff.with_business_writer_handoff(plan.snapshot, |held| {
        run_recovery_child(bytes, &plan.permit, || held.revalidate_before_commit())
    });
    match result {
        Ok(()) => journal.phase(Phase::Committed),
        Err(e) => {
            #[cfg(test)]
            eprintln!("recovery handoff refused: {e:?}");
            journal.phase(Phase::Unknown)?;
            Err(e)
        }
    }
}

fn run_recovery_child(
    bytes: &[u8],
    permit: &crate::product_recovery_child::Permit,
    revalidate: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    use rustix::fs::{fchmod, fcntl_add_seals, memfd_create, MemfdFlags, Mode, SealFlags};
    use rustix::thread::{capabilities, CapabilitySet};
    if !capabilities(None)
        .map_err(|_| Error::Denied)?
        .effective
        .contains(CapabilitySet::KILL | CapabilitySet::SETUID | CapabilitySet::SETGID)
    {
        return Err(Error::Denied);
    }
    let mut file = File::from(
        memfd_create(
            c"rustsimhub-product-migration",
            MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
        )
        .map_err(|_| Error::Io)?,
    );
    file.write_all(bytes).map_err(|_| Error::Io)?;
    fchmod(&file, Mode::from_raw_mode(0o555)).map_err(|_| Error::Denied)?;
    fcntl_add_seals(
        &file,
        SealFlags::SEAL | SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK,
    )
    .map_err(|_| Error::Denied)?;
    let (parent, child) = UnixStream::pair().map_err(|_| Error::Io)?;
    parent
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| Error::Io)?;
    parent
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|_| Error::Io)?;
    rustix::thread::set_no_new_privs(true).map_err(|_| Error::Denied)?;
    #[cfg(test)]
    eprintln!("recovery parent: spawn");
    let mut child = Reap(
        Command::new("/proc/self/fd/1")
            .arg0("rustsimhub-product-migration")
            .arg("--migrate-product-recovery-child")
            .uid(986)
            .gid(986)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::from(OwnedFd::from(child)))
            .stdout(Stdio::from(file))
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error::Denied)?,
    );
    let mut channel = parent;
    let raw = serde_json::to_vec(permit).map_err(|_| Error::Invalid)?;
    channel
        .write_all(&(raw.len() as u32).to_be_bytes())
        .and_then(|_| channel.write_all(&raw))
        .map_err(|_| Error::Io)?;
    let mut ack = [0];
    #[cfg(test)]
    eprintln!("recovery parent: read-R");
    channel.read_exact(&mut ack).map_err(|_| Error::Io)?;
    if ack != *b"R" {
        return Err(Error::Denied);
    }
    #[cfg(test)]
    eprintln!("recovery parent: revalidate-R");
    revalidate()?;
    channel.write_all(b"G").map_err(|_| Error::Io)?;
    #[cfg(test)]
    eprintln!("recovery parent: read-D");
    channel.read_exact(&mut ack).map_err(|_| Error::Integrity)?;
    if ack != *b"D" {
        return Err(Error::Integrity);
    }
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(s) = child.0.try_wait().map_err(|_| Error::Io)? {
            return if s.success() {
                Ok(())
            } else {
                Err(Error::Integrity)
            };
        }
        if Instant::now() >= until {
            return Err(Error::Integrity);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

type JournalStamp = (u64, u64, u32, u32, u32, u64, u64, i64, i64, i64, i64);
fn journal_stamp(m: std::fs::Metadata) -> JournalStamp {
    (
        m.dev(),
        m.ino(),
        m.mode(),
        m.uid(),
        m.gid(),
        m.nlink(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
pub(crate) struct CompletedUpgradeJournal {
    journal: UpgradeJournal,
    directory: File,
    directory_stamp: JournalStamp,
    state: File,
    state_stamp: JournalStamp,
    lock_stamp: JournalStamp,
}
impl UpgradeJournal {
    pub(crate) fn completed(self) -> Result<CompletedUpgradeJournal, Error> {
        if self.kind != "recovery9" {
            return Err(Error::Denied);
        }
        self.into_completed()
    }
    pub(crate) fn completed_email(self) -> Result<CompletedUpgradeJournal, Error> {
        if self.kind != "email4"
            || self
                .record
                .as_ref()
                .is_none_or(|r| r.format != 2 || r.email_source_sha256.is_none())
        {
            return Err(Error::Denied);
        }
        self.into_completed()
    }
    fn into_completed(self) -> Result<CompletedUpgradeJournal, Error> {
        if self
            .record
            .as_ref()
            .is_none_or(|r| r.phase != Phase::Committed)
        {
            return Err(Error::Denied);
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::DIRECTORY).bits() as i32,
            )
            .open(&self.directory)
            .map_err(|_| Error::Io)?;
        let directory_stamp = journal_stamp(directory.metadata().map_err(|_| Error::Io)?);
        let mut state = OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(self.directory.join("state.json"))
            .map_err(|_| Error::Io)?;
        let meta = state.metadata().map_err(|_| Error::Io)?;
        if !meta.is_file()
            || meta.nlink() != 1
            || meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o7777 != 0o600
            || meta.len() > 8192
        {
            return Err(Error::Denied);
        }
        let state_stamp = journal_stamp(meta);
        let mut raw = Vec::new();
        std::io::Read::by_ref(&mut state)
            .take(8193)
            .read_to_end(&mut raw)
            .map_err(|_| Error::Io)?;
        let actual: Record = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
        if self.record.as_ref() != Some(&actual) {
            return Err(Error::Busy);
        }
        let lock_stamp = journal_stamp(self._lock.metadata().map_err(|_| Error::Io)?);
        let result = CompletedUpgradeJournal {
            journal: self,
            directory,
            directory_stamp,
            state,
            state_stamp,
            lock_stamp,
        };
        result.revalidate()?;
        Ok(result)
    }
}
impl CompletedUpgradeJournal {
    pub(crate) fn email_source(&self) -> Result<&str, Error> {
        self.revalidate()?;
        if self.journal.kind != "email4" {
            return Err(Error::Denied);
        }
        self.journal
            .record
            .as_ref()
            .and_then(|r| r.email_source_sha256.as_deref())
            .ok_or(Error::Denied)
    }
    pub(crate) fn revalidate(&self) -> Result<(), Error> {
        if journal_stamp(self.directory.metadata().map_err(|_| Error::Io)?) != self.directory_stamp
            || journal_stamp(fs::symlink_metadata(&self.journal.directory).map_err(|_| Error::Io)?)
                != self.directory_stamp
            || journal_stamp(self.state.metadata().map_err(|_| Error::Io)?) != self.state_stamp
            || journal_stamp(
                fs::symlink_metadata(self.journal.directory.join("state.json"))
                    .map_err(|_| Error::Io)?,
            ) != self.state_stamp
            || journal_stamp(self.journal._lock.metadata().map_err(|_| Error::Io)?)
                != self.lock_stamp
            || journal_stamp(
                fs::symlink_metadata(self.journal.directory.join(".lock"))
                    .map_err(|_| Error::Io)?,
            ) != self.lock_stamp
        {
            return Err(Error::Busy);
        }
        for entry in fs::read_dir(&self.journal.directory).map_err(|_| Error::Io)? {
            let name = entry.map_err(|_| Error::Io)?.file_name();
            if name != ".lock" && name != "state.json" {
                return Err(Error::Busy);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn completed_acknowledgement_retains_mutex_and_rejects_replaced_or_pending_files() {
        for change in ["state", "lock", "pending", "directory"] {
            let parent = tempfile::tempdir().unwrap();
            let directory = parent.path().join("recovery9");
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            let mut journal = UpgradeJournal::at(&directory, "recovery9").unwrap();
            journal.record = Some(Record {
                format: 1,
                kind: "recovery9".into(),
                manifest_sha256: ["a".repeat(64), "b".repeat(64), "c".repeat(64)],
                phase: Phase::Committed,
                email_source_sha256: None,
            });
            journal.save().unwrap();
            let completed = journal.completed().unwrap();
            completed.revalidate().unwrap();
            assert!(matches!(
                UpgradeJournal::at(&directory, "recovery9"),
                Err(Error::Busy)
            ));
            match change {
                "state" | "lock" => {
                    let name = if change == "state" {
                        "state.json"
                    } else {
                        ".lock"
                    };
                    let next = directory.join("replacement");
                    fs::write(&next, fs::read(directory.join(name)).unwrap()).unwrap();
                    fs::set_permissions(&next, fs::Permissions::from_mode(0o600)).unwrap();
                    fs::rename(next, directory.join(name)).unwrap();
                }
                "pending" => fs::write(directory.join("state.next"), b"uncertain").unwrap(),
                "directory" => {
                    fs::rename(&directory, parent.path().join("previous")).unwrap();
                    fs::create_dir(&directory).unwrap();
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(completed.revalidate().is_err(), "{change}");
        }
    }

    #[test]
    fn email_backup_provenance_requires_the_explicit_committed_source_record() {
        for modern in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let mut journal = UpgradeJournal::at(directory.path(), "email4").unwrap();
            journal.record = Some(Record {
                format: if modern { 2 } else { 1 },
                kind: "email4".into(),
                manifest_sha256: ["a".repeat(64), "b".repeat(64), "c".repeat(64)],
                phase: Phase::Committed,
                email_source_sha256: modern.then(|| "d".repeat(64)),
            });
            journal.save().unwrap();
            drop(journal);
            let result = UpgradeJournal::at(directory.path(), "email4")
                .unwrap()
                .completed_email();
            if modern {
                assert_eq!(result.unwrap().email_source().unwrap(), "d".repeat(64));
            } else {
                assert!(result.is_err());
            }
        }
    }
    #[test]
    fn journal_reopen_keeps_executing_unknown_and_never_resubmits() {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let mut j = UpgradeJournal::at(d.path(), "recovery9").unwrap();
        j.record = Some(Record {
            format: 1,
            kind: "recovery9".into(),
            manifest_sha256: ["a".repeat(64), "b".repeat(64), "c".repeat(64)],
            phase: Phase::Executing,
            email_source_sha256: None,
        });
        j.save().unwrap();
        assert_eq!(j.inspect()["state"], "unknown");
        drop(j);
        let mut j = UpgradeJournal::at(d.path(), "recovery9").unwrap();
        assert_eq!(j.inspect()["state"], "unknown");
        j.phase(Phase::Committed).unwrap();
        drop(j);
        assert_eq!(
            UpgradeJournal::at(d.path(), "recovery9").unwrap().inspect()["state"],
            "committed"
        );
    }
}

// A completed on-disk acknowledgement is retained only with its original
// mutex, actual descriptors and exact generations. It never authorizes data
// migration and cannot be reconstructed from inspect() JSON.
