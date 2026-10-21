// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! One blocking actor owns SQLite. Call from a blocking worker, never Tokio's event loop.
use fs2::FileExt;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use rustsimhub_domain::{LineEpoch, SmsState};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};
use uuid::Uuid;

const APP_ID: i64 = 1381189698;
const SCHEMA: i64 = 5;
mod auth;
mod calls;
pub use calls::{
    CallEvent, CallEvents, CallIntent, CallObservation, CallOperation, CallPage, CallRequest,
    CallSummary,
};
mod inbox;
mod indicator;
pub use indicator::IndicatorSnapshot;
mod outgoing;
pub use inbox::{
    InboxCursor, InboxEvent, InboxEvents, InboxPage, IncomingSms, LineObservation, LineStatus,
    LineView, SavedSource, SmsDetail, SmsSummary, SourceSnapshot,
};
pub use outgoing::{SendAttempt, SendContext};
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Io,
    AlreadyOpen,
    Busy,
    Stopped,
    Database,
    Corrupt,
    Full,
    ForeignDatabase,
    NewerSchema,
    InvalidInput,
    Conflict,
    StaleLine,
    NotFound,
    Forbidden,
    InvalidTransition,
    ResyncRequired,
    BackupExists,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "storage: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(error, _) = value {
            use rusqlite::ErrorCode::*;
            return match error.code {
                DatabaseBusy | DatabaseLocked => Self::Busy,
                DiskFull => Self::Full,
                DatabaseCorrupt | NotADatabase => Self::Corrupt,
                _ => Self::Database,
            };
        }
        Self::Database
    }
}

