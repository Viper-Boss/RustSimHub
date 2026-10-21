// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Signed application manifests; never writes boot/rootfs or arbitrary paths.
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const MAX_MANIFEST: usize = 8192;
pub const MAX_PAYLOAD: u64 = 24 * 1024 * 1024;
pub const BOARD: &str = "ufi003-v02";
pub const FILES: [&str; 5] = [
    "rustsimhub-core",
    "simhubctl",
    "rustsimhub-edge",
    "rustsimhub-audio-alsa",
    "Opus-COPYING.txt",
];
const DOMAIN: &[u8] = b"RustSimHub.AppManifest.v1\0";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Denied,
    Invalid,
    Signature,
    Integrity,
    Board,
    Schema,
    Rollback,
    Busy,
    Io,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u16,
    pub kind: String,
    pub board: String,
    pub arch: String,
    pub epoch: u64,
    pub schema_min: u32,
    pub schema_max: u32,
    pub source_sha256: String,
    pub development_fixtures: bool,
    pub files: Vec<Entry>,
}
fn hash_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn hex_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn signing_bytes(manifest: &[u8]) -> Result<Vec<u8>, Error> {
    if manifest.is_empty() || manifest.len() > MAX_MANIFEST {
        return Err(Error::Invalid);
    }
    let mut bytes = DOMAIN.to_vec();
    bytes.extend_from_slice(manifest);
    Ok(bytes)
}
/// Signature is over the exact bytes, not a reserialized JSON document.
pub fn verify(
    manifest: &[u8],
    signature: &[u8],
    public_key: &[u8; 32],
    schema: u32,
    floor: u64,
) -> Result<Manifest, Error> {
    let signing = signing_bytes(manifest)?;
    let signature = Signature::from_slice(signature).map_err(|_| Error::Signature)?;
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| Error::Signature)?;
    key.verify_strict(&signing, &signature)
        .map_err(|_| Error::Signature)?;
    let m: Manifest = serde_json::from_slice(manifest).map_err(|_| Error::Invalid)?;
    if m.format != 1
        || m.kind != "application"
        || m.arch != "aarch64-linux"
        || m.development_fixtures
        || m.epoch == 0
        || !hex_digest(&m.source_sha256)
    {
        return Err(Error::Invalid);
    }
    if m.board != BOARD {
        return Err(Error::Board);
    }
    if m.schema_min > schema || m.schema_max < schema || m.schema_min > m.schema_max {
        return Err(Error::Schema);
    }
    if m.epoch <= floor {
        return Err(Error::Rollback);
    }
    if m.files.len() != FILES.len() {
        return Err(Error::Invalid);
    }
    let mut total = 0u64;
    for (e, name) in m.files.iter().zip(FILES) {
        if e.name != name || e.bytes == 0 || e.bytes > MAX_PAYLOAD || !hex_digest(&e.sha256) {
            return Err(Error::Invalid);
        }
        total = total.checked_add(e.bytes).ok_or(Error::Invalid)?;
    }
    if total > MAX_PAYLOAD {
        return Err(Error::Invalid);
    }
    Ok(m)
}
pub fn verify_payload(m: &Manifest, payloads: &[Vec<u8>]) -> Result<(), Error> {
    if payloads.len() != FILES.len() {
        return Err(Error::Integrity);
    }
    for (e, bytes) in m.files.iter().zip(payloads) {
        if bytes.len() as u64 != e.bytes || hash_hex(bytes) != e.sha256 {
            return Err(Error::Integrity);
        }
        if e.name != "Opus-COPYING.txt"
            && (bytes.len() < 64
                || &bytes[..4] != b"\x7fELF"
                || bytes[4] != 2
                || bytes[5] != 1
                || bytes[6] != 1
                || !matches!(u16::from_le_bytes([bytes[16], bytes[17]]), 2 | 3)
                || u16::from_le_bytes([bytes[52], bytes[53]]) != 64
                || u16::from_le_bytes([bytes[18], bytes[19]]) != 183)
        {
            return Err(Error::Board);
        }
    }
    Ok(())
}
pub fn manifest_hash(bytes: &[u8]) -> String {
    hash_hex(bytes)
}
#[cfg(target_os = "linux")]
pub mod indicator;
#[cfg(target_os = "linux")]
pub mod store;
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    pub(crate) fn fixture() -> (SigningKey, Manifest, Vec<Vec<u8>>) {
        let key = SigningKey::from_bytes(&[77; 32]);
        let payloads = FILES
            .iter()
            .map(|n| {
                if *n == "Opus-COPYING.txt" {
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
            .collect::<Vec<_>>();
        let m = Manifest {
            format: 1,
            kind: "application".into(),
            board: BOARD.into(),
            arch: "aarch64-linux".into(),
            epoch: 1,
            schema_min: 5,
            schema_max: 5,
            source_sha256: "a".repeat(64),
            development_fixtures: false,
            files: FILES
                .iter()
                .zip(&payloads)
                .map(|(n, b)| Entry {
                    name: (*n).into(),
                    bytes: b.len() as u64,
                    sha256: hash_hex(b),
                })
                .collect(),
        };
        (key, m, payloads)
    }
    #[test]
    fn exact_signature_payload_board_schema_and_epoch_are_enforced() {
        let (k, m, p) = fixture();
        let b = serde_json::to_vec(&m).unwrap();
        let s = k.sign(&signing_bytes(&b).unwrap());
        let v = verify(&b, &s.to_bytes(), &k.verifying_key().to_bytes(), 5, 0).unwrap();
        verify_payload(&v, &p).unwrap();
        assert_eq!(
            verify(&b, &s.to_bytes(), &k.verifying_key().to_bytes(), 6, 0).err(),
            Some(Error::Schema)
        );
        assert_eq!(
            verify(&b, &s.to_bytes(), &k.verifying_key().to_bytes(), 5, 1).err(),
            Some(Error::Rollback)
        );
        let mut changed = b.clone();
        changed.push(b' ');
        assert_eq!(
            verify(&changed, &s.to_bytes(), &k.verifying_key().to_bytes(), 5, 0).err(),
            Some(Error::Signature)
        );
        let mut changed = p;
        changed[1][25] = 1;
        assert_eq!(verify_payload(&v, &changed), Err(Error::Integrity));
    }
    #[test]
    fn signed_but_unsafe_manifests_are_rejected() {
        let (k, m, _) = fixture();
        let mut cases = vec![];
        let mut bad = m.clone();
        bad.files[0].name = "../boot".into();
        cases.push(bad);
        let mut bad = m.clone();
        bad.development_fixtures = true;
        cases.push(bad);
        let mut bad = m.clone();
        bad.kind = "system".into();
        cases.push(bad);
        let mut bad = m.clone();
        bad.board = "other-board".into();
        cases.push(bad);
        let mut bad = m.clone();
        bad.files[0].bytes = MAX_PAYLOAD + 1;
        cases.push(bad);
        for bad in cases {
            let b = serde_json::to_vec(&bad).unwrap();
            assert!(verify(
                &b,
                &k.sign(&signing_bytes(&b).unwrap()).to_bytes(),
                &k.verifying_key().to_bytes(),
                5,
                0
            )
            .is_err());
        }
    }
}
