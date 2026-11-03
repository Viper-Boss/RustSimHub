// SPDX-License-Identifier: AGPL-3.0-only
//! Version comparison only. Metadata provenance, actual data/Root observations
//! and the mutation fence belong to the maintenance caller, not these inputs.
use crate::{installation_contract, select_for_arch, Contract, Error, Metadata, Mode};
use serde::{Deserialize, Serialize};

#[path = "runtime_compatibility/mod.rs"]
mod runtime_compatibility;
pub use runtime_compatibility::ProductRuntimeModeV1;

const MAX_REQUEST: usize = 128 * 1024;
const OUTBOX_APP: i64 = 0x5253_4657;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    format: u16,
    target_arch: String,
    root_mode: Mode,
    root_contract: Option<String>,
    data: DataVersions,
    selected: Programs,
    candidate: Programs,
    fallback: Programs,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DataVersions {
    business_schema: u32,
    #[serde(deserialize_with = "required_outbox")]
    outbox: Option<OutboxVersion>,
}

fn required_outbox<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<OutboxVersion>, D::Error> {
    Option::deserialize(deserializer)
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OutboxVersion {
    application_id: i64,
    format: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Programs {
    // Original stdout strings preserve duplicate-field detection on parsing.
    core: String,
    worker: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Runtime {
    format: u16,
    worker_protocol: u16,
    outbox: OutboxRuntime,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct OutboxRuntime {
    application_id: i64,
    read_format: i64,
    write_format: i64,
    migrate_from: Vec<i64>,
}

impl Runtime {
    fn valid(&self) -> bool {
        // Known Root IPC and format-1 semantics only. Future contracts need a
        // reviewed adapter; declaring an arbitrary number is not support.
        self.format == 1
            && self.outbox.application_id == OUTBOX_APP
            && ((self.worker_protocol == 4
                && (1..=3).contains(&self.outbox.read_format)
                && self.outbox.read_format == self.outbox.write_format
                && self.outbox.migrate_from.len() <= 2
                && self
                    .outbox
                    .migrate_from
                    .iter()
                    .all(|v| (1..self.outbox.read_format).contains(v))
                && self.outbox.migrate_from.windows(2).all(|w| w[0] < w[1]))
                || (self.worker_protocol == 5
                    && self.outbox.read_format == 4
                    && self.outbox.write_format == 4
                    && self.outbox.migrate_from == [3]))
    }
}

#[derive(Deserialize)]
struct CoreRuntime {
    forwarding_runtime_contract: Option<Runtime>,
}

#[derive(Deserialize)]
struct Worker {
    worker: bool,
    protocol: u16,
    source_archive_sha256: String,
    development_fixtures: bool,
    forwarding_runtime_contract: Runtime,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Compatible,
    MaintenanceRequired,
    Rejected,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    InvalidMetadata,
    RootContractUnavailable,
    RootModeUnavailable,
    SchemaMismatch,
    MissingRuntimeContract,
    MissingWorkerMetadata,
    InvalidRuntimeContract,
    WorkerMismatch,
    ForeignOutbox,
    UnsupportedOutbox,
    MigrationRequired,
}

#[derive(Serialize)]
struct Assessment {
    decision: Decision,
    reasons: Vec<Reason>,
    #[serde(skip)]
    runtime: Option<Runtime>,
}

impl Assessment {
    fn note(&mut self, decision: Decision, reason: Reason) {
        self.decision = self.decision.max(decision);
        self.reasons.push(reason);
    }
}

/// A report is deliberately not an authorization token or a live admission
/// proof. The caller must bind signed bytes and hold actual Root/data fences.
#[derive(Serialize)]
pub struct UpgradeReport {
    format: u16,
    target_arch: String,
    root_mode: Mode,
    actual_data: DataVersions,
    decision: Decision,
    authorization_verified: bool,
    selected: Assessment,
    candidate: Assessment,
    fallback_after_candidate: Assessment,
    projected_data: DataVersions,
    #[serde(skip_serializing_if = "Option::is_none")]
    product: Option<runtime_compatibility::ProductExtension>,
}

impl UpgradeReport {
    /// Finite preflight status, not authorization: 0 compatible, 3 maintenance,
    /// 4 rejected. Keep a valid rejected report from looking like shell success.
    pub fn exit_code(&self) -> i32 {
        match self.decision {
            Decision::Compatible => 0,
            Decision::MaintenanceRequired => 3,
            Decision::Rejected => 4,
        }
    }
}

fn assess(programs: &Programs, request: &Request, data: DataVersions) -> Assessment {
    use Decision::*;
    let mut report = Assessment {
        decision: Compatible,
        reasons: vec![],
        runtime: None,
    };
    let installed = request.root_contract.as_deref().map(str::as_bytes);
    let parsed = serde_json::from_str::<Metadata>(&programs.core);
    let mode = select_for_arch(programs.core.as_bytes(), installed, &request.target_arch);
    let (Ok(core), Ok(mode)) = (parsed, mode) else {
        report.note(Rejected, Reason::InvalidMetadata);
        return report;
    };
    if data.business_schema != core.database_schema {
        report.note(Rejected, Reason::SchemaMismatch);
    }
    if request.root_mode == Mode::SetupAp {
        let expected: Contract =
            serde_json::from_value(installation_contract()).expect("compiled contract");
        if !installed
            .and_then(|s| serde_json::from_slice::<Contract>(s).ok())
            .is_some_and(|c| c == expected)
        {
            report.note(MaintenanceRequired, Reason::RootContractUnavailable);
        }
        if mode != Mode::SetupAp {
            report.note(MaintenanceRequired, Reason::RootModeUnavailable);
        }
    } else if request.root_mode == Mode::Forwarding && mode == Mode::Device {
        report.note(MaintenanceRequired, Reason::RootModeUnavailable);
    }
    // A fixed device-only Root entry never opens/migrates the retained outbox.
    if request.root_mode == Mode::Device {
        return report;
    }
    let Ok(envelope) = serde_json::from_str::<CoreRuntime>(&programs.core) else {
        report.note(Rejected, Reason::InvalidRuntimeContract);
        return report;
    };
    let Some(runtime) = envelope.forwarding_runtime_contract else {
        report.note(MaintenanceRequired, Reason::MissingRuntimeContract);
        return report;
    };
    if !runtime.valid() {
        report.note(Rejected, Reason::InvalidRuntimeContract);
        return report;
    }
    let Some(worker) = programs.worker.as_deref() else {
        report.note(MaintenanceRequired, Reason::MissingWorkerMetadata);
        return report;
    };
    let Ok(worker) = serde_json::from_str::<Worker>(worker) else {
        report.note(Rejected, Reason::WorkerMismatch);
        return report;
    };
    if !worker.worker
        || worker.development_fixtures
        || worker.protocol != runtime.worker_protocol
        || worker.source_archive_sha256 != core.source_archive_sha256
        || worker.forwarding_runtime_contract != runtime
    {
        report.note(Rejected, Reason::WorkerMismatch);
        return report;
    }
    if let Some(actual) = data.outbox {
        if actual.application_id != runtime.outbox.application_id {
            report.note(Rejected, Reason::ForeignOutbox);
        } else if actual.format != runtime.outbox.read_format {
            if runtime.outbox.migrate_from.contains(&actual.format) {
                report.note(MaintenanceRequired, Reason::MigrationRequired);
            } else {
                report.note(Rejected, Reason::UnsupportedOutbox);
            }
        }
    }
    report.runtime = Some(runtime);
    report
}

/// Compare all roles, including the candidate's possible persistent migration
/// BEFORE considering fallback. Never execute metadata-supplied paths/commands.
pub fn check_upgrade(bytes: &[u8]) -> Result<UpgradeReport, Error> {
    if bytes.is_empty() || bytes.len() > MAX_REQUEST {
        return Err(Error);
    }
    #[derive(Deserialize)]
    struct Envelope {
        format: u16,
    }
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| Error)?;
    if envelope.format == 2 {
        return runtime_compatibility::compare(bytes);
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| Error)?;
    if request.format != 1
        || !["aarch64", "x86_64"].contains(&request.target_arch.as_str())
        || request
            .root_contract
            .as_ref()
            .is_some_and(|s| s.len() > 4096)
        || [&request.selected, &request.candidate, &request.fallback]
            .iter()
            .any(|p| {
                p.core.is_empty()
                    || p.core.len() > 16384
                    || p.worker
                        .as_ref()
                        .is_some_and(|s| s.is_empty() || s.len() > 16384)
            })
    {
        return Err(Error);
    }
    let selected = assess(&request.selected, &request, request.data);
    let candidate = assess(&request.candidate, &request, request.data);
    let mut projected_data = request.data;
    if candidate.decision != Decision::Rejected {
        if let Some(runtime) = &candidate.runtime {
            projected_data.outbox = Some(OutboxVersion {
                application_id: runtime.outbox.application_id,
                format: runtime.outbox.write_format,
            });
        }
    }
    let fallback_after_candidate = assess(&request.fallback, &request, projected_data);
    Ok(UpgradeReport {
        format: 1,
        target_arch: request.target_arch.clone(),
        root_mode: request.root_mode,
        actual_data: request.data,
        decision: selected
            .decision
            .max(candidate.decision)
            .max(fallback_after_candidate.decision),
        authorization_verified: false,
        selected,
        candidate,
        fallback_after_candidate,
        projected_data,
        product: None,
    })
}

#[cfg(test)]
mod tests;