// No Debug on request/permit: phone numbers and text must not enter diagnostics.
#[derive(Clone, Serialize)]
pub struct SmsRequest {
    pub client_id: String,
    pub idempotency_key: String,
    pub line: LineEpoch,
    pub recipient: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Operation {
    pub id: String,
    pub line: LineEpoch,
    pub state: SmsState,
}
pub struct Submission {
    pub operation: Operation,
    pub recipient: String,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cursor {
    pub db_epoch: String,
    pub seq: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    pub seq: String,
    pub operation_id: Option<String>,
    pub line: LineEpoch,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct EventPage {
    pub events: Vec<Event>,
    pub cursor: Cursor,
}

type Job = Box<dyn FnOnce(&mut Database) + Send>;
enum Message {
    Run(Job),
    Stop,
}
#[derive(Clone)]
pub struct Handle {
    sender: mpsc::SyncSender<Message>,
}
pub struct Store {
    handle: Handle,
    worker: Option<thread::JoinHandle<()>>,
}

impl Store {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        Self::start(directory.as_ref().to_owned(), None)
    }
    /// Offline restore to a fresh directory only; replaces the event epoch.
    pub fn restore(
        backup_directory: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::start(
            destination.as_ref().to_owned(),
            Some(backup_directory.as_ref().to_owned()),
        )
    }
    fn start(directory: PathBuf, restore: Option<PathBuf>) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Message>(32);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("rustsimhub-sqlite".into())
            .spawn(move || {
                let mut database = match Database::open(&directory, restore.as_deref()) {
                    Ok(db) => {
                        let _ = ready_tx.send(Ok(()));
                        db
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                while let Ok(Message::Run(job)) = receiver.recv() {
                    job(&mut database);
                }
            })?;
        if let Err(error) = ready_rx.recv().map_err(|_| Error::Stopped)? {
            let _ = worker.join();
            return Err(error);
        }
        Ok(Self {
            handle: Handle { sender },
            worker: Some(worker),
        })
    }
    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.handle.sender.send(Message::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Handle {
    fn call<T: Send + 'static>(
        &self,
        job: impl FnOnce(&mut Database) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (sender, receiver) = mpsc::sync_channel(1);
        self.sender
            .try_send(Message::Run(Box::new(move |db| {
                let _ = sender.send(job(db));
            })))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => Error::Busy,
                mpsc::TrySendError::Disconnected(_) => Error::Stopped,
            })?;
        receiver.recv().map_err(|_| Error::Stopped)?
    }
    pub fn prepare_sms(&self, request: SmsRequest) -> Result<Operation> {
        self.call(move |db| db.prepare(request))
    }
    /// Returned only AFTER Submitting + event commit. Caller must already authenticate client.
    pub fn begin_submit(&self, id: &str, client: &str) -> Result<Submission> {
        let (id, client) = (id.to_owned(), client.to_owned());
        self.call(move |db| db.begin(&id, &client))
    }
    pub fn mark_submitted(&self, id: &str, client: &str) -> Result<Operation> {
        self.finish(id, client, "submitted")
    }
    pub fn mark_unknown(&self, id: &str, client: &str) -> Result<Operation> {
        self.finish(id, client, "unknown")
    }
    fn finish(&self, id: &str, client: &str, state: &'static str) -> Result<Operation> {
        let (id, client) = (id.to_owned(), client.to_owned());
        self.call(move |db| db.finish(&id, &client, state))
    }
    pub fn operation(&self, id: &str, client: &str) -> Result<Operation> {
        let (id, client) = (id.to_owned(), client.to_owned());
        self.call(move |db| operation(&db.connection, &id, &client))
    }
    pub fn change_line(&self, line: LineEpoch) -> Result<()> {
        self.call(move |db| db.change_line(line))
    }
    pub fn line_epoch(&self) -> Result<LineEpoch> {
        self.call(|db| {
            Ok(LineEpoch(db.connection.query_row(
                "SELECT line_epoch FROM meta WHERE singleton=1",
                [],
                |row| row.get::<_, i64>(0),
            )? as u64))
        })
    }
    pub fn cursor(&self) -> Result<Cursor> {
        self.call(|db| cursor(&db.connection))
    }
    pub fn events(&self, after: Cursor, limit: usize) -> Result<EventPage> {
        self.call(move |db| db.events(after, limit))
    }
    pub fn backup(&self, directory: impl AsRef<Path>) -> Result<()> {
        let directory = directory.as_ref().to_owned();
        self.call(move |db| db.backup(&directory))
    }
}

struct Database {
    connection: Connection,
    _lock: File,
}

fn private_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return Err(Error::Forbidden);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if meta.uid() != rustix::process::geteuid().as_raw()
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err(Error::Forbidden);
        }
    }
    Ok(())
}
fn private_file(path: &Path, new: bool) -> Result<File> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.is_file() {
            return Err(Error::Forbidden);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if meta.uid() != rustix::process::geteuid().as_raw()
                || meta.permissions().mode() & 0o077 != 0
            {
                return Err(Error::Forbidden);
            }
        }
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn reject_links(directory: &Path) -> Result<()> {
    for name in [
        "store.sqlite3",
        "store.sqlite3-wal",
        "store.sqlite3-shm",
        ".writer.lock",
        ".restore-pending",
    ] {
        if let Ok(meta) = fs::symlink_metadata(directory.join(name)) {
            if !meta.is_file() {
                return Err(Error::Forbidden);
            }
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o077 != 0 {
                    return Err(Error::Forbidden);
                }
            }
        }
    }
    Ok(())
}
fn checked_line(line: LineEpoch) -> Result<i64> {
    i64::try_from(line.0)
        .ok()
        .filter(|x| *x > 0)
        .ok_or(Error::InvalidInput)
}
fn parse_state(value: String) -> Result<SmsState> {
    match value.as_str() {
        "prepared" => Ok(SmsState::Prepared),
        "submitting" => Ok(SmsState::Submitting),
        "submitted" => Ok(SmsState::Submitted),
        "unknown" => Ok(SmsState::Unknown),
        "cancelled" => Ok(SmsState::Cancelled),
        _ => Err(Error::Corrupt),
    }
}
fn operation(connection: &Connection, id: &str, client: &str) -> Result<Operation> {
    let row: Option<(String, i64, String)> = connection
        .query_row(
            "SELECT client_id,line_epoch,state FROM sms_operations WHERE id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let (owner, line, state) = row.ok_or(Error::NotFound)?;
    if owner != client {
        return Err(Error::Forbidden);
    }
    Ok(Operation {
        id: id.into(),
        line: LineEpoch(line as u64),
        state: parse_state(state)?,
    })
}
fn event(tx: &Transaction, id: Option<&str>, line: i64, kind: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO events(operation_id,line_epoch,kind) VALUES(?,?,?)",
        params![id, line, kind],
    )?;
    Ok(())
}
fn cursor(connection: &Connection) -> Result<Cursor> {
    Ok(Cursor {
        db_epoch: connection.query_row(
            "SELECT db_epoch FROM meta WHERE singleton=1",
            [],
            |row| row.get(0),
        )?,
        seq: connection
            .query_row("SELECT COALESCE(MAX(seq),0) FROM events", [], |row| {
                row.get::<_, i64>(0)
            })?
            .to_string(),
    })
}
impl Database {
    fn open(directory: &Path, restore: Option<&Path>) -> Result<Self> {
        private_directory(directory)?;
        reject_links(directory)?;
        let lock = private_file(&directory.join(".writer.lock"), false)?;
        lock.try_lock_exclusive().map_err(|_| Error::AlreadyOpen)?;
        let path = directory.join("store.sqlite3");
        let restore_marker = directory.join(".restore-pending");
        if restore.is_none() && restore_marker.exists() && !path.exists() {
            return Err(Error::Corrupt);
        }
        if let Some(source) = restore {
            private_directory(source)?;
            if path.exists() {
                return Err(Error::BackupExists);
            }
            // Restrict source to completed backup; never copy a live WAL database.
            let source_path = source.join("backup.sqlite3");
            let meta = fs::symlink_metadata(&source_path)?;
            if !meta.is_file() {
                return Err(Error::Forbidden);
            }
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.uid() != rustix::process::geteuid().as_raw() || meta.mode() & 0o077 != 0 {
                    return Err(Error::Forbidden);
                }
            }
            let from = Connection::open_with_flags(&source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            private_file(&restore_marker, true)?.sync_all()?;
            #[cfg(unix)]
            File::open(directory)?.sync_all()?;
            private_file(&path, true)?;
            let mut to = Connection::open(&path)?;
            rusqlite::backup::Backup::new(&from, &mut to)?.run_to_completion(
                128,
                Duration::from_millis(1),
                None,
            )?;
        }
        private_file(&path, false)?;
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_secs(3))?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA {
            return Err(Error::NewerSchema);
        }
        if restore_marker.exists() && version == 0 {
            return Err(Error::Corrupt);
        }
        let app: i64 = connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let tables: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        if (version == 0 && (app != 0 || tables != 0)) || (version != 0 && app != APP_ID) {
            return Err(Error::ForeignDatabase);
        }
        let integrity: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(Error::Corrupt);
        }
        connection.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=256; PRAGMA journal_size_limit=4194304; PRAGMA max_page_count=8192;")?;
        let mode: String = connection.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        if mode != "wal" {
            return Err(Error::Database);
        }
        let mut db = Self {
            connection,
            _lock: lock,
        };
        if version == 0 {
            let tx = db.connection.transaction()?;
            tx.execute_batch(include_str!("schema.sql"))?;
            tx.execute(
                "INSERT INTO meta VALUES(1,?,1)",
                [Uuid::new_v4().to_string()],
            )?;
            tx.commit()?;
        }
        if version < 2 {
            let tx = db.connection.transaction()?;
            tx.execute_batch(include_str!("auth.sql"))?;
            tx.commit()?;
        }
        if version < 3 {
            let tx = db.connection.transaction()?;
            tx.execute_batch(include_str!("inbox.sql"))?;
            tx.commit()?;
        }
        if version < 4 {
            let tx = db.connection.transaction()?;
            tx.execute_batch(include_str!("outgoing.sql"))?;
            tx.commit()?;
        }
        if version < 5 {
            let tx = db.connection.transaction()?;
            tx.execute_batch(include_str!("calls.sql"))?;
            tx.commit()?;
        }

        let restoring = restore_marker.exists();
        db.recover(restoring)?;
        db.connection.execute(
            "UPDATE line_observation SET status='unavailable' WHERE singleton=1",
            [],
        )?;
        if restoring {
            fs::remove_file(&restore_marker)?;
            #[cfg(unix)]
            File::open(directory)?.sync_all()?;
        }
        Ok(db)
    }
    fn prepare(&mut self, request: SmsRequest) -> Result<Operation> {
        let line = checked_line(request.line)?;
        if request.client_id.is_empty()
            || request.client_id.len() > 128
            || request.idempotency_key.is_empty()
            || request.idempotency_key.len() > 128
            || request.text.is_empty()
            || request.text.len() > 4096
            || request.text.contains('\0')
            || !(3..=32).contains(&request.recipient.len())
            || !request
                .recipient
                .strip_prefix('+')
                .unwrap_or(&request.recipient)
                .bytes()
                .all(|x| x.is_ascii_digit())
        {
            return Err(Error::InvalidInput);
        }
        let fingerprint =
            Sha256::digest(serde_json::to_vec(&request).map_err(|_| Error::InvalidInput)?);
        let tx = self.connection.transaction()?;
        let existing: Option<(String, Vec<u8>)> = tx
            .query_row(
                "SELECT id,fingerprint FROM sms_operations WHERE client_id=? AND idempotency_key=?",
                params![request.client_id, request.idempotency_key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((id, prior)) = existing {
            if prior.as_slice() != fingerprint.as_slice() {
                return Err(Error::Conflict);
            }
            return operation(&tx, &id, &request.client_id);
        }
        let current: i64 =
            tx.query_row("SELECT line_epoch FROM meta WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        if line != current {
            return Err(Error::StaleLine);
        }
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO sms_operations VALUES(?,?,?,?,?,?,?,'prepared')",
            params![
                id,
                request.client_id,
                request.idempotency_key,
                line,
                fingerprint.as_slice(),
                request.recipient,
                request.text
            ],
        )?;
        event(&tx, Some(&id), line, "prepared")?;
        tx.commit()?;
        Ok(Operation {
            id,
            line: request.line,
            state: SmsState::Prepared,
        })
    }
    fn begin(&mut self, id: &str, client: &str) -> Result<Submission> {
        let tx = self.connection.transaction()?;
        let mut op = operation(&tx, id, client)?;
        let current: i64 =
            tx.query_row("SELECT line_epoch FROM meta WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        if op.line.0 != current as u64 {
            return Err(Error::StaleLine);
        }
        if op.state != SmsState::Prepared {
            return Err(Error::InvalidTransition);
        }
        let (recipient, text) = tx.query_row(
            "SELECT recipient,body FROM sms_operations WHERE id=?",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        tx.execute(
            "UPDATE sms_operations SET state='submitting' WHERE id=?",
            [id],
        )?;
        event(&tx, Some(id), current, "submitting")?;
        tx.commit()?;
        op.state = SmsState::Submitting;
        Ok(Submission {
            operation: op,
            recipient,
            text,
        })
    }
    fn finish(&mut self, id: &str, client: &str, state: &str) -> Result<Operation> {
        let tx = self.connection.transaction()?;
        let mut op = operation(&tx, id, client)?;
        if op.state != SmsState::Submitting {
            return Err(Error::InvalidTransition);
        }
        let current: i64 =
            tx.query_row("SELECT line_epoch FROM meta WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        if op.line.0 != current as u64 {
            return Err(Error::StaleLine);
        }
        tx.execute(
            "UPDATE sms_operations SET state=? WHERE id=?",
            params![state, id],
        )?;
        event(&tx, Some(id), current, state)?;
        tx.commit()?;
        op.state = parse_state(state.to_owned())?;
        Ok(op)
    }
    fn recover(&mut self, restoring: bool) -> Result<()> {
        let tx = self.connection.transaction()?;
        if restoring {
            tx.execute(
                "UPDATE meta SET db_epoch=? WHERE singleton=1",
                [Uuid::new_v4().to_string()],
            )?;
            // Old backups may precede a revocation. Preserve ownership, quarantine ALL grants.
            tx.execute("UPDATE principals SET active=0,generation=generation+1", [])?;
        }
        let pending: Vec<(String, i64)> = {
            let mut query = tx.prepare(
                "SELECT id,line_epoch FROM sms_operations WHERE state='submitting' OR (? AND state='prepared') ORDER BY id",
            )?;
            let rows = query.query_map([restoring], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for (id, line) in pending {
            tx.execute(
                "UPDATE sms_operations SET state='unknown' WHERE id=?",
                [&id],
            )?;
            event(&tx, Some(&id), line, "unknown")?;
        }
        tx.execute("UPDATE call_operations SET state='unknown' WHERE state='submitting' OR (? AND state='prepared')",[restoring])?;
        tx.execute("INSERT INTO call_events(call_id,line_epoch,kind) SELECT id,line_epoch,'unavailable' FROM calls WHERE availability='observed'",[])?;
        tx.execute(
            "UPDATE calls SET availability='reconciling' WHERE availability='observed'",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }
    fn change_line(&mut self, line: LineEpoch) -> Result<()> {
        let line = checked_line(line)?;
        let tx = self.connection.transaction()?;
        Self::change_line_in(&tx, line)?;
        tx.commit()?;
        Ok(())
    }
    fn change_line_in(tx: &Transaction<'_>, line: i64) -> Result<()> {
        let current: i64 =
            tx.query_row("SELECT line_epoch FROM meta WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        if line <= current {
            return Err(Error::StaleLine);
        }
        let pending: Vec<(String, i64, String)> = {
            let mut query=tx.prepare("SELECT id,line_epoch,state FROM sms_operations WHERE state IN ('prepared','submitting') ORDER BY id")?;
            let rows = query.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for (id, old, state) in pending {
            let next = if state == "prepared" {
                "cancelled"
            } else {
                "unknown"
            };
            tx.execute(
                "UPDATE sms_operations SET state=? WHERE id=?",
                params![next, id],
            )?;
            event(tx, Some(&id), old, next)?;
        }
        tx.execute("UPDATE call_operations SET state=CASE WHEN state='prepared' THEN 'cancelled' ELSE 'unknown' END WHERE state IN ('prepared','submitting')",[])?;
        tx.execute("INSERT INTO call_events(call_id,line_epoch,kind) SELECT id,line_epoch,'unavailable' FROM calls WHERE availability='observed'",[])?;
        tx.execute(
            "UPDATE calls SET availability='reconciling' WHERE availability='observed'",
            [],
        )?;
        tx.execute("UPDATE meta SET line_epoch=? WHERE singleton=1", [line])?;
        event(tx, None, line, "line_changed")?;
        Ok(())
    }
    fn events(&self, after: Cursor, limit: usize) -> Result<EventPage> {
        let latest = cursor(&self.connection)?;
        let seq = after.seq.parse::<i64>().map_err(|_| Error::InvalidInput)?;
        if !(1..=256).contains(&limit) || seq < 0 {
            return Err(Error::InvalidInput);
        }
        if after.db_epoch != latest.db_epoch
            || seq > latest.seq.parse::<i64>().map_err(|_| Error::Corrupt)?
        {
            return Err(Error::ResyncRequired);
        }
        let mut query = self.connection.prepare(
            "SELECT seq,operation_id,line_epoch,kind FROM events WHERE seq>? ORDER BY seq LIMIT ?",
        )?;
        let events: Vec<Event> = query
            .query_map(params![seq, limit as i64], |row| {
                Ok(Event {
                    seq: row.get::<_, i64>(0)?.to_string(),
                    operation_id: row.get(1)?,
                    line: LineEpoch(row.get::<_, i64>(2)? as u64),
                    kind: row.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        let next = events.last().map(|e| e.seq.clone()).unwrap_or(after.seq);
        Ok(EventPage {
            events,
            cursor: Cursor {
                db_epoch: latest.db_epoch,
                seq: next,
            },
        })
    }
    fn backup(&self, directory: &Path) -> Result<()> {
        private_directory(directory)?;
        let final_path = directory.join("backup.sqlite3");
        if final_path.exists() {
            return Err(Error::BackupExists);
        }
        let pending = directory.join("backup.pending");
        private_file(&pending, true)?;
        {
            let mut destination = Connection::open(&pending)?;
            rusqlite::backup::Backup::new(&self.connection, &mut destination)?.run_to_completion(
                128,
                Duration::from_millis(1),
                None,
            )?;
        }
        File::open(&pending)?.sync_all()?;
        fs::rename(&pending, &final_path)?;
        #[cfg(unix)]
        File::open(directory)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod outgoing_tests;

#[cfg(test)]
mod calls_tests;
