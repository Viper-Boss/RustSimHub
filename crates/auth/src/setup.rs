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

/// Effectively unlimited for an unowned device (user decision 2026-10-29).
const DEVICE_WINDOW: Duration = Duration::from_secs(10 * 365 * 24 * 3600);
pub struct LocalSetup {
    secret: Zeroizing<Vec<u8>>,
    deadline: Instant,
    remaining: u8,
    challenge: Option<String>,
    consumed: bool,
    device: Option<(String, [u8; 32], String)>,
    claimed: Option<Principal>,
}
/// Local-only proof context. Not serialized or accepted from a remote request.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceSetupContext {
    pub window_id: String,
    pub device_public_key: [u8; 32],
    pub realm: String,
    pub remaining: Duration,
    pub claimed: Option<Principal>,
}
impl LocalSetup {
    /// Explicit local installer preparation; never opens or renews a window.
    /// The code stays in its private file and is not returned or logged.
    pub fn prepare_device_code(directory: &Path, auth: &Auth) -> Result<()> {
        #[cfg(target_os = "linux")]
        let _lock = crate::lock_private_directory(directory)?;
        Self::prepare_device_code_unlocked(directory, auth)
    }
    fn prepare_device_code_unlocked(directory: &Path, auth: &Auth) -> Result<()> {
        crate::identity::check(directory, true)?;
        match std::fs::symlink_metadata(directory.join("setup.used")) {
            Ok(_) => return Err(Error::Forbidden),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Io),
        }
        if !auth.store.principals()?.is_empty() {
            return Err(Error::Forbidden);
        }
        let code = directory.join("setup.code");
        match std::fs::symlink_metadata(&code) {
            Ok(_) => {
                // Resume an interrupted local installation without changing the secret.
                if read_private(&code, 32)?.len() != 32 {
                    return Err(Error::InvalidInput);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let mut secret = Zeroizing::new([0u8; 32]);
                getrandom::fill(secret.as_mut()).map_err(|_| Error::Entropy)?;
                crate::write_private_new(&code, secret.as_ref())?;
            }
            Err(_) => return Err(Error::Io),
        }
        Ok(())
    }
    /// Export only the actual offline Core identity/realm. Does not open a window or renew setup.
    #[cfg(target_os = "linux")]
    pub fn prepare_device_ap(directory: &Path, auth: &Auth) -> Result<()> {
        Self::prepare_binding(directory, auth, true)
    }
    /// Owner-preserving maintenance export. Never creates a setup code/window.
    #[cfg(target_os = "linux")]
    pub fn prepare_device_ap_binding(directory: &Path, auth: &Auth) -> Result<()> {
        Self::prepare_binding(directory, auth, false)
    }
    #[cfg(target_os = "linux")]
    fn prepare_binding(directory: &Path, auth: &Auth, prepare_code: bool) -> Result<()> {
        let uid = rustix::process::geteuid().as_raw();
        if uid == 0 || uid == u32::MAX {
            return Err(Error::Forbidden);
        }
        let _lock = crate::lock_private_directory(directory)?;
        let bytes=serde_json::to_vec(&serde_json::json!({"format":1,"realm":auth.durable_realm()?,"core_uid":uid,"device_public_key":auth.public_key()})).map_err(|_|Error::InvalidInput)?;
        let path = directory.join("binding.json");
        let existing = match std::fs::symlink_metadata(&path) {
            Ok(_) => {
                if read_private(&path, 4096)?.as_slice() != bytes {
                    return Err(Error::Forbidden);
                }
                true
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(Error::Io),
        };
        if prepare_code {
            Self::prepare_device_code_unlocked(directory, auth)?;
        }
        if !existing {
            crate::write_private_new(&path, &bytes)?;
        }
        Ok(())
    }
    /// Explicit device startup: one process may open a locally requested window.
    /// A synced, identity-bound marker survives both process and business DB loss.
    /// This never renews a window or removes a marker; only trusted maintenance
    /// may request another window after inspecting the durable state.
    pub fn for_device(directory: &Path, auth: &Auth) -> Result<Option<Self>> {
        #[cfg(target_os = "linux")]
        let _lock = crate::lock_private_directory(directory)?;
        crate::identity::check(directory, true)?;
        let marker = directory.join("setup.used");
        let mut expected = b"RustSimHub.DeviceSetup.Window.v1\0".to_vec();
        expected.extend_from_slice(&auth.public_key());
        // User decision 2026-10-29: an unowned device stays in first setup across reboots
        // (a power cut before setup must not lock the user out). The marker only seals the
        // window once an Owner exists; a foreign marker is still refused.
        let marked = match std::fs::symlink_metadata(&marker) {
            Ok(_) => {
                if read_private(&marker, expected.len())?.as_slice() != expected {
                    return Err(Error::Forbidden);
                }
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(Error::Io),
        };
        if !auth.store.principals()?.is_empty() {
            if !marked {
                crate::write_private_new(&marker, &expected)?;
            }
            return Ok(None);
        }
        let code = directory.join("setup.code");
        match std::fs::symlink_metadata(&code) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Io),
        }
        let mut gate = Self::load(&code)?;
        gate.device = Some((
            crate::random_hex()?,
            auth.public_key(),
            auth.durable_realm()?,
        ));
        // No expiry while unowned; each hotspot round gets a fresh attempt budget.
        gate.deadline = Instant::now() + DEVICE_WINDOW;
        gate.remaining = u8::MAX;
        if !marked {
            crate::write_private_new(&marker, &expected)?;
        }
        Ok(Some(gate))
    }

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
            device: None,
            claimed: None,
        })
    }
    /// A real device window, including its original bounded claim-to-configuration period.
    /// Explicit load(), exhausted attempts, changed realm/key or replaced/revoked Owner never qualify.
    pub fn device_ap_context(&self, auth: &Auth) -> Result<DeviceSetupContext> {
        let (id, key, realm) = self.device.as_ref().ok_or(Error::Forbidden)?;
        if Instant::now() >= self.deadline
            || *key != auth.public_key()
            || *realm != auth.durable_realm()?
        {
            return Err(Error::Forbidden);
        }
        if self.consumed {
            let claimed = self.claimed.as_ref().ok_or(Error::Forbidden)?;
            if claimed.kind != crate::Kind::Owner
                || auth.current_principal(claimed.public_key)? != *claimed
            {
                return Err(Error::Forbidden);
            }
        } else {
            if self.remaining == 0 {
                return Err(Error::Forbidden);
            }
            self.require_empty(auth)?;
        }
        Ok(DeviceSetupContext {
            window_id: id.clone(),
            device_public_key: *key,
            realm: realm.clone(),
            remaining: self.deadline.saturating_duration_since(Instant::now()),
            claimed: self.claimed.clone(),
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
    /// Same one-shot window without the out-of-band code. The caller (Core) allows it only
    /// while the first-setup AP is Ready, i.e. the claimant is on the device's own hotspot.
    /// Attempts still count against the window, and the store must still have no principal.
    fn check_open(&mut self) -> Result<()> {
        if self.consumed || Instant::now() >= self.deadline || self.remaining == 0 {
            return Err(Error::Forbidden);
        }
        self.remaining -= 1;
        Ok(())
    }
    pub fn begin_open(&mut self, auth: &mut Auth, key: [u8; 32]) -> Result<Challenge> {
        self.check_open()?;
        self.require_empty(auth)?;
        let challenge = auth.bootstrap_challenge(key)?;
        self.challenge = Some(challenge.id.clone());
        Ok(challenge)
    }
    pub fn finish_open(
        &mut self,
        auth: &mut Auth,
        id: &str,
        signature: &[u8],
    ) -> Result<Principal> {
        self.check_open()?;
        self.require_empty(auth)?;
        if self.challenge.take().as_deref() != Some(id) {
            return Err(Error::Replay);
        }
        let owner = auth.bootstrap_finish(id, signature)?;
        self.consumed = true;
        self.claimed = Some(owner.clone());
        self.secret.fill(0);
        Ok(owner)
    }
    /// Device window for an owned device that may still lack Wi-Fi (user decision 2026-10-29:
    /// a power cut after creating the Owner must not lock first setup out). It is already
    /// consumed by the single active Owner, so it can never create another principal; Core opens
    /// rounds from it only while no Wi-Fi is saved.
    pub fn for_owned_device(directory: &Path, auth: &Auth) -> Result<Option<Self>> {
        #[cfg(target_os = "linux")]
        let _lock = crate::lock_private_directory(directory)?;
        crate::identity::check(directory, true)?;
        let marker = directory.join("setup.used");
        let mut expected = b"RustSimHub.DeviceSetup.Window.v1\0".to_vec();
        expected.extend_from_slice(&auth.public_key());
        match std::fs::symlink_metadata(&marker) {
            Ok(_) if read_private(&marker, expected.len())?.as_slice() == expected => {}
            Ok(_) => return Err(Error::Forbidden),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(Error::Io),
        }
        let owners: Vec<Principal> = auth
            .store
            .principals()?
            .into_iter()
            .filter(|p| p.kind == crate::Kind::Owner && p.active)
            .collect();
        let [owner] = owners.as_slice() else {
            return Ok(None);
        };
        let mut secret = Zeroizing::new(vec![0u8; 32]);
        getrandom::fill(secret.as_mut_slice()).map_err(|_| Error::Entropy)?;
        Ok(Some(Self {
            secret,
            deadline: Instant::now() + DEVICE_WINDOW,
            remaining: u8::MAX,
            challenge: None,
            consumed: true,
            device: Some((
                crate::random_hex()?,
                auth.public_key(),
                auth.durable_realm()?,
            )),
            claimed: Some(owner.clone()),
        }))
    }
    /// Next hotspot round of the same device window: a new window id (fresh journal names) and
    /// attempt budget. An unowned window needs an empty store; a consumed one needs its
    /// claimant to still be the current Owner.
    pub fn next_device_window(&mut self, auth: &Auth) -> Result<()> {
        if self.device.is_none() {
            return Err(Error::Forbidden);
        }
        if self.consumed {
            let claimed = self.claimed.as_ref().ok_or(Error::Forbidden)?;
            if claimed.kind != crate::Kind::Owner
                || auth.current_principal(claimed.public_key)? != *claimed
            {
                return Err(Error::Forbidden);
            }
        } else {
            self.require_empty(auth)?;
        }
        if let Some((id, _, _)) = self.device.as_mut() {
            *id = crate::random_hex()?;
        }
        self.challenge = None;
        self.remaining = u8::MAX;
        Ok(())
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
        self.claimed = Some(owner.clone());
        self.secret.fill(0);
        Ok(owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn binding_only_export_does_not_create_an_anonymous_setup_window() {
        if rustix::process::geteuid().as_raw() == 0 {
            return;
        }
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_ap_binding(setup.path(), &auth).unwrap();
        assert!(!setup.path().join("setup.code").exists());
        assert!(!setup.path().join("setup.used").exists());
        let original = std::fs::read(setup.path().join("binding.json")).unwrap();
        LocalSetup::prepare_device_ap_binding(setup.path(), &auth).unwrap();
        assert_eq!(
            std::fs::read(setup.path().join("binding.json")).unwrap(),
            original
        );
        let mut record: serde_json::Value = serde_json::from_slice(&original).unwrap();
        record["realm"] = "00000000-0000-4000-8000-000000000001".into();
        std::fs::write(
            setup.path().join("binding.json"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        assert!(LocalSetup::prepare_device_ap_binding(setup.path(), &auth).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn existing_owner_binding_export_keeps_used_marker_code_and_principal() {
        if rustix::process::geteuid().as_raw() == 0 {
            return;
        }
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        let mut gate = LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        let marker = std::fs::read(setup.path().join("setup.used")).unwrap();
        let raw = read_private(&setup.path().join("setup.code"), 32).unwrap();
        let code = raw.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let key = SigningKey::from_bytes(&[93; 32]);
        let c = gate
            .begin(&mut auth, &code, key.verifying_key().to_bytes())
            .unwrap();
        let principal = gate
            .finish(&mut auth, &code, &c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        LocalSetup::prepare_device_ap_binding(setup.path(), &auth).unwrap();
        assert_eq!(
            auth.current_principal(principal.public_key).unwrap(),
            principal
        );
        assert_eq!(
            std::fs::read(setup.path().join("setup.used")).unwrap(),
            marker
        );
        assert_eq!(
            read_private(&setup.path().join("setup.code"), 32).unwrap(),
            raw
        );
        assert!(LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .is_none());
        assert!(LocalSetup::prepare_device_ap(setup.path(), &auth).is_err());
    }
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_storage::Store;
    use std::fs;
    #[test]
    fn device_ap_proof_is_local_one_shot_and_survives_only_the_original_signed_claim_period() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        let explicit = LocalSetup::load(&setup.path().join("setup.code")).unwrap();
        assert!(explicit.device_ap_context(&auth).is_err());
        let mut gate = LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        let before = gate.device_ap_context(&auth).unwrap();
        assert!(before.claimed.is_none());
        assert_eq!(before.device_public_key, auth.public_key());
        let code = read_private(&setup.path().join("setup.code"), 32)
            .unwrap()
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<String>();
        let key = SigningKey::from_bytes(&[92; 32]);
        let challenge = gate
            .begin(&mut auth, &code, key.verifying_key().to_bytes())
            .unwrap();
        let owner = gate
            .finish(
                &mut auth,
                &code,
                &challenge.id,
                &key.sign(&challenge.transcript).to_bytes(),
            )
            .unwrap();
        let after = gate.device_ap_context(&auth).unwrap();
        assert!(!gate.available());
        assert_eq!(after.claimed, Some(owner));
        assert_eq!(before.window_id, after.window_id);
        assert!(after.remaining <= before.remaining);
        assert!(LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .is_none());
        gate.deadline = Instant::now() - Duration::from_secs(1);
        assert!(gate.device_ap_context(&auth).is_err());
    }
    #[test]
    fn open_claim_is_one_shot_signed_budgeted_and_never_replaces_an_owner() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        let mut gate = LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        let key = SigningKey::from_bytes(&[41; 32]);
        let public = key.verifying_key().to_bytes();
        // Only the last challenge can finish; a bad signature burns that challenge.
        let first = gate.begin_open(&mut auth, public).unwrap();
        let c = gate.begin_open(&mut auth, public).unwrap();
        assert!(gate
            .finish_open(
                &mut auth,
                &first.id,
                &key.sign(&first.transcript).to_bytes()
            )
            .is_err());
        let c2 = gate.begin_open(&mut auth, public).unwrap();
        assert!(gate.finish_open(&mut auth, &c2.id, &[0; 64]).is_err());
        assert!(gate
            .finish_open(&mut auth, &c.id, &key.sign(&c.transcript).to_bytes())
            .is_err());
        let c = gate.begin_open(&mut auth, public).unwrap();
        let owner = gate
            .finish_open(&mut auth, &c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        assert_eq!(owner.kind, crate::Kind::Owner);
        assert_eq!(owner.public_key, public);
        // Window consumed: neither the open path nor the coded path can claim again.
        assert!(gate.begin_open(&mut auth, [7; 32]).is_err());
        assert_eq!(gate.device_ap_context(&auth).unwrap().claimed, Some(owner));
        // A fresh window on an owned store refuses even before issuing a challenge.
        let other = private_directory();
        LocalSetup::prepare_device_code(other.path(), &auth).unwrap_or(());
        let mut loaded = LocalSetup::load(&setup.path().join("setup.code")).unwrap();
        assert!(loaded.begin_open(&mut auth, [7; 32]).is_err());
    }
    #[test]
    fn owned_device_window_needs_the_marker_and_exactly_one_owner_and_rotates() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        // No marker yet (window never opened): nothing.
        assert!(LocalSetup::for_owned_device(setup.path(), &auth)
            .unwrap()
            .is_none());
        let mut gate = LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        // Marker present but no Owner: nothing (the ordinary unowned window applies).
        assert!(LocalSetup::for_owned_device(setup.path(), &auth)
            .unwrap()
            .is_none());
        let key = SigningKey::from_bytes(&[44; 32]);
        let c = gate
            .begin_open(&mut auth, key.verifying_key().to_bytes())
            .unwrap();
        let owner = gate
            .finish_open(&mut auth, &c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        // After a restart: the owned window is consumed by that Owner and cannot claim again.
        let mut owned = LocalSetup::for_owned_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        let first = owned.device_ap_context(&auth).unwrap();
        assert_eq!(first.claimed, Some(owner));
        assert!(owned.begin_open(&mut auth, [9; 32]).is_err());
        owned.next_device_window(&auth).unwrap();
        assert_ne!(
            owned.device_ap_context(&auth).unwrap().window_id,
            first.window_id
        );
    }
    #[test]
    fn open_claim_attempt_budget_and_deadline_are_finite() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        let mut gate = LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        let key = SigningKey::from_bytes(&[42; 32]);
        let public = key.verifying_key().to_bytes();
        let mut issued = 0;
        while gate.begin_open(&mut auth, public).is_ok() {
            issued += 1;
            assert!(issued <= 255);
        }
        assert!(issued > 0);
        // A new hotspot round refreshes the budget under a new window id; an Owner ends it.
        let before = gate.device_ap_context(&auth).ok().map(|c| c.window_id);
        gate.next_device_window(&auth).unwrap();
        let after = gate.device_ap_context(&auth).unwrap().window_id;
        assert_ne!(before.as_deref(), Some(after.as_str()));
        assert!(gate.available());
        // Any principal in the store ends the rounds.
        let owned_state = private_directory();
        let owned_identity = private_directory();
        let owned_store = Store::open(owned_state.path()).unwrap();
        let mut owned = Auth::open(owned_identity.path(), owned_store.handle()).unwrap();
        let k = SigningKey::from_bytes(&[43; 32]);
        let c = owned
            .bootstrap_challenge(k.verifying_key().to_bytes())
            .unwrap();
        owned
            .bootstrap_finish(&c.id, &k.sign(&c.transcript).to_bytes())
            .unwrap();
        assert!(gate.next_device_window(&owned).is_err());
        // The explicit installer window keeps its fixed 5-minute expiry.
        let mut gate = LocalSetup::load(&setup.path().join("setup.code")).unwrap();
        gate.deadline = Instant::now() - Duration::from_secs(1);
        assert!(gate.begin_open(&mut auth, public).is_err());
    }
    #[test]
    fn device_ap_proof_is_denied_after_exhausted_attempts_or_outside_owner_creation() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        let mut gate = LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .unwrap();
        for _ in 0..u8::MAX {
            assert!(gate.begin(&mut auth, "bad", [4; 32]).is_err());
        }
        assert!(gate.device_ap_context(&auth).is_err());
        let other = private_directory();
        LocalSetup::prepare_device_code(other.path(), &auth).unwrap();
        let gate = LocalSetup::for_device(other.path(), &auth)
            .unwrap()
            .unwrap();
        let key = SigningKey::from_bytes(&[92; 32]);
        let c = auth
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        assert!(gate.device_ap_context(&auth).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn ap_binding_comes_from_real_core_and_does_not_reopen_used_setup() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let auth = Auth::open(identity.path(), store.handle()).unwrap();
        if rustix::process::geteuid().as_raw() == 0 {
            assert!(LocalSetup::prepare_device_ap(setup.path(), &auth).is_err());
            return;
        }
        LocalSetup::prepare_device_ap(setup.path(), &auth).unwrap();
        let code = fs::read(setup.path().join("setup.code")).unwrap();
        let binding = fs::read(setup.path().join("binding.json")).unwrap();
        let data: serde_json::Value = serde_json::from_slice(&binding).unwrap();
        assert_eq!(data["realm"], auth.durable_realm().unwrap());
        assert_eq!(
            data["device_public_key"],
            serde_json::json!(auth.public_key())
        );
        assert_eq!(data["core_uid"], rustix::process::geteuid().as_raw());
        assert!(!setup.path().join("setup.used").exists());
        LocalSetup::prepare_device_ap(setup.path(), &auth).unwrap();
        assert_eq!(code, fs::read(setup.path().join("setup.code")).unwrap());
        assert!(LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .is_some());
        assert!(LocalSetup::prepare_device_ap(setup.path(), &auth).is_err());
        assert_eq!(
            binding,
            fs::read(setup.path().join("binding.json")).unwrap()
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn wrong_binding_or_directory_contention_cannot_create_a_new_setup_secret() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let auth = Auth::open(identity.path(), store.handle()).unwrap();
        crate::write_private_new(&setup.path().join("binding.json"), b"{}").unwrap();
        assert!(LocalSetup::prepare_device_ap(setup.path(), &auth).is_err());
        assert!(!setup.path().join("setup.code").exists());
        let _lock = crate::lock_private_directory(setup.path()).unwrap();
        assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
        assert!(LocalSetup::for_device(setup.path(), &auth).is_err());
        assert!(!setup.path().join("setup.used").exists());
    }
    fn private_directory() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        dir
    }
    #[test]
    fn local_code_preparation_is_private_idempotent_and_never_reopens_a_used_window() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let auth = Auth::open(identity.path(), store.handle()).unwrap();
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        let code = setup.path().join("setup.code");
        let before = read_private(&code, 32).unwrap();
        assert_eq!(before.len(), 32);
        LocalSetup::prepare_device_code(setup.path(), &auth).unwrap();
        assert_eq!(read_private(&code, 32).unwrap(), before);
        assert!(!setup.path().join("setup.used").exists());
        assert!(LocalSetup::for_device(setup.path(), &auth)
            .unwrap()
            .is_some());
        assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
        fs::remove_file(&code).unwrap();
        assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
        assert!(!code.exists());
        #[cfg(target_os = "linux")]
        assert!(LocalSetup::prepare_device_ap(setup.path(), &auth).is_err());
    }
    #[test]
    fn owned_or_malformed_local_setup_is_not_repaired_or_overwritten() {
        let state = private_directory();
        let identity = private_directory();
        let setup = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let code = setup.path().join("setup.code");
        crate::write_private_new(&code, b"torn").unwrap();
        assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
        assert_eq!(fs::read(&code).unwrap(), b"torn");
        fs::remove_file(&code).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("missing", setup.path().join("setup.used")).unwrap();
            assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
            fs::remove_file(setup.path().join("setup.used")).unwrap();
            std::os::unix::fs::symlink("missing", &code).unwrap();
            assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
            fs::remove_file(&code).unwrap();
        }
        let key = SigningKey::from_bytes(&[12; 32]);
        let challenge = auth
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&challenge.id, &key.sign(&challenge.transcript).to_bytes())
            .unwrap();
        assert!(LocalSetup::prepare_device_code(setup.path(), &auth).is_err());
        #[cfg(target_os = "linux")]
        assert!(LocalSetup::prepare_device_ap(setup.path(), &auth).is_err());
        assert!(!setup.path().join("binding.json").exists());
        assert!(!code.exists());
    }
    #[test]
    fn device_window_is_opt_in_reopens_while_unowned_and_never_after_an_owner() {
        let state = private_directory();
        let identity = private_directory();
        let config = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .is_none());
        assert!(!config.path().join("setup.used").exists());
        crate::write_private_new(&config.path().join("setup.code"), &[11; 32]).unwrap();
        let mut gate = LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .unwrap();
        // User decision 2026-10-29: while unowned, a restart reopens the window.
        assert!(LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .is_some());
        let key = SigningKey::from_bytes(&[12; 32]);
        let challenge = gate
            .begin(&mut auth, &"0b".repeat(32), key.verifying_key().to_bytes())
            .unwrap();
        gate.finish(
            &mut auth,
            &"0b".repeat(32),
            &challenge.id,
            &key.sign(&challenge.transcript).to_bytes(),
        )
        .unwrap();
        assert!(!gate.available());
        let pin = auth.public_key();
        drop(gate);
        drop(auth);
        drop(store);
        // Missing setup code does not break a consumed window. A new business
        // DB with the old identity still cannot silently re-enrol an owner.
        fs::remove_file(config.path().join("setup.code")).unwrap();
        let lost_db = private_directory();
        let store = Store::open(lost_db.path()).unwrap();
        let auth = Auth::open(identity.path(), store.handle()).unwrap();
        assert_eq!(auth.public_key(), pin);
        assert!(LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .is_none());
    }
    #[test]
    fn existing_principal_seals_device_startup_without_reading_setup_code() {
        let state = private_directory();
        let identity = private_directory();
        let config = private_directory();
        let store = Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let key = SigningKey::from_bytes(&[12; 32]);
        let c = auth
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        // Corrupt code is irrelevant after existing ownership is observed.
        crate::write_private_new(&config.path().join("setup.code"), b"bad").unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .is_none());
        let marker = config.path().join("setup.used");
        let original = read_private(&marker, 128).unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .is_none());
        assert_eq!(read_private(&marker, 128).unwrap(), original);
        fs::write(&marker, b"truncated").unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn device_marker_links_and_foreign_identity_are_rejected_without_replacement() {
        let state = private_directory();
        let identity = private_directory();
        let config = private_directory();
        let store = Store::open(state.path()).unwrap();
        let auth = Auth::open(identity.path(), store.handle()).unwrap();
        crate::write_private_new(&config.path().join("setup.code"), &[11; 32]).unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth)
            .unwrap()
            .is_some());
        let marker = config.path().join("setup.used");
        let saved = read_private(&marker, 128).unwrap();
        let other_state = private_directory();
        let other_identity = private_directory();
        let other_store = Store::open(other_state.path()).unwrap();
        let other = Auth::open(other_identity.path(), other_store.handle()).unwrap();
        assert!(LocalSetup::for_device(config.path(), &other).is_err());
        assert_eq!(read_private(&marker, 128).unwrap(), saved);
        fs::hard_link(&marker, config.path().join("linked")).unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth).is_err());
        fs::remove_file(config.path().join("linked")).unwrap();
        fs::rename(&marker, config.path().join("saved")).unwrap();
        std::os::unix::fs::symlink("saved", &marker).unwrap();
        assert!(LocalSetup::for_device(config.path(), &auth).is_err());
    }

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
