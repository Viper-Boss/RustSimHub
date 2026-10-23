// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Serialized by core on a blocking worker. No remote-supplied clock or role.
mod client;
mod identity;
mod setup;
pub use client::{ClientKey, ProofExpectation};
use ed25519_dalek::{Signature, VerifyingKey};
#[cfg(target_os = "linux")]
pub use identity::{lock_private_directory, publish_private_directory};
pub use identity::{read_private, write_private_new};
pub use identity::{Identity, SimFingerprinter};
use rustsimhub_policy::{allows, mask, Kind, Principal, Scope, ALL_SCOPES};
use rustsimhub_storage::Handle;
use serde::{Deserialize, Serialize};
pub use setup::LocalSetup;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Io,
    Entropy,
    CorruptIdentity,
    Storage,
    Busy,
    InvalidInput,
    Forbidden,
    Expired,
    Replay,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "auth: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<rustsimhub_storage::Error> for Error {
    fn from(_: rustsimhub_storage::Error) -> Self {
        Self::Storage
    }
}

const CHALLENGE_TTL: Duration = Duration::from_secs(60);
const PAIR_TTL: Duration = Duration::from_secs(300);
const SESSION_TTL: Duration = Duration::from_secs(900);
const REAUTH_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Purpose {
    Bootstrap,
    Pair,
    Login,
    RecoverOwner,
}

