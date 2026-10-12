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
        write_private_new(&dir.join("identity.seed"), &bytes)
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }
    /// Local SIM pseudonym, never a public identifier. Domain separated HMAC with device seed.
    pub fn sim_fingerprint(&self, iccid: &str) -> Result<[u8; 32]> {
        use hmac::{Hmac, Mac};
        if !(8..=32).contains(&iccid.len()) || !iccid.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidInput);
        }
        let seed = Zeroizing::new(self.signing.to_bytes());
        let mut mac =
            Hmac::<Sha256>::new_from_slice(seed.as_ref()).map_err(|_| Error::InvalidInput)?;
        mac.update(b"RustSimHub.SimFingerprint.v1\0");
        mac.update(iccid.as_bytes());
        Ok(mac.finalize().into_bytes().into())
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
    let lock = open_private(&dir.join(lock_name), false)?;
    lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
    let path = dir.join(seed_name);
    let pending = dir.join(pending_name);
    if !path.try_exists().map_err(|_| Error::Io)? {
        if !pending.try_exists().map_err(|_| Error::Io)? {
            if !create {
                return Err(Error::Io);
            }
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
fn check(path: &Path, directory: bool) -> Result<()> {
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
    open_private(path, false)?
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
pub fn read_private(path: &Path, limit: usize) -> Result<Zeroizing<Vec<u8>>> {
    check(path.parent().ok_or(Error::InvalidInput)?, true)?;
    check(path, false)?;
    let mut bytes = Zeroizing::new(Vec::new());
    open_private(path, false)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > limit {
        return Err(Error::InvalidInput);
    }
    Ok(bytes)
}
