// SPDX-License-Identifier: AGPL-3.0-only
//! Confirmed history. A staged or failed version never becomes a backup.
use super::*;

const BACKUP_LIMIT: usize = 2;
const RETIRED_PREFIX: &str = ".retired-";

impl State {
    pub(super) fn valid_history(&self) -> bool {
        if self.history.len() > BACKUP_LIMIT {
            return false;
        }
        let Some(current) = &self.confirmed else {
            return self.history.is_empty();
        };
        let mut previous_epoch = current.epoch;
        let mut hashes = std::collections::HashSet::from([current.hash.as_str()]);
        if let Some(candidate) = &self.candidate {
            hashes.insert(candidate.hash.as_str());
        }
        self.history.iter().all(|backup| {
            let ordered = backup.epoch < previous_epoch;
            previous_epoch = backup.epoch;
            ordered && hashes.insert(&backup.hash)
        })
    }

    pub(super) fn confirm_candidate(&mut self, candidate: Version) {
        if let Some(previous) = self.confirmed.replace(candidate) {
            self.history.insert(0, previous);
            self.history.truncate(BACKUP_LIMIT);
        }
        self.candidate = None;
        self.attempted = false;
    }
}

impl Store {
    pub fn has_confirmed_version(&self) -> bool {
        self.state.confirmed.is_some()
    }

    /// Promote the first intact, schema-compatible backup, keeping the install
    /// floor. Invalid newer backups cannot cause repeated start attempts.
    pub(super) fn select_retained(&mut self) -> Result<Option<PathBuf>, Error> {
        if self.state.candidate.is_some() {
            return Err(Error::Busy);
        }
        let verified = self
            .state
            .history
            .iter()
            .enumerate()
            .find_map(|(index, version)| {
                self.verify_version(version).ok().map(|path| (index, path))
            });
        let Some((index, path)) = verified else {
            return Ok(None);
        };
        self.state.confirmed = Some(self.state.history.remove(index));
        self.state.history.drain(..index);
        self.persist()?;
        Ok(Some(path))
    }

    /// Bound the recovery action to the exact selection that systemd tried to
    /// start. A stale failure must not promote or discard a different slot.
    pub fn recover_selected_application(
        &mut self,
        runtime: &Path,
        payload_root: &Path,
    ) -> Result<bool, Error> {
        self.check_runtime_paths(runtime, payload_root)?;
        let target = fs::read_link(runtime.join("selected")).map_err(|_| Error::Io)?;
        if let Some(candidate) = &self.state.candidate {
            if !self.state.attempted || target != payload_root.join(&candidate.hash) {
                return Err(Error::Denied);
            }
            self.state.candidate = None;
            self.state.attempted = false;
            self.persist()?;
            return Ok(true);
        }
        if target != payload_root.join(self.confirmed_hash()?) {
            return Err(Error::Denied);
        }
        self.select_retained().map(|path| path.is_some())
    }

    #[cfg(test)]
    pub(super) fn retained_applications(&self) -> Vec<(u64, String)> {
        self.state
            .history
            .iter()
            .map(|version| (version.epoch, version.hash.clone()))
            .collect()
    }

    pub(crate) fn retention_status(
        &self,
        runtime: &Path,
        payload_root: &Path,
    ) -> Result<rustsimhub_wire::UpdateBackups, Error> {
        self.check_maintenance_selection(runtime, payload_root)?;
        let current = self.state.confirmed.as_ref().ok_or(Error::Denied)?;
        let project = |version: &Version| rustsimhub_wire::UpdateBackup {
            epoch: version.epoch,
            manifest_sha256: version.hash.clone(),
        };
        Ok(rustsimhub_wire::UpdateBackups {
            format: 1,
            current: project(current),
            backups: self.state.history.iter().map(project).collect(),
        })
    }

    /// Unlink the ledger reference first. An interrupted removal leaves an
    /// unreferenced slot for the normal cleanup instead of a broken fallback.
    pub fn remove_retained_application(
        &mut self,
        runtime: &Path,
        payload_root: &Path,
        epoch: u64,
        hash: &str,
    ) -> Result<(), Error> {
        self.check_maintenance_selection(runtime, payload_root)?;
        let index = self
            .state
            .history
            .iter()
            .position(|version| version.epoch == epoch && version.hash == hash)
            .ok_or(Error::Denied)?;
        let (path, parsed_epoch) = self.managed_slot(hash)?;
        if parsed_epoch != epoch {
            return Err(Error::Integrity);
        }
        self.state.history.remove(index);
        self.persist()?;
        self.remove_slot(&path)
    }

