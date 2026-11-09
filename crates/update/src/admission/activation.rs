// SPDX-License-Identifier: AGPL-3.0-only
//! Release only database readers/writers to actual Core startup, retaining the
//! original outer Root/target observations and original data inode descriptors.
use super::*;
pub(super) struct RuntimeHandoff {
    snapshot: MaintenanceSnapshot,
}
impl MaintenanceSnapshot {
    pub(super) fn handoff_runtime(mut self) -> Result<RuntimeHandoff> {
        self.revalidate()?;
        if let Some(outbox) = self.outbox.as_mut() {
            outbox.pause_writer()?;
        }
        self.business.pause_writer()?;
        Ok(RuntimeHandoff { snapshot: self })
    }
}
impl RuntimeHandoff {
    pub(super) fn revalidate(&self) -> Result<()> {
        let state = Path::new("/").join(STATE);
        let outbox = state.join("forwarding");
        for observed in &self.snapshot.observations {
            if observed.path == state || (self.snapshot.outbox.is_some() && observed.path == outbox)
            {
                let now = Observation::capture(&observed.path)?;
                let before = observed.stamp.as_ref().ok_or(Error::Integrity)?;
                let after = now.stamp.as_ref().ok_or(Error::Integrity)?;
                if before.dev != after.dev
                    || before.ino != after.ino
                    || before.mode != after.mode
                    || before.uid != after.uid
                    || before.gid != after.gid
                {
                    return Err(Error::Integrity);
                }
            } else {
                observed.revalidate()?;
            }
        }
        for file in &self.snapshot._root_files {
            file.revalidate()?;
        }
        self.snapshot.business.runtime_identity()?;
        if let Some(outbox) = &self.snapshot.outbox {
            outbox.runtime_identity()?;
        }
        Ok(())
    }
}
