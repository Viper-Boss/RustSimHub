// SPDX-License-Identifier: AGPL-3.0-only
//! Immutable display snapshot bound to the exact protected AP round, not a scan command.
use super::*;
use crate::nm::scan::Snapshot;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub format: u16,
    pub binding: Binding,
    pub snapshot: Snapshot,
}
#[cfg(test)]
mod tests;
impl Proof {
    pub fn validate(&self, binding: &Binding, now: u64) -> Result<()> {
        if self.format != 1
            || self.binding != *binding
            || !binding.valid()
            || !self.snapshot.valid()
            || self.snapshot.requested_uptime_ms < binding.created_uptime_ms
            || self.snapshot.completed_uptime_ms > now
            || self.snapshot.completed_uptime_ms >= binding.deadline_uptime_ms
            || now >= binding.deadline_uptime_ms
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
    pub fn parse(bytes: &[u8], binding: &Binding, now: u64) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Invalid);
        }
        let proof: Self = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        proof.validate(binding, now)?;
        Ok(proof)
    }
}
