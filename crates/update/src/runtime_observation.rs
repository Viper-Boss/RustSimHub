// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-path Linux read-only observation. Never a health or confirm capability.
use crate::Error;

/// Opaque live observation; no JSON constructor, cloning or confirmation token.
pub struct RuntimeObservation {
    _private: (),
}
impl RuntimeObservation {
    /// Interface checkpoint; subsequent implementation retains live read guards.
    pub fn observe() -> Result<Self, Error> {
        Err(Error::Denied)
    }
    pub fn revalidate(&self) -> Result<(), Error> {
        Err(Error::Denied)
    }
    pub fn report(&self) -> Result<serde_json::Value, Error> {
        self.revalidate()?;
        Err(Error::Denied)
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn interface_checkpoint_cannot_claim_success() {
        assert!(super::RuntimeObservation::observe().is_err());
    }
}
