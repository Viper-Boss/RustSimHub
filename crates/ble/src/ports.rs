// SPDX-License-Identifier: AGPL-3.0-only
use crate::{
    framing::{InputKind, OpaqueBytes},
    manifest::{Characteristic, FixedAdvertisement, FixedManifest},
    Error,
};
use std::{fmt, future::Future, pin::Pin};

pub const DBUS_CALL_MS: u64 = 5000;
pub type Pending<T> = Pin<Box<dyn Future<Output = Result<T, Error>> + Send + 'static>>;

/// Allocated by the owning process, never by an inbound message. The main
/// adapter must supply a fresh random boot domain and monotonically increasing
/// nonzero starts. This crate cannot prove uniqueness of caller-supplied domains.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Epoch {
    domain: [u8; 16],
    start: u64,
}

impl Epoch {
    pub fn new(domain: [u8; 16], start: u64) -> Result<Self, Error> {
        if domain == [0; 16] || start == 0 {
            return Err(Error::InvalidInput);
        }
        Ok(Self { domain, start })
    }
}

impl fmt::Debug for Epoch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Epoch(..)")
    }
}

/// Identity captured by the trusted D-Bus integration from an actual BlueZ
/// callback and its unique owner, never from AuthExchange/Command payloads.
/// Construction validates shape only and is not an authorization proof.
#[derive(Clone, PartialEq, Eq)]
pub struct Peer {
    owner: String,
    device: String,
}

impl Peer {
    pub fn device(&self) -> &str {
        &self.device
    }
    pub fn from_backend_callback(owner: &str, device: &str) -> Result<Self, Error> {
        let Some(unique) = owner.strip_prefix(':') else {
            return Err(Error::InvalidInput);
        };
        if owner.len() > 255
            || !unique.contains('.')
            || unique.split('.').any(|part| {
                part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            || device.len() > 255
            || !device.starts_with("/org/bluez/")
            || device.split('/').skip(1).any(|part| {
                part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            owner: owner.to_owned(),
            device: device.to_owned(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn device_path(&self) -> &str {
        &self.device
    }
}

impl fmt::Debug for Peer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Peer(..)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Disabled,
    Registering,
    Advertising,
    Connected,
    Stopping,
    Unknown,
}

/// A result reported by the *trusted injected* security backend. These variants
/// do not verify ciphertext and must not be constructed from a wire tag/bool.
/// Application results are encrypted and bound to epoch/current peer by that
/// backend; lifecycle code must check the callback context again after await.
#[derive(Debug)]
pub enum SecureReply {
    Handshake(OpaqueBytes),
    Application(OpaqueBytes),
    NoReply,
}

/// All futures own their request and remain pollable after a timeout. The owning
/// lifecycle driver must retain late registrations until compensated, use the
/// captured unique owner, and never clean up another epoch's object subtree.
pub trait GattBackendV1 {
    fn register_application(&mut self, epoch: Epoch, manifest: FixedManifest) -> Pending<()>;
    fn register_advertisement(
        &mut self,
        epoch: Epoch,
        advertisement: FixedAdvertisement,
    ) -> Pending<()>;
    fn unregister_advertisement(&mut self, epoch: Epoch) -> Pending<()>;
    fn unregister_application(&mut self, epoch: Epoch) -> Pending<()>;
    fn notify(
        &mut self,
        epoch: Epoch,
        peer: Peer,
        characteristic: Characteristic,
        frame: OpaqueBytes,
    ) -> Pending<()>;
}

/// No default or mock implementation is supplied in this production module.
/// The approved cryptographic session and Core recovery authority are separate
/// prerequisites. A None port must be rejected before beginning registration.
pub trait SecurePortV1 {
    fn receive(
        &mut self,
        epoch: Epoch,
        peer: Peer,
        kind: InputKind,
        bytes: OpaqueBytes,
    ) -> Pending<SecureReply>;

    fn disconnected(&mut self, epoch: Epoch, peer: Peer) -> Pending<()>;
}
