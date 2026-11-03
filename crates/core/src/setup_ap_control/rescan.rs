// SPDX-License-Identifier: AGPL-3.0-only
//! Owner-only producer for bounded Root rescans. Recovery only cancels/queries an exact intent.
use super::*;
use rustsimhub_setup_ap::channel::rescan::{
    self as protocol, Action as ScanAction, Intent, Phase, Request, Status as ScanStatus,
};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    format: u16,
    parent_key_sha256: String,
    owner: Owner,
    intent: Intent,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    format: u16,
    record: Record,
    root: ScanStatus,
}
pub(super) struct Scan {
    record: Record,
    phase: Phase,
    acknowledged: bool,
    finished: bool,
}
fn file(dir: &Path, id: &str, suffix: &str) -> Result<PathBuf> {
    if !station::uuid(id) {
        return Err(Error::InvalidInput);
    }
    Ok(dir.join(format!("rescan-{id}.{suffix}")))
}
fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    rustsimhub_auth::write_private_new(
        path,
        &serde_json::to_vec(value).map_err(|_| Error::InvalidInput)?,
    )
}
fn validate(r: &Record) -> Result<()> {
    if r.format != 1
        || !r.intent.valid()
        || r.owner.generation != r.intent.owner_generation
        || hash(&r.owner.key) != r.intent.owner_key_sha256
        || r.owner.scopes & !rustsimhub_policy::ALL_SCOPES != 0
        || r.parent_key_sha256.len() != 64
        || !r
            .parent_key_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn load(dir: &Path, id: &str) -> Result<Option<Record>> {
    let r: Option<Record> = station::read(&file(dir, id, "intent")?)?;
    if let Some(r) = &r {
        validate(r)?;
        if r.intent.scan_id != id {
            return Err(Error::Forbidden);
        }
    }
    Ok(r)
}
pub(super) fn pending(dir: &Path) -> Result<Option<Record>> {
    let r: Option<Record> = station::read(&dir.join("rescan.pending.json"))?;
    if let Some(r) = &r {
        validate(r)?;
        if load(dir, &r.intent.scan_id)?.as_ref() != Some(r) {
            return Err(Error::Forbidden);
        }
    }
    Ok(r)
}
fn known(s: &ScanStatus) -> bool {
    matches!(s.phase, Phase::Ready | Phase::Failed | Phase::Refused)
}
fn saved(dir: &Path, r: &Record) -> Result<Option<Saved>> {
    let s: Option<Saved> = station::read(&file(dir, &r.intent.scan_id, "result")?)?;
    if let Some(s) = &s {
        denied(
            s.root
                .validate(s.root.sample_uptime_ms, &s.root.sample_boot_id),
        )?;
        if s.format != 1
            || s.record != *r
            || s.root.intent != r.intent
            || s.root.phase == Phase::Scanning
        {
            return Err(Error::Forbidden);
        }
    }
    Ok(s)
}
fn save(dir: &Path, r: &Record, root: ScanStatus) -> Result<()> {
    if root.intent != r.intent || root.phase == Phase::Scanning {
        return Err(Error::Forbidden);
    }
    denied(root.validate(root.sample_uptime_ms, &root.sample_boot_id))?;
    if saved(dir, r)?.is_none() {
        write(
            &file(dir, &r.intent.scan_id, "result")?,
            &Saved {
                format: 1,
                record: r.clone(),
                root,
            },
        )?;
    }
    Ok(())
}
fn marker(dir: &Path, r: &Record, suffix: &str) -> Result<bool> {
    let m: Option<Record> = station::read(&file(dir, &r.intent.scan_id, suffix)?)?;
    match m {
        Some(old) if old == *r => Ok(true),
        Some(_) => Err(Error::Forbidden),
        None => Ok(false),
    }
}
fn mark(dir: &Path, r: &Record, suffix: &str) -> Result<()> {
    if !marker(dir, r, suffix)? {
        write(&file(dir, &r.intent.scan_id, suffix)?, r)?;
    }
    Ok(())
}
fn request(r: &Record, action: ScanAction) -> Result<Request> {
    let i = &r.intent;
    let q = Request {
        format: 1,
        binding: i.binding.clone(),
        scan_id: i.scan_id.clone(),
        owner_key_sha256: i.owner_key_sha256.clone(),
        owner_generation: i.owner_generation,
        requested_uptime_ms: i.requested_uptime_ms,
        deadline_uptime_ms: i.deadline_uptime_ms,
        action,
    };
    if denied(q.intent())? != r.intent {
        return Err(Error::Forbidden);
    }
    Ok(q)
}
fn slot(input: &Directory, r: &Record) -> Result<Option<Request>> {
    let Some(raw) = denied(input.read(Slot::RescanRequest))? else {
        return Ok(None);
    };
    let q: Request = serde_json::from_slice(&raw).map_err(|_| Error::InvalidInput)?;
    if denied(q.intent())? != r.intent {
        return Err(Error::Forbidden);
    }
    Ok(Some(q))
}
fn clear(input: &Directory, r: &Record) -> Result<()> {
    if slot(input, r)?.is_some() {
        denied(input.clear(Slot::RescanRequest))?;
    }
    Ok(())
}
fn cancel(input: &Directory, dir: &Path, r: &Record) -> Result<()> {
    mark(dir, r, "cancel")?;
    slot(input, r)?;
    if r.intent.binding.boot_id == boot()? && now()? < r.intent.binding.deadline_uptime_ms {
        denied(input.publish(Slot::RescanRequest, &request(r, ScanAction::Cancel)?))?;
    } else {
        clear(input, r)?;
    }
    Ok(())
}
fn query(input: &Directory, dir: &Path, r: &Record) -> Result<()> {
    // An historical read must not starve the active scan's single Root result slot.
    if pending(dir)?.as_ref().is_none_or(|p| p == r) {
        denied(input.publish(Slot::RescanQuery, &r.intent))?;
    }
    Ok(())
}
fn remove_pending(dir: &Path, r: &Record) -> Result<()> {
    if let Some(p) = pending(dir)? {
        if p != *r {
            return Err(Error::Forbidden);
        }
        fs::remove_file(dir.join("rescan.pending.json")).map_err(|_| Error::Io)?;
        fs::File::open(dir)
            .and_then(|f| f.sync_all())
            .map_err(|_| Error::Io)?;
    }
    Ok(())
}
fn parent_matches(r: &Record, p: &super::Record) -> bool {
    r.parent_key_sha256 == p.key_sha256 && r.owner == p.owner && r.intent.binding == p.binding
}
pub(super) fn recover(
    input: &Directory,
    dir: &Path,
    parent: Option<&super::Record>,
) -> Result<Option<Scan>> {
    let Some(r) = pending(dir)? else {
        return Ok(None);
    };
    if parent.is_none_or(|p| !parent_matches(&r, p)) {
        // Compatible cleanup is allowed only with an exact authoritative child result and
        // the original parent's close proof. Unknown is never silently archived.
        let closed: Status = station::read(&dir.join(format!("{}.closed", r.parent_key_sha256)))?
            .ok_or(Error::Forbidden)?;
        denied(closed.validate(&r.intent.binding, closed.sample_uptime_ms))?;
        if parent.is_some()
            || closed.state != State::Closed
            || !saved(dir, &r)?.is_some_and(|s| known(&s.root))
        {
            return Err(Error::Forbidden);
        }
        clear(input, &r)?;
        remove_pending(dir, &r)?;
        return Ok(None);
    }
    let old = saved(dir, &r)?;
    if old.as_ref().is_some_and(|s| known(&s.root)) {
        clear(input, &r)?;
        remove_pending(dir, &r)?;
        return Ok(None);
    }
    cancel(input, dir, &r)?;
    query(input, dir, &r)?;
    Ok(Some(Scan {
        record: r,
        phase: old.map_or(Phase::Unknown, |s| s.root.phase),
        acknowledged: false,
        finished: false,
    }))
}
fn owned(c: &Context, r: &Record, operation: &str) -> Result<()> {
    if r.intent.binding.request_id != operation
        || Owner::from(&c.principal) != r.owner
        || r.intent.binding.realm != c.realm
        || r.intent.binding.device_key_sha256 != c.key
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn still_owner(auth: &mut Auth, c: &Context, token: &str) -> Result<()> {
    if auth.recent_owner(token)? != c.principal
        || auth.authorize(token, Scope::NetworkManage)? != c.principal
        || auth.durable_realm()? != c.realm
        || hash(&auth.public_key()) != c.key
        || auth.observer_instance() != c.instance
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
impl Runtime {
    pub(super) fn rescan_blocks(&self, s: &Session) -> Result<bool> {
        Ok(s.rescan
            .as_ref()
            .is_some_and(|scan| !scan.finished || scan.phase == Phase::Unknown)
            || pending(&self.journal)?.is_some())
    }
    pub(super) fn rescan_capability(&self, s: &Session) -> Value {
        let c = (|| -> Result<protocol::Capability> {
            let raw = denied(self.output.read(Slot::RescanCapability))?.ok_or(Error::Forbidden)?;
            denied(protocol::Capability::parse(
                &raw,
                &s.record.binding,
                now()?,
                &boot()?,
            ))
        })();
        match c {
            Ok(c) => {
                let local_busy = self.rescan_blocks(s).unwrap_or(true)
                    || station::pending(&self.journal).map_or(true, |p| p.is_some())
                    || station::commit::pending(&self.journal).map_or(true, |p| p.is_some());
                let end_trial = station::rescan_trial_may_end(s) && !local_busy;
                let available = (c.available
                    || c.refusal == Some(protocol::Refusal::Conflict) && end_trial)
                    && !s.cancelled
                    && s.phase == State::Ready
                    && s.permit.is_some()
                    && !local_busy;
                json!({"supported":true,"available":available,"remaining":c.remaining,
                    "refusal":if available {None}else if local_busy||s.cancelled||s.phase!=State::Ready {Some(protocol::Refusal::Conflict)}else{c.refusal},
                    "root_refusal":c.refusal,"will_end_unsaved_trial":end_trial,
                    "cooldown_remaining_ms":c.retry_after_uptime_ms.map(|t|t.saturating_sub(now().unwrap_or(0))),"max_per_parent":c.max_per_parent})
            }
            Err(_) => json!({"supported":false,"available":false}),
        }
    }
    fn rescan_view(&self, r: &Record, s: Option<&Session>) -> Result<Value> {
        let saved = saved(&self.journal, r)?;
        let active = s.and_then(|s| s.rescan.as_ref()).filter(|t| t.record == *r);
        let root = saved.as_ref().map(|s| &s.root);
        let phase = root
            .map(|s| s.phase)
            .or_else(|| active.map(|s| s.phase))
            .unwrap_or(Phase::Unknown);
        let authoritative = root.is_some_and(known);
        let live_parent = s.is_some_and(|s| {
            s.record.binding == r.intent.binding
                && !s.cancelled
                && s.phase == State::Ready
                && r.intent.binding.boot_id == boot().unwrap_or_default()
                && now().is_ok_and(|t| t < r.intent.binding.deadline_uptime_ms)
        });
        // A parent can host several rounds. Its binding alone cannot make an
        // earlier immutable receipt the current round's network snapshot.
        let live = live_parent && active.is_some();
        let capability = s.filter(|_| live_parent).map(|s| self.rescan_capability(s));
        let parent_expires = live_parent.then(|| {
            r.intent
                .binding
                .deadline_uptime_ms
                .saturating_sub(now().unwrap_or(u64::MAX))
                / 1000
        });
        let cancelled = marker(&self.journal, r, "cancel")?;
        let state = match phase {
            Phase::Ready => "ready",
            Phase::Failed if cancelled => "cancelled",
            Phase::Failed => "failed",
            Phase::Refused => "refused",
            Phase::Scanning if active.is_some_and(|t| !t.acknowledged) => "submitting",
            Phase::Scanning => "scanning",
            Phase::Unknown => "unknown",
        };
        Ok(
            json!({"known":true,"operation_id":r.intent.binding.request_id,"scan_id":r.intent.scan_id,"state":state,
            "authoritative_result":authoritative,"root_observed":root.is_some()||active.is_some_and(|t|t.acknowledged),
            "execution_started":if phase==Phase::Refused {Some(false)}else if phase==Phase::Ready {Some(true)}else{None},
            "refusal":root.and_then(|s|s.refusal),"cancel_requested":cancelled,"historical":!live,
            "ap_return_verified":live&&authoritative&&root.is_some_and(|s|s.ap_https_verified),
            "snapshot":if authoritative&&phase==Phase::Ready {root.and_then(|s|s.snapshot.as_ref())}else{None},
            "scan":capability,"parent_expires_in_seconds":parent_expires,
            "settings_saved":false,"credentials_returned":false,"cellular_activation":false}),
        )
    }
    fn rescan_accept_result(&self, r: &Record, root: ScanStatus) -> Result<()> {
        if root.intent != r.intent || root.phase == Phase::Scanning {
            return Err(Error::Forbidden);
        }
        save(&self.journal, r, root.clone())?;
        if known(&root) && pending(&self.journal)?.as_ref().is_none_or(|p| p == r) {
            clear(&self.input, r)?;
            remove_pending(&self.journal, r)?;
        }
        Ok(())
    }
    fn rescan_observation(&self, r: &Record) -> Result<Option<ScanStatus>> {
        let Some(raw) = denied(self.output.read(Slot::RescanStatus))? else {
            return Ok(None);
        };
        Ok(ScanStatus::parse(&raw, &r.intent, now()?, &boot()?).ok())
    }
    pub(super) fn rescan_abort(&self, s: &mut Session) -> Result<()> {
        if let Some(t) = s
            .rescan
            .as_ref()
            .filter(|t| !t.finished || t.phase == Phase::Unknown)
        {
            if pending(&self.journal)?.as_ref() == Some(&t.record) {
                cancel(&self.input, &self.journal, &t.record)?;
            } else {
                mark(&self.journal, &t.record, "cancel")?;
            }
            query(&self.input, &self.journal, &t.record)?;
        }
        Ok(())
    }
    pub(super) fn rescan_tick(&self, s: &mut Session, time: u64) -> Result<()> {
        let mut parent_cancel = false;
        if let Some(t) = s.rescan.as_mut() {
            if t.finished {
                return Ok(());
            }
            if let Some(root) = self.rescan_observation(&t.record)? {
                t.acknowledged = true;
                t.phase = root.phase;
                if root.phase != Phase::Scanning {
                    self.rescan_accept_result(&t.record, root)?;
                    t.finished = true;
                    if t.phase == Phase::Unknown {
                        parent_cancel = true;
                    }
                }
            } else {
                query(&self.input, &self.journal, &t.record)?;
                if time >= t.record.intent.deadline_uptime_ms
                    || !t.acknowledged
                        && time.saturating_sub(t.record.intent.requested_uptime_ms)
                            >= channel::HEARTBEAT_MS
                {
                    t.phase = Phase::Unknown;
                    cancel(&self.input, &self.journal, &t.record)?;
                    parent_cancel = true;
                }
            }
        }
        if parent_cancel && !s.cancelled {
            s.cancelled = true;
            s.token = None;
            self.publish_cancel(s)?;
        }
        Ok(())
    }
    pub(super) fn rescan_parent_closed(&self, s: &mut Session) -> Result<bool> {
        self.rescan_tick(s, now()?)?;
        if self.rescan_blocks(s)? {
            if let Some(t) = s.rescan.as_ref() {
                query(&self.input, &self.journal, &t.record)?;
            }
            s.cancelled = true;
            s.token = None;
            s.phase = State::Unknown;
            return Ok(false);
        }
        Ok(true)
    }
    pub(super) async fn rescan_begin(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        id: String,
        idle: &impl Idle,
    ) -> Result<Value> {
        if !station::uuid(&id) {
            return Err(Error::InvalidInput);
        }
        let c = owner(auth.clone(), token.clone(), true).await?;
        if let Some(r) = load(&self.journal, &id)? {
            owned(&c, &r, &operation)?;
            return self.rescan_status(auth, token, operation, id).await;
        }
        idle.check(&token).await?;
        let after = owner(auth.clone(), token.clone(), true).await?;
        if c.principal != after.principal
            || c.realm != after.realm
            || c.key != after.key
            || c.instance != after.instance
        {
            return Err(Error::Forbidden);
        }
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_mut().ok_or(Error::Forbidden)?;
        let time = now()?;
        if s.record.binding.request_id != operation
            || s.record.owner != Owner::from(&c.principal)
            || s.record.binding.realm != c.realm
            || s.record.binding.device_key_sha256 != c.key
            || s.record.binding.core_instance != c.instance
            || s.cancelled
            || s.phase != State::Ready
            || s.permit.is_none()
            || self.rescan_blocks(s)?
            || station::pending(&self.journal)?.is_some()
            || station::commit::pending(&self.journal)?.is_some()
            || denied(self.input.read(Slot::StationCommitRequest))?.is_some()
        {
            return Err(Error::Forbidden);
        }
        let raw = denied(self.output.read(Slot::Status))?.ok_or(Error::Forbidden)?;
        let parent = denied(Status::parse(&raw, &s.record.binding, time))?;
        if parent.state != State::Ready
            || parent.sequence < s.root_sequence
            || parent.sequence > s.sequence
        {
            return Err(Error::Forbidden);
        }
        let raw = denied(self.output.read(Slot::RescanCapability))?.ok_or(Error::Forbidden)?;
        let capability = denied(protocol::Capability::parse(
            &raw,
            &s.record.binding,
            time,
            &boot()?,
        ))?;
        let end_trial = station::rescan_trial_may_end(s);
        if !capability.available
            && !(capability.refusal == Some(protocol::Refusal::Conflict) && end_trial)
        {
            return Err(Error::Busy);
        }
        if fs::read_dir(&self.journal)
            .map_err(|_| Error::Io)?
            .take(513)
            .count()
            >= 506
        {
            return Err(Error::Busy);
        }
        let q = Request {
            format: 1,
            binding: s.record.binding.clone(),
            scan_id: id.clone(),
            owner_key_sha256: hash(&c.principal.public_key),
            owner_generation: c.principal.generation,
            requested_uptime_ms: time,
            deadline_uptime_ms: time
                .checked_add(protocol::MAX_REQUEST_MS)
                .ok_or(Error::InvalidInput)?,
            action: ScanAction::Scan,
        };
        denied(q.validate(&s.record.binding, time))?;
        let record = Record {
            format: 1,
            parent_key_sha256: s.record.key_sha256.clone(),
            owner: s.record.owner.clone(),
            intent: denied(q.intent())?,
        };
        let mut authority = auth.try_lock().map_err(|_| Error::Busy)?;
        still_owner(&mut authority, &c, &token)?;
        write(&file(&self.journal, &id, "intent")?, &record)?;
        s.rescan = Some(Scan {
            record: record.clone(),
            phase: Phase::Unknown,
            acknowledged: false,
            finished: false,
        });
        let dispatch = (|| -> Result<()> {
            write(&self.journal.join("rescan.pending.json"), &record)?;
            // Ending a known unsaved trial is part of this explicitly authorized Begin. The
            // rescan intent is durable first, including when no Scan will ultimately be sent.
            if end_trial {
                self.station_end_for_rescan(s)?;
            }
            if denied(self.input.read(Slot::RescanRequest))?.is_some() {
                return Err(Error::Busy);
            }
            mark(&self.journal, &record, "dispatch")?;
            denied(self.input.publish(Slot::RescanRequest, &q))
        })();
        if dispatch.is_err() {
            s.cancelled = true;
            s.token = None;
            let _ = self.publish_cancel(s);
            return Err(Error::Io);
        }
        s.rescan.as_mut().ok_or(Error::Io)?.phase = Phase::Scanning;
        drop(authority);
        self.rescan_view(&record, Some(s))
    }
    pub(super) async fn rescan_status(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        id: String,
    ) -> Result<Value> {
        let c = owner(auth, token, false).await?;
        let Some(r) = load(&self.journal, &id)? else {
            return Ok(
                json!({"known":false,"operation_id":operation,"scan_id":id,"state":"not_found","not_found_is_not_proof":true}),
            );
        };
        owned(&c, &r, &operation)?;
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        if let Some(s) = active
            .as_mut()
            .filter(|s| s.record.binding == r.intent.binding)
        {
            self.rescan_tick(s, now()?)?;
        }
        if saved(&self.journal, &r)?.is_none() {
            if let Some(root) = self
                .rescan_observation(&r)?
                .filter(|s| s.phase != Phase::Scanning)
            {
                self.rescan_accept_result(&r, root)?;
            } else {
                query(&self.input, &self.journal, &r)?;
            }
        }
        self.rescan_view(&r, active.as_ref())
    }
    pub(super) async fn rescan_cancel(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        id: String,
    ) -> Result<Value> {
        let c = owner(auth.clone(), token.clone(), true).await?;
        let r = load(&self.journal, &id)?.ok_or(Error::Forbidden)?;
        owned(&c, &r, &operation)?;
        {
            let active = self.session.try_lock().map_err(|_| Error::Busy)?;
            let mut authority = auth.try_lock().map_err(|_| Error::Busy)?;
            still_owner(&mut authority, &c, &token)?;
            mark(&self.journal, &r, "cancel")?;
            if active
                .as_ref()
                .is_some_and(|s| s.record.binding == r.intent.binding)
                && !saved(&self.journal, &r)?.is_some_and(|s| known(&s.root))
            {
                cancel(&self.input, &self.journal, &r)?;
            }
            query(&self.input, &self.journal, &r)?;
        }
        self.rescan_status(auth, token, operation, id).await
    }
}
#[cfg(test)]
mod tests;
