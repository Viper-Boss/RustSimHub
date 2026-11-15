// SPDX-License-Identifier: AGPL-3.0-only
//! Durable participation permission, separate from aggregate queries and seeds.
//! No identity, clock, signer, Root port, network, or scheduler is used here.
use super::{platform, StoreError};
use crate::protocol::DISCLOSURE_VERSION;
use serde::{Deserialize, Serialize};
use std::{fs::File, io::Read, path::Path};

const DOMAIN: &str = "RustSimHub/usage-stats/participation/v1";
const LIMIT: u64 = 64 * 1024;
const RECEIPTS: usize = 64;
const MAX_GENERATION: u64 = i64::MAX as u64 - 1;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub core_public_key: [u8; 32],
    pub realm: String,
}
impl Binding {
    fn valid(&self) -> bool {
        self.core_public_key != [0; 32] && uuid(&self.realm)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Actor {
    pub public_key: [u8; 32],
    pub generation: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub request_id: String,
    pub core_instance: String,
    pub expected_generation: u64,
    pub enabled: bool,
    pub disclosure_version: Option<u16>,
}
impl Change {
    fn valid(&self) -> bool {
        uuid(&self.request_id)
            && context(&self.core_instance)
            && self.expected_generation <= MAX_GENERATION
            && acknowledgement(self.enabled, self.disclosure_version)
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub change: Change,
    pub actor: Actor,
    pub generation: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: u8,
    domain: String,
    binding: Binding,
    generation: u64,
    enabled: bool,
    disclosure_version: Option<u16>,
    disabled_generation: u64,
    receipts: Vec<Receipt>,
}

/// Only the host's current, independently authenticated Owner may call change.
/// Actor/Binding are data bindings, never authentication proofs themselves.
pub struct Ledger {
    directory: platform::Directory,
    lock: File,
    binding: Binding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub generation: u64,
    pub enabled: bool,
    pub disclosure_version: Option<u16>,
    pub disabled_generation: u64,
}
impl Snapshot {
    pub const fn absent() -> Self {
        Self {
            generation: 0,
            enabled: false,
            disclosure_version: None,
            disabled_generation: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Store(StoreError),
    InvalidInput,
    WrongBinding,
    Conflict,
    Exhausted,
    Corrupt,
}
impl From<StoreError> for Error {
    fn from(value: StoreError) -> Self {
        Self::Store(value)
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Store(value.into())
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "statistics participation: {self:?}")
    }
}
impl std::error::Error for Error {}

fn context(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn acknowledgement(enabled: bool, version: Option<u16>) -> bool {
    if enabled {
        version == Some(DISCLOSURE_VERSION)
    } else {
        version.is_none()
    }
}
fn valid_actor(a: &Actor) -> bool {
    a.public_key != [0; 32] && a.generation > 0 && a.generation <= MAX_GENERATION
}

impl Record {
    fn empty(binding: Binding) -> Self {
        Self {
            format: 1,
            domain: DOMAIN.into(),
            binding,
            generation: 0,
            enabled: false,
            disclosure_version: None,
            disabled_generation: 0,
            receipts: vec![],
        }
    }
    fn validate(&self, expected: &Binding) -> Result<(), Error> {
        if !expected.valid() {
            return Err(Error::InvalidInput);
        }
        if self.format != 1 || self.domain != DOMAIN {
            return Err(Error::Corrupt);
        }
        if &self.binding != expected {
            return Err(Error::WrongBinding);
        }
        if self.generation > MAX_GENERATION
            || self.disabled_generation > self.generation
            || !acknowledgement(self.enabled, self.disclosure_version)
            || self.receipts.len() > RECEIPTS
        {
            return Err(Error::Corrupt);
        }
        let mut previous = 0;
        for (index, r) in self.receipts.iter().enumerate() {
            if !r.change.valid()
                || !valid_actor(&r.actor)
                || r.change.expected_generation.checked_add(1) != Some(r.generation)
                || r.generation <= previous
                || r.generation > self.generation
                || self.receipts[..index]
                    .iter()
                    .any(|old| old.change.request_id == r.change.request_id)
            {
                return Err(Error::Corrupt);
            }
            previous = r.generation;
        }
        match self.receipts.last() {
            Some(r)
                if r.generation == self.generation
                    && r.change.enabled == self.enabled
                    && r.change.disclosure_version == self.disclosure_version => {}
            None if self.generation == 0 && !self.enabled && self.disabled_generation == 0 => (),
            _ => return Err(Error::Corrupt),
        }
        Ok(())
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            generation: self.generation,
            enabled: self.enabled,
            disclosure_version: self.disclosure_version,
            disabled_generation: self.disabled_generation,
        }
    }
    fn apply(&mut self, actor: Actor, change: Change) -> Result<Receipt, Error> {
        if !valid_actor(&actor) || !change.valid() {
            return Err(Error::InvalidInput);
        }
        if let Some(old) = self
            .receipts
            .iter()
            .find(|r| r.change.request_id == change.request_id)
        {
            return if old.actor == actor && old.change == change {
                Ok(old.clone())
            } else {
                Err(Error::Conflict)
            };
        }
        if change.expected_generation != self.generation {
            return Err(Error::Conflict);
        }
        let generation = self
            .generation
            .checked_add(1)
            .filter(|n| *n <= MAX_GENERATION)
            .ok_or(Error::Exhausted)?;
        if !change.enabled {
            self.disabled_generation = self
                .disabled_generation
                .checked_add(1)
                .ok_or(Error::Exhausted)?;
        }
        self.generation = generation;
        self.enabled = change.enabled;
        self.disclosure_version = change.disclosure_version;
        let receipt = Receipt {
            actor,
            change,
            generation,
        };
        if self.receipts.len() == RECEIPTS {
            self.receipts.remove(0);
        }
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }
}

impl Ledger {
    /// No implicit creation or repair. Only absence of the final directory is
    /// Disabled; incomplete/corrupt directories remain errors and are preserved.
    pub fn open_if_present(path: &Path, binding: Binding) -> Result<Option<Self>, Error> {
        if !binding.valid() {
            return Err(Error::InvalidInput);
        }
        match std::fs::symlink_metadata(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
            Ok(_) => (),
        }
        let directory = platform::Directory::open(path)?;
        let lock = directory.lock(false)?;
        let ledger = Self {
            directory,
            lock,
            binding,
        };
        ledger.read()?;
        Ok(Some(ledger))
    }
    /// Publishes the initial permission and exact receipt together, after the
    /// host authorized this change. Never generates statistics entropy.
    pub fn create(
        path: &Path,
        binding: Binding,
        actor: Actor,
        change: Change,
    ) -> Result<(Self, Receipt), Error> {
        if !binding.valid() {
            return Err(Error::InvalidInput);
        }
        let mut record = Record::empty(binding.clone());
        let receipt = record.apply(actor, change)?;
        record.validate(&binding)?;
        let directory = platform::Directory::create(path)?;
        let lock = directory.lock(true)?;
        let ledger = Self {
            directory,
            lock,
            binding,
        };
        ledger.write(&record, true)?;
        Ok((ledger, receipt))
    }
    fn read(&self) -> Result<Record, Error> {
        self.directory.verify_lock(&self.lock)?;
        let mut f = self.directory.data_file()?;
        if f.metadata()?.len() > LIMIT {
            return Err(Error::Store(StoreError::TooLarge));
        }
        let mut bytes = vec![];
        Read::by_ref(&mut f)
            .take(LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::Store(StoreError::TooLarge));
        }
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| Error::Corrupt)?;
        record.validate(&self.binding)?;
        Ok(record)
    }
    fn write(&self, record: &Record, new: bool) -> Result<(), Error> {
        record.validate(&self.binding)?;
        self.directory.verify_lock(&self.lock)?;
        let bytes = serde_json::to_vec(record).map_err(|_| Error::Corrupt)?;
        if bytes.len() as u64 > LIMIT {
            return Err(Error::Store(StoreError::TooLarge));
        }
        self.directory.atomic_write(&bytes, new)?;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        Ok(self.read()?.snapshot())
    }
    /// A permission saved by an old/revoked Owner is not a fresh Owner grant.
    pub fn authorized_for(&self, actor: &Actor) -> Result<bool, Error> {
        if !valid_actor(actor) {
            return Err(Error::InvalidInput);
        }
        let record = self.read()?;
        Ok(record.enabled && record.receipts.last().is_some_and(|r| &r.actor == actor))
    }
    pub fn receipt(&self, actor: &Actor, id: &str) -> Result<Option<Receipt>, Error> {
        if !valid_actor(actor) || !uuid(id) {
            return Err(Error::InvalidInput);
        }
        Ok(self
            .read()?
            .receipts
            .into_iter()
            .find(|r| r.change.request_id == id && &r.actor == actor))
    }
    pub fn change(&self, actor: Actor, change: Change) -> Result<Receipt, Error> {
        let mut record = self.read()?;
        let before = record.generation;
        let receipt = record.apply(actor, change)?;
        if record.generation != before {
            self.write(&record, false)?;
        }
        Ok(receipt)
    }
}
