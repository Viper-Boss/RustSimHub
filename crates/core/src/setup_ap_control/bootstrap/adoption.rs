// SPDX-License-Identifier: AGPL-3.0-only
//! Exact just-claimed Owner inherits the original AP, deadline and network guard.
//! The immutable journal bridges the two pending filenames after an interruption.
use super::super::{Context as OwnerContext, Record as OwnerRecord, Session};
use super::*;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transfer {
    format: u16,
    bootstrap: Record,
    owner: OwnerRecord,
}
fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    rustsimhub_auth::write_private_new(
        path,
        &serde_json::to_vec(value).map_err(|_| Error::InvalidInput)?,
    )
}
fn sync(dir: &Path) -> Result<()> {
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)
}
/// Metadata recovery only: a recovered Owner session is cancelled by Runtime.
/// This never reconstructs a login, extends a lease or publishes Start.
pub(in crate::setup_ap_control) fn recover(dir: &Path) -> Result<()> {
    let Some(bootstrap) = Bootstrap::read(&dir.join("first-setup.pending.json"))? else {
        return Ok(());
    };
    let Some(recorded) = transfer_owner(dir, &bootstrap)? else {
        return Ok(());
    };
    let pending = dir.join("pending.json");
    if let Some(old) = Runtime::read_record(&pending)? {
        if old != recorded {
            return Err(Error::Forbidden);
        }
    } else {
        write(&pending, &recorded)?;
    }
    fs::remove_file(dir.join("first-setup.pending.json")).map_err(|_| Error::Io)?;
    sync(dir)
}
fn transfer_owner(dir: &Path, bootstrap: &Record) -> Result<Option<OwnerRecord>> {
    let Some(transfer): Option<Transfer> = station::read(&dir.join("first-setup.owner.json"))?
    else {
        return Ok(None);
    };
    if transfer.format != 1
        || transfer.bootstrap != *bootstrap
        || transfer.owner.binding != bootstrap.binding
        || transfer.owner.format != 1
        || transfer.owner.owner.generation == 0
        || transfer.owner.owner.scopes & !rustsimhub_policy::ALL_SCOPES != 0
        || transfer.owner.key_sha256.len() != 64
        || !transfer
            .owner
            .key_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Forbidden);
    }
    let intent = dir.join(format!("{}.intent", transfer.owner.key_sha256));
    if let Some(recorded) = Runtime::read_record(&intent)? {
        if transfer.owner != recorded {
            return Err(Error::Forbidden);
        }
    } else {
        write(&intent, &transfer.owner)?;
    }
    Ok(Some(transfer.owner))
}
/// Finish an interrupted live handoff using the exact original rollback proof.
/// The proof is persisted before clearing either pending filename.
pub(super) fn closed(dir: &Path, bootstrap: &Record, status: &Status) -> Result<()> {
    let Some(record) = transfer_owner(dir, bootstrap)? else {
        return Ok(());
    };
    denied(status.validate(&record.binding, status.sample_uptime_ms))?;
    if status.state != State::Closed {
        return Err(Error::Forbidden);
    }
    let proof = dir.join(format!("{}.closed", record.key_sha256));
    let existing: Option<Status> = station::read(&proof)?;
    if let Some(old) = existing {
        denied(old.validate(&record.binding, old.sample_uptime_ms))?;
        if old.state != State::Closed {
            return Err(Error::Forbidden);
        }
    } else {
        write(&proof, status)?;
    }
    if let Some(old) = Runtime::read_record(&dir.join("pending.json"))? {
        if old != record {
            return Err(Error::Forbidden);
        }
        fs::remove_file(dir.join("pending.json")).map_err(|_| Error::Io)?;
    }
    sync(dir)
}
fn matches_claim(c: &OwnerContext, context: &super::Context, record: &Record) -> bool {
    same(context, record)
        && context.window.claimed.as_ref() == Some(&c.principal)
        && c.principal.kind == Kind::Owner
        && c.instance == record.binding.core_instance
        && c.realm == record.binding.realm
        && c.key == record.binding.device_key_sha256
}
impl Runtime {
    pub(in crate::setup_ap_control) async fn adopt_bootstrap(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        digest: String,
        c: OwnerContext,
        idle: &impl Idle,
    ) -> Result<Value> {
        let (record, gate) = {
            let a = self.bootstrap.active.try_lock().map_err(|_| Error::Busy)?;
            let s = a.as_ref().ok_or(Error::Forbidden)?;
            if self.bootstrap.stopped.load(Ordering::SeqCst)
                || s.cancelled
                || !s.acknowledged
                || s.phase != State::Ready
            {
                return Err(Error::Forbidden);
            }
            (s.record.clone(), s.gate.clone().ok_or(Error::Forbidden)?)
        };
        let claimed = context(auth.clone(), gate.clone()).await?;
        if !matches_claim(&c, &claimed, &record) {
            return Err(Error::Forbidden);
        }
        idle.check(&token).await?;
        idle.device(gate.clone()).await?;
        let after = owner(auth.clone(), token.clone(), true).await?;
        let claimed = context(auth, gate).await?;
        if c.principal != after.principal || !matches_claim(&after, &claimed, &record) {
            return Err(Error::Forbidden);
        }
        // Support was read from the fresh availability at round start (the record binds its
        // material); Root no longer refreshes availability while the window runs.
        let fresh = self.availability(&after).ok().is_some_and(|a| {
            a.material_sha256 == record.binding.material_sha256
                && a.station_trial_supported
                && a.station_save_supported
        });
        if !fresh && !self.bootstrap.supported()? {
            return Err(Error::Forbidden);
        }
        if station::pending(&self.journal)?.is_some()
            || station::commit::pending(&self.journal)?.is_some()
            || super::super::rescan::pending(&self.journal)?.is_some()
            || denied(self.input.read(Slot::RescanRequest))?.is_some()
            || denied(self.input.read(Slot::StationRequest))?.is_some()
            || denied(self.input.read(Slot::StationCommitRequest))?.is_some()
        {
            return Err(Error::Busy);
        }
        let mut parent = self.session.try_lock().map_err(|_| Error::Busy)?;
        if parent.as_ref().is_some_and(|s| s.phase != State::Closed) {
            return Err(Error::Busy);
        }
        let mut bootstrap = self.bootstrap.active.try_lock().map_err(|_| Error::Busy)?;
        let s = bootstrap.as_mut().ok_or(Error::Forbidden)?;
        let time = now()?;
        if s.record != record
            || s.cancelled
            || s.phase != State::Ready
            || !s.acknowledged
            || s.permit.is_none()
            || self.bootstrap.stopped.load(Ordering::SeqCst)
            || boot()? != record.binding.boot_id
            || time >= record.binding.deadline_uptime_ms
        {
            return Err(Error::Forbidden);
        }
        let root = denied(self.output.read(Slot::Status))?.ok_or(Error::Forbidden)?;
        let root = denied(Status::parse(&root, &record.binding, time))?;
        if root.state != State::Ready
            || root.sequence < s.root_sequence
            || root.sequence > s.sequence
        {
            return Err(Error::Forbidden);
        }
        let pending = Bootstrap::read(&self.journal.join("first-setup.pending.json"))?
            .ok_or(Error::Forbidden)?;
        if pending != record {
            return Err(Error::Forbidden);
        }
        if fs::read_dir(&self.journal)
            .map_err(|_| Error::Io)?
            .take(513)
            .count()
            >= 509
        {
            return Err(Error::Busy);
        }
        let owned = OwnerRecord {
            format: 1,
            key_sha256: digest.clone(),
            binding: record.binding.clone(),
            owner: Owner::from(&after.principal),
        };
        write(
            &self.journal.join("first-setup.owner.json"),
            &Transfer {
                format: 1,
                bootstrap: record.clone(),
                owner: owned.clone(),
            },
        )?;
        write(&self.journal.join(format!("{digest}.intent")), &owned)?;
        write(&self.journal.join("pending.json"), &owned)?;
        fs::remove_file(self.journal.join("first-setup.pending.json")).map_err(|_| Error::Io)?;
        sync(&self.journal)?;
        // Hold both state locks until the original exclusive guard is moved.
        // The Root sees the same binding/sequence; no new Start is emitted.
        let mut old = bootstrap.take().ok_or(Error::Forbidden)?;
        *parent = Some(Session {
            record: owned,
            token: Some(token),
            phase: State::Ready,
            sequence: old.sequence,
            acknowledged: true,
            root_sequence: old.root_sequence,
            cancelled: false,
            permit: old.permit.take(),
            station_supported: true,
            save_supported: true,
            committed: false,
            trial: None,
            rescan: None,
            station_count: 0,
            refusal: None,
            first_setup: true,
            keepalive_ms: 0,
        });
        Ok(self.session_view(parent.as_ref().ok_or(Error::Forbidden)?))
    }
}
