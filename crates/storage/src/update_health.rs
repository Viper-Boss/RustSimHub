// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded read lease on the EXISTING writer actor. No migration or second writer.
use super::*;
use rustsimhub_policy::{allows, Kind, Principal, Scope, ALL_SCOPES};
use std::{os::unix::fs::MetadataExt, time::Instant};
const MAX_HOLD: Duration = Duration::from_millis(5000);
enum Control {
    Check(mpsc::SyncSender<Result<()>>),
    Release,
}
const ADMISSION: Duration = Duration::from_millis(200);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardingState {
    pub enabled: bool,
    pub generation: u64,
    pub schema: Option<u32>,
}
#[derive(Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub schema: u32,
    pub realm: String,
    pub line: u64,
    pub device_key: [u8; 32],
    pub owner: Principal,
    pub device: u64,
    pub inode: u64,
    pub forwarding: ForwardingState,
}
/// Neither serializable nor cloneable. Dropping releases the original actor.
pub struct Lease {
    snapshot: Snapshot,
    until: Instant,
    release: Option<mpsc::SyncSender<Control>>,
    finished: mpsc::Receiver<Result<()>>,
}
impl Lease {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn revalidate(&self) -> Result<()> {
        if Instant::now() >= self.until {
            return Err(Error::Busy);
        }
        match self.finished.try_recv() {
            Err(mpsc::TryRecvError::Empty) => {
                let (tx, rx) = mpsc::sync_channel(1);
                self.release
                    .as_ref()
                    .ok_or(Error::Stopped)?
                    .try_send(Control::Check(tx))
                    .map_err(|_| Error::Busy)?;
                rx.recv_timeout(ADMISSION.min(self.until.saturating_duration_since(Instant::now())))
                    .map_err(|_| Error::Busy)?
            }
            _ => Err(Error::Stopped),
        }
    }
    pub fn release(mut self) -> Result<()> {
        self.release
            .take()
            .ok_or(Error::Stopped)?
            .send(Control::Release)
            .map_err(|_| Error::Stopped)?;
        self.finished
            .recv_timeout(ADMISSION)
            .map_err(|_| Error::Stopped)?
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(sender) = self.release.take() {
            let _ = sender.try_send(Control::Release);
        }
    }
}
impl Handle {
    /// Internal monotonic deadline; no remotely supplied duration or database path.
    pub fn update_health_lease(&self, until: Instant, queue_owned: bool) -> Result<Lease> {
        let now = Instant::now();
        if until <= now || until.duration_since(now) > MAX_HOLD {
            return Err(Error::InvalidInput);
        }
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        self.sender
            .try_send(Message::Run(Box::new(move |db| {
                let result = (|| {
                    if Instant::now() >= until {
                        return Err(Error::Busy);
                    }
                    let identity = db.health_identity()?;
                    let paths = [
                        db.health_directory.join("store.sqlite3"),
                        db.health_directory.join("store.sqlite3-wal"),
                        db.health_directory.join(".writer.lock"),
                        db.health_directory.join(".restore-pending"),
                    ];
                    let before: Vec<_> = paths
                        .iter()
                        .map(|p| optional_stamp(p))
                        .collect::<Result<_>>()?;
                    let forwarding = outbox(&db.health_directory, queue_owned)?;
                    let tx = db.connection.transaction()?;
                    let mut snapshot = read(&tx, identity.0, identity.1)?;
                    // Bind every source from the SAME transaction/actor; outbox is
                    // guarded by Core's existing forwarding mutex before this call.
                    snapshot.forwarding = forwarding.state.clone();
                    if Instant::now() >= until {
                        return Err(Error::Busy);
                    }
                    if ready_tx.send(Ok(snapshot)).is_err() {
                        return Ok(());
                    }
                    let check = || -> Result<()> {
                        forwarding.revalidate()?;
                        for (p, expected) in paths.iter().zip(&before) {
                            if &optional_stamp(p)? != expected {
                                return Err(Error::Busy);
                            }
                        }
                        Ok(())
                    };
                    while let Ok(Control::Check(reply)) =
                        release_rx.recv_timeout(until.saturating_duration_since(Instant::now()))
                    {
                        let result = check();
                        let _ = reply.send(result);
                        result?;
                    }
                    check()?;
                    tx.rollback()?;
                    db.health_identity()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = ready_tx.try_send(Err(error));
                }
                let _ = done_tx.send(result);
            })))
            .map_err(|e| match e {
                mpsc::TrySendError::Full(_) => Error::Busy,
                _ => Error::Stopped,
            })?;
        let snapshot = ready_rx
            .recv_timeout(ADMISSION.min(until.saturating_duration_since(Instant::now())))
            .map_err(|_| Error::Busy)??;
        Ok(Lease {
            snapshot,
            until,
            release: Some(release_tx),
            finished: done_rx,
        })
    }
}
impl Database {
    fn health_identity(&self) -> Result<(u64, u64)> {
        let file = self.health_file.metadata()?;
        let actual = fs::symlink_metadata(self.health_directory.join("store.sqlite3"))?;
        let lock = self._lock.0.metadata()?;
        let current_lock = fs::symlink_metadata(self.health_directory.join(".writer.lock"))?;
        if !actual.is_file()
            || actual.nlink() != 1
            || actual.uid() != rustix::process::geteuid().as_raw()
            || actual.mode() & 0o7777 != 0o600
            || file.dev() != actual.dev()
            || file.ino() != actual.ino()
            || lock.dev() != current_lock.dev()
            || lock.ino() != current_lock.ino()
            || self
                .health_directory
                .join(".restore-pending")
                .try_exists()?
        {
            return Err(Error::Forbidden);
        }
        Ok((file.dev(), file.ino()))
    }
}
fn read(c: &Connection, device: u64, inode: u64) -> Result<Snapshot> {
    if c.pragma_query_value::<i64, _>(None, "application_id", |r| r.get(0))? != APP_ID {
        return Err(Error::ForeignDatabase);
    }
    let schema = c.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))?;
    if !matches!(schema, 6 | 7 | 9) {
        return Err(Error::NewerSchema);
    }
    let (realm, line): (String, i64) = c.query_row(
        "SELECT db_epoch,line_epoch FROM meta WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if Uuid::parse_str(&realm)
        .map_err(|_| Error::Corrupt)?
        .to_string()
        != realm
        || line <= 0
    {
        return Err(Error::Corrupt);
    }
    let key: Vec<u8> = c.query_row(
        "SELECT public_key FROM device_identity WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let owners: Vec<Vec<u8>> = c
        .prepare("SELECT public_key FROM principals WHERE kind='owner' LIMIT 2")?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    if owners.len() != 1 {
        return Err(Error::Forbidden);
    }
    let owner = super::auth::lookup_principal(
        c,
        &owners[0]
            .as_slice()
            .try_into()
            .map_err(|_| Error::Corrupt)?,
    )?
    .ok_or(Error::Forbidden)?;
    if owner.kind != Kind::Owner
        || !owner.active
        || owner.generation == 0
        || owner.scopes & !ALL_SCOPES != 0
        || !allows(owner.scopes, Scope::SystemManage)
    {
        return Err(Error::Forbidden);
    }
    Ok(Snapshot {
        schema,
        realm,
        line: u64::try_from(line).map_err(|_| Error::Corrupt)?,
        owner,
        device_key: key.try_into().map_err(|_| Error::Corrupt)?,
        device,
        inode,
        forwarding: ForwardingState {
            enabled: false,
            generation: 0,
            schema: None,
        },
    })
}
#[derive(PartialEq, Eq)]
struct Stamp(u64, u64, u64, i64, i64, i64, i64);
fn stamp(p: &Path) -> Result<Stamp> {
    let m = fs::symlink_metadata(p)?;
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o7777 != 0o600
    {
        return Err(Error::Forbidden);
    }
    Ok(Stamp(
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    ))
}
fn optional_stamp(p: &Path) -> Result<Option<Stamp>> {
    match stamp(p) {
        Ok(s) => Ok(Some(s)),
        Err(Error::Io) if !p.try_exists()? => Ok(None),
        Err(e) => Err(e),
    }
}
struct Outbox {
    absent: Option<PathBuf>,
    state: ForwardingState,
    files: Vec<(PathBuf, Stamp)>,
    _connection: Option<Connection>,
}
impl Outbox {
    fn revalidate(&self) -> Result<()> {
        if let Some(p) = &self.absent {
            if p.try_exists()? {
                return Err(Error::Busy);
            }
        }
        for (path, original) in &self.files {
            if &stamp(path)? != original {
                return Err(Error::Busy);
            }
        }
        Ok(())
    }
}
fn outbox(directory: &Path, queue_owned: bool) -> Result<Outbox> {
    let path = directory.join("forwarding");
    let m = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Outbox {
                absent: Some(path),
                state: ForwardingState {
                    enabled: false,
                    generation: 0,
                    schema: None,
                },
                files: vec![],
                _connection: None,
            })
        }
        Err(e) => return Err(e.into()),
    };
    // An uninitialized controller cannot assert policy from a disk config.
    if !queue_owned
        || !m.is_dir()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o7777 != 0o700
    {
        return Err(Error::Busy);
    }
    let lock_path = path.join(".writer.lock");
    let lock_stamp = stamp(&lock_path)?;
    let path = path.join("outbox.sqlite3");
    let observed = stamp(&path)?;
    let c = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    c.busy_timeout(Duration::ZERO)?;
    c.execute_batch("PRAGMA query_only=ON; PRAGMA trusted_schema=OFF; BEGIN;")?;
    if c.pragma_query_value::<i64, _>(None, "application_id", |r| r.get(0))? != 0x52534657 {
        return Err(Error::ForeignDatabase);
    }
    let schema = c.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))?;
    if !matches!(schema, 3 | 4) {
        return Err(Error::NewerSchema);
    }
    // Deliberately never SELECT policy, jobs, request or credential contents.
    let (enabled, generation): (bool, i64) = c.query_row(
        "SELECT enabled,generation FROM meta WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if stamp(&path)? != observed || stamp(&lock_path)? != lock_stamp {
        return Err(Error::Busy);
    }
    Ok(Outbox {
        absent: None,
        state: ForwardingState {
            enabled,
            generation: u64::try_from(generation).map_err(|_| Error::Corrupt)?,
            schema: Some(schema),
        },
        files: vec![(path, observed), (lock_path, lock_stamp)],
        _connection: Some(c),
    })
}

#[cfg(test)]
mod tests;
