// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde_json::{json, Value};

fn runtime(format: i64) -> Value {
    json!({"format":1,"worker_protocol":4,"outbox":{"application_id":OUTBOX_APP,
        "read_format":format,"write_format":format,"migrate_from":(1..format).collect::<Vec<_>>()}})
}

fn programs(format: i64) -> Value {
    let runtime = runtime(format);
    json!({"core":json!({"component":"core","target_arch":"aarch64","source_archive_sha256":"a".repeat(64),
        "database_schema":6,"development_fixtures":false,
        "service_launch":{"format":1,"modes":["device","forwarding_managed","setup_ap"]},
        "setup_ap_installation_contract":installation_contract(),"forwarding_runtime_contract":runtime}).to_string(),
        "worker":json!({"worker":true,"protocol":4,"source_archive_sha256":"a".repeat(64),
            "development_fixtures":false,"forwarding_runtime_contract":runtime}).to_string()})
}

fn request(format: i64) -> Value {
    json!({"format":1,"target_arch":"aarch64","root_mode":"setup_ap",
        "root_contract":installation_contract().to_string(),
        "data":{"business_schema":6,"outbox":{"application_id":OUTBOX_APP,"format":format}},
        "selected":programs(3),"candidate":programs(3),"fallback":programs(3)})
}

fn report(request: &Value) -> Value {
    serde_json::to_value(check_upgrade(&serde_json::to_vec(request).unwrap()).unwrap()).unwrap()
}

fn edit_program(request: &mut Value, role: &str, component: &str, edit: impl FnOnce(&mut Value)) {
    let mut info: Value = serde_json::from_str(request[role][component].as_str().unwrap()).unwrap();
    edit(&mut info);
    request[role][component] = json!(info.to_string());
}

#[test]
fn matching_current_versions_are_compatible_without_authorizing_an_update() {
    let result = report(&request(3));
    assert_eq!(result["decision"], "compatible");
    assert_eq!(result["authorization_verified"], false);
    assert_eq!(result["actual_data"], result["projected_data"]);
    for role in ["selected", "candidate", "fallback_after_candidate"] {
        assert_eq!(result[role]["reasons"], json!([]));
    }
}

#[test]
fn migration_is_explicit_and_fallback_is_checked_against_projected_format() {
    for format in [1, 2] {
        let mut input = request(format);
        let result = report(&input);
        assert_eq!(result["decision"], "maintenance_required");
        assert_eq!(
            result["candidate"]["reasons"],
            json!(["migration_required"])
        );
        assert_eq!(result["projected_data"]["outbox"]["format"], 3);
        assert_eq!(result["fallback_after_candidate"]["decision"], "compatible");
        input["fallback"] = programs(format);
        let rejected = report(&input);
        assert_eq!(rejected["decision"], "rejected");
        assert_eq!(
            rejected["fallback_after_candidate"]["reasons"],
            json!(["unsupported_outbox"])
        );
    }
}

#[test]
fn even_absent_outbox_can_be_created_by_candidate_and_break_an_old_fallback() {
    let mut input = request(3);
    input["data"]["outbox"] = Value::Null;
    input["fallback"] = programs(2);
    let result = report(&input);
    assert_eq!(result["actual_data"]["outbox"], Value::Null);
    assert_eq!(result["projected_data"]["outbox"]["format"], 3);
    assert_eq!(result["decision"], "rejected");
}

