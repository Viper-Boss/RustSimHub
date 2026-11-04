// SPDX-License-Identifier: AGPL-3.0-only
//! Persistent, finite Root-material transaction. No database or application-slot
//! mutation and no service start/enable operation exists in this executor.
use super::{
    material_sha256, Error, ExpectedBindings, MaterialPhase, Result, TargetId,
    VerifiedProductMaterials,
};
use super::{
    nss::NssObservation,
    rootio::{FileSnapshot, Root, Stamp},
    services::{Services, Systemd},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    time::{Duration, Instant},
};

const BASE: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/product-install";
const INBOX: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox";
const ACTIVE: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/product-install/active.json";
const MAX_JOURNAL: usize = 64 * 1024;
const MAX_FILE: usize = 32 * 1024 * 1024;
const MAX_TOTAL: usize = 64 * 1024 * 1024;
const FLAGS: [&str; 2] = [
    "/var/lib/rustsimhub/rustsimhub-v1/system/product/nkn.enabled",
    "/var/lib/rustsimhub/rustsimhub-v1/system/product/ble.enabled",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallState {
    Prepared,
    Applying,
    Applied,
    RollingBack,
    RolledBack,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencePhase {
    Prepare,
    BeforeWrite,
    AfterWrite,
    Recover,
    Finalize,
}
/// Live caller-owned maintenance/data/Store locks. The adapter must check real
/// state, preserving data locks while allowing ONLY the supplied proven Root
/// mutations. Implementing this from JSON is forbidden by the integration
/// contract. The shared admission owner (window8) supplies that adapter.
pub trait LiveInstallationFence {
    fn revalidate(&self, phase: FencePhase, changes: &OwnedRootChanges<'_>) -> Result<()>;
}
/// Inspection of this executor's Root journal, not an authorization object.
/// No public constructor/Deserialize; never a substitute for a data guard.
pub struct OwnedRootChanges<'a> {
    journal: Option<&'a Journal>,
    root: &'a Root,
}
impl<'a> OwnedRootChanges<'a> {
    fn new(root: &'a Root, journal: Option<&'a Journal>) -> Self {
        Self { root, journal }
    }
    /// Actual fd-backed operations, not names or permissions supplied by the
    /// bundle. Caller still validates the finite transaction scope and data.
    pub fn operation_facts(&self) -> Result<Vec<super::rootio::OwnedNodeFact>> {
        self.root.owned_facts()
    }
    pub fn directory_versions(&self) -> Result<Vec<super::OwnedDirectoryVersion>> {
        self.root.owned_parent_versions()
    }

    /// Fixed paths derived from THIS retained journal. This predicate alone
    /// is no permission: operation_facts still verifies actual retained fds,
    /// and the caller binds the transaction, original Root and data generation.
    pub fn contains_operation(&self, fact: &super::OwnedNodeFact) -> bool {
        let Some(journal) = self.journal else {
            return false;
        };
        let tx = transaction_dir(&journal.transaction);
        let mut files = vec![
            format!("{BASE}/.install.lock"),
            ACTIVE.into(),
            journal_path(&journal.transaction),
            format!("{tx}/materials/manifest.json"),
            format!("{tx}/materials/manifest.sig"),
        ];
        for entry in &journal.entries {
            files.push(entry.id.spec().path.into());
            files.push(format!("{tx}/materials/{}", entry.id.payload_name()));
            files.push(backup_path(&journal.transaction, entry.id));
        }
        if fact.directory {
            if files
                .iter()
                .any(|p| p.starts_with(&(fact.path.clone() + "/")))
            {
                return true;
            }
            return journal.bindings.profile.product()
                && (fact.path == "/run"
                    || fact.path == "/run/rustsimhub"
                    || fact.path == "/run/rustsimhub/nkn"
                    || (journal.bindings.profile.recovery()
                        && fact.path == "/run/rustsimhub/recovery")
                    || (journal.bindings.profile == super::Profile::RecoveryHealth
                        && [
                            "/run/rustsimhub/update-health",
                            "/var/lib/rustsimhub/rustsimhub-v1/system/update-health",
                        ]
                        .contains(&fact.path.as_str())));
        }
        if fact.temporary {
            let Some((parent, name)) = fact.path.rsplit_once('/') else {
                return false;
            };
            return name.starts_with(".product-stage-")
                && files
                    .iter()
                    .any(|p| p.rsplit_once('/').is_some_and(|(dir, _)| dir == parent));
        }
        files.contains(&fact.path)
    }
    pub fn validates_target_fact(&self, fact: &super::OwnedNodeFact) -> bool {
        let Some(expected) = self
            .target_expectations()
            .into_iter()
            .find(|e| e.target.spec().path == fact.path)
        else {
            return true;
        };
        match &fact.after {
            Some(after) => {
                let id = (after.device, after.inode);
                let written = expected.written_identity == Some(id)
                    && fact.sha256.as_ref() == Some(&expected.new_sha256)
                    && after.mode == expected.new_mode;
                let restored = (expected.restoring || expected.restored)
                    && expected.restored_identity == Some(id)
                    && fact.sha256 == expected.before_sha256
                    && Some(after.mode) == expected.before_mode;
                written || restored
            }
            None => (expected.restoring || expected.restored) && expected.before_identity.is_none(),
        }
    }
    pub fn transaction(&self) -> Option<&str> {
        self.journal.map(|j| j.transaction.as_str())
    }
    pub fn targets(&self) -> Vec<TargetId> {
        self.journal
            .map(|j| j.entries.iter().map(|e| e.id).collect())
            .unwrap_or_default()
    }
    pub fn written_targets(&self) -> Vec<TargetId> {
        self.journal
            .map(|j| {
                j.entries
                    .iter()
                    .filter(|e| {
                        matches!(
                            e.state,
                            EntryState::Written | EntryState::Replacing | EntryState::Restoring
                        )
                    })
                    .map(|e| e.id)
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn runtime_directories_requested(&self) -> bool {
        self.journal.is_some_and(|j| j.bindings.profile.product())
    }
    pub fn target_expectations(&self) -> Vec<RootTargetExpectation> {
        self.journal
            .map(|j| {
                j.entries
                    .iter()
                    .map(|e| RootTargetExpectation {
                        target: e.id,
                        before_sha256: e.before.as_ref().map(|v| v.sha256.clone()),
                        before_mode: e.before.as_ref().map(|v| v.stamp.mode),
                        new_sha256: e.new_sha256.clone(),
                        new_mode: e.id.spec().mode,
                        before_identity: e.before.as_ref().map(|v| (v.stamp.device, v.stamp.inode)),
                        written_identity: e
                            .written
                            .as_ref()
                            .or(e.pending.as_ref())
                            .map(|v| (v.device, v.inode)),
                        restoring: e.state == EntryState::Restoring,
                        restored: e.state == EntryState::Restored,
                        restored_identity: e
                            .restored
                            .as_ref()
                            .or_else(|| {
                                if e.state == EntryState::Restoring {
                                    e.pending.as_ref()
                                } else {
                                    None
                                }
                            })
                            .map(|v| (v.device, v.inode)),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn state(&self) -> Option<InstallState> {
        self.journal.map(|j| j.state)
    }
}
/// Read-only constrained revalidation inputs for the shared admission adapter.
/// These public fields describe evidence; they cannot construct the live lease.
#[derive(Debug, Serialize)]
pub struct RootTargetExpectation {
    pub target: TargetId,
    pub before_sha256: Option<String>,
    pub before_mode: Option<u32>,
    pub new_sha256: String,
    pub new_mode: u32,
    pub before_identity: Option<(u64, u64)>,
    pub written_identity: Option<(u64, u64)>,
    pub restoring: bool,
    pub restored: bool,
    pub restored_identity: Option<(u64, u64)>,
}
#[derive(Debug, Serialize)]
pub struct InstallReport {
    pub format: u16,
    pub state: InstallState,
    pub transaction: String,
    pub files: Vec<FileReport>,
    pub retained_transaction_materials: bool,
    pub retained_runtime_directories: bool,
    pub starts_services: bool,
    pub changes_business_data: bool,
    pub health_verified: bool,
    pub may_confirm: bool,
    pub shared_cli_integrated: bool,
}
#[derive(Debug, Serialize)]
pub struct FileReport {
    pub target: TargetId,
    pub state: &'static str,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EntryState {
    Pending,
    Replacing,
    Written,
    Restoring,
    Restored,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Before {
    sha256: String,
    stamp: Stamp,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: TargetId,
    before: Option<Before>,
    new_sha256: String,
    new_bytes: u64,
    state: EntryState,
    pending: Option<Stamp>,
    written: Option<Stamp>,
    restored: Option<Stamp>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    format: u16,
    kind: String,
    transaction: String,
    bindings: ExpectedBindings,
    nss_baseline_sha256: String,
    state: InstallState,
    entries: Vec<Entry>,
    failure: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pointer {
    format: u16,
    transaction: String,
}

#[cfg(test)]
type TestFaultHook = Box<dyn FnMut(TestBoundary, Option<TargetId>) -> Result<()>>;

/// The retained package/NSS descriptors and journal mutex outlive every action.
/// Production roots/paths are fixed; isolated root injection is cfg(test) only.
pub struct ProductInstaller<'a> {
    root: Root,
    materials: VerifiedProductMaterials,
    nss: NssObservation,
    fence: &'a dyn LiveInstallationFence,
    services: Box<dyn Services>,
    _lock: File,
    journal: Journal,
    journal_file: Option<FileSnapshot>,
    started: Instant,
    #[cfg(test)]
    fault: Option<TestFaultHook>,
}
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestBoundary {
    Prepared,
    BeforeReplace,
    AfterReplace,
    AfterJournal,
    BeforeReload,
    AfterReload,
    BeforeRestore,
    AfterRestore,
}

impl<'a> ProductInstaller<'a> {
    /// Caller holds installed trust and real admission; no path, PID, command,
    /// approved flag, alternate trust root or test switch is accepted.
    pub fn prepare(
        key: &[u8; 32],
        expected: &ExpectedBindings,
        fence: &'a dyn LiveInstallationFence,
    ) -> Result<Self> {
        Self::open(
            Root::production()?,
            key,
            expected,
            fence,
            Box::new(Systemd),
            false,
        )
    }
    /// Open the retained signed transaction after an interrupted invocation.
    /// The original inbox may be absent. Its copied original signature and all
    /// bytes are verified again under the caller's installed trust.
    pub fn reopen(
        key: &[u8; 32],
        expected: &ExpectedBindings,
        fence: &'a dyn LiveInstallationFence,
    ) -> Result<Self> {
        Self::open(
            Root::production()?,
            key,
            expected,
            fence,
            Box::new(Systemd),
            true,
        )
    }
    fn open(
        root: Root,
        key: &[u8; 32],
        expected: &ExpectedBindings,
        fence: &'a dyn LiveInstallationFence,
        services: Box<dyn Services>,
        reopen: bool,
    ) -> Result<Self> {
        let started = Instant::now();
        fence.revalidate(
            if reopen {
                FencePhase::Recover
            } else {
                FencePhase::Prepare
            },
            &OwnedRootChanges::new(&root, None),
        )?;
        flags_off(&root)?;
        let nss = observe_nss(&root, expected.profile.product())?;
        if nss.policy_sha256() != expected.approved_role_policy_sha256 {
            return Err(Error::Denied);
        }
        // Verification and admission precede creation of any transaction file.
        let source = if reopen {
            let pointer = read_pointer(&root)?.ok_or(Error::Invalid)?;
            format!("{BASE}/transactions/{}/materials", pointer.transaction)
        } else {
            INBOX.into()
        };
        let materials = VerifiedProductMaterials::open(&root.open_dir(&source)?, key, expected)?;
        materials.validate_role_materials(nss.nkn_uid(), nss.ble_uid())?;
        root.ensure_dir(BASE, 0o700)?;
        let lock = root.lock_fixed(&format!("{BASE}/.install.lock"))?;
        services.prepare_off()?;
        let transaction = materials.manifest_sha256().to_owned();
        let path = journal_path(&transaction);
        let existing = root.read(&path, MAX_JOURNAL)?;
        let (journal, journal_file) = if let Some(file) = existing {
            let journal: Journal = parse(&file.bytes)?;
            validate_journal(&journal, &materials)?;
            if journal.nss_baseline_sha256 != nss.baseline_sha256() {
                return Err(Error::Conflict);
            }
            if read_pointer(&root)?.is_none_or(|p| p.transaction != transaction) {
                // A crash can leave the durable prepared journal immediately
                // before active.json is published. Adopt only a pristine
                // preparation with the exact still-current old targets.
                validate_unpublished(&root, &journal)?;
                let retained = VerifiedProductMaterials::open(
                    &root.open_dir(&format!("{}/materials", transaction_dir(&transaction)))?,
                    key,
                    expected,
                )?;
                if retained.manifest_sha256() != transaction {
                    return Err(Error::Integrity);
                }
                let pointer = root.read(ACTIVE, 2048)?;
                validate_previous(&root, read_pointer(&root)?.as_ref())?;
                materials.revalidate()?;
                nss.revalidate(&root)?;
                services.revalidate_off()?;
                fence.revalidate(
                    FencePhase::Prepare,
                    &OwnedRootChanges::new(&root, Some(&journal)),
                )?;
                root.replace(
                    ACTIVE,
                    pointer.as_ref(),
                    &encoded(&Pointer {
                        format: 1,
                        transaction: transaction.clone(),
                    })?,
                    0o600,
                )?;
            }
            (journal, Some(file))
        } else {
            if reopen {
                return Err(Error::Invalid);
            }
            validate_previous(&root, read_pointer(&root)?.as_ref())?;
            let mut entries = Vec::new();
            let mut total = 0usize;
            for spec in expected.profile.targets() {
                let old = root.read(spec.path, MAX_FILE)?;
                if let Some(old) = &old {
                    total = total.checked_add(old.bytes.len()).ok_or(Error::Invalid)?;
                    if total > MAX_TOTAL || old.stamp.mode & 0o7000 != 0 {
                        return Err(Error::Denied);
                    }
                }
                let payload = materials.payload(spec.id)?;
                entries.push(Entry {
                    id: spec.id,
                    before: old.as_ref().map(|old| Before {
                        sha256: material_sha256(&old.bytes),
                        stamp: old.stamp.clone(),
                    }),
                    new_sha256: material_sha256(payload),
                    new_bytes: payload.len() as u64,
                    state: EntryState::Pending,
                    pending: None,
                    written: None,
                    restored: None,
                });
            }
            let journal = Journal {
                format: 1,
                kind: "product-install-transaction".into(),
                transaction: transaction.clone(),
                bindings: expected.clone(),
                nss_baseline_sha256: nss.baseline_sha256(),
                state: InstallState::Prepared,
                entries,
                failure: None,
            };
            let tx = transaction_dir(&transaction);
            root.ensure_dir(&tx, 0o700)?;
            root.ensure_dir(&format!("{tx}/materials"), 0o700)?;
            root.ensure_dir(&format!("{tx}/before"), 0o700)?;
            // Persist exact signed source bytes, not reserialized JSON.
            for (name, bytes) in materials.source_files() {
                let name = format!("{tx}/materials/{name}");
                put_once(&root, &name, bytes, 0o600)?;
            }
            for entry in &journal.entries {
                if let Some(before) = &entry.before {
                    let current = root
                        .read(entry.id.spec().path, MAX_FILE)?
                        .ok_or(Error::Conflict)?;
                    if current.stamp != before.stamp
                        || material_sha256(&current.bytes) != before.sha256
                    {
                        return Err(Error::Conflict);
                    }
                    put_once(
                        &root,
                        &backup_path(&transaction, entry.id),
                        &current.bytes,
                        0o600,
                    )?;
                }
            }
            materials.revalidate()?;
            nss.revalidate(&root)?;
            fence.revalidate(
                FencePhase::Prepare,
                &OwnedRootChanges::new(&root, Some(&journal)),
            )?;
            let file = root.replace(&path, None, &encoded(&journal)?, 0o600)?;
            let pointer = root.read(ACTIVE, 2048)?;
            root.replace(
                ACTIVE,
                pointer.as_ref(),
                &encoded(&Pointer {
                    format: 1,
                    transaction: transaction.clone(),
                })?,
                0o600,
            )?;
            (journal, Some(file))
        };
        let installer = Self {
            root,
            materials,
            nss,
            fence,
            services,
            _lock: lock,
            journal,
            journal_file,
            started,
            #[cfg(test)]
            fault: None,
        };
        installer.check(FencePhase::Recover)?;
        Ok(installer)
    }
    pub fn inspect(&self) -> Result<InstallReport> {
        self.check(FencePhase::Recover)?;
        for entry in &self.journal.entries {
            self.classify(entry)?;
        }
        Ok(self.report())
    }
    /// Output-only plan bound to this inspected transaction's real baseline.
    /// Its booleans/digests never replace the borrowed live installation fence.
    pub fn plan(&self) -> Result<super::ProductMaterialPlanV1> {
        self.inspect()?;
        let baseline = self
            .journal
            .entries
            .iter()
            .map(|entry| (entry.id, &entry.before))
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&baseline).map_err(|_| Error::Invalid)?;
        Ok(self.materials.plan()?.observed_baselines(
            material_sha256(&bytes),
            self.journal.nss_baseline_sha256.clone(),
            self.journal.state == InstallState::Applied,
        ))
    }
    pub fn apply(&mut self) -> Result<InstallReport> {
        let result = self.apply_inner();
        if let Err(error) = result {
            self.mark_unknown(error);
            return Err(error);
        }
        Ok(self.report())
    }
    fn apply_inner(&mut self) -> Result<()> {
        self.check(FencePhase::BeforeWrite)?;
        if self.journal.state == InstallState::RolledBack {
            return Err(Error::Conflict);
        }
        if self.journal.state == InstallState::Applied {
            self.runtime_directories(false)?;
            for entry in &self.journal.entries {
                if self.classify(entry)? != Current::New {
                    return Err(Error::Conflict);
                }
            }
            return Ok(());
        }
        self.journal.state = InstallState::Applying;
        self.journal.failure = None;
        self.persist()?;
        let mut order = (0..self.journal.entries.len()).collect::<Vec<_>>();
        order.sort_by_key(|i| self.journal.entries[*i].id.spec().phase);
        for index in order {
            self.check(FencePhase::BeforeWrite)?;
            let current = self.classify(&self.journal.entries[index])?;
            if current == Current::New {
                let actual = self
                    .root
                    .read(self.journal.entries[index].id.spec().path, MAX_FILE)?
                    .ok_or(Error::Conflict)?;
                self.journal.entries[index].written = Some(actual.stamp);
                self.journal.entries[index].state = EntryState::Written;
                self.persist()?;
                continue;
            }
            if current != Current::Before {
                return Err(Error::Conflict);
            }
            let id = self.journal.entries[index].id;
            let spec = id.spec();
            if spec.phase == MaterialPhase::LauncherLast {
                // No current executor/updater/trust target exists in the table.
                // The launcher changes last, while the original guards remain.
                self.check(FencePhase::BeforeWrite)?;
            }
            self.root
                .ensure_dir(spec.path.rsplit_once('/').ok_or(Error::Invalid)?.0, 0o755)?;
            self.journal.entries[index].state = EntryState::Replacing;
            self.persist()?;
            #[cfg(test)]
            self.boundary(TestBoundary::BeforeReplace, Some(id))?;
            let old = self.root.read(spec.path, MAX_FILE)?;
            let bytes = self.materials.payload(id)?;
            let root = &self.root;
            let nss = &self.nss;
            let services = &self.services;
            let fence = self.fence;
            let materials = &self.materials;
            let journal = &mut self.journal;
            let journal_file = &mut self.journal_file;
            let transaction = journal.transaction.clone();
            let written =
                root.replace_with_intent(spec.path, old.as_ref(), bytes, spec.mode, |stamp| {
                    journal.entries[index].pending = Some(stamp.clone());
                    persist_journal(root, journal, journal_file, &transaction)?;
                    materials.revalidate()?;
                    nss.revalidate(root)?;
                    flags_off(root)?;
                    services.revalidate_off()?;
                    fence.revalidate(
                        FencePhase::BeforeWrite,
                        &OwnedRootChanges::new(root, Some(journal)),
                    )
                })?;
            #[cfg(test)]
            self.boundary(TestBoundary::AfterReplace, Some(id))?;
            self.journal.entries[index].written = Some(written.stamp);
            self.journal.entries[index].state = EntryState::Written;
            self.persist()?;
            #[cfg(test)]
            self.boundary(TestBoundary::AfterJournal, Some(id))?;
            self.check(FencePhase::AfterWrite)?;
        }
        #[cfg(test)]
        self.boundary(TestBoundary::BeforeReload, None)?;
        self.runtime_directories(true)?;
        self.services.reload()?;
        #[cfg(test)]
        self.boundary(TestBoundary::AfterReload, None)?;
        self.check(FencePhase::Finalize)?;
        for entry in &self.journal.entries {
            if self.classify(entry)? != Current::New {
                return Err(Error::Conflict);
            }
        }
        self.runtime_directories(false)?;
        self.journal.state = InstallState::Applied;
        self.journal.failure = None;
        self.persist()?;
        self.check(FencePhase::Finalize)
    }
    /// Recovery is explicit: resume validated writes or restore ONLY this
    /// transaction's proven targets. Neither operation touches later data.
    pub fn recover(&mut self, rollback: bool) -> Result<InstallReport> {
        if !rollback {
            return self.apply();
        }
        let result = self.rollback_inner();
        if let Err(error) = result {
            self.mark_unknown(error);
            return Err(error);
        }
        Ok(self.report())
    }
    fn rollback_inner(&mut self) -> Result<()> {
        self.check(FencePhase::Recover)?;
        self.journal.state = InstallState::RollingBack;
        self.journal.failure = None;
        self.persist()?;
        let mut order = (0..self.journal.entries.len()).collect::<Vec<_>>();
        order.sort_by_key(|i| std::cmp::Reverse(self.journal.entries[*i].id.spec().phase));
        let mut failure = None;
        for index in order {
            if let Err(error) = self.rollback_entry(index) {
                failure.get_or_insert(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        self.services.reload()?;
        self.check(FencePhase::Finalize)?;
        self.journal.state = InstallState::RolledBack;
        self.persist()?;
        self.check(FencePhase::Finalize)
    }
    fn rollback_entry(&mut self, index: usize) -> Result<()> {
        self.check(FencePhase::Recover)?;
        let entry = self.journal.entries[index].clone();
        let current = self.classify(&entry)?;
        if matches!(current, Current::Before | Current::Restored) {
            self.journal.entries[index].state = EntryState::Restored;
            return self.persist();
        }
        if current != Current::New {
            return Err(Error::Conflict);
        }
        let id = entry.id;
        let path = id.spec().path;
        let actual = self.root.read(path, MAX_FILE)?.ok_or(Error::Conflict)?;
        self.journal.entries[index].state = EntryState::Restoring;
        self.journal.entries[index].pending = None;
        self.persist()?;
        #[cfg(test)]
        self.boundary(TestBoundary::BeforeRestore, Some(id))?;
        if let Some(before) = &entry.before {
            let backup = self
                .root
                .read(&backup_path(&self.journal.transaction, id), MAX_FILE)?
                .ok_or(Error::Integrity)?;
            if material_sha256(&backup.bytes) != before.sha256
                || backup.bytes.len() as u64 != before.stamp.size
            {
                return Err(Error::Integrity);
            }
            let root = &self.root;
            let nss = &self.nss;
            let services = &self.services;
            let fence = self.fence;
            let materials = &self.materials;
            let journal = &mut self.journal;
            let journal_file = &mut self.journal_file;
            let transaction = journal.transaction.clone();
            let restored = root.replace_with_intent(
                path,
                Some(&actual),
                &backup.bytes,
                before.stamp.mode,
                |stamp| {
                    journal.entries[index].pending = Some(stamp.clone());
                    persist_journal(root, journal, journal_file, &transaction)?;
                    materials.revalidate()?;
                    nss.revalidate(root)?;
                    flags_off(root)?;
                    services.revalidate_off()?;
                    fence.revalidate(
                        FencePhase::BeforeWrite,
                        &OwnedRootChanges::new(root, Some(journal)),
                    )
                },
            )?;
            self.journal.entries[index].restored = Some(restored.stamp);
        } else {
            self.check(FencePhase::BeforeWrite)?;
            self.root.remove(path, &actual)?;
            self.journal.entries[index].restored = None;
        }
        #[cfg(test)]
        self.boundary(TestBoundary::AfterRestore, Some(id))?;
        self.journal.entries[index].state = EntryState::Restored;
        self.persist()?;
        self.check(FencePhase::AfterWrite)
    }
    fn classify(&self, entry: &Entry) -> Result<Current> {
        let actual = self.root.read(entry.id.spec().path, MAX_FILE)?;
        if let Some(before) = &entry.before {
            if actual.as_ref().is_some_and(|a| {
                a.stamp == before.stamp && material_sha256(&a.bytes) == before.sha256
            }) {
                return Ok(Current::Before);
            }
        } else if actual.is_none() {
            return Ok(Current::Before);
        }
        if let Some(actual) = &actual {
            let digest = material_sha256(&actual.bytes);
            if entry.state == EntryState::Restoring {
                if let (Some(before), Some(stamp)) = (&entry.before, &entry.pending) {
                    if digest == before.sha256 && renamed(stamp, &actual.stamp) {
                        return Ok(Current::Restored);
                    }
                }
            }
            if entry.restored.as_ref().is_some_and(|s| s == &actual.stamp)
                && entry.before.as_ref().is_some_and(|b| digest == b.sha256)
            {
                return Ok(Current::Restored);
            }
            let proof = entry.written.as_ref().is_some_and(|s| s == &actual.stamp)
                || entry
                    .pending
                    .as_ref()
                    .is_some_and(|s| renamed(s, &actual.stamp));
            if proof
                && digest == entry.new_sha256
                && actual.bytes.len() as u64 == entry.new_bytes
                && actual.stamp.mode == entry.id.spec().mode
            {
                return Ok(Current::New);
            }
        }
        Err(Error::Conflict)
    }
    fn runtime_directories(&self, create: bool) -> Result<()> {
        if self.journal.bindings.profile == super::Profile::RecoveryHealth {
            self.root.health_directories(create)?;
        }
        if self.journal.bindings.profile.product() {
            self.root
                .runtime_directory(false, self.nss.nkn_uid(), create)?;
            self.root
                .runtime_directory(true, self.nss.ble_uid(), create)?;
        }
        Ok(())
    }
    fn persist(&mut self) -> Result<()> {
        let tx = self.journal.transaction.clone();
        persist_journal(&self.root, &self.journal, &mut self.journal_file, &tx)
    }
    fn mark_unknown(&mut self, error: Error) {
        self.journal.state = InstallState::Unknown;
        self.journal.failure = Some(
            match error {
                Error::Denied => "denied",
                Error::Invalid => "invalid",
                Error::Integrity => "integrity",
                Error::Signature => "signature",
                Error::Busy => "busy",
                Error::Io => "io",
                Error::Conflict => "conflict",
                Error::Unsupported => "unsupported",
                Error::Unknown => "unknown",
            }
            .into(),
        );
        let _ = self.persist(); // Original failure remains; never call failed cleanup successful.
    }
    fn check(&self, phase: FencePhase) -> Result<()> {
        if self.started.elapsed() > Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        self.materials.revalidate()?;
        self.nss.revalidate(&self.root)?;
        flags_off(&self.root)?;
        self.services.revalidate_off()?;
        let active = read_pointer(&self.root)?.ok_or(Error::Unknown)?;
        if active.transaction != self.journal.transaction {
            return Err(Error::Conflict);
        }
        let current = self
            .root
            .read(&journal_path(&self.journal.transaction), MAX_JOURNAL)?
            .ok_or(Error::Unknown)?;
        if self
            .journal_file
            .as_ref()
            .is_none_or(|file| file.stamp != current.stamp || file.bytes != current.bytes)
        {
            return Err(Error::Conflict);
        }
        self.fence.revalidate(
            phase,
            &OwnedRootChanges::new(&self.root, Some(&self.journal)),
        )
    }
    fn report(&self) -> InstallReport {
        InstallReport {
            format: 1,
            state: self.journal.state,
            transaction: self.journal.transaction.clone(),
            files: self
                .journal
                .entries
                .iter()
                .map(|e| FileReport {
                    target: e.id,
                    state: match e.state {
                        EntryState::Pending => "pending",
                        EntryState::Replacing => "replacing",
                        EntryState::Written => "written",
                        EntryState::Restoring => "restoring",
                        EntryState::Restored => "restored",
                    },
                })
                .collect(),
            retained_transaction_materials: true,
            retained_runtime_directories: self.journal.bindings.profile.product(),
            starts_services: false,
            changes_business_data: false,
            health_verified: false,
            may_confirm: false,
            shared_cli_integrated: false,
        }
    }
    #[cfg(test)]
    pub(super) fn isolated(
        root: Root,
        key: &[u8; 32],
        expected: &ExpectedBindings,
        fence: &'a dyn LiveInstallationFence,
        services: Box<dyn Services>,
        reopen: bool,
    ) -> Result<Self> {
        Self::open(root, key, expected, fence, services, reopen)
    }
    #[cfg(test)]
    pub(super) fn set_fault(
        &mut self,
        fault: impl FnMut(TestBoundary, Option<TargetId>) -> Result<()> + 'static,
    ) {
        self.fault = Some(Box::new(fault));
    }
    #[cfg(test)]
    fn boundary(&mut self, point: TestBoundary, id: Option<TargetId>) -> Result<()> {
        if let Some(fault) = &mut self.fault {
            fault(point, id)
        } else {
            Ok(())
        }
    }
}
#[derive(PartialEq, Eq)]
enum Current {
    Before,
    New,
    Restored,
}
fn renamed(before: &Stamp, after: &Stamp) -> bool {
    before.device == after.device
        && before.inode == after.inode
        && before.uid == after.uid
        && before.gid == after.gid
        && before.mode == after.mode
        && before.size == after.size
        && before.links == after.links
        && before.mtime == after.mtime
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn transaction_dir(tx: &str) -> String {
    format!("{BASE}/transactions/{tx}")
}
fn journal_path(tx: &str) -> String {
    format!("{}/journal.json", transaction_dir(tx))
}
fn backup_path(tx: &str, id: TargetId) -> String {
    format!("{}/before/{}", transaction_dir(tx), id.as_str())
}
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|_| Error::Integrity)
}
fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let b = serde_json::to_vec(value).map_err(|_| Error::Invalid)?;
    if b.len() > MAX_JOURNAL {
        return Err(Error::Invalid);
    }
    Ok(b)
}
fn read_pointer(root: &Root) -> Result<Option<Pointer>> {
    let Some(file) = root.read(ACTIVE, 2048)? else {
        return Ok(None);
    };
    let p: Pointer = parse(&file.bytes)?;
    if p.format != 1 || !hex(&p.transaction) {
        return Err(Error::Integrity);
    }
    Ok(Some(p))
}
fn put_once(root: &Root, path: &str, bytes: &[u8], mode: u32) -> Result<()> {
    if let Some(existing) = root.read(path, MAX_FILE)? {
        if existing.bytes != bytes || existing.stamp.mode != mode {
            return Err(Error::Conflict);
        }
        return Ok(());
    }
    root.replace(path, None, bytes, mode)?;
    Ok(())
}
fn validate_previous(root: &Root, pointer: Option<&Pointer>) -> Result<()> {
    if let Some(pointer) = pointer {
        let prior = root
            .read(&journal_path(&pointer.transaction), MAX_JOURNAL)?
            .ok_or(Error::Unknown)?;
        let old: Journal = parse(&prior.bytes)?;
        if old.format != 1
            || old.kind != "product-install-transaction"
            || old.transaction != pointer.transaction
            || !matches!(old.state, InstallState::Applied | InstallState::RolledBack)
        {
            return Err(Error::Busy);
        }
    }
    Ok(())
}
fn validate_unpublished(root: &Root, journal: &Journal) -> Result<()> {
    if journal.state != InstallState::Prepared {
        return Err(Error::Conflict);
    }
    for entry in &journal.entries {
        if entry.state != EntryState::Pending
            || entry.pending.is_some()
            || entry.written.is_some()
            || entry.restored.is_some()
        {
            return Err(Error::Conflict);
        }
        let current = root.read(entry.id.spec().path, MAX_FILE)?;
        match (&entry.before, current) {
            (None, None) => {}
            (Some(before), Some(current))
                if current.stamp == before.stamp
                    && material_sha256(&current.bytes) == before.sha256 =>
            {
                let backup = root
                    .read(&backup_path(&journal.transaction, entry.id), MAX_FILE)?
                    .ok_or(Error::Integrity)?;
                if backup.bytes != current.bytes {
                    return Err(Error::Integrity);
                }
            }
            _ => return Err(Error::Conflict),
        }
    }
    Ok(())
}
fn persist_journal(
    root: &Root,
    journal: &Journal,
    file: &mut Option<FileSnapshot>,
    tx: &str,
) -> Result<()> {
    *file = Some(root.replace(&journal_path(tx), file.as_ref(), &encoded(journal)?, 0o600)?);
    Ok(())
}
fn validate_journal(j: &Journal, m: &VerifiedProductMaterials) -> Result<()> {
    if !hex(&j.nss_baseline_sha256)
        || j.format != 1
        || j.kind != "product-install-transaction"
        || j.transaction != m.manifest_sha256()
        || j.bindings != *m.bindings()
        || j.entries.len() != m.bindings().profile.targets().len()
    {
        return Err(Error::Integrity);
    }
    let mut total = 0u64;
    for (entry, spec) in j.entries.iter().zip(m.bindings().profile.targets()) {
        if entry.id != spec.id
            || entry.new_sha256 != material_sha256(m.payload(spec.id)?)
            || entry.new_bytes != m.payload(spec.id)?.len() as u64
        {
            return Err(Error::Integrity);
        }
        if let Some(old) = &entry.before {
            total = total.checked_add(old.stamp.size).ok_or(Error::Invalid)?;
            if !hex(&old.sha256)
                || old.stamp.size > MAX_FILE as u64
                || old.stamp.mode & 0o7022 != 0
                || old.stamp.links != 1
            {
                return Err(Error::Integrity);
            }
        }
    }
    if total > MAX_TOTAL as u64 {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn flags_off(root: &Root) -> Result<()> {
    for path in FLAGS {
        if root.read(path, 1024)?.is_some() {
            return Err(Error::Denied);
        }
    }
    Ok(())
}
fn observe_nss(root: &Root, optional: bool) -> Result<NssObservation> {
    #[cfg(test)]
    if root.is_isolated() {
        return NssObservation::observe_isolated_for(root, optional);
    }
    NssObservation::observe_for(root, optional)
}

#[cfg(test)]
#[derive(Default)]
pub struct IsolatedServiceState {
    pub events: std::cell::RefCell<Vec<&'static str>>,
    pub fail_reload: std::cell::Cell<bool>,
    pub off: std::cell::Cell<bool>,
}
#[cfg(test)]
struct IsolatedServices(std::rc::Rc<IsolatedServiceState>);
#[cfg(test)]
impl Services for IsolatedServices {
    fn prepare_off(&self) -> Result<()> {
        self.0.events.borrow_mut().push("stop_optional");
        if self.0.off.get() {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
    fn revalidate_off(&self) -> Result<()> {
        if self.0.off.get() {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
    fn reload(&self) -> Result<()> {
        self.0.events.borrow_mut().push("daemon_reload");
        if self.0.fail_reload.get() {
            Err(Error::Unknown)
        } else {
            Ok(())
        }
    }
}
#[cfg(test)]
impl<'a> ProductInstaller<'a> {
    pub fn prepare_isolated(
        path: &std::path::Path,
        key: &[u8; 32],
        expected: &ExpectedBindings,
        fence: &'a dyn LiveInstallationFence,
        services: std::rc::Rc<IsolatedServiceState>,
        reopen: bool,
    ) -> Result<Self> {
        Self::isolated(
            Root::isolated(path)?,
            key,
            expected,
            fence,
            Box::new(IsolatedServices(services)),
            reopen,
        )
    }
    pub fn fault_at(
        &mut self,
        hook: impl FnMut(TestBoundary, Option<TargetId>) -> Result<()> + 'static,
    ) {
        self.set_fault(hook);
    }
    pub fn prepare_isolated_accounts(path: &std::path::Path) -> Result<()> {
        super::nss::prepare_accounts(&Root::isolated(path)?)
    }
}
