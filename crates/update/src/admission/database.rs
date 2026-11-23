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
    _connection: Option<Connection>,
    _writer: Option<File>,
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
            _connection: Some(connection),
            _writer: Some(writer),
            _database: database,
            observations,
        };
        result.revalidate()?;
        Ok(result)
    }
    pub(super) fn check_web_update_owner(
        &self,
        binding: &rustsimhub_wire::UpdateUploadBinding,
    ) -> Result<()> {
        self.revalidate()?;
        let connection = self._connection.as_ref().ok_or(Error::Busy)?;
        let (realm, key, generation, scopes): (String, Vec<u8>, i64, u16) = connection
            .query_row(
                "SELECT meta.db_epoch, p.public_key, p.generation, p.scopes                  FROM meta CROSS JOIN principals p                  WHERE meta.singleton=1 AND p.kind='owner' AND p.active=1                  AND (SELECT count(*) FROM principals WHERE kind='owner' AND active=1)=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|_| Error::Denied)?;
        if realm != binding.realm
            || key != binding.owner_key
            || u64::try_from(generation).ok() != Some(binding.generation)
            || scopes & rustsimhub_wire::UPDATE_MANAGE_SCOPE == 0
        {
            return Err(Error::Denied);
        }
        Ok(())
    }

    pub(super) fn web_update_owner_proof(
        &self,
        binding: &rustsimhub_wire::UpdateUploadBinding,
    ) -> Result<String> {
        self.check_web_update_owner(binding)?;
        let connection = self._connection.as_ref().ok_or(Error::Busy)?;
        let device_key: Vec<u8> = connection
            .query_row(
                "SELECT public_key FROM device_identity WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| Error::Denied)?;
        let device_key: [u8; 32] = device_key.try_into().map_err(|_| Error::Integrity)?;
        let proof = serde_json::to_vec(&(
            device_key,
            binding.owner_key,
            binding.generation,
            &binding.realm,
        ))
        .map_err(|_| Error::Invalid)?;
        self.revalidate()?;
        Ok(crate::manifest_hash(&proof))
    }

    pub fn revalidate(&self) -> Result<()> {
        if self._writer.is_none() || self._connection.is_none() {
            return Err(Error::Busy);
        }
        self.revalidate_source()
    }
    pub(super) fn revalidate_source(&self) -> Result<()> {
        for observed in &self.observations {
            observed.revalidate()?;
        }
        Ok(())
    }
}

#[cfg(feature = "email-production-candidate")]
impl Database {
    pub(super) fn email_binding(&self) -> Result<rustsimhub_forwarder::Binding> {
        self.revalidate()?;
        let raw: String = self
            ._connection
            .as_ref()
            .ok_or(Error::Busy)?
            .query_row("SELECT binding FROM meta WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .map_err(|_| Error::Integrity)?;
        if raw.len() > 4096 {
            return Err(Error::Integrity);
        }
        let binding: rustsimhub_forwarder::Binding =
            serde_json::from_str(&raw).map_err(|_| Error::Integrity)?;
        binding.validate().map_err(|_| Error::Integrity)?;
        Ok(binding)
    }
    /// Same persisted pin/realm/current Owner and scopes as the original Auth
    /// fence. Reads only the already locked main DB; never opens its writer or
    /// private identity seed, or trusts an Owner supplied in maintenance JSON.
    pub(super) fn check_email_binding(
        &self,
        binding: &rustsimhub_forwarder::Binding,
    ) -> Result<()> {
        use rustsimhub_policy::{allows, Scope, ALL_SCOPES};
        self.revalidate()?;
        binding.validate().map_err(|_| Error::Integrity)?;
        let realm: String = self
            ._connection
            .as_ref()
            .ok_or(Error::Busy)?
            .query_row("SELECT db_epoch FROM meta WHERE singleton=1", [], |r| {
                r.get(0)
            })
            .map_err(|_| Error::Integrity)?;
        let pin: Vec<u8> = self
            ._connection
            .as_ref()
            .ok_or(Error::Busy)?
            .query_row(
                "SELECT public_key FROM device_identity WHERE singleton=1",
                [],
                |r| r.get(0),
            )
            .map_err(|_| Error::Integrity)?;
        let (kind, generation, active, scopes): (String, i64, bool, u16) = self
            ._connection
            .as_ref()
            .ok_or(Error::Busy)?
            .query_row(
                "SELECT kind,generation,active,scopes FROM principals WHERE public_key=?",
                [binding.owner_key.as_slice()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|_| Error::Integrity)?;
        let owners: i64 = self
            ._connection
            .as_ref()
            .ok_or(Error::Busy)?
            .query_row(
                "SELECT count(*) FROM principals WHERE kind='owner'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| Error::Integrity)?;
        if realm != binding.realm
            || pin != binding.device_key
            || kind != "owner"
            || owners != 1
            || !active
            || u64::try_from(generation).ok() != Some(binding.owner_generation)
            || scopes & !ALL_SCOPES != 0
            || !allows(scopes, Scope::SystemManage)
            || !allows(scopes, Scope::SmsRead)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}

impl Database {
    /// Release ONLY this database's reader/writer into the fixed original
    /// migration actor. The original database fd and observations stay held.
    pub(super) fn backup_source(&self) -> Result<File> {
        use std::io::{Seek, SeekFrom};
        self.revalidate()?;
        let mut source = self._database.try_clone().map_err(|_| Error::Io)?;
        source.seek(SeekFrom::Start(0)).map_err(|_| Error::Io)?;
        Ok(source)
    }
    pub(super) fn pause_writer(&mut self) -> Result<()> {
        self.revalidate()?;
        self._connection.take();
        self._writer.take();
        Ok(())
    }
    pub(super) fn resume_recovery(&mut self, directory: &Path, uid: u32) -> Result<()> {
        if self._writer.is_some() || self._connection.is_some() {
            return Err(Error::Busy);
        }
        let original = self._database.metadata().map_err(|_| Error::Io)?;
        let current =
            fs::symlink_metadata(directory.join("store.sqlite3")).map_err(|_| Error::Io)?;
        if original.dev() != current.dev() || original.ino() != current.ino() {
            return Err(Error::Integrity);
        }
        let resumed = Self::observe(directory, "store.sqlite3", uid, true, 512 * 1024 * 1024)?;
        if resumed.version.application_id != 1381189698 || resumed.version.format != 9 {
            return Err(Error::Integrity);
        }
        *self = resumed;
        Ok(())
    }
}

impl Database {
    pub(super) fn runtime_identity(&self) -> Result<()> {
        if self._writer.is_some() || self._connection.is_some() {
            return Err(Error::Busy);
        }
        let directory = self.observations.first().ok_or(Error::Integrity)?;
        let now = Observation::capture(&directory.path)?;
        let before = directory.stamp.as_ref().ok_or(Error::Integrity)?;
        let after = now.stamp.as_ref().ok_or(Error::Integrity)?;
        if before.dev != after.dev
            || before.ino != after.ino
            || before.uid != after.uid
            || before.gid != after.gid
            || before.mode != after.mode
        {
            return Err(Error::Integrity);
        }
        // Only the original Core/Store writer may now change database bytes.
        // Keep both named/held identities; the later live SQL lease validates
        // schema/Auth/generation, not a cached maintenance report.
        let data = self.observations.last().ok_or(Error::Integrity)?;
        let named = fs::symlink_metadata(&data.path).map_err(|_| Error::Io)?;
        let held = self._database.metadata().map_err(|_| Error::Io)?;
        let before = data.stamp.as_ref().ok_or(Error::Integrity)?;
        if before.dev != named.dev()
            || before.ino != named.ino()
            || named.dev() != held.dev()
            || named.ino() != held.ino()
            || before.uid != named.uid()
            || before.gid != named.gid()
            || before.mode != named.mode()
            || named.nlink() != 1
        {
            return Err(Error::Integrity);
        }
        // The cooperative lock inode itself remains pinned.
        self.observations
            .get(1)
            .ok_or(Error::Integrity)?
            .revalidate()
    }
}
