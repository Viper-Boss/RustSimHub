// SPDX-License-Identifier: AGPL-3.0-only
use crate::{Error, Result};
use ed25519_dalek::{Signer, SigningKey};
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;

/// Never Debug/Serialize/Clone; secret seed remains outside SQLite and backups.
pub struct Identity {
    signing: SigningKey,
}
/// Purpose-limited observer key, without access to device signing or authentication state.
pub struct SimFingerprinter {
    key: Zeroizing<[u8; 32]>,
}
impl SimFingerprinter {
    pub fn fingerprint(&self, iccid: &str) -> Result<[u8; 32]> {
        use hmac::{Hmac, Mac};
        let iccid =
            rustsimhub_domain::canonical_sim_identifier(iccid).ok_or(Error::InvalidInput)?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.key.as_ref()).map_err(|_| Error::InvalidInput)?;
        mac.update(b"RustSimHub.SimFingerprint.v1\0");
        mac.update(iccid.as_bytes());
        Ok(mac.finalize().into_bytes().into())
    }
}
impl Identity {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            signing: open_key(directory.as_ref(), false, true)?,
        })
    }
    pub fn load(directory: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            signing: open_key(directory.as_ref(), false, false)?,
        })
    }
    /// Separate private identity backup, never part of generic firmware/DB archive.
    pub fn backup(&self, directory: impl AsRef<Path>) -> Result<()> {
        let dir = directory.as_ref();
        check(dir, true)?;
        let bytes = encode_seed(&self.signing, b"RSHKEY01");
        write_private_new(&dir.join("identity.seed"), &bytes)?;
        write_private_new(&dir.join(".identity.lock"), b"")
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }
    /// Local SIM pseudonym, never a public identifier. Domain separated HMAC with device seed.
    pub fn sim_fingerprint(&self, iccid: &str) -> Result<[u8; 32]> {
        self.sim_fingerprinter()?.fingerprint(iccid)
    }
    pub fn sim_fingerprinter(&self) -> Result<SimFingerprinter> {
        use hmac::{Hmac, Mac};
        let seed = Zeroizing::new(self.signing.to_bytes());
        let mut mac =
            Hmac::<Sha256>::new_from_slice(seed.as_ref()).map_err(|_| Error::InvalidInput)?;
        mac.update(b"RustSimHub.SimObserverKey.v1\0");
        Ok(SimFingerprinter {
            key: Zeroizing::new(mac.finalize().into_bytes().into()),
        })
    }
    pub(crate) fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.signing.sign(message).to_bytes().to_vec()
    }
}
fn encode_seed(signing: &SigningKey, magic: &[u8; 8]) -> Zeroizing<Vec<u8>> {
    let seed = Zeroizing::new(signing.to_bytes());
    let mut bytes = Zeroizing::new(Vec::with_capacity(72));
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(seed.as_ref());
    let hash = Sha256::digest(&bytes[..]);
    bytes.extend_from_slice(&hash);
    bytes
}
pub(crate) fn open_key(dir: &Path, client: bool, create: bool) -> Result<SigningKey> {
    check(dir, true)?;
    let (seed_name, pending_name, lock_name, magic) = if client {
        ("client.seed", "client.pending", ".client.lock", b"RSHCLI01")
    } else {
        (
            "identity.seed",
            "identity.pending",
            ".identity.lock",
            b"RSHKEY01",
        )
    };
    let lock = if create {
        open_private(&dir.join(lock_name), false)?
    } else {
        open_private_read(&dir.join(lock_name))?
    };
    lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
    let path = dir.join(seed_name);
    let pending = dir.join(pending_name);
    if !path.try_exists().map_err(|_| Error::Io)? {
        // Loading is strictly read-only, including an interrupted creation.
        // Only the explicit create/open path may publish a pending seed.
        if !create {
            return Err(Error::Io);
        }
        if !pending.try_exists().map_err(|_| Error::Io)? {
            let mut seed = Zeroizing::new([0u8; 32]);
            getrandom::fill(seed.as_mut()).map_err(|_| Error::Entropy)?;
            let mut bytes = Zeroizing::new(Vec::with_capacity(72));
            bytes.extend_from_slice(magic);
            bytes.extend_from_slice(seed.as_ref());
            let hash = Sha256::digest(&bytes[..]);
            bytes.extend_from_slice(&hash);
            let mut file = open_private(&pending, true)?;
            file.write_all(&bytes).map_err(|_| Error::Io)?;
            file.sync_all().map_err(|_| Error::Io)?;
        }
        // Complete pending seed survives interruption; malformed evidence is preserved.
        read_seed(&pending, magic)?;
        fs::rename(&pending, &path).map_err(|_| Error::Io)?;
        sync_directory(dir)?;
    }
    let seed = read_seed(&path, magic)?;
    Ok(SigningKey::from_bytes(&seed))
}
fn sync_directory(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)?;
    #[cfg(not(unix))]
    let _ = dir;
    Ok(())
}
pub(super) fn check(path: &Path, directory: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
        return Err(Error::Forbidden);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o077 != 0
            || (!directory && meta.nlink() != 1)
        {
            return Err(Error::Forbidden);
        }
    }
    Ok(())
}
fn open_private(path: &Path, new: bool) -> Result<File> {
    match fs::symlink_metadata(path) {
        Ok(_) => check(path, false)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Error::Io),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let file = options.open(path).map_err(|_| Error::Io)?;
    check(path, false)?;
    Ok(file)
}
fn read_seed(path: &Path, magic: &[u8; 8]) -> Result<Zeroizing<[u8; 32]>> {
    check(path, false)?;
    let mut bytes = Zeroizing::new(Vec::new());
    open_private_read(path)?
        .take(73)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() != 72 || &bytes[..8] != magic || Sha256::digest(&bytes[..40])[..] != bytes[40..]
    {
        return Err(Error::CorruptIdentity);
    }
    let mut seed = Zeroizing::new([0u8; 32]);
    seed.copy_from_slice(&bytes[8..40]);
    Ok(seed)
}

