// SPDX-License-Identifier: AGPL-3.0-only
//! Owned immutable bytes checked once with the original signature/ELF/payload
//! verifier. Live slot/ledger guards remain separate and must stay valid.
use crate::{store::SignedGroup, verify, verify_payload, Error, Manifest};

pub(crate) struct VerifiedApplication {
    group: SignedGroup,
    manifest: Manifest,
    key: [u8; 32],
}
impl VerifiedApplication {
    pub(crate) fn verify(group: SignedGroup, key: &[u8; 32], schema: u32) -> Result<Self, Error> {
        Self::verify_using(group, key, schema, verify_payload)
    }
    fn verify_using(
        group: SignedGroup,
        key: &[u8; 32],
        schema: u32,
        check: fn(&Manifest, &[Vec<u8>]) -> Result<(), Error>,
    ) -> Result<Self, Error> {
        let manifest = verify(&group.0, &group.1, key, schema, 0)?;
        check(&manifest, &group.2)?;
        Ok(Self {
            group,
            manifest,
            key: *key,
        })
    }
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
    #[cfg(test)]
    pub(crate) fn verify_native(
        group: SignedGroup,
        key: &[u8; 32],
        schema: u32,
    ) -> Result<Self, Error> {
        Self::verify_using(group, key, schema, crate::verify_native_fixture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;

    #[test]
    fn immutable_application_requires_original_signature_payload_and_schema() {
        let (key, mut manifest, payloads) = crate::tests::forwarding_fixture();
        manifest.schema_max = 9;
        let bytes = serde_json::to_vec(&manifest).unwrap();
        let signature = key
            .sign(&crate::signing_bytes(&bytes).unwrap())
            .to_bytes()
            .to_vec();
        let group = (bytes, signature, payloads);
        let public = key.verifying_key().to_bytes();
        let checked = VerifiedApplication::verify(group.clone(), &public, 6).unwrap();
        checked.require_schema(9).unwrap();
        assert_eq!(checked.require_schema(10), Err(Error::Schema));
        assert_eq!(checked.group(), &group);
        let mut wrong = group.clone();
        wrong.1[0] ^= 1;
        assert!(matches!(
            VerifiedApplication::verify(wrong, &public, 6),
            Err(Error::Signature)
        ));
        let mut wrong = group.clone();
        wrong.2[0][0] ^= 1;
        assert!(matches!(
            VerifiedApplication::verify(wrong, &public, 6),
            Err(Error::Integrity)
        ));
        assert!(VerifiedApplication::verify(group, &[0; 32], 6).is_err());
    }
}
