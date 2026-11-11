// SPDX-License-Identifier: AGPL-3.0-only
//! Continuous original maintenance ownership and bounded executor Root changes.
use super::*;
use crate::product_materials::{
    FencePhase, LiveInstallationFence, OwnedNodeFact, OwnedRootChanges,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::OsString,
    time::{Duration, Instant},
};

/// Implemented only by the fixed production transaction, which retains actual
/// installed trust, signed application/slot, stopped units, NSS and backups.
/// No public JSON or arbitrary-path constructor is available.
pub(crate) trait RetainedAuthority {
    fn check_source(&self) -> Result<()>;
    fn transaction(&self) -> &str;
    fn profile(&self) -> crate::product_materials::Profile;
    fn revalidate(&self, phase: FencePhase, changes: &OwnedRootChanges<'_>) -> Result<()>;
    fn runtime_directory(&self, path: &str) -> Option<(u32, u32, u32)>;
}
#[derive(Clone, PartialEq, Eq)]
struct Entry {
    stamp: Stamp,
    link: Option<PathBuf>,
}
impl Entry {
    fn capture(path: &Path) -> Result<Self> {
        let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
        let link = if m.file_type().is_symlink() {
            Some(fs::read_link(path).map_err(|_| Error::Io)?)
        } else {
            None
        };
        Ok(Self {
            stamp: m.into(),
            link,
        })
    }
    fn unchanged(&self, other: &Self) -> bool {
        if self.stamp.mode & 0o170000 == 0o040000 {
            // Contents of an unrelated existing child directory are not a
            // blanket Root fact; explicitly watched directories have their
            // OWN complete bounded inventory below.
            identity(&self.stamp, &other.stamp)
                && self.stamp.mode == other.stamp.mode
                && self.stamp.uid == other.stamp.uid
                && self.stamp.gid == other.stamp.gid
        } else {
            self == other
        }
    }
}
struct Directory {
    file: File,
    stamp: Stamp,
    entries: BTreeMap<OsString, Entry>,
}
fn identity(a: &Stamp, b: &Stamp) -> bool {
    a.dev == b.dev && a.ino == b.ino
}
fn entries(path: &Path) -> Result<BTreeMap<OsString, Entry>> {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(path).map_err(|_| Error::Io)? {
        if entries.len() >= 4096 {
            return Err(Error::Invalid);
        }
        let entry = entry.map_err(|_| Error::Io)?;
        entries.insert(entry.file_name(), Entry::capture(&entry.path())?);
    }
    Ok(entries)
}
fn current(path: &Path) -> Result<Option<Stamp>> {
    Ok(Observation::capture(path)?.stamp)
}
fn matches_proof(
    stamp: &Stamp,
    proof: &crate::product_materials::OwnedNodeStamp,
    directory: bool,
) -> bool {
    stamp.dev == proof.device
        && stamp.ino == proof.inode
        && stamp.uid == proof.uid
        && stamp.gid == proof.gid
        && stamp.mode & 0o7777 == proof.mode
        && if directory {
            stamp.mode & 0o170000 == 0o040000
        } else {
            stamp.mode & 0o170000 == 0o100000
                && stamp.links == proof.links
                && stamp.size == proof.size
                && stamp.modified == proof.mtime
                && stamp.changed == proof.ctime
        }
}
// rootio stays private in production; compare via the public OwnedNodeFact
// projection while its opaque source has already rechecked the actual fds.
pub(crate) struct DataFence<'a> {
    snapshot: RefCell<Option<MaintenanceSnapshot>>,
    authority: &'a dyn RetainedAuthority,
    root: PathBuf,
    original: BTreeMap<PathBuf, Option<Stamp>>,
    directories: BTreeMap<PathBuf, Directory>,
    started: Instant,
    invalid: Cell<bool>,
}
impl<'a> DataFence<'a> {
    pub(crate) fn new(
        snapshot: MaintenanceSnapshot,
        authority: &'a dyn RetainedAuthority,
    ) -> Result<Self> {
        Self::at(snapshot, authority, Path::new("/"))
    }
    fn at(
        mut snapshot: MaintenanceSnapshot,
        authority: &'a dyn RetainedAuthority,
        root: &Path,
    ) -> Result<Self> {
        snapshot.revalidate()?;
        if !crate::hex_digest(authority.transaction()) {
            return Err(Error::Invalid);
        }
        // Pin every finite installer target, including currently absent nodes,
        // BEFORE the executor creates a journal or stages a replacement.
        let tx = format!(
            "/var/lib/rustsimhub/rustsimhub-v1/system/product-install/transactions/{}",
            authority.transaction()
        );
        let mut extra = authority
            .profile()
            .targets()
            .iter()
            .map(|s| s.path.to_owned())
            .collect::<Vec<_>>();
        extra.extend([
            "/var/lib/rustsimhub/rustsimhub-v1/system/product-install/.install.lock".into(),
            "/var/lib/rustsimhub/rustsimhub-v1/system/product-install/active.json".into(),
            format!("{tx}/journal.json"),
        ]);
        for path in extra {
            let target = root.join(path.trim_start_matches('/'));
            for parent in target.ancestors().skip(1) {
                if !parent.starts_with(root) {
                    break;
                }
                let observed = Observation::capture(parent)?;
                if let Some(s) = &observed.stamp {
                    if s.mode & 0o170000 != 0o040000
                        || s.uid != rustix::process::geteuid().as_raw()
                        || s.mode & 0o022 != 0
                    {
                        return Err(Error::Denied);
                    }
                }
                snapshot.observations.push(observed);
            }
            let observed = Observation::capture(&target)?;
            if let Some(s) = &observed.stamp {
                if s.mode & 0o170000 != 0o100000
                    || s.uid != rustix::process::geteuid().as_raw()
                    || s.mode & 0o022 != 0
                    || s.links != 1
                {
                    return Err(Error::Denied);
                }
            }
            if let Some(stamp) = &observed.stamp {
                let file = private_file(
                    &target,
                    rustix::process::geteuid().as_raw(),
                    stamp.mode & 0o7777,
                    32 * 1024 * 1024,
                )?;
                if Stamp::from(file.metadata().map_err(|_| Error::Io)?) != *stamp {
                    return Err(Error::Busy);
                }
                if !snapshot
                    ._root_files
                    .iter()
                    .any(|f| f.observed.path == target)
                {
                    snapshot._root_files.push(RootFile {
                        _file: file,
                        observed: Observation {
                            path: observed.path.clone(),
                            stamp: observed.stamp.clone(),
                        },
                    });
                }
            }
            snapshot.observations.push(observed);
        }
        let mut original = BTreeMap::new();
        let mut directories = BTreeMap::new();
        for observed in snapshot
            .observations
            .iter()
            .chain(snapshot._root_files.iter().map(|f| &f.observed))
        {
            original.insert(observed.path.clone(), observed.stamp.clone());
            if observed
                .stamp
                .as_ref()
                .is_some_and(|s| s.mode & 0o170000 == 0o040000)
                && !directories.contains_key(&observed.path)
            {
                let file = denied_io(
                    OpenOptions::new()
                        .read(true)
                        .custom_flags(
                            (rustix::fs::OFlags::DIRECTORY
                                | rustix::fs::OFlags::NOFOLLOW
                                | rustix::fs::OFlags::CLOEXEC)
                                .bits() as i32,
                        )
                        .open(&observed.path),
                )?;
                let stamp = file.metadata().map_err(|_| Error::Io)?.into();
                let inventory = entries(&observed.path)?;
                directories.insert(
                    observed.path.clone(),
                    Directory {
                        file,
                        stamp,
                        entries: inventory,
                    },
                );
            }
        }
        snapshot.revalidate()?;
        Ok(Self {
            snapshot: RefCell::new(Some(snapshot)),
            authority,
            root: root.into(),
            original,
            directories,
            started: Instant::now(),
            invalid: Cell::new(false),
        })
    }
    pub(crate) fn with_snapshot<T>(
        &self,
        work: impl FnOnce(&MaintenanceSnapshot) -> Result<T>,
    ) -> Result<T> {
        if self.invalid.get() || self.started.elapsed() >= Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        self.authority.check_source()?;
        let borrowed = self.snapshot.try_borrow().map_err(|_| Error::Busy)?;
        let snapshot = borrowed.as_ref().ok_or(Error::Busy)?;
        snapshot.revalidate()?;
        let result = work(snapshot)?;
        snapshot.revalidate()?;
        self.authority.check_source()?;
        Ok(result)
    }
    pub(crate) fn into_snapshot(self) -> Result<MaintenanceSnapshot> {
        if self.invalid.get() || self.started.elapsed() >= Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        self.authority.check_source()?;
        let snapshot = self.snapshot.into_inner().ok_or(Error::Busy)?;
        snapshot.revalidate()?;
        Ok(snapshot)
    }
    fn named(&self, path: &str) -> Result<PathBuf> {
        if !path.starts_with('/')
            || Path::new(path).components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(Error::Denied);
        }
        Ok(self.root.join(path.trim_start_matches('/')))
    }
    fn verify_fact(&self, fact: &OwnedNodeFact) -> Result<()> {
        let path = self.named(&fact.path)?;
        if let Some(original) = self.original.get(&path) {
            match (original, &fact.before) {
                (None, None) => (),
                (Some(stamp), Some(before)) if matches_proof(stamp, before, fact.directory) => (),
                _ => return Err(Error::Integrity),
            }
        }
        match (current(&path)?, &fact.after) {
            (None, None) => Ok(()),
            (Some(stamp), Some(after)) if matches_proof(&stamp, after, fact.directory) => {
                if fact.directory {
                    let base = "/var/lib/rustsimhub/rustsimhub-v1/system/product-install";
                    let tx = format!("{base}/transactions/{}", self.authority.transaction());
                    let expected_mode = if fact.path == base
                        || fact.path == tx
                        || fact.path == format!("{tx}/materials")
                        || fact.path == format!("{tx}/before")
                    {
                        0o700
                    } else {
                        0o755
                    };
                    let ordinary = stamp.uid == rustix::process::geteuid().as_raw()
                        && stamp.mode & 0o7777 == expected_mode;
                    let runtime = self.authority.runtime_directory(&fact.path).is_some_and(
                        |(uid, gid, mode)| {
                            stamp.uid == uid && stamp.gid == gid && stamp.mode & 0o7777 == mode
                        },
                    );
                    if !ordinary && !runtime {
                        return Err(Error::Denied);
                    }
                }
                Ok(())
            }
            _ => Err(Error::Integrity),
        }
    }
    fn directory(
        &self,
        path: &Path,
        directory: &Directory,
        facts: &BTreeMap<PathBuf, &OwnedNodeFact>,
        versions: &BTreeMap<PathBuf, crate::product_materials::OwnedDirectoryVersion>,
    ) -> Result<()> {
        let now = current(path)?.ok_or(Error::Integrity)?;
        let held: Stamp = directory.file.metadata().map_err(|_| Error::Io)?.into();
        if !identity(&directory.stamp, &now)
            || !identity(&held, &now)
            || directory.stamp.mode != now.mode
            || directory.stamp.uid != now.uid
            || directory.stamp.gid != now.gid
        {
            return Err(Error::Integrity);
        }
        if now != directory.stamp {
            let version = versions.get(path).ok_or(Error::Integrity)?;
            if !matches_proof(&now, &version.stamp, true)
                || now.modified != version.stamp.mtime
                || now.changed != version.stamp.ctime
                || now.size != version.stamp.size
                || now.links != version.stamp.links
            {
                return Err(Error::Integrity);
            }
        }
        let current = entries(path)?;
        for name in directory.entries.keys().chain(current.keys()) {
            let full = path.join(name);
            if let Some(fact) = facts.get(&full) {
                self.verify_fact(fact)?;
                continue;
            }
            match (directory.entries.get(name), current.get(name)) {
                (Some(before), Some(after)) if before.unchanged(after) => (),
                _ => return Err(Error::Integrity),
            }
        }
        Ok(())
    }
    fn reconcile(&self, phase: FencePhase, changes: &OwnedRootChanges<'_>) -> Result<()> {
        if self.invalid.get() || self.started.elapsed() >= Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        self.authority.revalidate(phase, changes)?;
        if changes
            .transaction()
            .is_some_and(|s| s != self.authority.transaction())
        {
            return Err(Error::Integrity);
        }
        let facts = changes.operation_facts().map_err(|_| Error::Integrity)?;
        let mut by_path = BTreeMap::new();
        for fact in &facts {
            if !changes.contains_operation(fact) || !changes.validates_target_fact(fact) {
                return Err(Error::Denied);
            }
            self.verify_fact(fact)?;
            by_path.insert(self.named(&fact.path)?, fact);
        }
        let mut versions = BTreeMap::new();
        for version in changes.directory_versions().map_err(|_| Error::Integrity)? {
            let path = self.named(&version.path)?;
            if !by_path.keys().any(|p| p.parent() == Some(path.as_path())) {
                return Err(Error::Denied);
            }
            versions.insert(path, version);
        }
        for (path, directory) in &self.directories {
            self.directory(path, directory, &by_path, &versions)?;
        }
        // A newly created directory has no old child inventory: ONLY actual
        // operations proven by this executor may occur inside it.
        for (path, fact) in &by_path {
            if fact.directory
                && fact.after.is_some()
                && fact.before.is_none()
                && !self.directories.contains_key(path)
            {
                for name in entries(path)?.keys() {
                    if !by_path.contains_key(&path.join(name)) {
                        return Err(Error::Integrity);
                    }
                }
            }
        }
        let mut held = self.snapshot.try_borrow_mut().map_err(|_| Error::Busy)?;
        let snapshot = held.as_mut().ok_or(Error::Busy)?;
        for observed in &mut snapshot.observations {
            if by_path.contains_key(&observed.path) || self.directories.contains_key(&observed.path)
            {
                observed.stamp = current(&observed.path)?;
            } else {
                observed.revalidate()?;
            }
        }
        for file in &mut snapshot._root_files {
            if let Some(fact) = by_path.get(&file.observed.path) {
                if fact.after.is_none() {
                    return Err(Error::Integrity);
                }
                let mode = fact.after.as_ref().ok_or(Error::Integrity)?.mode;
                let actual = private_file(
                    &file.observed.path,
                    rustix::process::geteuid().as_raw(),
                    mode,
                    32 * 1024 * 1024,
                )?;
                file._file = actual;
                file.observed.stamp = current(&file.observed.path)?;
            } else {
                file.revalidate()?;
            }
        }
        // Transfer actual descriptors for newly created fixed Root targets
        // into the SAME original snapshot before the installer is dropped.
        // Facts are still borrowed from this executor and have already passed
        // its held-fd/full-generation proof; serialized names cannot add files.
        for (path, fact) in &by_path {
            if fact.directory
                || fact.after.is_none()
                || snapshot
                    ._root_files
                    .iter()
                    .any(|f| &f.observed.path == path)
                || !self
                    .authority
                    .profile()
                    .targets()
                    .iter()
                    .any(|s| self.root.join(s.path.trim_start_matches('/')) == *path)
            {
                continue;
            }
            let observed = Observation::capture(path)?;
            let stamp = observed.stamp.as_ref().ok_or(Error::Integrity)?;
            if !matches_proof(stamp, fact.after.as_ref().ok_or(Error::Integrity)?, false) {
                return Err(Error::Integrity);
            }
            let file = private_file(
                path,
                rustix::process::geteuid().as_raw(),
                stamp.mode & 0o7777,
                32 * 1024 * 1024,
            )?;
            if Stamp::from(file.metadata().map_err(|_| Error::Io)?) != *stamp {
                return Err(Error::Busy);
            }
            snapshot._root_files.push(RootFile {
                _file: file,
                observed,
            });
        }
        snapshot.revalidate()?;
        self.authority.revalidate(phase, changes)?;
        if self.started.elapsed() >= Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        Ok(())
    }
}
impl LiveInstallationFence for DataFence<'_> {
    fn revalidate(
        &self,
        phase: FencePhase,
        changes: &OwnedRootChanges<'_>,
    ) -> crate::product_materials::Result<()> {
        match self.reconcile(phase, changes) {
            Ok(()) => Ok(()),
            Err(_) => {
                self.invalid.set(true);
                Err(crate::product_materials::Error::Denied)
            }
        }
    }
}

#[cfg(test)]
mod tests;