pub fn write_private_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or(Error::InvalidInput)?;
    check(parent, true)?;
    let mut file = open_private(path, true)?;
    file.write_all(bytes).map_err(|_| Error::Io)?;
    file.sync_all().map_err(|_| Error::Io)?;
    sync_directory(parent)
}

#[cfg(target_os = "linux")]
pub fn lock_private_directory(path: &Path) -> Result<File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    check(path, true)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::DIRECTORY | rustix::fs::OFlags::NOFOLLOW).bits() as i32)
        .open(path)
        .map_err(|_| Error::Io)?;
    let meta = file.metadata().map_err(|_| Error::Io)?;
    if !meta.is_dir()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o077 != 0
    {
        return Err(Error::Forbidden);
    }
    file.try_lock_exclusive().map_err(|_| Error::Busy)?;
    Ok(file)
}

/// Publish a complete private directory without replacing an existing target.
#[cfg(target_os = "linux")]
pub fn publish_private_directory(pending: &Path, destination: &Path) -> Result<()> {
    let parent = pending.parent().ok_or(Error::InvalidInput)?;
    if destination.parent() != Some(parent) {
        return Err(Error::InvalidInput);
    }
    check(parent, true)?;
    check(pending, true)?;
    File::open(pending)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)?;
    sync_directory(parent)?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        pending,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|_| Error::Io)?;
    sync_directory(parent)
}
pub fn read_private(path: &Path, limit: usize) -> Result<Zeroizing<Vec<u8>>> {
    check(path.parent().ok_or(Error::InvalidInput)?, true)?;
    check(path, false)?;
    let mut bytes = Zeroizing::new(Vec::new());
    open_private_read(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > limit {
        return Err(Error::InvalidInput);
    }
    Ok(bytes)
}

#[cfg(all(test, target_os = "linux"))]
mod directory_publication_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn directory_lock_and_publication_never_replace_existing_contents() {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let lock = lock_private_directory(d.path()).unwrap();
        assert!(matches!(lock_private_directory(d.path()), Err(Error::Busy)));
        let pending = d.path().join("pending");
        let target = d.path().join("complete");
        for path in [&pending, &target] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        write_private_new(&pending.join("new"), b"new").unwrap();
        write_private_new(&target.join("old"), b"preserve").unwrap();
        assert!(publish_private_directory(&pending, &target).is_err());
        assert_eq!(fs::read(target.join("old")).unwrap(), b"preserve");
        assert!(pending.exists());
        let final_path = d.path().join("final");
        publish_private_directory(&pending, &final_path).unwrap();
        assert_eq!(fs::read(final_path.join("new")).unwrap(), b"new");
        drop(lock);
    }
}

fn open_private_read(path: &Path) -> Result<File> {
    check(path, false)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options.open(path).map_err(|_| Error::Io)?;
    // Validate the actual opened inode too, rather than trusting a second path lookup.
    let meta = file.metadata().map_err(|_| Error::Io)?;
    if !meta.is_file() {
        return Err(Error::Forbidden);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != rustix::process::geteuid().as_raw()
            || meta.mode() & 0o077 != 0
            || meta.nlink() != 1
        {
            return Err(Error::Forbidden);
        }
    }
    Ok(file)
}

#[cfg(all(test, target_os = "linux"))]
mod readonly_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn private_read_needs_no_write_permission_and_never_creates_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("certificate.der");
        write_private_new(&path, b"synthetic certificate").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(
            read_private(&path, 64).unwrap().as_slice(),
            b"synthetic certificate"
        );
        assert!(read_private(&dir.path().join("absent"), 64).is_err());
        assert!(!dir.path().join("absent").exists());
        let linked = dir.path().join("hardlink");
        fs::hard_link(&path, &linked).unwrap();
        assert!(read_private(&path, 64).is_err());
    }
}

#[cfg(unix)]
#[cfg(test)]
mod read_only_key_tests {
    use super::*;
    use crate::ClientKey;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn identity_and_client_load_only_read_existing_files_and_lock() {
        for client in [false, true] {
            let d = tempfile::tempdir().unwrap();
            fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let public = if client {
                ClientKey::create(d.path()).unwrap().public_key()
            } else {
                Identity::open(d.path()).unwrap().public_key()
            };
            let (key, lock) = if client {
                ("client.seed", ".client.lock")
            } else {
                ("identity.seed", ".identity.lock")
            };
            fs::set_permissions(d.path().join(key), fs::Permissions::from_mode(0o400)).unwrap();
            fs::set_permissions(d.path().join(lock), fs::Permissions::from_mode(0o400)).unwrap();
            fs::set_permissions(d.path(), fs::Permissions::from_mode(0o500)).unwrap();
            let loaded = if client {
                ClientKey::load(d.path()).unwrap().public_key()
            } else {
                Identity::load(d.path()).unwrap().public_key()
            };
            assert_eq!(public, loaded);
            assert_eq!(fs::read_dir(d.path()).unwrap().count(), 2);
            fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(Identity::load(d.path()).is_err());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }
}
