// SPDX-License-Identifier: AGPL-3.0-only
//! Protected fixed slots and a Root durable at-most-once intent. No arbitrary path API on the wire.
use super::*;
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
#[derive(Clone, Copy)]
pub enum Slot {
    Hint,
    Status,
    Availability,
}
impl Slot {
    fn name(self) -> &'static str {
        match self {
            Self::Hint => "hint.json",
            Self::Status => "status.json",
            Self::Availability => "availability.json",
        }
    }
}
#[derive(Clone)]
pub struct Directory {
    path: PathBuf,
    owner: u32,
    group: u32,
    mode: u32,
}
impl Directory {
    /// Paths/identity are supplied by the trusted service configuration, never a network request.
    pub fn open(path: &Path, owner: u32, group: u32, mode: u32) -> Result<Self> {
        if !path.is_absolute() || ![0o700, 0o2750].contains(&mode) {
            return Err(Error::Invalid);
        }
        let dir = Self {
            path: path.into(),
            owner,
            group,
            mode,
        };
        dir.check()?;
        Ok(dir)
    }
    fn check(&self) -> Result<()> {
        let m = fs::symlink_metadata(&self.path).map_err(|_| Error::Io)?;
        if !m.is_dir()
            || m.uid() != self.owner
            || m.gid() != self.group
            || m.mode() & 0o7777 != self.mode
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn flags() -> i32 {
        (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32
    }
    fn metadata(&self, file: &File, mode: u32) -> Result<()> {
        let m = file.metadata().map_err(|_| Error::Io)?;
        if !m.is_file()
            || m.uid() != self.owner
            || m.gid() != self.group
            || m.mode() & 0o7777 != mode
            || m.nlink() != 1
            || m.len() > MAX_BYTES as u64
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    fn read_name(&self, name: &str, mode: u32) -> Result<Option<Vec<u8>>> {
        self.check()?;
        let f = match OpenOptions::new()
            .read(true)
            .custom_flags(Self::flags())
            .open(self.path.join(name))
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Io),
        };
        self.metadata(&f, mode)?;
        let mut b = Vec::new();
        f.take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut b)
            .map_err(|_| Error::Io)?;
        if b.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        Ok(Some(b))
    }
    fn write_name(&self, name: &str, bytes: &[u8], mode: u32, new: bool) -> Result<()> {
        self.check()?;
        if self.owner != rustix::process::geteuid().as_raw() || bytes.len() > MAX_BYTES {
            return Err(Error::Denied);
        }
        if new {
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(mode)
                .custom_flags(Self::flags())
                .open(self.path.join(name))
                .map_err(|_| Error::Busy)?;
            f.set_permissions(fs::Permissions::from_mode(mode))
                .map_err(|_| Error::Io)?;
            self.metadata(&f, mode)?;
            f.write_all(bytes).map_err(|_| Error::Io)?;
            f.sync_all().map_err(|_| Error::Io)?;
        } else {
            // Validate an existing destination without following links, then atomically publish.
            self.read_name(name, mode)?;
            let mut random = [0u8; 16];
            getrandom::fill(&mut random).map_err(|_| Error::Io)?;
            let suffix = random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            let pending = format!(".{name}.{suffix}.pending");
            let mut f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(mode)
                .custom_flags(Self::flags())
                .open(self.path.join(&pending))
                .map_err(|_| Error::Io)?;
            f.set_permissions(fs::Permissions::from_mode(mode))
                .map_err(|_| Error::Io)?;
            self.metadata(&f, mode)?;
            f.write_all(bytes).map_err(|_| Error::Io)?;
            f.sync_all().map_err(|_| Error::Io)?;
            drop(f);
            fs::rename(self.path.join(pending), self.path.join(name)).map_err(|_| Error::Io)?;
        }
        File::open(&self.path)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)
    }
    pub fn read(&self, slot: Slot) -> Result<Option<Vec<u8>>> {
        self.read_name(slot.name(), 0o640)
    }
    pub fn publish<T: Serialize>(&self, slot: Slot, value: &T) -> Result<()> {
        self.write_name(
            slot.name(),
            &serde_json::to_vec(value).map_err(|_| Error::Invalid)?,
            0o640,
            false,
        )
    }
}

pub struct Journal {
    dir: Directory,
    _lock: File,
}
impl Journal {
    pub fn root(path: &Path) -> Result<Self> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        Self::open(Directory::open(path, 0, 0, 0o700)?)
    }
    fn open(dir: Directory) -> Result<Self> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(Directory::flags())
            .open(dir.path.join(".lock"))
            .map_err(|_| Error::Io)?;
        dir.metadata(&f, 0o600)?;
        f.try_lock_exclusive().map_err(|_| Error::Busy)?;
        Ok(Self { dir, _lock: f })
    }
    pub fn pending(&self) -> Result<Option<Binding>> {
        let Some(b) = self.dir.read_name("active.json", 0o600)? else {
            return Ok(None);
        };
        let v: Binding = serde_json::from_slice(&b).map_err(|_| Error::Invalid)?;
        if !v.valid() {
            return Err(Error::Denied);
        }
        Ok(Some(v))
    }
    /// Must succeed before firewall/NetworkManager side effects. Existing intents never replay.
    pub fn consume(&self, binding: &Binding) -> Result<()> {
        let key = binding.journal_key()?;
        if self.pending()?.is_some() {
            return Err(Error::Busy);
        }
        let count = fs::read_dir(&self.dir.path)
            .map_err(|_| Error::Io)?
            .take(513)
            .count();
        if count >= 512 {
            return Err(Error::Busy);
        }
        let bytes = serde_json::to_vec(binding).map_err(|_| Error::Invalid)?;
        self.dir
            .write_name(&format!("{key}.consumed"), &bytes, 0o600, true)?;
        self.dir.write_name("active.json", &bytes, 0o600, true)
    }
    /// No Unknown or unverified cleanup can clear the pending marker.
    pub fn finish_closed(&self, status: &Status, now: u64) -> Result<()> {
        let binding = self.pending()?.ok_or(Error::Denied)?;
        status.validate(&binding, now)?;
        if status.state != State::Closed {
            return Err(Error::Denied);
        }
        let key = binding.journal_key()?;
        self.dir.write_name(
            &format!("{key}.closed"),
            &serde_json::to_vec(status).map_err(|_| Error::Invalid)?,
            0o600,
            true,
        )?;
        fs::remove_file(self.dir.path.join("active.json")).map_err(|_| Error::Io)?;
        File::open(&self.dir.path)
            .and_then(|d| d.sync_all())
            .map_err(|_| Error::Io)
    }
}

#[cfg(test)]
mod tests;
