// SPDX-License-Identifier: AGPL-3.0-only
//! Pure fixed-mode metadata. No DB, service activation or migration on this path.
//! Window8 wires `--product-runtime-compatibility MODE` BEFORE activation prefixes.
use serde_json::{json, Value};

pub fn metadata(mode: &str) -> Result<Value, &'static str> {
    let product = cfg!(feature = "product-providers-candidate");
    let email = cfg!(feature = "email-production-candidate");
    let recovery = cfg!(feature = "recovery-execution");
    let prefixes: &[&str] = match mode {
        "legacy-v1" => &[],
        "product-v1" if product && !email => &["--enable-product-providers-candidate"],
        "product-email-v1" if product && email => &["--enable-product-providers-candidate"],
        "product-recovery-health-v1" if product && recovery && !email => &[
            "--enable-update-health-candidate",
            "--enable-product-providers-candidate",
            "--enable-secure-recovery-candidate",
        ],
        "product-recovery-v1" if product && recovery && !email => &[
            "--enable-product-providers-candidate",
            "--enable-secure-recovery-candidate",
        ],
        "product-email-recovery-v1" if product && recovery && email => &[
            "--enable-product-providers-candidate",
            "--enable-secure-recovery-candidate",
        ],
        _ => return Err("unsupported product runtime mode"),
    };
    let schema = if matches!(
        mode,
        "product-recovery-v1" | "product-email-recovery-v1" | "product-recovery-health-v1"
    ) {
        9
    } else {
        6
    };
    let source = rustsimhub_wire::build_source_sha256().ok_or("missing committed source stamp")?;
    if cfg!(feature = "dev-fixtures") {
        return Err("fixture Core is not production metadata");
    }
    Ok(
        json!({"format":1,"kind":"product-runtime-compatibility","component":"core",
        "mode":mode,"source_archive_sha256":source,"target_arch":std::env::consts::ARCH,
        "development_fixtures":false,"prefixes":prefixes,
        "business":{"application_id":1381189698_i64,"read":[schema],"write":schema,
            "maintenance_from":if schema==9 {vec![6,7]} else {vec![]},"automatic_migration":false},
        "forwarding_runtime_contract":rustsimhub_forwarder::runtime_compatibility(),
        "features":{"product_providers":product,"email":email,"recovery_execution":recovery},
        "opens_database":false,"starts_services":false,"authorization_verified":false}),
    )
}
