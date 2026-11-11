// SPDX-License-Identifier: AGPL-3.0-only
use super::{ReadOnlyState, Result};
use crate::{
    application_files, manifest_hash,
    store::read_checked,
    verified_application::{VerifiedApplication, VerifiedManifest},
    Error, Manifest, MAX_MANIFEST,
};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const SELECTED: &str = "/run/rustsimhub-app/selected";
const VERSIONS: &str = "/opt/rustsimhub/versions";

#[derive(Debug, Clone, PartialEq, Eq)]
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
    file: Option<File>,
}
impl Guard {
    fn capture(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.into(),
            stamp: fs::symlink_metadata(path).map_err(|_| Error::Io)?.into(),
            file: None,
        })
    }
    fn check(&self) -> Result<()> {
        if Stamp::from(fs::symlink_metadata(&self.path).map_err(|_| Error::Io)?) != self.stamp
            || self.file.as_ref().is_some_and(|file| {
                file.metadata()
                    .map(Stamp::from)
                    .map_or(true, |stamp| stamp != self.stamp)
            })
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}
/// Borrowed only from a signature/payload-verified live slot. The original fd
/// pins its inode, and full mode/owner/link/size/mtime/ctime stamps bind both fd
/// and path. This is never constructed from caller-supplied paths or digests.
pub(super) struct Executable<'a> {
    guard: &'a Guard,
}
impl Executable<'_> {
    pub(super) fn check(&self, process_exe: &File) -> Result<()> {
        self.guard.check()?;
        if Stamp::from(process_exe.metadata().map_err(|_| Error::Io)?) != self.guard.stamp {
            return Err(Error::Integrity);
        }
        Ok(())
    }
    pub(super) fn stamp(&self) -> Stamp {
        self.guard.stamp.clone()
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
    verified: VerifiedApplication,
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
        Self::at_using(
            ledger,
            selected,
            versions,
            ancestors,
            VerifiedManifest::bind,
        )
    }
    fn at_using(
        ledger: &ReadOnlyState,
        selected: &Path,
        versions: &Path,
        ancestors: &[&Path],
        check: fn(VerifiedManifest, Vec<Vec<u8>>) -> Result<VerifiedApplication>,
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
        let signed_manifest = VerifiedManifest::verify(bytes, signature, ledger.runtime_key()?, 6)?;
        let manifest = signed_manifest.manifest().clone();
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
        let verified = check(signed_manifest, payloads)?;
        let slot = Self {
            manifest,
            hash: hash.into(),
            attempted,
            candidate_present,
            directory: target,
            selected: selected.into(),
            guards,
            verified,
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
    pub(super) fn verified(&self) -> &VerifiedApplication {
        &self.verified
    }
    #[cfg(test)]
    pub(super) fn signed(&self) -> Result<crate::store::SignedGroup> {
        Ok(self.verified.group().clone())
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
    pub(super) fn bound_executable(&self, role: &str) -> Result<Executable<'_>> {
        if !super::ROLES
            .iter()
            .any(|(_, executable, _, _)| *executable == role)
        {
            return Err(Error::Invalid);
        }
        let (path, _) = self.executable(role)?;
        let guard = self
            .guards
            .iter()
            .find(|guard| guard.path == path && guard.file.is_some())
            .ok_or(Error::Integrity)?;
        guard.check()?;
        Ok(Executable { guard })
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
    let mut guard = Guard::capture(path)?;
    let bytes = read_checked(path, max)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|_| Error::Io)?;
    guard.file = Some(file);
    guard.check()?;
    guards.push(guard);
    Ok(bytes)
}

#[cfg(test)]
impl Slot {
    pub(super) fn native_fixture(
        ledger: &ReadOnlyState,
        selected: &Path,
        versions: &Path,
    ) -> Result<Self> {
        Self::at_using(
            ledger,
            selected,
            versions,
            &[],
            VerifiedManifest::bind_native,
        )
    }
}
