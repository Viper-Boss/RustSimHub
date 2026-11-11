// SPDX-License-Identifier: AGPL-3.0-only
//! Root-installed, source/byte-bound opt-in launch profiles. No new API or ON request.
use crate::{arguments, digest, select, Error};
use serde::{Deserialize, Serialize};

pub const PRODUCT_PROFILE: &str = "/etc/rustsimhub/product-profile.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
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
impl Profile {
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
    fn email(self) -> bool {
        matches!(self, Self::Email | Self::EmailRecovery)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    format: u16,
    profile: Profile,
    source_archive_sha256: String,
    core_sha256: String,
}
#[derive(Deserialize)]
struct Info {
    source_archive_sha256: String,
    forwarding_runtime_contract: Runtime,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    format: u16,
    worker_protocol: u16,
    outbox: Outbox,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outbox {
    application_id: i64,
    read_format: i64,
    write_format: i64,
    migrate_from: Vec<i64>,
}

/// Fixed prefixes only. This does not authorize migration, start NKN or enable forwarding.
pub fn profile_arguments(
    metadata: &[u8],
    ap_contract: Option<&[u8]>,
    installed: Option<&[u8]>,
    core_sha256: &str,
) -> Result<Vec<&'static str>, Error> {
    let base = arguments(select(metadata, ap_contract)?);
    let Some(bytes) = installed else {
        return Ok(base);
    };
    if bytes.is_empty() || bytes.len() > 4096 {
        return Err(Error);
    }
    let config: Installed = serde_json::from_slice(bytes).map_err(|_| Error)?;
    let info: Info = serde_json::from_slice(metadata).map_err(|_| Error)?;
    if config.format != 1
        || !digest(&config.source_archive_sha256)
        || !digest(&config.core_sha256)
        || config.core_sha256 != core_sha256
        || config.source_archive_sha256 != info.source_archive_sha256
    {
        return Err(Error);
    }
    if config.profile == Profile::RecoveryHealth {
        let health: serde_json::Value = serde_json::from_slice(metadata).map_err(|_| Error)?;
        if health["update_health_protocol"] != 1 || health["update_health_default_enabled"] != false
        {
            return Err(Error);
        }
    }
    let runtime = info.forwarding_runtime_contract;
    let (protocol, format, migrations): (_, _, &[i64]) = if config.profile.email() {
        (5, 4, &[3])
    } else {
        (4, 3, &[1, 2])
    };
    if runtime.format != 1
        || runtime.worker_protocol != protocol
        || runtime.outbox.application_id != 0x5253_4657
        || runtime.outbox.read_format != format
        || runtime.outbox.write_format != format
        || runtime.outbox.migrate_from != migrations
    {
        return Err(Error);
    }
    Ok(config
        .profile
        .prefixes()
        .iter()
        .copied()
        .chain(base)
        .collect())
}

/// Offline comparison only. The installed runtime reads its own trusted files instead.
pub fn check_product_profile(bytes: &[u8]) -> Result<serde_json::Value, Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        metadata: String,
        ap_contract: Option<String>,
        profile: Option<String>,
        core_sha256: String,
    }
    if bytes.len() > 32768 {
        return Err(Error);
    }
    let request: Request = serde_json::from_slice(bytes).map_err(|_| Error)?;
    let args = profile_arguments(
        request.metadata.as_bytes(),
        request.ap_contract.as_deref().map(str::as_bytes),
        request.profile.as_deref().map(str::as_bytes),
        &request.core_sha256,
    )?;
    Ok(
        serde_json::json!({"format":1,"arguments":args,"authorization_verified":false,"starts_services":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn info(email: bool) -> Vec<u8> {
        serde_json::to_vec(&json!({"component":"core","target_arch":std::env::consts::ARCH,
            "source_archive_sha256":"a".repeat(64),"development_fixtures":false,"database_schema":6,
            "service_launch":{"format":1,"modes":["device","forwarding_managed","setup_ap"]},
            "forwarding_runtime_contract":{"format":1,"worker_protocol":if email {5} else {4},
                "outbox":{"application_id":0x5253_4657_i64,"read_format":if email {4} else {3},
                    "write_format":if email {4} else {3},"migrate_from":if email {vec![3]} else {vec![1,2]}}}})).unwrap()
    }
    fn config(profile: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({"format":1,"profile":profile,
            "source_archive_sha256":"a".repeat(64),"core_sha256":"b".repeat(64)}))
        .unwrap()
    }
    #[test]
    fn missing_profile_preserves_original_launch_without_new_prefix() {
        let meta = info(false);
        assert_eq!(
            profile_arguments(&meta, None, None, ""),
            Ok(arguments(select(&meta, None).unwrap()))
        );
    }
    #[test]
    fn each_finite_profile_has_only_actual_core_prefixes() {
        for (name, email, prefixes) in [
            ("legacy-v1", false, vec![]),
            (
                "product-v1",
                false,
                vec!["--enable-product-providers-candidate"],
            ),
            (
                "product-email-v1",
                true,
                vec!["--enable-product-providers-candidate"],
            ),
            (
                "product-recovery-v1",
                false,
                vec![
                    "--enable-product-providers-candidate",
                    "--enable-secure-recovery-candidate",
                ],
            ),
            (
                "product-email-recovery-v1",
                true,
                vec![
                    "--enable-product-providers-candidate",
                    "--enable-secure-recovery-candidate",
                ],
            ),
        ] {
            let meta = info(email);
            let args =
                profile_arguments(&meta, None, Some(&config(name)), &"b".repeat(64)).unwrap();
            assert_eq!(&args[..prefixes.len()], prefixes);
            assert_eq!(
                &args[prefixes.len()..],
                arguments(select(&meta, None).unwrap())
            );
        }
    }
    #[test]
    fn mixed_source_bytes_email_protocol_and_arbitrary_profile_are_refused() {
        let meta = info(false);
        assert!(profile_arguments(
            &meta,
            None,
            Some(&config("product-email-v1")),
            &"b".repeat(64)
        )
        .is_err());
        assert!(profile_arguments(
            &info(true),
            None,
            Some(&config("product-v1")),
            &"b".repeat(64)
        )
        .is_err());
        assert!(
            profile_arguments(&meta, None, Some(&config("product-v1")), &"c".repeat(64)).is_err()
        );
        for field in ["profile", "core_sha256", "source_archive_sha256"] {
            let mut value: serde_json::Value =
                serde_json::from_slice(&config("product-v1")).unwrap();
            value[field] = json!("/bin/sh");
            assert!(profile_arguments(
                &meta,
                None,
                Some(&serde_json::to_vec(&value).unwrap()),
                &"b".repeat(64)
            )
            .is_err());
        }
    }
    #[test]
    fn corrupt_duplicate_extended_and_empty_profile_does_not_fallback() {
        let good = String::from_utf8(config("product-v1")).unwrap();
        for bytes in [
            vec![],
            b"{}".to_vec(),
            vec![b' '; 4097],
            good.replacen('{', "{\"format\":1,", 1).into_bytes(),
            good.replacen('{', "{\"command\":\"sh\",", 1).into_bytes(),
        ] {
            assert!(profile_arguments(&info(false), None, Some(&bytes), &"b".repeat(64)).is_err());
        }
    }
}
