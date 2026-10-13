// SPDX-License-Identifier: AGPL-3.0-only
//! Root-owned application slots. Data and firmware partitions are outside this namespace.
use crate::{
    hex_digest, manifest_hash, verify, verify_payload, Error, Manifest, FILES, MAX_MANIFEST,
    MAX_PAYLOAD,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Version {
    hash: String,
    epoch: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    format: u16,
    key_sha256: String,
    floor: u64,
    confirmed: Option<Version>,
    candidate: Option<Version>,
    attempted: bool,
}
impl State {
    fn valid(&self) -> bool {
        self.format == 1
            && hex_digest(&self.key_sha256)
            && [&self.confirmed, &self.candidate]
                .into_iter()
                .flatten()
                .all(|v| hex_digest(&v.hash) && v.epoch > 0 && v.epoch <= self.floor)
            && (!self.attempted || self.candidate.is_some())
    }
}
fn dir(path: &Path) -> Result<(), Error> {
    let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if !m.is_dir() || m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o022 != 0 {
        return Err(Error::Denied);
    }
    Ok(())
}
fn metadata(f: &File) -> Result<(), Error> {
    let m = f.metadata().map_err(|_| Error::Io)?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o022 != 0
        || m.nlink() != 1
    {
        return Err(Error::Denied);
    }
    Ok(())
}
pub fn read_checked(path: &Path, limit: usize) -> Result<Vec<u8>, Error> {
    dir(path.parent().ok_or(Error::Denied)?)?;
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(libc_flags())
        .open(path)
        .map_err(|_| Error::Io)?;
    metadata(&f)?;
    if f.metadata().map_err(|_| Error::Io)?.len() > limit as u64 {
        return Err(Error::Invalid);
    }
    let mut b = Vec::new();
    f.take(limit as u64 + 1)
        .read_to_end(&mut b)
        .map_err(|_| Error::Io)?;
    if b.len() > limit {
        return Err(Error::Invalid);
    }
    Ok(b)
}
fn libc_flags() -> i32 {
    (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32
}
pub fn write_new(path: &Path, bytes: &[u8], executable: bool) -> Result<(), Error> {
    dir(path.parent().ok_or(Error::Denied)?)?;
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(if executable { 0o755 } else { 0o600 })
        .custom_flags(libc_flags())
        .open(path)
        .map_err(|_| Error::Io)?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| Error::Io)?;
    Ok(())
}
fn sync_dir(path: &Path) -> Result<(), Error> {
    dir(path)?;
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error::Io)
}
pub struct Store {
    root: PathBuf,
    key: [u8; 32],
    schema: u32,
    state: State,
    _lock: File,
}
impl Store {
    pub fn open(root: &Path, key: [u8; 32], schema: u32) -> Result<Self, Error> {
        if !root.is_absolute() {
            return Err(Error::Denied);
        }
        dir(root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc_flags())
            .open(root.join(".update.lock"))
            .map_err(|_| Error::Io)?;
        metadata(&lock)?;
        lock.try_lock_exclusive().map_err(|_| Error::Busy)?;
        let versions = root.join("versions");
        match fs::symlink_metadata(&versions) {
            Ok(_) => dir(&versions)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&versions).map_err(|_| Error::Io)?;
                fs::set_permissions(&versions, fs::Permissions::from_mode(0o700))
                    .map_err(|_| Error::Io)?;
                sync_dir(root)?;
            }
            _ => return Err(Error::Io),
        }
        let mut state = match fs::symlink_metadata(root.join("state.json")) {
            Ok(_) => parse_state(&read_checked(&root.join("state.json"), 8192)?)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State {
                format: 1,
                key_sha256: manifest_hash(&key),
                ..State::default()
            },
            _ => return Err(Error::Io),
        };
        let next = root.join("state.next");
        if fs::symlink_metadata(&next).is_ok() {
            let bytes = read_checked(&next, 8192)?;
            match parse_state(&bytes) {
                Ok(candidate)
                    if candidate.floor >= state.floor
                        && candidate.key_sha256 == state.key_sha256 =>
                {
                    fs::rename(&next, root.join("state.json")).map_err(|_| Error::Io)?;
                    sync_dir(root)?;
                    state = candidate;
                }
                _ => {
                    fs::remove_file(&next).map_err(|_| Error::Io)?;
                    sync_dir(root)?;
                }
            }
        }
        if state.key_sha256 != manifest_hash(&key) {
            return Err(Error::Denied);
        }
        Ok(Self {
            root: root.to_path_buf(),
            key,
            schema,
            state,
            _lock: lock,
        })
    }
    fn persist(&mut self) -> Result<(), Error> {
        if !self.state.valid() {
            return Err(Error::Invalid);
        }
        write_new(
            &self.root.join("state.next"),
            &serde_json::to_vec(&self.state).map_err(|_| Error::Invalid)?,
            false,
        )?;
        sync_dir(&self.root)?;
        fs::rename(self.root.join("state.next"), self.root.join("state.json"))
            .map_err(|_| Error::Io)?;
        sync_dir(&self.root)
    }
    pub fn floor(&self) -> u64 {
        self.state.floor
    }
    pub fn stage(
        &mut self,
        manifest: &[u8],
        signature: &[u8],
        payloads: &[Vec<u8>],
    ) -> Result<String, Error> {
        if self.state.candidate.is_some() {
            return Err(Error::Busy);
        }
        let m = verify(
            manifest,
            signature,
            &self.key,
            self.schema,
            self.state.floor,
        )?;
        verify_payload(&m, payloads)?;
        let hash = manifest_hash(manifest);
        let target = self.root.join("versions").join(&hash);
        // Never overwrite an incomplete or previously present directory.
        fs::create_dir(&target).map_err(|_| Error::Io)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).map_err(|_| Error::Io)?;
        write_new(&target.join("manifest.json"), manifest, false)?;
        write_new(&target.join("manifest.sig"), signature, false)?;
        for (name, payload) in FILES.into_iter().zip(payloads) {
            write_new(&target.join(name), payload, name != "Opus-COPYING.txt")?;
        }
        sync_dir(&target)?;
        sync_dir(&self.root.join("versions"))?;
        self.state.floor = m.epoch;
        self.state.candidate = Some(Version {
            hash: hash.clone(),
            epoch: m.epoch,
        });
        self.state.attempted = false;
        self.persist()?;
        Ok(hash)
    }
    fn verify_version(&self, v: &Version) -> Result<PathBuf, Error> {
        let path = self.root.join("versions").join(&v.hash);
        dir(&path)?;
        let manifest = read_checked(&path.join("manifest.json"), MAX_MANIFEST)?;
        if manifest_hash(&manifest) != v.hash {
            return Err(Error::Integrity);
        }
        let sig = read_checked(&path.join("manifest.sig"), 64)?;
        let m = verify(&manifest, &sig, &self.key, self.schema, 0)?;
        if m.epoch != v.epoch {
            return Err(Error::Integrity);
        }
        let payloads = load_payloads(&path, &m)?;
        verify_payload(&m, &payloads)?;
        Ok(path)
    }
    /// Persist first-attempt BEFORE returning executable paths. A second start rolls back.
    pub fn choose(&mut self) -> Result<PathBuf, Error> {
        if let Some(v) = self.state.candidate.clone() {
            if !self.state.attempted {
                if let Ok(path) = self.verify_version(&v) {
                    self.state.attempted = true;
                    self.persist()?;
                    return Ok(path);
                }
            }
            self.state.candidate = None;
            self.state.attempted = false;
            self.persist()?;
        }
        let v = self.state.confirmed.as_ref().ok_or(Error::Denied)?;
        self.verify_version(v)
    }
    /// Caller must have completed service/data health checks for this exact hash.
    pub fn confirm(&mut self, hash: &str) -> Result<(), Error> {
        let v = self.state.candidate.clone().ok_or(Error::Denied)?;
        if !self.state.attempted || hash != v.hash {
            return Err(Error::Denied);
        }
        self.verify_version(&v)?;
        self.state.confirmed = Some(v);
        self.state.candidate = None;
        self.state.attempted = false;
        self.persist()
    }
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"format":1,"floor_epoch":self.state.floor,"confirmed":self.state.confirmed.as_ref().map(|v|&v.hash),"candidate":self.state.candidate.as_ref().map(|v|&v.hash),"candidate_attempted":self.state.attempted,"system_ota_supported":false})
    }
}
fn parse_state(bytes: &[u8]) -> Result<State, Error> {
    let s: State = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
    if !s.valid() {
        return Err(Error::Invalid);
    }
    Ok(s)
}
pub fn load_payloads(directory: &Path, m: &Manifest) -> Result<Vec<Vec<u8>>, Error> {
    let mut total = 0u64;
    let mut payloads = Vec::new();
    for e in &m.files {
        if !FILES.contains(&e.name.as_str()) || e.bytes > MAX_PAYLOAD {
            return Err(Error::Invalid);
        }
        total = total.checked_add(e.bytes).ok_or(Error::Invalid)?;
        if total > MAX_PAYLOAD {
            return Err(Error::Invalid);
        }
        let bytes = read_checked(&directory.join(&e.name), e.bytes as usize)?;
        if bytes.len() as u64 != e.bytes {
            return Err(Error::Integrity);
        }
        payloads.push(bytes);
    }
    Ok(payloads)
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn package(epoch: u64) -> (Vec<u8>, [u8; 64], Vec<Vec<u8>>) {
        let (k, mut m, p) = crate::tests::fixture();
        m.epoch = epoch;
        let b = serde_json::to_vec(&m).unwrap();
        let s = k.sign(&crate::signing_bytes(&b).unwrap()).to_bytes();
        (b, s, p)
    }
    fn directory() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        d
    }
    fn key() -> [u8; 32] {
        SigningKey::from_bytes(&[77; 32]).verifying_key().to_bytes()
    }
    #[test]
    fn confirmed_factory_failed_candidate_and_epoch_floor_survive_reopen() {
        let d = directory();
        let (mut s, (b, g, p)) = (Store::open(d.path(), key(), 5).unwrap(), package(1));
        let first = s.stage(&b, &g, &p).unwrap();
        let path = s.choose().unwrap();
        assert!(path.ends_with(&first));
        s.confirm(&first).unwrap();
        let (b, g, p) = package(2);
        let second = s.stage(&b, &g, &p).unwrap();
        assert!(s.choose().unwrap().ends_with(&second));
        drop(s);
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        assert!(s.choose().unwrap().ends_with(&first));
        assert_eq!(s.floor(), 2);
        assert_eq!(s.stage(&b, &g, &p), Err(Error::Rollback));
    }
    #[test]
    fn atomic_state_interruption_and_slot_corruption_never_select_bad_payload() {
        let d = directory();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        let (b, g, p) = package(1);
        let first = s.stage(&b, &g, &p).unwrap();
        s.choose().unwrap();
        s.confirm(&first).unwrap();
        let (b, g, p) = package(2);
        let second = s.stage(&b, &g, &p).unwrap();
        let target = d.path().join("versions").join(&second).join("simhubctl");
        fs::write(target, b"corrupt").unwrap();
        drop(s);
        fs::write(d.path().join("state.next"), b"{torn").unwrap();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        assert!(s.choose().unwrap().ends_with(&first));
        assert!(!d.path().join("state.next").exists());
        assert_eq!(s.floor(), 2);
    }
    #[test]
    fn locks_symlinks_and_wrong_confirmation_are_rejected() {
        let d = directory();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        assert!(matches!(Store::open(d.path(), key(), 5), Err(Error::Busy)));
        let (b, g, p) = package(1);
        let hash = s.stage(&b, &g, &p).unwrap();
        assert_eq!(s.confirm(&hash), Err(Error::Denied));
        s.choose().unwrap();
        assert_eq!(s.confirm(&"f".repeat(64)), Err(Error::Denied));
        let p = d.path().join("bad");
        std::os::unix::fs::symlink(d.path().join("state.json"), &p).unwrap();
        assert!(read_checked(&p, 8192).is_err());
    }
}
