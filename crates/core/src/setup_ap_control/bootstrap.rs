// SPDX-License-Identifier: AGPL-3.0-only
//! One-shot local device-window AP producer. No anonymous API, bearer or invented Owner.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    window_id: String,
    binding: Binding,
}
struct Active {
    record: Record,
    phase: State,
    sequence: u64,
    root_sequence: u64,
    acknowledged: bool,
    cancelled: bool,
    gate: Option<Arc<Mutex<rustsimhub_auth::LocalSetup>>>,
    permit: Option<OwnedRwLockWriteGuard<()>>,
}
struct Context {
    window: rustsimhub_auth::DeviceSetupContext,
    instance: String,
    deadline: u64,
}
async fn context(
    auth: Arc<Mutex<Auth>>,
    gate: Arc<Mutex<rustsimhub_auth::LocalSetup>>,
) -> Result<Context> {
    let sample = now()?;
    tokio::task::spawn_blocking(move || {
        let a = auth.try_lock().map_err(|_| Error::Busy)?;
        let g = gate.try_lock().map_err(|_| Error::Busy)?;
        let window = g.device_ap_context(&a)?;
        let deadline = sample
            .checked_add(
                u64::try_from(window.remaining.as_millis()).map_err(|_| Error::InvalidInput)?,
            )
            .ok_or(Error::InvalidInput)?;
        Ok(Context {
            window,
            instance: a.observer_instance(),
            deadline,
        })
    })
    .await
    .map_err(|_| Error::Busy)?
}
fn same(c: &Context, r: &Record) -> bool {
    c.window.window_id == r.window_id
        && c.window.realm == r.binding.realm
        && hash(&c.window.device_public_key) == r.binding.device_key_sha256
        && c.instance == r.binding.core_instance
}
pub(super) struct Bootstrap {
    input: Directory,
    output: Directory,
    journal: PathBuf,
    coordination: Arc<crate::coordination::Coordination>,
    active: Mutex<Option<Active>>,
    stopped: AtomicBool,
}
impl Bootstrap {
    fn path(&self) -> PathBuf {
        self.journal.join("first-setup.pending.json")
    }
    fn read(path: &Path) -> Result<Option<Record>> {
        match fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Io),
        }
        let r: Record = serde_json::from_slice(&rustsimhub_auth::read_private(path, 8192)?)
            .map_err(|_| Error::InvalidInput)?;
        if r.format != 1
            || !r.binding.valid()
            || r.window_id.len() != 64
            || !r
                .window_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Forbidden);
        }
        Ok(Some(r))
    }
    pub(super) fn open(
        input: Directory,
        output: Directory,
        journal: PathBuf,
        coordination: Arc<crate::coordination::Coordination>,
    ) -> Result<Self> {
        let record = Self::read(&journal.join("first-setup.pending.json"))?;
        let active = record
            .map(|record| -> Result<Active> {
                Ok(Active {
                    record,
                    phase: State::Unknown,
                    sequence: 1,
                    root_sequence: 0,
                    acknowledged: true,
                    cancelled: true,
                    gate: None,
                    permit: Some(coordination.network().map_err(|_| Error::Busy)?),
                })
            })
            .transpose()?;
        let v = Self {
            input,
            output,
            journal,
            coordination,
            active: Mutex::new(active),
            stopped: AtomicBool::new(false),
        };
        {
            let mut a = v.active.try_lock().map_err(|_| Error::Busy)?;
            if let Some(s) = a.as_mut() {
                v.publish_cancel(s)?;
                s.phase = State::Unknown;
            }
        }
        Ok(v)
    }
    fn projection(s: &Active) -> Value {
        json!({"available":true,"operation_id":s.record.binding.request_id,"state":s.phase,"first_setup":true,
            "expires_in_seconds":s.record.binding.deadline_uptime_ms.saturating_sub(now().unwrap_or(u64::MAX))/1000,
            "wifi_only":true,"credentials_returned":false,"cellular_activation":false})
    }
    /// Caller already verified a real device Owner; this view contains no nonce or credentials.
    pub(super) fn view(&self) -> Result<Option<Value>> {
        let a = self.active.try_lock().map_err(|_| Error::Busy)?;
        Ok(a.as_ref()
            .filter(|s| s.phase != State::Closed)
            .map(Self::projection))
    }
    pub(super) fn cancel(&self, id: &str) -> Result<Option<Value>> {
        let mut a = self.active.try_lock().map_err(|_| Error::Busy)?;
        let Some(s) = a
            .as_mut()
            .filter(|s| s.record.binding.request_id == id && s.phase != State::Closed)
        else {
            return Ok(None);
        };
        s.cancelled = true;
        s.gate = None;
        self.publish_cancel(s)?;
        Ok(Some(Self::projection(s)))
    }
    fn publish_cancel(&self, s: &mut Active) -> Result<()> {
        s.phase = State::Closing;
        let t = now()?;
        if boot()? != s.record.binding.boot_id || t >= s.record.binding.deadline_uptime_ms {
            return Ok(());
        }
        s.sequence = s
            .sequence
            .checked_add(1)
            .filter(|v| *v <= 1024)
            .ok_or(Error::Forbidden)?;
        denied(self.input.publish(
            Slot::Hint,
            &Hint {
                format: 1,
                binding: s.record.binding.clone(),
                action: Action::Cancel,
                sequence: s.sequence,
                valid_until_uptime_ms:
                    (t + channel::HEARTBEAT_MS).min(s.record.binding.deadline_uptime_ms),
            },
        ))
    }
    pub(super) fn failed(&self) {
        if let Ok(mut a) = self.active.try_lock() {
            if let Some(s) = a.as_mut() {
                if s.phase != State::Closed {
                    s.cancelled = true;
                    s.gate = None;
                    let _ = self.publish_cancel(s);
                    s.phase = State::Unknown;
                }
            }
        }
    }
    pub(super) fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.failed();
    }
    fn available(&self, c: &Context) -> Result<Availability> {
        let raw = denied(self.output.read(Slot::Availability))?.ok_or(Error::Forbidden)?;
        denied(Availability::parse(
            &raw,
            now()?,
            &boot()?,
            &c.window.realm,
            &hash(&c.window.device_public_key),
        ))
    }
    async fn start(
        &self,
        auth: Arc<Mutex<Auth>>,
        gate: Arc<Mutex<rustsimhub_auth::LocalSetup>>,
        idle: &impl Idle,
    ) -> Result<()> {
        let c = context(auth.clone(), gate.clone()).await?;
        if c.window.claimed.is_some() {
            return Ok(());
        }
        self.available(&c)?;
        let permit = self.coordination.network().map_err(|_| Error::Busy)?;
        idle.device(gate.clone()).await?;
        let after = context(auth, gate.clone()).await?;
        if self.stopped.load(Ordering::SeqCst) {
            return Err(Error::Forbidden);
        }
        if after.window.claimed.is_some()
            || c.window.window_id != after.window.window_id
            || c.window.realm != after.window.realm
            || c.window.device_public_key != after.window.device_public_key
            || c.instance != after.instance
        {
            return Err(Error::Forbidden);
        }
        let available = self.available(&after)?;
        let t = now()?;
        let remaining = after.deadline.saturating_sub(t).min(300000);
        if remaining < 1000 {
            return Err(Error::Forbidden);
        }
        let record = Record {
            format: 1,
            window_id: after.window.window_id,
            binding: Binding {
                request_id: request_id()?,
                boot_id: boot()?,
                realm: after.window.realm,
                device_key_sha256: hash(&after.window.device_public_key),
                material_sha256: available.material_sha256,
                core_instance: after.instance,
                nonce: random()?,
                created_uptime_ms: t,
                deadline_uptime_ms: t.checked_add(remaining).ok_or(Error::InvalidInput)?,
            },
        };
        let mut a = self.active.try_lock().map_err(|_| Error::Busy)?;
        if a.is_some() {
            return Ok(());
        }
        if fs::read_dir(&self.journal)
            .map_err(|_| Error::Io)?
            .take(513)
            .count()
            >= 512
        {
            return Err(Error::Busy);
        }
        let bytes = serde_json::to_vec(&record).map_err(|_| Error::InvalidInput)?;
        rustsimhub_auth::write_private_new(
            &self
                .journal
                .join(format!("first-setup.{}.intent", record.window_id)),
            &bytes,
        )?;
        rustsimhub_auth::write_private_new(&self.path(), &bytes)?;
        let hint = Hint {
            format: 1,
            binding: record.binding.clone(),
            action: Action::Start,
            sequence: 1,
            valid_until_uptime_ms: (t + channel::HEARTBEAT_MS)
                .min(record.binding.deadline_uptime_ms),
        };
        *a = Some(Active {
            record,
            phase: State::Unknown,
            sequence: 1,
            root_sequence: 0,
            acknowledged: false,
            cancelled: false,
            gate: Some(gate),
            permit: Some(permit),
        });
        let result = denied(self.input.publish(Slot::Hint, &hint));
        let s = a.as_mut().ok_or(Error::Io)?;
        if result.is_ok() {
            s.phase = State::Starting;
        } else {
            s.cancelled = true;
            s.gate = None;
        }
        result
    }
    fn closed(&self, s: &mut Active, status: &Status) -> Result<()> {
        let pending = Self::read(&self.path())?.ok_or(Error::Forbidden)?;
        if pending.binding != s.record.binding || pending.window_id != s.record.window_id {
            return Err(Error::Forbidden);
        }
        let path = self
            .journal
            .join(format!("first-setup.{}.closed", s.record.window_id));
        if fs::symlink_metadata(&path).is_ok() {
            let old: Status = serde_json::from_slice(&rustsimhub_auth::read_private(&path, 8192)?)
                .map_err(|_| Error::InvalidInput)?;
            denied(old.validate(&s.record.binding, old.sample_uptime_ms))?;
            if old.state != State::Closed {
                return Err(Error::Forbidden);
            }
        } else {
            rustsimhub_auth::write_private_new(
                &path,
                &serde_json::to_vec(status).map_err(|_| Error::InvalidInput)?,
            )?;
        }
        fs::remove_file(self.path()).map_err(|_| Error::Io)?;
        fs::File::open(&self.journal)
            .and_then(|f| f.sync_all())
            .map_err(|_| Error::Io)?;
        s.phase = State::Closed;
        s.gate = None;
        s.permit.take();
        Ok(())
    }
    pub(super) async fn tick(
        &self,
        auth: Arc<Mutex<Auth>>,
        setup: Option<Arc<Mutex<rustsimhub_auth::LocalSetup>>>,
        idle: &impl Idle,
    ) -> Result<()> {
        if self.stopped.load(Ordering::SeqCst) {
            return Ok(());
        }
        let active = {
            let a = self.active.try_lock().map_err(|_| Error::Busy)?;
            match a.as_ref() {
                Some(s) if s.phase == State::Closed => return Ok(()),
                Some(s) => Some((s.record.clone(), s.gate.clone(), s.cancelled)),
                None => None,
            }
        };
        let Some((record, gate, cancelled)) = active else {
            if let Some(g) = setup {
                self.start(auth, g, idle).await?;
            }
            return Ok(());
        };
        if let Some(raw) = denied(self.output.read(Slot::Status))? {
            if let Ok(status) = Status::parse(&raw, &record.binding, now()?) {
                let mut a = self.active.try_lock().map_err(|_| Error::Busy)?;
                let s = a.as_mut().ok_or(Error::Busy)?;
                if status.state == State::Closed {
                    return self.closed(s, &status);
                }
                if status.sequence >= s.root_sequence {
                    s.acknowledged = true;
                    s.root_sequence = status.sequence;
                    if status.sequence > s.sequence
                        || matches!(status.state, State::Unknown | State::Closing)
                    {
                        s.cancelled = true;
                        s.gate = None;
                        s.phase = State::Unknown;
                    } else if !s.cancelled
                        && !(s.phase == State::Ready && status.state == State::Starting)
                    {
                        s.phase = status.state;
                    }
                }
            }
        }
        let allowed = if let Some(g) = gate.filter(|_| !cancelled) {
            if context(auth.clone(), g.clone())
                .await
                .is_ok_and(|c| same(&c, &record))
                && idle.device(g.clone()).await.is_ok()
            {
                context(auth, g).await.is_ok_and(|c| same(&c, &record))
            } else {
                false
            }
        } else {
            false
        };
        let t = now()?;
        let mut a = self.active.try_lock().map_err(|_| Error::Busy)?;
        let s = a.as_mut().ok_or(Error::Busy)?;
        if s.phase == State::Closed {
            return Ok(());
        }
        if !allowed || s.cancelled || t >= record.binding.deadline_uptime_ms {
            if !s.cancelled {
                s.cancelled = true;
                s.gate = None;
                self.publish_cancel(s)?;
            }
            return Ok(());
        }
        if !s.acknowledged {
            if t >= record.binding.created_uptime_ms + channel::HEARTBEAT_MS {
                s.cancelled = true;
                s.gate = None;
                s.phase = State::Unknown;
            }
            return Ok(());
        }
        s.sequence = s
            .sequence
            .checked_add(1)
            .filter(|v| *v <= 1024)
            .ok_or(Error::Forbidden)?;
        denied(self.input.publish(
            Slot::Hint,
            &Hint {
                format: 1,
                binding: record.binding.clone(),
                action: Action::KeepAlive,
                sequence: s.sequence,
                valid_until_uptime_ms:
                    (t + channel::HEARTBEAT_MS).min(record.binding.deadline_uptime_ms),
            },
        ))
    }
}
#[cfg(test)]
mod tests;
