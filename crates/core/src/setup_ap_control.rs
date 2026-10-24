// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit opt-in Owner control plane; no Root execution or implicit AP activation here.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_policy::{Kind, Principal, Scope};
use rustsimhub_setup_ap::channel::{
    self,
    files::{Directory, Slot},
    Action, Availability, Binding, Hint, State, Status,
};
use rustsimhub_wire::AuthCommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};
use tokio::sync::OwnedRwLockWriteGuard;
mod bootstrap;
type Result<T> = rustsimhub_auth::Result<T>;
static RUNTIME: OnceLock<Arc<Runtime>> = OnceLock::new();
fn denied<T>(v: rustsimhub_setup_ap::Result<T>) -> Result<T> {
    v.map_err(|_| Error::Forbidden)
}
fn now() -> Result<u64> {
    denied(channel::uptime_ms())
}
fn boot() -> Result<String> {
    denied(channel::boot_id())
}
fn hash(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn random() -> Result<String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|_| Error::Entropy)?;
    Ok(b.iter().map(|v| format!("{v:02x}")).collect())
}
fn request_id() -> Result<String> {
    let s = random()?;
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &s[..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..32]
    ))
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    id: String,
    key: [u8; 32],
    generation: u64,
    scopes: u16,
}
impl From<&Principal> for Owner {
    fn from(p: &Principal) -> Self {
        Self {
            id: p.id.clone(),
            key: p.public_key,
            generation: p.generation,
            scopes: p.scopes,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u16,
    key_sha256: String,
    binding: Binding,
    owner: Owner,
}
struct Session {
    record: Record,
    token: Option<String>,
    phase: State,
    sequence: u64,
    acknowledged: bool,
    root_sequence: u64,
    cancelled: bool,
    permit: Option<OwnedRwLockWriteGuard<()>>,
}
struct Context {
    principal: Principal,
    realm: String,
    key: String,
    instance: String,
}
async fn owner(auth: Arc<Mutex<Auth>>, token: String, recent: bool) -> Result<Context> {
    tokio::task::spawn_blocking(move || {
        let mut a = auth.try_lock().map_err(|_| Error::Busy)?;
        let p = if recent {
            a.recent_owner(&token)?
        } else {
            a.authorize(&token, Scope::NetworkManage)?
        };
        if p.kind != Kind::Owner || a.authorize(&token, Scope::NetworkManage)? != p {
            return Err(Error::Forbidden);
        }
        Ok(Context {
            principal: p,
            realm: a.durable_realm()?,
            key: hash(&a.public_key()),
            instance: a.observer_instance(),
        })
    })
    .await
    .map_err(|_| Error::Busy)?
}
trait Idle {
    fn check(&self, token: &str) -> impl std::future::Future<Output = Result<()>> + Send;
    fn device(
        &self,
        setup: Arc<Mutex<rustsimhub_auth::LocalSetup>>,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}
struct Calls(Option<crate::calls::Controller>);
impl Idle for Calls {
    async fn check(&self, token: &str) -> Result<()> {
        self.0
            .as_ref()
            .ok_or(Error::Forbidden)?
            .setup_ap_idle(token.into())
            .await
            .map_err(|_| Error::Busy)
    }
    async fn device(&self, setup: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
        self.0
            .as_ref()
            .ok_or(Error::Forbidden)?
            .device_setup_idle(setup)
            .await
            .map_err(|_| Error::Busy)
    }
}
pub(crate) struct Runtime {
    bootstrap: bootstrap::Bootstrap,
    input: Directory,
    output: Directory,
    journal: PathBuf,
    session: Mutex<Option<Session>>,
    running: AtomicBool,
    coordination: Arc<crate::coordination::Coordination>,
}
impl Runtime {
    fn new(input: Directory, output: Directory, journal: PathBuf) -> Result<Self> {
        Self::with_coordination(input, output, journal, crate::coordination::global())
    }
    fn with_coordination(
        input: Directory,
        output: Directory,
        journal: PathBuf,
        coordination: Arc<crate::coordination::Coordination>,
    ) -> Result<Self> {
        let m = fs::symlink_metadata(&journal).map_err(|_| Error::Io)?;
        if !journal.is_absolute()
            || !m.is_dir()
            || m.uid() != rustix::process::geteuid().as_raw()
            || m.mode() & 0o777 != 0o700
        {
            return Err(Error::Forbidden);
        }
        let record = Self::read_record(&journal.join("pending.json"))?;
        if record.is_some()
            && fs::symlink_metadata(journal.join("first-setup.pending.json")).is_ok()
        {
            return Err(Error::Forbidden);
        }
        let session = record
            .map(|record| -> Result<Session> {
                let permit = coordination.network().map_err(|_| Error::Busy)?;
                Ok(Session {
                    record,
                    token: None,
                    phase: State::Unknown,
                    sequence: 1,
                    acknowledged: true,
                    root_sequence: 0,
                    cancelled: true,
                    permit: Some(permit),
                })
            })
            .transpose()?;
        let runtime = Self {
            bootstrap: bootstrap::Bootstrap::open(
                input.clone(),
                output.clone(),
                journal.clone(),
                coordination.clone(),
            )?,
            input,
            output,
            journal,
            session: Mutex::new(session),
            running: AtomicBool::new(false),
            coordination,
        };
        {
            let mut recovered = runtime.session.try_lock().map_err(|_| Error::Busy)?;
            if let Some(s) = recovered.as_mut() {
                runtime.publish_cancel(s)?;
                s.phase = State::Unknown;
            }
        }
        Ok(runtime)
    }
    fn read_record(path: &Path) -> Result<Option<Record>> {
        match fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Io),
            Ok(_) => {}
        }
        let r: Record = serde_json::from_slice(&rustsimhub_auth::read_private(path, 8192)?)
            .map_err(|_| Error::InvalidInput)?;
        if r.format != 1
            || !r.binding.valid()
            || r.key_sha256.len() != 64
            || !r
                .key_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || r.owner.generation == 0
            || r.owner.scopes & !rustsimhub_policy::ALL_SCOPES != 0
        {
            return Err(Error::Forbidden);
        }
        Ok(Some(r))
    }
    fn view(s: &Session) -> Value {
        json!({"available":true,"operation_id":s.record.binding.request_id,"state":s.phase,
            "expires_in_seconds":s.record.binding.deadline_uptime_ms.saturating_sub(now().unwrap_or(u64::MAX))/1000,
            "wifi_only":true,"cellular_activation":false,"credentials_returned":false})
    }
    fn availability(&self, c: &Context) -> Result<Availability> {
        let bytes = denied(self.output.read(Slot::Availability))?.ok_or(Error::Forbidden)?;
        denied(Availability::parse(
            &bytes,
            now()?,
            &boot()?,
            &c.realm,
            &c.key,
        ))
    }
    async fn status(&self, auth: Arc<Mutex<Auth>>, token: String) -> Result<Value> {
        let c = owner(auth, token, false).await?;
        let active = self.session.try_lock().map_err(|_| Error::Busy)?;
        if let Some(s) = active.as_ref() {
            if s.record.owner != Owner::from(&c.principal) {
                return Err(Error::Forbidden);
            }
            return Ok(Self::view(s));
        }
        drop(active);
        if let Some(view) = self.bootstrap.view()? {
            return Ok(view);
        }
        Ok(
            json!({"available":self.availability(&c).is_ok(),"state":"idle","credentials_returned":false,"cellular_activation":false}),
        )
    }
    async fn start(
        &self,
        auth: Arc<Mutex<Auth>>,
        token: String,
        key: String,
        idle: &impl Idle,
    ) -> Result<Value> {
        let c = owner(auth.clone(), token.clone(), true).await?;
        let digest = hash(key.as_bytes());
        {
            let active = self.session.try_lock().map_err(|_| Error::Busy)?;
            if let Some(s) = active.as_ref() {
                if s.record.owner != Owner::from(&c.principal) && s.phase != State::Closed {
                    return Err(Error::Forbidden);
                }
                if s.record.key_sha256 == digest {
                    if s.record.owner != Owner::from(&c.principal) {
                        return Err(Error::Forbidden);
                    }
                    return Ok(Self::view(s));
                }
                if s.phase != State::Closed {
                    return Err(Error::Busy);
                }
            }
        }
        if Self::read_record(&self.journal.join(format!("{digest}.intent")))?.is_some() {
            return Err(Error::Forbidden);
        }
        self.availability(&c)?;
        let permit = self.coordination.network().map_err(|_| Error::Busy)?;
        idle.check(&token).await?;
        let after = owner(auth, token.clone(), true).await?;
        if after.principal != c.principal
            || after.realm != c.realm
            || after.key != c.key
            || after.instance != c.instance
        {
            return Err(Error::Forbidden);
        }
        let available = self.availability(&after)?;
        let time = now()?;
        let binding = Binding {
            request_id: request_id()?,
            boot_id: boot()?,
            realm: c.realm,
            device_key_sha256: c.key,
            material_sha256: available.material_sha256,
            core_instance: c.instance,
            nonce: random()?,
            created_uptime_ms: time,
            deadline_uptime_ms: time.checked_add(300000).ok_or(Error::InvalidInput)?,
        };
        if !binding.valid() {
            return Err(Error::InvalidInput);
        }
        let record = Record {
            format: 1,
            key_sha256: digest.clone(),
            binding: binding.clone(),
            owner: Owner::from(&c.principal),
        };
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        if active.as_ref().is_some_and(|s| s.phase != State::Closed) {
            return Err(Error::Busy);
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
        rustsimhub_auth::write_private_new(&self.journal.join(format!("{digest}.intent")), &bytes)?;
        rustsimhub_auth::write_private_new(&self.journal.join("pending.json"), &bytes)?;
        let hint = Hint {
            format: 1,
            binding,
            action: Action::Start,
            sequence: 1,
            valid_until_uptime_ms: time + channel::HEARTBEAT_MS,
        };
        *active = Some(Session {
            record,
            token: Some(token),
            phase: State::Unknown,
            sequence: 1,
            acknowledged: false,
            root_sequence: 0,
            cancelled: false,
            permit: Some(permit),
        });
        let published = denied(self.input.publish(Slot::Hint, &hint));
        let s = active.as_mut().ok_or(Error::Io)?;
        if published.is_ok() {
            s.phase = State::Starting;
        } else {
            s.token = None;
            s.cancelled = true;
        }
        published?;
        Ok(Self::view(s))
    }
    async fn cancel(&self, auth: Arc<Mutex<Auth>>, token: String, id: String) -> Result<Value> {
        let c = owner(auth, token, true).await?;
        if let Some(view) = self.bootstrap.cancel(&id)? {
            return Ok(view);
        }
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_mut().ok_or(Error::Forbidden)?;
        if s.record.owner != Owner::from(&c.principal) || s.record.binding.request_id != id {
            return Err(Error::Forbidden);
        }
        if s.phase != State::Closed {
            s.cancelled = true;
            s.token = None;
            self.publish_cancel(s)?;
        }
        Ok(Self::view(s))
    }
    fn publish_cancel(&self, s: &mut Session) -> Result<()> {
        s.phase = State::Closing;
        let time = now()?;
        if time >= s.record.binding.deadline_uptime_ms || s.record.binding.boot_id != boot()? {
            return Ok(());
        }
        s.sequence = s.sequence.checked_add(1).ok_or(Error::InvalidInput)?;
        if s.sequence > 1024 {
            return Err(Error::Forbidden);
        }
        denied(self.input.publish(
            Slot::Hint,
            &Hint {
                format: 1,
                binding: s.record.binding.clone(),
                action: Action::Cancel,
                sequence: s.sequence,
                valid_until_uptime_ms:
                    (time + channel::HEARTBEAT_MS).min(s.record.binding.deadline_uptime_ms),
            },
        ))
    }
    fn closed(&self, s: &mut Session, status: &Status, time: u64) -> Result<()> {
        denied(status.validate(&s.record.binding, time))?;
        let pending =
            Self::read_record(&self.journal.join("pending.json"))?.ok_or(Error::Forbidden)?;
        if pending.binding != s.record.binding
            || pending.owner != s.record.owner
            || pending.key_sha256 != s.record.key_sha256
        {
            return Err(Error::Forbidden);
        }
        let path = self.journal.join(format!("{}.closed", s.record.key_sha256));
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
        fs::remove_file(self.journal.join("pending.json")).map_err(|_| Error::Io)?;
        fs::File::open(&self.journal)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)?;
        s.token = None;
        s.phase = State::Closed;
        s.permit.take();
        Ok(())
    }
    async fn tick(&self, auth: Arc<Mutex<Auth>>, idle: &impl Idle) -> Result<()> {
        let (record, token, cancelled) = {
            let active = self.session.try_lock().map_err(|_| Error::Busy)?;
            let Some(s) = active.as_ref() else {
                return Ok(());
            };
            if s.phase == State::Closed {
                return Ok(());
            }
            (s.record.clone(), s.token.clone(), s.cancelled)
        };
        let time = now()?;
        if let Some(bytes) = denied(self.output.read(Slot::Status))? {
            if let Ok(status) = Status::parse(&bytes, &record.binding, time) {
                let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
                let s = active.as_mut().ok_or(Error::Busy)?;
                if s.record.binding != record.binding {
                    return Err(Error::Busy);
                }
                if status.state == State::Closed {
                    return self.closed(s, &status, time);
                }
                if status.sequence >= s.root_sequence {
                    s.root_sequence = status.sequence;
                    s.acknowledged = true;
                    if !s.cancelled && status.sequence > s.sequence {
                        s.phase = State::Unknown;
                        s.cancelled = true;
                        s.token = None;
                    } else if matches!(status.state, State::Closing | State::Unknown) {
                        s.phase = status.state;
                        s.cancelled = true;
                        s.token = None;
                    } else if !s.cancelled
                        && !(s.phase == State::Ready && status.state == State::Starting)
                    {
                        s.phase = status.state;
                    }
                }
            }
        }
        let allowed = if let Some(token) = token.as_ref().filter(|_| !cancelled) {
            match owner(auth.clone(), token.clone(), true).await {
                Ok(c)
                    if Owner::from(&c.principal) == record.owner
                        && c.realm == record.binding.realm
                        && c.key == record.binding.device_key_sha256
                        && c.instance == record.binding.core_instance =>
                {
                    idle.check(token).await.is_ok()
                        && owner(auth, token.clone(), true).await.is_ok_and(|p| {
                            p.principal == c.principal
                                && p.realm == c.realm
                                && p.key == c.key
                                && p.instance == c.instance
                        })
                }
                _ => false,
            }
        } else {
            false
        };
        let time = now()?;
        let mut active = self.session.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_mut().ok_or(Error::Busy)?;
        if s.record.binding != record.binding {
            return Err(Error::Busy);
        }
        if s.phase == State::Closed {
            return Ok(());
        }
        if !allowed || s.cancelled || time >= s.record.binding.deadline_uptime_ms {
            if !s.cancelled {
                s.cancelled = true;
                s.token = None;
                self.publish_cancel(s)?;
            }
            return Ok(());
        }
        // Keep the original Start byte-for-byte until Root acknowledges its durable consumption.
        if !s.acknowledged {
            if time >= s.record.binding.created_uptime_ms + channel::HEARTBEAT_MS {
                s.cancelled = true;
                s.token = None;
                s.phase = State::Unknown;
            }
            return Ok(());
        }
        s.sequence = s.sequence.checked_add(1).ok_or(Error::InvalidInput)?;
        if s.sequence > 1024 {
            return Err(Error::Forbidden);
        }
        denied(self.input.publish(
            Slot::Hint,
            &Hint {
                format: 1,
                binding: s.record.binding.clone(),
                action: Action::KeepAlive,
                sequence: s.sequence,
                valid_until_uptime_ms:
                    (time + channel::HEARTBEAT_MS).min(s.record.binding.deadline_uptime_ms),
            },
        ))
    }
}
pub(crate) fn enable(input: &Path, output: &Path, journal: &Path) -> Result<()> {
    let uid = rustix::process::geteuid().as_raw();
    let runtime = Runtime::new(
        denied(Directory::open(input, uid, 0, 0o2750))?,
        denied(Directory::open(output, 0, uid, 0o2750))?,
        journal.into(),
    )?;
    RUNTIME.set(Arc::new(runtime)).map_err(|_| Error::Forbidden)
}
pub(crate) fn bind(
    auth: Arc<Mutex<Auth>>,
    calls: Option<crate::calls::Controller>,
    setup: Option<Arc<Mutex<rustsimhub_auth::LocalSetup>>>,
) {
    let Some(runtime) = RUNTIME.get().cloned() else {
        return;
    };
    if runtime.running.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        let idle = Calls(calls);
        loop {
            if !runtime.running.load(Ordering::SeqCst) {
                return;
            }
            if runtime
                .bootstrap
                .tick(auth.clone(), setup.clone(), &idle)
                .await
                .is_err()
            {
                runtime.bootstrap.failed();
            }
            if runtime.tick(auth.clone(), &idle).await.is_err() {
                if let Ok(mut active) = runtime.session.try_lock() {
                    if let Some(s) = active.as_mut() {
                        if s.phase != State::Closed {
                            s.phase = State::Unknown;
                            s.token = None;
                            s.cancelled = true;
                            let _ = runtime.publish_cancel(s);
                            s.phase = State::Unknown;
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    });
}
pub(crate) async fn shutdown() {
    if let Some(runtime) = RUNTIME.get() {
        runtime.running.store(false, Ordering::SeqCst);
        runtime.bootstrap.stop();
        if let Ok(mut state) = runtime.session.try_lock() {
            if let Some(s) = state.as_mut() {
                if s.phase != State::Closed {
                    s.cancelled = true;
                    s.token = None;
                    let _ = runtime.publish_cancel(s);
                }
            }
        }
    }
}
pub(crate) async fn authorized(
    auth: Arc<Mutex<Auth>>,
    calls: Option<crate::calls::Controller>,
    command: AuthCommand,
) -> Result<Value> {
    let (token, recent) = match &command {
        AuthCommand::SetupApStatus { token } => (token.clone(), false),
        AuthCommand::SetupApStart { token, .. } | AuthCommand::SetupApCancel { token, .. } => {
            (token.clone(), true)
        }
        _ => return Err(Error::InvalidInput),
    };
    owner(auth.clone(), token, recent).await?;
    let Some(runtime) = RUNTIME.get() else {
        return if matches!(command, AuthCommand::SetupApStatus { .. }) {
            Ok(json!({"available":false,"state":"disabled"}))
        } else {
            Err(Error::Forbidden)
        };
    };
    match command {
        AuthCommand::SetupApStatus { token } => runtime.status(auth, token).await,
        AuthCommand::SetupApStart {
            token,
            idempotency_key,
        } => {
            runtime
                .start(auth, token, idempotency_key, &Calls(calls))
                .await
        }
        AuthCommand::SetupApCancel {
            token,
            operation_id,
        } => runtime.cancel(auth, token, operation_id).await,
        _ => Err(Error::InvalidInput),
    }
}

#[cfg(test)]
mod tests;
