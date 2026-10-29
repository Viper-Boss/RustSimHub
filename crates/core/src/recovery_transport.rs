// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded local transport, not proof of a recovery key or permission to manage Wi-Fi.
//! The caller must supply the approved verifier and same-Core authorization separately.
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::{timeout_at, Instant},
};
use zeroize::Zeroizing;

pub const MAX_ENVELOPE: usize = 2048;
pub const MAX_BODY: usize = MAX_ENVELOPE + 2;
const IO_BUDGET: Duration = Duration::from_secs(5);
const VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Denied,
    InvalidFrame,
    Closed,
    Deadline,
    Io,
    Entropy,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "recovery transport: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Handshake = 1,
    EncryptedRequest = 2,
    CloseTransport = 3,
}
/// Opaque application bytes. Never Debug/Serialize or ordinary AuthCommand passthrough.
pub struct Packet {
    kind: Kind,
    payload: Zeroizing<Vec<u8>>,
}
impl Packet {
    pub fn new(kind: Kind, payload: Vec<u8>) -> Result<Self> {
        let payload = Zeroizing::new(payload);
        if payload.len() > MAX_ENVELOPE || (kind == Kind::CloseTransport && !payload.is_empty()) {
            return Err(Error::InvalidFrame);
        }
        Ok(Self { kind, payload })
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Actual installation role assignments, not request fields. Construction grants no authority.
pub struct Roles {
    edge: u32,
    forwarder: u32,
    network: u32,
    ble: u32,
}
impl Roles {
    pub fn new(edge: u32, forwarder: u32, network: u32, ble: u32) -> Result<Self> {
        let roles = Self {
            edge,
            forwarder,
            network,
            ble,
        };
        roles.validate()?;
        Ok(roles)
    }
    fn validate(&self) -> Result<()> {
        let ids = [
            rustix::process::geteuid().as_raw(),
            self.edge,
            self.forwarder,
            self.network,
            self.ble,
        ];
        if ids.contains(&0)
            || ids.contains(&u32::MAX)
            || ids.into_iter().collect::<HashSet<_>>().len() != 5
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
/// Kernel transport provenance, not a verified recovery session or business capability.
/// Private fields and no Clone/Deserialize prevent construction from a request payload.
pub struct PeerContext {
    uid: u32,
    gid: u32,
    pid: i32,
    nonce: Zeroizing<[u8; 32]>,
}
impl PeerContext {
    pub fn uid(&self) -> u32 {
        self.uid
    }
    pub fn gid(&self) -> u32 {
        self.gid
    }
    pub fn pid(&self) -> i32 {
        self.pid
    }
    pub fn connection_nonce(&self) -> &[u8; 32] {
        &self.nonce
    }
}
/// Checks the kernel peer before any packet allocation; cannot mint Owner/grant authority.
pub struct SourceGuard {
    roles: Roles,
    active: Arc<AtomicBool>,
}
impl SourceGuard {
    pub fn new(roles: Roles) -> Result<Self> {
        roles.validate()?;
        Ok(Self {
            roles,
            active: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn accept(&self, stream: UnixStream) -> Result<PeerConnection> {
        self.roles.validate()?;
        let cred = stream.peer_cred().map_err(|_| Error::Denied)?;
        if cred.uid() != self.roles.ble {
            return Err(Error::Denied);
        }
        let pid = cred.pid().filter(|v| *v > 0).ok_or(Error::Denied)?;
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::Denied)?;
        let slot = Slot(self.active.clone());
        let mut nonce = Zeroizing::new([0; 32]);
        getrandom::fill(nonce.as_mut()).map_err(|_| Error::Entropy)?;
        if nonce.iter().all(|v| *v == 0) {
            return Err(Error::Entropy);
        }
        Ok(PeerConnection {
            context: PeerContext {
                uid: cred.uid(),
                gid: cred.gid(),
                pid,
                nonce,
            },
            io: FramedIo::new(stream),
            _slot: slot,
        })
    }
}

/// Framing alone: creating this type deliberately proves no UID/key/session authorization.
pub struct FramedIo {
    stream: UnixStream,
    poisoned: bool,
}
impl FramedIo {
    pub fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            poisoned: false,
        }
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub async fn read(&mut self) -> Result<Packet> {
        self.read_until(Instant::now() + IO_BUDGET).await
    }
    async fn read_until(&mut self, deadline: Instant) -> Result<Packet> {
        if self.poisoned {
            return Err(Error::Closed);
        }
        // Set before the first await: cancellation of a polled future keeps the stream fenced.
        self.poisoned = true;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        let result = timeout_at(deadline, async {
            let mut header = [0u8; 4];
            self.stream
                .read_exact(&mut header)
                .await
                .map_err(|_| Error::Io)?;
            let length = u32::from_be_bytes(header) as usize;
            if !(2..=MAX_BODY).contains(&length) {
                return Err(Error::InvalidFrame);
            }
            let mut prefix = [0u8; 2];
            self.stream
                .read_exact(&mut prefix)
                .await
                .map_err(|_| Error::Io)?;
            let kind = match (prefix[0], prefix[1]) {
                (VERSION, 1) => Kind::Handshake,
                (VERSION, 2) => Kind::EncryptedRequest,
                (VERSION, 3) => Kind::CloseTransport,
                _ => return Err(Error::InvalidFrame),
            };
            if kind == Kind::CloseTransport && length != 2 {
                return Err(Error::InvalidFrame);
            }
            // Validate the bounded length/version/kind before allocating opaque bytes.
            let mut payload = Zeroizing::new(vec![0; length - 2]);
            self.stream
                .read_exact(&mut payload)
                .await
                .map_err(|_| Error::Io)?;
            Ok(Packet { kind, payload })
        })
        .await
        .map_err(|_| Error::Deadline)?;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        if result
            .as_ref()
            .is_ok_and(|packet| packet.kind != Kind::CloseTransport)
        {
            self.poisoned = false;
        }
        result
    }
    pub async fn write(&mut self, packet: &Packet) -> Result<()> {
        self.write_until(packet, Instant::now() + IO_BUDGET).await
    }
    async fn write_until(&mut self, packet: &Packet, deadline: Instant) -> Result<()> {
        if self.poisoned {
            return Err(Error::Closed);
        }
        self.poisoned = true;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        let result = timeout_at(deadline, async {
            let header = ((packet.payload.len() + 2) as u32).to_be_bytes();
            self.stream
                .write_all(&header)
                .await
                .map_err(|_| Error::Io)?;
            self.stream
                .write_all(&[VERSION, packet.kind as u8])
                .await
                .map_err(|_| Error::Io)?;
            self.stream
                .write_all(&packet.payload)
                .await
                .map_err(|_| Error::Io)?;
            Ok(())
        })
        .await
        .map_err(|_| Error::Deadline)?;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        if result.is_ok() && packet.kind != Kind::CloseTransport {
            self.poisoned = false;
        }
        result
    }
}
/// Owns the exact accepted descriptor. No into_inner/Clone escape for reusing a poisoned stream.
pub struct PeerConnection {
    context: PeerContext,
    io: FramedIo,
    // Last field: close the owned stream before releasing its admission slot.
    _slot: Slot,
}
struct Slot(Arc<AtomicBool>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl PeerConnection {
    /// None after timeout/cancellation/error. Some is only provenance, never an authorization.
    pub fn context(&self) -> Option<&PeerContext> {
        (!self.io.poisoned).then_some(&self.context)
    }
    pub async fn read(&mut self) -> Result<Packet> {
        self.io.read().await
    }
    pub async fn write(&mut self, packet: &Packet) -> Result<()> {
        self.io.write(packet).await
    }
    pub fn is_poisoned(&self) -> bool {
        self.io.is_poisoned()
    }
}

#[cfg(test)]
mod tests;
