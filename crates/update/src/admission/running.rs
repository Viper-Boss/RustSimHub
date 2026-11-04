// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit schema6/forwarding candidate Root contract, no business writer.
use super::*;
mod product;
use rustsimhub_wire::update_health::Snapshot;
use std::os::unix::fs::FileTypeExt;
pub(crate) const SCRATCH: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/update-health";
const DROP: &str = "etc/systemd/system/rustsimhub-core.service.d/55-update-health.conf";
const HEALTH_DROP: &str =
    include_str!("../../../../firmware/system/update-health/55-update-health.conf");
fn materials() -> Vec<Material> {
    let mut list = forwarding();
    list.push(Material {
        path: BASE_PATH,
        expected: Some(text(BASE)),
        mode: 0o644,
    });
    for (path, bytes) in [
        (DROP, HEALTH_DROP),
        (
            "etc/tmpfiles.d/rustsimhub-update-health.conf",
            include_str!("../../../../firmware/system/update-health/tmpfiles.conf"),
        ),
        (
            "etc/systemd/system/rustsimhub-update-health.service",
            include_str!(
                "../../../../firmware/system/update-health/rustsimhub-update-health.service"
            ),
        ),
        (
            "etc/systemd/system/rustsimhub-update-health.timer",
            include_str!(
                "../../../../firmware/system/update-health/rustsimhub-update-health.timer"
            ),
        ),
    ] {
        list.push(Material {
            path,
            expected: Some(text(bytes)),
            mode: 0o644,
        });
    }
    list
}
/// Core-held SQL read lease supplies values; Root independently binds file IDs.
pub(crate) struct Data {
    observations: Vec<Observation>,
    _files: Vec<File>,
}
impl Data {
    #[cfg(test)]
    pub(crate) fn observe(snapshot: &Snapshot) -> Result<Self> {
        Self::at_schema(Path::new("/"), 0, 986, snapshot, 6)
    }
    pub(crate) fn observe_for(snapshot: &Snapshot, root: &Root) -> Result<Self> {
        Self::at_schema(
            Path::new("/"),
            0,
            986,
            snapshot,
            if root.product.is_some() { 9 } else { 6 },
        )
    }
    fn at_schema(
        root: &Path,
        root_uid: u32,
        data_uid: u32,
        snapshot: &Snapshot,
        schema: u32,
    ) -> Result<Self> {
        if snapshot.data_schema != schema {
            return Err(Error::Schema);
        }
        let mut observations = vec![];
        let relative = Path::new(STATE).join("store.sqlite3");
        if !parents(root, &relative, &[root_uid, data_uid], &mut observations)? {
            return Err(Error::Denied);
        }
        let directory = root.join(STATE);
        absent(&directory.join(".restore-pending"), &mut observations)?;
        let mut files = vec![];
        for name in ["store.sqlite3", ".writer.lock"] {
            let path = directory.join(name);
            let observed = Observation::capture(&path)?;
            let file = private_file(&path, data_uid, 0o600, 512 * 1024 * 1024)?;
            let m = file.metadata().map_err(|_| Error::Io)?;
            if observed.stamp.as_ref() != Some(&m.clone().into())
                || (name == "store.sqlite3"
                    && (m.dev(), m.ino()) != (snapshot.data_device, snapshot.data_inode))
            {
                return Err(Error::Integrity);
            }
            files.push(file);
            observations.push(observed);
        }
        observations.push(Observation::capture(&directory.join("store.sqlite3-wal"))?);
        // Metadata only: never read the private identity seed body. A disk
        // rotation/pending restore cannot be hidden by an older in-memory Auth.
        let identity_relative =
            Path::new("var/lib/rustsimhub/rustsimhub-v1/core/identity/identity.seed");
        if !parents(
            root,
            identity_relative,
            &[root_uid, data_uid],
            &mut observations,
        )? {
            return Err(Error::Denied);
        }
        let identity = root
            .join(identity_relative)
            .parent()
            .ok_or(Error::Denied)?
            .to_path_buf();
        absent(&identity.join("identity.pending"), &mut observations)?;
        for name in ["identity.seed", ".identity.lock"] {
            let path = identity.join(name);
            let observed = Observation::capture(&path)?;
            let file = private_file(&path, data_uid, 0o600, 72)?;
            if observed.stamp.as_ref() != Some(&file.metadata().map_err(|_| Error::Io)?.into()) {
                return Err(Error::Busy);
            }
            observations.push(observed);
            files.push(file);
        }
        let data = Self {
            observations,
            _files: files,
        };
        data.revalidate()?;
        Ok(data)
    }
    pub(crate) fn revalidate(&self) -> Result<()> {
        for observed in &self.observations {
            observed.revalidate()?;
        }
        Ok(())
    }
}
pub(crate) struct Root {
    product: Option<product::ProductRoot>,
    observations: Vec<Observation>,
    _files: Vec<File>,
    scratch: PathBuf,
    scratch_identity: (u64, u64),
    uid: u32,
    effective: Box<dyn Fn() -> Result<Vec<u8>>>,
}
impl Root {
    pub(crate) fn observe_for(
        runtime: &crate::runtime_observation::RuntimeObservation,
    ) -> Result<Self> {
        Self::at_product(
            Path::new("/"),
            0,
            Box::new(systemd::running_unit),
            product::ProductRoot::observe(runtime)?,
        )
    }
    #[cfg(test)]
    fn at(root: &Path, uid: u32, effective: Box<dyn Fn() -> Result<Vec<u8>>>) -> Result<Self> {
        Self::at_product(root, uid, effective, None)
    }
    fn at_product(
        root: &Path,
        uid: u32,
        effective: Box<dyn Fn() -> Result<Vec<u8>>>,
        product: Option<product::ProductRoot>,
    ) -> Result<Self> {
        let mut observations = vec![];
        let mut files = vec![];
        for spec in materials()
            .into_iter()
            .filter(|s| product.is_none() || s.path != DROP)
        {
            if !parents(root, Path::new(spec.path), &[uid], &mut observations)? {
                return Err(Error::Denied);
            }
            let path = root.join(spec.path);
            let observed = Observation::capture(&path)?;
            let mut file = private_file(&path, uid, spec.mode, 256 * 1024)?;
            let mut bytes = vec![];
            file.by_ref()
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::Io)?;
            if spec.expected.as_ref() != Some(&bytes)
                || observed.stamp.as_ref() != Some(&file.metadata().map_err(|_| Error::Io)?.into())
            {
                return Err(Error::Integrity);
            }
            observations.push(observed);
            files.push(file);
        }
        // A health profile never guesses how to run AP/recovery/provider
        // overrides. Loaded-unit validation also rejects runtime/global drops.
        let directory = root.join("etc/systemd/system/rustsimhub-core.service.d");
        let entries = fs::read_dir(&directory)
            .map_err(|_| Error::Io)?
            .map(|e| e.map(|e| e.file_name()).map_err(|_| Error::Io))
            .collect::<Result<Vec<_>>>()?;
        let expected: &[&str] = if product.is_some() {
            &[
                "30-forwarding.conf",
                "50-product-profile.conf",
                "55-product-health.conf",
            ]
        } else {
            &["30-forwarding.conf", "55-update-health.conf"]
        };
        if entries.len() != expected.len()
            || entries
                .iter()
                .any(|n| !n.to_str().is_some_and(|n| expected.contains(&n)))
        {
            return Err(Error::Denied);
        }
        observations.push(Observation::capture(&directory)?);
        let scratch = root.join(SCRATCH.trim_start_matches('/'));
        if !parents(
            root,
            Path::new(SCRATCH.trim_start_matches('/')),
            &[uid],
            &mut observations,
        )? {
            return Err(Error::Denied);
        }
        let sm = fs::symlink_metadata(&scratch).map_err(|_| Error::Io)?;
        if !sm.is_dir() || sm.uid() != uid || sm.mode() & 0o7777 != 0o700 {
            return Err(Error::Denied);
        }
        // Scratch directory timestamps necessarily change on preparation; keep
        // its original inode/owner/mode, never refresh the business guards.
        let result = Self {
            product,
            observations,
            _files: files,
            scratch,
            scratch_identity: (sm.dev(), sm.ino()),
            uid,
            effective,
        };
        result.revalidate()?;
        Ok(result)
    }
    pub(crate) fn product_health(&self) -> bool {
        self.product.is_some()
    }
    pub(crate) fn compatible_product(
        &self,
        snapshot: &Snapshot,
        candidate: &crate::product_admission::SignedProductPrograms,
        baseline: &crate::product_admission::SignedProductPrograms,
    ) -> Result<()> {
        if self.product.is_none() {
            return Err(Error::Denied);
        }
        self.revalidate()?;
        product::compatible(snapshot, candidate, baseline)
    }
    pub(crate) fn scratch(&self) -> &Path {
        &self.scratch
    }
    pub(crate) fn revalidate(&self) -> Result<()> {
        for guard in &self.observations {
            guard.revalidate()?;
        }
        let m = fs::symlink_metadata(&self.scratch).map_err(|_| Error::Io)?;
        if !m.is_dir()
            || m.uid() != self.uid
            || m.mode() & 0o7777 != 0o700
            || (m.dev(), m.ino()) != self.scratch_identity
        {
            return Err(Error::Integrity);
        }
        if let Some(product) = &self.product {
            product.revalidate()?;
        }
        effective_profile(&((self.effective)()?), self.product.is_some())
    }
}
#[cfg(test)]
fn effective(bytes: &[u8]) -> Result<()> {
    effective_profile(bytes, false)
}
fn effective_profile(bytes: &[u8], product: bool) -> Result<()> {
    let mut values = std::collections::BTreeMap::new();
    for line in std::str::from_utf8(bytes)
        .map_err(|_| Error::Invalid)?
        .lines()
    {
        let (k, v) = line.split_once('=').ok_or(Error::Invalid)?;
        if ![
            "LoadState",
            "FragmentPath",
            "DropInPaths",
            "NeedDaemonReload",
        ]
        .contains(&k)
            || values.insert(k, v).is_some()
        {
            return Err(Error::Invalid);
        }
    }
    let expected = if product {
        "/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf /etc/systemd/system/rustsimhub-core.service.d/50-product-profile.conf /etc/systemd/system/rustsimhub-core.service.d/55-product-health.conf"
    } else {
        "/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf /etc/systemd/system/rustsimhub-core.service.d/55-update-health.conf"
    };
    if values.len() != 4
        || values["LoadState"] != "loaded"
        || values["FragmentPath"] != "/etc/systemd/system/rustsimhub-core.service"
        || values["DropInPaths"] != expected
        || values["NeedDaemonReload"] != "no"
    {
        return Err(Error::Integrity);
    }
    Ok(())
}
/// Bind an endpoint inode as well as kernel peer credentials. No caller path.
pub(crate) fn socket() -> Result<(u64, u64)> {
    for (path, uid, mode) in [
        ("/run", 0, None),
        ("/run/rustsimhub", 0, None),
        ("/run/rustsimhub/update-health", 986, Some(0o700)),
    ] {
        let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
        if !m.is_dir()
            || m.uid() != uid
            || m.mode() & 0o022 != 0
            || mode.is_some_and(|p| m.mode() & 0o7777 != p)
        {
            return Err(Error::Denied);
        }
    }
    let m = fs::symlink_metadata(rustsimhub_wire::update_health::SOCKET).map_err(|_| Error::Io)?;
    if !m.file_type().is_socket() || m.uid() != 986 || m.mode() & 0o7777 != 0o600 || m.nlink() != 1
    {
        return Err(Error::Denied);
    }
    Ok((m.dev(), m.ino()))
}
#[cfg(test)]
mod tests;

