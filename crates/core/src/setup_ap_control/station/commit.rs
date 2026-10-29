// SPDX-License-Identifier: AGPL-3.0-only
//! Current Owner confirmation and read-only recovery. No NM or arbitrary Root command.
use super::*;
use rustsimhub_setup_ap::channel::commit::{
    self as protocol, Intent as CommitIntent, Phase as CommitPhase, Status as CommitStatus,
};
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::setup_ap_control) struct Record {
    format: u16,
    trial: super::Record,
    intent: CommitIntent,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    format: u16,
    record: Record,
    root: CommitStatus,
}
fn file(dir: &Path, id: &str, suffix: &str) -> Result<PathBuf> {
    if !uuid(id) {
        return Err(Error::InvalidInput);
    }
    Ok(dir.join(format!("commit-{id}.{suffix}")))
}
fn validate(r: &Record) -> Result<()> {
    super::validate(&r.trial)?;
    if r.format != 1 || !r.intent.valid() || r.intent.trial != r.trial.intent {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(in crate::setup_ap_control) fn pending(dir: &Path) -> Result<Option<Record>> {
    let r: Option<Record> = read(&dir.join("commit.pending.json"))?;
    if let Some(r) = &r {
        validate(r)?;
    }
    Ok(r)
}
fn load(dir: &Path, id: &str) -> Result<Option<Record>> {
    let r: Option<Record> = read(&file(dir, id, "intent")?)?;
    if let Some(r) = &r {
        validate(r)?;
        if r.intent.commit_id != id {
            return Err(Error::Forbidden);
        }
    }
    Ok(r)
}
fn saved(dir: &Path, r: &Record) -> Result<Option<Saved>> {
    let s: Option<Saved> = read(&file(dir, &r.intent.commit_id, "result")?)?;
    if let Some(s) = &s {
        check_saved(s, r)?;
    }
    Ok(s)
}
fn check_saved(s: &Saved, r: &Record) -> Result<()> {
    denied(
        s.root
            .validate(s.root.sample_uptime_ms, &s.root.observation_boot_id),
    )?;
    if s.format != 1
        || s.record != *r
        || s.root.intent != r.intent
        || s.root.phase == CommitPhase::Saving
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn remove_pending(dir: &Path, r: &Record) -> Result<()> {
    if let Some(old) = pending(dir)? {
        if old != *r {
            return Err(Error::Forbidden);
        }
        fs::remove_file(dir.join("commit.pending.json")).map_err(|_| Error::Io)?;
        fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)?;
    }
    Ok(())
}
fn clear_payload(input: &Directory, r: &Record) -> Result<()> {
    if let Some(bytes) = denied(input.read(Slot::StationCommitRequest))? {
        let bytes = Zeroizing::new(bytes);
        let q: protocol::Request =
            serde_json::from_slice(&bytes).map_err(|_| Error::InvalidInput)?;
        if denied(q.intent())? != r.intent {
            return Err(Error::Forbidden);
        }
        denied(input.clear(Slot::StationCommitRequest))?;
    }
    Ok(())
}
fn query(input: &Directory, r: &Record) -> Result<()> {
    denied(input.publish(Slot::StationCommitQuery, &r.intent))
}
fn parent_matches(r: &Record, parent: &super::super::Record) -> bool {
    r.trial.parent_key_sha256 == parent.key_sha256
        && r.trial.owner == parent.owner
        && r.intent.trial.binding == parent.binding
}
fn completed_parent(input: &Directory, dir: &Path, r: &Record) -> Result<()> {
    clear_payload(input, r)?;
    remove_pending(dir, r)?;
    super::clear(input, &r.trial)?;
    super::remove_pending(dir, &r.trial)?;
    if let Some(parent) = Runtime::read_record(&dir.join("pending.json"))? {
        if !parent_matches(r, &parent) {
            return Err(Error::Forbidden);
        }
        fs::remove_file(dir.join("pending.json")).map_err(|_| Error::Io)?;
        fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)?;
    }
    Ok(())
}
pub(in crate::setup_ap_control) fn recover(
    input: &Directory,
    dir: &Path,
    parent: &mut Option<super::super::Record>,
) -> Result<()> {
    if let Some(p) = parent.as_ref() {
        let proof: Option<Saved> = read(&dir.join(format!("{}.committed", p.key_sha256)))?;
        if let Some(proof) = proof {
            validate(&proof.record)?;
            check_saved(&proof, &proof.record)?;
            if proof.root.phase != CommitPhase::Saved || !parent_matches(&proof.record, p) {
                return Err(Error::Forbidden);
            }
            completed_parent(input, dir, &proof.record)?;
            *parent = None;
        }
    }
    if let Some(r) = pending(dir)? {
        if load(dir, &r.intent.commit_id)?.as_ref() != Some(&r) {
            return Err(Error::Forbidden);
        }
        if parent.as_ref().is_none_or(|p| !parent_matches(&r, p)) {
            return Err(Error::Forbidden);
        }
        // A fresh Root observation may confirm an already completed save. This
        // query contains no credentials and never replays the old confirmation.
        clear_payload(input, &r)?;
        query(input, &r)?;
    }
    Ok(())
}
pub(in crate::setup_ap_control) fn parent_committed(
    dir: &Path,
    parent: &super::super::Record,
) -> Result<bool> {
    let proof: Option<Saved> = read(&dir.join(format!("{}.committed", parent.key_sha256)))?;
    let Some(proof) = proof else {
        return Ok(false);
    };
    validate(&proof.record)?;
    check_saved(&proof, &proof.record)?;
    if proof.root.phase != CommitPhase::Saved || !parent_matches(&proof.record, parent) {
        return Err(Error::Forbidden);
    }
    Ok(true)
}
fn owned(c: &Context, r: &Record, operation: &str) -> Result<()> {
    super::owned(c, &r.trial, operation)
}
fn request(
    r: &super::Record,
    id: String,
    ssid: String,
    passphrase: Zeroizing<String>,
    requested: u64,
    deadline: u64,
) -> protocol::Request {
    protocol::Request {
        format: 1,
        trial: Request {
            format: 1,
            binding: r.intent.binding.clone(),
            trial_id: r.intent.trial_id.clone(),
            owner_key_sha256: r.intent.owner_key_sha256.clone(),
            owner_generation: r.intent.owner_generation,
            requested_uptime_ms: r.intent.requested_uptime_ms,
            deadline_uptime_ms: r.intent.deadline_uptime_ms,
            operation: Operation::Test {
                ssid,
                passphrase: Secret::new(passphrase),
            },
        },
        commit_id: id,
        requested_uptime_ms: requested,
        deadline_uptime_ms: deadline,
    }
}
fn view(r: &Record, root: Option<&CommitStatus>, live: bool) -> Value {
    let phase = root.map_or(if live { "submitted" } else { "unknown" }, |s| {
        match s.phase {
            CommitPhase::Saving => "saving",
            CommitPhase::Saved => "saved",
            CommitPhase::RestoredAp => "restored_ap",
            CommitPhase::Unknown => "unknown",
        }
    });
    json!({"known":true,"operation_id":r.intent.trial.binding.request_id,"trial_id":r.intent.trial.trial_id,"commit_id":r.intent.commit_id,
        "state":phase,"settings_saved":root.is_some_and(|s|s.phase==CommitPhase::Saved),"authoritative_result":root.is_some_and(|s|s.phase!=CommitPhase::Saving),"acknowledged":root.is_some(),
        "historical":!live,"credentials_returned":false,"cellular_activation":false,"not_resubmitted":true})
}
impl Runtime {
    pub(in crate::setup_ap_control) fn commit_recover(&self, s: &Session) -> Result<()> {
        if let Some(r) = pending(&self.journal)? {
            if !parent_matches(&r, &s.record) {
                return Err(Error::Forbidden);
            }
            query(&self.input, &r)?;
        }
        Ok(())
    }
    pub(in crate::setup_ap_control) fn commit_tick(
        &self,
        s: &mut Session,
        time: u64,
    ) -> Result<()> {
        let Some(r) = pending(&self.journal)? else {
            return Ok(());
        };
        if !parent_matches(&r, &s.record) {
            return Err(Error::Forbidden);
        }
        let status = if let Some(raw) = denied(self.output.read(Slot::StationCommitStatus))? {
            serde_json::from_slice::<CommitStatus>(&raw)
                .ok()
                .filter(|v| {
                    v.intent == r.intent && v.validate(time, &boot().unwrap_or_default()).is_ok()
                })
        } else {
            None
        };
        if let Some(status) = status {
            clear_payload(&self.input, &r)?;
            if status.phase != CommitPhase::Saving {
                if let Some(old) = saved(&self.journal, &r)? {
                    if old.root.phase != status.phase
                        || old.root.execution_uptime_ms != status.execution_uptime_ms
                    {
                        return Err(Error::Forbidden);
                    }
                } else {
                    write(
                        &file(&self.journal, &r.intent.commit_id, "result")?,
                        &Saved {
                            format: 1,
                            record: r.clone(),
                            root: status.clone(),
                        },
                    )?;
                }
                if status.phase == CommitPhase::Saved {
                    let path = self
                        .journal
                        .join(format!("{}.committed", r.trial.parent_key_sha256));
                    if let Some(old) = read::<Saved>(&path)? {
                        check_saved(&old, &r)?;
                        if old.root.phase != CommitPhase::Saved {
                            return Err(Error::Forbidden);
                        }
                    } else {
                        write(
                            &path,
                            &Saved {
                                format: 1,
                                record: r.clone(),
                                root: status,
                            },
                        )?;
                    }
                    completed_parent(&self.input, &self.journal, &r)?;
                    s.phase = State::Closed;
                    s.cancelled = true;
                    s.token = None;
                    s.committed = true;
                    s.permit.take();
                } else if status.phase == CommitPhase::RestoredAp {
                    remove_pending(&self.journal, &r)?;
                }
                return Ok(());
            }
        } else if (time >= r.intent.deadline_uptime_ms
            || time.saturating_sub(r.intent.requested_uptime_ms) >= channel::HEARTBEAT_MS)
            && !s.cancelled
        {
            clear_payload(&self.input, &r)?;
            s.cancelled = true;
            s.token = None;
            self.publish_cancel(s)?;
            query(&self.input, &r)?;
        }
        Ok(())
    }
    pub(in crate::setup_ap_control) fn commit_cancel(
        &self,
        s: &mut Session,
        trial: &super::Record,
    ) -> Result<()> {
        if let Some(r) = pending(&self.journal)? {
            if r.trial == *trial && !s.cancelled {
                clear_payload(&self.input, &r)?;
                s.cancelled = true;
                s.token = None;
                self.publish_cancel(s)?;
                query(&self.input, &r)?;
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::setup_ap_control) async fn commit_begin(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        trial_id: String,
        id: String,
        ssid: String,
        passphrase: Zeroizing<String>,
        idle: &impl Idle,
    ) -> Result<Value> {
        if !uuid(&id) || !rustsimhub_wire::wifi_credentials_valid(&ssid, &passphrase) {
            return Err(Error::InvalidInput);
        }
        let c = owner(auth.clone(), token.clone(), true).await?;
        if let Some(old) = load(&self.journal, &id)? {
            owned(&c, &old, &operation)?;
            let q = request(
                &old.trial,
                id.clone(),
                ssid,
                passphrase,
                old.intent.requested_uptime_ms,
                old.intent.deadline_uptime_ms,
            );
            if trial_id != old.intent.trial.trial_id || denied(q.intent())? != old.intent {
                return Err(Error::Forbidden);
            }
            return self.commit_status(auth, token, operation, id).await;
        }
        let trial = super::load(&self.journal, &trial_id)?.ok_or(Error::Forbidden)?;
        super::owned(&c, &trial, &operation)?;
        let proof = super::saved(&self.journal, &trial)?
            .and_then(|v| v.root)
            .ok_or(Error::Forbidden)?;
        if proof.phase != Phase::Ready || super::cancel_mark(&self.journal, &trial)? {
            return Err(Error::Forbidden);
        }
        idle.check(&token).await?;
        let after = owner(auth.clone(), token, true).await?;
        if c.principal != after.principal
            || c.realm != after.realm
            || c.key != after.key
            || c.instance != after.instance
        {
            return Err(Error::Forbidden);
        }
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_mut().ok_or(Error::Forbidden)?;
        if self.rescan_blocks(s)? {
            return Err(Error::Busy);
        }
        let time = now()?;
        if !s.save_supported
            || s.cancelled
            || s.phase != State::Ready
            || s.committed
            || s.record.key_sha256 != trial.parent_key_sha256
            || s.record.owner != trial.owner
            || s.record.binding != trial.intent.binding
            || s.record.binding.core_instance != c.instance
            || pending(&self.journal)?.is_some()
            || denied(self.input.read(Slot::StationCommitRequest))?.is_some()
            || super::pending(&self.journal)?.is_some()
            || s.trial
                .as_ref()
                .is_none_or(|t| t.record != trial || !t.finished || t.cancelled)
        {
            return Err(Error::Forbidden);
        }
        let q = request(
            &trial,
            id.clone(),
            ssid,
            passphrase,
            time,
            time.checked_add(40_000).ok_or(Error::InvalidInput)?,
        );
        if denied(q.trial.intent())? != trial.intent {
            return Err(Error::Forbidden);
        }
        denied(q.validate(&s.record.binding, time))?;
        let parent = denied(self.output.read(Slot::Status))?.ok_or(Error::Forbidden)?;
        let parent = denied(Status::parse(&parent, &s.record.binding, time))?;
        if parent.state != State::Ready
            || parent.sequence < s.root_sequence
            || parent.sequence > s.sequence
        {
            return Err(Error::Forbidden);
        }
        if fs::read_dir(&self.journal)
            .map_err(|_| Error::Io)?
            .take(513)
            .count()
            >= 508
        {
            return Err(Error::Busy);
        }
        let r = Record {
            format: 1,
            trial,
            intent: denied(q.intent())?,
        };
        write(&file(&self.journal, &id, "intent")?, &r)?;
        write(&self.journal.join("commit.pending.json"), &r)?;
        if denied(self.input.publish(Slot::StationCommitRequest, &q)).is_err() {
            s.cancelled = true;
            s.token = None;
            let _ = self.publish_cancel(s);
            return Err(Error::Io);
        }
        Ok(view(&r, None, true))
    }
    pub(in crate::setup_ap_control) async fn commit_status(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        id: String,
    ) -> Result<Value> {
        let c = owner(auth, token, false).await?;
        let Some(r) = load(&self.journal, &id)? else {
            return Ok(
                json!({"known":false,"commit_id":id,"state":"not_found","not_found_is_not_proof":true}),
            );
        };
        owned(&c, &r, &operation)?;
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        if let Some(s) = active.as_mut().filter(|s| parent_matches(&r, &s.record)) {
            self.commit_tick(s, now()?)?;
        }
        let live = active.as_ref().is_some_and(|s| {
            parent_matches(&r, &s.record) && !s.cancelled && s.phase != State::Closed
        });
        if let Some(saved) = saved(&self.journal, &r)? {
            return Ok(view(&r, Some(&saved.root), live));
        }
        query(&self.input, &r)?;
        let fresh = denied(self.output.read(Slot::StationCommitStatus))?
            .and_then(|b| serde_json::from_slice::<CommitStatus>(&b).ok())
            .filter(|s| {
                s.intent == r.intent
                    && s.validate(now().unwrap_or(u64::MAX), &boot().unwrap_or_default())
                        .is_ok()
            });
        Ok(view(&r, fresh.as_ref(), live))
    }
}
#[cfg(test)]
mod tests;
