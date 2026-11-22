// SPDX-License-Identifier: AGPL-3.0-only
//! Single-file transport for the existing signed application package.
use crate::{
    manifest_hash, verify, verify_manifest, verify_payload, verify_payload_machine, Error,
    Manifest, MAX_MANIFEST, MAX_PAYLOAD,
};
use serde::Serialize;

const MAGIC: &[u8; 8] = b"RSHAPP01";
const HEADER: usize = MAGIC.len() + 4;
const SIGNATURE: usize = 64;
pub const MAX_BUNDLE: usize = HEADER + MAX_MANIFEST + SIGNATURE + MAX_PAYLOAD as usize;

#[derive(Debug, Serialize)]
pub struct Preview {
    pub verified: bool,
    pub epoch: u64,
    pub manifest_sha256: String,
    pub source_sha256: String,
    pub schema_min: u32,
    pub schema_max: u32,
}

/// Preserve the exact signed manifest, signature, and payload order.
pub fn pack(
    manifest: &[u8],
    signature: &[u8],
    payloads: &[Vec<u8>],
    public_key: &[u8; 32],
    schema: u32,
) -> Result<Vec<u8>, Error> {
    let verified = verify(manifest, signature, public_key, schema, 0)?;
    verify_payload(&verified, payloads)?;
    let size = HEADER + manifest.len() + SIGNATURE + payloads.iter().map(Vec::len).sum::<usize>();
    let mut bundle = Vec::with_capacity(size);
    bundle.extend_from_slice(MAGIC);
    bundle.extend_from_slice(&(manifest.len() as u32).to_be_bytes());
    bundle.extend_from_slice(manifest);
    bundle.extend_from_slice(signature);
    for payload in payloads {
        bundle.extend_from_slice(payload);
    }
    Ok(bundle)
}

/// Check an upload without touching the installed application, writable
/// ledger, or payload paths. A release is authorized separately.
pub fn inspect(
    bytes: &[u8],
    public_key: &[u8; 32],
    schema: u32,
    floor: u64,
) -> Result<Preview, Error> {
    Ok(verify_for_admission(bytes, public_key, schema, floor)?.preview())
}

/// Report the signed package metadata without asserting device admission.
pub fn preview(bytes: &[u8], public_key: &[u8; 32]) -> Result<Preview, Error> {
    Ok(check_bundle(bytes, public_key, None)?.preview())
}

/// Signed bytes preserved for the privileged maintenance admission path.
pub struct SignedBundle<'a> {
    pub manifest: &'a [u8],
    pub signature: &'a [u8],
    pub payloads: Vec<&'a [u8]>,
    parsed: Manifest,
}

impl SignedBundle<'_> {
    pub fn epoch(&self) -> u64 {
        self.parsed.epoch
    }

    fn preview(self) -> Preview {
        Preview {
            verified: true,
            epoch: self.parsed.epoch,
            manifest_sha256: manifest_hash(self.manifest),
            source_sha256: self.parsed.source_sha256,
            schema_min: self.parsed.schema_min,
            schema_max: self.parsed.schema_max,
        }
    }
}

pub fn verify_for_admission<'a>(
    bytes: &'a [u8],
    public_key: &[u8; 32],
    schema: u32,
    floor: u64,
) -> Result<SignedBundle<'a>, Error> {
    check_bundle(bytes, public_key, Some((schema, floor)))
}

fn check_bundle<'a>(
    bytes: &'a [u8],
    public_key: &[u8; 32],
    admission: Option<(u32, u64)>,
) -> Result<SignedBundle<'a>, Error> {
    if bytes.len() > MAX_BUNDLE {
        return Err(Error::Invalid);
    }
    let tail = bytes.strip_prefix(MAGIC).ok_or(Error::Invalid)?;
    let (length, tail) = tail.split_at_checked(4).ok_or(Error::Invalid)?;
    let length = u32::from_be_bytes(length.try_into().map_err(|_| Error::Invalid)?) as usize;
    if length == 0 || length > MAX_MANIFEST {
        return Err(Error::Invalid);
    }
    let (manifest, tail) = tail.split_at_checked(length).ok_or(Error::Invalid)?;
    let (signature, mut tail) = tail.split_at_checked(SIGNATURE).ok_or(Error::Invalid)?;
    let parsed = match admission {
        Some((schema, floor)) => verify(manifest, signature, public_key, schema, floor)?,
        None => verify_manifest(manifest, signature, public_key)?,
    };
    let mut payloads = Vec::with_capacity(parsed.files.len());
    for entry in &parsed.files {
        let (payload, rest) = tail
            .split_at_checked(entry.bytes as usize)
            .ok_or(Error::Integrity)?;
        payloads.push(payload);
        tail = rest;
    }
    if !tail.is_empty() {
        return Err(Error::Invalid);
    }
    verify_payload_machine(&parsed, &payloads, 183)?;
    Ok(SignedBundle {
        manifest,
        signature,
        payloads,
        parsed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    #[test]
    fn preview_checks_original_signature_payload_size_and_epoch() {
        let (key, manifest, payloads) = crate::tests::forwarding_fixture();
        let schema = manifest.schema_min;
        let raw = serde_json::to_vec(&manifest).unwrap();
        let signed = key.sign(&crate::signing_bytes(&raw).unwrap()).to_bytes();
        let public = key.verifying_key().to_bytes();
        let bundle = pack(&raw, &signed, &payloads, &public, schema).unwrap();
        let preview = inspect(&bundle, &public, schema, manifest.epoch - 1).unwrap();
        assert!(preview.verified);
        assert_eq!(preview.epoch, manifest.epoch);
        assert_eq!(preview.manifest_sha256, manifest_hash(&raw));
        assert_eq!(preview.source_sha256, manifest.source_sha256);
        let standalone = super::preview(&bundle, &public).unwrap();
        assert_eq!(standalone.epoch, manifest.epoch);
        assert_eq!(standalone.schema_min, manifest.schema_min);
        assert_eq!(standalone.schema_max, manifest.schema_max);
        let for_admission = verify_for_admission(&bundle, &public, schema, 0).unwrap();
        assert_eq!(for_admission.manifest, raw);
        assert_eq!(for_admission.signature, signed);
        assert_eq!(for_admission.payloads, payloads);
        assert!(matches!(
            verify_for_admission(&bundle, &public, schema + 1, 0),
            Err(Error::Schema)
        ));
        assert!(matches!(
            inspect(&bundle, &public, schema, manifest.epoch),
            Err(Error::Rollback)
        ));
        assert!(matches!(
            inspect(&bundle, &[0; 32], schema, 0),
            Err(Error::Signature)
        ));
        assert!(matches!(
            inspect(&bundle[..bundle.len() - 1], &public, schema, 0),
            Err(Error::Integrity)
        ));
        let mut extra = bundle.clone();
        extra.push(0);
        assert!(matches!(
            inspect(&extra, &public, schema, 0),
            Err(Error::Invalid)
        ));
        let mut bad_length = bundle.clone();
        bad_length[MAGIC.len()..HEADER].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            inspect(&bad_length, &public, schema, 0),
            Err(Error::Invalid)
        ));
        let mut changed = bundle.clone();
        changed[HEADER + raw.len()] ^= 1;
        assert!(matches!(
            inspect(&changed, &public, schema, 0),
            Err(Error::Signature)
        ));
        *changed.last_mut().unwrap() ^= 1;
        changed[HEADER + raw.len()] ^= 1;
        assert!(matches!(
            inspect(&changed, &public, schema, 0),
            Err(Error::Integrity)
        ));
    }
}
