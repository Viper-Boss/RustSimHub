// SPDX-License-Identifier: AGPL-3.0-only
//! Inspect an existing ledger without Store::open's recovery/creation side
//! effects. This is NOT package, runtime health or maintenance authorization.
use super::{dir, libc_flags, metadata, parse_state, State, StoreLock};
use crate::{manifest_hash, Error};
use fs2::FileExt;
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const ROOT: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/app";
const TRUST: &str = "/usr/share/rustsimhub/release.public";

pub(crate) mod confirmation;

type Result<T> = std::result::Result<T, Error>;

#[derive(PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    links: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl From<Metadata> for Stamp {
    fn from(m: Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            mode: m.mode(),
            uid: m.uid(),
            gid: m.gid(),
            links: m.nlink(),
            size: m.len(),
            modified: (m.mtime(), m.mtime_nsec()),
            changed: (m.ctime(), m.ctime_nsec()),
        }
    }
}
struct Observed {
    path: PathBuf,
    stamp: Stamp,
}
impl Observed {
    fn capture(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            stamp: fs::symlink_metadata(path).map_err(|_| Error::Io)?.into(),
        })
    }

    fn check(&self) -> Result<()> {
        let now = fs::symlink_metadata(&self.path).map_err(|_| Error::Io)?;
        if self.stamp != now.into() {
            return Err(Error::Busy);
        }
        Ok(())
    }
}

fn open(path: &Path, modes: &[u32]) -> Result<File> {
    dir(path.parent().ok_or(Error::Denied)?)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc_flags())
        .open(path)
        .map_err(|_| Error::Io)?;
    metadata(&file)?;
    if !modes.contains(&(file.metadata().map_err(|_| Error::Io)?.mode() & 0o7777)) {
        return Err(Error::Denied);
    }
    Ok(file)
}

fn read(path: &Path, modes: &[u32], limit: usize) -> Result<(Vec<u8>, Observed)> {
    let file = open(path, modes)?;
    let stamp: Stamp = file.metadata().map_err(|_| Error::Io)?.into();
    if stamp.size > limit as u64 {
        return Err(Error::Invalid);
    }
    let observed = Observed {
        path: path.into(),
        stamp,
    };
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > limit {
        return Err(Error::Invalid);
    }
    observed.check()?;
    Ok((bytes, observed))
}

/// Keeps a shared, nonblocking lock on the EXISTING updater lock file. Missing
/// files are errors; a pending state.next needs an explicit recovery action.
/// Hold this object through consuming its report. It is only a point-in-time
/// ledger observation, not a mutation capability or candidate-health verdict.
pub struct ReadOnlyState {
    root: PathBuf,
    key: [u8; 32],
    state: State,
    observed: Vec<Observed>,
    _lock: StoreLock,
}

impl ReadOnlyState {
    /// Production paths/trust are fixed and Root-only, like the updater.
    pub fn observe() -> Result<Self> {
        if rustix::process::geteuid().as_raw() != 0 {
            return Err(Error::Denied);
        }
        Self::at(Path::new(ROOT), Path::new(TRUST))
    }

    fn at(root: &Path, trust: &Path) -> Result<Self> {
        if !root.is_absolute() || !trust.is_absolute() {
            return Err(Error::Denied);
        }
        dir(root)?;
        let lock_path = root.join(".update.lock");
        let lock = open(&lock_path, &[0o600])?;
        FileExt::try_lock_shared(&lock).map_err(|_| Error::Busy)?;
        let lock = StoreLock(lock);
        let mut observed = vec![Observed::capture(root)?, Observed::capture(&lock_path)?];
        // The image installs its public trust as 0444. A Root-owned 0644
        // installation is also valid; neither permits group/other writes.
        let (public, public_observed) = read(trust, &[0o444, 0o644], 32)?;
        if public.len() != 32 {
            return Err(Error::Invalid);
        }
        let (bytes, state_observed) = read(&root.join("state.json"), &[0o600], 8192)?;
        let state = parse_state(&bytes)?;
        if state.key_sha256 != manifest_hash(&public) {
            return Err(Error::Denied);
        }
        observed.extend([public_observed, state_observed]);
        let snapshot = Self {
            root: root.into(),
            key: public.as_slice().try_into().map_err(|_| Error::Invalid)?,
            state,
            observed,
            _lock: lock,
        };
        snapshot.revalidate()?;
        Ok(snapshot)
    }

    /// No state.next deletion/promotion, version creation, floor or selection
    /// changes. A substituted lock inode or changed source is not accepted.
    pub fn revalidate(&self) -> Result<()> {
        for item in &self.observed {
            item.check()?;
        }
        let fd_stamp: Stamp = self._lock.0.metadata().map_err(|_| Error::Io)?.into();
        if fd_stamp != self.observed[1].stamp {
            return Err(Error::Busy);
        }
        match fs::symlink_metadata(self.root.join("state.next")) {
            Ok(_) => Err(Error::Busy),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Error::Io),
        }
    }

    pub fn report(&self) -> Result<serde_json::Value> {
        self.revalidate()?;
        Ok(serde_json::json!({
            "format": 1,
            "scope": "application-ledger",
            "read_only": true,
            "authorization_verified": false,
            "trust_binding_verified": true,
            "application_integrity_verified": false,
            "runtime_selection_verified": false,
            "health_verified": false,
            "state": &self.state,
        }))
    }

    /// The running selection may be the confirmed group while a candidate is
    /// merely staged, or the already-attempted candidate. Never consume it.
    pub(crate) fn runtime_version(&self) -> Result<(&str, u64, bool, bool)> {
        self.revalidate()?;
        let version = if self.state.attempted {
            self.state.candidate.as_ref()
        } else {
            self.state.confirmed.as_ref()
        }
        .ok_or(Error::Denied)?;
        Ok((
            &version.hash,
            version.epoch,
            self.state.attempted,
            self.state.candidate.is_some(),
        ))
    }

    pub(crate) fn pending_trial_hash(&self) -> Result<Option<&str>> {
        self.revalidate()?;
        Ok(self
            .state
            .candidate
            .as_ref()
            .map(|version| version.hash.as_str()))
    }

    pub(crate) fn floor_epoch(&self) -> Result<u64> {
        self.revalidate()?;
        Ok(self.state.floor)
    }

    pub(crate) fn runtime_key(&self) -> Result<&[u8; 32]> {
        self.revalidate()?;
        Ok(&self.key)
    }

    #[cfg(test)]
    pub(crate) fn runtime_test_at(root: &Path, trust: &Path) -> Result<Self> {
        Self::at(root, trust)
    }
}

#[cfg(test)]
mod tests;
