// SPDX-License-Identifier: AGPL-3.0-only
//! Offline version inspection, deliberately independent from Queue::open:
//! no initialization, migrations, binding adoption or in-flight recovery.
use crate::{
    assess_outbox_compatibility, runtime_compatibility, Error, OutboxCompatibilityAssessment,
    OutboxVersion, Result,
};
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::{Duration, Instant},
};

const SIDE_CARS: [&str; 3] = [
    "outbox.sqlite3-journal",
    "outbox.sqlite3-wal",
    "outbox.sqlite3-shm",
];

/// Keep this guard until the report has been consumed. It fences the actual
/// Queue writer; dropping it makes the observation historical, not admission.
pub struct OutboxInspection {
    version: OutboxVersion,
    // Drop SQLite (and its read transaction) before releasing the writer lock.
    _connection: Connection,
    _lock: File,
    _database: File,
}

impl OutboxInspection {
    #[cfg(feature = "smtp-production-candidate")]
    pub(crate) fn finish_read(&self) -> Result<()> {
        self._connection.execute_batch("ROLLBACK;")?;
        Ok(())
    }

    pub fn version(&self) -> OutboxVersion {
        self.version
    }

    pub fn assessment(&self) -> Result<OutboxCompatibilityAssessment> {
        assess_outbox_compatibility(&runtime_compatibility(), self.version)
    }
}

fn owned_file(path: &Path, limit: u64) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.len() > limit
    {
        return Err(Error::Forbidden);
    }
    Ok(file)
}

fn no_sidecars(directory: &Path) -> Result<()> {
    for name in SIDE_CARS {
        match fs::symlink_metadata(directory.join(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // Even an empty sidecar or dangling link requires maintenance.
            _ => return Err(Error::Conflict),
        }
    }
    Ok(())
}

fn query_budget(connection: &Connection, budget: Duration, maximum_ops: u64) -> Result<()> {
    let deadline = Instant::now() + budget;
    let mut operations = 0u64;
    connection.progress_handler(
        1000,
        Some(move || {
            operations += 1000;
            operations >= maximum_ops || Instant::now() >= deadline
        }),
    )?;
    Ok(())
}

/// Inspect only an existing, quiescent, private outbox owned by the effective
/// user. Missing data/lock never creates files. A live writer, any sidecar,
/// non-DELETE database or invalid SQLite fails instead of recovering it.
/// No message, destination, secret or binding is returned. This checks SQLite
/// consistency and version facts, not business-schema or Owner validity.
pub fn inspect_outbox(directory: &Path) -> Result<OutboxInspection> {
    if !directory.is_absolute() {
        return Err(Error::Forbidden);
    }
    let dm = fs::symlink_metadata(directory)?;
    if !dm.is_dir()
        || dm.uid() != rustix::process::geteuid().as_raw()
        || dm.mode() & 0o7777 != 0o700
    {
        return Err(Error::Forbidden);
    }
    let lock = owned_file(&directory.join(".writer.lock"), 0)?;
    lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
    no_sidecars(directory)?;
    let path = directory.join("outbox.sqlite3");
    let mut database = owned_file(&path, 32 * 1024 * 1024)?;
    let before = database.metadata()?;
    if before.len() < 512 {
        return Err(Error::Storage);
    }
    // SQLite READ_ONLY can still create WAL/SHM files when the directory is
    // writable. Reject WAL header mode before giving SQLite this pathname.
    let mut header = [0u8; 100];
    database.read_exact(&mut header)?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(Error::Storage);
    }
    if header[18..20] != [1, 1] {
        return Err(Error::Conflict);
    }
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    connection.busy_timeout(Duration::ZERO)?;
    // Bound SQLite VM work on untrusted/corrupt files without another thread
    // or changing database PRAGMAs. The enclosing maintenance caller must also
    // enforce its wall-clock process budget (filesystem IO is outside the VM).
    query_budget(&connection, Duration::from_secs(3), 10_000_000)?;
    connection.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; BEGIN;")?;
    let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    if mode != "delete" {
        return Err(Error::Conflict);
    }
    let application_id = connection.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let format = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let check: String = connection.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(Error::Storage);
    }
    no_sidecars(directory)?;
    let after = fs::symlink_metadata(&path)?;
    let current_dir = fs::symlink_metadata(directory)?;
    if (
        before.dev(),
        before.ino(),
        before.len(),
        before.mtime(),
        before.mtime_nsec(),
    ) != (
        after.dev(),
        after.ino(),
        after.len(),
        after.mtime(),
        after.mtime_nsec(),
    ) || (dm.dev(), dm.ino()) != (current_dir.dev(), current_dir.ino())
    {
        return Err(Error::Conflict);
    }
    Ok(OutboxInspection {
        version: OutboxVersion {
            application_id,
            format,
        },
        _connection: connection,
        _lock: lock,
        _database: database,
    })
}

#[cfg(test)]
mod tests;
