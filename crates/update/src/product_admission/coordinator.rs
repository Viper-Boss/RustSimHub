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
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    kind: String,
    manifest_sha256: [String; 3],
    phase: Phase,
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
                if r.format != 1
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
    let checked = ProductDataAdmission::compare(&snapshot, selected, candidate, fallback)?;
    if checked.exit_code() == 4 {
        return Err(Error::Denied);
    }
    checked.revalidate()?;
    drop(checked);
    journal.begin("email4", [selected, candidate, fallback])?;
    journal.phase(Phase::Executing)?;
    let result = crate::admission::migrate_email4(
        store,
        snapshot,
        &candidate._manifest,
        &candidate._signature,
        &candidate._payloads,
    );
    match result {
        Ok(()) => journal.phase(Phase::Committed),
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
            ProductRuntimeModeV1::Recovery | ProductRuntimeModeV1::EmailRecovery
        )
        || !matches!(
            fallback.mode,
            ProductRuntimeModeV1::Recovery | ProductRuntimeModeV1::EmailRecovery
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
    channel.read_exact(&mut ack).map_err(|_| Error::Io)?;
    if ack != *b"R" {
        return Err(Error::Denied);
    }
    revalidate()?;
    channel.write_all(b"G").map_err(|_| Error::Io)?;
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
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
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
