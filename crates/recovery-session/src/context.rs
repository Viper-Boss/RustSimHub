// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Error, Result};
use ed25519_dalek::{Signature, VerifyingKey};

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Purpose {
    BleRecovery = 1,
    NknControl = 2,
}
impl Purpose {
    pub(crate) fn domain(self) -> &'static [u8] {
        match self {
            Self::BleRecovery => b"RustSimHub/RecoveryBinding/v1\0",
            Self::NknControl => b"RustSimHub/NknBinding/v1\0",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Device = 1,
    Client = 2,
}
/// Public handshake context, not a grant, credential, or Auth capability.
/// The device host must obtain its expected peer/generation from real Auth.
#[derive(Clone, PartialEq, Eq)]
pub struct Context {
    pub(crate) purpose: Purpose,
    pub(crate) device: [u8; 32],
    pub(crate) client: [u8; 32],
    pub(crate) generation: u64,
    pub(crate) nonce: [u8; 32],
}
impl Context {
    pub fn new(
        purpose: Purpose,
        device: [u8; 32],
        client: [u8; 32],
        generation: u64,
        nonce: [u8; 32],
    ) -> Result<Self> {
        if generation == 0 || nonce == [0; 32] || device == client {
            return Err(Error::Context);
        }
        for key in [&device, &client] {
            let key = VerifyingKey::from_bytes(key).map_err(|_| Error::Context)?;
            if key.is_weak() {
                return Err(Error::Context);
            }
        }
        Ok(Self {
            purpose,
            device,
            client,
            generation,
            nonce,
        })
    }
    pub fn purpose(&self) -> Purpose {
        self.purpose
    }
    pub fn device_key(&self) -> &[u8; 32] {
        &self.device
    }
    pub fn client_key(&self) -> &[u8; 32] {
        &self.client
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn session_nonce(&self) -> &[u8; 32] {
        &self.nonce
    }
    pub(crate) fn prologue(&self) -> Vec<u8> {
        let mut bytes = self.purpose.domain().to_vec();
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.push(self.purpose as u8);
        bytes.extend_from_slice(&self.device);
        bytes.extend_from_slice(&self.client);
        bytes.extend_from_slice(&self.generation.to_be_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes
    }
    pub(crate) fn signed(&self, role: Role, binding: &CoreBinding, hash: &[u8; 32]) -> Vec<u8> {
        let mut bytes = self.purpose.domain().to_vec();
        bytes.extend_from_slice(&1u16.to_be_bytes());
        bytes.extend_from_slice(&[self.purpose as u8, role as u8]);
        bytes.extend_from_slice(&self.device);
        bytes.extend_from_slice(&self.client);
        bytes.extend_from_slice(&self.generation.to_be_bytes());
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&binding.bytes());
        bytes.extend_from_slice(hash);
        bytes
    }
    pub(crate) fn verify(
        &self,
        role: Role,
        binding: &CoreBinding,
        hash: &[u8; 32],
        signature: &[u8],
    ) -> Result<()> {
        let key = if role == Role::Device {
            &self.device
        } else {
            &self.client
        };
        VerifyingKey::from_bytes(key)
            .map_err(|_| Error::Signature)?
            .verify_strict(
                &self.signed(role, binding, hash),
                &Signature::from_slice(signature).map_err(|_| Error::Signature)?,
            )
            .map_err(|_| Error::Signature)
    }
}
/// Three values must come from the same actual Core/accepted transport. Client
/// adoption occurs only after the pinned device's signature is verified.
#[derive(Clone, PartialEq, Eq)]
pub struct CoreBinding {
    instance_hash: [u8; 32],
    connection_nonce: [u8; 32],
    transport_nonce: [u8; 32],
}
impl CoreBinding {
    pub fn new(
        instance_hash: [u8; 32],
        connection_nonce: [u8; 32],
        transport_nonce: [u8; 32],
    ) -> Result<Self> {
        if [instance_hash, connection_nonce, transport_nonce].contains(&[0; 32]) {
            return Err(Error::Context);
        }
        Ok(Self {
            instance_hash,
            connection_nonce,
            transport_nonce,
        })
    }
    pub fn instance_hash(&self) -> &[u8; 32] {
        &self.instance_hash
    }
    pub fn connection_nonce(&self) -> &[u8; 32] {
        &self.connection_nonce
    }
    pub fn transport_nonce(&self) -> &[u8; 32] {
        &self.transport_nonce
    }
    pub fn bytes(&self) -> [u8; 96] {
        let mut bytes = [0; 96];
        bytes[..32].copy_from_slice(&self.instance_hash);
        bytes[32..64].copy_from_slice(&self.connection_nonce);
        bytes[64..].copy_from_slice(&self.transport_nonce);
        bytes
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 96 {
            return Err(Error::Record);
        }
        Self::new(
            bytes[..32].try_into().map_err(|_| Error::Record)?,
            bytes[32..64].try_into().map_err(|_| Error::Record)?,
            bytes[64..].try_into().map_err(|_| Error::Record)?,
        )
    }
}
