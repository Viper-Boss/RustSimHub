// SPDX-License-Identifier: AGPL-3.0-only
//! Authenticated producer for the fixed Root station slots. Journals contain
//! only immutable intent, never SSID, password, bearer or arbitrary NM settings.
use super::*;
use rustsimhub_setup_ap::channel::station::{
    Intent, Operation, Phase, Request, Secret, Status as TrialStatus,
};
use zeroize::Zeroizing;

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
    root: Option<TrialStatus>,
    parent_closed: bool,
    cancelled: bool,
}
pub(super) struct Trial {
    record: Record,
    phase: Phase,
    acknowledged: bool,
    finished: bool,
    cancelled: bool,
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn path(dir: &Path, id: &str, suffix: &str) -> Result<PathBuf> {
    if !uuid(id) {
        return Err(Error::InvalidInput);
    }
    Ok(dir.join(format!("station-{id}.{suffix}")))
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::Io),
        Ok(_) => {}
    }
    serde_json::from_slice(&rustsimhub_auth::read_private(path, 8192)?)
        .map(Some)
        .map_err(|_| Error::InvalidInput)
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
pub(super) fn pending(dir: &Path) -> Result<Option<Record>> {
    let r: Option<Record> = read(&dir.join("station.pending.json"))?;
    if let Some(r) = &r {
        validate(r)?;
    }
    Ok(r)
}
fn load(dir: &Path, id: &str) -> Result<Option<Record>> {
    let r: Option<Record> = read(&path(dir, id, "intent")?)?;
    if let Some(r) = &r {
        validate(r)?;
        if r.intent.trial_id != id {
            return Err(Error::Forbidden);
        }
    }
    Ok(r)
}
fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    rustsimhub_auth::write_private_new(
        path,
        &serde_json::to_vec(value).map_err(|_| Error::InvalidInput)?,
    )
}
fn remove_pending(dir: &Path, r: &Record) -> Result<()> {
    if let Some(old) = pending(dir)? {
        if old != *r {
            return Err(Error::Forbidden);
        }
        fs::remove_file(dir.join("station.pending.json")).map_err(|_| Error::Io)?;
        fs::File::open(dir)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)?;
    }
    Ok(())
}
fn saved(dir: &Path, r: &Record) -> Result<Option<Saved>> {
    let value: Option<Saved> = read(&path(dir, &r.intent.trial_id, "result")?)?;
    if let Some(v) = &value {
        if v.format != 1 || v.record != *r || v.root.is_none() && !v.parent_closed {
            return Err(Error::Forbidden);
        }
        if let Some(s) = &v.root {
            denied(s.validate(s.sample_uptime_ms))?;
            if s.intent != r.intent || s.phase == Phase::Testing {
                return Err(Error::Forbidden);
            }
        }
    }
    Ok(value)
}
fn save(
    dir: &Path,
    r: &Record,
    root: Option<TrialStatus>,
    parent_closed: bool,
    cancelled: bool,
) -> Result<()> {
    if saved(dir, r)?.is_none() {
        write(
            &path(dir, &r.intent.trial_id, "result")?,
            &Saved {
                format: 1,
                record: r.clone(),
                root,
                parent_closed,
                cancelled,
            },
        )?;
    }
    Ok(())
}
fn cancel_mark(dir: &Path, r: &Record) -> Result<bool> {
    let marker: Option<Record> = read(&path(dir, &r.intent.trial_id, "cancel")?)?;
    match marker {
        Some(old) if old == *r => Ok(true),
        Some(_) => Err(Error::Forbidden),
        None => Ok(false),
    }
}
fn slot(input: &Directory, r: &Record) -> Result<Option<Request>> {
    let Some(raw) = denied(input.read(Slot::StationRequest))? else {
        return Ok(None);
    };
    let raw = Zeroizing::new(raw);
    let q: Request = serde_json::from_slice(&raw).map_err(|_| Error::InvalidInput)?;
    let intent = denied(q.intent())?;
    if q.binding != r.intent.binding
        || q.trial_id != r.intent.trial_id
        || q.owner_generation != r.intent.owner_generation
        || q.owner_key_sha256 != r.intent.owner_key_sha256
        || matches!(q.operation, Operation::Test { .. }) && intent != r.intent
    {
        return Err(Error::Forbidden);
    }
    Ok(Some(q))
}
fn clear(input: &Directory, r: &Record) -> Result<()> {
    if slot(input, r)?.is_some() {
        denied(input.clear(Slot::StationRequest))?;
    }
    Ok(())
}
fn cancel_request(input: &Directory, r: &Record) -> Result<()> {
    slot(input, r)?;
    let time = now()?;
    let deadline = (time.saturating_add(40_000))
        .min(r.intent.binding.deadline_uptime_ms.saturating_sub(20_000));
    if r.intent.binding.boot_id != boot()? || deadline <= time {
        return clear(input, r);
    }
    let q = Request {
        format: 1,
        binding: r.intent.binding.clone(),
        trial_id: r.intent.trial_id.clone(),
        owner_key_sha256: r.intent.owner_key_sha256.clone(),
        owner_generation: r.intent.owner_generation,
        requested_uptime_ms: time,
        deadline_uptime_ms: deadline,
        operation: Operation::Cancel,
    };
    denied(q.validate(&r.intent.binding, time))?;
    denied(input.publish(Slot::StationRequest, &q))
}
pub(super) fn recover(
    input: &Directory,
    dir: &Path,
    parent: Option<&super::Record>,
) -> Result<Option<Trial>> {
    let Some(r) = pending(dir)? else {
        return Ok(None);
    };
    if load(dir, &r.intent.trial_id)?.as_ref() != Some(&r) {
        return Err(Error::Forbidden);
    }
    if let Some(parent) = parent {
        if r.intent.binding != parent.binding
            || r.owner != parent.owner
            || r.parent_key_sha256 != parent.key_sha256
        {
            return Err(Error::Forbidden);
        }
        return Ok(Some(Trial {
            record: r,
            phase: Phase::Unknown,
            acknowledged: false,
            finished: false,
            cancelled: true,
        }));
    }
    // An older compatible Core may have closed the parent without knowing
    // station files. Its exact protected Closed proof permits cleanup only.
    let closed: Status =
        read(&dir.join(format!("{}.closed", r.parent_key_sha256)))?.ok_or(Error::Forbidden)?;
    denied(closed.validate(&r.intent.binding, closed.sample_uptime_ms))?;
    if closed.state != State::Closed {
        return Err(Error::Forbidden);
    }
    save(dir, &r, None, true, true)?;
    clear(input, &r)?;
    remove_pending(dir, &r)?;
    Ok(None)
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
fn view(r: &Record, phase: Phase, authoritative: bool, cancelled: bool, live: bool) -> Value {
    let state = if cancelled && matches!(phase, Phase::Ready | Phase::Failed) {
        "cancelled"
    } else {
        match phase {
            Phase::Testing => "testing",
            Phase::Ready => "ready",
            Phase::Failed => "failed",
            Phase::Unknown => "unknown",
        }
    };
    json!({"known":true,"operation_id":r.intent.binding.request_id,"trial_id":r.intent.trial_id,"state":state,
        "authoritative_result":authoritative,"cancel_requested":cancelled,"historical":!live,
        "ap_return_verified":authoritative&&matches!(phase,Phase::Ready|Phase::Failed),
        "requires_confirmation":live&&authoritative&&phase==Phase::Ready&&!cancelled,
        "wifi_only":true,"cellular_activation":false,"credentials_returned":false,"settings_saved":false})
}
impl Runtime {
    fn station_view(&self, r: &Record, s: Option<&Session>) -> Result<Value> {
        let live = s.is_some_and(|s| {
            s.record.binding == r.intent.binding
                && !s.cancelled
                && s.phase == State::Ready
                && now().is_ok_and(|time| time < s.record.binding.deadline_uptime_ms)
                && boot().is_ok_and(|boot| boot == s.record.binding.boot_id)
        });
        let cancelled = cancel_mark(&self.journal, r)?;
        if let Some(v) = saved(&self.journal, r)? {
            let phase = v.root.as_ref().map_or(Phase::Unknown, |v| v.phase);
            return Ok(view(
                r,
                phase,
                v.root.is_some(),
                cancelled || v.cancelled,
                live,
            ));
        }
        let current = s.and_then(|s| s.trial.as_ref()).filter(|t| t.record == *r);
        Ok(view(
            r,
            current.map_or(Phase::Unknown, |t| t.phase),
            false,
            cancelled || current.is_some_and(|t| t.cancelled),
            live,
        ))
    }
    pub(super) fn station_recover(&self, s: &mut Session) -> Result<()> {
        if let Some(t) = &s.trial {
            cancel_request(&self.input, &t.record)?;
        }
        Ok(())
    }
    pub(super) fn station_parent_closed(&self, s: &mut Session) -> Result<()> {
        if let Some(t) = s.trial.as_mut() {
            if !t.finished {
                save(&self.journal, &t.record, None, true, true)?;
                t.phase = Phase::Unknown;
                t.finished = true;
            }
            clear(&self.input, &t.record)?;
            remove_pending(&self.journal, &t.record)?;
        } else if pending(&self.journal)?.is_some() {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    pub(super) fn station_tick(&self, s: &mut Session, time: u64) -> Result<()> {
        let Some(t) = s.trial.as_mut() else {
            return Ok(());
        };
        if t.finished {
            return Ok(());
        }
        if let Some(raw) = denied(self.output.read(Slot::StationStatus))? {
            if let Ok(status) = serde_json::from_slice::<TrialStatus>(&raw) {
                if status.intent == t.record.intent && status.validate(time).is_ok() {
                    t.acknowledged = true;
                    t.phase = status.phase;
                    // Testing acknowledges consumption of the secret Test,
                    // not consumption of a later Cancel. Keep Cancel until a
                    // terminal Root proof so a stale Testing heartbeat cannot
                    // erase the user's cleanup request.
                    if status.phase != Phase::Testing
                        || slot(&self.input, &t.record)?
                            .is_some_and(|q| matches!(q.operation, Operation::Test { .. }))
                    {
                        clear(&self.input, &t.record)?;
                    }
                    if status.phase != Phase::Testing {
                        save(&self.journal, &t.record, Some(status), false, t.cancelled)?;
                        remove_pending(&self.journal, &t.record)?;
                        t.finished = true;
                        if t.phase == Phase::Unknown && !s.cancelled {
                            s.cancelled = true;
                            s.token = None;
                            self.publish_cancel(s)?;
                        }
                    }
                    return Ok(());
                }
            }
        }
        if time >= t.record.intent.deadline_uptime_ms
            || !t.acknowledged
                && time.saturating_sub(t.record.intent.requested_uptime_ms) >= channel::HEARTBEAT_MS
        {
            t.phase = Phase::Unknown;
            t.cancelled = true;
            cancel_request(&self.input, &t.record)?;
            if !s.cancelled {
                s.cancelled = true;
                s.token = None;
                self.publish_cancel(s)?;
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn station_begin(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
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
            let request = Request {
                format: 1,
                binding: old.intent.binding.clone(),
                trial_id: id,
                owner_key_sha256: old.intent.owner_key_sha256.clone(),
                owner_generation: old.intent.owner_generation,
                requested_uptime_ms: old.intent.requested_uptime_ms,
                deadline_uptime_ms: old.intent.deadline_uptime_ms,
                operation: Operation::Test {
                    ssid,
                    passphrase: Secret::new(passphrase),
                },
            };
            if denied(request.intent())? != old.intent {
                return Err(Error::Forbidden);
            }
            let active = self.session.try_lock().map_err(|_| Error::Busy)?;
            return self.station_view(&old, active.as_ref());
        }
        idle.check(&token).await?;
        let after = owner(auth, token, true).await?;
        if c.principal != after.principal
            || c.realm != after.realm
            || c.key != after.key
            || c.instance != after.instance
        {
            return Err(Error::Forbidden);
        }
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_mut().ok_or(Error::Forbidden)?;
        if s.record.binding.request_id != operation
            || s.record.owner != Owner::from(&c.principal)
            || s.record.binding.realm != c.realm
            || s.record.binding.device_key_sha256 != c.key
            || s.record.binding.core_instance != c.instance
            || s.cancelled
            || !s.station_supported
            || s.phase != State::Ready
            || s.station_count >= 16
            || s.trial.as_ref().is_some_and(|t| !t.finished)
            || pending(&self.journal)?.is_some()
        {
            return Err(Error::Forbidden);
        }
        let time = now()?;
        let raw = denied(self.output.read(Slot::Status))?.ok_or(Error::Forbidden)?;
        let proof = denied(Status::parse(&raw, &s.record.binding, time))?;
        if proof.state != State::Ready
            || proof.sequence < s.root_sequence
            || proof.sequence > s.sequence
        {
            return Err(Error::Forbidden);
        }
        let request = Request {
            format: 1,
            binding: s.record.binding.clone(),
            trial_id: id.clone(),
            owner_key_sha256: hash(&c.principal.public_key),
            owner_generation: c.principal.generation,
            requested_uptime_ms: time,
            deadline_uptime_ms: time.checked_add(40_000).ok_or(Error::InvalidInput)?,
            operation: Operation::Test {
                ssid,
                passphrase: Secret::new(passphrase),
            },
        };
        denied(request.validate(&s.record.binding, time))?;
        if fs::read_dir(&self.journal)
            .map_err(|_| Error::Io)?
            .take(513)
            .count()
            >= 508
        {
            return Err(Error::Busy);
        }
        let record = Record {
            format: 1,
            parent_key_sha256: s.record.key_sha256.clone(),
            owner: s.record.owner.clone(),
            intent: denied(request.intent())?,
        };
        write(&path(&self.journal, &id, "intent")?, &record)?;
        s.trial = Some(Trial {
            record: record.clone(),
            phase: Phase::Unknown,
            acknowledged: false,
            finished: false,
            cancelled: false,
        });
        s.station_count += 1;
        write(&self.journal.join("station.pending.json"), &record)?;
        if denied(self.input.publish(Slot::StationRequest, &request)).is_err() {
            s.cancelled = true;
            s.token = None;
            let _ = self.publish_cancel(s);
            return Err(Error::Io);
        }
        s.trial.as_mut().ok_or(Error::Io)?.phase = Phase::Testing;
        self.station_view(&record, Some(s))
    }
    pub(super) async fn station_status(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        id: String,
    ) -> Result<Value> {
        let c = owner(auth, token, false).await?;
        let Some(record) = load(&self.journal, &id)? else {
            return Ok(
                json!({"known":false,"operation_id":operation,"trial_id":id,"state":"not_found","not_found_is_not_proof":true}),
            );
        };
        owned(&c, &record, &operation)?;
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        if let Some(s) = active
            .as_mut()
            .filter(|s| s.record.binding == record.intent.binding)
        {
            self.station_tick(s, now()?)?;
        }
        self.station_view(&record, active.as_ref())
    }
    pub(super) async fn station_cancel(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        operation: String,
        id: String,
    ) -> Result<Value> {
        let c = owner(auth, token, true).await?;
        let record = load(&self.journal, &id)?.ok_or(Error::Forbidden)?;
        owned(&c, &record, &operation)?;
        if !cancel_mark(&self.journal, &record)? {
            write(&path(&self.journal, &id, "cancel")?, &record)?;
        }
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        if let Some(s) = active
            .as_mut()
            .filter(|s| s.record.binding == record.intent.binding)
        {
            if let Some(t) = s.trial.as_mut().filter(|t| t.record == record) {
                t.cancelled = true;
                if !t.finished {
                    cancel_request(&self.input, &record)?;
                }
            }
        }
        self.station_view(&record, active.as_ref())
    }
}

#[cfg(test)]
mod tests;
