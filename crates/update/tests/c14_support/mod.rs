// SPDX-License-Identifier: AGPL-3.0-only
// Synthetic package construction follows the existing C13 fixture, not an updater implementation.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_update::{
    application_files, manifest_hash, signing_bytes, store::Store, Entry, Manifest, BOARD,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
pub struct Package {
    pub manifest: Vec<u8>,
    pub signature: [u8; 64],
    pub payloads: Vec<Vec<u8>>,
    pub hash: String,
}
pub fn package(format: u16, epoch: u64, schema: u32) -> Package {
    let key = SigningKey::from_bytes(&[77; 32]);
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
        schema_min: schema,
        schema_max: schema,
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
pub fn mode(p: &Path, b: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(b)).unwrap();
}
pub fn open(p: &Path) -> Store {
    Store::open(
        p,
        SigningKey::from_bytes(&[77; 32]).verifying_key().to_bytes(),
        6,
    )
    .unwrap()
}
pub struct Fixture {
    pub root: tempfile::TempDir,
    pub a: PathBuf,
}
impl Fixture {
    pub fn new(format: u16, parent: Option<&Path>) -> Self {
        assert_ne!(rustix::process::geteuid().as_raw(), 0);
        let root = match parent {
            Some(p) => tempfile::tempdir_in(p),
            None => tempfile::tempdir(),
        }
        .unwrap();
        mode(root.path(), 0o700);
        let p = package(format, 1, 6);
        let a = open(root.path())
            .install_factory(&p.manifest, &p.signature, &p.payloads)
            .unwrap();
        Self { root, a }
    }
    pub fn ledger(&self) -> Vec<u8> {
        fs::read(self.root.path().join("state.json")).unwrap()
    }
    pub fn target(&self, p: &Package) -> PathBuf {
        self.root.path().join("versions").join(&p.hash)
    }
}
pub fn snapshot(p: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, p: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in fs::read_dir(p).unwrap() {
            let p = e.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            assert!(!m.file_type().is_symlink());
            if m.is_dir() {
                walk(root, &p, out)
            } else {
                assert!(m.is_file());
                out.insert(
                    p.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(p).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(p, p, &mut out);
    out
}
