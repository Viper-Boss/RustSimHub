// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags};
use std::{
    os::unix::ffi::OsStrExt,
    time::{Duration, Instant},
};

fn immutable_uri(path: &Path) -> String {
    let mut uri = String::from("file:");
    for byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            uri.push(*byte as char);
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri.push_str("?immutable=1");
    uri
}

#[derive(Clone, Copy, Serialize)]
pub(super) struct Version {
    pub application_id: i64,
    pub format: i64,
}

pub(super) struct Database {
    pub version: Version,
    // Close SQLite before releasing the cooperative writer fence.
    _connection: Connection,
    _writer: File,
    _database: File,
    observations: Vec<Observation>,
}
impl Database {
    pub fn observe(
        directory: &Path,
        name: &str,
        uid: u32,
        wal_header: bool,
        limit: u64,
    ) -> Result<Self> {
        let observed = Observation::capture(directory)?;
        let dir = observed.stamp.as_ref().ok_or(Error::Denied)?;
        if dir.mode & 0o170000 != 0o040000 || dir.mode & 0o7777 != 0o700 || dir.uid != uid {
            return Err(Error::Denied);
        }
        let mut observations = vec![observed];
        let lock_path = directory.join(".writer.lock");
        let writer = private_file(&lock_path, uid, 0o600, 0)?;
        writer.try_lock_exclusive().map_err(|_| Error::Busy)?;
        let observed_lock = Observation::capture(&lock_path)?;
        if observed_lock.stamp.as_ref() != Some(&denied_io(writer.metadata())?.into()) {
            return Err(Error::Busy);
        }
        observations.push(observed_lock);
        for suffix in ["-journal", "-wal", "-shm"] {
            absent(
                &directory.join(format!("{name}{suffix}")),
                &mut observations,
            )?;
        }
        let path = directory.join(name);
        let mut database = private_file(&path, uid, 0o600, limit)?;
        let stamp = denied_io(database.metadata())?;
        if stamp.len() < 512 {
            return Err(Error::Integrity);
        }
        let observed_file = Observation::capture(&path)?;
        if observed_file.stamp.as_ref() != Some(&stamp.into()) {
            return Err(Error::Busy);
        }
        observations.push(observed_file);
        let mut header = [0u8; 100];
        denied_io(database.read_exact(&mut header))?;
        if &header[..16] != b"SQLite format 3\0"
            || !(header[18..20] == [1, 1] || (wal_header && header[18..20] == [2, 2]))
        {
            return Err(Error::Integrity);
        }
        // immutable=1 is valid ONLY with this writer fence and no auxiliary
        // files. It prevents SQLite creating WAL/SHM for a clean WAL-header DB.
        // The production pathname is fixed. Encode every special byte so even
        // private fixture roots cannot inject URI parameters. Revalidate all
        // directory/file identities after SQLite opens it under the fence.
        let uri = immutable_uri(&path);
        let connection = Connection::open_with_flags(
            uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(|_| Error::Integrity)?;
        connection
            .busy_timeout(Duration::ZERO)
            .map_err(|_| Error::Io)?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut operations = 0u64;
        connection
            .progress_handler(
                1000,
                Some(move || {
                    operations += 1000;
                    operations >= 10_000_000 || Instant::now() >= deadline
                }),
            )
            .map_err(|_| Error::Io)?;
        connection
            .execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; BEGIN;")
            .map_err(|_| Error::Integrity)?;
        let application_id = connection
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(|_| Error::Integrity)?;
        let format = connection
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|_| Error::Integrity)?;
        let check: String = connection
            .query_row("PRAGMA quick_check(1)", [], |r| r.get(0))
            .map_err(|_| Error::Integrity)?;
        if check != "ok" {
            return Err(Error::Integrity);
        }
        let result = Self {
            version: Version {
                application_id,
                format,
            },
            _connection: connection,
            _writer: writer,
            _database: database,
            observations,
        };
        result.revalidate()?;
        Ok(result)
    }
    pub fn revalidate(&self) -> Result<()> {
        for observed in &self.observations {
            observed.revalidate()?;
        }
        Ok(())
    }
}
