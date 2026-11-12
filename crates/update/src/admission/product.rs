// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-path production owner of the original maintenance/slot/data guards.
use super::*;
use crate::{
    product_admission::{
        coordinator, ProductDataAdmission, ProductRuntimeModeV1, SignedProductPrograms,
    },
    product_materials::{
        self as materials,
        nss::NssObservation,
        rootio::Root,
        services::{self, Services, Systemd},
        ExpectedBindings, FencePhase, OwnedRootChanges, ProductInstaller, ProductManifestV1,
        Profile, VerifiedProductMaterials,
    },
    store::{self, Store},
    verify, verify_payload, MAX_MANIFEST,
};
use std::{
    cell::RefCell,
    io::Write,
    time::{Duration, Instant},
};
const APP: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/app";
const CANDIDATE: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/app-inbox";
const INBOX: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/product-inbox";
const INSTALL: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/product-install";
const TRUST: &str = "/usr/share/rustsimhub/release.public";
const BACKUPS: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/product-backups";
fn install<T>(r: materials::Result<T>) -> Result<T> {
    r.map_err(|_| Error::Denied)
}
struct HeldFile {
    file: File,
    observation: Observation,
}
impl HeldFile {
    fn capture(path: &Path, limit: u64) -> Result<Self> {
        let observed = Observation::capture(path)?;
        let mode = observed.stamp.as_ref().ok_or(Error::Denied)?.mode & 0o7777;
        if mode & 0o222 != 0 && mode & 0o022 != 0 {
            return Err(Error::Denied);
        }
        let file = private_file(path, 0, mode, limit)?;
        if observed.stamp.as_ref() != Some(&file.metadata().map_err(|_| Error::Io)?.into()) {
            return Err(Error::Busy);
        }
        Ok(Self {
            file,
            observation: observed,
        })
    }
    fn selected_generation(&self, entry: &crate::Entry) -> Result<Self> {
        use std::io::{Seek, SeekFrom};
        let before = self.observation.stamp.as_ref().ok_or(Error::Integrity)?;
        let observation = Observation::capture(&self.observation.path)?;
        let now = observation.stamp.as_ref().ok_or(Error::Integrity)?;
        let mode = if entry.name == "Opus-COPYING.txt" {
            0o444
        } else {
            0o555
        };
        // Only the actual original selector's chmod is admitted. No inode,
        // ownership, bytes, link count, size or mtime change is adopted.
        if self.observation.path.file_name().and_then(|n| n.to_str()) != Some(entry.name.as_str())
            || before.dev != now.dev
            || before.ino != now.ino
            || before.uid != now.uid
            || before.gid != now.gid
            || before.links != now.links
            || before.size != now.size
            || before.modified != now.modified
            || now.mode & 0o7777 != mode
            || now.size != entry.bytes
        {
            return Err(Error::Integrity);
        }
        let mut file = self.file.try_clone().map_err(|_| Error::Io)?;
        if Some(now) != Some(&file.metadata().map_err(|_| Error::Io)?.into()) {
            return Err(Error::Busy);
        }
        file.seek(SeekFrom::Start(0)).map_err(|_| Error::Io)?;
        let mut bytes = Vec::new();
        std::io::Read::by_ref(&mut file)
            .take(crate::MAX_PAYLOAD + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Io)?;
        if bytes.len() as u64 != entry.bytes || crate::manifest_hash(&bytes) != entry.sha256 {
            return Err(Error::Integrity);
        }
        let result = Self { file, observation };
        result.check()?;
        Ok(result)
    }
    fn check(&self) -> Result<()> {
        self.observation.revalidate()?;
        if self.observation.stamp.as_ref()
            != Some(&self.file.metadata().map_err(|_| Error::Io)?.into())
        {
            return Err(Error::Busy);
        }
        Ok(())
    }
}
struct Authority {
    store: RefCell<Store>,
    files: Vec<HeldFile>,
    root: Root,
    nss: NssObservation,
    package: VerifiedProductMaterials,
    expected: ExpectedBindings,
    transaction: String,
    started: Instant,
    completed_recovery: Option<coordinator::CompletedUpgradeJournal>,
    completed_email: Option<coordinator::CompletedUpgradeJournal>,
}
impl retained::RetainedAuthority for Authority {
    fn transaction(&self) -> &str {
        &self.transaction
    }
    fn profile(&self) -> Profile {
        self.expected.profile
    }
    fn runtime_directory(&self, path: &str) -> Option<(u32, u32, u32)> {
        if self.expected.profile == Profile::RecoveryHealth {
            match path {
                "/run/rustsimhub/update-health" => return Some((986, 986, 0o700)),
                "/var/lib/rustsimhub/rustsimhub-v1/system/update-health" => {
                    return Some((0, 0, 0o700))
                }
                _ => (),
            }
        }
        match path {
            "/run/rustsimhub/nkn" if self.expected.profile.product() => {
                Some((986, self.nss.nkn_uid(), 0o2750))
            }
            // Every product's signed fixed tmpfiles template contains both
            // role directories, even when recovery is not its startup mode.
            "/run/rustsimhub/recovery" if self.expected.profile.product() => {
                Some((986, self.nss.ble_uid(), 0o2750))
            }
            _ => None,
        }
    }
    fn check_source(&self) -> Result<()> {
        self.check()
    }
    fn revalidate(&self, _: FencePhase, _: &OwnedRootChanges<'_>) -> Result<()> {
        self.check()
    }
}
impl Authority {
    fn check(&self) -> Result<()> {
        if self.started.elapsed() >= Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        for file in &self.files {
            file.check()?;
        }
        if let Some(completed) = &self.completed_recovery {
            completed.revalidate()?;
        }
        if let Some(email) = &self.completed_email {
            email.revalidate()?;
        }
        install(self.nss.revalidate(&self.root))?;
        install(self.package.revalidate())?;
        install(Systemd.revalidate_off())?;
        self.store.borrow().revalidate_product_lock()?;
        match fs::symlink_metadata(Path::new(APP).join("state.next")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => return Err(Error::Busy),
        }
        Ok(())
    }
}
fn mode(profile: Profile) -> ProductRuntimeModeV1 {
    match profile {
        Profile::Legacy => ProductRuntimeModeV1::Legacy,
        Profile::Product => ProductRuntimeModeV1::Product,
        Profile::Email => ProductRuntimeModeV1::Email,
        Profile::Recovery => ProductRuntimeModeV1::Recovery,
        Profile::EmailRecovery => ProductRuntimeModeV1::EmailRecovery,
        Profile::RecoveryHealth => ProductRuntimeModeV1::RecoveryHealth,
    }
}
pub(super) fn source_directory(reopen: bool) -> Result<PathBuf> {
    if !reopen {
        return Ok(INBOX.into());
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Pointer {
        format: u16,
        transaction: String,
    }
    let bytes = store::read_checked(&Path::new(INSTALL).join("active.json"), 2048)?;
    let pointer: Pointer = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
    if pointer.format != 1 || !crate::hex_digest(&pointer.transaction) {
        return Err(Error::Invalid);
    }
    Ok(Path::new(INSTALL)
        .join("transactions")
        .join(pointer.transaction)
        .join("materials"))
}
/// Exact commands only. No path, hash, UID, schema, root, or health boolean is
/// accepted. Extra arguments fail before Store::open can recover/create files.
pub fn dispatch(args: &[String]) -> Result<serde_json::Value> {
    let [command] = args else {
        return Err(Error::Invalid);
    };
    if !matches!(
        command.as_str(),
        "product-prepare"
            | "product-apply"
            | "product-inspect"
            | "product-recover"
            | "product-rollback"
            | "product-activate"
    ) {
        return Err(Error::Invalid);
    }
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    let started = Instant::now();
    let reopen = matches!(
        command.as_str(),
        "product-inspect" | "product-recover" | "product-rollback" | "product-activate"
    );
    let root = install(Root::production())?;
    let trust = HeldFile::capture(Path::new(TRUST), 32)?;
    let key: [u8; 32] = store::read_checked(Path::new(TRUST), 32)?
        .try_into()
        .map_err(|_| Error::Invalid)?;
    // Never let a diagnostic/preparation implicitly recover state.next.
    if fs::symlink_metadata(Path::new(APP).join("state.next")).is_ok() {
        return Err(Error::Busy);
    }
    let state = HeldFile::capture(&Path::new(APP).join("state.json"), 8192)?;
    let mut files = vec![trust, state];
    let schema = actual_schema()?;
    let store = Store::open(Path::new(APP), key, schema)?;
    // A duplicate activation must fail BEFORE stopping a live candidate.
    if command == "product-activate" && !store.product_activation_pending() {
        return Err(Error::Denied);
    }
    let baseline = store.product_baseline()?;
    let baseline_manifest = verify(&baseline.0, &baseline.1, &key, schema, 0)?;
    let baseline_directory = Path::new(APP)
        .join("versions")
        .join(crate::manifest_hash(&baseline.0));
    for name in ["manifest.json", "manifest.sig"].into_iter().chain(
        crate::application_files(baseline_manifest.format)?
            .iter()
            .copied(),
    ) {
        files.push(HeldFile::capture(
            &baseline_directory.join(name),
            crate::MAX_PAYLOAD,
        )?);
    }
    let source = source_directory(reopen)?;
    let bytes = store::read_checked(
        &source.join("manifest.json"),
        materials::package_manifest_limit(),
    )?;
    let preview: ProductManifestV1 = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
    // Root installation and data migration precede application staging. After
    // interruption, no slot candidate exists yet, but the retained signed Root
    // transaction still binds the inbox application, not the confirmed baseline.
    // Preview only selects the fixed input path; full installed-key verification
    // below independently binds that exact application and every Root payload.
    let inspect_confirmed = command == "product-inspect"
        && store.product_pending().is_none()
        && preview.bindings.application_manifest_sha256 == crate::manifest_hash(&baseline.0);
    let candidate_directory = if inspect_confirmed {
        baseline_directory.clone()
    } else {
        store
            .product_pending()
            .map(|h| Path::new(APP).join("versions").join(h))
            .unwrap_or_else(|| PathBuf::from(CANDIDATE))
    };
    let candidate_path = candidate_directory.as_path();
    let manifest = store::read_checked(&candidate_path.join("manifest.json"), MAX_MANIFEST)?;
    let signature = store::read_checked(&candidate_path.join("manifest.sig"), 64)?;
    let candidate = verify(
        &manifest,
        &signature,
        &key,
        schema,
        if store.product_pending().is_some() || inspect_confirmed {
            0
        } else {
            store.floor()
        },
    )?;
    if let Some(hash) = store.product_pending() {
        if crate::manifest_hash(&manifest) != hash {
            return Err(Error::Integrity);
        }
    }
    let payloads = store::load_payloads(candidate_path, &candidate)?;
    verify_payload(&candidate, &payloads)?;
    for name in ["manifest.json", "manifest.sig"]
        .into_iter()
        .chain(crate::application_files(candidate.format)?.iter().copied())
    {
        files.push(HeldFile::capture(
            &candidate_path.join(name),
            crate::MAX_PAYLOAD,
        )?);
    }
    let nss = install(NssObservation::observe_for(
        &root,
        preview.bindings.profile.product(),
    ))?;
    let mut expected = preview.bindings;
    // Commit is covered by the installed-key signature; archive/Core/application
    // identities are independently obtained from the actual signed application.
    expected.source_archive_sha256 = candidate.source_sha256.clone();
    expected.application_manifest_sha256 = crate::manifest_hash(&manifest);
    expected.core_sha256 = crate::manifest_hash(&payloads[0]);
    expected.approved_role_policy_sha256 = nss.policy_sha256().into();
    let package = install(VerifiedProductMaterials::open(
        &install(root.open_dir(source.to_str().ok_or(Error::Invalid)?))?,
        &key,
        &expected,
    ))?;
    install(package.validate_role_materials(nss.nkn_uid(), nss.ble_uid()))?;
    // v1 installed profile binds a single Core. A different fallback Core would
    // fail the launcher after rollback; reject that combination before stopping.
    if expected.profile.product() && crate::manifest_hash(&baseline.2[0]) != expected.core_sha256 {
        return Err(Error::Denied);
    }
    let desired = mode(expected.profile);
    let selected_mode = current_mode(&root)?;
    #[cfg(test)]
    eprintln!("product dispatch: {command} signed-programs");
    let selected = SignedProductPrograms::inspect(
        &baseline.0,
        &baseline.1,
        &key,
        &baseline.2,
        schema,
        selected_mode,
    )?;
    let fallback = SignedProductPrograms::inspect(
        &baseline.0,
        &baseline.1,
        &key,
        &baseline.2,
        schema,
        desired,
    )?;
    let next =
        SignedProductPrograms::inspect(&manifest, &signature, &key, &payloads, schema, desired)?;
    let transaction = package.manifest_sha256().to_owned();
    let completed_recovery = if expected.profile.recovery() && schema == 9 {
        Some(
            coordinator::UpgradeJournal::production(coordinator::UpgradeKind::Recovery9)?
                .completed()?,
        )
    } else {
        None
    };
    // Provision only Root-owned fixed private bookkeeping parents before
    // capturing the original Root generation and data writer snapshot.
    install(root.ensure_dir(&format!("{BACKUPS}/{transaction}"), 0o700))?;
    for name in ["email4", "recovery9"] {
        install(root.ensure_dir(
            &format!("/var/lib/rustsimhub/rustsimhub-v1/system/product-upgrade/{name}"),
            0o700,
        ))?;
    }
    if command == "product-inspect" {
        install(Systemd.revalidate_off())?;
    } else {
        install(services::stop_business())?;
    }
    #[cfg(test)]
    eprintln!("product dispatch: {command} snapshot");
    let snapshot = MaintenanceSnapshot::observe_qualified(Path::new("/"), 0, 986, true)?;
    systemd::effective_product(&snapshot)?;
    if expected.profile == Profile::RecoveryHealth
        && snapshot.root_mode != rustsimhub_core_launcher::Mode::Forwarding
    {
        return Err(Error::Denied);
    }
    // A default updater cannot perform the explicit SMTP5 transition. Refuse
    // before any signed Root target is installed, not after switching profile.
    if matches!(expected.profile, Profile::Email | Profile::EmailRecovery)
        && snapshot
            .outbox
            .as_ref()
            .is_some_and(|d| d.version.format == 3)
        && !cfg!(feature = "email-production-candidate")
    {
        return Err(Error::Denied);
    }
    #[cfg(feature = "email-production-candidate")]
    if matches!(expected.profile, Profile::Email | Profile::EmailRecovery) {
        if let Some(outbox) = snapshot.outbox.as_ref().filter(|db| db.version.format == 3) {
            // Check the ORIGINAL active Owner/generation/binding while both
            // writers are still held, before preparing or writing Root targets.
            // The original email child repeats this check at its handoff.
            snapshot
                .business
                .check_email_binding(&outbox.email_binding()?)?;
        }
    }
    #[cfg(test)]
    eprintln!("product dispatch: {command} comparison");
    // v1 binds candidate/fallback Core bytes but does not retain a verified
    // fallback launch-profile capability. Reverting Root prefixes after a
    // schema9 migration could start the same Core in its schema6 base mode.
    // Application fallback keeps the installed recovery profile; explicit
    // Root rollback must remain closed until such before-profile proof exists.
    if command == "product-rollback"
        && expected.profile.recovery()
        && snapshot.business.version.format == 9
    {
        return Err(Error::Denied);
    }
    let admission = ProductDataAdmission::compare(&snapshot, &selected, &next, &fallback)?;
    if admission.exit_code() == 4 {
        return Err(Error::Denied);
    }
    let data_report = admission.report()?;
    drop(admission);
    let mut completed_email = None;
    #[cfg(test)]
    eprintln!("product dispatch: {command} backup");
    backup(
        &snapshot,
        &root,
        &transaction,
        &mut files,
        &mut completed_email,
    )?;
    let authority = Authority {
        store: RefCell::new(store),
        files,
        root,
        nss,
        package,
        expected,
        transaction,
        started,
        completed_recovery,
        completed_email,
    };
    #[cfg(test)]
    eprintln!("product dispatch: {command} authority");
    authority.check()?;
    let fence = retained::DataFence::new(snapshot, &authority)?;
    #[cfg(test)]
    eprintln!("product dispatch: {command} root-reopen");
    let mut installer = install(if reopen {
        ProductInstaller::reopen(&key, &authority.expected, &fence)
    } else {
        ProductInstaller::prepare(&key, &authority.expected, &fence)
    })?;
    let report =
        match command.as_str() {
            "product-prepare" => {
                serde_json::to_value(install(installer.plan())?).map_err(|_| Error::Invalid)?
            }
            "product-inspect" | "product-activate" => {
                serde_json::to_value(install(installer.inspect())?).map_err(|_| Error::Invalid)?
            }
            "product-rollback" => serde_json::to_value(install(installer.recover(true))?)
                .map_err(|_| Error::Invalid)?,
            "product-recover" => serde_json::to_value(install(installer.recover(false))?)
                .map_err(|_| Error::Invalid)?,
            "product-apply" => {
                serde_json::to_value(install(installer.apply())?).map_err(|_| Error::Invalid)?
            }
            _ => return Err(Error::Invalid),
        };
    drop(installer);
    fence.with_snapshot(MaintenanceSnapshot::revalidate)?;
    let mut snapshot = fence.into_snapshot()?;
    if command == "product-activate" {
        if authority.expected.profile != Profile::RecoveryHealth || report["state"] != "applied" {
            return Err(Error::Denied);
        }
        let comparison = ProductDataAdmission::compare(&snapshot, &next, &next, &fallback)?;
        if comparison.exit_code() != 0 {
            return Err(Error::Denied);
        }
        comparison.revalidate()?;
        drop(comparison);
        systemd::effective_product(&snapshot)?;
        authority.check()?;
        authority
            .store
            .borrow_mut()
            .activate_product(&crate::manifest_hash(&manifest))?;
        // Only the two database writer guards are consumed before Core starts.
        // The original updater lock, signed bytes, trust and Root fds stay held.
        let selected_files = authority
            .files
            .iter()
            .filter_map(|file| {
                if file.observation.path.parent() != Some(candidate_path) {
                    return None;
                }
                candidate
                    .files
                    .iter()
                    .find(|entry| {
                        file.observation.path.file_name().and_then(|n| n.to_str())
                            == Some(entry.name.as_str())
                    })
                    .map(|entry| file.selected_generation(entry))
            })
            .collect::<Result<Vec<_>>>()?;
        if selected_files.len() != candidate.files.len() {
            return Err(Error::Integrity);
        }
        let handoff = snapshot.handoff_runtime()?;
        let selected_state = HeldFile::capture(&Path::new(APP).join("state.json"), 8192)?;
        authority.store.borrow().revalidate_product_state()?;
        install(services::activate_business())?;
        handoff.revalidate()?;
        selected_state.check()?;
        authority.store.borrow().revalidate_product_state()?;
        for file in &authority.files {
            if file.observation.path == Path::new(APP).join("state.json") {
                continue;
            }
            if let Some(selected) = selected_files
                .iter()
                .find(|selected| selected.observation.path == file.observation.path)
            {
                selected.check()?;
            } else {
                file.check()?;
            }
        }
        install(authority.nss.revalidate(&authority.root))?;
        install(authority.package.revalidate())?;
        if authority.started.elapsed() >= Duration::from_secs(300) {
            return Err(Error::Busy);
        }
        return Ok(
            serde_json::json!({"format":1,"command":command,"attempt_consumed":true,
            "started_fixed_business_units":true,"health_verified":false,"may_confirm":false}),
        );
    }
    if command == "product-apply" || command == "product-recover" {
        // Preserve the original email baseline/base6/SMTP5 restrictions; when
        // both are required email must complete before recovery changes base6.
        if matches!(
            authority.expected.profile,
            Profile::Email | Profile::EmailRecovery
        ) && snapshot
            .outbox
            .as_ref()
            .is_some_and(|d| d.version.format == 3)
        {
            #[cfg(feature = "email-production-candidate")]
            {
                let mut journal =
                    coordinator::UpgradeJournal::production(coordinator::UpgradeKind::Email4)?;
                snapshot = coordinator::migrate_email4_retained(
                    &mut authority.store.borrow_mut(),
                    snapshot,
                    &selected,
                    &next,
                    &fallback,
                    &mut journal,
                )?;
                #[cfg(test)]
                tests::after_email_boundary(&snapshot);
            }
            #[cfg(not(feature = "email-production-candidate"))]
            return Err(Error::Denied);
        }
        if authority.expected.profile.recovery() && snapshot.business.version.format != 9 {
            let plan = coordinator::prepare_recovery9(
                snapshot,
                &selected,
                &next,
                &fallback,
                &authority.transaction,
            )?;
            let check = || authority.check();
            let mut handoff = recovery::RecoveryHandoff::new(plan.permit(), &check);
            let mut journal =
                coordinator::UpgradeJournal::production(coordinator::UpgradeKind::Recovery9)?;
            coordinator::migrate_recovery9(plan, &mut journal, &mut handoff)?;
            snapshot = handoff.finish()?;
        }
        authority.check()?;
        let comparison = ProductDataAdmission::compare(&snapshot, &next, &next, &fallback)?;
        if comparison.exit_code() != 0 {
            return Err(Error::Denied);
        }
        comparison.revalidate()?;
        drop(comparison);
        systemd::effective_product(&snapshot)?;
        authority.check()?;
        authority
            .store
            .borrow_mut()
            .stage_product(&snapshot, &manifest, &signature, &payloads)?;
        snapshot.revalidate()?;
        // Store mutation is now durable. The old state observation is consumed,
        // never refreshed and reused as an earlier slot-generation permit.
    }
    Ok(
        serde_json::json!({"format":1,"command":command,"shared_cli_integrated":true,
        "root_install":report,"data_admission":data_report,"starts_services":false,
        "health_verified":false,"may_confirm":false}),
    )
}
fn current_mode(root: &Root) -> Result<ProductRuntimeModeV1> {
    let Some(profile) = install(root.read("/etc/rustsimhub/product-profile.json", 4096))? else {
        return Ok(ProductRuntimeModeV1::Legacy);
    };
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Installed {
        format: u16,
        profile: Profile,
        source_archive_sha256: String,
        core_sha256: String,
    }
    let profile: Installed = serde_json::from_slice(&profile.bytes).map_err(|_| Error::Invalid)?;
    if profile.format != 1
        || !crate::hex_digest(&profile.source_archive_sha256)
        || !crate::hex_digest(&profile.core_sha256)
    {
        return Err(Error::Invalid);
    }
    Ok(mode(profile.profile))
}
fn actual_schema() -> Result<u32> {
    // No writer is opened and no number supplied by the caller. The complete
    // immutable/quick-check observation follows after bounded business stop.
    let mut f = private_file(
        &Path::new("/").join(STATE).join("store.sqlite3"),
        986,
        0o600,
        512 * 1024 * 1024,
    )?;
    let mut header = [0; 100];
    f.read_exact(&mut header).map_err(|_| Error::Io)?;
    if &header[..16] != b"SQLite format 3\0" {
        return Err(Error::Integrity);
    }
    let schema = u32::from_be_bytes(header[60..64].try_into().map_err(|_| Error::Invalid)?);
    if !matches!(schema, 6 | 7 | 9) {
        return Err(Error::Schema);
    }
    Ok(schema)
}
fn backup(
    snapshot: &MaintenanceSnapshot,
    root: &Root,
    transaction: &str,
    files: &mut Vec<HeldFile>,
    completed_email: &mut Option<coordinator::CompletedUpgradeJournal>,
) -> Result<()> {
    snapshot.revalidate()?;
    for (name, database) in [
        ("business.sqlite3", Some(&snapshot.business)),
        ("outbox.sqlite3", snapshot.outbox.as_ref()),
    ] {
        let Some(database) = database else {
            continue;
        };
        let path = format!("{BACKUPS}/{transaction}/{name}");
        let existing_backup = install(root.read(&path, 64 * 1024 * 1024))?;
        if existing_backup.is_none() {
            let mut source = database.backup_source()?;
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                .open(&path)
                .map_err(|_| Error::Io)?;
            std::io::copy(&mut source, &mut output).map_err(|_| Error::Io)?;
            output
                .flush()
                .and_then(|_| output.sync_all())
                .map_err(|_| Error::Io)?;
            File::open(Path::new(&path).parent().ok_or(Error::Invalid)?)
                .and_then(|f| f.sync_all())
                .map_err(|_| Error::Io)?;
        }
        // Backups are evidence retained across retries, never auto-restored.
        let mut source = database.backup_source()?;
        let mut current = Vec::new();
        source.read_to_end(&mut current).map_err(|_| Error::Io)?;
        let current_hash = crate::manifest_hash(&current);
        // A new schema9 maintenance plan already backed up these exact current
        // bytes. Only a retained pre-migration backup needs the old child
        // provenance; never require a fresh transaction to impersonate that ID.
        let expected_hash = if existing_backup
            .as_ref()
            .is_some_and(|b| crate::manifest_hash(&b.bytes) != current_hash)
            && name == "business.sqlite3"
            && database.version.format == 9
        {
            crate::product_recovery_child::backup_source_root(transaction)
                .map_err(|_| Error::Denied)?
        } else if existing_backup
            .as_ref()
            .is_some_and(|b| crate::manifest_hash(&b.bytes) != current_hash)
            && name == "outbox.sqlite3"
            && database.version.format == 4
        {
            if completed_email.is_none() {
                *completed_email = Some(
                    coordinator::UpgradeJournal::production(coordinator::UpgradeKind::Email4)?
                        .completed_email()?,
                );
            }
            completed_email
                .as_ref()
                .ok_or(Error::Denied)?
                .email_source()?
                .to_owned()
        } else {
            current_hash
        };
        let retained = install(root.read(&path, 64 * 1024 * 1024))?.ok_or(Error::Integrity)?;
        if crate::manifest_hash(&retained.bytes) != expected_hash {
            return Err(Error::Integrity);
        }
        files.push(HeldFile::capture(Path::new(&path), 512 * 1024 * 1024)?);
        snapshot.revalidate()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
