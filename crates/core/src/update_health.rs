// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit private Root-only source. The response is not a confirmation token.
use rustsimhub_auth::Auth;
use rustsimhub_wire::update_health as wire;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::UnixStream,
    },
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
mod io;
#[cfg(test)]
mod tests;
pub const SOCKET: &str = wire::SOCKET;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Denied,
    Busy,
    Protocol,
    Closed,
    Deadline,
    Io,
    Source,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "update health: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
/// Live controller state, constructed inside the Core's existing control lock.
#[derive(Clone, Copy)]
pub struct ForwarderLease {
    pub queue_owned: bool,
    pub admitted_generation: Option<u64>,
    pub admitted_until: Option<Instant>,
}
impl ForwarderLease {
    fn valid(&self, policy: &rustsimhub_storage::update_health::ForwardingState) -> bool {
        if policy.enabled {
            self.admitted_generation == Some(policy.generation)
                && self.admitted_until.is_some_and(|t| t > Instant::now())
        } else {
            self.admitted_generation.is_none() && self.admitted_until.is_none()
        }
    }
}
/// Adapter holds the SAME forwarding mutex through work. It does not fabricate
/// a producer: Auth, identity, SQL and the single writer lease stay in this module.
pub trait RuntimeSources: Send + Sync {
    fn business_ready(&self) -> bool;
    fn with_forwarder(
        &self,
        auth: &Auth,
        work: &mut dyn FnMut(ForwarderLease) -> Result<()>,
    ) -> Result<()>;
}
#[derive(PartialEq, Eq)]
struct Peer {
    pid: u32,
    uid: u32,
    start: u64,
    comm: String,
    boot: String,
}
impl Peer {
    fn read(stream: &UnixStream, uid: u32) -> Result<Self> {
        let c = rustix::net::sockopt::socket_peercred(stream).map_err(|_| Error::Denied)?;
        if c.uid.as_raw() != uid {
            return Err(Error::Denied);
        }
        let pid = c.pid.as_raw_pid() as u32;
        let mut raw = Vec::new();
        fs::File::open(format!("/proc/{pid}/stat"))
            .map_err(|_| Error::Denied)?
            .take(8193)
            .read_to_end(&mut raw)
            .map_err(|_| Error::Io)?;
        if raw.len() > 8192 {
            return Err(Error::Denied);
        }
        let text = std::str::from_utf8(&raw).map_err(|_| Error::Denied)?;
        let start = text
            .rsplit_once(')')
            .ok_or(Error::Denied)?
            .1
            .split_whitespace()
            .nth(19)
            .ok_or(Error::Denied)?
            .parse()
            .map_err(|_| Error::Denied)?;
        // Core cannot ptrace/dereference a Root peer's exe. Use kernel peer
        // credentials, current UIDs and start ticks; Root checks Core's exe on
        // its privileged side. A normal exec closes this CLOEXEC connection.
        let mut status_bytes = Vec::new();
        fs::File::open(format!("/proc/{pid}/status"))
            .map_err(|_| Error::Denied)?
            .take(65537)
            .read_to_end(&mut status_bytes)
            .map_err(|_| Error::Denied)?;
        if status_bytes.len() > 65536 {
            return Err(Error::Denied);
        }
        let status = std::str::from_utf8(&status_bytes).map_err(|_| Error::Denied)?;
        let uids: Vec<u32> = status
            .lines()
            .find_map(|l| l.strip_prefix("Uid:"))
            .ok_or(Error::Denied)?
            .split_whitespace()
            .map(|s| s.parse().map_err(|_| Error::Denied))
            .collect::<Result<_>>()?;
        if uids.len() != 4 || uids.iter().any(|u| *u != uid) {
            return Err(Error::Denied);
        }
        let comm = text
            .split_once('(')
            .and_then(|(_, s)| s.rsplit_once(')').map(|(comm, _)| comm.to_owned()))
            .ok_or(Error::Denied)?;
        let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| Error::Io)?;
        Ok(Self {
            pid,
            uid,
            start,
            comm,
            boot,
        })
    }
}
fn nonce() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| Error::Source)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
/// Handles one accepted kernel peer; no input may choose a path, PID or clock.
/// The production caller must retain the endpoint until this bounded work ends.
fn connection(
    mut stream: UnixStream,
    auth: Arc<Mutex<Auth>>,
    sources: Arc<dyn RuntimeSources>,
    root_uid: u32,
) -> Result<()> {
    let until = Instant::now() + Duration::from_millis(wire::MAX_OBSERVATION_MS);
    let peer = Peer::read(&stream, root_uid)?;
    let request =
        wire::parse_request(&io::read(&mut stream, until, false)?).map_err(|_| Error::Protocol)?;
    if request.step != wire::Step::Observe {
        return Err(Error::Protocol);
    }
    let a = auth.try_lock().map_err(|_| Error::Busy)?;
    if !sources.business_ready() {
        return Err(Error::Source);
    }
    let connection = nonce()?;
    sources.with_forwarder(&a, &mut |forwarder| {
        let lease = a
            .update_health_lease(until, forwarder.queue_owned)
            .map_err(|_| Error::Source)?;
        let data = lease.snapshot();
        let owner_binding = serde_json::to_vec(&(
            data.device_key,
            data.owner.public_key,
            data.owner.generation,
            &data.realm,
        ))
        .map_err(|_| Error::Source)?;
        let snapshot = wire::Snapshot {
            protocol: wire::VERSION,
            challenge: request.challenge.clone(),
            connection: connection.clone(),
            core_instance: lease.instance().into(),
            source_sha256: rustsimhub_wire::build_source_sha256().map(str::to_owned),
            business_ready: true,
            data_schema: data.schema,
            data_realm: data.realm.clone(),
            line_generation: data.line,
            data_device: data.device,
            data_inode: data.inode,
            authorization_generation: data.owner.generation,
            authorization_binding: Sha256::digest(owner_binding)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            forwarding: wire::Forwarding {
                policy: if data.forwarding.enabled {
                    wire::Policy::On
                } else {
                    wire::Policy::Off
                },
                generation: data.forwarding.generation,
                outbox_schema: data.forwarding.schema,
                worker_admitted: forwarder.admitted_generation.is_some(),
            },
        };
        if !snapshot.valid() || !forwarder.valid(&data.forwarding) {
            return Err(Error::Source);
        }
        let bytes = serde_json::to_vec(&snapshot).map_err(|_| Error::Protocol)?;
        let check = |stream: &UnixStream| -> Result<()> {
            lease.revalidate().map_err(|_| Error::Source)?;
            if Peer::read(stream, root_uid)? != peer
                || !sources.business_ready()
                || !forwarder.valid(&lease.snapshot().forwarding)
            {
                return Err(Error::Source);
            }
            Ok(())
        };
        check(&stream)?;
        io::write(&mut stream, &bytes, until)?;
        // At most one recheck. A challenge cannot be replayed to renew the lease.
        for step in [wire::Step::Recheck, wire::Step::Finish] {
            let next = wire::parse_request(&io::read(&mut stream, until, true)?)
                .map_err(|_| Error::Protocol)?;
            if next.step != step
                || next.challenge != request.challenge
                || next.connection.as_deref() != Some(&connection)
            {
                return Err(Error::Denied);
            }
            check(&stream)?;
            if step == wire::Step::Recheck {
                io::write(&mut stream, &bytes, until)?;
            }
        }
        lease.release().map_err(|_| Error::Source)?;
        io::write(&mut stream, b"{\"protocol\":1,\"released\":true}", until)
    })
}
struct PathGuard {
    path: std::path::PathBuf,
    dev: u64,
    ino: u64,
}
impl Drop for PathGuard {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path)
            .is_ok_and(|m| m.file_type().is_socket() && m.dev() == self.dev && m.ino() == self.ino)
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
/// Root tmpfiles creates the Core-owned 0700 directory. Core never chmods or
/// adopts an unsafe directory, replaces a live socket, or opens a second DB.
pub async fn serve(auth: Arc<Mutex<Auth>>, sources: Arc<dyn RuntimeSources>) -> Result<()> {
    if rustix::process::geteuid().as_raw() != 986 {
        return Err(Error::Denied);
    }
    for p in ["/run", "/run/rustsimhub"] {
        let m = fs::symlink_metadata(p).map_err(|_| Error::Denied)?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(Error::Denied);
        }
    }
    let dir = fs::symlink_metadata(Path::new(SOCKET).parent().ok_or(Error::Denied)?)
        .map_err(|_| Error::Denied)?;
    if !dir.is_dir() || dir.uid() != 986 || dir.mode() & 0o7777 != 0o700 {
        return Err(Error::Denied);
    }
    // The lifetime lock refuses a second process before stale-socket handling.
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open("/run/rustsimhub/update-health/.core.sock.lock")
        .map_err(|_| Error::Io)?;
    let m = lock.metadata().map_err(|_| Error::Io)?;
    if !m.is_file() || m.uid() != 986 || m.mode() & 0o7777 != 0o600 || m.nlink() != 1 {
        return Err(Error::Denied);
    }
    lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
    if let Ok(old) = fs::symlink_metadata(SOCKET) {
        if !old.file_type().is_socket() || old.uid() != 986 || old.mode() & 0o7777 != 0o600 {
            return Err(Error::Denied);
        }
        use rustix::net::{
            connect, socket_with, AddressFamily, SocketAddrUnix, SocketFlags, SocketType,
        };
        let probe = socket_with(
            AddressFamily::UNIX,
            SocketType::STREAM,
            SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
            None,
        )
        .map_err(|_| Error::Io)?;
        let address = SocketAddrUnix::new(SOCKET).map_err(|_| Error::Io)?;
        if !matches!(connect(&probe,&address),Err(e) if e==rustix::io::Errno::CONNREFUSED) {
            return Err(Error::Busy);
        }
        let now = fs::symlink_metadata(SOCKET).map_err(|_| Error::Denied)?;
        if old.dev() != now.dev() || old.ino() != now.ino() {
            return Err(Error::Busy);
        }
        fs::remove_file(SOCKET).map_err(|_| Error::Io)?;
    }
    let listener = tokio::net::UnixListener::bind(SOCKET).map_err(|_| Error::Busy)?;
    fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o600)).map_err(|_| Error::Io)?;
    let m = fs::symlink_metadata(SOCKET).map_err(|_| Error::Io)?;
    let _path = PathGuard {
        path: SOCKET.into(),
        dev: m.dev(),
        ino: m.ino(),
    };
    let mut task: Option<tokio::task::JoinHandle<Result<()>>> = None;
    loop {
        tokio::select! {
            r=async {if let Some(t)=task.as_mut(){Some(t.await)}else{std::future::pending().await}}=>{
                if let Some(Ok(Err(error)))=r {
                    eprintln!("Core update health request ended: {error:?}");
                }
                task=None;
            },
            accepted=listener.accept()=>{
                let (stream,_)=accepted.map_err(|_|Error::Io)?;
                if task.is_some()||stream.peer_cred().map(|p|p.uid()).ok()!=Some(0){continue;}
                let stream=stream.into_std().map_err(|_|Error::Io)?;stream.set_nonblocking(false).map_err(|_|Error::Io)?;
                let a=auth.clone();let s=sources.clone();
                task=Some(tokio::task::spawn_blocking(move||connection(stream,a,s,0)));
            }
        }
    }
}
