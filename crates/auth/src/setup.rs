// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit local provisioning window; no ownership reset or implicit production enable.
use crate::{read_private, Auth, Challenge, Error, Principal, Result};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub struct LocalSetup {
    secret: Zeroizing<Vec<u8>>,
    deadline: Instant,
    remaining: u8,
    challenge: Option<String>,
    consumed: bool,
}
impl LocalSetup {
    /// Input is a 32-byte private file, supplied by the trusted local installer.
    /// Its value is delivered out of band, never published in identity or logs.
    pub fn load(path: &Path) -> Result<Self> {
        let secret = read_private(path, 32)?;
        if secret.len() != 32 {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            secret,
            deadline: Instant::now() + Duration::from_secs(300),
            remaining: 16,
            challenge: None,
            consumed: false,
        })
    }
    fn check(&mut self, presented: &str) -> Result<()> {
        if self.consumed || Instant::now() >= self.deadline || self.remaining == 0 {
            return Err(Error::Forbidden);
        }
        self.remaining -= 1;
        if presented.len() != 64
            || !presented
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Forbidden);
        }
        let mut candidate = Zeroizing::new([0u8; 32]);
        for (i, b) in candidate.iter_mut().enumerate() {
            *b = u8::from_str_radix(&presented[i * 2..i * 2 + 2], 16)
                .map_err(|_| Error::Forbidden)?;
        }
        // Compare fixed-domain keyed tags using the upstream constant-time verifier.
        let mut expected =
            Hmac::<Sha256>::new_from_slice(&self.secret).map_err(|_| Error::InvalidInput)?;
        expected.update(b"RustSimHub.LocalSetup.v1\0");
        let mut proposed =
            Hmac::<Sha256>::new_from_slice(candidate.as_ref()).map_err(|_| Error::InvalidInput)?;
        proposed.update(b"RustSimHub.LocalSetup.v1\0");
        expected
            .verify_slice(&proposed.finalize().into_bytes())
            .map_err(|_| Error::Forbidden)
    }
    pub fn available(&self) -> bool {
        !self.consumed && Instant::now() < self.deadline && self.remaining > 0
    }
    pub fn require_empty(&self, auth: &Auth) -> Result<()> {
        if !auth.store.principals()?.is_empty() {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    pub fn begin(&mut self, auth: &mut Auth, code: &str, key: [u8; 32]) -> Result<Challenge> {
        self.check(code)?;
        self.require_empty(auth)?;
        // Only the last issued setup challenge can complete this window.
        let challenge = auth.bootstrap_challenge(key)?;
        self.challenge = Some(challenge.id.clone());
        Ok(challenge)
    }
    pub fn finish(
        &mut self,
        auth: &mut Auth,
        code: &str,
        id: &str,
        signature: &[u8],
    ) -> Result<Principal> {
        self.check(code)?;
        self.require_empty(auth)?;
        if self.challenge.take().as_deref() != Some(id) {
            return Err(Error::Replay);
        }
        let owner = auth.bootstrap_finish(id, signature)?;
        self.consumed = true;
        self.secret.fill(0);
        Ok(owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_storage::Store;
    use std::fs;
    #[test]
    fn local_code_signed_key_single_owner_expiry_and_last_challenge_are_required() {
        let state = tempfile::tempdir().unwrap();
        let identity = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for dir in [&state, &identity, &config] {
                fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        crate::write_private_new(&config.path().join("setup.code"), &[11; 32]).unwrap();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let mut gate = LocalSetup::load(&config.path().join("setup.code")).unwrap();
        let code = "0b".repeat(32);
        let key = SigningKey::from_bytes(&[12; 32]);
        let public = key.verifying_key().to_bytes();
        assert!(gate.begin(&mut auth, &"00".repeat(32), public).is_err());
        let first = gate.begin(&mut auth, &code, public).unwrap();
        let latest = gate.begin(&mut auth, &code, public).unwrap();
        let sig = key.sign(&first.transcript).to_bytes();
        assert!(gate.finish(&mut auth, &code, &first.id, &sig).is_err());
        // A mismatched finish consumes the local pending proof; obtain a new challenge.
        assert!(gate
            .finish(
                &mut auth,
                &code,
                &latest.id,
                &key.sign(&latest.transcript).to_bytes()
            )
            .is_err());
        let c = gate.begin(&mut auth, &code, public).unwrap();
        assert!(gate.finish(&mut auth, &code, &c.id, &[0; 64]).is_err());
        assert!(gate
            .finish(&mut auth, &code, &c.id, &key.sign(&c.transcript).to_bytes())
            .is_err());
        let c = gate.begin(&mut auth, &code, public).unwrap();
        let owner = gate
            .finish(&mut auth, &code, &c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        assert_eq!(owner.kind, crate::Kind::Owner);
        assert!(gate.begin(&mut auth, &code, public).is_err());
        // Reopening a local window still cannot replace any existing principal/owner.
        let mut other = LocalSetup::load(&config.path().join("setup.code")).unwrap();
        assert!(other.begin(&mut auth, &code, public).is_err());
        other.deadline = Instant::now() - Duration::from_secs(1);
        assert!(other.check(&code).is_err());
    }
    #[test]
    fn setup_file_is_private_bounded_and_attempt_budget_is_finite() {
        let config = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(config.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = config.path().join("code");
        crate::write_private_new(&path, &[11; 31]).unwrap();
        assert!(LocalSetup::load(&path).is_err());
        fs::remove_file(&path).unwrap();
        crate::write_private_new(&path, &[11; 32]).unwrap();
        let mut gate = LocalSetup::load(&path).unwrap();
        for _ in 0..16 {
            assert!(gate.check("wrong").is_err());
        }
        assert!(gate.check(&"0b".repeat(32)).is_err());
    }
}
