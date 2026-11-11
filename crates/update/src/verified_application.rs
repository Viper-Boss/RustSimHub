// SPDX-License-Identifier: AGPL-3.0-only
//! Owned immutable bytes checked once with the original signature/ELF/payload
//! verifier. Live slot/ledger guards remain separate and must stay valid.
use crate::{store::SignedGroup, verify, verify_payload, Error, Manifest};

pub(crate) struct VerifiedApplication {
    group: SignedGroup,
    manifest: Manifest,
    key: [u8; 32],
}
/// Signature-checked manifest owns the exact envelope while its bounded
/// payload files are loaded. Binding consumes it and checks every payload once.
pub(crate) struct VerifiedManifest {
    bytes: Vec<u8>,
    signature: Vec<u8>,
    manifest: Manifest,
    key: [u8; 32],
}
impl VerifiedManifest {
    pub(crate) fn verify(
        bytes: Vec<u8>,
        signature: Vec<u8>,
        key: &[u8; 32],
        schema: u32,
    ) -> Result<Self, Error> {
        let manifest = verify(&bytes, &signature, key, schema, 0)?;
        Ok(Self {
            bytes,
            signature,
            manifest,
            key: *key,
        })
    }
    pub(crate) fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub(crate) fn bind(self, payloads: Vec<Vec<u8>>) -> Result<VerifiedApplication, Error> {
        self.bind_using(payloads, verify_payload)
    }
    fn bind_using(
        self,
        payloads: Vec<Vec<u8>>,
        check: fn(&Manifest, &[Vec<u8>]) -> Result<(), Error>,
    ) -> Result<VerifiedApplication, Error> {
        check(&self.manifest, &payloads)?;
        Ok(VerifiedApplication {
            group: (self.bytes, self.signature, payloads),
            manifest: self.manifest,
            key: self.key,
        })
    }
    #[cfg(test)]
    pub(crate) fn bind_native(self, payloads: Vec<Vec<u8>>) -> Result<VerifiedApplication, Error> {
        self.bind_using(payloads, crate::verify_native_fixture)
    }
}
impl VerifiedApplication {
    pub(crate) fn group(&self) -> &SignedGroup {
        &self.group
    }
    pub(crate) fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub(crate) fn key(&self) -> &[u8; 32] {
        &self.key
    }
    pub(crate) fn require_schema(&self, schema: u32) -> Result<(), Error> {
        if !(self.manifest.schema_min..=self.manifest.schema_max).contains(&schema) {
            return Err(Error::Schema);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    #[test]
    fn immutable_application_requires_original_signature_payload_and_schema() {
        let verify_group = |group: SignedGroup, key: &[u8; 32], schema: u32| {
            VerifiedManifest::verify(group.0, group.1, key, schema)?.bind(group.2)
        };
        let (key, mut manifest, payloads) = crate::tests::forwarding_fixture();
        manifest.schema_max = 9;
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let signature = key
            .sign(&crate::signing_bytes(&bytes).unwrap())
            .to_bytes()
            .to_vec();
        let group = (bytes, signature, payloads);
        let public = key.verifying_key().to_bytes();
        let checked = verify_group(group.clone(), &public, 6).unwrap();
        checked.require_schema(9).unwrap();
        assert_eq!(checked.require_schema(10), Err(Error::Schema));
        assert_eq!(checked.group(), &group);
        let mut wrong = group.clone();
        wrong.1[0] ^= 1;
        assert!(matches!(
            verify_group(wrong, &public, 6),
            Err(Error::Signature)
        ));
        let mut wrong = group.clone();
        wrong.2[0][0] ^= 1;
        assert!(matches!(
            verify_group(wrong, &public, 6),
            Err(Error::Integrity)
        ));
        assert!(verify_group(group, &[0; 32], 6).is_err());
    }
}
