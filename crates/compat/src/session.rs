// SPDX-License-Identifier: AGPL-3.0-only
//! Explicitly approved NAS aliases, device-bound legacy signatures and persistent replay fences.
use crate::{
    history::{directory, private_file},
    signing_bytes, verify_signature,
};
use fs2::FileExt;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use rustsimhub_policy::{allows, mask, Kind, Principal, Scope};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    path::Path,
    time::{Duration, Instant},
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use zeroize::Zeroizing;

const APP: i64 = 0x52534c31;
const READ_SCOPES: u16 = (1 << (Scope::StatusRead as u16))
    | (1 << (Scope::SmsRead as u16))
    | (1 << (Scope::CallsRead as u16));
const SKEW: i64 = 300;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Forbidden,
    Storage,
    Busy,
    Replay,
    Clock,
    Invalid,
    Unsupported,
}
pub type Result<T> = std::result::Result<T, Error>;
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "legacy: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Storage
    }
}
#[derive(Clone, Serialize)]
pub struct Binding {
    pub core_id: String,
    pub public_key: [u8; 32],
    pub generation: u64,
    pub binding_epoch: String,
    pub scopes: u16,
}
struct Session {
    binding: Binding,
    deadline: Instant,
}
pub struct Authority {
    _lock: File,
    _file: File,
    db: Connection,
    device: [u8; 32],
    sessions: HashMap<[u8; 32], Session>,
}
pub struct SignedSession<'a> {
    pub core_id: &'a str,
    pub device_id: &'a str,
    pub binding_epoch: &'a str,
    pub timestamp: &'a str,
    pub nonce: &'a str,
    pub signature: &'a str,
    pub device_proof: &'a str,
    pub body: &'a [u8],
}
pub struct Issued {
    pub token: Zeroizing<String>,
    pub expires_at: String,
}
fn random_hex() -> Result<String> {
    let mut data = [0u8; 32];
    getrandom::fill(&mut data).map_err(|_| Error::Storage)?;
    Ok(data.iter().map(|b| format!("{b:02x}")).collect())
}
pub fn core_id_valid(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn epoch_valid(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn device_bound_bytes(device: [u8; 32], epoch: &str, canonical: &[u8]) -> Result<Vec<u8>> {
    if !epoch_valid(epoch) || canonical.len() > 66000 {
        return Err(Error::Invalid);
    }
    let mut result = b"RustSimHub.LegacyDeviceProof.v1\0".to_vec();
    result.extend(device);
    for pair in epoch.as_bytes().as_chunks::<2>().0 {
        let s = std::str::from_utf8(pair).map_err(|_| Error::Invalid)?;
        result.push(u8::from_str_radix(s, 16).map_err(|_| Error::Invalid)?);
    }
    result.extend(canonical);
    Ok(result)
}
fn owner(p: &Principal) -> Result<()> {
    if p.active && p.kind == Kind::Owner && allows(p.scopes, Scope::ClientsManage) {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
impl Authority {
    pub fn open(path: &Path, device: [u8; 32], realm: &str) -> Result<Self> {
        if realm.len() != 36 {
            return Err(Error::Invalid);
        }
        directory(path).map_err(|_| Error::Forbidden)?;
        let lp = path.join(".legacy.lock");
        let lock = private_file(&lp, true, !lp.try_exists()?).map_err(|_| Error::Forbidden)?;
        FileExt::try_lock_exclusive(&lock).map_err(|_| Error::Busy)?;
        let file_path = path.join("legacy.sqlite");
        let new = !file_path.try_exists()?;
        let file = private_file(&file_path, true, new).map_err(|_| Error::Forbidden)?;
        if file.metadata()?.len() > 1024 * 1024 {
            return Err(Error::Invalid);
        }
        let db = Connection::open_with_flags(
            &file_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;")?;
        let size: i64 = db.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        if !(512..=65536).contains(&size) {
            return Err(Error::Storage);
        }
        db.execute_batch(&format!("PRAGMA max_page_count={};", 1024 * 1024 / size))?;
        if new {
            let tx = db.unchecked_transaction()?;
            tx.execute_batch("CREATE TABLE realm(singleton INTEGER PRIMARY KEY CHECK(singleton=1),device BLOB NOT NULL,epoch TEXT NOT NULL,clock_floor INTEGER NOT NULL);CREATE TABLE aliases(core_id TEXT PRIMARY KEY,public_key BLOB NOT NULL,generation INTEGER NOT NULL,binding_epoch TEXT NOT NULL,scopes INTEGER NOT NULL);CREATE TABLE nonces(digest BLOB PRIMARY KEY CHECK(length(digest)=32),until INTEGER NOT NULL);PRAGMA application_id=1381190705;PRAGMA user_version=1;")?;
            tx.execute(
                "INSERT INTO realm VALUES(1,?1,?2,0)",
                params![device, realm],
            )?;
            tx.commit()?;
        }
        if db.query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))? != APP
            || db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? != 1
        {
            return Err(Error::Storage);
        }
        let bound = db.query_row(
            "SELECT device,epoch FROM realm WHERE singleton=1",
            [],
            |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)),
        )?;
        if bound != (device.to_vec(), realm.to_owned()) {
            return Err(Error::Forbidden);
        }
        Ok(Self {
            _lock: lock,
            _file: file,
            db,
            device,
            sessions: HashMap::new(),
        })
    }
    pub fn bind(
        &mut self,
        actor: &Principal,
        current: &Principal,
        core_id: &str,
        requested: &[Scope],
    ) -> Result<Binding> {
        owner(actor)?;
        let scopes = mask(requested)
            .filter(|m| *m & !READ_SCOPES == 0 && *m & current.scopes == *m)
            .ok_or(Error::Forbidden)?;
        if !core_id_valid(core_id)
            || !current.active
            || current.kind != Kind::Nas
            || current.generation == 0
            || current.generation > i64::MAX as u64
        {
            return Err(Error::Forbidden);
        }
        let exists = self
            .db
            .query_row("SELECT 1 FROM aliases WHERE core_id=?1", [core_id], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .is_some();
        if !exists
            && self
                .db
                .query_row("SELECT COUNT(*) FROM aliases", [], |r| r.get::<_, i64>(0))?
                >= 16
        {
            return Err(Error::Busy);
        }
        let binding = Binding {
            core_id: core_id.into(),
            public_key: current.public_key,
            generation: current.generation,
            binding_epoch: random_hex()?,
            scopes,
        };
        self.db.execute("INSERT INTO aliases VALUES(?1,?2,?3,?4,?5) ON CONFLICT(core_id) DO UPDATE SET public_key=excluded.public_key,generation=excluded.generation,binding_epoch=excluded.binding_epoch,scopes=excluded.scopes",params![core_id,current.public_key,current.generation as i64,binding.binding_epoch,scopes])?;
        self.sessions.retain(|_, s| s.binding.core_id != core_id);
        self._file.sync_all()?;
        Ok(binding)
    }
    pub fn revoke(&mut self, actor: &Principal, core_id: &str) -> Result<()> {
        owner(actor)?;
        if !core_id_valid(core_id) {
            return Err(Error::Invalid);
        }
        self.db
            .execute("DELETE FROM aliases WHERE core_id=?1", [core_id])?;
        self.sessions.retain(|_, s| s.binding.core_id != core_id);
        self._file.sync_all()?;
        Ok(())
    }
    pub fn binding(&self, id: &str) -> Result<Binding> {
        if !core_id_valid(id) {
            return Err(Error::Forbidden);
        }
        self.db
            .query_row(
                "SELECT public_key,generation,binding_epoch,scopes FROM aliases WHERE core_id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, u16>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(Error::Forbidden)
            .and_then(|(key, generation, epoch, scopes)| {
                if generation < 1
                    || !epoch_valid(&epoch)
                    || scopes == 0
                    || scopes & !READ_SCOPES != 0
                {
                    return Err(Error::Storage);
                }
                Ok(Binding {
                    core_id: id.into(),
                    public_key: key.try_into().map_err(|_| Error::Storage)?,
                    generation: generation as u64,
                    binding_epoch: epoch,
                    scopes,
                })
            })
    }
    pub fn issue(
        &mut self,
        current: &Principal,
        signed: SignedSession<'_>,
        now: i64,
    ) -> Result<Issued> {
        let binding = self.binding(signed.core_id)?;
        if !current.active
            || current.kind != Kind::Nas
            || current.public_key != binding.public_key
            || current.generation != binding.generation
            || signed.device_id != crate::device_id(&self.device)
            || signed.binding_epoch != binding.binding_epoch
        {
            return Err(Error::Forbidden);
        }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Body {
            core_id: String,
        }
        let body: Body = serde_json::from_slice(signed.body).map_err(|_| Error::Invalid)?;
        if body.core_id != signed.core_id {
            return Err(Error::Forbidden);
        }
        let canonical = signing_bytes(
            "POST",
            "/v1/session",
            signed.timestamp,
            signed.nonce,
            signed.body,
        )
        .map_err(|_| Error::Invalid)?;
        verify_signature(&current.public_key, signed.signature, &canonical)
            .map_err(|_| Error::Forbidden)?;
        verify_signature(
            &current.public_key,
            signed.device_proof,
            &device_bound_bytes(self.device, signed.binding_epoch, &canonical)?,
        )
        .map_err(|_| Error::Forbidden)?;
        let stamp = OffsetDateTime::parse(signed.timestamp, &Rfc3339)
            .map_err(|_| Error::Clock)?
            .unix_timestamp();
        if now < 0 || stamp.abs_diff(now) > SKEW as u64 {
            return Err(Error::Clock);
        }
        self.sessions.retain(|_, s| Instant::now() < s.deadline);
        if self.sessions.len() >= 16
            || self
                .sessions
                .values()
                .filter(|s| s.binding.core_id == signed.core_id)
                .count()
                >= 4
        {
            return Err(Error::Busy);
        }
        let token = Zeroizing::new(random_hex()?);
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let expires = now.checked_add(900).ok_or(Error::Clock)?;
        let expires_at = OffsetDateTime::from_unix_timestamp(expires)
            .map_err(|_| Error::Clock)?
            .format(&Rfc3339)
            .map_err(|_| Error::Clock)?;
        let mut digest = Sha256::new();
        digest.update(current.public_key);
        digest.update(signed.nonce.as_bytes());
        let digest: [u8; 32] = digest.finalize().into();
        let tx = self.db.unchecked_transaction()?;
        let floor: i64 =
            tx.query_row("SELECT clock_floor FROM realm WHERE singleton=1", [], |r| {
                r.get(0)
            })?;
        if now < floor {
            return Err(Error::Clock);
        }
        tx.execute("DELETE FROM nonces WHERE until<?1", [now])?;
        if tx
            .query_row("SELECT 1 FROM nonces WHERE digest=?1", [digest], |r| {
                r.get::<_, i64>(0)
            })
            .optional()?
            .is_some()
        {
            return Err(Error::Replay);
        }
        if tx.query_row("SELECT COUNT(*) FROM nonces", [], |r| r.get::<_, i64>(0))? >= 2048 {
            return Err(Error::Busy);
        }
        tx.execute(
            "INSERT INTO nonces VALUES(?1,?2)",
            params![digest, stamp.checked_add(SKEW).ok_or(Error::Clock)?],
        )?;
        tx.execute("UPDATE realm SET clock_floor=?1 WHERE singleton=1", [now])?;
        tx.commit()?;
        self._file.sync_all()?;
        self.sessions.insert(
            hash,
            Session {
                binding,
                deadline: Instant::now() + Duration::from_secs(900),
            },
        );
        Ok(Issued { token, expires_at })
    }
    pub fn subject(&self, token: &str) -> Result<Binding> {
        if !epoch_valid(token) {
            return Err(Error::Forbidden);
        }
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let s = self
            .sessions
            .get(&hash)
            .filter(|s| Instant::now() < s.deadline)
            .ok_or(Error::Forbidden)?;
        let current = self.binding(&s.binding.core_id)?;
        if current.binding_epoch != s.binding.binding_epoch {
            return Err(Error::Forbidden);
        }
        Ok(current)
    }
    pub fn authorize(&self, token: &str, current: &Principal, scope: Scope) -> Result<()> {
        let b = self.subject(token)?;
        if !current.active
            || current.kind != Kind::Nas
            || current.public_key != b.public_key
            || current.generation != b.generation
            || !allows(current.scopes, scope)
            || !allows(b.scopes, scope)
        {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
