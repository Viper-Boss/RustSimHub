// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Offline age/X25519 identity backup. No passwords, SSH keys, plugins, remote restore or overwrite.
use age::secrecy::ExposeSecret;
use ed25519_dalek::SigningKey;
use rustsimhub_auth::{read_private, write_private_new, Error, Identity, Result};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;
const MAGIC: &[u8; 8] = b"RSHIBK01";
const PLAINTEXT: usize = 112;
const MAX_ENCRYPTED: usize = 8192;
pub fn keygen(directory: &Path) -> Result<()> {
    let key = age::x25519::Identity::generate();
    let secret = key.to_string();
    write_private_new(
        &directory.join("recovery.agekey"),
        secret.expose_secret().as_bytes(),
    )?;
    write_private_new(
        &directory.join("recipient.txt"),
        key.to_public().to_string().as_bytes(),
    )
}
fn recipient(path: &Path) -> Result<age::x25519::Recipient> {
    let bytes = read_private(path, 128)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| Error::InvalidInput)?
        .parse()
        .map_err(|_| Error::InvalidInput)
}
fn private_key(path: &Path) -> Result<age::x25519::Identity> {
    let bytes = read_private(path, 128)?;
    std::str::from_utf8(&bytes)
        .map_err(|_| Error::InvalidInput)?
        .parse()
        .map_err(|_| Error::InvalidInput)
}
fn pin_from_seed(seed: &[u8]) -> Result<[u8; 32]> {
    if seed.len() != 72
        || &seed[..8] != b"RSHKEY01"
        || Sha256::digest(&seed[..40])[..] != seed[40..]
    {
        return Err(Error::CorruptIdentity);
    }
    let mut private = Zeroizing::new([0u8; 32]);
    private.copy_from_slice(&seed[8..40]);
    Ok(SigningKey::from_bytes(&private).verifying_key().to_bytes())
}
pub fn backup(identity: &Path, public: &Path, output: &Path) -> Result<()> {
    // Read first: a backup never creates a missing identity.
    let seed = read_private(&identity.join("identity.seed"), 72)?;
    let pin = pin_from_seed(&seed)?;
    if Identity::load(identity)?.public_key() != pin {
        return Err(Error::CorruptIdentity);
    }
    let to = recipient(public)?;
    let mut plain = Zeroizing::new(Vec::with_capacity(PLAINTEXT));
    plain.extend_from_slice(MAGIC);
    plain.extend_from_slice(&pin);
    plain.extend_from_slice(&seed);
    let encryptor = age::Encryptor::with_recipients(std::iter::once(&to as &dyn age::Recipient))
        .map_err(|_| Error::InvalidInput)?;
    let mut writer = encryptor.wrap_output(Vec::new()).map_err(|_| Error::Io)?;
    writer.write_all(&plain).map_err(|_| Error::Io)?;
    let encrypted = writer.finish().map_err(|_| Error::Io)?;
    if encrypted.len() > MAX_ENCRYPTED {
        return Err(Error::InvalidInput);
    }
    write_private_new(output, &encrypted)
}
pub fn restore(key: &Path, backup: &Path, expected: [u8; 32], destination: &Path) -> Result<()> {
    if !destination.is_absolute() {
        return Err(Error::Forbidden);
    }
    let m = std::fs::symlink_metadata(destination).map_err(|_| Error::Io)?;
    if !m.is_dir() {
        return Err(Error::Forbidden);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        if m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o077 != 0 {
            return Err(Error::Forbidden);
        }
    }
    if std::fs::read_dir(destination)
        .map_err(|_| Error::Io)?
        .next()
        .is_some()
    {
        return Err(Error::Forbidden);
    }
    let key = private_key(key)?;
    let encrypted = read_private(backup, MAX_ENCRYPTED)?;
    let decryptor =
        age::Decryptor::new_buffered(encrypted.as_slice()).map_err(|_| Error::InvalidInput)?;
    if decryptor.is_scrypt() {
        return Err(Error::Forbidden);
    }
    let mut reader = decryptor
        .decrypt(std::iter::once(&key as &dyn age::Identity))
        .map_err(|_| Error::Forbidden)?;
    let mut plain = Zeroizing::new(Vec::with_capacity(PLAINTEXT + 1));
    reader
        .by_ref()
        .take((PLAINTEXT + 1) as u64)
        .read_to_end(&mut plain)
        .map_err(|_| Error::Forbidden)?;
    if plain.len() != PLAINTEXT
        || &plain[..8] != MAGIC
        || plain[8..40] != expected
        || pin_from_seed(&plain[40..])? != expected
    {
        return Err(Error::Forbidden);
    }
    // No decrypted bytes reach disk until the authenticated stream and expected pin passed.
    write_private_new(&destination.join("identity.seed"), &plain[40..])?;
    if Identity::load(destination)?.public_key() != expected {
        return Err(Error::CorruptIdentity);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        d
    }
    #[test]
    fn encrypted_restore_preserves_pin_and_refuses_overwrite() {
        let source = dir();
        let keys = dir();
        let out = dir();
        let restored = dir();
        let pin = Identity::open(source.path()).unwrap().public_key();
        keygen(keys.path()).unwrap();
        backup(
            source.path(),
            &keys.path().join("recipient.txt"),
            &out.path().join("identity.age"),
        )
        .unwrap();
        let encrypted = std::fs::read(out.path().join("identity.age")).unwrap();
        let seed = read_private(&source.path().join("identity.seed"), 72).unwrap();
        assert!(!encrypted.windows(72).any(|v| v == seed.as_slice()));
        restore(
            &keys.path().join("recovery.agekey"),
            &out.path().join("identity.age"),
            pin,
            restored.path(),
        )
        .unwrap();
        assert_eq!(Identity::load(restored.path()).unwrap().public_key(), pin);
        assert!(restore(
            &keys.path().join("recovery.agekey"),
            &out.path().join("identity.age"),
            pin,
            restored.path()
        )
        .is_err());
        assert!(backup(
            source.path(),
            &keys.path().join("recipient.txt"),
            &out.path().join("identity.age")
        )
        .is_err());
        assert!(keygen(keys.path()).is_err());
    }
    #[test]
    fn tamper_wrong_recipient_and_wrong_pin_write_nothing() {
        let source = dir();
        let keys = dir();
        let wrong = dir();
        let out = dir();
        let pin = Identity::open(source.path()).unwrap().public_key();
        keygen(keys.path()).unwrap();
        keygen(wrong.path()).unwrap();
        backup(
            source.path(),
            &keys.path().join("recipient.txt"),
            &out.path().join("identity.age"),
        )
        .unwrap();
        let raw = std::fs::read(out.path().join("identity.age")).unwrap();
        for position in [0, raw.len() / 2, raw.len() - 1] {
            let mut altered = raw.clone();
            altered[position] ^= 1;
            let name = out.path().join(format!("altered-{position}"));
            write_private_new(&name, &altered).unwrap();
            let dest = dir();
            assert!(restore(
                &keys.path().join("recovery.agekey"),
                &name,
                pin,
                dest.path()
            )
            .is_err());
            assert_eq!(std::fs::read_dir(dest.path()).unwrap().count(), 0);
        }
        for (key, expected) in [
            (wrong.path().join("recovery.agekey"), pin),
            (keys.path().join("recovery.agekey"), [0; 32]),
        ] {
            let dest = dir();
            assert!(restore(
                &key,
                &out.path().join("identity.age"),
                expected,
                dest.path()
            )
            .is_err());
            assert_eq!(std::fs::read_dir(dest.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn missing_identity_truncated_and_oversized_input_are_denied() {
        let absent = dir();
        let keys = dir();
        let out = dir();
        keygen(keys.path()).unwrap();
        assert!(backup(
            absent.path(),
            &keys.path().join("recipient.txt"),
            &out.path().join("x")
        )
        .is_err());
        assert_eq!(std::fs::read_dir(absent.path()).unwrap().count(), 0);
        for bytes in [
            b"age-encryption.org/v1\n".to_vec(),
            vec![0; MAX_ENCRYPTED + 1],
        ] {
            let target = dir();
            let dest = dir();
            write_private_new(&target.path().join("bad.age"), &bytes).unwrap();
            assert!(restore(
                &keys.path().join("recovery.agekey"),
                &target.path().join("bad.age"),
                [1; 32],
                dest.path()
            )
            .is_err());
            assert_eq!(std::fs::read_dir(dest.path()).unwrap().count(), 0);
        }
    }
    #[cfg(unix)]
    #[test]
    fn linked_recovery_key_and_output_destination_are_denied() {
        use std::os::unix::fs::symlink;
        let source = dir();
        let keys = dir();
        let out = dir();
        let pin = Identity::open(source.path()).unwrap().public_key();
        keygen(keys.path()).unwrap();
        backup(
            source.path(),
            &keys.path().join("recipient.txt"),
            &out.path().join("identity.age"),
        )
        .unwrap();
        symlink(
            keys.path().join("recovery.agekey"),
            keys.path().join("linked-key"),
        )
        .unwrap();
        let dest = dir();
        assert!(restore(
            &keys.path().join("linked-key"),
            &out.path().join("identity.age"),
            pin,
            dest.path()
        )
        .is_err());
        let linked = out.path().join("linked-dir");
        symlink(dest.path(), &linked).unwrap();
        assert!(restore(
            &keys.path().join("recovery.agekey"),
            &out.path().join("identity.age"),
            pin,
            &linked
        )
        .is_err());
        assert_eq!(std::fs::read_dir(dest.path()).unwrap().count(), 0);
    }
}
