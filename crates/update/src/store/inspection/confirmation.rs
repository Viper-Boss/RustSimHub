// SPDX-License-Identifier: AGPL-3.0-only
//! The read guard is upgraded in place. No Store::open, creation or recovery.
use super::*;
use crate::{
    store::{load_payloads, sync_dir, write_new, Store},
    verified_application::{VerifiedApplication, VerifiedManifest},
    MAX_MANIFEST,
};

/// Retains the fallback's signed bytes and metadata through the live decision.
pub(crate) struct Plan {
    baseline: VerifiedApplication,
    observed: Vec<Observed>,
    hash: String,
    epoch: u64,
    floor: u64,
}
impl Plan {
    #[cfg(test)]
    pub(crate) fn baseline(&self) -> &crate::store::SignedGroup {
        self.baseline.group()
    }
    pub(crate) fn verified_baseline(&self) -> &VerifiedApplication {
        &self.baseline
    }
    pub(crate) fn revalidate(&self, ledger: &ReadOnlyState) -> Result<()> {
        ledger.revalidate()?;
        for guard in &self.observed {
            guard.check()?;
        }
        let (hash, epoch, attempted, candidate) = ledger.runtime_version()?;
        if !attempted
            || !candidate
            || hash != self.hash
            || epoch != self.epoch
            || ledger.state.floor != self.floor
        {
            return Err(Error::Integrity);
        }
        Ok(())
    }
}
impl ReadOnlyState {
    pub(crate) fn confirmation_plan(&self) -> Result<Plan> {
        self.plan_using(VerifiedManifest::bind)
    }
    fn plan_using(
        &self,
        check: fn(VerifiedManifest, Vec<Vec<u8>>) -> Result<VerifiedApplication>,
    ) -> Result<Plan> {
        self.revalidate()?;
        let v = self.state.candidate.as_ref().ok_or(Error::Denied)?;
        let baseline = self.state.confirmed.as_ref().ok_or(Error::Denied)?;
        if !self.state.attempted
            || self.state.floor != v.epoch
            || baseline.hash == v.hash
            || baseline.epoch >= v.epoch
        {
            return Err(Error::Denied);
        }
        let path = self.root.join("versions").join(&baseline.hash);
        dir(&path)?;
        let mut observed = vec![
            Observed::capture(&self.root.join("versions"))?,
            Observed::capture(&path)?,
        ];
        let (bytes, manifest_guard) = read(
            &path.join("manifest.json"),
            &[0o600, 0o444, 0o644],
            MAX_MANIFEST,
        )?;
        let (signature, signature_guard) =
            read(&path.join("manifest.sig"), &[0o600, 0o444, 0o644], 64)?;
        if manifest_hash(&bytes) != baseline.hash {
            return Err(Error::Integrity);
        }
        let signed_manifest = VerifiedManifest::verify(bytes, signature, &self.key, 6)?;
        let manifest = signed_manifest.manifest().clone();
        if manifest.epoch != baseline.epoch {
            return Err(Error::Integrity);
        }
        observed.extend([manifest_guard, signature_guard]);
        for entry in &manifest.files {
            observed.push(Observed::capture(&path.join(&entry.name))?);
        }
        let payloads = load_payloads(&path, &manifest)?;
        let baseline = check(signed_manifest, payloads)?;
        let expected: std::collections::BTreeSet<_> = manifest
            .files
            .iter()
            .map(|e| e.name.as_str())
            .chain(["manifest.json", "manifest.sig"])
            .collect();
        let entries = fs::read_dir(&path)
            .map_err(|_| Error::Io)?
            .map(|e| e.map(|e| e.file_name()).map_err(|_| Error::Io))
            .collect::<Result<Vec<_>>>()?;
        if entries.len() != expected.len()
            || entries
                .iter()
                .any(|n| !n.to_str().is_some_and(|n| expected.contains(n)))
        {
            return Err(Error::Integrity);
        }
        let plan = Plan {
            baseline,
            observed,
            hash: v.hash.clone(),
            epoch: v.epoch,
            floor: self.state.floor,
        };
        plan.revalidate(self)?;
        Ok(plan)
    }
    /// flock conversion may briefly drop SH: every original inode/content guard
    /// is rechecked AFTER EX is acquired. Failure consumes the outer capability.
    pub(crate) fn exclusive(&self, plan: &Plan) -> Result<Store> {
        FileExt::try_lock_exclusive(&self._lock.0).map_err(|_| Error::Busy)?;
        plan.revalidate(self)?;
        Ok(Store {
            root: self.root.clone(),
            key: self.key,
            schema: 6,
            state: self.state.clone(),
            _lock: StoreLock(self._lock.0.try_clone().map_err(|_| Error::Io)?),
        })
    }
}
impl Store {
    /// `gate` retains the original live slot/signature/peer/Auth/data guards.
    /// No autorecoverable state.next exists until the LAST successful gate.
    pub(crate) fn confirm_live(
        &mut self,
        plan: &Plan,
        scratch: &Path,
        gate: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let v = self.state.candidate.clone().ok_or(Error::Denied)?;
        if !self.state.attempted
            || v.hash != plan.hash
            || v.epoch != plan.epoch
            || self.state.floor != plan.floor
        {
            return Err(Error::Denied);
        }
        dir(scratch)?;
        let m = fs::symlink_metadata(scratch).map_err(|_| Error::Io)?;
        if m.mode() & 0o7777 != 0o700
            || m.dev() != fs::metadata(&self.root).map_err(|_| Error::Io)?.dev()
        {
            return Err(Error::Denied);
        }
        // A failed/crashed PRE-intent stays here for explicit maintenance. It
        // is NEVER recovered/renamed by Store::open or a future health attempt.
        let pending = scratch.join("confirmation.next");
        let mut confirmed = self.state.clone();
        confirmed.confirmed = Some(v);
        confirmed.candidate = None;
        confirmed.attempted = false;
        if !confirmed.valid() {
            return Err(Error::Invalid);
        }
        write_new(
            &pending,
            &serde_json::to_vec(&confirmed).map_err(|_| Error::Invalid)?,
            false,
        )?;
        sync_dir(scratch)?;
        gate()?;
        // Linearization: publish the already authorized intent to the ORIGINAL
        // updater recovery namespace. From this point an error is uncertain;
        // query inspect-state/recover under maintenance, never retry blindly.
        match fs::symlink_metadata(self.root.join("state.next")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Ok(_) => return Err(Error::Busy),
            Err(_) => return Err(Error::Io),
        }
        fs::rename(&pending, self.root.join("state.next")).map_err(|_| Error::Io)?;
        sync_dir(scratch)?;
        sync_dir(&self.root)?;
        fs::rename(self.root.join("state.next"), self.root.join("state.json"))
            .map_err(|_| Error::Io)?;
        sync_dir(&self.root)?;
        self.state = confirmed;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
impl ReadOnlyState {
    pub(crate) fn native_fixture_plan(&self) -> Result<Plan> {
        self.plan_using(VerifiedManifest::bind_native)
    }
}
