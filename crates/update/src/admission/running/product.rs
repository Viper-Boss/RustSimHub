// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit installed 14-target health profile, independent of a JSON verdict.
use super::*;
use crate::{
    product_admission::{ProductRuntimeModeV1, SignedProductPrograms},
    product_materials::{
        self as materials,
        nss::NssObservation,
        rootio::{FileSnapshot, Root as IoRoot},
        ExpectedBindings, ProductManifestV1, Profile, VerifiedProductMaterials,
    },
    runtime_observation::RuntimeObservation,
};
fn checked<T>(result: materials::Result<T>) -> Result<T> {
    result.map_err(|_| Error::Denied)
}
pub(super) struct ProductRoot {
    root: IoRoot,
    nss: NssObservation,
    package: VerifiedProductMaterials,
    files: Vec<(&'static str, FileSnapshot)>,
    pointer: FileSnapshot,
}
impl ProductRoot {
    pub(super) fn observe(runtime: &RuntimeObservation) -> Result<Option<Self>> {
        let root = checked(IoRoot::production())?;
        let Some(config) = checked(root.read("/etc/rustsimhub/product-profile.json", 4096))? else {
            return Ok(None);
        };
        let value: serde_json::Value =
            serde_json::from_slice(&config.bytes).map_err(|_| Error::Invalid)?;
        if value["profile"] != "product-recovery-health-v1" {
            return Err(Error::Denied);
        }
        let source = super::super::product::source_directory(true)?;
        let pointer = checked(root.read(
            "/var/lib/rustsimhub/rustsimhub-v1/system/product-install/active.json",
            2048,
        ))?
        .ok_or(Error::Denied)?;
        let manifest = checked(
            root.read(
                source
                    .join("manifest.json")
                    .to_str()
                    .ok_or(Error::Invalid)?,
                32768,
            ),
        )?
        .ok_or(Error::Denied)?;
        let manifest: ProductManifestV1 =
            serde_json::from_slice(&manifest.bytes).map_err(|_| Error::Invalid)?;
        let nss = checked(NssObservation::observe_for(&root, true))?;
        let (application, hash, key) = runtime.product_binding()?;
        let expected = ExpectedBindings {
            profile: Profile::RecoveryHealth,
            source_commit: manifest.bindings.source_commit,
            source_archive_sha256: application.source_sha256.clone(),
            application_manifest_sha256: hash.to_owned(),
            core_sha256: application.files[0].sha256.clone(),
            approved_role_policy_sha256: nss.policy_sha256().into(),
        };
        let package = checked(VerifiedProductMaterials::open(
            &checked(root.open_dir(source.to_str().ok_or(Error::Invalid)?))?,
            key,
            &expected,
        ))?;
        checked(package.validate_role_materials(nss.nkn_uid(), nss.ble_uid()))?;
        let mut files = vec![];
        for spec in Profile::RecoveryHealth.targets() {
            let current = checked(root.read(spec.path, 32 * 1024 * 1024))?.ok_or(Error::Denied)?;
            if current.stamp.mode != spec.mode
                || current.bytes != checked(package.payload(spec.id))?
            {
                return Err(Error::Integrity);
            }
            files.push((spec.path, current));
        }
        checked(root.runtime_directory(false, nss.nkn_uid(), false))?;
        checked(root.runtime_directory(true, nss.ble_uid(), false))?;
        checked(root.health_directories(false))?;
        let result = Self {
            root,
            nss,
            package,
            files,
            pointer,
        };
        result.revalidate()?;
        Ok(Some(result))
    }
    pub(super) fn revalidate(&self) -> Result<()> {
        checked(self.nss.revalidate(&self.root))?;
        checked(self.package.revalidate())?;
        checked(self.pointer.recheck(
            &self.root,
            "/var/lib/rustsimhub/rustsimhub-v1/system/product-install/active.json",
        ))?;
        for (path, file) in &self.files {
            checked(file.recheck(&self.root, path))?;
        }
        checked(
            self.root
                .runtime_directory(false, self.nss.nkn_uid(), false),
        )?;
        checked(self.root.runtime_directory(true, self.nss.ble_uid(), false))?;
        checked(self.root.health_directories(false))
    }
}
pub(super) fn compatible(
    snapshot: &Snapshot,
    candidate: &SignedProductPrograms,
    baseline: &SignedProductPrograms,
) -> Result<()> {
    if snapshot.data_schema != 9
        || candidate.mode() != ProductRuntimeModeV1::RecoveryHealth
        || baseline.mode() != ProductRuntimeModeV1::RecoveryHealth
    {
        return Err(Error::Schema);
    }
    let request = serde_json::json!({"format":2,"target_arch":crate::product_admission::target_arch(),"root_mode":"forwarding","root_contract":null,
        "data":{"business_schema":snapshot.data_schema,"outbox":snapshot.forwarding.outbox_schema.map(|s|serde_json::json!({"application_id":0x52534657i64,"format":s}))},
        "selected":candidate.role(),"candidate":candidate.role(),"fallback":baseline.role()});
    let comparison = rustsimhub_core_launcher::check_upgrade(
        &serde_json::to_vec(&request).map_err(|_| Error::Invalid)?,
    )
    .map_err(|_| Error::Invalid)?;
    if comparison.exit_code() != 0 {
        return Err(Error::Schema);
    }
    Ok(())
}
