// SPDX-License-Identifier: AGPL-3.0-only
//! Algorithm-compatible membership proof from nknguard c32952d7, original
//! pkg/membership and pkg/identity (AGPL-3.0). A proof alone grants no Core role.
use super::*;
use hmac::{Hmac, Mac};
type HmacSha256 = Hmac<Sha256>;
fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for byte in bytes {
        acc = (acc << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(ALPHABET[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(ALPHABET[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}
pub struct MembershipKey {
    network: String,
    proof: Zeroizing<[u8; 32]>,
    instance: [u8; 32],
    generation: u64,
    live: Arc<AtomicBool>,
}
impl MembershipKey {
    /// Trusted private provisioning, not network JSON or a public topic key.
    pub fn from_secret(
        network: [u8; 16],
        secret: Zeroizing<[u8; 32]>,
        generation: u64,
    ) -> Result<Self> {
        if network == [0; 16] || *secret == [0; 32] || generation == 0 {
            return Err(Error::InvalidInput);
        }
        let network = format!("nkgnet_{}", base32(&network));
        let mut mac =
            HmacSha256::new_from_slice(secret.as_ref()).map_err(|_| Error::InvalidInput)?;
        mac.update(b"nknguard/v1/membership-proof\0");
        mac.update(network.as_bytes());
        mac.update(&[1]);
        let proof = Zeroizing::new(mac.finalize().into_bytes().into());
        let mut instance = [0; 32];
        getrandom::fill(&mut instance).map_err(|_| Error::Entropy)?;
        Ok(Self {
            network,
            proof,
            instance,
            generation,
            live: Arc::new(AtomicBool::new(true)),
        })
    }
    pub fn network_id(&self) -> &str {
        &self.network
    }
    pub fn device_id(root: &[u8; 32]) -> String {
        format!("nkg_{}", base32(&Sha256::digest(root)[..10]))
    }
    fn mac(&self, root: &[u8; 32]) -> Result<HmacSha256> {
        if !self.live.load(Ordering::Acquire) {
            return Err(Error::Forbidden);
        }
        let mut mac =
            HmacSha256::new_from_slice(self.proof.as_ref()).map_err(|_| Error::Forbidden)?;
        mac.update(self.network.as_bytes());
        mac.update(&[0]);
        mac.update(Self::device_id(root).as_bytes());
        mac.update(&[0]);
        mac.update(root);
        Ok(mac)
    }
    pub fn proof(&self, root: &[u8; 32]) -> Result<[u8; 32]> {
        key_valid(root)?;
        Ok(self.mac(root)?.finalize().into_bytes().into())
    }
    pub fn revoke(&self) {
        self.live.store(false, Ordering::Release);
    }
    pub(super) fn verify(&self, root: [u8; 32], proof: &[u8; 32]) -> Result<Member> {
        key_valid(&root)?;
        self.mac(&root)?
            .verify_slice(proof)
            .map_err(|_| Error::Forbidden)?;
        Ok(Member {
            root,
            network: self.network.clone(),
            proof: *proof,
            instance: self.instance,
            generation: self.generation,
            live: self.live.clone(),
        })
    }
}
impl Drop for MembershipKey {
    fn drop(&mut self) {
        self.revoke();
    }
}
#[derive(Clone)]
pub(super) struct Member {
    pub root: [u8; 32],
    pub network: String,
    pub proof: [u8; 32],
    pub instance: [u8; 32],
    pub generation: u64,
    pub live: Arc<AtomicBool>,
}
impl PartialEq for Member {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
            && self.network == other.network
            && self.proof == other.proof
            && self.instance == other.instance
            && self.generation == other.generation
            && Arc::ptr_eq(&self.live, &other.live)
    }
}
impl Eq for Member {}
