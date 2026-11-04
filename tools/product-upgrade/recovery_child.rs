// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed recovery maintenance child. Reuses Store's two existing migrations.
//! A private direct Root socket parent is necessary; JSON is never permission.
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsFd,
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            net::UnixStream,
        },
    },
    path::Path,
    time::Duration,
};
const DATA: &str = "/var/lib/rustsimhub/rustsimhub-v1/core/state";
const LIMIT: u64 = 32 * 1024 * 1024;
#[derive(Debug)]
pub enum Error {
    Denied,
    Invalid,
    Io,
    Busy,
    Unknown,
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Permit {
    pub format: u16,
    pub source_schema: u32,
    pub target_schema: u32,
    pub source_sha256: String,
    pub binding_sha256: String,
    pub request_id: String,
    pub deadline_ms: u64,
}
#[derive(Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
struct Journal {
    format: u16,
    request_id: String,
    source_schema: u32,
    source_sha256: String,
    binding_sha256: String,
    phase: Phase,
    after_sha256: Option<String>,
}
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Prepared,
    ExecutingGrants,
    CommittedGrants,
    ExecutingExecution,
    CommittedExecution,
}
fn digest(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn boot_ms() -> Result<u64> {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    Ok(t.tv_sec as u64 * 1000 + t.tv_nsec as u64 / 1_000_000)
}
impl Permit {
    fn live(&self) -> Result<()> {
        let now = boot_ms()?;
        if self.format != 1
            || !matches!(self.source_schema, 6 | 7)
            || self.target_schema != 9
            || !hex(&self.source_sha256)
            || !hex(&self.binding_sha256)
            || !hex(&self.request_id)
            || self.request_id == "0".repeat(64)
            || self.deadline_ms <= now
            || self.deadline_ms - now > 30_000
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|_| Error::Io)?;
    let m = f.metadata().map_err(|_| Error::Io)?;
    if !m.is_file()
        || m.uid() != expected_uid(path)
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.len() > limit
    {
        return Err(Error::Denied);
    }
    let mut b = Vec::new();
    (&mut f)
        .take(limit + 1)
        .read_to_end(&mut b)
        .map_err(|_| Error::Io)?;
    if b.len() as u64 > limit {
        return Err(Error::Invalid);
    }
    let n = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if m.ino() != n.ino()
        || m.dev() != n.dev()
        || m.ctime() != n.ctime()
        || m.ctime_nsec() != n.ctime_nsec()
        || m.len() != n.len()
    {
        return Err(Error::Busy);
    }
    Ok(b)
}
fn expected_uid(path: &Path) -> u32 {
    if rustix::process::geteuid().as_raw() == 0 && path.starts_with(DATA) {
        986
    } else {
        rustix::process::geteuid().as_raw()
    }
}
fn private_dir(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if !m.is_dir() || m.uid() != expected_uid(path) || m.mode() & 0o7777 != 0o700 {
        return Err(Error::Denied);
    }
    Ok(())
}
fn observed(directory: &Path) -> Result<(u32, String, String)> {
    private_dir(directory)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        if fs::symlink_metadata(directory.join(format!("store.sqlite3{suffix}"))).is_ok() {
            return Err(Error::Busy);
        }
    }
    let path = directory.join("store.sqlite3");
    let bytes = read(&path, LIMIT)?;
    if bytes.len() < 512
        || &bytes[..16] != b"SQLite format 3\0"
        || u32::from_be_bytes(bytes[68..72].try_into().map_err(|_| Error::Invalid)?) != 1381189698
    {
        return Err(Error::Invalid);
    }
    let version = u32::from_be_bytes(bytes[60..64].try_into().map_err(|_| Error::Invalid)?);
    // /var/lib fixed production path; temporary test paths are URI-escaped.
    let escaped = path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-._~".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    let conn = rusqlite::Connection::open_with_flags(
        format!("file:{escaped}?immutable=1"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| Error::Invalid)?;
    conn.execute_batch("PRAGMA query_only=ON;PRAGMA trusted_schema=OFF;")
        .map_err(|_| Error::Invalid)?;
    let realm: String = conn
        .query_row("SELECT db_epoch FROM meta WHERE singleton=1", [], |r| {
            r.get(0)
        })
        .map_err(|_| Error::Invalid)?;
    let pin: Vec<u8> = conn
        .query_row(
            "SELECT public_key FROM device_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| Error::Invalid)?;
    if realm.len() != 36 || pin.len() != 32 {
        return Err(Error::Invalid);
    }
    let mut binding = pin;
    binding.extend_from_slice(realm.as_bytes());
    Ok((version, digest(&bytes), digest(&binding)))
}
fn persist(dir: &Path, j: &Journal) -> Result<()> {
    let raw = serde_json::to_vec(j).map_err(|_| Error::Invalid)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(dir.join("state.next"))
        .map_err(|_| Error::Io)?;
    f.write_all(&raw)
        .and_then(|_| f.sync_all())
        .map_err(|_| Error::Io)?;
    fs::rename(dir.join("state.next"), dir.join("state.json")).map_err(|_| Error::Io)?;
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)
}
fn execute(
    directory: &Path,
    permit: &Permit,
    before_commit: impl FnOnce() -> Result<()>,
    mut boundary: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    permit.live()?;
    let (version, source, binding) = observed(directory)?;
    if version != permit.source_schema
        || source != permit.source_sha256
        || binding != permit.binding_sha256
    {
        return Err(Error::Denied);
    }
    let lock = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(directory.join(".writer.lock"))
        .map_err(|_| Error::Io)?;
    let m = lock.metadata().map_err(|_| Error::Io)?;
    if !m.is_file()
        || m.uid() != expected_uid(&directory.join(".writer.lock"))
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
    {
        return Err(Error::Denied);
    }
    lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
    before_commit()?;
    permit.live()?;
    if observed(directory)? != (version, source.clone(), binding.clone()) {
        return Err(Error::Busy);
    }
    let journal = directory.join("product-upgrade");
    if fs::symlink_metadata(&journal).is_ok() {
        return Err(Error::Busy);
    }
    fs::create_dir(&journal).map_err(|_| Error::Io)?;
    fs::set_permissions(&journal, fs::Permissions::from_mode(0o700)).map_err(|_| Error::Io)?;
    File::open(directory)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)?;
    let mut j = Journal {
        format: 1,
        request_id: permit.request_id.clone(),
        source_schema: version,
        source_sha256: source,
        binding_sha256: binding.clone(),
        phase: Phase::Prepared,
        after_sha256: None,
    };
    persist(&journal, &j)?;
    boundary("prepared")?;
    // Explicit ownership handoff to the ORIGINAL Store writer. The production
    // parent must keep Core stopped and the Root/slot fence live across this gap.
    FileExt::unlock(&lock).map_err(|_| Error::Io)?;
    drop(lock);
    if version == 6 {
        j.phase = Phase::ExecutingGrants;
        persist(&journal, &j)?;
        permit.live()?;
        let backup = journal.join("before-grants");
        fs::create_dir(&backup).map_err(|_| Error::Io)?;
        fs::set_permissions(&backup, fs::Permissions::from_mode(0o700)).map_err(|_| Error::Io)?;
        let store = rustsimhub_storage::Store::migrate_recovery_grants(directory, &backup)
            .map_err(|_| Error::Unknown)?;
        drop(store);
        boundary("grants-commit-before-journal")?;
        let (schema, hash, new_binding) = observed(directory)?;
        if schema != 7 || new_binding != binding {
            return Err(Error::Unknown);
        }
        j.phase = Phase::CommittedGrants;
        j.after_sha256 = Some(hash);
        persist(&journal, &j)?;
        boundary("grants-committed")?;
    }
    permit.live()?;
    j.phase = Phase::ExecutingExecution;
    persist(&journal, &j)?;
    let backup = journal.join("before-execution");
    fs::create_dir(&backup).map_err(|_| Error::Io)?;
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o700)).map_err(|_| Error::Io)?;
    rustsimhub_storage::Store::migrate_recovery_execution(directory, &backup)
        .map_err(|_| Error::Unknown)?;
    boundary("execution-commit-before-journal")?;
    let (schema, hash, new_binding) = observed(directory)?;
    if schema != 9 || new_binding != binding {
        return Err(Error::Unknown);
    }
    j.phase = Phase::CommittedExecution;
    j.after_sha256 = Some(hash);
    persist(&journal, &j)?;
    boundary("execution-committed")?;
    Ok(())
}
/// Strictly fixed private maintenance entry; caller wires it before Core startup.
pub fn child() -> Result<()> {
    let input = std::io::stdin();
    let peer = rustix::net::sockopt::socket_peercred(input.as_fd()).map_err(|_| Error::Denied)?;
    if peer.uid.as_raw() != 0
        || Some(peer.pid) != rustix::process::getppid()
        || peer.pid.as_raw_nonzero().get() <= 1
    {
        return Err(Error::Denied);
    }
    let status = fs::read_to_string("/proc/self/status").map_err(|_| Error::Io)?;
    for key in ["Uid:", "Gid:"] {
        if status
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .ok_or(Error::Denied)?
            .split_whitespace()
            .collect::<Vec<_>>()
            != ["986"; 4]
        {
            return Err(Error::Denied);
        }
    }
    if status
        .lines()
        .find_map(|l| l.strip_prefix("Groups:"))
        .ok_or(Error::Denied)?
        .trim()
        != ""
        || !rustix::thread::no_new_privs().map_err(|_| Error::Denied)?
    {
        return Err(Error::Denied);
    }
    for key in ["CapInh:", "CapPrm:", "CapEff:", "CapAmb:"] {
        if u64::from_str_radix(
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))
                .ok_or(Error::Denied)?
                .trim(),
            16,
        )
        .ok()
            != Some(0)
        {
            return Err(Error::Denied);
        }
    }
    let fd = rustix::io::dup(input.as_fd()).map_err(|_| Error::Io)?;
    let mut channel = UnixStream::from(fd);
    channel
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| Error::Io)?;
    channel
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| Error::Io)?;
    let mut size = [0; 4];
    channel.read_exact(&mut size).map_err(|_| Error::Io)?;
    let n = u32::from_be_bytes(size) as usize;
    if n == 0 || n > 2048 {
        return Err(Error::Invalid);
    }
    let mut raw = vec![0; n];
    channel.read_exact(&mut raw).map_err(|_| Error::Io)?;
    let permit: Permit = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
    execute(
        Path::new(DATA),
        &permit,
        || {
            channel.write_all(b"R").map_err(|_| Error::Io)?;
            let mut ack = [0];
            channel.read_exact(&mut ack).map_err(|_| Error::Io)?;
            if ack != *b"G" {
                return Err(Error::Denied);
            }
            Ok(())
        },
        |_| Ok(()),
    )?;
    channel.write_all(b"D").map_err(|_| Error::Io)
}
/// A conservative read-only query. Schema alone never upgrades an unknown
/// journal to a committed result; there is no automatic data rollback/retry.
pub fn inspect() -> Result<serde_json::Value> {
    inspect_at(Path::new(DATA))
}
fn inspect_at(directory: &Path) -> Result<serde_json::Value> {
    let (schema, hash, binding) = observed(directory)?;
    let path = directory.join("product-upgrade/state.json");
    let j: Journal = serde_json::from_slice(&read(&path, 8192)?).map_err(|_| Error::Invalid)?;
    if j.format != 1 || !hex(&j.request_id) || !hex(&j.source_sha256) || !hex(&j.binding_sha256) {
        return Err(Error::Invalid);
    }
    let state = if j.binding_sha256 != binding {
        "unknown"
    } else if j.phase == Phase::Prepared && schema == j.source_schema && hash == j.source_sha256 {
        "not_started"
    } else if j.phase == Phase::CommittedExecution
        && schema == 9
        && j.after_sha256.as_deref() == Some(&hash)
    {
        "committed"
    } else if j.phase == Phase::CommittedGrants
        && schema == 7
        && j.after_sha256.as_deref() == Some(&hash)
    {
        "grants_committed"
    } else {
        "unknown"
    };
    Ok(
        serde_json::json!({"format":1,"state":state,"actual_schema":schema,"read_only":true,"may_resume_automatically":false,"may_restore_database":false,"authorization_verified":false}),
    )
}
#[cfg(test)]
mod tests;

/// Fixed-path Root preflight only. The parent still needs the live signed-mode
/// admission and stopped-Core/writer handoff before using this private permit.
pub fn prepare_root(request_id: &str) -> Result<Permit> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    let (schema, source_sha256, binding_sha256) = observed(Path::new(DATA))?;
    let p = Permit {
        format: 1,
        source_schema: schema,
        target_schema: 9,
        source_sha256,
        binding_sha256,
        request_id: request_id.into(),
        deadline_ms: boot_ms()? + 30_000,
    };
    p.live()?;
    Ok(p)
}
