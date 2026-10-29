// SPDX-License-Identifier: AGPL-3.0-only
//! Public Store API, synthetic signatures, ordinary Linux UID. Never executes payloads.
#![cfg(target_os = "linux")]
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_update::{
    application_files, manifest_hash, signing_bytes, store::Store, Entry, Error, Manifest, BOARD,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

struct Package {
    manifest: Vec<u8>,
    signature: [u8; 64],
    payloads: Vec<Vec<u8>>,
    hash: String,
}
fn package(format: u16, epoch: u64) -> Package {
    let key = SigningKey::from_bytes(&[77; 32]); // Explicit synthetic fixture key, never production.
    let payloads: Vec<_> = application_files(format)
        .unwrap()
        .iter()
        .map(|name| {
            if *name == "Opus-COPYING.txt" {
                return b"synthetic license fixture".to_vec();
            }
            let mut b = vec![0; 64];
            b[..4].copy_from_slice(b"\x7fELF");
            b[4] = 2;
            b[5] = 1;
            b[6] = 1;
            b[16] = 2;
            b[18] = 183;
            b[52] = 64;
            b
        })
        .collect();
    let m = Manifest {
        format,
        kind: "application".into(),
        board: BOARD.into(),
        arch: "aarch64-linux".into(),
        epoch,
        schema_min: 6,
        schema_max: 6,
        source_sha256: "a".repeat(64),
        development_fixtures: false,
        files: application_files(format)
            .unwrap()
            .iter()
            .zip(&payloads)
            .map(|(name, b)| Entry {
                name: (*name).into(),
                bytes: b.len() as u64,
                sha256: format!("{:x}", Sha256::digest(b)),
            })
            .collect(),
    };
    let manifest = serde_json::to_vec(&m).unwrap();
    let signature = key.sign(&signing_bytes(&manifest).unwrap()).to_bytes();
    let hash = manifest_hash(&manifest);
    Package {
        manifest,
        signature,
        payloads,
        hash,
    }
}
struct Fixture {
    root: tempfile::TempDir,
    a: PathBuf,
}
impl Fixture {
    fn new(format: u16) -> Self {
        assert_ne!(
            rustix::process::geteuid().as_raw(),
            0,
            "ordinary UID required; no Root permission bypass"
        );
        let root = tempfile::tempdir().unwrap();
        mode(root.path(), 0o700);
        let p = package(format, 1);
        let a = Self::open_at(root.path())
            .install_factory(&p.manifest, &p.signature, &p.payloads)
            .unwrap();
        Self { root, a }
    }
    fn open_at(root: &Path) -> Store {
        Store::open(
            root,
            SigningKey::from_bytes(&[77; 32]).verifying_key().to_bytes(),
            6,
        )
        .unwrap()
    }
    fn open(&self) -> Store {
        Self::open_at(self.root.path())
    }
    fn ledger(&self) -> Vec<u8> {
        fs::read(self.root.path().join("state.json")).unwrap()
    }
    fn disk(&self) -> Value {
        serde_json::from_slice(&self.ledger()).unwrap()
    }
    fn next(&self) -> PathBuf {
        self.root.path().join("state.next")
    }
    fn target(&self, p: &Package) -> PathBuf {
        self.root.path().join("versions").join(&p.hash)
    }
    fn conflict(&self) {
        fs::write(self.next(), b"{torn").unwrap();
        mode(&self.next(), 0o600);
    }
}
fn mode(path: &Path, bits: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(bits)).unwrap();
}
struct RestoreMode(PathBuf);
impl RestoreMode {
    fn readonly(path: &Path) -> Self {
        mode(path, 0o500);
        Self(path.to_path_buf())
    }
}
impl Drop for RestoreMode {
    fn drop(&mut self) {
        mode(&self.0, 0o700);
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, here: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for item in fs::read_dir(here).unwrap() {
            let path = item.unwrap().path();
            let kind = fs::symlink_metadata(&path).unwrap();
            assert!(!kind.file_type().is_symlink());
            if kind.is_dir() {
                walk(root, &path, out);
            } else {
                assert!(kind.is_file());
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn stage(s: &mut Store, p: &Package) {
    assert_eq!(
        s.stage(&p.manifest, &p.signature, &p.payloads),
        Ok(p.hash.clone())
    );
}
fn stable_a(f: &Fixture, s: &Store, floor: u64) {
    assert_eq!(s.floor(), floor);
    assert_eq!(
        s.status()["confirmed"],
        f.a.file_name().unwrap().to_str().unwrap()
    );
    assert!(s.status()["candidate"].is_null());
    assert_eq!(s.status()["candidate_attempted"], false);
}
fn writable_control(format: u16) {
    let f = Fixture::new(format);
    let mut s = f.open();
    let p = package(format, 2);
    stage(&mut s, &p);
    assert_eq!(s.choose().unwrap(), f.target(&p));
    s.confirm(&p.hash).unwrap();
    drop(s);
    let mut s = f.open();
    assert_eq!(s.choose().unwrap(), f.target(&p));
    assert_eq!(s.floor(), 2);
}

#[test]
fn p01_real_directory_eacces_preserves_baseline_and_reopens() {
    for format in [1, 2] {
        writable_control(format);
        let f = Fixture::new(format);
        let mut s = f.open();
        let p = package(format, 2);
        let ledger = f.ledger();
        let original = snapshot(&f.a);
        let versions = f.root.path().join("versions");
        let guard = RestoreMode::readonly(&versions);
        assert_eq!(
            fs::create_dir(versions.join("probe"))
                .unwrap_err()
                .raw_os_error(),
            Some(13)
        );
        assert_eq!(
            s.stage(&p.manifest, &p.signature, &p.payloads),
            Err(Error::Io)
        );
        assert_eq!(f.ledger(), ledger);
        assert_eq!(snapshot(&f.a), original);
        assert!(!f.target(&p).exists());
        stable_a(&f, &s, 1);
        drop(guard);
        drop(s);
        let mut reopened = f.open();
        stable_a(&f, &reopened, 1);
        assert_eq!(reopened.choose().unwrap(), f.a);
    }
}
#[test]
fn p02_stage_persist_collision_keeps_orphan_out_of_durable_ledger() {
    for format in [1, 2] {
        let f = Fixture::new(format);
        let mut s = f.open();
        let p = package(format, 2);
        let ledger = f.ledger();
        let original = snapshot(&f.a);
        f.conflict();
        assert_eq!(
            s.stage(&p.manifest, &p.signature, &p.payloads),
            Err(Error::Io)
        );
        assert_eq!(f.ledger(), ledger);
        assert_eq!(snapshot(&f.a), original);
        assert_eq!(fs::read(f.next()).unwrap(), b"{torn");
        let orphan = snapshot(&f.target(&p));
        assert_eq!(orphan.len(), application_files(format).unwrap().len() + 2);
        assert_eq!(s.floor(), 2);
        assert_eq!(s.status()["candidate"], p.hash);
        assert_eq!(f.disk()["floor"], 1);
        drop(s);
        let mut reopened = f.open();
        stable_a(&f, &reopened, 1);
        assert!(!f.next().exists());
        assert_eq!(reopened.choose().unwrap(), f.a);
        assert_eq!(f.ledger(), ledger);
        assert_eq!(snapshot(&f.target(&p)), orphan);
        println!("C13-P02 format={format} memory_floor=2 durable_floor=1 orphan_preserved=true");
    }
}
#[test]
fn p03_failed_choose_returns_no_path_and_only_durable_attempt_counts() {
    for format in [1, 2] {
        let f = Fixture::new(format);
        let mut s = f.open();
        let p = package(format, 2);
        stage(&mut s, &p);
        let ledger = f.ledger();
        let groups = snapshot(&f.root.path().join("versions"));
        f.conflict();
        assert_eq!(s.choose(), Err(Error::Io));
        assert_eq!(f.ledger(), ledger);
        assert_eq!(s.status()["candidate_attempted"], true);
        assert_eq!(f.disk()["attempted"], false);
        assert_eq!(snapshot(&f.root.path().join("versions")), groups);
        drop(s);
        let mut reopened = f.open();
        assert_eq!(reopened.choose().unwrap(), f.target(&p));
        assert_eq!(f.disk()["attempted"], true);
        drop(reopened);
        let mut again = f.open();
        assert_eq!(again.choose().unwrap(), f.a);
        stable_a(&f, &again, 2);
        assert_eq!(snapshot(&f.root.path().join("versions")), groups);
        println!("C13-P03 format={format} failed_choose_returned_no_path=true first_durable_attempt_then_fallback=true");
    }
}
#[test]
fn p04_failed_confirm_does_not_durably_confirm_and_reopens_to_fallback() {
    for format in [1, 2] {
        let f = Fixture::new(format);
        let mut s = f.open();
        let p = package(format, 2);
        stage(&mut s, &p);
        assert_eq!(s.choose().unwrap(), f.target(&p));
        let ledger = f.ledger();
        let groups = snapshot(&f.root.path().join("versions"));
        f.conflict();
        assert_eq!(s.confirm(&p.hash), Err(Error::Io));
        assert_eq!(f.ledger(), ledger);
        assert_eq!(s.status()["confirmed"], p.hash);
        assert!(s.status()["candidate"].is_null());
        assert_eq!(
            f.disk()["confirmed"]["hash"],
            f.a.file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(snapshot(&f.root.path().join("versions")), groups);
        drop(s);
        let mut reopened = f.open();
        assert_eq!(reopened.choose().unwrap(), f.a);
        stable_a(&f, &reopened, 2);
        assert_eq!(snapshot(&f.root.path().join("versions")), groups);
        println!("C13-P04 format={format} memory_confirmed_candidate=true durable_confirmed_baseline=true");
    }
}
#[test]
fn p05_runtime_link_eacces_preserves_selected_but_consumes_attempt() {
    for format in [1, 2] {
        let f = Fixture::new(format);
        let mut s = f.open();
        let p = package(format, 2);
        let runtime = f.root.path().join("runtime");
        fs::create_dir(&runtime).unwrap();
        mode(&runtime, 0o700);
        let versions = f.root.path().join("versions");
        assert_eq!(s.select_runtime(&runtime, &versions).unwrap(), f.a);
        stage(&mut s, &p);
        let groups = snapshot(&versions);
        let guard = RestoreMode::readonly(&runtime);
        assert_eq!(
            fs::write(runtime.join("probe"), b"probe")
                .unwrap_err()
                .raw_os_error(),
            Some(13)
        );
        s.check_runtime_paths(&runtime, &versions).unwrap();
        assert_eq!(s.select_runtime(&runtime, &versions), Err(Error::Io));
        assert_eq!(fs::read_link(runtime.join("selected")).unwrap(), f.a);
        assert!(!runtime.join("selected.next").exists());
        assert_eq!(f.disk()["floor"], 2);
        assert_eq!(f.disk()["attempted"], true);
        assert_eq!(f.disk()["candidate"]["hash"], p.hash);
        assert_eq!(snapshot(&versions), groups);
        drop(guard);
        drop(s);
        let mut reopened = f.open();
        assert_eq!(reopened.select_runtime(&runtime, &versions).unwrap(), f.a);
        stable_a(&f, &reopened, 2);
        assert_eq!(fs::read_link(runtime.join("selected")).unwrap(), f.a);
        assert_eq!(snapshot(&versions), groups);
        println!("C13-P05 format={format} previous_selected_preserved=true durable_attempt_consumed=true");
    }
}
#[test]
fn p06_repeated_confirm_refuses_without_changing_durable_state() {
    for format in [1, 2] {
        let f = Fixture::new(format);
        let mut s = f.open();
        let p = package(format, 2);
        stage(&mut s, &p);
        assert_eq!(s.choose().unwrap(), f.target(&p));
        s.confirm(&p.hash).unwrap();
        let ledger = f.ledger();
        let groups = snapshot(&f.root.path().join("versions"));
        assert_eq!(s.confirm(&p.hash), Err(Error::Denied));
        assert_eq!(f.ledger(), ledger);
        drop(s);
        let mut reopened = f.open();
        assert_eq!(reopened.choose().unwrap(), f.target(&p));
        assert_eq!(reopened.floor(), 2);
        assert!(reopened.status()["candidate"].is_null());
        assert_eq!(f.ledger(), ledger);
        assert_eq!(snapshot(&f.root.path().join("versions")), groups);
    }
}
#[test]
fn p07_valid_stale_floor_and_foreign_trust_next_are_never_adopted() {
    for format in [1, 2] {
        for foreign in [false, true] {
            let f = Fixture::new(format);
            let old = f.ledger();
            let mut s = f.open();
            let p = package(format, 2);
            stage(&mut s, &p);
            let ledger = f.ledger();
            let groups = snapshot(&f.root.path().join("versions"));
            drop(s);
            let next = if foreign {
                // A real independently installed ledger, valid and with a HIGHER floor:
                // reject it for its key binding, not accidentally for stale floor or JSON.
                let d = tempfile::tempdir().unwrap();
                mode(d.path(), 0o700);
                let key = SigningKey::from_bytes(&[78; 32]);
                let mut other = Store::open(d.path(), key.verifying_key().to_bytes(), 6).unwrap();
                let mut q = package(format, 3);
                q.signature = key.sign(&signing_bytes(&q.manifest).unwrap()).to_bytes();
                other
                    .install_factory(&q.manifest, &q.signature, &q.payloads)
                    .unwrap();
                drop(other);
                fs::read(d.path().join("state.json")).unwrap()
            } else {
                old
            };
            fs::write(f.next(), next).unwrap();
            mode(&f.next(), 0o600);
            let reopened = f.open();
            assert!(!f.next().exists());
            assert_eq!(f.ledger(), ledger);
            assert_eq!(reopened.floor(), 2);
            assert_eq!(reopened.status()["candidate"], p.hash);
            assert_eq!(reopened.status()["candidate_attempted"], false);
            assert_eq!(snapshot(&f.root.path().join("versions")), groups);
        }
    }
}
