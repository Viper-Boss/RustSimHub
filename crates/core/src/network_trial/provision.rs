// SPDX-License-Identifier: AGPL-3.0-only
//! Owner-confirmed WPA2 provisioning. NM owns the finite rollback timer, not the Core task.
use super::*;
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use zeroize::Zeroizing;

static ENABLED: OnceLock<bool> = OnceLock::new();
static ACTIVE: tokio::sync::Mutex<Option<Session>> = tokio::sync::Mutex::const_new(None);
const DEADLINE: Duration = Duration::from_secs(40);
pub(crate) fn enabled() -> bool {
    ENABLED.get() == Some(&true)
}
pub(crate) fn enable(path: &Path) -> Result<()> {
    super::enable(path)?;
    ENABLED.set(true).map_err(|_| Error::Forbidden)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Starting,
    Connecting,
    Ready,
    Saving,
    Saved,
    RolledBack,
    Unknown,
}
impl State {
    fn terminal(self) -> bool {
        matches!(self, Self::Saved | Self::RolledBack | Self::Unknown)
    }
}
struct Credentials {
    ssid: String,
    passphrase: Zeroizing<String>,
}
impl Credentials {
    fn valid(&self) -> bool {
        rustsimhub_wire::wifi_credentials_valid(&self.ssid, &self.passphrase)
    }
    fn fingerprint(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update((self.ssid.len() as u64).to_be_bytes());
        h.update(self.ssid.as_bytes());
        h.update(self.passphrase.as_bytes());
        h.finalize().into()
    }
}
struct Session {
    id: String,
    key: String,
    principal: Principal,
    fingerprint: [u8; 32],
    state: Arc<tokio::sync::Mutex<State>>,
    control: mpsc::Sender<Control>,
    deadline: Instant,
}
struct Control {
    confirm: bool,
    reply: oneshot::Sender<State>,
}
fn view(id: &str, state: State, deadline: Instant) -> Value {
    json!({"trial_id":id,"state":state,"expires_in_seconds":deadline.saturating_duration_since(Instant::now()).as_secs(),
        "wifi_only":true,"requires_confirmation":!state.terminal(),"credentials_returned":false,"cellular_activation":false})
}
async fn owner(auth: Arc<Mutex<Auth>>, token: String) -> Result<Principal> {
    tokio::task::spawn_blocking(move || {
        let mut a = auth.try_lock().map_err(|_| Error::Busy)?;
        let p = a.recent_owner(&token)?;
        if a.authorize(&token, Scope::NetworkManage)? != p {
            return Err(Error::Forbidden);
        }
        Ok(p)
    })
    .await
    .map_err(|_| Error::Busy)?
}
struct OwnerGate {
    auth: Arc<Mutex<Auth>>,
    token: String,
    expected: Principal,
}
impl Gate for OwnerGate {
    async fn check(&self) -> Result<()> {
        if owner(self.auth.clone(), self.token.clone()).await? != self.expected {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
struct Audit {
    limit: Duration,
    started: PathBuf,
    confirmation: PathBuf,
    result: PathBuf,
    record: Value,
}
impl Audit {
    fn new(dir: &Path, key: &str, p: &Principal) -> Result<Self> {
        directory(dir)?;
        let digest = Sha256::digest(key.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let prefix = dir.join(format!("wifi-{digest}"));
        Ok(Self {
            limit: DEADLINE,
            started: prefix.with_extension("started"),
            confirmation: prefix.with_extension("confirmation"),
            result: prefix.with_extension("result"),
            record: json!({"schema":1,"principal":p.id,"generation":p.generation}),
        })
    }
    fn write(&self, path: &Path, value: &Value) -> Result<()> {
        rustsimhub_auth::write_private_new(
            path,
            &serde_json::to_vec(value).map_err(|_| Error::InvalidInput)?,
        )
    }
    fn begin(&self) -> Result<()> {
        if std::fs::read_dir(self.started.parent().ok_or(Error::Forbidden)?)
            .map_err(|_| Error::Io)?
            .take(385)
            .count()
            >= 384
        {
            return Err(Error::Busy);
        }
        self.write(&self.started, &self.record)
    }
    fn confirm(&self, id: &str) -> Result<()> {
        // The user's choice becomes durable BEFORE attempting persistence. A crash cannot be
        // advertised as a known failed save, nor may a request silently replay after restart.
        let mut record = self.record.clone();
        record["trial_id"] = id.into();
        self.write(&self.confirmation, &record)
    }
    fn finish(&self, state: State) -> Result<()> {
        let mut record = self.record.clone();
        record["state"] = serde_json::to_value(state).map_err(|_| Error::Io)?;
        self.write(&self.result, &record)
    }
}
trait ProvisionBackend {
    fn start(
        &mut self,
        c: &Credentials,
        id: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn ready(&self) -> impl std::future::Future<Output = Result<bool>> + Send;
    fn commit(&mut self, c: &Credentials) -> impl std::future::Future<Output = Result<()>> + Send;
    fn rollback(&mut self) -> impl std::future::Future<Output = Result<()>> + Send;
}
async fn run_trial(
    b: &mut impl ProvisionBackend,
    g: &impl Gate,
    c: Credentials,
    id: &str,
    a: &Audit,
    state: &tokio::sync::Mutex<State>,
    rx: &mut mpsc::Receiver<Control>,
) {
    let mut terminal = State::Unknown;
    let mut pending_reply = None;
    let outcome = timeout(a.limit, async {
        g.check().await?;
        b.start(&c, id).await?;
        *state.lock().await = State::Connecting;
        loop {
            g.check().await?;
            let ready = b.ready().await?;
            *state.lock().await = if ready {
                State::Ready
            } else {
                State::Connecting
            };
            tokio::select! {
                message=rx.recv()=>{
                    let Some(control)=message else { return Ok(false); };
                    pending_reply=Some(control.reply);
                    if !control.confirm { return Ok(false); }
                    if !ready { return Err(Error::Forbidden); }
                    g.check().await?;
                    a.confirm(id)?;
                    *state.lock().await=State::Saving;
                    b.commit(&c).await?;
                    g.check().await?;
                    return Ok(true);
                }
                _=tokio::time::sleep(Duration::from_millis(150))=>{}
            }
        }
    })
    .await;
    match outcome {
        Ok(Ok(true)) => terminal = State::Saved,
        _ => {
            if timeout(Duration::from_secs(5), b.rollback())
                .await
                .is_ok_and(|r| r.is_ok())
            {
                terminal = State::RolledBack;
            }
        }
    }
    if a.finish(terminal).is_err() {
        terminal = State::Unknown;
    }
    *state.lock().await = terminal;
    if let Some(reply) = pending_reply {
        let _ = reply.send(terminal);
    }
}
pub(crate) async fn authorized(auth: Arc<Mutex<Auth>>, command: AuthCommand) -> Result<Value> {
    if !enabled() {
        return Err(Error::Forbidden);
    }
    match command {
        AuthCommand::WifiBegin {
            token,
            idempotency_key,
            ssid,
            passphrase,
        } => {
            let credentials = Credentials {
                ssid,
                passphrase: Zeroizing::new(passphrase),
            };
            if !credentials.valid()
                || idempotency_key.is_empty()
                || idempotency_key.len() > 128
                || idempotency_key.chars().any(char::is_control)
            {
                return Err(Error::InvalidInput);
            }
            let p = owner(auth.clone(), token.clone()).await?;
            let mut active = ACTIVE.try_lock().map_err(|_| Error::Busy)?;
            if let Some(old) = active.as_ref() {
                let old_state = *old.state.lock().await;
                if old.key == idempotency_key {
                    if old.principal != p || old.fingerprint != credentials.fingerprint() {
                        return Err(Error::Forbidden);
                    }
                    return Ok(view(&old.id, old_state, old.deadline));
                }
                if !old_state.terminal() {
                    return Err(Error::Busy);
                }
            }
            let permit = TRIALS.try_acquire().map_err(|_| Error::Busy)?;
            let audit = Audit::new(
                DIRECTORY.get().ok_or(Error::Forbidden)?,
                &idempotency_key,
                &p,
            )?;
            audit.begin()?;
            let mut bytes = [0u8; 32];
            getrandom::fill(&mut bytes).map_err(|_| Error::Entropy)?;
            let id = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
            let state = Arc::new(tokio::sync::Mutex::new(State::Starting));
            let (tx, mut rx) = mpsc::channel(1);
            let deadline = Instant::now() + DEADLINE;
            let session = Session {
                id: id.clone(),
                key: idempotency_key,
                principal: p.clone(),
                fingerprint: credentials.fingerprint(),
                state: state.clone(),
                control: tx,
                deadline,
            };
            *active = Some(session);
            tokio::spawn(async move {
                let gate = OwnerGate {
                    auth,
                    token,
                    expected: p,
                };
                let mut backend = NmProvision::default();
                run_trial(
                    &mut backend,
                    &gate,
                    credentials,
                    &id,
                    &audit,
                    &state,
                    &mut rx,
                )
                .await;
                drop(permit);
            });
            Ok(view(
                &active.as_ref().ok_or(Error::Io)?.id,
                State::Starting,
                deadline,
            ))
        }
        AuthCommand::WifiStatus { token, trial_id } => {
            let p = owner(auth, token).await?;
            let active = ACTIVE.try_lock().map_err(|_| Error::Busy)?;
            let s = active.as_ref().ok_or(Error::Forbidden)?;
            if s.id != trial_id || s.principal != p {
                return Err(Error::Forbidden);
            }
            let state = *s.state.lock().await;
            Ok(view(&s.id, state, s.deadline))
        }
        AuthCommand::WifiConfirm { token, trial_id } => control(auth, token, trial_id, true).await,
        AuthCommand::WifiCancel { token, trial_id } => control(auth, token, trial_id, false).await,
        _ => Err(Error::Forbidden),
    }
}
async fn control(
    auth: Arc<Mutex<Auth>>,
    token: String,
    id: String,
    confirm: bool,
) -> Result<Value> {
    let p = owner(auth, token).await?;
    let (tx, deadline) = {
        let active = ACTIVE.try_lock().map_err(|_| Error::Busy)?;
        let s = active.as_ref().ok_or(Error::Forbidden)?;
        if s.id != id || s.principal != p {
            return Err(Error::Forbidden);
        }
        let state = *s.state.lock().await;
        if state.terminal() {
            return Ok(view(&id, state, s.deadline));
        }
        if confirm && (state != State::Ready || Instant::now() >= s.deadline) {
            return Err(Error::Forbidden);
        }
        (s.control.clone(), s.deadline)
    };
    let (reply, rx) = oneshot::channel();
    tx.try_send(Control { confirm, reply })
        .map_err(|_| Error::Busy)?;
    let state = timeout(Duration::from_secs(8), rx)
        .await
        .map_err(|_| Error::Busy)?
        .map_err(|_| Error::Busy)?;
    Ok(view(&id, state, deadline))
}
pub(crate) async fn shutdown() {
    if !enabled() {
        return;
    }
    let tx = {
        let active = ACTIVE.lock().await;
        active.as_ref().map(|s| s.control.clone())
    };
    if let Some(tx) = tx {
        let (reply, rx) = oneshot::channel();
        let _ = tx.try_send(Control {
            confirm: false,
            reply,
        });
        let _ = timeout(Duration::from_secs(6), rx).await;
    }
}

type Settings = HashMap<String, HashMap<String, OwnedValue>>;
fn string(value: &str) -> OwnedValue {
    OwnedValue::from(zbus::zvariant::Str::from(value))
}
fn uuid(id: &str) -> String {
    format!(
        "{}-{}-4{}-8{}-{}",
        &id[..8],
        &id[8..12],
        &id[12..15],
        &id[15..18],
        &id[18..30]
    )
}
fn settings(c: &Credentials, id: &str, confirmed: bool) -> Result<Settings> {
    Ok(HashMap::from([
        (
            "connection".into(),
            HashMap::from([
                ("id".into(), string("RustSimHub Wi-Fi")),
                ("uuid".into(), string(&uuid(id))),
                ("type".into(), string("802-11-wireless")),
                ("interface-name".into(), string("wlan0")),
                ("autoconnect".into(), OwnedValue::from(confirmed)),
                ("autoconnect-priority".into(), OwnedValue::from(50i32)),
            ]),
        ),
        (
            "802-11-wireless".into(),
            HashMap::from([
                (
                    "ssid".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(c.ssid.as_bytes().to_vec()))
                        .map_err(|_| Error::InvalidInput)?,
                ),
                ("mode".into(), string("infrastructure")),
                ("hidden".into(), OwnedValue::from(false)),
            ]),
        ),
        (
            "802-11-wireless-security".into(),
            HashMap::from([
                ("key-mgmt".into(), string("wpa-psk")),
                (
                    "proto".into(),
                    OwnedValue::try_from(zbus::zvariant::Value::from(vec!["rsn"]))
                        .map_err(|_| Error::InvalidInput)?,
                ),
                ("psk".into(), string(&c.passphrase)),
                ("psk-flags".into(), OwnedValue::from(0u32)),
            ]),
        ),
        (
            "ipv4".into(),
            HashMap::from([
                ("method".into(), string("auto")),
                ("may-fail".into(), OwnedValue::from(false)),
            ]),
        ),
        (
            "ipv6".into(),
            HashMap::from([
                ("method".into(), string("disabled")),
                ("never-default".into(), OwnedValue::from(true)),
            ]),
        ),
    ]))
}
#[derive(Default)]
struct NmProvision {
    nm: Option<NetworkManager>,
    base: Option<Snapshot>,
    cp: Option<OwnedObjectPath>,
    profile: Option<OwnedObjectPath>,
    id: String,
    ssid: Vec<u8>,
    activation_uncertain: bool,
}
impl NmProvision {
    fn parts(&self) -> Result<(&NetworkManager, &Snapshot)> {
        Ok((
            self.nm.as_ref().ok_or(Error::Io)?,
            self.base.as_ref().ok_or(Error::Io)?,
        ))
    }
    async fn exact_profile(&self) -> Result<()> {
        let (nm, b) = self.parts()?;
        nm.owner_fence(b).await?;
        let proxy = nm
            .proxy(
                &b.owner,
                self.profile.as_ref().ok_or(Error::Io)?.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await?;
        let s: Settings = proxy
            .call("GetSettings", &())
            .await
            .map_err(|_| Error::Io)?;
        let connection = s.get("connection").ok_or(Error::Forbidden)?;
        let ssid: Vec<u8> = s
            .get("802-11-wireless")
            .and_then(|w| w.get("ssid"))
            .ok_or(Error::Forbidden)?
            .try_clone()
            .map_err(|_| Error::Io)?
            .try_into()
            .map_err(|_| Error::Forbidden)?;
        if connection
            .get("uuid")
            .and_then(|v| <&str>::try_from(v).ok())
            != Some(uuid(&self.id).as_str())
            || connection
                .get("type")
                .and_then(|v| <&str>::try_from(v).ok())
                != Some("802-11-wireless")
            || connection
                .get("interface-name")
                .and_then(|v| <&str>::try_from(v).ok())
                != Some("wlan0")
            || ssid != self.ssid
        {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    async fn checkpoint_present(&self) -> Result<bool> {
        let (nm, b) = self.parts()?;
        nm.owner_fence(b).await?;
        let proxy = nm.proxy(&b.owner, ROOT, NM).await?;
        let paths: Vec<OwnedObjectPath> = proxy
            .get_property("Checkpoints")
            .await
            .map_err(|_| Error::Io)?;
        Ok(self.cp.as_ref().is_some_and(|cp| paths.contains(cp)))
    }
}
impl ProvisionBackend for NmProvision {
    async fn start(&mut self, c: &Credentials, id: &str) -> Result<()> {
        self.nm = Some(NetworkManager {
            c: zbus::Connection::system().await.map_err(|_| Error::Io)?,
        });
        self.id = id.into();
        self.ssid = c.ssid.as_bytes().to_vec();
        let nm = self.nm.as_ref().ok_or(Error::Io)?;
        self.base = Some(nm.snapshot_inner(true).await?);
        let b = self.base.as_ref().ok_or(Error::Io)?;
        nm.cellular_autoconnect_absent(&b.owner).await?;
        nm.fence(b).await?;
        let proxy = nm.proxy(&b.owner, ROOT, NM).await?;
        let cp: OwnedObjectPath = proxy
            .call("CheckpointCreate", &(vec![b.wifi.clone()], 45u32, 0u32))
            .await
            .map_err(|_| Error::Io)?;
        if !cp
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Checkpoint/")
        {
            return Err(Error::Forbidden);
        }
        self.cp = Some(cp);
        nm.fence(b).await?;
        if b.profile.as_str() != "/" && nm.active_profile(&b.owner, &b.wifi).await? != b.profile {
            return Err(Error::Forbidden);
        }
        let options = HashMap::from([
            ("persist", string("volatile")),
            ("bind-activation", string("dbus-client")),
        ]);
        self.activation_uncertain = true;
        let (profile, active, _): (
            OwnedObjectPath,
            OwnedObjectPath,
            HashMap<String, OwnedValue>,
        ) = proxy
            .call(
                "AddAndActivateConnection2",
                &(
                    settings(c, id, false)?,
                    &b.wifi,
                    OwnedObjectPath::try_from("/").map_err(|_| Error::Io)?,
                    options,
                ),
            )
            .await
            .map_err(|_| Error::Io)?;
        if !profile
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/Settings/")
            || !active
                .as_str()
                .starts_with("/org/freedesktop/NetworkManager/ActiveConnection/")
        {
            return Err(Error::Forbidden);
        }
        self.profile = Some(profile);
        self.activation_uncertain = false;
        nm.fence(b).await?;
        self.exact_profile().await
    }
    async fn ready(&self) -> Result<bool> {
        let (nm, b) = self.parts()?;
        nm.fence(b).await?;
        nm.cellular_autoconnect_absent(&b.owner).await?;
        if !self.checkpoint_present().await? {
            return Err(Error::Forbidden);
        }
        self.exact_profile().await?;
        if nm.active_profile(&b.owner, &b.wifi).await.ok().as_ref() != self.profile.as_ref() {
            return Ok(false);
        }
        let device = nm
            .proxy(
                &b.owner,
                b.wifi.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;
        let wireless = nm
            .proxy(
                &b.owner,
                b.wifi.as_str(),
                "org.freedesktop.NetworkManager.Device.Wireless",
            )
            .await?;
        let access: OwnedObjectPath = wireless
            .get_property("ActiveAccessPoint")
            .await
            .map_err(|_| Error::Io)?;
        if !access
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/AccessPoint/")
        {
            return Ok(false);
        }
        let ap = nm
            .proxy(
                &b.owner,
                access.as_str(),
                "org.freedesktop.NetworkManager.AccessPoint",
            )
            .await?;
        let actual_ssid: Vec<u8> = ap.get_property("Ssid").await.map_err(|_| Error::Io)?;
        if actual_ssid != self.ssid {
            return Err(Error::Forbidden);
        }
        let ip: OwnedObjectPath = device
            .get_property("Ip4Config")
            .await
            .map_err(|_| Error::Io)?;
        if !ip
            .as_str()
            .starts_with("/org/freedesktop/NetworkManager/IP4Config/")
        {
            return Ok(false);
        }
        let proxy = nm
            .proxy(
                &b.owner,
                ip.as_str(),
                "org.freedesktop.NetworkManager.IP4Config",
            )
            .await?;
        let rows: Vec<HashMap<String, OwnedValue>> = proxy
            .get_property("AddressData")
            .await
            .map_err(|_| Error::Io)?;
        if rows.len() > 16 {
            return Err(Error::Forbidden);
        }
        Ok(rows.iter().any(|r| {
            r.get("address")
                .and_then(|v| <&str>::try_from(v).ok())
                .and_then(|s| s.parse::<std::net::Ipv4Addr>().ok())
                .is_some_and(|ip| {
                    !ip.is_unspecified()
                        && !ip.is_loopback()
                        && !ip.is_link_local()
                        && !ip.is_multicast()
                })
        }))
    }
    async fn commit(&mut self, c: &Credentials) -> Result<()> {
        if !self.ready().await? {
            return Err(Error::Forbidden);
        }
        let (nm, b) = self.parts()?;
        let proxy = nm
            .proxy(
                &b.owner,
                self.profile.as_ref().ok_or(Error::Io)?.as_str(),
                "org.freedesktop.NetworkManager.Settings.Connection",
            )
            .await?;
        let _: HashMap<String, OwnedValue> = proxy
            .call(
                "Update2",
                &(
                    settings(c, &self.id, true)?,
                    1u32,
                    HashMap::<String, OwnedValue>::new(),
                ),
            )
            .await
            .map_err(|_| Error::Io)?;
        self.exact_profile().await?;
        if proxy
            .get_property::<bool>("Unsaved")
            .await
            .map_err(|_| Error::Io)?
            || proxy
                .get_property::<u32>("Flags")
                .await
                .map_err(|_| Error::Io)?
                & 4
                != 0
        {
            return Err(Error::Forbidden);
        }
        if !self.ready().await? {
            return Err(Error::Forbidden);
        }
        let root = nm.proxy(&b.owner, ROOT, NM).await?;
        root.call::<_, _, ()>("CheckpointDestroy", &(self.cp.as_ref().ok_or(Error::Io)?,))
            .await
            .map_err(|_| Error::Io)?;
        nm.fence(b).await?;
        if self.checkpoint_present().await? {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    async fn rollback(&mut self) -> Result<()> {
        if self.base.is_none() {
            return Ok(());
        }
        let (nm, b) = self.parts()?;
        nm.owner_fence(b).await?;
        if self.checkpoint_present().await? {
            nm.restore(b, self.cp.as_ref().ok_or(Error::Io)?).await?;
        }
        if let Some(profile) = &self.profile {
            let root = nm
                .proxy(
                    &b.owner,
                    "/org/freedesktop/NetworkManager/Settings",
                    "org.freedesktop.NetworkManager.Settings",
                )
                .await?;
            let profiles: Vec<OwnedObjectPath> = root
                .call("ListConnections", &())
                .await
                .map_err(|_| Error::Io)?;
            if profiles.len() > 64 {
                return Err(Error::Forbidden);
            }
            if profiles.contains(profile) {
                self.exact_profile().await?;
                nm.proxy(
                    &b.owner,
                    profile.as_str(),
                    "org.freedesktop.NetworkManager.Settings.Connection",
                )
                .await?
                .call::<_, _, ()>("Delete", &())
                .await
                .map_err(|_| Error::Io)?;
            }
        }
        if b.profile.as_str() != "/"
            && nm.active_profile(&b.owner, &b.wifi).await.ok().as_ref() != Some(&b.profile)
        {
            let proxy = nm.proxy(&b.owner, ROOT, NM).await?;
            let _: OwnedObjectPath = proxy
                .call(
                    "ActivateConnection",
                    &(
                        &b.profile,
                        &b.wifi,
                        OwnedObjectPath::try_from("/").map_err(|_| Error::Io)?,
                    ),
                )
                .await
                .map_err(|_| Error::Io)?;
        }
        let restored: Result<()> = timeout(Duration::from_secs(3), async {
            loop {
                nm.owner_fence(b).await?;
                let disconnected = if b.profile.as_str() == "/" {
                    nm.proxy(
                        &b.owner,
                        b.wifi.as_str(),
                        "org.freedesktop.NetworkManager.Device",
                    )
                    .await?
                    .get_property::<u32>("State")
                    .await
                    .map_err(|_| Error::Io)?
                        == 30
                } else {
                    false
                };
                if disconnected
                    || nm.active_profile(&b.owner, &b.wifi).await.ok().as_ref() == Some(&b.profile)
                {
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .map_err(|_| Error::Busy)?;
        restored?;
        if self.activation_uncertain {
            Err(Error::Forbidden)
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests;