/// Sign the EXACT transcript bytes, never reconstructed JSON. It binds both keys and purpose.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub id: String,
    pub transcript: Vec<u8>,
    pub device_signature: Vec<u8>,
    pub expires_in_seconds: u32,
}
#[derive(Serialize)]
struct Transcript<'a> {
    domain: &'static str,
    purpose: Purpose,
    instance: &'a str,
    challenge_id: &'a str,
    device_public_key: [u8; 32],
    client_public_key: [u8; 32],
    kind: Kind,
    scopes: u16,
    generation: u64,
}
struct PendingChallenge {
    challenge: Challenge,
    key: [u8; 32],
    kind: Kind,
    scopes: u16,
    generation: u64,
    purpose: Purpose,
    deadline: Instant,
}
struct Pairing {
    key: [u8; 32],
    kind: Kind,
    scopes: u16,
    generation: u64,
    deadline: Instant,
}
struct Session {
    principal: Principal,
    deadline: Instant,
    authenticated: Instant,
}
/// Raw opaque token is returned once, and never Debug. Engine stores SHA-256 only.
pub struct Token(Zeroizing<String>);
impl Token {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

pub struct Auth {
    identity: Identity,
    store: Handle,
    instance: String,
    challenges: HashMap<String, PendingChallenge>,
    pairings: HashMap<String, Pairing>,
    sessions: HashMap<[u8; 32], Session>,
    window: Option<Instant>,
    budget: u32,
    refill: Instant,
}
fn random_hex() -> Result<String> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    getrandom::fill(bytes.as_mut()).map_err(|_| Error::Entropy)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn key_valid(key: &[u8; 32]) -> Result<VerifyingKey> {
    let key = VerifyingKey::from_bytes(key).map_err(|_| Error::InvalidInput)?;
    if key.is_weak() {
        return Err(Error::InvalidInput);
    }
    Ok(key)
}
fn token_digest(token: &str) -> Result<[u8; 32]> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Forbidden);
    }
    Ok(Sha256::digest(token.as_bytes()).into())
}
impl Auth {
    pub fn observer_instance(&self) -> String {
        self.instance.clone()
    }
    pub fn sim_fingerprinter(&self) -> Result<SimFingerprinter> {
        self.identity.sim_fingerprinter()
    }
    pub fn sim_fingerprint(&self, iccid: &str) -> Result<[u8; 32]> {
        self.identity.sim_fingerprint(iccid)
    }
    pub fn calls(
        &mut self,
        token: &str,
        after: Option<rustsimhub_storage::InboxCursor>,
        limit: usize,
    ) -> Result<rustsimhub_storage::CallPage> {
        self.authorize(token, Scope::CallsRead)?;
        Ok(self.store.calls(after, limit)?)
    }
    pub fn call_detail(
        &mut self,
        token: &str,
        id: String,
    ) -> Result<rustsimhub_storage::CallSummary> {
        self.authorize(token, Scope::CallsRead)?;
        Ok(self.store.call_detail(id)?)
    }
    pub fn call_events(
        &mut self,
        token: &str,
        after: Option<rustsimhub_storage::InboxCursor>,
        limit: usize,
    ) -> Result<rustsimhub_storage::CallEvents> {
        self.authorize(token, Scope::CallsRead)?;
        Ok(self.store.call_events(after, limit)?)
    }
    pub fn line_status(&mut self, token: &str) -> Result<rustsimhub_storage::LineView> {
        self.authorize(token, Scope::StatusRead)?;
        Ok(self.store.line_status()?)
    }
    pub fn inbox(
        &mut self,
        token: &str,
        after: Option<rustsimhub_storage::InboxCursor>,
        limit: usize,
    ) -> Result<rustsimhub_storage::InboxPage> {
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.store.inbox(after, limit)?)
    }
    pub fn sms_detail(&mut self, token: &str, id: String) -> Result<rustsimhub_storage::SmsDetail> {
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.store.sms_detail(id)?)
    }
    pub fn inbox_events(
        &mut self,
        token: &str,
        after: Option<rustsimhub_storage::InboxCursor>,
        limit: usize,
    ) -> Result<rustsimhub_storage::InboxEvents> {
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.store.inbox_events(after, limit)?)
    }
    pub fn sms_management_available(&mut self, token: &str) -> Result<bool> {
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.owner(token).is_ok())
    }
    pub fn set_sms_deleted(
        &mut self,
        token: &str,
        id: String,
        deleted: bool,
    ) -> Result<rustsimhub_storage::SmsVisibility> {
        let owner = self.owner(token)?;
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.store.set_sms_deleted(owner, id, deleted)?)
    }
    pub fn deleted_inbox(
        &mut self,
        token: &str,
        after: Option<rustsimhub_storage::InboxCursor>,
        limit: usize,
    ) -> Result<rustsimhub_storage::InboxPage> {
        let owner = self.owner(token)?;
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.store.deleted_inbox(owner, after, limit)?)
    }
    pub fn deleted_sms_detail(
        &mut self,
        token: &str,
        id: String,
    ) -> Result<rustsimhub_storage::SmsDetail> {
        let owner = self.owner(token)?;
        self.authorize(token, Scope::SmsRead)?;
        Ok(self.store.deleted_sms_detail(owner, id)?)
    }
    /// Never generate a replacement seed for a DB that already has a bound identity.
    pub fn open(directory: impl AsRef<std::path::Path>, store: Handle) -> Result<Self> {
        let identity = if store.identity_public_key()?.is_some() {
            Identity::load(directory)?
        } else {
            Identity::open(directory)?
        };
        Self::new(identity, store)
    }
    pub fn new(identity: Identity, store: Handle) -> Result<Self> {
        store.bind_identity(identity.public_key())?;
        Ok(Self {
            identity,
            store,
            instance: random_hex()?,
            challenges: HashMap::new(),
            pairings: HashMap::new(),
            sessions: HashMap::new(),
            window: None,
            budget: 32,
            refill: Instant::now(),
        })
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.identity.public_key()
    }
    /// Separate optional policy files must be bound to this durable business realm.
    pub fn durable_realm(&self) -> Result<String> {
        Ok(self.store.cursor()?.db_epoch)
    }
    /// For explicit delegation only; the caller first authenticates a recent owner.
    pub fn current_principal(&self, key: [u8; 32]) -> Result<Principal> {
        self.store
            .principal(key)?
            .filter(|p| p.active)
            .ok_or(Error::Forbidden)
    }
    /// A separate legacy session authority checks its bearer before using these
    /// views. Still recheck the exact live NAS principal; no identity inferred from IPC UID.
    pub fn nas_read(
        &self,
        p: &Principal,
        entity: rustsimhub_domain::NasEntity,
        after: Option<rustsimhub_storage::InboxCursor>,
        limit: usize,
        id: Option<String>,
    ) -> Result<serde_json::Value> {
        use rustsimhub_domain::NasEntity;
        let scope = match entity {
            NasEntity::Status => Scope::StatusRead,
            NasEntity::Calls => Scope::CallsRead,
            _ => Scope::SmsRead,
        };
        if p.kind != Kind::Nas
            || !p.active
            || !allows(p.scopes, scope)
            || self.current_principal(p.public_key)? != *p
        {
            return Err(Error::Forbidden);
        }
        let value = match entity {
            NasEntity::Status => serde_json::to_value(self.store.line_status()?),
            NasEntity::Sms => serde_json::to_value(self.store.inbox(after, limit)?),
            NasEntity::Calls => serde_json::to_value(self.store.calls(after, limit)?),
            NasEntity::SmsDetail => {
                serde_json::to_value(self.store.sms_detail(id.ok_or(Error::InvalidInput)?)?)
            }
        }
        .map_err(|_| Error::InvalidInput)?;
        if self.current_principal(p.public_key)? != *p {
            return Err(Error::Forbidden);
        }
        Ok(value)
    }
    pub fn recent_owner(&mut self, token: &str) -> Result<Principal> {
        self.owner(token)
    }
    fn prune(&mut self, now: Instant) {
        self.challenges.retain(|_, p| now < p.deadline);
        self.pairings.retain(|_, p| now < p.deadline);
        self.sessions.retain(|_, s| now < s.deadline);
        if self.window.is_some_and(|d| now >= d) {
            self.window = None;
        }
    }
    fn rate_limit(&mut self, now: Instant) -> Result<()> {
        let seconds = now.duration_since(self.refill).as_secs();
        if seconds > 0 {
            self.budget = (self.budget as u64 + seconds.min(32) * 8).min(32) as u32;
            self.refill = now;
        }
        if self.budget == 0 {
            return Err(Error::Busy);
        }
        self.budget -= 1;
        self.prune(now);
        Ok(())
    }
    fn challenge(
        &mut self,
        key: [u8; 32],
        kind: Kind,
        scopes: u16,
        generation: u64,
        purpose: Purpose,
    ) -> Result<Challenge> {
        self.rate_limit(Instant::now())?;
        key_valid(&key)?;
        if key == self.public_key() || self.challenges.len() >= 32 {
            return Err(Error::Forbidden);
        }
        let id = random_hex()?;
        let transcript = serde_json::to_vec(&Transcript {
            domain: "RustSimHub.Auth.v1",
            purpose,
            instance: &self.instance,
            challenge_id: &id,
            device_public_key: self.public_key(),
            client_public_key: key,
            kind,
            scopes,
            generation,
        })
        .map_err(|_| Error::InvalidInput)?;
        let reply = Challenge {
            id: id.clone(),
            device_signature: self.identity.sign(&transcript),
            transcript: transcript.clone(),
            expires_in_seconds: 60,
        };
        let saved = Challenge {
            id: id.clone(),
            transcript,
            device_signature: vec![],
            expires_in_seconds: 60,
        };
        self.challenges.insert(
            id,
            PendingChallenge {
                challenge: saved,
                key,
                kind,
                scopes,
                generation,
                purpose,
                deadline: Instant::now() + CHALLENGE_TTL,
            },
        );
        Ok(reply)
    }
    /// Offline provisioning primitive. Ordinary IPC denies it; an explicit
    /// LocalSetup window additionally requires the private out-of-band code.
    pub fn bootstrap_challenge(&mut self, key: [u8; 32]) -> Result<Challenge> {
        self.challenge(key, Kind::Owner, ALL_SCOPES, 0, Purpose::Bootstrap)
    }
    fn consume(
        &mut self,
        id: &str,
        signature: &[u8],
        purpose: Purpose,
    ) -> Result<PendingChallenge> {
        self.rate_limit(Instant::now())?;
        if id.len() != 64 || signature.len() != 64 {
            return Err(Error::InvalidInput);
        }
        // Take before verification: a bad proof consumes this nonce too.
        let p = self.challenges.remove(id).ok_or(Error::Replay)?;
        if p.purpose != purpose {
            return Err(Error::Forbidden);
        }
        if Instant::now() >= p.deadline {
            return Err(Error::Expired);
        }
        key_valid(&p.key)?
            .verify_strict(
                &p.challenge.transcript,
                &Signature::from_slice(signature).map_err(|_| Error::InvalidInput)?,
            )
            .map_err(|_| Error::Forbidden)?;
        Ok(p)
    }
    pub fn bootstrap_finish(&mut self, id: &str, signature: &[u8]) -> Result<Principal> {
        let p = self.consume(id, signature, Purpose::Bootstrap)?;
        Ok(self.store.bootstrap_owner(p.key)?)
    }
    /// Trusted offline tool only. There is no corresponding IPC command.
    pub fn recovery_challenge(&mut self, key: [u8; 32]) -> Result<Challenge> {
        let p = self
            .store
            .principal(key)?
            .filter(|p| p.kind == Kind::Owner && !p.active && p.scopes == ALL_SCOPES)
            .ok_or(Error::Forbidden)?;
        self.challenge(
            key,
            Kind::Owner,
            p.scopes,
            p.generation,
            Purpose::RecoverOwner,
        )
    }
    pub fn recovery_finish(&mut self, id: &str, signature: &[u8]) -> Result<Principal> {
        let proof = self.consume(id, signature, Purpose::RecoverOwner)?;
        let owner = self
            .store
            .principal(proof.key)?
            .filter(|p| {
                p.kind == Kind::Owner
                    && !p.active
                    && p.generation == proof.generation
                    && p.scopes == proof.scopes
            })
            .ok_or(Error::Forbidden)?;
        let recovered = self.store.recover_owner(owner)?;
        self.challenges.clear();
        self.pairings.clear();
        self.sessions.clear();
        self.window = None;
        Ok(recovered)
    }
    pub fn login_challenge(&mut self, key: [u8; 32]) -> Result<Challenge> {
        let p = self
            .store
            .principal(key)?
            .filter(|p| p.active)
            .ok_or(Error::Forbidden)?;
        self.challenge(key, p.kind, p.scopes, p.generation, Purpose::Login)
    }
    pub fn login_finish(&mut self, id: &str, signature: &[u8]) -> Result<Token> {
        let p = self.consume(id, signature, Purpose::Login)?;
        let current = self
            .store
            .principal(p.key)?
            .filter(|c| {
                c.active && c.generation == p.generation && c.kind == p.kind && c.scopes == p.scopes
            })
            .ok_or(Error::Forbidden)?;
        if self.sessions.len() >= 64 {
            return Err(Error::Busy);
        }
        let token = Token(Zeroizing::new(random_hex()?));
        let now = Instant::now();
        self.sessions.insert(
            token_digest(token.expose())?,
            Session {
                principal: current,
                deadline: now + SESSION_TTL,
                authenticated: now,
            },
        );
        Ok(token)
    }
    pub fn authorize(&mut self, token: &str, scope: Scope) -> Result<Principal> {
        self.prune(Instant::now());
        let digest = token_digest(token)?;
        let session = self.sessions.get(&digest).ok_or(Error::Forbidden)?;
        let current = self
            .store
            .principal(session.principal.public_key)?
            .filter(|p| p.active && *p == session.principal)
            .ok_or(Error::Forbidden)?;
        if !allows(current.scopes, scope) {
            return Err(Error::Forbidden);
        }
        Ok(current)
    }
    fn owner(&mut self, token: &str) -> Result<Principal> {
        let p = self.authorize(token, Scope::ClientsManage)?;
        if p.kind != Kind::Owner {
            return Err(Error::Forbidden);
        }
        let s = self
            .sessions
            .get(&token_digest(token)?)
            .ok_or(Error::Forbidden)?;
        if Instant::now().duration_since(s.authenticated) >= REAUTH_TTL {
            return Err(Error::Expired);
        }
        Ok(p)
    }
    pub fn open_pairing(&mut self, owner_token: &str) -> Result<()> {
        self.owner(owner_token)?;
        // Reopening a window cancels old requests, even if their proofs were already supplied.
        self.challenges.retain(|_, p| p.purpose != Purpose::Pair);
        self.pairings.clear();
        self.window = Some(Instant::now() + PAIR_TTL);
        Ok(())
    }
    pub fn pairing_challenge(
        &mut self,
        key: [u8; 32],
        kind: Kind,
        scopes: &[Scope],
    ) -> Result<Challenge> {
        self.prune(Instant::now());
        let prior = self.store.principal(key)?;
        if self.window.is_none()
            || kind == Kind::Owner
            || prior
                .as_ref()
                .is_some_and(|p| p.active || p.kind == Kind::Owner)
        {
            return Err(Error::Forbidden);
        }
        let scopes = mask(scopes).ok_or(Error::InvalidInput)?;
        self.challenge(
            key,
            kind,
            scopes,
            prior.map(|p| p.generation).unwrap_or(0),
            Purpose::Pair,
        )
    }
    pub fn pairing_finish(&mut self, id: &str, signature: &[u8]) -> Result<String> {
        let p = self.consume(id, signature, Purpose::Pair)?;
        let deadline = self
            .window
            .filter(|d| Instant::now() < *d)
            .ok_or(Error::Expired)?;
        if self.pairings.len() >= 16 {
            return Err(Error::Busy);
        }
        let id = random_hex()?;
        self.pairings.insert(
            id.clone(),
            Pairing {
                key: p.key,
                kind: p.kind,
                scopes: p.scopes,
                generation: p.generation,
                deadline,
            },
        );
        Ok(id)
    }
    /// Approval explicitly names the public key and exact permissions shown to the owner.
    pub fn approve(
        &mut self,
        token: &str,
        id: &str,
        key: [u8; 32],
        kind: Kind,
        scopes: &[Scope],
    ) -> Result<Principal> {
        let owner = self.owner(token)?;
        let p = self.pairings.remove(id).ok_or(Error::Replay)?;
        if Instant::now() >= p.deadline || self.window.is_none() {
            return Err(Error::Expired);
        }
        if p.key != key || p.kind != kind || Some(p.scopes) != mask(scopes) {
            return Err(Error::Forbidden);
        }
        Ok(self
            .store
            .approve_client_if(owner, key, kind, p.scopes, p.generation)?)
    }
    pub fn revoke(&mut self, token: &str, key: [u8; 32]) -> Result<()> {
        let owner = self.owner(token)?;
        self.store.revoke_client(owner, key)?;
        self.sessions.retain(|_, s| s.principal.public_key != key);
        self.challenges.retain(|_, p| p.key != key);
        Ok(())
    }
    pub fn clients(&mut self, token: &str) -> Result<Vec<Principal>> {
        self.owner(token)?;
        Ok(self.store.principals()?)
    }
    pub fn pending_pairings(&mut self, token: &str) -> Result<Vec<PendingPairing>> {
        self.owner(token)?;
        let now = Instant::now();
        self.prune(now);
        let mut pending: Vec<_> = self
            .pairings
            .iter()
            .map(|(id, p)| PendingPairing {
                id: id.clone(),
                public_key: p.key,
                kind: p.kind,
                scopes: p.scopes,
                expires_in_seconds: p.deadline.saturating_duration_since(now).as_secs(),
            })
            .collect();
        pending.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(pending)
    }
}

#[derive(Serialize)]
pub struct PendingPairing {
    pub id: String,
    pub public_key: [u8; 32],
    pub kind: Kind,
    pub scopes: u16,
    pub expires_in_seconds: u64,
}

#[cfg(test)]
mod tests;
