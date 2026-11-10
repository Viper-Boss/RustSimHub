// SPDX-License-Identifier: AGPL-3.0-only
//! Root-relative file descriptors; only fixed installer paths call this module.
//! There is no public caller-selected production root or generic filesystem API.
use super::{Error, Result};
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

mod owned;
pub use owned::{OwnedDirectoryVersion, OwnedNodeFact};

const MAX_FILE: usize = 64 * 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    /// Permission/special bits, not file type. File type is checked separately.
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub links: u64,
    pub ctime: (i64, i64),
    pub mtime: (i64, i64),
}
impl From<std::fs::Metadata> for Stamp {
    fn from(m: std::fs::Metadata) -> Self {
        Self {
            mode: m.mode() & 0o7777,
            uid: m.uid(),
            gid: m.gid(),
            device: m.dev(),
            inode: m.ino(),
            size: m.len(),
            links: m.nlink(),
            ctime: (m.ctime(), m.ctime_nsec()),
            mtime: (m.mtime(), m.mtime_nsec()),
        }
    }
}
impl Stamp {
    fn identity(&self, other: &Self) -> bool {
        self.device == other.device && self.inode == other.inode
    }
    // rename/exchange changes ctime. The held fd and complete bytes below must
    // still match; a different inode or any data/permission change is rejected.
    fn renamed(&self, other: &Self) -> bool {
        self.identity(other)
            && self.mode == other.mode
            && self.uid == other.uid
            && self.gid == other.gid
            && self.size == other.size
            && self.links == other.links
            && self.mtime == other.mtime
    }
}
/// The fd remains live. Serialized Stamp data alone is never this read guard.
pub struct FileSnapshot {
    pub bytes: Vec<u8>,
    pub stamp: Stamp,
    file: File,
}
impl std::fmt::Debug for FileSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSnapshot")
            .field("stamp", &self.stamp)
            .field(
                "bytes",
                &format_args!("<redacted: {} bytes>", self.bytes.len()),
            )
            .finish()
    }
}
impl FileSnapshot {
    pub fn recheck(&self, root: &Root, path: &str) -> Result<()> {
        root.recheck(path, Some(self))
    }
    /// Readonly running observation only: no mutation or renamed-generation
    /// adoption. Initial bytes were checked against the signed material; the
    /// original fd and named file must keep their complete, unchanged stamp.
    pub(crate) fn recheck_unchanged(&self, root: &Root, path: &str) -> Result<()> {
        let (parent, name, parent_path) = root.parent(path)?;
        let named = File::from(
            rustix::fs::openat(
                &parent,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| Error::Conflict)?,
        );
        if Stamp::from(self.file.metadata().map_err(|_| Error::Io)?) != self.stamp
            || Stamp::from(named.metadata().map_err(|_| Error::Io)?) != self.stamp
        {
            return Err(Error::Conflict);
        }
        root.parent_still(&parent_path, &parent)?;
        if Stamp::from(self.file.metadata().map_err(|_| Error::Io)?) != self.stamp {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}
pub struct Root {
    file: File,
    path: PathBuf,
    stamp: Stamp,
    owner: u32,
    group: u32,
    isolated: bool,
    owned: owned::Recorder,
}
impl Root {
    pub fn production() -> Result<Self> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        Self::at(Path::new("/"), false)
    }
    #[cfg(test)]
    pub fn isolated(path: &Path) -> Result<Self> {
        if !path.is_absolute() || path == Path::new("/") {
            return Err(Error::Denied);
        }
        let m = std::fs::symlink_metadata(path).map_err(|_| Error::Io)?;
        if m.mode() & 0o7777 != 0o700 {
            return Err(Error::Denied);
        }
        Self::at(path, true)
    }
    fn at(path: &Path, isolated: bool) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags((OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC).bits() as i32)
            .open(path)
            .map_err(|_| Error::Io)?;
        let m = file.metadata().map_err(|_| Error::Io)?;
        let owner = rustix::process::geteuid().as_raw();
        if !m.is_dir() || m.uid() != owner || m.mode() & 0o022 != 0 {
            return Err(Error::Denied);
        }
        let stamp = m.into();
        Ok(Self {
            file,
            path: path.into(),
            stamp,
            owner,
            group: if isolated {
                rustix::process::getegid().as_raw()
            } else {
                0
            },
            isolated,
            owned: owned::Recorder::default(),
        })
    }
    pub fn is_isolated(&self) -> bool {
        self.isolated
    }
    fn check_root(&self) -> Result<()> {
        let m = std::fs::symlink_metadata(&self.path).map_err(|_| Error::Io)?;
        let now: Stamp = m.into();
        let held: Stamp = self.file.metadata().map_err(|_| Error::Io)?.into();
        if !self.stamp.identity(&now)
            || !self.stamp.identity(&held)
            || now.mode != self.stamp.mode
            || now.uid != self.owner
            || now.mode & 0o022 != 0
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    pub fn open_dir(&self, path: &str) -> Result<File> {
        self.open_dir_optional(path)?.ok_or(Error::Io)
    }
    fn open_dir_optional(&self, path: &str) -> Result<Option<File>> {
        self.check_root()?;
        let mut directory = self.file.try_clone().map_err(|_| Error::Io)?;
        for name in components(path)? {
            let fd = match rustix::fs::openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(e) if e == rustix::io::Errno::NOENT => {
                    self.check_root()?;
                    return Ok(None);
                }
                Err(_) => return Err(Error::Denied),
            };
            directory = File::from(fd);
            self.check_directory(&directory)?;
        }
        self.check_root()?;
        Ok(Some(directory))
    }
    fn check_directory(&self, directory: &File) -> Result<()> {
        let m = directory.metadata().map_err(|_| Error::Io)?;
        if !m.is_dir() || m.uid() != self.owner || m.mode() & 0o022 != 0 {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn parent(&self, path: &str) -> Result<(File, String, String)> {
        let parts = components(path)?;
        let name = parts.last().ok_or(Error::Denied)?.to_string();
        let parent = if parts.len() == 1 {
            "/".into()
        } else {
            format!("/{}", parts[..parts.len() - 1].join("/"))
        };
        Ok((self.open_dir(&parent)?, name, parent))
    }
    fn parent_still(&self, path: &str, parent: &File) -> Result<()> {
        self.check_root()?;
        let current = self.open_dir(path)?;
        let before: Stamp = parent.metadata().map_err(|_| Error::Io)?.into();
        let now: Stamp = current.metadata().map_err(|_| Error::Io)?.into();
        if !before.identity(&now)
            || before.mode != now.mode
            || before.uid != now.uid
            || before.gid != now.gid
        {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    pub fn ensure_dir(&self, path: &str, mode: u32) -> Result<()> {
        if !matches!(mode, 0o700 | 0o750 | 0o755 | 0o2750) {
            return Err(Error::Denied);
        }
        let parts = components(path)?;
        let mut directory = self.file.try_clone().map_err(|_| Error::Io)?;
        let mut prefix = String::new();
        for (index, name) in parts.iter().enumerate() {
            let wanted = if index + 1 == parts.len() {
                mode
            } else {
                0o755
            };
            let fd = match rustix::fs::openat(
                &directory,
                *name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(fd) => fd,
                Err(e) if e == rustix::io::Errno::NOENT => {
                    rustix::fs::mkdirat(&directory, *name, Mode::from_bits_truncate(wanted))
                        .map_err(|_| Error::Io)?;
                    directory.sync_all().map_err(|_| Error::Unknown)?;
                    let fd = rustix::fs::openat(
                        &directory,
                        *name,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|_| Error::Unknown)?;
                    let made = File::from(fd);
                    let metadata = made.metadata().map_err(|_| Error::Unknown)?;
                    if metadata.uid() != self.owner || metadata.gid() != self.group {
                        rustix::fs::fchown(
                            &made,
                            Some(rustix::process::Uid::from_raw(self.owner)),
                            Some(rustix::process::Gid::from_raw(self.group)),
                        )
                        .map_err(|_| Error::Unknown)?;
                    }
                    made.set_permissions(std::fs::Permissions::from_mode(wanted))
                        .map_err(|_| Error::Unknown)?;
                    made.sync_all().map_err(|_| Error::Unknown)?;
                    self.record_directory(&format!("{prefix}/{name}"), &made)?;
                    rustix::io::dup(&made).map_err(|_| Error::Io)?
                }
                Err(_) => return Err(Error::Denied),
            };
            prefix.push('/');
            prefix.push_str(name);
            directory = File::from(fd);
            self.check_directory(&directory)?;
            self.parent_still(&prefix, &directory)?;
        }
        Ok(())
    }
    pub fn read(&self, path: &str, max: usize) -> Result<Option<FileSnapshot>> {
        if max == 0 || max > MAX_FILE {
            return Err(Error::Invalid);
        }
        let parts = components(path)?;
        let name = parts.last().ok_or(Error::Denied)?.to_string();
        let parent_path = if parts.len() == 1 {
            "/".into()
        } else {
            format!("/{}", parts[..parts.len() - 1].join("/"))
        };
        let Some(parent) = self.open_dir_optional(&parent_path)? else {
            return Ok(None);
        };
        let result = self.read_at(&parent, &name, max)?;
        self.parent_still(&parent_path, &parent)?;
        Ok(result)
    }
    fn read_at(&self, parent: &File, name: &str, max: usize) -> Result<Option<FileSnapshot>> {
        let file = match rustix::fs::openat(
            parent,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => File::from(fd),
            Err(e) if e == rustix::io::Errno::NOENT => return Ok(None),
            Err(_) => return Err(Error::Denied),
        };
        let m = file.metadata().map_err(|_| Error::Io)?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != self.owner
            || m.gid() != self.group
            || m.mode() & 0o022 != 0
        {
            return Err(Error::Denied);
        }
        if m.len() > max as u64 {
            return Err(Error::Invalid);
        }
        let stamp: Stamp = m.into();
        let mut bytes = Vec::new();
        (&file)
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        if bytes.len() > max || bytes.len() as u64 != stamp.size {
            return Err(Error::Invalid);
        }
        let after: Stamp = file.metadata().map_err(|_| Error::Io)?.into();
        if after != stamp {
            return Err(Error::Conflict);
        }
        let named = File::from(
            rustix::fs::openat(
                parent,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| Error::Conflict)?,
        );
        if Stamp::from(named.metadata().map_err(|_| Error::Io)?) != stamp {
            return Err(Error::Conflict);
        }
        Ok(Some(FileSnapshot { bytes, stamp, file }))
    }
    pub fn recheck(&self, path: &str, expected: Option<&FileSnapshot>) -> Result<()> {
        let now = self.read(path, MAX_FILE)?;
        match (expected, now) {
            (None, None) => Ok(()),
            (Some(old), Some(now)) => {
                let held: Stamp = old.file.metadata().map_err(|_| Error::Io)?.into();
                if held != old.stamp || now.stamp != old.stamp || now.bytes != old.bytes {
                    return Err(Error::Conflict);
                }
                Ok(())
            }
            _ => Err(Error::Conflict),
        }
    }
    pub fn replace(
        &self,
        path: &str,
        expected: Option<&FileSnapshot>,
        bytes: &[u8],
        mode: u32,
    ) -> Result<FileSnapshot> {
        self.replace_with_intent(path, expected, bytes, mode, |_| Ok(()))
    }
    pub fn replace_with_intent(
        &self,
        path: &str,
        expected: Option<&FileSnapshot>,
        bytes: &[u8],
        mode: u32,
        record: impl FnOnce(&Stamp) -> Result<()>,
    ) -> Result<FileSnapshot> {
        if bytes.len() > MAX_FILE
            || !matches!(
                mode,
                0o400 | 0o444 | 0o600 | 0o644 | 0o500 | 0o555 | 0o700 | 0o750 | 0o755
            )
        {
            return Err(Error::Invalid);
        }
        let (parent, name, parent_path) = self.parent(path)?;
        self.recheck(path, expected)?;
        let temporary = temporary_name()?;
        let fd = rustix::fs::openat(
            &parent,
            &temporary,
            OFlags::WRONLY
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| Error::Io)?;
        let mut stage = File::from(fd);
        let preparation = (|| {
            stage.write_all(bytes).map_err(|_| Error::Io)?;
            stage
                .set_permissions(std::fs::Permissions::from_mode(mode))
                .map_err(|_| Error::Io)?;
            let m = stage.metadata().map_err(|_| Error::Io)?;
            if m.uid() != self.owner || m.gid() != self.group {
                rustix::fs::fchown(
                    &stage,
                    Some(rustix::process::Uid::from_raw(self.owner)),
                    Some(rustix::process::Gid::from_raw(self.group)),
                )
                .map_err(|_| Error::Denied)?;
            }
            stage.sync_all().map_err(|_| Error::Io)?;
            let check = self
                .read_at(&parent, &temporary, MAX_FILE)?
                .ok_or(Error::Integrity)?;
            if check.bytes != bytes || check.stamp.mode != mode {
                return Err(Error::Integrity);
            }
            self.parent_still(&parent_path, &parent)?;
            self.recheck(path, expected)?;
            Ok(check)
        })();
        let staged = match preparation {
            Ok(s) => s,
            Err(e) => {
                self.cleanup_created(&parent, &temporary, &stage, &parent_path)?;
                return Err(e);
            }
        };
        let staged_path = format!("{}/{}", parent_path.trim_end_matches('/'), temporary);
        self.record_file(&staged_path, None, &staged, true)?;
        if let Err(error) = record(&staged.stamp) {
            self.cleanup_created(&parent, &temporary, &stage, &parent_path)?;
            return Err(error);
        }
        // The durable intent callback may take time; never authorize a stale
        // expected target or a displaced parent simply because staging passed.
        if let Err(error) = self
            .parent_still(&parent_path, &parent)
            .and_then(|_| self.recheck(path, expected))
        {
            self.cleanup_created(&parent, &temporary, &stage, &parent_path)?;
            return Err(error);
        }
        let flags = if expected.is_some() {
            RenameFlags::EXCHANGE
        } else {
            RenameFlags::NOREPLACE
        };
        if let Err(e) = rustix::fs::renameat_with(&parent, &temporary, &parent, &name, flags) {
            self.cleanup_created(&parent, &temporary, &stage, &parent_path)?;
            return Err(
                if e == rustix::io::Errno::EXIST || e == rustix::io::Errno::NOENT {
                    Error::Conflict
                } else if [
                    rustix::io::Errno::NOSYS,
                    rustix::io::Errno::INVAL,
                    rustix::io::Errno::OPNOTSUPP,
                ]
                .contains(&e)
                {
                    Error::Unsupported
                } else {
                    Error::Io
                },
            );
        }
        if let Some(old) = expected {
            let displaced = self.read_at(&parent, &temporary, MAX_FILE);
            let matches = displaced.as_ref().is_ok_and(|s| {
                s.as_ref()
                    .is_some_and(|s| old.stamp.renamed(&s.stamp) && old.bytes == s.bytes)
            });
            if !matches {
                // Do not delete the unexpected displaced target. Exchange back
                // only while our exact staged inode is still at the live name.
                let live = self.read_at(&parent, &name, MAX_FILE);
                if live.as_ref().is_ok_and(|s| {
                    s.as_ref()
                        .is_some_and(|s| staged.stamp.renamed(&s.stamp) && s.bytes == bytes)
                }) && rustix::fs::renameat_with(
                    &parent,
                    &temporary,
                    &parent,
                    &name,
                    RenameFlags::EXCHANGE,
                )
                .is_ok()
                {
                    self.cleanup_created(&parent, &temporary, &stage, &parent_path)?;
                    parent.sync_all().map_err(|_| Error::Unknown)?;
                    return Err(Error::Conflict);
                }
                return Err(Error::Unknown);
            }
            rustix::fs::unlinkat(&parent, &temporary, AtFlags::empty())
                .map_err(|_| Error::Unknown)?;
        }
        parent.sync_all().map_err(|_| Error::Unknown)?;
        self.parent_still(&parent_path, &parent)
            .map_err(|_| Error::Unknown)?;
        let installed = self
            .read_at(&parent, &name, MAX_FILE)
            .map_err(|_| Error::Unknown)?
            .ok_or(Error::Unknown)?;
        if !staged.stamp.renamed(&installed.stamp)
            || installed.bytes != bytes
            || installed.stamp.mode != mode
        {
            return Err(Error::Unknown);
        }
        self.record_removed(&staged_path, None, true)?;
        self.record_file(path, expected.map(|f| &f.stamp), &installed, false)?;
        Ok(installed)
    }
    fn cleanup_created(
        &self,
        parent: &File,
        name: &str,
        held: &File,
        parent_path: &str,
    ) -> Result<()> {
        let expected: Stamp = held.metadata().map_err(|_| Error::Unknown)?.into();
        let current = self
            .read_at(parent, name, MAX_FILE)
            .map_err(|_| Error::Unknown)?;
        let Some(current) = current else {
            return Err(Error::Unknown);
        };
        if !expected.identity(&current.stamp) {
            return Err(Error::Unknown);
        }
        rustix::fs::unlinkat(parent, name, AtFlags::empty()).map_err(|_| Error::Unknown)?;
        parent.sync_all().map_err(|_| Error::Unknown)?;
        self.record_removed(
            &format!("{}/{}", parent_path.trim_end_matches('/'), name),
            None,
            true,
        )
    }
    pub fn lock_fixed(&self, path: &str) -> Result<File> {
        let (parent, name, parent_path) = self.parent(path)?;
        let (fd, created) = match rustix::fs::openat(
            &parent,
            &name,
            OFlags::RDONLY
                | OFlags::CREATE
                | OFlags::EXCL
                | OFlags::NOFOLLOW
                | OFlags::NONBLOCK
                | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        ) {
            Ok(fd) => (fd, true),
            Err(e) if e == rustix::io::Errno::EXIST => (
                rustix::fs::openat(
                    &parent,
                    &name,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| Error::Denied)?,
                false,
            ),
            Err(_) => return Err(Error::Denied),
        };
        let file = File::from(fd);
        if created {
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| Error::Unknown)?;
            let metadata = file.metadata().map_err(|_| Error::Unknown)?;
            if metadata.uid() != self.owner || metadata.gid() != self.group {
                rustix::fs::fchown(
                    &file,
                    Some(rustix::process::Uid::from_raw(self.owner)),
                    Some(rustix::process::Gid::from_raw(self.group)),
                )
                .map_err(|_| Error::Unknown)?;
            }
            file.sync_all().map_err(|_| Error::Unknown)?;
        }
        let m = file.metadata().map_err(|_| Error::Io)?;
        if !m.is_file()
            || m.nlink() != 1
            || m.uid() != self.owner
            || m.gid() != self.group
            || m.mode() & 0o7777 != 0o600
            || m.len() > 32
        {
            return Err(Error::Denied);
        }
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| Error::Busy)?;
        self.parent_still(&parent_path, &parent)?;
        let fresh = self.read_at(&parent, &name, 32)?.ok_or(Error::Conflict)?;
        if !Stamp::from(file.metadata().map_err(|_| Error::Io)?).identity(&fresh.stamp) {
            return Err(Error::Conflict);
        }
        parent.sync_all().map_err(|_| Error::Io)?;
        if created {
            self.record_file(path, None, &fresh, false)?;
        }
        Ok(file)
    }
    pub fn remove(&self, path: &str, expected: &FileSnapshot) -> Result<()> {
        let (parent, name, parent_path) = self.parent(path)?;
        self.recheck(path, Some(expected))?;
        let temporary = temporary_name()?;
        rustix::fs::renameat_with(&parent, &name, &parent, &temporary, RenameFlags::NOREPLACE)
            .map_err(|e| {
                if [
                    rustix::io::Errno::NOSYS,
                    rustix::io::Errno::INVAL,
                    rustix::io::Errno::OPNOTSUPP,
                ]
                .contains(&e)
                {
                    Error::Unsupported
                } else {
                    Error::Conflict
                }
            })?;
        let moved = self
            .read_at(&parent, &temporary, MAX_FILE)
            .map_err(|_| Error::Unknown)?;
        if !moved
            .as_ref()
            .is_some_and(|s| expected.stamp.renamed(&s.stamp) && expected.bytes == s.bytes)
        {
            // Never replace a subsequently created live target during undo.
            let _ = rustix::fs::renameat_with(
                &parent,
                &temporary,
                &parent,
                &name,
                RenameFlags::NOREPLACE,
            );
            let _ = parent.sync_all();
            return Err(Error::Unknown);
        }
        rustix::fs::unlinkat(&parent, &temporary, AtFlags::empty()).map_err(|_| Error::Unknown)?;
        parent.sync_all().map_err(|_| Error::Unknown)?;
        self.parent_still(&parent_path, &parent)
            .map_err(|_| Error::Unknown)?;
        self.record_removed(path, Some(&expected.stamp), false)
    }
}
impl Root {
    /// Only the two directories present in the signed fixed tmpfiles template.
    /// IDs come from the independently observed approval policy, not the bundle.
    pub fn runtime_directory(&self, recovery: bool, approved_gid: u32, create: bool) -> Result<()> {
        if matches!(approved_gid, 0 | 29 | 983..=987 | 65534 | u32::MAX) {
            return Err(Error::Denied);
        }
        self.fixed_runtime_directory(
            "/run/rustsimhub",
            if recovery { "recovery" } else { "nkn" },
            986,
            approved_gid,
            0o2750,
            create,
        )
    }
    /// Explicit health-profile additions, never a general tmpfiles executor.
    pub(crate) fn health_directories(&self, create: bool) -> Result<()> {
        self.fixed_runtime_directory("/run/rustsimhub", "update-health", 986, 986, 0o700, create)?;
        self.fixed_runtime_directory(
            "/var/lib/rustsimhub/rustsimhub-v1/system",
            "update-health",
            0,
            0,
            0o700,
            create,
        )
    }
    fn fixed_runtime_directory(
        &self,
        parent_path: &str,
        name: &str,
        uid: u32,
        gid: u32,
        mode: u32,
        create: bool,
    ) -> Result<()> {
        if create {
            self.ensure_dir(parent_path, 0o755)?;
        }
        let parent = self.open_dir(parent_path)?;
        #[cfg(test)]
        let (uid, gid) = if self.isolated && self.owner != 0 {
            (self.owner, self.group)
        } else {
            (uid, gid)
        };
        let existing = rustix::fs::statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW);
        match existing {
            Ok(stat) => {
                if stat.st_mode & 0o170000 != 0o040000
                    || stat.st_uid != uid
                    || stat.st_gid != gid
                    || stat.st_mode & 0o7777 != mode
                {
                    return Err(Error::Denied);
                }
            }
            Err(e) if e == rustix::io::Errno::NOENT && create => {
                rustix::fs::mkdirat(&parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR)
                    .map_err(|_| Error::Io)?;
                parent.sync_all().map_err(|_| Error::Unknown)?;
                let fd = rustix::fs::openat(
                    &parent,
                    name,
                    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .map_err(|_| Error::Unknown)?;
                let directory = File::from(fd);
                let m = directory.metadata().map_err(|_| Error::Unknown)?;
                if m.uid() != self.owner || m.mode() & 0o7777 != 0o700 {
                    return Err(Error::Unknown);
                }
                if m.uid() != uid || m.gid() != gid {
                    rustix::fs::fchown(
                        &directory,
                        Some(rustix::process::Uid::from_raw(uid)),
                        Some(rustix::process::Gid::from_raw(gid)),
                    )
                    .map_err(|_| Error::Unknown)?;
                }
                directory
                    .set_permissions(std::fs::Permissions::from_mode(mode))
                    .map_err(|_| Error::Unknown)?;
                directory.sync_all().map_err(|_| Error::Unknown)?;
                parent.sync_all().map_err(|_| Error::Unknown)?;
                self.record_directory(&format!("{parent_path}/{name}"), &directory)?;
            }
            Err(_) => return Err(Error::Denied),
        }
        let stat = rustix::fs::statat(&parent, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| Error::Unknown)?;
        if stat.st_mode & 0o170000 != 0o040000
            || stat.st_mode & 0o7777 != mode
            || stat.st_uid != uid
            || stat.st_gid != gid
        {
            return Err(Error::Conflict);
        }
        self.parent_still(parent_path, &parent)
    }
}

fn components(path: &str) -> Result<Vec<&str>> {
    if path == "/" {
        return Ok(Vec::new());
    }
    if !path.starts_with('/') || path.len() > 512 {
        return Err(Error::Denied);
    }
    let parts = path[1..].split('/').collect::<Vec<_>>();
    if parts.iter().any(|c| {
        c.is_empty()
            || *c == "."
            || *c == ".."
            || c.len() > 255
            || !c
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    }) {
        return Err(Error::Denied);
    }
    Ok(parts)
}
fn temporary_name() -> Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Io)?;
    Ok(format!(
        ".product-stage-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
