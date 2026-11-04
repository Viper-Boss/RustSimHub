// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit format2 product comparison. Format1 retains its old base6 contract.
use super::{
    Assessment, DataVersions, Decision, Error, OutboxVersion, Reason, Runtime, UpgradeReport,
    OUTBOX_APP,
};
use crate::{digest, installation_contract, Mode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductRuntimeModeV1 {
    #[serde(rename = "legacy-v1")]
    Legacy,
    #[serde(rename = "product-v1")]
    Product,
    #[serde(rename = "product-email-v1")]
    Email,
    #[serde(rename = "product-recovery-v1")]
    Recovery,
    #[serde(rename = "product-email-recovery-v1")]
    EmailRecovery,
    #[serde(rename = "product-recovery-health-v1")]
    RecoveryHealth,
}
impl ProductRuntimeModeV1 {
    pub fn prefixes(self) -> &'static [&'static str] {
        match self {
            Self::RecoveryHealth => &[
                "--enable-update-health-candidate",
                "--enable-product-providers-candidate",
                "--enable-secure-recovery-candidate",
            ],
            Self::Legacy => &[],
            Self::Product | Self::Email => &["--enable-product-providers-candidate"],
            Self::Recovery | Self::EmailRecovery => &[
                "--enable-product-providers-candidate",
                "--enable-secure-recovery-candidate",
            ],
        }
    }
    fn recovery(self) -> bool {
        matches!(
            self,
            Self::Recovery | Self::EmailRecovery | Self::RecoveryHealth
        )
    }
    fn email(self) -> bool {
        matches!(self, Self::Email | Self::EmailRecovery)
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Business {
    application_id: i64,
    read: Vec<u32>,
    write: u32,
    maintenance_from: Vec<u32>,
    automatic_migration: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Features {
    product_providers: bool,
    email: bool,
    recovery_execution: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Core {
    format: u16,
    kind: String,
    component: String,
    mode: ProductRuntimeModeV1,
    source_archive_sha256: String,
    target_arch: String,
    development_fixtures: bool,
    prefixes: Vec<String>,
    business: Business,
    forwarding_runtime_contract: Runtime,
    features: Features,
    opens_database: bool,
    starts_services: bool,
    authorization_verified: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Worker {
    worker: bool,
    finite_root_actions: bool,
    protocol: u16,
    source_archive_sha256: String,
    development_fixtures: bool,
    forwarding_runtime_contract: Runtime,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Role {
    mode: ProductRuntimeModeV1,
    core: String,
    worker: String,
    core_sha256: String,
    worker_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    format: u16,
    target_arch: String,
    root_mode: Mode,
    root_contract: Option<String>,
    data: DataVersions,
    selected: Role,
    candidate: Role,
    fallback: Role,
}
#[derive(Serialize)]
pub(super) struct ProductExtension {
    pub(super) protocol: u16,
    modes: [ProductRuntimeModeV1; 3],
    business_migration_required: bool,
    business_after_candidate: u32,
    mode_metadata_verified: bool,
}

fn parse(role: &Role, request: &Request) -> Result<Core, Error> {
    if role.core.len() > 16384
        || role.worker.len() > 16384
        || !digest(&role.core_sha256)
        || !digest(&role.worker_sha256)
    {
        return Err(Error);
    }
    let core: Core = serde_json::from_str(&role.core).map_err(|_| Error)?;
    let worker: Worker = serde_json::from_str(&role.worker).map_err(|_| Error)?;
    let schema = if role.mode.recovery() { 9 } else { 6 };
    if core.format != 1
        || core.kind != "product-runtime-compatibility"
        || core.component != "core"
        || core.mode != role.mode
        || core.target_arch != request.target_arch
        || !digest(&core.source_archive_sha256)
        || core.development_fixtures
        || core.opens_database
        || core.starts_services
        || core.authorization_verified
        || core.prefixes != role.mode.prefixes()
        || core.business.application_id != 1381189698
        || core.business.read != [schema]
        || core.business.write != schema
        || core.business.maintenance_from != if schema == 9 { vec![6, 7] } else { vec![] }
        || core.business.automatic_migration
        || !core.forwarding_runtime_contract.valid()
        || (role.mode != ProductRuntimeModeV1::Legacy
            && (!core.features.product_providers || core.features.email != role.mode.email()))
        || (role.mode.recovery() && !core.features.recovery_execution)
        || core.features.email != (core.forwarding_runtime_contract.worker_protocol == 5)
        || !worker.worker
        || !worker.finite_root_actions
        || worker.development_fixtures
        || worker.protocol != core.forwarding_runtime_contract.worker_protocol
        || worker.source_archive_sha256 != core.source_archive_sha256
        || worker.forwarding_runtime_contract != core.forwarding_runtime_contract
    {
        return Err(Error);
    }
    Ok(core)
}
fn assessment(core: Option<&Core>, request: &Request, data: DataVersions) -> Assessment {
    let mut a = Assessment {
        decision: Decision::Compatible,
        reasons: vec![],
        runtime: None,
    };
    let Some(core) = core else {
        a.note(Decision::Rejected, Reason::InvalidMetadata);
        return a;
    };
    if request.root_mode == Mode::Device && core.mode != ProductRuntimeModeV1::Legacy {
        a.note(Decision::Rejected, Reason::RootModeUnavailable);
    }
    let expected: crate::Contract =
        serde_json::from_value(installation_contract()).expect("compiled Root contract");
    if request.root_mode == Mode::SetupAp
        && request
            .root_contract
            .as_ref()
            .and_then(|s| serde_json::from_str::<crate::Contract>(s).ok())
            != Some(expected)
    {
        a.note(Decision::Rejected, Reason::RootContractUnavailable);
    }
    if !core.business.read.contains(&data.business_schema) {
        if core
            .business
            .maintenance_from
            .contains(&data.business_schema)
        {
            a.note(Decision::MaintenanceRequired, Reason::MigrationRequired);
        } else {
            a.note(Decision::Rejected, Reason::SchemaMismatch);
        }
    }
    let runtime = &core.forwarding_runtime_contract;
    if let Some(outbox) = data.outbox {
        if outbox.application_id != OUTBOX_APP {
            a.note(Decision::Rejected, Reason::ForeignOutbox);
        } else if outbox.format != runtime.outbox.read_format {
            if runtime.outbox.migrate_from.contains(&outbox.format) {
                a.note(Decision::MaintenanceRequired, Reason::MigrationRequired);
            } else {
                a.note(Decision::Rejected, Reason::UnsupportedOutbox);
            }
        }
    }
    a.runtime = Some(runtime.clone());
    a
}
pub(super) fn compare(bytes: &[u8]) -> Result<UpgradeReport, Error> {
    let request: Request = serde_json::from_slice(bytes).map_err(|_| Error)?;
    if request.format != 2
        || !["aarch64", "x86_64"].contains(&request.target_arch.as_str())
        || request
            .root_contract
            .as_ref()
            .is_some_and(|v| v.len() > 4096)
    {
        return Err(Error);
    }
    let cores =
        [&request.selected, &request.candidate, &request.fallback].map(|r| parse(r, &request).ok());
    let selected = assessment(cores[0].as_ref(), &request, request.data);
    let candidate = assessment(cores[1].as_ref(), &request, request.data);
    let mut projected = request.data;
    if candidate.decision != Decision::Rejected {
        if let Some(core) = &cores[1] {
            projected.business_schema = core.business.write;
            projected.outbox = Some(OutboxVersion {
                application_id: OUTBOX_APP,
                format: core.forwarding_runtime_contract.outbox.write_format,
            });
        }
    }
    let fallback = assessment(cores[2].as_ref(), &request, projected);
    // Fallback may never require a *new* migration to read candidate effects.
    let mut fallback = fallback;
    if fallback.decision == Decision::MaintenanceRequired {
        fallback.note(Decision::Rejected, Reason::SchemaMismatch);
    }
    let product = ProductExtension {
        protocol: 1,
        modes: [
            request.selected.mode,
            request.candidate.mode,
            request.fallback.mode,
        ],
        business_migration_required: projected.business_schema != request.data.business_schema,
        business_after_candidate: projected.business_schema,
        mode_metadata_verified: false,
    };
    Ok(UpgradeReport {
        format: 2,
        target_arch: request.target_arch,
        root_mode: request.root_mode,
        actual_data: request.data,
        decision: selected
            .decision
            .max(candidate.decision)
            .max(fallback.decision),
        authorization_verified: false,
        selected,
        candidate,
        fallback_after_candidate: fallback,
        projected_data: projected,
        product: Some(product),
    })
}
#[cfg(test)]
mod tests;
