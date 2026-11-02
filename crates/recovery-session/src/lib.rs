// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Candidate standard Noise + Ed25519 application binding. A bound cipher is
//! never an Owner/Recovery capability: actual Core authorization is still needed
//! for every request/reply. No listeners, identity storage, default verifier or
//! production activation. Browser private signing keys never enter this crate.
mod context;
mod record;
#[cfg(target_arch = "wasm32")]
mod wasm;
pub use context::{ClientHello, Context, CoreBinding, Purpose, Role};
pub use record::{Action, Request};
use snow::{Builder, HandshakeState, TransportState};
use std::collections::BTreeSet;
use zeroize::Zeroizing;

pub const SUITE: &str = "Noise_XX_25519_AESGCM_SHA256";
pub const MAX_WIRE: usize = 2048;
pub const MAX_BODY: usize = 1024;
pub const MAX_REQUESTS: usize = 128;
const MAX_RECORDS: u64 = MAX_REQUESTS as u64 + 2;
const HANDSHAKE_MS: u64 = 10_000;
const SESSION_MS: u64 = 300_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Context,
    Entropy,
    State,
    Crypto,
    Signature,
    Record,
    Limit,
    Expired,
    Clock,
    Replay,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "secure session: {self:?}")
    }
}
impl std::error::Error for Error {}
enum Cipher {
    Handshake(Box<HandshakeState>),
    Transport(TransportState),
    Closed,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Handshake,
    DeviceBinding,
    ClientBinding,
    BindingAck,
    Bound,
    Closed,
}
/// Owns one cipher per session. Close drops cipher state; snow's internal memory
/// wiping is not promised. Application plaintext temporaries use Zeroizing.
pub struct Session {
    role: Role,
    context: Context,
    binding: Option<CoreBinding>,
    cipher: Cipher,
    phase: Phase,
    hash: [u8; 32],
    tx: u64,
    rx: u64,
    pending: Option<[u8; 16]>,
    requests: BTreeSet<[u8; 16]>,
    last_now: u64,
    handshake_deadline: u64,
    deadline: u64,
}
impl Session {
    pub fn client(context: Context, now_ms: u64, valid_until_ms: u64) -> Result<Self> {
        Self::new(Role::Client, context, None, now_ms, valid_until_ms)
    }
    pub fn device(
        context: Context,
        binding: CoreBinding,
        now_ms: u64,
        valid_until_ms: u64,
    ) -> Result<Self> {
        Self::new(Role::Device, context, Some(binding), now_ms, valid_until_ms)
    }
    fn new(
        role: Role,
        context: Context,
        binding: Option<CoreBinding>,
        now_ms: u64,
        valid_until_ms: u64,
    ) -> Result<Self> {
        let deadline = valid_until_ms.min(now_ms.checked_add(SESSION_MS).ok_or(Error::Limit)?);
        if deadline <= now_ms {
            return Err(Error::Expired);
        }
        let handshake_deadline =
            deadline.min(now_ms.checked_add(HANDSHAKE_MS).ok_or(Error::Limit)?);
        let builder = Builder::new(SUITE.parse().map_err(|_| Error::Crypto)?);
        let key = builder.generate_keypair().map_err(|_| Error::Entropy)?;
        let private = Zeroizing::new(key.private);
        let prologue = context.prologue();
        let builder = builder
            .local_private_key(&private)
            .map_err(|_| Error::Crypto)?
            .prologue(&prologue)
            .map_err(|_| Error::Crypto)?;
        let handshake = if role == Role::Client {
            builder.build_initiator()
        } else {
            builder.build_responder()
        }
        .map_err(|_| Error::Crypto)?;
        Ok(Self {
            role,
            context,
            binding,
            cipher: Cipher::Handshake(Box::new(handshake)),
            phase: Phase::Handshake,
            hash: [0; 32],
            tx: 0,
            rx: 0,
            pending: None,
            requests: BTreeSet::new(),
            last_now: now_ms,
            handshake_deadline,
            deadline,
        })
    }
    fn check_time(&mut self, now: u64) -> Result<()> {
        if self.phase == Phase::Closed {
            return Err(Error::State);
        }
        if now < self.last_now {
            return Err(Error::Clock);
        }
        self.last_now = now;
        let deadline = if self.phase == Phase::Bound {
            self.deadline
        } else {
            self.handshake_deadline
        };
        if now >= deadline {
            return Err(Error::Expired);
        }
        Ok(())
    }
    fn finish_noise(&mut self) -> Result<()> {
        let Cipher::Handshake(state) = &self.cipher else {
            return Err(Error::State);
        };
        if !state.is_handshake_finished() {
            return Ok(());
        }
        self.hash.copy_from_slice(state.get_handshake_hash());
        let Cipher::Handshake(state) = std::mem::replace(&mut self.cipher, Cipher::Closed) else {
            return Err(Error::State);
        };
        self.cipher = Cipher::Transport(state.into_transport_mode().map_err(|_| Error::Crypto)?);
        self.phase = Phase::DeviceBinding;
        Ok(())
    }
    pub fn write_handshake(&mut self, now_ms: u64) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.phase != Phase::Handshake {
                return Err(Error::State);
            }
            let Cipher::Handshake(state) = &mut self.cipher else {
                return Err(Error::State);
            };
            let mut wire = vec![0; MAX_WIRE];
            let n = state
                .write_message(&[], &mut wire)
                .map_err(|_| Error::Crypto)?;
            wire.truncate(n);
            self.finish_noise()?;
            Ok(wire)
        })();
        self.guard(result)
    }
    pub fn write_client_hello(&mut self, now_ms: u64) -> Result<Vec<u8>> {
        if self.role != Role::Client {
            return self.guard(Err(Error::State));
        }
        let noise = self.write_handshake(now_ms)?;
        if noise.len() != 32 {
            return self.guard(Err(Error::State));
        }
        let mut wire = vec![1, self.context.purpose as u8];
        wire.extend_from_slice(&self.context.client);
        wire.extend_from_slice(&self.context.generation.to_be_bytes());
        wire.extend_from_slice(&self.context.nonce);
        wire.extend_from_slice(&noise);
        Ok(wire)
    }
    pub fn read_handshake(&mut self, wire: &[u8], now_ms: u64) -> Result<()> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.phase != Phase::Handshake {
                return Err(Error::State);
            }
            if wire.is_empty() || wire.len() > MAX_WIRE {
                return Err(Error::Limit);
            }
            let Cipher::Handshake(state) = &mut self.cipher else {
                return Err(Error::State);
            };
            let mut plain = Zeroizing::new(vec![0; MAX_WIRE]);
            if state
                .read_message(wire, &mut plain)
                .map_err(|_| Error::Crypto)?
                != 0
            {
                return Err(Error::Record);
            }
            self.finish_noise()
        })();
        self.guard(result)
    }
    pub fn binding_message(&mut self, now_ms: u64) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if !matches!(
                (self.role, self.phase),
                (Role::Device, Phase::DeviceBinding) | (Role::Client, Phase::ClientBinding)
            ) {
                return Err(Error::State);
            }
            Ok(self.context.signed(
                self.role,
                self.binding.as_ref().ok_or(Error::Context)?,
                &self.hash,
            ))
        })();
        self.guard(result)
    }
    /// Public canonical transcript of the actually verified peer, available
    /// only after the encrypted binding ACK. Used by the separate NKN root-key
    /// membership proof; never accepts a caller-supplied transcript or hash.
    pub fn verified_peer_binding_message(&mut self, now_ms: u64) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.phase != Phase::Bound {
                return Err(Error::State);
            }
            let peer = if self.role == Role::Client {
                Role::Device
            } else {
                Role::Client
            };
            Ok(self.context.signed(
                peer,
                self.binding.as_ref().ok_or(Error::Context)?,
                &self.hash,
            ))
        })();
        self.guard(result)
    }
    pub fn seal_binding(&mut self, signature: &[u8], now_ms: u64) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if !matches!(
                (self.role, self.phase),
                (Role::Device, Phase::DeviceBinding) | (Role::Client, Phase::ClientBinding)
            ) {
                return Err(Error::State);
            }
            let binding = self.binding.as_ref().ok_or(Error::Context)?;
            self.context
                .verify(self.role, binding, &self.hash, signature)?;
            let mut body = Vec::new();
            let kind = if self.role == Role::Device {
                body.extend_from_slice(&binding.bytes());
                1
            } else {
                2
            };
            body.extend_from_slice(signature);
            let wire = self.write_record(kind, &body)?;
            self.phase = if self.role == Role::Device {
                Phase::ClientBinding
            } else {
                Phase::BindingAck
            };
            Ok(wire)
        })();
        self.guard(result)
    }
    pub fn open_binding(&mut self, wire: &[u8], now_ms: u64) -> Result<()> {
        let result = (|| {
            self.check_time(now_ms)?;
            let (kind, role) = match (self.role, self.phase) {
                (Role::Client, Phase::DeviceBinding) => (1, Role::Device),
                (Role::Device, Phase::ClientBinding) => (2, Role::Client),
                _ => return Err(Error::State),
            };
            let body = self.read_record(kind, wire)?;
            if role == Role::Device {
                if body.len() != 160 {
                    return Err(Error::Record);
                }
                let binding = CoreBinding::from_bytes(&body[..96])?;
                self.context
                    .verify(role, &binding, &self.hash, &body[96..])?;
                self.binding = Some(binding);
                self.phase = Phase::ClientBinding;
            } else {
                if body.len() != 64 {
                    return Err(Error::Record);
                }
                self.context.verify(
                    role,
                    self.binding.as_ref().ok_or(Error::Context)?,
                    &self.hash,
                    &body,
                )?;
                self.phase = Phase::BindingAck;
            }
            Ok(())
        })();
        self.guard(result)
    }
    /// Cryptographic binding only. This method never asserts a Core grant/scope.
    pub fn is_bound(&self) -> bool {
        self.phase == Phase::Bound
    }
    pub fn seal_binding_ack(&mut self, now_ms: u64) -> Result<Vec<u8>> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.role != Role::Device || self.phase != Phase::BindingAck {
                return Err(Error::State);
            }
            let wire = self.write_record(5, &[])?;
            self.phase = Phase::Bound;
            Ok(wire)
        })();
        self.guard(result)
    }
    pub fn open_binding_ack(&mut self, wire: &[u8], now_ms: u64) -> Result<()> {
        let result = (|| {
            self.check_time(now_ms)?;
            if self.role != Role::Client || self.phase != Phase::BindingAck {
                return Err(Error::State);
            }
            if !self.read_record(5, wire)?.is_empty() {
                return Err(Error::Record);
            }
            self.phase = Phase::Bound;
            Ok(())
        })();
        self.guard(result)
    }
    pub fn is_closed(&self) -> bool {
        self.phase == Phase::Closed
    }
    pub fn session_id(&self) -> Result<&[u8; 32]> {
        if self.phase == Phase::Bound {
            Ok(&self.hash)
        } else {
            Err(Error::State)
        }
    }
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn local_role(&self) -> Role {
        self.role
    }
    pub fn core_binding(&self) -> Option<&CoreBinding> {
        self.binding.as_ref()
    }
    pub fn valid_until_ms(&self) -> u64 {
        self.deadline
    }
    pub fn close(&mut self) {
        self.cipher = Cipher::Closed;
        self.phase = Phase::Closed;
        self.binding = None;
        self.pending = None;
        self.requests.clear();
        self.hash = [0; 32];
    }
    pub(crate) fn guard<T>(&mut self, result: Result<T>) -> Result<T> {
        if result.is_err() {
            self.close();
        }
        result
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests;
