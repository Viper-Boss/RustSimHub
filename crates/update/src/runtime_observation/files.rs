// SPDX-License-Identifier: AGPL-3.0-only
use super::{ReadOnlyState, Result};
use crate::{
    application_files, manifest_hash, store::read_checked, verify, verify_payload, Error, Manifest,
    MAX_MANIFEST,
};
use std::{
    fs::{self, Metadata},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

const SELECTED: &str = "/run/rustsimhub-app/selected";
const VERSIONS: &str = "/opt/rustsimhub/versions";

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Stamp {
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
struct Guard {
    path: PathBuf,
    stamp: Stamp,
}
impl Guard {
    fn capture(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            stamp: fs::symlink_metadata(path).map_err(|_| Error::Io)?.into(),
        })
    }
    fn check(&self) -> Result<()> {
        if Stamp::from(fs::symlink_metadata(&self.path).map_err(|_| Error::Io)?) != self.stamp {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}
pub(super) struct Slot {
    pub(super) manifest: Manifest,
    pub(super) hash: String,
    pub(super) attempted: bool,
    pub(super) candidate_present: bool,
    pub(super) directory: PathBuf,
    selected: PathBuf,
    guards: Vec<Guard>,
}
impl Slot {
    pub(super) fn observe(ledger: &ReadOnlyState) -> Result<Self> {
        // All ancestors are actual Root-owned directories. No relative roots,
        // mutable search path or caller-controlled symlink traversal.
        let ancestors = [
            "/",
            "/var",
            "/var/lib",
            "/var/lib/rustsimhub",
            "/var/lib/rustsimhub/rustsimhub-v1",
            "/var/lib/rustsimhub/rustsimhub-v1/system",
            "/usr",
            "/usr/share",
            "/usr/share/rustsimhub",
            "/run",
            "/opt",
            "/opt/rustsimhub",
        ];
        Self::at(
            ledger,
            Path::new(SELECTED),
            Path::new(VERSIONS),
            &ancestors.map(Path::new),
        )
    }
    pub(super) fn at(
        ledger: &ReadOnlyState,
        selected: &Path,
        versions: &Path,
        ancestors: &[&Path],
    ) -> Result<Self> {
        let mut guards = Vec::new();
        for directory in ancestors
            .iter()
            .copied()
            .chain([selected.parent().ok_or(Error::Denied)?, versions])
        {
            directory_guard(directory, &mut guards)?;
        }
        absent(&selected.with_file_name("selected.next"))?;
        let (hash, epoch, attempted, candidate_present) = ledger.runtime_version()?;
        let target = versions.join(hash);
        let link = Guard::capture(selected)?;
        let metadata = fs::symlink_metadata(selected).map_err(|_| Error::Io)?;
        if !metadata.file_type().is_symlink()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || fs::read_link(selected).map_err(|_| Error::Io)? != target
        {
            return Err(Error::Denied);
        }
        guards.push(link);
        directory_guard(&target, &mut guards)?;
        let bytes = read(&target.join("manifest.json"), MAX_MANIFEST, &mut guards)?;
        if manifest_hash(&bytes) != hash {
            return Err(Error::Integrity);
        }
        let signature = read(&target.join("manifest.sig"), 64, &mut guards)?;
        // The ledger has already consumed the floor on staging. This validates
        // the installed group's exact ledger epoch, NOT admission of a new one.
        let manifest = verify(&bytes, &signature, ledger.runtime_key()?, 6, 0)?;
        if manifest.epoch != epoch {
            return Err(Error::Integrity);
        }
        let names = application_files(manifest.format)?;
        let expected: std::collections::BTreeSet<_> = names
            .iter()
            .copied()
            .chain(["manifest.json", "manifest.sig"])
            .collect();
        let mut count = 0;
        for entry in fs::read_dir(&target).map_err(|_| Error::Io)? {
            let entry = entry.map_err(|_| Error::Io)?;
            count += 1;
            if count > expected.len()
                || !entry
                    .file_name()
                    .to_str()
                    .is_some_and(|n| expected.contains(n))
            {
                return Err(Error::Integrity);
            }
        }
        if count != expected.len() {
            return Err(Error::Integrity);
        }
        let payloads = manifest
            .files
            .iter()
            .map(|e| read(&target.join(&e.name), e.bytes as usize, &mut guards))
            .collect::<Result<Vec<_>>>()?;
        verify_payload(&manifest, &payloads)?;
        let slot = Self {
            manifest,
            hash: hash.into(),
            attempted,
            candidate_present,
            directory: target,
            selected: selected.into(),
            guards,
        };
        slot.revalidate(ledger)?;
        Ok(slot)
    }
    pub(super) fn revalidate(&self, ledger: &ReadOnlyState) -> Result<()> {
        ledger.revalidate()?;
        for guard in &self.guards {
            guard.check()?;
        }
        absent(&self.selected.with_file_name("selected.next"))?;
        if fs::read_link(&self.selected).map_err(|_| Error::Io)? != self.directory {
            return Err(Error::Integrity);
        }
        let (hash, epoch, attempted, candidate) = ledger.runtime_version()?;
        if hash != self.hash
            || epoch != self.manifest.epoch
            || attempted != self.attempted
            || candidate != self.candidate_present
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub(super) fn executable(&self, role: &str) -> Result<(PathBuf, &str)> {
        let entry = self
            .manifest
            .files
            .iter()
            .find(|e| e.name == role)
            .ok_or(Error::Integrity)?;
        Ok((self.directory.join(role), &entry.sha256))
    }
}
fn absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(Error::Busy),
        Err(_) => Err(Error::Io),
    }
}
fn directory_guard(path: &Path, guards: &mut Vec<Guard>) -> Result<()> {
    let guard = Guard::capture(path)?;
    let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if !m.is_dir() || m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o022 != 0 {
        return Err(Error::Denied);
    }
    guard.check()?;
    guards.push(guard);
    Ok(())
}
fn read(path: &Path, max: usize, guards: &mut Vec<Guard>) -> Result<Vec<u8>> {
    let guard = Guard::capture(path)?;
    let bytes = read_checked(path, max)?;
    guard.check()?;
    guards.push(guard);
    Ok(bytes)
}
