// SPDX-License-Identifier: AGPL-3.0-only
//! The one business-writer handoff; the original outer snapshot stays owned.
use super::*;
use crate::product_admission::coordinator::RecoveryMigrationHandoff;
use crate::product_recovery_child::Permit;
use std::{cell::RefCell, collections::BTreeMap};

pub(crate) struct RecoveryHandoff<'a> {
    snapshot: RefCell<Option<MaintenanceSnapshot>>,
    permit: Permit,
    check: &'a dyn Fn() -> Result<()>,
}
impl<'a> RecoveryHandoff<'a> {
    pub(crate) fn new(permit: Permit, check: &'a dyn Fn() -> Result<()>) -> Self {
        Self {
            snapshot: RefCell::new(None),
            permit,
            check,
        }
    }
    pub(crate) fn finish(self) -> Result<MaintenanceSnapshot> {
        (self.check)()?;
        let snapshot = self.snapshot.into_inner().ok_or(Error::Busy)?;
        snapshot.revalidate()?;
        Ok(snapshot)
    }
}
impl RecoveryMigrationHandoff for RecoveryHandoff<'_> {
    fn revalidate_before_handoff(&self) -> Result<()> {
        (self.check)()
    }
    fn with_business_writer_handoff(
        &mut self,
        mut snapshot: MaintenanceSnapshot,
        work: impl FnOnce(&Self) -> Result<()>,
    ) -> Result<()> {
        if self.snapshot.borrow().is_some() {
            return Err(Error::Busy);
        }
        snapshot.revalidate()?;
        (self.check)()?;
        let directory = Path::new("/").join(STATE);
        #[cfg(test)]
        eprintln!("recovery handoff: inventory");
        let before = inventory(&directory)?;
        if before.contains_key(std::ffi::OsStr::new("product-upgrade")) {
            return Err(Error::Busy);
        }
        #[cfg(test)]
        eprintln!("recovery handoff: pause-writer");
        snapshot.business.pause_writer()?;
        *self.snapshot.borrow_mut() = Some(snapshot);
        // The child obtains the original .writer.lock, sends R before it
        // touches DB/journal, and cannot proceed without our fresh G check.
        #[cfg(test)]
        eprintln!("recovery handoff: child-work");
        work(self)?;
        (self.check)()?;
        #[cfg(test)]
        eprintln!("recovery handoff: verify-committed");
        crate::product_recovery_child::verify_committed_root(&self.permit)
            .map_err(|_| Error::Integrity)?;
        let after = inventory(&directory)?;
        for name in before.keys().chain(after.keys()) {
            if name == "store.sqlite3" || name == "product-upgrade" {
                continue;
            }
            if before.get(name) != after.get(name) {
                return Err(Error::Integrity);
            }
        }
        let mut held = self.snapshot.borrow_mut();
        let snapshot = held.as_mut().ok_or(Error::Busy)?;
        // Check all other Root and outbox facts before refreshing ONLY this
        // actual child-owned directory/database, never a second snapshot.
        for observed in &snapshot.observations {
            if observed.path != directory {
                observed.revalidate()?;
            }
        }
        for file in &snapshot._root_files {
            file.revalidate()?;
        }
        if let Some(outbox) = &snapshot.outbox {
            outbox.revalidate()?;
        }
        #[cfg(test)]
        eprintln!("recovery handoff: resume-writer");
        snapshot.business.resume_recovery(&directory, 986)?;
        // Recheck proof under reacquired writer fence, closing D->lock races.
        crate::product_recovery_child::verify_committed_root(&self.permit)
            .map_err(|_| Error::Integrity)?;
        for observed in &mut snapshot.observations {
            if observed.path == directory {
                *observed = Observation::capture(&directory)?;
            }
        }
        snapshot.revalidate()?;
        (self.check)()
    }
    fn revalidate_before_commit(&self) -> Result<()> {
        (self.check)()?;
        let held = self.snapshot.borrow();
        let snapshot = held.as_ref().ok_or(Error::Busy)?;
        for observed in &snapshot.observations {
            observed.revalidate()?;
        }
        for file in &snapshot._root_files {
            file.revalidate()?;
        }
        if let Some(outbox) = &snapshot.outbox {
            outbox.revalidate()?;
        }
        snapshot.business.revalidate_source()?;
        // Bind source schema/hash/pin/realm again at R, while child holds lock.
        #[cfg(test)]
        eprintln!("recovery handoff: revalidate-child-source");
        let actual = crate::product_recovery_child::prepare_root(&self.permit.request_id)
            .map_err(|_| Error::Denied)?;
        if actual.source_schema != self.permit.source_schema
            || actual.source_sha256 != self.permit.source_sha256
            || actual.binding_sha256 != self.permit.binding_sha256
        {
            return Err(Error::Integrity);
        }
        (self.check)()
    }
}
fn inventory(directory: &Path) -> Result<BTreeMap<std::ffi::OsString, Stamp>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(|_| Error::Io)? {
        if result.len() >= 64 {
            return Err(Error::Invalid);
        }
        let entry = entry.map_err(|_| Error::Io)?;
        result.insert(
            entry.file_name(),
            fs::symlink_metadata(entry.path())
                .map_err(|_| Error::Io)?
                .into(),
        );
    }
    Ok(result)
}
