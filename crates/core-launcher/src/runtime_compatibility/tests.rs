// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde_json::{json, Value};
fn role(mode: &str) -> Value {
    let recovery = mode.contains("recovery");
    let email = mode.contains("email");
    let product = mode != "legacy-v1";
    let runtime = json!({"format":1,"worker_protocol":if email{5}else{4},"outbox":{"application_id":OUTBOX_APP,"read_format":if email{4}else{3},"write_format":if email{4}else{3},"migrate_from":if email{vec![3]}else{vec![1,2]}}});
    let prefixes = if recovery {
        vec![
            "--enable-product-providers-candidate",
            "--enable-secure-recovery-candidate",
        ]
    } else if product {
        vec!["--enable-product-providers-candidate"]
    } else {
        vec![]
    };
    let core = json!({"format":1,"kind":"product-runtime-compatibility","component":"core","mode":mode,"source_archive_sha256":"a".repeat(64),"target_arch":"aarch64","development_fixtures":false,"prefixes":prefixes,"business":{"application_id":1381189698_i64,"read":[if recovery{9}else{6}],"write":if recovery{9}else{6},"maintenance_from":if recovery{vec![6,7]}else{vec![]},"automatic_migration":false},"forwarding_runtime_contract":runtime,"features":{"product_providers":product,"email":email,"recovery_execution":recovery},"opens_database":false,"starts_services":false,"authorization_verified":false});
    let worker = json!({"worker":true,"finite_root_actions":true,"protocol":if email{5}else{4},"source_archive_sha256":"a".repeat(64),"development_fixtures":false,"forwarding_runtime_contract":runtime});
    json!({"mode":mode,"core":core.to_string(),"worker":worker.to_string(),"core_sha256":"b".repeat(64),"worker_sha256":"c".repeat(64)})
}
fn request(mode: &str, schema: u32, outbox: i64) -> Value {
    json!({"format":2,"target_arch":"aarch64","root_mode":"setup_ap","root_contract":installation_contract().to_string(),"data":{"business_schema":schema,"outbox":{"application_id":OUTBOX_APP,"format":outbox}},"selected":role(mode),"candidate":role(mode),"fallback":role(mode)})
}
fn compare(v: &Value) -> UpgradeReport {
    super::super::check_upgrade(&serde_json::to_vec(v).unwrap()).unwrap()
}
fn edit(v: &mut Value, which: &str, edit: impl FnOnce(&mut Value)) {
    let mut core: Value = serde_json::from_str(v[which]["core"].as_str().unwrap()).unwrap();
    edit(&mut core);
    v[which]["core"] = core.to_string().into();
}
#[test]
fn known_modes_and_projection_do_not_grant_authority() {
    for (mode, schema, outbox) in [
        ("legacy-v1", 6, 3),
        ("product-v1", 6, 3),
        ("product-email-v1", 6, 4),
        ("product-recovery-v1", 9, 3),
        ("product-email-recovery-v1", 9, 4),
    ] {
        let v = request(mode, schema, outbox);
        let r = compare(&v);
        assert_eq!(r.exit_code(), 0);
        let j = serde_json::to_value(r).unwrap();
        assert_eq!(j["format"], 2);
        assert_eq!(j["authorization_verified"], false);
        assert_eq!(j["product"]["mode_metadata_verified"], false);
    }
}
#[test]
fn mode9_is_not_base6_or_a_downgrade_permission() {
    let mut v = request("legacy-v1", 6, 3);
    v["candidate"] = role("product-recovery-v1");
    assert_eq!(compare(&v).exit_code(), 4);
    v["fallback"] = role("product-recovery-v1");
    assert_eq!(compare(&v).exit_code(), 3);
    let r = serde_json::to_value(compare(&v)).unwrap();
    assert_eq!(r["projected_data"]["business_schema"], 9);
    let v = request("product-recovery-v1", 7, 3);
    assert_eq!(compare(&v).exit_code(), 3);
    assert_eq!(compare(&request("legacy-v1", 9, 3)).exit_code(), 4);
    assert_eq!(
        compare(&request("product-recovery-v1", 10, 3)).exit_code(),
        4
    );
}
#[test]
fn queue_upgrade_requires_an_able_fallback() {
    let mut v = request("product-email-v1", 6, 3);
    assert_eq!(compare(&v).exit_code(), 3);
    v["fallback"] = role("product-v1");
    assert_eq!(compare(&v).exit_code(), 4);
    v["data"]["outbox"]["application_id"] = 0.into();
    assert_eq!(compare(&v).exit_code(), 4);
}
#[test]
fn fake_modes_and_claimed_schema_without_compilation_are_rejected() {
    for case in [
        "mode",
        "feature",
        "prefix",
        "schema",
        "fixture",
        "source",
        "extra",
        "duplicate",
    ] {
        let mut v = request("product-recovery-v1", 9, 3);
        match case {
            "mode" => v["candidate"]["mode"] = "arbitrary".into(),
            "feature" => edit(&mut v, "candidate", |c| {
                c["features"]["recovery_execution"] = false.into()
            }),
            "prefix" => edit(&mut v, "candidate", |c| c["prefixes"] = json!([])),
            "schema" => edit(&mut v, "candidate", |c| {
                c["business"]["read"] = json!([6, 7, 9])
            }),
            "fixture" => edit(&mut v, "candidate", |c| {
                c["development_fixtures"] = true.into()
            }),
            "source" => edit(&mut v, "candidate", |c| {
                c["source_archive_sha256"] = "d".repeat(64).into()
            }),
            "extra" => edit(&mut v, "candidate", |c| c["healthy"] = true.into()),
            _ => {
                let s = v["candidate"]["core"].as_str().unwrap();
                v["candidate"]["core"] = format!("{{\"format\":1,{}", &s[1..]).into();
            }
        }
        let r = super::super::check_upgrade(&serde_json::to_vec(&v).unwrap());
        assert!(r.is_err() || r.unwrap().exit_code() == 4, "{case}");
    }
}
#[test]
fn exact_outer_schema_and_budget_are_required() {
    let mut v = request("legacy-v1", 6, 3);
    v["healthy"] = true.into();
    assert!(super::super::check_upgrade(&serde_json::to_vec(&v).unwrap()).is_err());
    assert!(super::super::check_upgrade(&vec![b' '; 128 * 1024 + 1]).is_err());
    let mut v = request("legacy-v1", 6, 3);
    v["candidate"]["worker"] = Value::Null;
    assert!(super::super::check_upgrade(&serde_json::to_vec(&v).unwrap()).is_err());
    let bytes = serde_json::to_string(&request("legacy-v1", 6, 3)).unwrap();
    assert!(
        super::super::check_upgrade(format!("{{\"format\":2,{}", &bytes[1..]).as_bytes()).is_err()
    );
}
