// SPDX-License-Identifier: AGPL-3.0-only
//! Finite Root maintenance entry. Called with signed-role version facts by the
//! updater; neither this function nor its child starts Core/a worker/network.
use super::*;
use crate::{
    queue::migration::{boot_ms, Permit, DIRECTORY},
    RuntimeCompatibility,
};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            net::UnixStream,
        },
    },
    process::{Child, Command, Stdio},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::timeout,
};
use zeroize::Zeroizing;

fn absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(Error::Busy),
    }
}
fn source(path: &Path, uid: u32, max: u64) -> Result<File> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.uid() != uid
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.len() > max
    {
        return Err(Error::Forbidden);
    }
    Ok(f)
}
struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

/// Explicit opt-in API, not a boot action. `programs` must be taken from the
/// updater's verified signed selected/candidate/fallback bytes while its slot
/// and business-data locks remain held. The exact selected executable digest
/// is checked again here before a sealed copy is run. No JSON report grants
/// permission; the actual Root worker guard and authenticated child do that.
pub async fn migrate_email4(
    c: Context,
    programs: [RuntimeCompatibility; 3],
    selected_worker_sha256: [u8; 32],
) -> Result<()> {
    c.validate()?;
    if c.core_uid != 986
        || c.program != Path::new("/opt/rustsimhub/selected/rustsimhub-forwarder")
        || c.persistent != Path::new("/var/lib/rustsimhub/rustsimhub-v1/system/forwarding")
        || c.runtime != Path::new("/run/rustsimhub-forwarding")
        || c.unit != "rustsimhub-forwarder.service"
        || programs
            .iter()
            .any(|p| p != &crate::runtime_compatibility())
    {
        return Err(Error::Forbidden);
    }
    let _control = lock(&c.runtime.join("control.lock"))?;
    let quiet = || -> Result<()> {
        absent(&c.gate())?;
        absent(&c.runtime.join("settings.json"))
    };
    quiet()?;
    System::new(&c).empty().await?;
    let directory = Path::new(DIRECTORY);
    let m = fs::symlink_metadata(directory)?;
    if !m.is_dir() || m.uid() != c.core_uid || m.mode() & 0o7777 != 0o700 {
        return Err(Error::Forbidden);
    }
    let writer = source(&directory.join(".writer.lock"), c.core_uid, 0)?;
    writer.try_lock_exclusive().map_err(|_| Error::Busy)?;
    for suffix in ["-journal", "-wal", "-shm"] {
        absent(&directory.join(format!("outbox.sqlite3{suffix}")))?;
    }
    let mut bytes = Zeroizing::new(Vec::new());
    source(
        &directory.join("outbox.sqlite3"),
        c.core_uid,
        32 * 1024 * 1024,
    )?
    .take(32 * 1024 * 1024 + 1)
    .read_to_end(&mut bytes)?;
    if bytes.len() < 100
        || bytes.len() > 32 * 1024 * 1024
        || &bytes[..16] != b"SQLite format 3\0"
        || bytes[18..20] != [1, 1]
        || u32::from_be_bytes(bytes[60..64].try_into().map_err(|_| Error::Invalid)?) != 3
        || u32::from_be_bytes(bytes[68..72].try_into().map_err(|_| Error::Invalid)?)
            != crate::OUTBOX_APPLICATION_ID as u32
    {
        return Err(Error::Invalid);
    }
    let digest: [u8; 32] = Sha256::digest(&*bytes).into();
    let backups = c.persistent.join("email-backups");
    match fs::create_dir(&backups) {
        Ok(()) => fs::set_permissions(&backups, fs::Permissions::from_mode(0o700))?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    };
    let m = fs::symlink_metadata(&backups)?;
    if !m.is_dir() || m.uid() != 0 || m.mode() & 0o7777 != 0o700 {
        return Err(Error::Forbidden);
    }
    let backup = backups.join(format!("{}.sqlite3", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&backup)?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    drop(f);
    File::open(&backups)?.sync_all()?;
    let mut check = Zeroizing::new(Vec::new());
    source(&backup, 0, 32 * 1024 * 1024)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut check)?;
    if *check != *bytes || <[u8; 32]>::from(Sha256::digest(&*check)) != digest {
        return Err(Error::Storage);
    }
    drop(check);
    drop(bytes);
    // No path race between the verified program and exec. FD1 is a sealed
    // executable; this maintenance child never writes stdout or arbitrary files.
    system::program(&c.program)?;
    let mut executable = Zeroizing::new(Vec::new());
    File::open(&c.program)?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut executable)?;
    if executable.len() > 64 * 1024 * 1024
        || <[u8; 32]>::from(Sha256::digest(&*executable)) != selected_worker_sha256
    {
        return Err(Error::Forbidden);
    }
    use rustix::fs::{fchmod, fcntl_add_seals, memfd_create, MemfdFlags, Mode, SealFlags};
    let fd = memfd_create(
        c"rustsimhub-email-migration",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .map_err(|_| Error::Storage)?;
    let mut sealed = File::from(fd);
    sealed.write_all(&executable)?;
    fchmod(&sealed, Mode::from_raw_mode(0o555)).map_err(|_| Error::Storage)?;
    fcntl_add_seals(
        &sealed,
        SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL,
    )
    .map_err(|_| Error::Storage)?;
    drop(executable);
    quiet()?;
    System::new(&c).empty().await?;
    FileExt::unlock(&writer)?;
    drop(writer);
    let permit = Permit {
        format: 1,
        source_sha256: digest,
        backup_sha256: digest,
        deadline_ms: boot_ms()? + 30_000,
        data_uid: c.core_uid,
    };
    let raw = serde_json::to_vec(&permit)?;
    let (parent, child) = UnixStream::pair()?;
    parent.set_nonblocking(true)?;
    let mut parent = tokio::net::UnixStream::from_std(parent)?;
    use std::os::unix::process::CommandExt;
    rustix::thread::set_no_new_privs(true).map_err(|_| Error::Forbidden)?;
    let mut child = Reap(
        Command::new("/proc/self/fd/1")
            .arg0("rustsimhub-email-migration")
            .arg("--migrate-email4-child")
            .uid(c.core_uid)
            .gid(c.core_uid)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::from(OwnedFd::from(child)))
            .stdout(Stdio::from(sealed))
            .stderr(Stdio::null())
            .spawn()?,
    );
    let exchange = timeout(Duration::from_secs(10), async {
        parent.write_all(&(raw.len() as u32).to_be_bytes()).await?;
        parent.write_all(&raw).await?;
        let mut phase = [0];
        parent.read_exact(&mut phase).await?;
        if phase != *b"R" {
            return Err(Error::Invalid);
        }
        quiet()?;
        System::new(&c).empty().await?;
        parent.write_all(b"G").await?;
        parent.read_exact(&mut phase).await?;
        if phase != *b"D" {
            return Err(Error::Storage);
        }
        loop {
            if let Some(status) = child.0.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(Error::Storage)
                };
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .map_err(|_| Error::Busy)?;
    exchange?;
    quiet()?;
    System::new(&c).empty().await?;
    // The opaque backup persists after success AND every ambiguous failure.
    Ok(())
}

/// Same explicit action for the synchronous updater. No global runtime/thread.
pub fn migrate_email4_blocking(
    c: Context,
    programs: [RuntimeCompatibility; 3],
    digest: [u8; 32],
) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(migrate_email4(c, programs, digest))
}
