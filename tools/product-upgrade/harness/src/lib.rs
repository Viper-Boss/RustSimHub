// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[path = "../../../../crates/core/src/runtime_compatibility.rs"]
pub mod core_runtime;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_compiled_mode_metadata_has_no_database_or_activation_side_effect() {
        let legacy = core_runtime::metadata("legacy-v1");
        if cfg!(feature = "dev-fixtures") {
            assert!(legacy.is_err());
            return;
        }
        let legacy = legacy.unwrap();
        assert_eq!(legacy["business"]["write"], 6);
        assert_eq!(legacy["opens_database"], false);
        assert!(core_runtime::metadata("arbitrary").is_err());
        let product = cfg!(feature = "product-providers-candidate");
        let email = cfg!(feature = "email-production-candidate");
        let mode = if email {
            "product-email-recovery-v1"
        } else {
            "product-recovery-v1"
        };
        let v = core_runtime::metadata(mode);
        if product {
            let v = v.unwrap();
            assert_eq!(v["business"]["read"], serde_json::json!([9]));
            assert_eq!(v["starts_services"], false);
        } else {
            assert!(v.is_err());
        }
    }
}
