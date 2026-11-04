// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only authority from this live Auth and its existing Store actor.
use super::*;
use std::time::Instant;
pub struct UpdateHealthLease {
    pub(crate) lease: rustsimhub_storage::update_health::Lease,
    instance: String,
}
impl Auth {
    pub fn update_health_lease(
        &self,
        until: Instant,
        queue_owned: bool,
    ) -> Result<UpdateHealthLease> {
        let lease = self.store.update_health_lease(until, queue_owned)?;
        if lease.snapshot().device_key != self.public_key() {
            return Err(Error::Forbidden);
        }
        Ok(UpdateHealthLease {
            lease,
            instance: self.instance.clone(),
        })
    }
}
impl UpdateHealthLease {
    pub fn instance(&self) -> &str {
        &self.instance
    }
    pub fn snapshot(&self) -> &rustsimhub_storage::update_health::Snapshot {
        self.lease.snapshot()
    }
    pub fn revalidate(&self) -> Result<()> {
        self.lease.revalidate().map_err(Into::into)
    }
    pub fn release(self) -> Result<()> {
        self.lease.release().map_err(Into::into)
    }
}