pub(crate) fn compatible(
    snapshot: &Snapshot,
    candidate: &SignedPrograms,
    baseline: &SignedPrograms,
) -> Result<()> {
    candidate.supports_running_health()?;
    // The same explicit Root prefix is retained on fallback startup.
    baseline.supports_running_health()?;
    let request = serde_json::json!({"format":1,"target_arch":"aarch64","root_mode":"forwarding","root_contract":null,
        "data":{"business_schema":snapshot.data_schema,"outbox":snapshot.forwarding.outbox_schema.map(|s|serde_json::json!({"application_id":0x52534657i64,"format":s}))},
        "selected":candidate.metadata(),"candidate":candidate.metadata(),"fallback":baseline.metadata()});
    let report = rustsimhub_core_launcher::check_upgrade(
        &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
    )
    .map_err(|_| Error::Invalid)?;
    if report.exit_code() != 0 {
        return Err(Error::Schema);
    }
    Ok(())
}

#[cfg(test)]
impl Root {
    /// Explicit native/PID1 fixture only. Its effective manager output is
    /// synthetic; every installed byte/Root/data identity guard remains real.
    pub(crate) fn native_fixture_install() -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        for spec in materials() {
            let path = Path::new("/").join(spec.path);
            fs::create_dir_all(path.parent().ok_or(Error::Denied)?).map_err(|_| Error::Io)?;
            fs::write(&path, spec.expected.ok_or(Error::Invalid)?).map_err(|_| Error::Io)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(spec.mode))
                .map_err(|_| Error::Io)?;
        }
        fs::create_dir_all(SCRATCH).map_err(|_| Error::Io)?;
        fs::set_permissions(SCRATCH, fs::Permissions::from_mode(0o700)).map_err(|_| Error::Io)?;
        Self::at(
            Path::new("/"),
            0,
            Box::new(|| {
                Ok(b"LoadState=loaded\nFragmentPath=/etc/systemd/system/rustsimhub-core.service\nDropInPaths=/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf /etc/systemd/system/rustsimhub-core.service.d/55-update-health.conf\nNeedDaemonReload=no\n".to_vec())
            }),
        )
    }
}
