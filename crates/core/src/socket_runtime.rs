// SPDX-License-Identifier: AGPL-3.0-only
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use tokio::net::UnixListener;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub struct SocketRuntime {
    path: PathBuf,
    dev: u64,
    ino: u64,
    _lock: File,
}
impl SocketRuntime {
    pub fn bind(path: &Path) -> Result<(UnixListener, Self)> {
        let parent = path.parent().ok_or("socket parent missing")?;
        let meta = fs::symlink_metadata(parent)?;
        let uid = rustix::process::geteuid().as_raw();
        if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o022 != 0 {
            return Err("unsafe socket directory".into());
        }
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .filter(|v| v.len() <= 64)
            .ok_or("invalid socket name")?;
        let lock_path = parent.join(format!(".{name}.lock"));
        if let Ok(m) = fs::symlink_metadata(&lock_path) {
            if !m.is_file() || m.uid() != uid || m.mode() & 0o077 != 0 || m.nlink() != 1 {
                return Err("unsafe socket lock".into());
            }
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&lock_path)?;
        lock.try_lock_exclusive()
            .map_err(|_| "socket already owned")?;
        match fs::symlink_metadata(path) {
            Ok(old) => {
                if !old.file_type().is_socket() || old.uid() != uid {
                    return Err("socket path is not an owned socket".into());
                }
                use rustix::net::{
                    connect, socket_with, AddressFamily, SocketAddrUnix, SocketFlags, SocketType,
                };
                let probe = socket_with(
                    AddressFamily::UNIX,
                    SocketType::STREAM,
                    SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
                    None,
                )?;
                let addr = SocketAddrUnix::new(path)?;
                match connect(&probe, &addr) {
                    Err(e) if e == rustix::io::Errno::CONNREFUSED => {}
                    _ => return Err("socket is active or cannot safely be reclaimed".into()),
                }
                let current = fs::symlink_metadata(path)?;
                if current.dev() != old.dev()
                    || current.ino() != old.ino()
                    || !current.file_type().is_socket()
                {
                    return Err("socket path changed".into());
                }
                fs::remove_file(path)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let listener = crate::linux::bind_listener(path.to_str().ok_or("invalid socket path")?)?;
        let m = fs::symlink_metadata(path)?;
        Ok((
            listener,
            Self {
                path: path.to_owned(),
                dev: m.dev(),
                ino: m.ino(),
                _lock: lock,
            },
        ))
    }
}
impl Drop for SocketRuntime {
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
    use std::os::unix::{
        fs::{symlink, PermissionsExt},
        net::UnixListener as StdListener,
    };
    fn dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    #[tokio::test]
    async fn live_owner_and_legacy_listener_cannot_be_unlinked() {
        let d = dir();
        let p = d.path().join("core.sock");
        let (l, g) = SocketRuntime::bind(&p).unwrap();
        let ino = fs::symlink_metadata(&p).unwrap().ino();
        assert!(SocketRuntime::bind(&p).is_err());
        assert_eq!(fs::symlink_metadata(&p).unwrap().ino(), ino);
        drop(l);
        drop(g);
        assert!(!p.exists());
        let legacy = StdListener::bind(&p).unwrap();
        assert!(SocketRuntime::bind(&p).is_err());
        assert!(p.exists());
        drop(legacy);
    }
    #[tokio::test]
    async fn stale_owned_socket_is_reclaimed_but_replacement_file_is_preserved() {
        let d = dir();
        let p = d.path().join("core.sock");
        drop(StdListener::bind(&p).unwrap());
        let (l, g) = SocketRuntime::bind(&p).unwrap();
        fs::remove_file(&p).unwrap();
        fs::write(&p, b"replacement evidence").unwrap();
        drop(l);
        drop(g);
        assert_eq!(fs::read(&p).unwrap(), b"replacement evidence");
        assert!(SocketRuntime::bind(&p).is_err());
    }
    #[tokio::test]
    async fn symlink_lock_and_writable_parent_are_rejected() {
        let d = dir();
        let p = d.path().join("core.sock");
        let target = d.path().join("evidence");
        fs::write(&target, b"preserve").unwrap();
        symlink(&target, d.path().join(".core.sock.lock")).unwrap();
        assert!(SocketRuntime::bind(&p).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"preserve");
        let other = dir();
        fs::set_permissions(other.path(), fs::Permissions::from_mode(0o777)).unwrap();
        assert!(SocketRuntime::bind(&other.path().join("core.sock")).is_err());
    }
}
