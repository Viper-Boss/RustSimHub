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
        let mut state_present = true;
        let mut state = match fs::symlink_metadata(root.join("state.json")) {
            Ok(_) => parse_state(&read_checked(&root.join("state.json"), 8192)?)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                state_present = false;
                State {
                    format: 1,
                    key_sha256: manifest_hash(&key),
                    ..State::default()
                }
            }
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
                    state_present = true;
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
        // Lost state must not silently reset an existing installation's epoch floor.
        // A complete state.next above may recover an interrupted first publication.
        if !state_present
            && fs::read_dir(&versions)
                .map_err(|_| Error::Io)?
                .next()
                .is_some()
        {
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
    /// Select once for the whole boot group. The versions bind must name the
    /// same directory on the root filesystem; the ledger stays on persistent state.
    pub fn check_runtime_paths(&self, runtime: &Path, payload_root: &Path) -> Result<(), Error> {
        if !runtime.is_absolute() || !payload_root.is_absolute() {
            return Err(Error::Denied);
        }
        dir(runtime)?;
        dir(payload_root)?;
        let alias = fs::symlink_metadata(self.root.join("versions")).map_err(|_| Error::Io)?;
        let physical = fs::symlink_metadata(payload_root).map_err(|_| Error::Io)?;
        if alias.dev() != physical.dev() || alias.ino() != physical.ino() {
            return Err(Error::Denied);
        }
        for name in ["selected", "selected.next"] {
            let path = runtime.join(name);
            match fs::symlink_metadata(&path) {
                Ok(m) => {
                    if !m.file_type().is_symlink() || m.uid() != rustix::process::geteuid().as_raw()
                    {
                        return Err(Error::Denied);
                    }
                    let target = fs::read_link(&path).map_err(|_| Error::Io)?;
                    if target.parent() != Some(payload_root)
                        || !target
                            .file_name()
                            .and_then(|s| s.to_str())
                            .is_some_and(hex_digest)
                    {
                        return Err(Error::Denied);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(Error::Io),
            }
        }
        Ok(())
    }
    pub fn select_runtime(
        &mut self,
        runtime: &Path,
        payload_root: &Path,
    ) -> Result<PathBuf, Error> {
        use std::os::unix::fs::symlink;
        self.check_runtime_paths(runtime, payload_root)?;
        let selected = self.choose()?;
        let hash = selected.file_name().ok_or(Error::Invalid)?;
        for name in FILES {
            let path = selected.join(name);
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if name == "Opus-COPYING.txt" {
                    0o444
                } else {
                    0o555
                }),
            )
            .map_err(|_| Error::Io)?;
            File::open(path)
                .and_then(|f| f.sync_all())
                .map_err(|_| Error::Io)?;
        }
        fs::set_permissions(&selected, fs::Permissions::from_mode(0o755)).map_err(|_| Error::Io)?;
        sync_dir(&selected)?;
        let destination = payload_root.join(hash);
        let next = runtime.join("selected.next");
        if fs::symlink_metadata(&next).is_ok() {
            fs::remove_file(&next).map_err(|_| Error::Io)?;
        }
        symlink(&destination, &next).map_err(|_| Error::Io)?;
        sync_dir(runtime)?;
        fs::rename(&next, runtime.join("selected")).map_err(|_| Error::Io)?;
        sync_dir(runtime)?;
        Ok(destination)
    }
    /// Explicit installer operation for a signed factory baseline, before business startup.
    /// It is not a health confirmation for an update. Never replaces an existing baseline,
    /// resets its epoch floor, or consumes a pending update's boot attempt.
    pub fn install_factory(
        &mut self,
        manifest: &[u8],
        signature: &[u8],
        payloads: &[Vec<u8>],
    ) -> Result<PathBuf, Error> {
        let m = verify(manifest, signature, &self.key, self.schema, 0)?;
        verify_payload(&m, payloads)?;
        if let Some(confirmed) = &self.state.confirmed {
            return self.verify_version(confirmed);
        }
        let hash = manifest_hash(manifest);
        if let Some(candidate) = &self.state.candidate {
            // Resume only the exact, fully published factory. A different package
            // cannot turn an unconfirmed first update into the factory baseline.
            if candidate.hash != hash || candidate.epoch != m.epoch || self.state.floor != m.epoch {
                return Err(Error::Denied);
            }
            self.verify_version(candidate)?;
        } else {
            if self.state.floor != 0 || self.state.attempted {
                return Err(Error::Denied);
            }
            self.stage(manifest, signature, payloads)?;
        }
        let factory = self.state.candidate.clone().ok_or(Error::Denied)?;
        let path = self.verify_version(&factory)?;
        self.state.confirmed = Some(factory);
        self.state.candidate = None;
        self.state.attempted = false;
        self.persist()?;
        Ok(path)
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
        // Admission must retain a verified fallback under this updater's schema.
        // Refuse before writing payloads or advancing the epoch floor. Merely
        // replacing the updater cannot turn an incompatible old baseline into
        // a safe maintenance transition.
        if let Some(baseline) = &self.state.confirmed {
            self.verify_version(baseline)?;
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
    fn schema_package(epoch: u64, schema: u32) -> (Vec<u8>, [u8; 64], Vec<Vec<u8>>) {
        let (k, mut m, p) = crate::tests::fixture();
        m.epoch = epoch;
        m.schema_min = schema;
        m.schema_max = schema;
        let b = serde_json::to_vec(&m).unwrap();
        let sig = k.sign(&crate::signing_bytes(&b).unwrap()).to_bytes();
        (b, sig, p)
    }
    #[test]
    fn schema_transition_cannot_stage_over_an_unreadable_fallback_or_advance_floor() {
        let d = directory();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        let (b, g, p) = package(1);
        let baseline = s.install_factory(&b, &g, &p).unwrap();
        let before = fs::read(d.path().join("state.json")).unwrap();
        drop(s);
        let (b, g, p) = schema_package(2, 6);
        let hash = manifest_hash(&b);
        let mut s = Store::open(d.path(), key(), 6).unwrap();
        assert_eq!(s.stage(&b, &g, &p), Err(Error::Schema));
        assert_eq!(fs::read(d.path().join("state.json")).unwrap(), before);
        assert!(!d.path().join("versions").join(hash).exists());
        drop(s);
        let mut old = Store::open(d.path(), key(), 5).unwrap();
        assert_eq!(old.stage(&b, &g, &p), Err(Error::Schema));
        assert_eq!(old.choose().unwrap(), baseline);
        assert_eq!(old.floor(), 1);
    }
    #[test]
    fn corrupt_confirmed_group_blocks_admission_before_candidate_files_or_floor() {
        let d = directory();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        let (b, g, p) = package(1);
        let baseline = s.install_factory(&b, &g, &p).unwrap();
        fs::set_permissions(baseline.join(FILES[0]), fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(baseline.join(FILES[0]), b"corrupt").unwrap();
        let before = fs::read(d.path().join("state.json")).unwrap();
        let (b, g, p) = package(2);
        assert!(s.stage(&b, &g, &p).is_err());
        assert_eq!(fs::read(d.path().join("state.json")).unwrap(), before);
        assert!(!d.path().join("versions").join(manifest_hash(&b)).exists());
    }
    #[test]
    fn compatible_schema_six_group_reopens_and_falls_back_without_lowering_floor() {
        let d = directory();
        let mut s = Store::open(d.path(), key(), 6).unwrap();
        let (b, g, p) = schema_package(1, 6);
        let baseline = s.install_factory(&b, &g, &p).unwrap();
        let (b, g, p) = schema_package(2, 6);
        let candidate = s.stage(&b, &g, &p).unwrap();
        assert!(s.choose().unwrap().ends_with(&candidate));
        drop(s);
        let mut s = Store::open(d.path(), key(), 6).unwrap();
        assert_eq!(s.choose().unwrap(), baseline);
        assert_eq!(s.floor(), 2);
        assert_eq!(s.stage(&b, &g, &p), Err(Error::Rollback));
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
        drop(s);
        let other = SigningKey::from_bytes(&[78; 32]).verifying_key().to_bytes();
        assert!(matches!(
            Store::open(d.path(), other, 5),
            Err(Error::Denied)
        ));
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
    #[test]
    fn factory_install_is_idempotent_and_preserves_pending_update_and_floor() {
        let d = directory();
        let (b, g, p) = package(1);
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        let factory = s.install_factory(&b, &g, &p).unwrap();
        assert_eq!(s.choose().unwrap(), factory);
        let (update, sig, payload) = package(2);
        let candidate = s.stage(&update, &sig, &payload).unwrap();
        let before = fs::read(d.path().join("state.json")).unwrap();
        assert_eq!(s.install_factory(&b, &g, &p).unwrap(), factory);
        assert_eq!(fs::read(d.path().join("state.json")).unwrap(), before);
        assert!(s.choose().unwrap().ends_with(&candidate));
        s.confirm(&candidate).unwrap();
        drop(s);
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        // A later boot with the old immutable factory does not downgrade the app.
        assert!(s.install_factory(&b, &g, &p).unwrap().ends_with(&candidate));
        assert_eq!(s.floor(), 2);
        assert_eq!(s.stage(&b, &g, &p), Err(Error::Rollback));
    }
    #[test]
    fn factory_resume_requires_exact_verified_package_and_accepts_attempted_state() {
        for attempted in [false, true] {
            let d = directory();
            let (b, g, p) = package(1);
            let mut s = Store::open(d.path(), key(), 5).unwrap();
            let hash = s.stage(&b, &g, &p).unwrap();
            if attempted {
                s.choose().unwrap();
            }
            drop(s);
            let mut s = Store::open(d.path(), key(), 5).unwrap();
            let (other, sig, payload) = package(2);
            let before = fs::read(d.path().join("state.json")).unwrap();
            assert_eq!(
                s.install_factory(&other, &sig, &payload),
                Err(Error::Denied)
            );
            assert_eq!(fs::read(d.path().join("state.json")).unwrap(), before);
            assert!(s.install_factory(&b, &g, &p).unwrap().ends_with(&hash));
            assert_eq!(s.status()["candidate"], serde_json::Value::Null);
            assert_eq!(s.status()["confirmed"], hash);
        }
    }
    #[test]
    fn missing_ledger_with_existing_slots_is_denied_but_complete_next_recovers() {
        let d = directory();
        let (b, g, p) = package(1);
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        s.install_factory(&b, &g, &p).unwrap();
        drop(s);
        let state = fs::read(d.path().join("state.json")).unwrap();
        fs::remove_file(d.path().join("state.json")).unwrap();
        assert!(matches!(
            Store::open(d.path(), key(), 5),
            Err(Error::Denied)
        ));
        write_new(&d.path().join("state.next"), b"{torn", false).unwrap();
        assert!(matches!(
            Store::open(d.path(), key(), 5),
            Err(Error::Denied)
        ));
        write_new(&d.path().join("state.next"), &state, false).unwrap();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        assert_eq!(s.floor(), 1);
        assert!(s.choose().is_ok());
    }
    #[test]
    fn factory_signature_payload_and_orphan_slot_fail_closed() {
        let d = directory();
        let (b, g, mut p) = package(1);
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        assert_eq!(s.install_factory(&b, &[0; 64], &p), Err(Error::Signature));
        p[0][20] ^= 1;
        assert_eq!(s.install_factory(&b, &g, &p), Err(Error::Integrity));
        assert!(fs::read_dir(d.path().join("versions"))
            .unwrap()
            .next()
            .is_none());
        let hash = manifest_hash(&b);
        fs::create_dir(d.path().join("versions").join(hash)).unwrap();
        let (_, _, p) = package(1);
        assert_eq!(s.install_factory(&b, &g, &p), Err(Error::Io));
        assert!(!d.path().join("state.json").exists());
    }
    #[test]
    fn one_boot_selection_publishes_the_group_then_falls_back_without_confirming() {
        let d = directory();
        let runtime = directory();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        let (b, g, p) = package(1);
        let factory = s.install_factory(&b, &g, &p).unwrap();
        let physical = d.path().join("versions");
        assert_eq!(
            s.select_runtime(runtime.path(), &physical).unwrap(),
            factory
        );
        let (b, g, p) = package(2);
        let second = s.stage(&b, &g, &p).unwrap();
        let selected = s.select_runtime(runtime.path(), &physical).unwrap();
        assert!(selected.ends_with(&second));
        assert_eq!(
            fs::read_link(runtime.path().join("selected")).unwrap(),
            selected
        );
        assert_eq!(
            fs::metadata(selected.join(FILES[0])).unwrap().mode() & 0o777,
            0o555
        );
        assert_eq!(s.status()["candidate_attempted"], true);
        // A crash after creating a pending runtime link cannot replay the candidate.
        std::os::unix::fs::symlink(&selected, runtime.path().join("selected.next")).unwrap();
        assert_eq!(
            s.select_runtime(runtime.path(), &physical).unwrap(),
            factory
        );
        assert_eq!(s.floor(), 2);
        assert!(!runtime.path().join("selected.next").exists());
    }
    #[test]
    fn wrong_versions_bind_or_unsafe_runtime_path_does_not_consume_candidate() {
        let d = directory();
        let runtime = directory();
        let wrong = directory();
        let mut s = Store::open(d.path(), key(), 5).unwrap();
        let (b, g, p) = package(1);
        s.install_factory(&b, &g, &p).unwrap();
        let (b, g, p) = package(2);
        s.stage(&b, &g, &p).unwrap();
        let before = fs::read(d.path().join("state.json")).unwrap();
        assert_eq!(
            s.select_runtime(runtime.path(), wrong.path()),
            Err(Error::Denied)
        );
        let physical = d.path().join("versions");
        let target = runtime.path().join("selected");
        std::os::unix::fs::symlink("/unexpected/escape", &target).unwrap();
        assert_eq!(
            s.select_runtime(runtime.path(), &physical),
            Err(Error::Denied)
        );
        fs::remove_file(&target).unwrap();
        fs::write(&target, b"do not overwrite").unwrap();
        assert_eq!(
            s.select_runtime(runtime.path(), &physical),
            Err(Error::Denied)
        );
        assert_eq!(fs::read(&target).unwrap(), b"do not overwrite");
        assert_eq!(fs::read(d.path().join("state.json")).unwrap(), before);
    }
}
