// SPDX-License-Identifier: AGPL-3.0-only
//! Actual Root/engine ports for the existing finite supervisor. Construction
//! performs no Root IPC, SDK startup or identity work. Only an explicit recent
//! local Owner can create a finite proof; no wire-deserializable handle exists.
use rustsimhub_auth::{nkn_session::NknRootAuthority, Auth};
use rustsimhub_nkn_agent::{
    Budget, EnginePort, EngineStart, EngineState, PortError, RootPort, RootStart, RootState,
};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::Shutdown,
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        net::UnixStream,
    },
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
const SOCKET: &str = "/run/rustsimhub-nkn-root/root.sock";
mod roles;
mod status;
#[cfg(feature = "usage-root-transport-candidate")]
mod transactions;
mod usage;
pub use roles::InstalledRoles;
pub use status::{Report as StatusReport, Status as ReadOnlyStatus};
#[cfg(feature = "usage-root-transport-candidate")]
pub use transactions::{TransactionCall, TransactionConclusion};
type Result<T> = std::result::Result<T, PortError>;
fn boot_ms() -> Result<u64> {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    let secs = u64::try_from(t.tv_sec).map_err(|_| PortError::Protocol)?;
    let nanos = u64::try_from(t.tv_nsec).map_err(|_| PortError::Protocol)?;
    secs.checked_mul(1000)
        .and_then(|v| v.checked_add(nanos / 1_000_000 + 1))
        .ok_or(PortError::Protocol)
}
struct PolicyDetails {
    hash: [u8; 32],
    status: bool,
    usage: bool,
    #[cfg(feature = "usage-root-transport-candidate")]
    transactions: Option<rustsimhub_usage_stats::protocol::StatsPublicKey>,
}
fn policy_details(device: [u8; 32]) -> Result<PolicyDetails> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open("/etc/rustsimhub/nkn-policy.json")
        .map_err(|_| PortError::Unavailable)?;
    let m = file.metadata().map_err(|_| PortError::Io)?;
    if !m.is_file() || m.uid() != 0 || m.nlink() != 1 || m.mode() & 0o022 != 0 || m.len() > 4096 {
        return Err(PortError::Denied);
    }
    let mut raw = vec![];
    std::io::Read::by_ref(&mut file)
        .take(4097)
        .read_to_end(&mut raw)
        .map_err(|_| PortError::Io)?;
    if raw.is_empty() || raw.len() > 4096 {
        return Err(PortError::Denied);
    }
    let start = raw
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .ok_or(PortError::Denied)?;
    let end = raw
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .ok_or(PortError::Denied)?;
    let policy: serde_json::Value =
        serde_json::from_slice(&raw[start..=end]).map_err(|_| PortError::Protocol)?;
    let expected = device
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if policy["core_pin"].as_str() != Some(expected.as_str()) {
        return Err(PortError::Denied);
    }
    Ok(PolicyDetails {
        hash: Sha256::digest(&raw[start..=end]).into(),
        status: policy["status_protocol"].as_u64() == Some(1),
        usage: policy["usage_query_protocol"].as_u64() == Some(1),
        #[cfg(feature = "usage-root-transport-candidate")]
        transactions: transactions::policy_key(&raw[start..=end], device)?,
    })
}
fn connect() -> Result<UnixStream> {
    use rustix::net::{AddressFamily, SocketAddrUnix, SocketFlags, SocketType};
    let fd = rustix::net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::CLOEXEC | SocketFlags::NONBLOCK,
        None,
    )
    .map_err(|_| PortError::Io)?;
    let address = SocketAddrUnix::new(SOCKET).map_err(|_| PortError::Protocol)?;
    rustix::net::connect(&fd, &address).map_err(|_| PortError::Unavailable)?;
    let peer = rustix::net::sockopt::socket_peercred(&fd).map_err(|_| PortError::Denied)?;
    if !peer.uid.is_root() || peer.pid.as_raw_nonzero().get() <= 1 {
        return Err(PortError::Denied);
    }
    let stream = UnixStream::from(fd);
    stream.set_nonblocking(false).map_err(|_| PortError::Io)?;
    Ok(stream)
}
#[derive(Clone, Copy)]
struct Observation {
    generation: u32,
    root: u8,
    engine: u8,
    pid: u32,
}
struct Connection {
    stream: Option<UnixStream>,
    poisoned: bool,
}
struct Shared {
    auth: Arc<Mutex<Auth>>,
    authority: NknRootAuthority,
    generation: AtomicU32,
    nonce: [u8; 32],
    policy: [u8; 32],
    deadline: Instant,
    boot_deadline: u64,
    live: AtomicBool,
    sent: AtomicBool,
    connection: Mutex<Connection>,
    observed_ready: AtomicBool,
    observed_pid: AtomicU32,
    observed_at: AtomicU64,
    observed_root: AtomicU32,
    observed_engine: AtomicU32,
    sdk_status: bool,
    sdk_usage: bool,
    #[cfg(feature = "usage-root-transport-candidate")]
    transaction_key: Option<rustsimhub_usage_stats::protocol::StatsPublicKey>,
    host_closed: OnceLock<Arc<AtomicBool>>,
}
impl Drop for Shared {
    fn drop(&mut self) {
        self.authority.invalidate();
        if let Ok(c) = self.connection.get_mut() {
            if let Some(s) = &c.stream {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
    }
}
impl Shared {
    fn local_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
            && self
                .host_closed
                .get()
                .is_none_or(|closed| !closed.load(Ordering::Acquire))
            && Instant::now() < self.deadline
            && boot_ms().is_ok_and(|now| now < self.boot_deadline)
    }
    fn authority(&self) -> Result<()> {
        if self
            .host_closed
            .get()
            .is_some_and(|closed| closed.load(Ordering::Acquire))
        {
            self.fence();
            return Err(PortError::Denied);
        }
        if !self.live.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
            || boot_ms()? >= self.boot_deadline
        {
            return Err(PortError::Denied);
        }
        let valid = self
            .auth
            .try_lock()
            .map_err(|_| PortError::Unavailable)?
            .check_nkn_root_authority(&self.authority)
            .is_ok();
        if !valid {
            self.fence();
            return Err(PortError::Denied);
        }
        Ok(())
    }
    fn fence(&self) {
        self.live.store(false, Ordering::Release);
        self.authority.invalidate();
        if let Ok(mut c) = self.connection.try_lock() {
            c.poisoned = true;
            if let Some(s) = c.stream.take() {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
    }
    fn request(&self, op: u8, deadline: Instant, cleanup: bool) -> Result<Observation> {
        if !cleanup {
            self.authority()?
        }
        if Instant::now() >= deadline {
            return Err(PortError::Deadline);
        }
        let mut c = self
            .connection
            .try_lock()
            .map_err(|_| PortError::Unavailable)?;
        if c.poisoned && op != 3 && op != 5 {
            return Err(PortError::Denied);
        }
        let recovery = c.poisoned;
        if c.stream.is_none() {
            c.stream = Some(connect()?)
        }
        let stream = c.stream.as_mut().ok_or(PortError::Unavailable)?;
        let mut raw = [0u8; 82];
        raw[..5].copy_from_slice(b"RSCN\x01");
        raw[5] = op;
        let generation = self.generation.load(Ordering::Acquire);
        raw[6..10].copy_from_slice(&generation.to_be_bytes());
        raw[10..18].copy_from_slice(&self.boot_deadline.to_be_bytes());
        raw[18..50].copy_from_slice(&self.nonce);
        raw[50..].copy_from_slice(&self.policy);
        let result = (|| {
            stream
                .set_write_timeout(Some(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_nanos(1)),
                ))
                .map_err(|_| PortError::Io)?;
            self.sent.store(true, Ordering::Release);
            if stream.write(&raw).map_err(|_| PortError::Io)? != raw.len() {
                return Err(PortError::Protocol);
            }
            let mut reply = [0u8; 87];
            let mut offset = 0;
            while offset < reply.len() {
                let now = Instant::now();
                if now >= deadline {
                    return Err(PortError::Deadline);
                }
                stream
                    .set_read_timeout(Some(deadline - now))
                    .map_err(|_| PortError::Io)?;
                let n = stream
                    .read(&mut reply[offset..])
                    .map_err(|_| PortError::Io)?;
                if n == 0 {
                    return Err(PortError::Io);
                };
                offset += n;
            }
            let obs = parse_reply(
                &reply,
                generation,
                self.boot_deadline,
                &self.nonce,
                &self.policy,
            )?;
            if Instant::now() >= deadline {
                return Err(PortError::Deadline);
            }
            if !cleanup {
                self.authority()?
            }
            Ok(obs)
        })();
        if result.is_err() || recovery {
            if let Some(s) = c.stream.take() {
                let _ = s.shutdown(Shutdown::Both);
            }
            c.poisoned = true;
        }
        drop(c);
        if let Ok(obs) = result {
            if generation == 0 {
                self.generation
                    .compare_exchange(0, obs.generation, Ordering::AcqRel, Ordering::Acquire)
                    .map_err(|_| PortError::Protocol)?;
            }
            if obs.root != 1 || obs.engine != 2 {
                self.observed_ready.store(false, Ordering::Release);
            }
            self.observed_pid.store(obs.pid, Ordering::Release);
            self.observed_root
                .store(u32::from(obs.root), Ordering::Release);
            self.observed_engine
                .store(u32::from(obs.engine), Ordering::Release);
            self.observed_at.store(boot_ms()?, Ordering::Release);
            if obs.root == 1 && obs.engine == 2 {
                self.observed_ready.store(true, Ordering::Release);
            }
            Ok(obs)
        } else {
            self.live.store(false, Ordering::Release);
            result
        }
    }
    fn source_live(&self, pid: u32) -> bool {
        if !self.local_live() {
            return false;
        }
        let at = self.observed_at.load(Ordering::Acquire);
        self.observed_ready.load(Ordering::Acquire)
            && self.observed_pid.load(Ordering::Acquire) == pid
            && boot_ms().is_ok_and(|now| now >= at && now - at < 1000)
    }
    fn observation(&self, op: u8, deadline: Instant, cleanup: bool) -> Result<Observation> {
        match self.request(op, deadline, cleanup) {
            Err(PortError::Unavailable) if op == 3 && !cleanup => {
                // Another in-flight read can hold the one owned descriptor.
                // Reuse only the already verified, <=1s observation of this
                // same ready child; never use this for start/stop/release.
                let pid = self.observed_pid.load(Ordering::Acquire);
                if !self.source_live(pid) {
                    return Err(PortError::Unavailable);
                }
                Ok(Observation {
                    generation: self.generation.load(Ordering::Acquire),
                    root: 1,
                    engine: 2,
                    pid,
                })
            }
            result => result,
        }
    }
}
fn parse_reply(
    raw: &[u8],
    generation: u32,
    deadline: u64,
    nonce: &[u8; 32],
    policy: &[u8; 32],
) -> Result<Observation> {
    if raw.len() != 87
        || &raw[..5] != b"RSHS\x01"
        || generation != 0 && raw[5..9] != generation.to_be_bytes()
        || raw[5..9] == [0; 4]
        || raw[9..17] != deadline.to_be_bytes()
        || raw[17..49] != *nonce
        || raw[49..81] != *policy
        || !(1..=3).contains(&raw[85])
        || !(1..=4).contains(&raw[86])
    {
        return Err(PortError::Protocol);
    }
    let pid = u32::from_be_bytes(raw[81..85].try_into().map_err(|_| PortError::Protocol)?);
    if raw[85] == 2 && raw[86] != 3 || raw[86] == 2 && pid <= 1 {
        return Err(PortError::Protocol);
    }
    Ok(Observation {
        generation: u32::from_be_bytes(raw[5..9].try_into().map_err(|_| PortError::Protocol)?),
        root: raw[85],
        engine: raw[86],
        pid,
    })
}
pub struct Policy {
    hash: [u8; 32],
}
pub struct Proof(Arc<Shared>);
pub struct Root(Arc<Shared>);
pub struct Engine(Arc<Shared>);
pub struct Process {
    nonce: [u8; 32],
    pid: u32,
}
pub struct Admission {
    shared: Arc<Shared>,
    pid: u32,
    connection: [u8; 32],
    reference: Option<Arc<AtomicBool>>,
}
impl Admission {
    pub fn deadline(&self) -> Instant {
        self.shared.deadline
    }
    pub fn is_live(&self) -> bool {
        self.reference
            .as_ref()
            .is_none_or(|r| r.load(Ordering::Acquire))
            && self.shared.source_live(self.pid)
    }
    pub fn parent_fence(&self) -> rustsimhub_auth::nkn_session::NknParentFence {
        let shared = Arc::downgrade(&self.shared);
        let pid = self.pid;
        let reference = self.reference.clone();
        Arc::new(move || {
            reference.as_ref().is_none_or(|r| r.load(Ordering::Acquire))
                && shared.upgrade().is_some_and(|s| s.source_live(pid))
        })
    }
    pub fn matches(&self, peer: &crate::recovery_transport::PeerContext) -> bool {
        peer.pid() > 1
            && peer.pid() as u32 == self.pid
            && peer.connection_nonce() == &self.connection
            && self.is_live()
    }
    pub fn refresh(&self) -> bool {
        match self.shared.request(
            3,
            (Instant::now() + Duration::from_millis(750)).min(self.shared.deadline),
            false,
        ) {
            Ok(_) | Err(PortError::Unavailable) => self.is_live(),
            Err(_) => false,
        }
    }
}
pub struct Source {
    shared: Arc<Shared>,
    reference: Option<Arc<AtomicBool>>,
}
impl Source {
    pub fn admit(&self, peer: &crate::recovery_transport::PeerContext) -> Result<Admission> {
        if self
            .reference
            .as_ref()
            .is_some_and(|r| !r.load(Ordering::Acquire))
        {
            return Err(PortError::Denied);
        }
        let mut admission = admit(&self.shared, peer)?;
        admission.reference = self.reference.clone();
        if !admission.is_live() {
            return Err(PortError::Denied);
        }
        Ok(admission)
    }
}
impl Proof {
    /// Admission binds the actual kernel peer PID and fresh original Core
    /// connection nonce to Root's exact currently owned, ready child.
    pub fn admit(&self, peer: &crate::recovery_transport::PeerContext) -> Result<Admission> {
        admit(&self.0, peer)
    }
    pub fn source(&self) -> Source {
        Source {
            shared: self.0.clone(),
            reference: None,
        }
    }
    pub fn invalidate(&self) {
        self.0.fence()
    }
}
fn admit(shared: &Arc<Shared>, peer: &crate::recovery_transport::PeerContext) -> Result<Admission> {
    let deadline = (Instant::now() + Duration::from_secs(1)).min(shared.deadline);
    // Admission is read-only and binds the kernel peer to the SAME original
    // <=1s ready-PID observation. A concurrent status poll cannot silently
    // consume/reject the first authenticated transport record.
    let obs = shared.observation(3, deadline, false)?;
    if obs.root != 1 || obs.engine != 2 || peer.pid() <= 1 || obs.pid != peer.pid() as u32 {
        return Err(PortError::Denied);
    }
    Ok(Admission {
        shared: shared.clone(),
        pid: obs.pid,
        connection: *peer.connection_nonce(),
        reference: None,
    })
}
/// Explicit recent local Owner operation, not status, prepares the opaque
/// policy/proof and actual ports. No Root call occurs until Supervisor.begin.
pub fn authorized_ports(
    auth: Arc<Mutex<Auth>>,
    token: &str,
) -> Result<(Root, Engine, Policy, Proof)> {
    let (authority, device) = {
        let mut current = auth.try_lock().map_err(|_| PortError::Unavailable)?;
        (
            current
                .nkn_root_authority(token)
                .map_err(|_| PortError::Denied)?,
            current.public_key(),
        )
    };
    let deadline = authority.deadline();
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).map_err(|_| PortError::Unavailable)?;
    if nonce == [0; 32] {
        return Err(PortError::Denied);
    }
    let details = policy_details(device)?;
    let policy = details.hash;
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .as_millis();
    let boot_deadline = boot_ms()?
        .checked_add(u64::try_from(remaining).map_err(|_| PortError::Protocol)?)
        .ok_or(PortError::Protocol)?;
    let shared = Arc::new(Shared {
        auth,
        authority,
        generation: AtomicU32::new(0),
        nonce,
        policy,
        deadline,
        boot_deadline,
        live: AtomicBool::new(true),
        sent: AtomicBool::new(false),
        connection: Mutex::new(Connection {
            stream: None,
            poisoned: false,
        }),
        observed_ready: AtomicBool::new(false),
        observed_pid: AtomicU32::new(0),
        observed_at: AtomicU64::new(0),
        observed_root: AtomicU32::new(0),
        observed_engine: AtomicU32::new(0),
        sdk_status: details.status,
        sdk_usage: details.usage,
        #[cfg(feature = "usage-root-transport-candidate")]
        transaction_key: details.transactions,
        host_closed: OnceLock::new(),
    });
    Ok((
        Root(shared.clone()),
        Engine(shared.clone()),
        Policy { hash: policy },
        Proof(shared),
    ))
}
impl RootPort for Root {
    type Policy = Policy;
    type Proof = Proof;
    fn expires_at(&self, p: &Proof) -> Option<Instant> {
        Arc::ptr_eq(&self.0, &p.0).then_some(p.0.deadline)
    }
    fn start(&mut self, policy: &Policy, p: &Proof, b: &Budget) -> RootStart {
        if !Arc::ptr_eq(&self.0, &p.0) || policy.hash != self.0.policy || b.cancelled() {
            return RootStart::Rejected(PortError::Denied);
        }
        match self.0.request(1, b.deadline(), false) {
            Ok(v) if v.root == 1 => RootStart::Active,
            Ok(v) if v.root == 2 => RootStart::Rejected(PortError::Denied),
            Err(error) if !self.0.sent.load(Ordering::Acquire) => RootStart::Rejected(error),
            _ => RootStart::Unknown(PortError::Protocol),
        }
    }
    fn status(&mut self, p: &Proof, b: &Budget) -> Result<RootState> {
        if !Arc::ptr_eq(&self.0, &p.0) {
            return Err(PortError::Denied);
        }
        let v = self.0.observation(3, b.deadline(), b.is_cleanup())?;
        Ok(match v.root {
            1 => RootState::Active,
            2 => RootState::Released,
            _ => RootState::Unknown,
        })
    }
    fn stop(&mut self, p: &Proof, b: &Budget) -> Result<RootState> {
        if !Arc::ptr_eq(&self.0, &p.0) {
            return Err(PortError::Denied);
        }
        let v = self.0.request(5, b.deadline(), true)?;
        self.0.live.store(false, Ordering::Release);
        Ok(if v.root == 2 {
            RootState::Released
        } else {
            RootState::Unknown
        })
    }
}
fn engine_state(v: Observation) -> EngineState {
    match v.engine {
        1 => EngineState::Starting,
        2 => EngineState::Ready,
        3 => EngineState::Exited,
        _ => EngineState::Unknown,
    }
}
impl EnginePort<Proof> for Engine {
    type Process = Process;
    fn supported(&self) -> bool {
        true
    }
    fn start(&mut self, p: &Proof, b: &Budget) -> EngineStart<Process> {
        if !Arc::ptr_eq(&self.0, &p.0) || b.cancelled() {
            return EngineStart::Rejected(PortError::Denied);
        }
        match self.0.request(2, b.deadline(), false) {
            Ok(v) if v.root == 1 && v.pid > 1 => EngineStart::Started(Process {
                nonce: self.0.nonce,
                pid: v.pid,
            }),
            _ => EngineStart::Unknown(PortError::Protocol),
        }
    }
    fn status(&mut self, p: &mut Process, b: &Budget) -> Result<EngineState> {
        if p.nonce != self.0.nonce {
            return Err(PortError::Denied);
        }
        let v = self.0.observation(3, b.deadline(), b.is_cleanup())?;
        if v.pid != p.pid {
            return Err(PortError::Denied);
        };
        Ok(engine_state(v))
    }
    fn stop(&mut self, p: &mut Process, b: &Budget) -> Result<EngineState> {
        if p.nonce != self.0.nonce {
            return Err(PortError::Denied);
        }
        let v = self.0.request(4, b.deadline(), true)?;
        if v.pid != p.pid {
            return Err(PortError::Denied);
        };
        Ok(engine_state(v))
    }
}

/// Core-owned assembly of the original three-reference supervisor and these
/// actual Root/SDK ports. Status construction and reads never activate Root.
pub struct Controller {
    closed: Arc<AtomicBool>,
    auth: Arc<Mutex<Auth>>,
    run: Mutex<Option<Run>>,
}
struct Run {
    supervisor: rustsimhub_nkn_agent::Supervisor<Root, Engine, rustsimhub_nkn_agent::SystemClock>,
    source: Arc<Source>,
    references: [Option<rustsimhub_nkn_agent::Reference>; 3],
    remote: Option<Arc<AtomicBool>>,
    #[cfg(feature = "usage-root-transport-candidate")]
    statistics_epoch: u64,
}
impl Drop for Run {
    fn drop(&mut self) {
        if let Some(remote) = &self.remote {
            remote.store(false, Ordering::Release)
        };
        self.source.shared.fence();
    }
}
fn owner_index(owner: rustsimhub_nkn_agent::Owner) -> usize {
    match owner {
        rustsimhub_nkn_agent::Owner::ManualEngine => 0,
        rustsimhub_nkn_agent::Owner::Remote => 1,
        rustsimhub_nkn_agent::Owner::Statistics => 2,
    }
}
impl Controller {
    pub fn new(auth: Arc<Mutex<Auth>>) -> Self {
        Self {
            closed: Arc::new(AtomicBool::new(false)),
            auth,
            run: Mutex::new(None),
        }
    }
    /// Irreversible host shutdown. Parent fences fail immediately; a busy
    /// original worker retains its FD until the post-IO closed check drains.
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(state) = self.run.try_lock() {
            if let Some(run) = state.as_ref() {
                run.source.shared.fence();
            }
        }
    }
    pub fn snapshot(&self) -> Result<Option<rustsimhub_nkn_agent::Snapshot>> {
        Ok(self
            .run
            .try_lock()
            .map_err(|_| PortError::Unavailable)?
            .as_ref()
            .map(|r| r.supervisor.snapshot()))
    }
    /// Call on the Core's owned blocking worker. The explicit recent Owner
    /// request creates one scope; another feature only adds its own reference
    /// and cannot refresh the existing proof or stop another owner's SDK.
    pub fn enable(
        &self,
        token: &str,
        owner: rustsimhub_nkn_agent::Owner,
    ) -> Result<rustsimhub_nkn_agent::Snapshot> {
        if self.closed.load(Ordering::Acquire) {
            return Err(PortError::Denied);
        }
        self.auth
            .try_lock()
            .map_err(|_| PortError::Unavailable)?
            .nkn_root_authority(token)
            .map_err(|_| PortError::Denied)?
            .invalidate();
        let mut state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        if state.as_ref().is_some_and(|r| {
            r.supervisor.snapshot().phase == rustsimhub_nkn_agent::Phase::Disabled
                && r.supervisor.snapshot().references == 0
        }) {
            *state = None;
        }
        if state.is_none() {
            let (root, engine, policy, proof) = authorized_ports(self.auth.clone(), token)?;
            proof
                .0
                .host_closed
                .set(self.closed.clone())
                .map_err(|_| PortError::Denied)?;
            if self.closed.load(Ordering::Acquire) {
                proof.invalidate();
                return Err(PortError::Denied);
            }
            let source = Arc::new(proof.source());
            let mut supervisor = rustsimhub_nkn_agent::Supervisor::new(
                root,
                engine,
                rustsimhub_nkn_agent::SystemClock,
            )
            .map_err(|_| PortError::Unavailable)?;
            let reference = supervisor.acquire(owner).map_err(|_| PortError::Denied)?;
            let cancel = supervisor.arm().map_err(|_| PortError::Denied)?;
            let mut references = [None, None, None];
            references[owner_index(owner)] = Some(reference);
            let remote = (owner == rustsimhub_nkn_agent::Owner::Remote)
                .then(|| Arc::new(AtomicBool::new(true)));
            *state = Some(Run {
                supervisor,
                source,
                references,
                remote,
                #[cfg(feature = "usage-root-transport-candidate")]
                statistics_epoch: u64::from(owner == rustsimhub_nkn_agent::Owner::Statistics),
            });
            return state
                .as_mut()
                .ok_or(PortError::Unavailable)?
                .supervisor
                .begin(cancel, policy, proof)
                .map_err(|_| PortError::Protocol);
        }
        let run = state.as_mut().ok_or(PortError::Unavailable)?;
        run.source.shared.authority()?; // adding a reference never replaces the original proof
        if !matches!(
            run.supervisor.snapshot().phase,
            rustsimhub_nkn_agent::Phase::Starting
                | rustsimhub_nkn_agent::Phase::Ready
                | rustsimhub_nkn_agent::Phase::WaitingRetry
        ) {
            return Err(PortError::Denied);
        }
        let reference = run
            .supervisor
            .acquire(owner)
            .map_err(|_| PortError::Denied)?;
        #[cfg(feature = "usage-root-transport-candidate")]
        if owner == rustsimhub_nkn_agent::Owner::Statistics {
            let Some(next) = run.statistics_epoch.checked_add(1) else {
                let _ = run.supervisor.release(reference);
                return Err(PortError::Denied);
            };
            run.statistics_epoch = next;
        }
        run.references[owner_index(owner)] = Some(reference);
        if owner == rustsimhub_nkn_agent::Owner::Remote {
            run.remote = Some(Arc::new(AtomicBool::new(true)));
        }
        Ok(run.supervisor.snapshot())
    }
    pub fn disable(
        &self,
        token: &str,
        owner: rustsimhub_nkn_agent::Owner,
    ) -> Result<rustsimhub_nkn_agent::Snapshot> {
        self.auth
            .try_lock()
            .map_err(|_| PortError::Unavailable)?
            .recent_owner(token)
            .map_err(|_| PortError::Denied)?;
        let mut state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        let run = state.as_mut().ok_or(PortError::Denied)?;
        let reference = run.references[owner_index(owner)]
            .take()
            .ok_or(PortError::Denied)?;
        #[cfg(feature = "usage-root-transport-candidate")]
        if owner == rustsimhub_nkn_agent::Owner::Statistics {
            run.statistics_epoch = run.statistics_epoch.saturating_add(1);
        }
        if owner == rustsimhub_nkn_agent::Owner::Remote {
            if let Some(remote) = run.remote.take() {
                remote.store(false, Ordering::Release);
            }
        }
        run.supervisor
            .release(reference)
            .map_err(|_| PortError::Protocol)
    }
    /// Driven only while an explicit owned run exists. It can reconcile/clean
    /// its original resources but never creates a fresh authority or scope.
    pub fn drive(&self) -> Result<Option<rustsimhub_nkn_agent::Snapshot>> {
        let mut state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        state
            .as_mut()
            .map(|r| r.supervisor.drive().map_err(|_| PortError::Protocol))
            .transpose()
    }
    pub fn source(&self) -> Result<Arc<Source>> {
        let state = self.run.try_lock().map_err(|_| PortError::Unavailable)?;
        let run = state
            .as_ref()
            .filter(|r| r.supervisor.snapshot().phase == rustsimhub_nkn_agent::Phase::Ready)
            .ok_or(PortError::Unavailable)?;
        let reference = run
            .remote
            .as_ref()
            .filter(|r| r.load(Ordering::Acquire))
            .ok_or(PortError::Denied)?
            .clone();
        Ok(Arc::new(Source {
            shared: run.source.shared.clone(),
            reference: Some(reference),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply() -> Vec<u8> {
        let mut b = vec![0; 87];
        b[..5].copy_from_slice(b"RSHS\x01");
        b[5..9].copy_from_slice(&7u32.to_be_bytes());
        b[9..17].copy_from_slice(&99u64.to_be_bytes());
        b[17..49].fill(1);
        b[49..81].fill(2);
        b[81..85].copy_from_slice(&123u32.to_be_bytes());
        b[85] = 1;
        b[86] = 2;
        b
    }
    #[test]
    fn root_reply_binds_exact_generation_nonce_policy_expiry_and_observed_child() {
        let raw = reply();
        assert_eq!(
            parse_reply(&raw, 7, 99, &[1; 32], &[2; 32]).unwrap().pid,
            123
        );
        assert_eq!(
            parse_reply(&raw, 0, 99, &[1; 32], &[2; 32])
                .unwrap()
                .generation,
            7
        );
        for at in [0, 4, 5, 9, 17, 49] {
            let mut other = raw.clone();
            other[at] ^= 1;
            assert!(parse_reply(&other, 7, 99, &[1; 32], &[2; 32]).is_err());
        }
        let mut released = raw.clone();
        released[85] = 2;
        assert!(parse_reply(&released, 7, 99, &[1; 32], &[2; 32]).is_err());
        let mut no_pid = raw.clone();
        no_pid[81..85].fill(0);
        assert!(parse_reply(&no_pid, 7, 99, &[1; 32], &[2; 32]).is_err());
        assert!(parse_reply(&raw[..86], 7, 99, &[1; 32], &[2; 32]).is_err());
    }
    #[test]
    fn root_clock_is_real_boottime_and_never_a_client_timestamp() {
        let before = boot_ms().unwrap();
        let after = boot_ms().unwrap();
        assert!(before > 0 && after >= before);
    }
    #[test]
    #[ignore = "explicit private cgroup/network namespace fixture only"]
    fn root_ports_kernel_uid() {
        use ed25519_dalek::{Signer, SigningKey};
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(rustix::process::geteuid().as_raw(), 1001);
        let storage = tempfile::tempdir().unwrap();
        let identity = tempfile::tempdir().unwrap();
        for dir in [storage.path(), identity.path()] {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = rustsimhub_storage::Store::open(storage.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let signing = SigningKey::from_bytes(&[71; 32]);
        let challenge = auth
            .bootstrap_challenge(signing.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(
            &challenge.id,
            &signing.sign(&challenge.transcript).to_bytes(),
        )
        .unwrap();
        let challenge = auth
            .login_challenge(signing.verifying_key().to_bytes())
            .unwrap();
        let owner = auth
            .login_finish(
                &challenge.id,
                &signing.sign(&challenge.transcript).to_bytes(),
            )
            .unwrap();
        let pin = auth
            .public_key()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let report =
            std::env::var("RSH_ROOT_PIN_REPORT").expect("explicit kernel fixture required");
        let mut public = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(report)
            .unwrap();
        public.write_all(pin.as_bytes()).unwrap();
        public.sync_all().unwrap();
        let controller = Controller::new(Arc::new(Mutex::new(auth)));
        assert!(controller.snapshot().unwrap().is_none());
        assert!(controller.drive().unwrap().is_none());
        assert!(controller.source().is_err());
        assert!(!std::path::Path::new(
            "/sys/fs/cgroup/system.slice/rustsimhub-nkn-root.service/engine"
        )
        .exists());
        use rustsimhub_nkn_agent::{Owner, Phase};
        assert!(controller.enable("invalid", Owner::Remote).is_err());
        let root_deadline = Instant::now() + Duration::from_secs(10);
        while !std::path::Path::new(SOCKET).exists() {
            assert!(Instant::now() < root_deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        controller
            .enable(owner.expose(), Owner::ManualEngine)
            .unwrap();
        controller
            .enable(owner.expose(), Owner::Statistics)
            .unwrap();
        assert!(controller.source().is_err());
        controller.enable(owner.expose(), Owner::Remote).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while controller.drive().unwrap().unwrap().phase != Phase::Ready {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(50));
        }
        let report = controller.read_status(owner.expose()).unwrap();
        assert_eq!(report.status.source, "engine");
        assert_eq!(report.status.generation, 2);
        assert_eq!(report.status.sequence, 1);
        assert_eq!(report.status.enabled, "on");
        assert_eq!(report.status.service, "running");
        assert_eq!(report.status.dht, "unknown");
        assert_eq!(report.status.path, "unknown");
        assert!(report.status.age_ms.is_some_and(|n| n <= 1000));
        assert!(report.status.rx_bytes.is_none());
        assert!(report.owners.manual && report.owners.remote && report.owners.statistics);
        // The ephemeral observer closed after the read. The original owner FD,
        // shared SDK and all three references must still be live.
        let repeated = controller.read_status(owner.expose()).unwrap();
        assert_eq!(repeated.status.source, "unavailable");
        assert_eq!(repeated.status.last_error.as_deref(), Some("stale"));
        assert_eq!(controller.drive().unwrap().unwrap().phase, Phase::Ready);
        let source = controller.source().unwrap();
        let pid = source.shared.observed_pid.load(Ordering::Acquire);
        assert!(pid > 1);
        controller
            .disable(owner.expose(), Owner::Statistics)
            .unwrap();
        assert_eq!(controller.drive().unwrap().unwrap().references, 2);
        controller.disable(owner.expose(), Owner::Remote).unwrap();
        assert!(!source.reference.as_ref().unwrap().load(Ordering::Acquire));
        assert!(controller.source().is_err());
        assert_eq!(controller.drive().unwrap().unwrap().phase, Phase::Ready);
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists());
        controller.enable(owner.expose(), Owner::Remote).unwrap();
        let fresh = controller.source().unwrap();
        assert!(!Arc::ptr_eq(
            source.reference.as_ref().unwrap(),
            fresh.reference.as_ref().unwrap()
        ));
        assert!(!source.reference.as_ref().unwrap().load(Ordering::Acquire));
        controller
            .disable(owner.expose(), Owner::ManualEngine)
            .unwrap();
        assert_eq!(controller.drive().unwrap().unwrap().references, 1);
        let stopped = Instant::now() + Duration::from_secs(5);
        controller.disable(owner.expose(), Owner::Remote).unwrap();
        while controller.drive().unwrap().unwrap().phase != Phase::Disabled {
            assert!(Instant::now() < stopped);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(!std::path::Path::new(
            "/sys/fs/cgroup/system.slice/rustsimhub-nkn-root.service/engine"
        )
        .exists());
        assert!(!fresh.shared.local_live());
    }
}