#[test]
fn verified_historical_start_modes_do_not_invent_outbox_capabilities() {
    for source in [crate::FACTORY16, crate::FORWARDING48, crate::APP49] {
        let mut input = request(3);
        input["fallback"]["core"] = json!(json!({"source_archive_sha256":source,
            "database_schema":6,"development_fixtures":false})
        .to_string());
        input["fallback"]["worker"] = Value::Null;
        let result = report(&input);
        assert_eq!(result["decision"], "maintenance_required");
        assert!(result["fallback_after_candidate"]["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("missing_runtime_contract")));
    }
    let mut input = request(3);
    input["fallback"]["core"] = json!(json!({"source_archive_sha256":"b".repeat(64),
        "database_schema":6,"development_fixtures":false})
    .to_string());
    assert_eq!(report(&input)["decision"], "rejected");
}

#[test]
fn invalid_or_missing_root_contract_needs_maintenance_without_changing_basic_startup() {
    for contract in [
        Value::Null,
        json!("{}"),
        json!("{\"format\":1,\"format\":1}"),
    ] {
        let mut input = request(3);
        input["root_contract"] = contract;
        let result = report(&input);
        assert_eq!(result["decision"], "maintenance_required");
        assert!(result["candidate"]["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("root_contract_unavailable")));
        input["root_mode"] = json!("device");
        assert_eq!(report(&input)["decision"], "compatible");
    }
}

#[test]
fn root_mode_and_database_schema_are_independent_requirements() {
    let mut input = request(3);
    edit_program(&mut input, "candidate", "core", |v| {
        v["service_launch"]["modes"] = json!(["device"])
    });
    assert_eq!(report(&input)["decision"], "maintenance_required");
    input["data"]["business_schema"] = json!(5);
    let result = report(&input);
    assert_eq!(result["decision"], "rejected");
    assert!(result["candidate"]["reasons"]
        .as_array()
        .unwrap()
        .contains(&json!("schema_mismatch")));
}

#[test]
fn wrong_source_protocol_architecture_and_development_programs_are_rejected_per_role() {
    for role in ["selected", "candidate", "fallback"] {
        for (field, value) in [
            ("source_archive_sha256", json!("b".repeat(64))),
            ("protocol", json!(3)),
            ("development_fixtures", json!(true)),
            ("worker", json!(false)),
        ] {
            let mut input = request(3);
            edit_program(&mut input, role, "worker", |v| v[field] = value);
            assert_eq!(report(&input)["decision"], "rejected");
        }
        for (field, value) in [
            ("target_arch", json!("x86_64")),
            ("development_fixtures", json!(true)),
        ] {
            let mut input = request(3);
            edit_program(&mut input, role, "core", |v| v[field] = value);
            assert_eq!(report(&input)["decision"], "rejected");
        }
    }
}

#[test]
fn malformed_and_unknown_runtime_contracts_are_not_guessed() {
    for (field, value) in [
        ("format", json!(2)),
        ("worker_protocol", json!(5)),
        ("format", json!(true)),
        ("extra", json!(1)),
    ] {
        let mut input = request(3);
        edit_program(&mut input, "candidate", "core", |v| {
            v["forwarding_runtime_contract"][field] = value
        });
        assert_eq!(report(&input)["decision"], "rejected");
    }
    for (field, value) in [
        ("read_format", json!(4)),
        ("write_format", json!(2)),
        ("migrate_from", json!([2, 1])),
        ("migrate_from", json!([1, 1])),
        ("migrate_from", json!([0, 1])),
        ("migrate_from", json!([1, 2, 3])),
        ("application_id", json!(0)),
    ] {
        let mut input = request(3);
        edit_program(&mut input, "candidate", "core", |v| {
            v["forwarding_runtime_contract"]["outbox"][field] = value
        });
        assert_eq!(report(&input)["decision"], "rejected");
    }
}

#[test]
fn foreign_unknown_and_unreadable_outbox_formats_are_never_projected_away() {
    for value in [
        json!({"application_id":0,"format":3}),
        json!({"application_id":OUTBOX_APP,"format":4}),
    ] {
        let mut input = request(3);
        input["data"]["outbox"] = value.clone();
        let result = report(&input);
        assert_eq!(result["decision"], "rejected");
        assert_eq!(result["projected_data"]["outbox"], value);
    }
}

#[test]
fn duplicate_metadata_fields_keep_their_original_meaning_and_are_rejected() {
    for component in ["core", "worker"] {
        let mut input = request(3);
        let text = input["candidate"][component].as_str().unwrap().replacen(
            '{',
            "{\"development_fixtures\":false,",
            1,
        );
        input["candidate"][component] = json!(text);
        assert_eq!(report(&input)["decision"], "rejected");
    }
}

#[test]
fn input_bounds_types_duplicates_and_missing_data_observation_are_refused() {
    let mut missing = request(3);
    missing["data"].as_object_mut().unwrap().remove("outbox");
    assert!(check_upgrade(&serde_json::to_vec(&missing).unwrap()).is_err());
    for (field, value) in [
        ("format", json!(true)),
        ("format", json!(2)),
        ("target_arch", json!("arm")),
        ("root_mode", json!("shell")),
        ("root_contract", json!(" ".repeat(4097))),
        ("extra", json!(1)),
    ] {
        let mut input = request(3);
        input[field] = value;
        assert!(check_upgrade(&serde_json::to_vec(&input).unwrap()).is_err());
    }
    let raw = request(3).to_string().replacen('{', "{\"format\":1,", 1);
    assert!(check_upgrade(raw.as_bytes()).is_err());
    for bytes in [vec![], vec![b' '; MAX_REQUEST + 1], b"null".to_vec()] {
        assert!(check_upgrade(&bytes).is_err());
    }
}
