// SPDX-License-Identifier: AGPL-3.0-only
use crate::{model::cursor, *};
use fs2::FileExt;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use rustsimhub_domain::{InboxCursor, LineEpoch};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::Duration,
};
use uuid::Uuid;

const APP_ID: i64 = 0x52534657;
const FORMAT: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Queued,
    InFlight,
    Confirmed,
    Failed,
    Unknown,
    Cancelled,
}
impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InFlight => "in_flight",
            Self::Confirmed => "confirmed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
            Self::Cancelled => "cancelled",
        }
    }
    fn parse(s: &str) -> Result<Self> {
        match s {
            "queued" => Ok(Self::Queued),
            "in_flight" => Ok(Self::InFlight),
            "confirmed" => Ok(Self::Confirmed),
            "failed" => Ok(Self::Failed),
            "unknown" => Ok(Self::Unknown),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(Error::Storage),
        }
    }
}
#[derive(Debug, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub generation: u64,
    pub queued: usize,
    pub in_flight: usize,
    pub unknown: usize,
    // Intent alone is never proof that the eventual service has stopped.
    pub process_state_verified: bool,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub seq: String,
    pub id: String,
    pub message_id: String,
    pub target_id: String,
    pub rule_id: String,
    pub generation: u64,
    pub state: State,
}

// Never Debug: contains the immutable destination, credentials and SMS payload.
pub struct Claim {
    pub id: String,
    pub attempt_id: String,
    pub generation: u64,
    pub line: LineEpoch,
    pub request: PreparedRequest,
}

/// Sole writer, intended to be owned by Core's blocking actor. A worker must
/// claim through its separately authenticated bounded IPC, not open this DB.
pub struct Queue {
    conn: Connection,
    _lock: File,
    binding: Binding,
}
struct Meta {
    generation: i64,
    enabled: bool,
    seq: i64,
    policy: Policy,
}

