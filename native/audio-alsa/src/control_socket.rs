// SPDX-License-Identifier: AGPL-3.0-only
//! Media-owned IPC with an exclusive lock and inode-checked stale cleanup.
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use tokio::net::UnixListener;
type Result<T> = std::result::Result<T, &'static str>;
pub struct Guard {
    path: PathBuf,
    dev: u64,
    ino: u64,
    _lock: File,
}
pub fn bind(path: &Path) -> Result<(UnixListener, Guard)> {
    let parent = path.parent().ok_or("missing worker socket parent")?;
    let meta = fs::symlink_metadata(parent).map_err(|_| "worker socket directory unavailable")?;
    let uid = rustix::process::geteuid().as_raw();
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o022 != 0 {
        return Err("unsafe worker socket directory");
    }
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .filter(|v| v.len() <= 64)
        .ok_or("invalid worker socket name")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(parent.join(format!(".{name}.lock")))
        .map_err(|_| "worker lock unavailable")?;
    let m = lock.metadata().map_err(|_| "worker lock unavailable")?;
    if !m.is_file() || m.uid() != uid || m.mode() & 0o077 != 0 || m.nlink() != 1 {
        return Err("unsafe worker lock");
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| "worker already running")?;
    match fs::symlink_metadata(path) {
        Ok(old) => {
            if !old.file_type().is_socket() || old.uid() != uid {
                return Err("unsafe existing worker endpoint");
            }
            use rustix::net::{
                connect, socket_with, AddressFamily, SocketAddrUnix, SocketFlags, SocketType,
            };
            let probe = socket_with(
                AddressFamily::UNIX,
                SocketType::STREAM,
                SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
                None,
            )
            .map_err(|_| "worker probe unavailable")?;
            let address = SocketAddrUnix::new(path).map_err(|_| "invalid worker address")?;
            match connect(&probe, &address) {
                Err(e) if e == rustix::io::Errno::CONNREFUSED => {}
                _ => return Err("existing worker endpoint is live or uncertain"),
            }
            let current = fs::symlink_metadata(path).map_err(|_| "worker path changed")?;
            if current.dev() != old.dev()
                || current.ino() != old.ino()
                || !current.file_type().is_socket()
            {
                return Err("worker path changed");
            }
            fs::remove_file(path).map_err(|_| "stale worker endpoint unavailable")?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("worker endpoint unavailable"),
    }
    let listener = UnixListener::bind(path).map_err(|_| "worker bind failed")?;
    let m = fs::symlink_metadata(path).map_err(|_| "worker metadata unavailable")?;
    let guard = Guard {
        path: path.to_owned(),
        dev: m.dev(),
        ino: m.ino(),
        _lock: lock,
    };
    if m.uid() != uid || m.gid() != meta.gid() {
        return Err("worker socket must inherit the protected IPC directory group");
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o660))
        .map_err(|_| "worker mode unavailable")?;
    Ok((listener, guard))
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(m) = fs::symlink_metadata(&self.path) {
            if m.file_type().is_socket() && m.dev() == self.dev && m.ino() == self.ino {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::{fs::symlink, net::UnixListener as StdListener};
    fn directory() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    #[tokio::test]
    async fn exclusive_owner_and_legacy_live_socket_are_preserved() {
        let d = directory();
        let path = d.path().join("worker.sock");
        let (listener, guard) = bind(&path).unwrap();
        let ino = fs::symlink_metadata(&path).unwrap().ino();
        assert!(bind(&path).is_err());
        assert_eq!(fs::symlink_metadata(&path).unwrap().ino(), ino);
        drop(listener);
        drop(guard);
        assert!(!path.exists());
        let legacy = StdListener::bind(&path).unwrap();
        assert!(bind(&path).is_err());
        drop(legacy);
        let (listener, guard) = bind(&path).unwrap();
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"preserve replacement").unwrap();
        drop(listener);
        drop(guard);
        assert_eq!(fs::read(&path).unwrap(), b"preserve replacement");
    }
    #[tokio::test]
    async fn writable_parent_linked_lock_and_hardlinked_lock_fail_closed() {
        let d = directory();
        let path = d.path().join("worker.sock");
        let lock = d.path().join(".worker.sock.lock");
        let evidence = d.path().join("evidence");
        fs::write(&evidence, b"preserve").unwrap();
        symlink(&evidence, &lock).unwrap();
        assert!(bind(&path).is_err());
        assert_eq!(fs::read(&evidence).unwrap(), b"preserve");
        fs::remove_file(lock).unwrap();
        fs::set_permissions(&evidence, fs::Permissions::from_mode(0o600)).unwrap();
        fs::hard_link(&evidence, d.path().join(".worker.sock.lock")).unwrap();
        assert!(bind(&path).is_err());
        let other = directory();
        fs::set_permissions(other.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(bind(&other.path().join("worker.sock")).is_err());
    }
}
