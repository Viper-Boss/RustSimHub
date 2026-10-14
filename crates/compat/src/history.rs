// SPDX-License-Identifier: AGPL-3.0-only
//! Offline, private history archive. Never creates principals, live lines or send operations.
use fs2::FileExt;
use rusqlite::{params, types::ValueRef, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
};
mod reader;
pub use reader::{HistoryService, HistorySource};

const APP_ID: i64 = 0x5253484d;
const MAX_SNAPSHOT: u64 = 256 * 1024 * 1024;
const MAX_ROWS: usize = 100_000;
const MAX_ROW: usize = 128 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Io,
    Forbidden,
    Busy,
    SnapshotRequired,
    Schema,
    Corrupt,
    Bound,
    Conflict,
    Database,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "history: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Io
    }
}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::Database
    }
}

#[derive(Default, Serialize)]
pub struct Report {
    pub rows_by_entity: BTreeMap<String, usize>,
    pub inserted: usize,
    pub unchanged: usize,
    pub history_only: bool,
    pub hardware_operations: u8,
    pub credentials_imported: bool,
}
struct Contract {
    table: &'static str,
    keys: &'static [&'static str],
    columns: &'static [&'static str],
}
// Explicit projection: new columns never silently broaden what can be imported.
// Schema facts only; source implementation and authentication tables are not copied.
const CONTRACTS: &[Contract] = &[
    Contract {
        table: "sms",
        keys: &["id"],
        columns: &[
            "id",
            "request_id",
            "line_id",
            "endpoint_line_id",
            "endpoint_message_id",
            "imsi",
            "iccid",
            "peer",
            "sender",
            "recipient",
            "content",
            "type",
            "status",
            "state",
            "delivery_status",
            "revision",
            "timestamp",
            "created_at",
            "deleted_at",
        ],
    },
    Contract {
        table: "sms_contacts",
        keys: &["line_id", "peer"],
        columns: &[
            "line_id",
            "imsi",
            "iccid",
            "peer",
            "last_sms_id",
            "last_timestamp",
            "last_content",
            "last_type",
            "unread_count",
            "marked_unread",
            "is_favorite",
            "created_at",
            "updated_at",
        ],
    },
    Contract {
        table: "contacts",
        keys: &["id"],
        columns: &[
            "id",
            "owner_user_id",
            "display_name",
            "avatar",
            "notes",
            "preferred_line_id",
            "is_favorite",
            "revision",
            "created_at",
            "updated_at",
        ],
    },
    Contract {
        table: "contact_phones",
        keys: &["id"],
        columns: &[
            "id",
            "contact_id",
            "label",
            "original_number",
            "canonical_e164",
            "region",
            "is_primary",
        ],
    },
    Contract {
        table: "call_history",
        keys: &["id"],
        columns: &[
            "id",
            "request_id",
            "line_id",
            "endpoint_line_id",
            "local_phone",
            "line_imsi",
            "line_iccid",
            "direction",
            "remote_number",
            "endpoint_id",
            "endpoint_call_id",
            "phase",
            "revision",
            "created_at",
            "updated_at",
            "active_at",
            "ended_at",
            "read_at",
            "is_favorite",
            "end_reason",
            "failure_code",
        ],
    },
    Contract {
        table: "modemdeck_user_message_thread_state",
        keys: &["user_id", "line_id", "peer"],
        columns: &[
            "user_id",
            "line_id",
            "peer",
            "last_read_sms_id",
            "marked_unread",
            "is_favorite",
            "updated_at",
        ],
    },
    Contract {
        table: "modemdeck_user_call_state",
        keys: &["user_id", "call_id"],
        columns: &["user_id", "call_id", "is_read", "is_favorite", "updated_at"],
    },
    Contract {
        table: "modemdeck_lines",
        keys: &["line_id"],
        columns: &[
            "line_id",
            "phone_number",
            "home_country_iso",
            "line_label",
            "line_color",
            "created_at",
            "updated_at",
        ],
    },
    Contract {
        table: "modemdeck_legacy_endpoint_lines",
        keys: &["endpoint_id"],
        columns: &["endpoint_id", "line_id", "created_at"],
    },
    Contract {
        table: "modemdeck_user_lines",
        keys: &["user_id", "line_id"],
        columns: &["user_id", "line_id", "assigned_at"],
    },
];
fn report() -> Report {
    Report {
        history_only: true,
        ..Report::default()
    }
}
fn namespace(source: &str) -> Result<()> {
    if source.len() != 64
        || !source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(crate) fn directory(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !path.is_absolute()
        || !m.is_dir()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o077 != 0
        || fs::canonicalize(path)? != path
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(crate) fn private_file(path: &Path, write: bool, create: bool) -> Result<File> {
    directory(path.parent().ok_or(Error::Forbidden)?)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(write)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .mode(0o600);
    if create {
        options.create_new(true);
    }
    let file = options.open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o077 != 0
        || m.nlink() != 1
    {
        return Err(Error::Forbidden);
    }
    Ok(file)
}
fn hash(file: &mut File) -> Result<[u8; 32]> {
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 32768];
    let mut total = 0;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_SNAPSHOT {
            return Err(Error::Bound);
        }
        digest.update(&buffer[..n]);
    }
    Ok(digest.finalize().into())
}
fn db_read(file: &File) -> Result<Connection> {
    let db = Connection::open_with_flags(
        format!(
            "file:/proc/self/fd/{}?mode=ro&immutable=1",
            file.as_raw_fd()
        ),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON;")?;
    if db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err(Error::Corrupt);
    }
    Ok(db)
}
struct Snapshot {
    file: File,
    db: Connection,
    digest: [u8; 32],
}
impl Snapshot {
    fn open(path: &Path) -> Result<Self> {
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sibling = path.as_os_str().to_os_string();
            sibling.push(suffix);
            match fs::symlink_metadata(sibling) {
                Ok(_) => return Err(Error::SnapshotRequired),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::Io),
            }
        }
        let mut file = private_file(path, false, false)?;
        FileExt::try_lock_shared(&file).map_err(|_| Error::Busy)?;
        if file.metadata()?.len() < 512 {
            return Err(Error::Corrupt);
        }
        let digest = hash(&mut file)?;
        let db = db_read(&file)?;
        for c in CONTRACTS {
            let table = db
                .query_row(
                    "SELECT type,rootpage FROM sqlite_schema WHERE name=?1",
                    [c.table],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                )
                .optional()?
                .ok_or(Error::Schema)?;
            if table.0 != "table" || table.1 <= 0 {
                return Err(Error::Schema);
            }
            let sql = format!("SELECT {} FROM {} LIMIT 0", c.columns.join(","), c.table);
            db.prepare(&sql).map_err(|_| Error::Schema)?;
        }
        Ok(Self { file, db, digest })
    }
    fn unchanged(&mut self) -> Result<()> {
        if hash(&mut self.file)? != self.digest {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}
struct Record {
    key: String,
    payload: String,
    digest: [u8; 32],
}
fn visit(
    snapshot: &Snapshot,
    mut accept: impl FnMut(&Contract, Record) -> Result<()>,
) -> Result<Report> {
    let mut report = report();
    let mut total = 0;
    for c in CONTRACTS {
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {}",
            c.columns.join(","),
            c.table,
            c.keys.join(",")
        );
        let mut statement = snapshot.db.prepare(&sql)?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            total += 1;
            count += 1;
            if total > MAX_ROWS {
                return Err(Error::Bound);
            }
            let mut object = Map::new();
            for (i, name) in c.columns.iter().enumerate() {
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => Value::from(v),
                    ValueRef::Text(v) => {
                        if v.len() > MAX_ROW {
                            return Err(Error::Bound);
                        }
                        Value::String(
                            std::str::from_utf8(v)
                                .map_err(|_| Error::Corrupt)?
                                .to_owned(),
                        )
                    }
                    _ => return Err(Error::Schema),
                };
                object.insert((*name).into(), value);
            }
            let mut key = Vec::new();
            for name in c.keys {
                match object.get(*name) {
                    Some(Value::String(v)) if !v.is_empty() && v.len() <= 512 => {
                        key.push(Value::String(v.clone()))
                    }
                    Some(Value::Number(v)) if v.as_i64().is_some_and(|n| n > 0) => {
                        key.push(Value::Number(v.clone()))
                    }
                    _ => return Err(Error::Schema),
                }
            }
            let payload = serde_json::to_string(&object).map_err(|_| Error::Corrupt)?;
            if payload.len() > MAX_ROW {
                return Err(Error::Bound);
            }
            let digest = Sha256::digest(payload.as_bytes()).into();
            accept(
                c,
                Record {
                    key: serde_json::to_string(&key).map_err(|_| Error::Corrupt)?,
                    payload,
                    digest,
                },
            )?;
        }
        report.rows_by_entity.insert(c.table.into(), count);
    }
    Ok(report)
}
pub fn validate(snapshot: &Path, source: &str) -> Result<Report> {
    namespace(source)?;
    let mut snapshot = Snapshot::open(snapshot)?;
    let report = visit(&snapshot, |_, _| Ok(()))?;
    snapshot.unchanged()?;
    Ok(report)
}
fn archive_lock(directory_path: &Path, write: bool) -> Result<File> {
    directory(directory_path)?;
    let path = directory_path.join(".history.lock");
    let lock = private_file(&path, write, write && !path.try_exists()?)?;
    if write {
        FileExt::try_lock_exclusive(&lock)
    } else {
        FileExt::try_lock_shared(&lock)
    }
    .map_err(|_| Error::Busy)?;
    Ok(lock)
}
fn archive_check(db: &Connection) -> Result<()> {
    if db.query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))? != APP_ID
        || db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? != 1
    {
        return Err(Error::Schema);
    }
    if db.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err(Error::Corrupt);
    }
    Ok(())
}
pub fn import(input: &Path, source: &str, destination: &Path) -> Result<Report> {
    namespace(source)?;
    let mut snapshot = Snapshot::open(input)?;
    // Validate all data before creating archive files; malformed snapshots leave no partial archive.
    visit(&snapshot, |_, _| Ok(()))?;
    let _lock = archive_lock(destination, true)?;
    let path = destination.join("history.sqlite");
    if fs::canonicalize(input)? == path {
        return Err(Error::Forbidden);
    }
    let new = !path.try_exists()?;
    let file = private_file(&path, true, new)?;
    if file.metadata()?.len() > MAX_ARCHIVE_BYTES {
        return Err(Error::Bound);
    }
    let mut db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    if !new {
        archive_check(&db)?;
    }
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON; PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
    let page_size = db.query_row("PRAGMA page_size", [], |r| r.get::<_, i64>(0))?;
    if !(512..=65536).contains(&page_size) {
        return Err(Error::Corrupt);
    }
    db.execute_batch(&format!(
        "PRAGMA max_page_count={};",
        MAX_ARCHIVE_BYTES / page_size as u64
    ))?;
    if new {
        db.execute_batch("BEGIN IMMEDIATE; CREATE TABLE history_records(seq INTEGER PRIMARY KEY,source TEXT NOT NULL,entity TEXT NOT NULL,legacy_key TEXT NOT NULL,payload TEXT NOT NULL,digest BLOB NOT NULL CHECK(length(digest)=32),UNIQUE(source,entity,legacy_key)); CREATE TABLE history_batches(source TEXT NOT NULL,snapshot_digest BLOB NOT NULL CHECK(length(snapshot_digest)=32),PRIMARY KEY(source,snapshot_digest)); PRAGMA application_id=1381189709; PRAGMA user_version=1; COMMIT;")?;
    }
    archive_check(&db)?;
    let tx = db.transaction()?;
    let mut inserted = 0;
    let mut unchanged = 0;
    let mut report = visit(&snapshot, |c, record| {
        let existing=tx.query_row("SELECT digest,payload FROM history_records WHERE source=?1 AND entity=?2 AND legacy_key=?3",params![source,c.table,record.key],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,String>(1)?))).optional()?;
        if let Some((digest, payload)) = existing {
            if digest != record.digest || payload != record.payload {
                return Err(Error::Conflict);
            }
            unchanged += 1;
        } else {
            tx.execute("INSERT INTO history_records(source,entity,legacy_key,payload,digest) VALUES(?1,?2,?3,?4,?5)",params![source,c.table,record.key,record.payload,record.digest])?;
            inserted += 1;
        }
        Ok(())
    })?;
    snapshot.unchanged()?;
    tx.execute(
        "INSERT OR IGNORE INTO history_batches(source,snapshot_digest) VALUES(?1,?2)",
        params![source, snapshot.digest],
    )?;
    tx.commit()?;
    db.execute_batch("PRAGMA optimize;")?;
    file.sync_all()?;
    File::open(destination)?.sync_all()?;
    report.inserted = inserted;
    report.unchanged = unchanged;
    Ok(report)
}
pub fn inspect(directory_path: &Path) -> Result<Report> {
    let _lock = archive_lock(directory_path, false)?;
    let file = private_file(&directory_path.join("history.sqlite"), false, false)?;
    let db = db_read(&file)?;
    archive_check(&db)?;
    let mut report = report();
    let mut statement =
        db.prepare("SELECT entity,COUNT(*) FROM history_records GROUP BY entity ORDER BY entity")?;
    let rows = statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (entity, count) = row?;
        report
            .rows_by_entity
            .insert(entity, usize::try_from(count).map_err(|_| Error::Corrupt)?);
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
