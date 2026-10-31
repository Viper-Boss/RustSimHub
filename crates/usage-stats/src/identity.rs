//! An independent, pseudonymous identity for this product's usage statistics.
//!
//! Callers must supply fresh statistics-only entropy to `from_seed`; this module
//! never reads an Owner, SIM, messaging, or communication identity.

use crate::protocol::StatsPublicKey;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::Zeroizing;

/// Product and purpose separation for the statistics identity and private store.
pub const IDENTITY_DOMAIN: &str = "RustSimHub/usage-stats/ed25519-seed/v1";

/// Holds entropy for only the statistics identity. It cannot be serialized or
/// exported through the public API, and its debug representation is redacted.
pub struct StatisticsIdentity {
    seed: Zeroizing<[u8; 32]>,
    public_key: StatsPublicKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityError {
    EntropyUnavailable,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("statistics identity entropy unavailable")
    }
}

impl std::error::Error for IdentityError {}

impl fmt::Debug for StatisticsIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StatisticsIdentity")
            .field("public_key", &self.public_key)
            .field("seed", &"[REDACTED]")
            .finish()
    }
}

impl StatisticsIdentity {
    pub fn generate() -> Result<Self, IdentityError> {
        let mut seed = Zeroizing::new([0; 32]);
        getrandom::fill(seed.as_mut()).map_err(|_| IdentityError::EntropyUnavailable)?;
        Ok(Self::from_secret(seed))
    }

    /// Inject fresh, dedicated statistics entropy (for restoration or tests).
    /// A copied seed still held by the caller remains the caller's responsibility.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self::from_secret(Zeroizing::new(seed))
    }

    fn from_secret(seed: Zeroizing<[u8; 32]>) -> Self {
        // SHA-256 with an explicit purpose prefix derives a fixed Ed25519 seed;
        // ed25519-dalek performs key generation, rather than custom curve code.
        let mut hash = Sha256::new();
        hash.update(IDENTITY_DOMAIN.as_bytes());
        hash.update([0]);
        hash.update(seed.as_ref());
        let derived = Zeroizing::new(<[u8; 32]>::from(hash.finalize()));
        let signing = SigningKey::from_bytes(&derived);
        let public_key = StatsPublicKey::from_bytes(signing.verifying_key().to_bytes());
        Self { seed, public_key }
    }

    pub fn public_key(&self) -> StatsPublicKey {
        self.public_key
    }

    /// Visible only to the private persistence implementation.
    pub(crate) fn private_seed(&self) -> &[u8; 32] {
        &self.seed
    }
}
