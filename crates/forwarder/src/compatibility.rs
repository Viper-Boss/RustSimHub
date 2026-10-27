// SPDX-License-Identifier: AGPL-3.0-only
//! Public build-time compatibility, never an authorization or a live DB probe.
//! Reading an old outbox requires the stated migration; it is not a downgrade.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const FORWARDING_WORKER_PROTOCOL: u16 = 4;
pub const OUTBOX_APPLICATION_ID: i64 = 0x5253_4657;
pub const OUTBOX_CURRENT_FORMAT: i64 = 3;
pub const OUTBOX_MIGRATION_FORMATS: [i64; 2] = [1, 2];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboxCompatibility {
    pub application_id: i64,
    pub read_format: i64,
    pub write_format: i64,
    pub migrate_from: [i64; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeCompatibility {
    pub format: u16,
    pub worker_protocol: u16,
    pub outbox: OutboxCompatibility,
}

/// Version facts only: neither this value nor an assessment authenticates an
/// Owner or authorizes a migration, delivery, update or rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutboxVersion {
    pub application_id: i64,
    pub format: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxCompatibilityAssessment {
    ReadWrite,
    MigrationRequired,
    ForeignApplication,
    UnsupportedFormat,
}

/// Only an explicitly supported runtime contract can be compared. In
/// particular, absent historical metadata is not today's capability.
pub fn assess_outbox_compatibility(
    runtime: &RuntimeCompatibility,
    actual: OutboxVersion,
) -> Result<OutboxCompatibilityAssessment> {
    use OutboxCompatibilityAssessment::*;
    if runtime != &runtime_compatibility() {
        return Err(Error::Invalid);
    }
    Ok(if actual.application_id != runtime.outbox.application_id {
        ForeignApplication
    } else if actual.format == runtime.outbox.read_format {
        ReadWrite
    } else if runtime.outbox.migrate_from.contains(&actual.format) {
        MigrationRequired
    } else {
        UnsupportedFormat
    })
}

pub fn runtime_compatibility() -> RuntimeCompatibility {
    RuntimeCompatibility {
        format: 1,
        worker_protocol: FORWARDING_WORKER_PROTOCOL,
        outbox: OutboxCompatibility {
            application_id: OUTBOX_APPLICATION_ID,
            read_format: OUTBOX_CURRENT_FORMAT,
            write_format: OUTBOX_CURRENT_FORMAT,
            migrate_from: OUTBOX_MIGRATION_FORMATS,
        },
    }
}

/// Parse only this implementation's supported contract. An unknown contract
/// needs a reviewed adapter, never an inferred latest capability. The caller
/// must separately verify binary provenance and the actual Root/data state.
pub fn parse_runtime_compatibility(bytes: &[u8]) -> Result<RuntimeCompatibility> {
    if bytes.is_empty() || bytes.len() > 4096 {
        return Err(Error::Invalid);
    }
    let contract: RuntimeCompatibility = serde_json::from_slice(bytes)?;
    if contract != runtime_compatibility() {
        return Err(Error::Invalid);
    }
    Ok(contract)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn current_contract_distinguishes_reading_from_required_migration() {
        let expected = json!({"format":1,"worker_protocol":4,"outbox":{
            "application_id":0x5253_4657_i64,"read_format":3,"write_format":3,"migrate_from":[1,2]}});
        let parsed = parse_runtime_compatibility(&serde_json::to_vec(&expected).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), expected);
    }

    #[test]
    fn unknown_runtime_cannot_claim_current_data_compatibility() {
        let mut runtime = runtime_compatibility();
        runtime.outbox.read_format = 2;
        let current = OutboxVersion {
            application_id: OUTBOX_APPLICATION_ID,
            format: 3,
        };
        assert_eq!(
            assess_outbox_compatibility(&runtime, current),
            Err(Error::Invalid)
        );
        runtime = runtime_compatibility();
        runtime.worker_protocol += 1;
        assert_eq!(
            assess_outbox_compatibility(&runtime, current),
            Err(Error::Invalid)
        );
    }

    #[test]
    fn incompatible_ambiguous_and_unbounded_contracts_are_not_promoted() {
        let expected = serde_json::to_value(runtime_compatibility()).unwrap();
        for (field, value) in [
            ("format", json!(2)),
            ("format", json!(true)),
            ("worker_protocol", json!(5)),
            ("worker_protocol", json!(null)),
        ] {
            let mut v = expected.clone();
            v[field] = value;
            assert!(parse_runtime_compatibility(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        for (field, value) in [
            ("application_id", json!(0)),
            ("read_format", json!(2)),
            ("write_format", json!(4)),
            ("migrate_from", json!([1, 2, 3])),
            ("migrate_from", json!([2, 1])),
            ("migrate_from", json!([-1, 2])),
            ("extra", json!(true)),
        ] {
            let mut v = expected.clone();
            v["outbox"][field] = value;
            assert!(parse_runtime_compatibility(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let raw = serde_json::to_string(&expected).unwrap();
        let duplicate = raw.replacen('{', "{\"format\":1,", 1);
        assert!(parse_runtime_compatibility(duplicate.as_bytes()).is_err());
        let nested = raw.replacen("\"outbox\":{", "\"outbox\":{\"read_format\":3,", 1);
        assert!(parse_runtime_compatibility(nested.as_bytes()).is_err());
        for bytes in [vec![], vec![b' '; 4097], b"null".to_vec(), b"{}".to_vec()] {
            assert!(parse_runtime_compatibility(&bytes).is_err());
        }
    }
}
