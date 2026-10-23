// SPDX-License-Identifier: AGPL-3.0-only
use crate::{identity::open_key, key_valid, Challenge, Error, Purpose, Result};
use ed25519_dalek::{Signature, Signer, SigningKey};
use rustsimhub_policy::{Kind, ALL_SCOPES};
use serde::Deserialize;
use std::path::Path;

/// Independent client key; never reuse device keys or expose arbitrary-message signing.
pub struct ClientKey {
    signing: SigningKey,
}
#[derive(Clone, Copy)]
pub enum ProofExpectation {
    Bootstrap,
    RecoverOwner,
    Login,
    SessionRenew { session_digest: [u8; 32] },
    Pair { kind: Kind, scopes: u16 },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcript {
    domain: String,
    purpose: Purpose,
    instance: String,
    challenge_id: String,
    device_public_key: [u8; 32],
    client_public_key: [u8; 32],
    kind: Kind,
    scopes: u16,
    generation: u64,
    #[serde(default)]
    session_digest: Option<[u8; 32]>,
}
fn nonce_valid(nonce: &str) -> bool {
    nonce.len() == 64
        && nonce
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ClientKey {
    pub fn create(directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            signing: open_key(directory.as_ref(), true, true)?,
        })
    }
    pub fn load(directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            signing: open_key(directory.as_ref(), true, false)?,
        })
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }
    pub fn prove(
        &self,
        c: &Challenge,
        pinned_device: [u8; 32],
        expectation: ProofExpectation,
    ) -> Result<Vec<u8>> {
        if c.transcript.is_empty()
            || c.transcript.len() > 4096
            || c.device_signature.len() != 64
            || !nonce_valid(&c.id)
            || c.expires_in_seconds != 60
        {
            return Err(Error::InvalidInput);
        }
        key_valid(&pinned_device)?
            .verify_strict(
                &c.transcript,
                &Signature::from_slice(&c.device_signature).map_err(|_| Error::InvalidInput)?,
            )
            .map_err(|_| Error::Forbidden)?;
        let t: Transcript =
            serde_json::from_slice(&c.transcript).map_err(|_| Error::InvalidInput)?;
        if t.domain != "RustSimHub.Auth.v1"
            || t.device_public_key != pinned_device
            || t.client_public_key != self.public_key()
            || t.challenge_id != c.id
            || !nonce_valid(&t.instance)
            || t.scopes == 0
            || t.scopes & !ALL_SCOPES != 0
        {
            return Err(Error::Forbidden);
        }
        let valid = match expectation {
            ProofExpectation::Bootstrap => {
                t.purpose == Purpose::Bootstrap
                    && t.kind == Kind::Owner
                    && t.scopes == ALL_SCOPES
                    && t.generation == 0
            }
            ProofExpectation::RecoverOwner => {
                t.purpose == Purpose::RecoverOwner
                    && t.kind == Kind::Owner
                    && t.scopes == ALL_SCOPES
                    && t.generation > 0
            }
            ProofExpectation::Login => t.purpose == Purpose::Login && t.generation > 0,
            ProofExpectation::SessionRenew { session_digest } => {
                t.purpose == Purpose::SessionRenew
                    && t.generation > 0
                    && t.session_digest == Some(session_digest)
            }
            ProofExpectation::Pair { kind, scopes } => {
                kind != Kind::Owner
                    && t.purpose == Purpose::Pair
                    && t.kind == kind
                    && t.scopes == scopes
            }
        };
        if !valid
            || !matches!(expectation, ProofExpectation::SessionRenew { .. })
                && t.session_digest.is_some()
        {
            return Err(Error::Forbidden);
        }
        Ok(self.signing.sign(&c.transcript).to_bytes().to_vec())
    }
}
