// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Error, Result};
use fs2::FileExt;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};
pub(super) fn directory(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !path.is_absolute() || !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(super) fn read(path: &Path, max: usize) -> Result<Vec<u8>> {
    if !path.is_absolute() {
        return Err(Error::Invalid);
    }
    let f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.uid() != 0
        || m.nlink() != 1
        || m.mode() & 0o022 != 0
        || m.len() > max as u64
    {
        return Err(Error::Forbidden);
    }
    let mut bytes = vec![];
    f.take(max as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(Error::Invalid);
    }
    Ok(bytes)
}
pub(super) fn optional<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(_) => Ok(Some(serde_json::from_slice(&read(path, 8192)?)?)),
    }
}
pub(super) fn publish(dir: &Path, name: &str, value: &impl Serialize, mode: u32) -> Result<()> {
    directory(dir)?;
    let target = dir.join(name);
    if fs::symlink_metadata(&target).is_ok() {
        read(&target, 8192)?;
    }
    let bytes = zeroize::Zeroizing::new(serde_json::to_vec(value)?);
    if bytes.len() > 8192 {
        return Err(Error::Invalid);
    }
    let tmp = dir.join(format!(".{}.pending", uuid::Uuid::new_v4()));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)?;
    f.set_permissions(fs::Permissions::from_mode(mode))?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, target)?;
    File::open(dir)?.sync_all()?;
    Ok(())
}
pub(super) fn remove(path: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return Ok(());
    }
    read(path, 8192)?;
    fs::remove_file(path)?;
    File::open(path.parent().ok_or(Error::Invalid)?)?.sync_all()?;
    Ok(())
}
pub(super) fn lock(path: &Path) -> Result<File> {
    directory(path.parent().ok_or(Error::Invalid)?)?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let m = f.metadata()?;
    if !m.is_file() || m.uid() != 0 || m.nlink() != 1 || m.mode() & 0o777 != 0o600 || m.len() != 0 {
        return Err(Error::Forbidden);
    }
    f.try_lock_exclusive().map_err(|_| Error::Busy)?;
    Ok(f)
}