fn file(path: &Path) -> Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o777 != 0o600
        || m.nlink() != 1
        || m.len() > 32 * 1024 * 1024
    {
        return Err(Error::Forbidden);
    }
    Ok(f)
}
impl Queue {
    pub(crate) fn binding(&self) -> &Binding {
        &self.binding
    }
    /// Recheck the exact immutable in-flight request as well as its claim IDs.
    /// Called by the authenticated adapter immediately before transport writes.
    pub(crate) fn check_claim(
        &self,
        binding: &Binding,
        claim: &Claim,
        line: LineEpoch,
    ) -> Result<()> {
        if !self.fence(binding, claim.generation)?.enabled || claim.line != line {
            return Err(Error::Stale);
        }
        let exact:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=? AND attempt_id=? AND generation=? AND line=? AND state='in_flight' AND request=?)",params![claim.id,claim.attempt_id,claim.generation as i64,line.0 as i64,serde_json::to_string(&claim.request)?],|r|r.get(0))?;
        if !exact {
            return Err(Error::Stale);
        }
        Ok(())
    }
    /// Existing private directory only. A mismatching Owner/realm is refused;
    /// it never silently adopts old jobs for a new owner or restored database.
    pub fn open(directory: &Path, binding: Binding) -> Result<Self> {
        binding.validate()?;
        let m = fs::symlink_metadata(directory)?;
        if !m.is_dir()
            || m.uid() != rustix::process::geteuid().as_raw()
            || m.mode() & 0o777 != 0o700
        {
            return Err(Error::Forbidden);
        }
        let lock = file(&directory.join(".writer.lock"))?;
        lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
        for name in [
            "outbox.sqlite3-journal",
            "outbox.sqlite3-wal",
            "outbox.sqlite3-shm",
        ] {
            if directory.join(name).try_exists()? {
                file(&directory.join(name))?;
            }
        }
        let path = directory.join("outbox.sqlite3");
        file(&path)?;
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(3))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let app: i64 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
        let objects: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )?;
        if version == 0 {
            if app != 0 || objects != 0 {
                return Err(Error::Forbidden);
            }
        } else if version != FORMAT || app != APP_ID {
            return Err(Error::Forbidden);
        }
        conn.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA synchronous=FULL;PRAGMA foreign_keys=ON;PRAGMA secure_delete=ON;PRAGMA max_page_count=8192;")?;
        let mode: String = conn.query_row("PRAGMA journal_mode=DELETE", [], |r| r.get(0))?;
        if mode != "delete" {
            return Err(Error::Storage);
        }
        if version == 0 {
            let tx = conn.transaction()?;
            tx.execute_batch("CREATE TABLE meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),binding TEXT NOT NULL,generation INTEGER NOT NULL CHECK(generation>=0),enabled INTEGER NOT NULL CHECK(enabled IN(0,1)),cursor INTEGER NOT NULL CHECK(cursor>=0),policy TEXT NOT NULL) STRICT;
CREATE TABLE requests(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,digest BLOB NOT NULL CHECK(length(digest)=32),generation INTEGER NOT NULL) STRICT;
CREATE TABLE jobs(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,message_id TEXT NOT NULL,target_id TEXT NOT NULL,rule_id TEXT NOT NULL,generation INTEGER NOT NULL,line INTEGER NOT NULL,state TEXT NOT NULL CHECK(state IN('queued','in_flight','confirmed','failed','unknown','cancelled')),attempt_id TEXT,request TEXT,UNIQUE(message_id,target_id,generation)) STRICT;
CREATE INDEX pending ON jobs(state,seq);")?;
            tx.execute(
                "INSERT INTO meta VALUES(1,?,0,0,0,?)",
                params![
                    serde_json::to_string(&binding)?,
                    serde_json::to_string(&Policy::default())?
                ],
            )?;
            tx.pragma_update(None, "application_id", APP_ID)?;
            tx.pragma_update(None, "user_version", FORMAT)?;
            tx.commit()?;
        }
        let saved: String =
            conn.query_row("SELECT binding FROM meta WHERE singleton=1", [], |r| {
                r.get(0)
            })?;
        if saved.len() > 4096 || serde_json::from_str::<Binding>(&saved)? != binding {
            return Err(Error::Forbidden);
        }
        let integrity: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(Error::Storage);
        }
        // A lost worker/Core reply is never queued for automatic delivery again.
        conn.execute(
            "UPDATE jobs SET state='unknown' WHERE state='in_flight'",
            [],
        )?;
        let q = Self {
            conn,
            _lock: lock,
            binding,
        };
        q.meta()?;
        File::open(directory)?.sync_all()?;
        Ok(q)
    }
    fn fence(&self, binding: &Binding, generation: u64) -> Result<Meta> {
        if binding != &self.binding {
            return Err(Error::Forbidden);
        }
        let meta = self.meta()?;
        if meta.generation as u64 != generation {
            return Err(Error::Stale);
        }
        Ok(meta)
    }
    fn meta(&self) -> Result<Meta> {
        let (generation, enabled, seq, json): (i64, bool, i64, String) = self.conn.query_row(
            "SELECT generation,enabled,cursor,policy FROM meta WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        if json.len() > 65536 || generation < 0 || seq < 0 {
            return Err(Error::Storage);
        }
        let policy: Policy = serde_json::from_str(&json)?;
        policy.validate()?;
        if policy.enabled != enabled {
            return Err(Error::Storage);
        }
        Ok(Meta {
            generation,
            enabled,
            seq,
            policy,
        })
    }
    pub fn status(&self) -> Result<Status> {
        let m = self.meta()?;
        let count = |state: &str| -> Result<usize> {
            let count: i64 =
                self.conn
                    .query_row("SELECT count(*) FROM jobs WHERE state=?", [state], |r| {
                        r.get(0)
                    })?;
            usize::try_from(count).map_err(|_| Error::Storage)
        };
        Ok(Status {
            enabled: m.enabled,
            generation: m.generation as u64,
            queued: count("queued")?,
            in_flight: count("in_flight")?,
            unknown: count("unknown")?,
            process_state_verified: false,
        })
    }
    /// Caller authenticates and rechecks the actual recent Owner. Start/change
    /// uses the REAL committed business event head, never a caller's old cursor.
    pub fn configure(
        &mut self,
        binding: &Binding,
        expected: u64,
        request_id: &str,
        policy: Policy,
        head: InboxCursor,
    ) -> Result<u64> {
        binding.validate()?;
        policy.validate()?;
        if binding != &self.binding || !crate::model::uuid(request_id) {
            return Err(Error::Forbidden);
        }
        let seq = cursor(&head, &binding.realm)?;
        let encoded = serde_json::to_string(&policy)?;
        let digest = Sha256::digest(serde_json::to_vec(&(binding, expected, &encoded, &head))?);
        let old = self
            .conn
            .query_row(
                "SELECT digest,generation FROM requests WHERE id=?",
                [request_id],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;
        if let Some((saved, generation)) = old {
            if saved != digest.as_slice() {
                return Err(Error::Conflict);
            }
            if self.meta()?.generation != generation {
                return Err(Error::Stale);
            }
            return Ok(generation as u64);
        }
        if seq < self.fence(binding, expected)?.seq {
            return Err(Error::Stale);
        }
        let next = expected
            .checked_add(1)
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or(Error::Full)?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE jobs SET state='cancelled',request=NULL WHERE state='queued'",
            [],
        )?;
        tx.execute(
            "UPDATE jobs SET state='unknown' WHERE state='in_flight'",
            [],
        )?;
        tx.execute(
            "UPDATE meta SET generation=?,enabled=?,cursor=?,policy=? WHERE singleton=1",
            params![next as i64, policy.enabled, seq, encoded],
        )?;
        tx.execute(
            "INSERT INTO requests(id,digest,generation) VALUES(?,?,?)",
            params![request_id, digest.as_slice(), next as i64],
        )?;
        // Expired request IDs cannot replay: their old expected generation no
        // longer matches. Keep a bounded 64-response idempotency window.
        tx.execute("DELETE FROM requests WHERE seq NOT IN(SELECT seq FROM requests ORDER BY seq DESC LIMIT 64)",[])?;
        tx.commit()?;
        Ok(next)
    }
    pub fn event_cursor(&self) -> Result<InboxCursor> {
        let m = self.meta()?;
        Ok(InboxCursor {
            db_epoch: self.binding.realm.clone(),
            seq: m.seq.to_string(),
        })
    }
    /// Cursor advance and all recipients commit atomically. Backpressure leaves
    /// this cursor intact; no successfully queued message is silently dropped.
    pub fn ingest(
        &mut self,
        binding: &Binding,
        generation: u64,
        at: InboxCursor,
        message: Option<&Message>,
    ) -> Result<usize> {
        let m = self.fence(binding, generation)?;
        if !m.enabled {
            return Err(Error::Disabled);
        }
        let seq = cursor(&at, &binding.realm)?;
        if seq < m.seq {
            return Err(Error::Stale);
        }
        if seq == m.seq {
            return Ok(0);
        }
        let mut jobs = Vec::new();
        let mut recipients = std::collections::HashSet::new();
        if let Some(message) = message {
            message.validate()?;
            for rule in &m.policy.rules {
                if rule.matches(message) {
                    for target_id in &rule.targets {
                        if !recipients.insert(target_id) {
                            continue;
                        }
                        let exists:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE message_id=? AND target_id=? AND generation=?)",params![message.id,target_id,m.generation],|r|r.get(0))?;
                        if exists {
                            continue;
                        }
                        let target = m
                            .policy
                            .targets
                            .iter()
                            .find(|t| &t.id == target_id)
                            .ok_or(Error::Storage)?;
                        let id = Uuid::new_v4().to_string();
                        let request = serde_json::to_string(&prepare(target, rule, message, &id)?)?;
                        if request.len() > 65536 {
                            return Err(Error::Invalid);
                        }
                        jobs.push((id, target_id.clone(), rule.id.clone(), request));
                    }
                }
            }
        }
        let (pending, total): (i64, i64) = self.conn.query_row(
            "SELECT coalesce(sum(state IN('queued','in_flight','unknown')),0),count(*) FROM jobs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let pending = usize::try_from(pending).map_err(|_| Error::Storage)?;
        let total = usize::try_from(total).map_err(|_| Error::Storage)?;
        if pending > MAX_PENDING
            || total > MAX_RECEIPTS
            || jobs.len() > MAX_PENDING - pending
            || jobs.len() > MAX_RECEIPTS - total
        {
            return Err(Error::Full);
        }
        let count = jobs.len();
        let tx = self.conn.transaction()?;
        for (id, target, rule, request) in jobs {
            let message = message.ok_or(Error::Storage)?;
            tx.execute("INSERT INTO jobs(id,message_id,target_id,rule_id,generation,line,state,request) VALUES(?,?,?,?,?,?,'queued',?)",params![id,message.id,target,rule,m.generation,message.line.0 as i64,request])?;
        }
        tx.execute("UPDATE meta SET cursor=? WHERE singleton=1", [seq])?;
        tx.commit()?;
        Ok(count)
    }
    /// Use actual Store events/details, not a separately invented SMS feed.
    /// Called only while enabled by Core. No polling loop is started here.
    pub fn capture(
        &mut self,
        binding: &Binding,
        generation: u64,
        reader: &rustsimhub_storage::Handle,
    ) -> Result<usize> {
        if !self.fence(binding, generation)?.enabled {
            return Err(Error::Disabled);
        }
        let before = reader.line_status().map_err(|_| Error::Storage)?;
        if before.status != rustsimhub_storage::LineStatus::Ready {
            return Err(Error::Stale);
        }
        let page = reader
            .inbox_events(Some(self.event_cursor()?), 32)
            .map_err(|e| {
                if e == rustsimhub_storage::Error::ResyncRequired {
                    Error::Stale
                } else {
                    Error::Storage
                }
            })?;
        if page.cursor.db_epoch != binding.realm {
            return Err(Error::Stale);
        }
        let mut count = 0;
        for event in page.events {
            let message = if event.kind == "sms_received" && event.line == before.line {
                let id = event.sms_id.ok_or(Error::Storage)?;
                match reader.sms_detail(id.clone()) {
                    Ok(d) => {
                        if d.summary.id != id || d.summary.line != event.line {
                            return Err(Error::Stale);
                        }
                        Some(Message {
                            id: d.summary.id,
                            line: d.summary.line,
                            sender: d.sender,
                            timestamp: d.timestamp,
                            body: d.text,
                            duplicate_candidate: d.summary.duplicate_candidate.is_some(),
                            binary: d.summary.binary,
                        })
                    }
                    Err(rustsimhub_storage::Error::NotFound) => None, // Local trash is never an alternate source.
                    Err(_) => return Err(Error::Storage),
                }
            } else {
                None
            };
            let live = reader.line_status().map_err(|_| Error::Storage)?;
            if live.status != rustsimhub_storage::LineStatus::Ready || live.line != before.line {
                return Err(Error::Stale);
            }
            count += self.ingest(
                binding,
                generation,
                InboxCursor {
                    db_epoch: binding.realm.clone(),
                    seq: event.seq,
                },
                message.as_ref(),
            )?;
        }
        Ok(count)
    }
    /// SINGLE application attempt. The eventual transport must recheck the
    /// live authority/line fence again immediately before application writes.
    pub fn claim(
        &mut self,
        binding: &Binding,
        generation: u64,
        live_line: LineEpoch,
    ) -> Result<Option<Claim>> {
        let m = self.fence(binding, generation)?;
        if !m.enabled {
            return Err(Error::Disabled);
        }
        if live_line.0 == 0 || live_line.0 > i64::MAX as u64 {
            return Err(Error::Invalid);
        }
        let tx = self.conn.transaction()?;
        let active: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE state='in_flight')",
            [],
            |r| r.get(0),
        )?;
        if active {
            return Err(Error::Busy);
        }
        tx.execute(
            "UPDATE jobs SET state='cancelled',request=NULL WHERE state='queued' AND line!=?",
            [live_line.0 as i64],
        )?;
        let row = tx
            .query_row(
                "SELECT id,generation,request FROM jobs WHERE state='queued' ORDER BY seq LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let result = if let Some((id, generation, request)) = row {
            if generation != m.generation || request.len() > 65536 {
                return Err(Error::Stale);
            }
            let request: PreparedRequest = serde_json::from_str(&request)?;
            let attempt_id = Uuid::new_v4().to_string();
            tx.execute(
                "UPDATE jobs SET state='in_flight',attempt_id=? WHERE id=? AND state='queued'",
                params![attempt_id, id],
            )?;
            Some(Claim {
                id,
                attempt_id,
                generation: generation as u64,
                line: live_line,
                request,
            })
        } else {
            None
        };
        tx.commit()?;
        Ok(result)
    }
    /// Only the exact outstanding attempt can report a result. A late response
    /// after disable/restart remains Unknown rather than reviving an old job.
    pub fn finish(&mut self, binding: &Binding, claim: &Claim, state: State) -> Result<()> {
        if ![State::Confirmed, State::Failed, State::Unknown].contains(&state) {
            return Err(Error::Invalid);
        }
        if !self.fence(binding, claim.generation)?.enabled {
            return Err(Error::Disabled);
        }
        let count=self.conn.execute("UPDATE jobs SET state=?,request=CASE WHEN ?='unknown' THEN request ELSE NULL END WHERE id=? AND attempt_id=? AND generation=? AND line=? AND state='in_flight'",params![state.name(),state.name(),claim.id,claim.attempt_id,claim.generation as i64,claim.line.0 as i64])?;
        if count != 1 {
            return Err(Error::Stale);
        }
        Ok(())
    }
    /// No content, credentials, URLs or bearer tickets in receipts.
    pub fn receipts(&self, after: i64, limit: usize) -> Result<Vec<Receipt>> {
        if after < 0 || !(1..=32).contains(&limit) {
            return Err(Error::Invalid);
        }
        let mut statement=self.conn.prepare("SELECT seq,id,message_id,target_id,rule_id,generation,state FROM jobs WHERE seq>? ORDER BY seq LIMIT ?")?;
        let rows = statement.query_map(params![after, limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (seq, id, message_id, target_id, rule_id, generation, state) = row?;
            let generation = u64::try_from(generation)
                .ok()
                .filter(|n| *n > 0)
                .ok_or(Error::Storage)?;
            Ok(Receipt {
                seq: seq.to_string(),
                id,
                message_id,
                target_id,
                rule_id,
                generation,
                state: State::parse(&state)?,
            })
        })
        .collect()
    }
    /// Explicit bounded housekeeping, to be exposed only behind recent Owner
    /// approval. Unknown/in-flight/queued receipts and payloads are never pruned.
    pub fn prune_terminal(
        &mut self,
        binding: &Binding,
        generation: u64,
        through: i64,
    ) -> Result<usize> {
        self.fence(binding, generation)?;
        if through < 0 {
            return Err(Error::Invalid);
        }
        Ok(self.conn.execute(
            "DELETE FROM jobs WHERE seq<=? AND state IN('confirmed','failed','cancelled')",
            [through],
        )?)
    }
}

#[cfg(test)]
mod tests;
