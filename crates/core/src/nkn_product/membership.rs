// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit private installation credential. Never generated from a web request.
use rustsimhub_auth::{nkn_session::MembershipKey, Error, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    sync::Arc,
};
use zeroize::Zeroizing;
const PATH: &str = "/etc/rustsimhub/nkn-membership.bin";
const SIZE: usize = 93;
pub(super) struct InstalledMembership {
    pub key: Arc<MembershipKey>,
    digest: [u8; 32],
    pin: [u8; 32],
}
fn read() -> Result<Zeroizing<Vec<u8>>> {
    for p in ["/etc", "/etc/rustsimhub"] {
        let m = std::fs::symlink_metadata(p).map_err(|_| Error::Forbidden)?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(Error::Forbidden);
        }
    }
    let roles = rustsimhub_core::nkn_root::InstalledRoles::read().map_err(|_| Error::Forbidden)?;
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(PATH)
        .map_err(|_| Error::Forbidden)?;
    let m = f.metadata().map_err(|_| Error::Forbidden)?;
    if !m.is_file()
        || m.uid() != 0
        || m.gid() != roles.core_gid
        || m.mode() & 0o7777 != 0o640
        || m.nlink() != 1
        || m.len() != SIZE as u64
    {
        return Err(Error::Forbidden);
    }
    let mut raw = Zeroizing::new(Vec::with_capacity(SIZE));
    Read::by_ref(&mut f)
        .take((SIZE + 1) as u64)
        .read_to_end(&mut raw)
        .map_err(|_| Error::Forbidden)?;
    if raw.len() != SIZE {
        return Err(Error::Forbidden);
    }
    Ok(raw)
}
fn decode(raw: &[u8], pin: &[u8; 32]) -> Result<MembershipKey> {
    if raw.len() != SIZE || &raw[..5] != b"RSMK\x01" || &raw[61..93] != pin {
        return Err(Error::Forbidden);
    }
    let network = raw[5..21].try_into().map_err(|_| Error::Forbidden)?;
    let secret = Zeroizing::new(raw[21..53].try_into().map_err(|_| Error::Forbidden)?);
    let generation = u64::from_be_bytes(raw[53..61].try_into().map_err(|_| Error::Forbidden)?);
    MembershipKey::from_secret(network, secret, generation)
}
impl InstalledMembership {
    pub fn load(pin: [u8; 32]) -> Result<Self> {
        let raw = read()?;
        let key = Arc::new(decode(&raw, &pin)?);
        Ok(Self {
            key,
            digest: Sha256::digest(&raw).into(),
            pin,
        })
    }
    /// Bounded Root installation changes retire all existing key/session copies.
    pub fn current(&self) -> bool {
        let ok = read().is_ok_and(|raw| {
            Sha256::digest(&raw).as_slice() == self.digest && raw[61..93] == self.pin
        });
        if !ok {
            self.key.revoke()
        }
        ok
    }
}
impl Drop for InstalledMembership {
    fn drop(&mut self) {
        self.key.revoke();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_credential_requires_exact_version_pin_and_nonzero_context() {
        let pin = [17; 32];
        let mut raw = b"RSMK\x01".to_vec();
        raw.extend([18; 16]);
        raw.extend([19; 32]);
        raw.extend(1u64.to_be_bytes());
        raw.extend(pin);
        assert!(decode(&raw, &pin).is_ok());
        assert!(decode(&raw, &[20; 32]).is_err());
        assert!(decode(&raw[..92], &pin).is_err());
        let mut bad = raw.clone();
        bad[53..61].fill(0);
        assert!(decode(&bad, &pin).is_err());
        let mut bad = raw.clone();
        bad[21..53].fill(0);
        assert!(decode(&bad, &pin).is_err());
        raw.push(0);
        assert!(decode(&raw, &pin).is_err());
    }
}