    /// Only program slots with a signed manifest and known file names can be
    /// removed. Leave foreign paths and interrupted unknown files for review.
    pub fn prune_unreferenced_applications(
        &self,
        runtime: &Path,
        payload_root: &Path,
    ) -> Result<usize, Error> {
        self.check_maintenance_selection(runtime, payload_root)?;
        let confirmed = self.confirmed_hash()?;
        let versions = self.root.join("versions");
        let mut removed = 0;
        let entries = fs::read_dir(versions)
            .and_then(|entries| entries.collect::<std::io::Result<Vec<_>>>())
            .map_err(|_| Error::Io)?;
        for entry in entries {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let retired = name.strip_prefix(RETIRED_PREFIX);
            let hash = retired.unwrap_or(name);
            if !hex_digest(hash) {
                continue;
            }
            if hash == confirmed
                || self
                    .state
                    .history
                    .iter()
                    .any(|version| version.hash == hash)
            {
                continue;
            }
            if retired.is_some() {
                let path = entry.path();
                self.check_retired_slot(&path)?;
                fs::remove_dir_all(path).map_err(|_| Error::Io)?;
                sync_dir(&self.root.join("versions"))?;
                removed += 1;
            } else {
                let Ok((path, epoch)) = self.managed_slot(hash) else {
                    continue;
                };
                if epoch > self.state.floor {
                    continue;
                }
                self.remove_slot(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn check_maintenance_selection(
        &self,
        runtime: &Path,
        payload_root: &Path,
    ) -> Result<(), Error> {
        self.check_runtime_paths(runtime, payload_root)?;
        if self.state.candidate.is_some() || self.state.attempted {
            return Err(Error::Busy);
        }
        let selected = fs::read_link(runtime.join("selected")).map_err(|_| Error::Io)?;
        if selected != payload_root.join(self.confirmed_hash()?) {
            return Err(Error::Denied);
        }
        Ok(())
    }

    fn managed_slot(&self, hash: &str) -> Result<(PathBuf, u64), Error> {
        if !hex_digest(hash) {
            return Err(Error::Denied);
        }
        let path = self.root.join("versions").join(hash);
        dir(&path)?;
        let manifest = read_checked(&path.join("manifest.json"), MAX_MANIFEST)?;
        let signature = read_checked(&path.join("manifest.sig"), 64)?;
        if manifest_hash(&manifest) != hash {
            return Err(Error::Integrity);
        }
        // Old managed versions can be cleaned even if the current database
        // schema cannot run them. Verify their own signed schema here.
        let schema = serde_json::from_slice::<Manifest>(&manifest)
            .map_err(|_| Error::Invalid)?
            .schema_min;
        let parsed = verify(&manifest, &signature, &self.key, schema, 0)?;
        let names = application_files(parsed.format)?;
        for entry in fs::read_dir(&path).map_err(|_| Error::Io)? {
            let entry = entry.map_err(|_| Error::Io)?;
            if !entry.file_name().to_str().is_some_and(|name| {
                names.contains(&name) || matches!(name, "manifest.json" | "manifest.sig")
            }) {
                return Err(Error::Denied);
            }
            let meta = fs::symlink_metadata(entry.path()).map_err(|_| Error::Io)?;
            if !meta.is_file()
                || meta.nlink() != 1
                || meta.uid() != rustix::process::geteuid().as_raw()
            {
                return Err(Error::Denied);
            }
        }
        Ok((path, parsed.epoch))
    }

    fn remove_slot(&self, path: &Path) -> Result<(), Error> {
        let versions = self.root.join("versions");
        let hash = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(Error::Denied)?;
        let retired = versions.join(format!("{RETIRED_PREFIX}{hash}"));
        rustix::fs::renameat_with(
            rustix::fs::CWD,
            path,
            rustix::fs::CWD,
            &retired,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|_| Error::Io)?;
        sync_dir(&versions)?;
        fs::remove_dir_all(retired).map_err(|_| Error::Io)?;
        sync_dir(&versions)
    }

    fn check_retired_slot(&self, path: &Path) -> Result<(), Error> {
        dir(path)?;
        let names = application_files(2)?;
        for item in fs::read_dir(path).map_err(|_| Error::Io)? {
            let entry = item.map_err(|_| Error::Io)?;
            let meta = fs::symlink_metadata(entry.path()).map_err(|_| Error::Io)?;
            if !meta.is_file()
                || meta.nlink() != 1
                || meta.uid() != rustix::process::geteuid().as_raw()
                || !entry.file_name().to_str().is_some_and(|name| {
                    names.contains(&name) || matches!(name, "manifest.json" | "manifest.sig")
                })
            {
                return Err(Error::Denied);
            }
        }
        Ok(())
    }
}
