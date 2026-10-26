// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde_json::json;

fn old(source: &str) -> Value {
    json!({"source_archive_sha256":source,"database_schema":6,"development_fixtures":false})
}
fn current() -> Value {
    let mut v = old(&"a".repeat(64));
    v["component"] = json!("core");
    v["target_arch"] = json!(std::env::consts::ARCH);
    v["service_launch"] = json!({"format":1,"modes":["device","forwarding_managed","setup_ap"]});
    v["setup_ap_installation_contract"] = installation_contract();
    v
}
fn choose(v: &Value, contract: Option<&[u8]>) -> Result<Mode, Error> {
    select(&serde_json::to_vec(v).unwrap(), contract)
}
fn contract() -> Vec<u8> {
    serde_json::to_vec(&installation_contract()).unwrap()
}

#[test]
fn known_factory_ignores_new_optional_ap_material() {
    assert_eq!(choose(&old(FACTORY16), Some(&contract())), Ok(Mode::Device));
}
#[test]
fn known_forwarding_release_keeps_managed_mode() {
    for material in [None, Some(contract())] {
        assert_eq!(
            choose(&old(FORWARDING48), material.as_deref()),
            Ok(Mode::Forwarding)
        );
    }
}
#[test]
fn known_app49_requires_both_ap_contracts() {
    let mut v = old(APP49);
    assert_eq!(choose(&v, Some(&contract())), Ok(Mode::Forwarding));
    v["setup_ap_installation_contract"] = installation_contract();
    assert_eq!(choose(&v, None), Ok(Mode::Forwarding));
    assert_eq!(choose(&v, Some(&contract())), Ok(Mode::SetupAp));
}
#[test]
fn unknown_historical_build_is_not_guessed() {
    assert_eq!(choose(&old(&"a".repeat(64)), None), Err(Error));
}
#[test]
fn declared_modes_enable_only_available_services() {
    let mut v = current();
    for (modes, expected) in [
        (json!(["device"]), Mode::Device),
        (json!(["device", "forwarding_managed"]), Mode::Forwarding),
        (
            json!(["setup_ap", "device", "forwarding_managed"]),
            Mode::SetupAp,
        ),
    ] {
        v["service_launch"]["modes"] = modes;
        assert_eq!(choose(&v, Some(&contract())), Ok(expected));
    }
}
#[test]
fn malformed_capabilities_are_refused() {
    for caps in [
        json!({"format":2,"modes":["device"]}),
        json!({"format":true,"modes":["device"]}),
        json!({"format":1.0,"modes":["device"]}),
        json!({"format":1,"modes":[]}),
        json!({"format":1,"modes":["forwarding_managed"]}),
        json!({"format":1,"modes":["device","device"]}),
        json!({"format":1,"modes":["device","setup_ap"]}),
        json!({"format":1,"modes":["device","shell"]}),
        json!({"format":1,"modes":["device"],"command":"/bin/sh"}),
    ] {
        let mut v = current();
        v["service_launch"] = caps;
        assert_eq!(choose(&v, Some(&contract())), Err(Error));
    }
}
#[test]
fn explicit_capabilities_require_component_and_architecture() {
    for field in ["component", "target_arch"] {
        let mut v = current();
        v.as_object_mut().unwrap().remove(field);
        assert_eq!(choose(&v, None), Err(Error));
        v[field] = json!("different");
        assert_eq!(choose(&v, None), Err(Error));
    }
}
#[test]
fn schema_development_and_source_are_checked() {
    for (field, value) in [
        ("database_schema", json!(5)),
        ("database_schema", json!(6.0)),
        ("development_fixtures", json!(true)),
        ("development_fixtures", json!(0)),
        ("source_archive_sha256", json!("A".repeat(64))),
        ("source_archive_sha256", json!("a".repeat(63))),
        ("source_archive_sha256", Value::Null),
    ] {
        let mut v = current();
        v[field] = value;
        assert_eq!(choose(&v, Some(&contract())), Err(Error));
    }
}
#[test]
fn missing_or_unusable_root_contract_only_disables_ap() {
    for bytes in [
        b"".to_vec(),
        b"null".to_vec(),
        b"{".to_vec(),
        vec![b' '; 4097],
    ] {
        assert_eq!(choose(&current(), Some(&bytes)), Ok(Mode::Forwarding));
    }
    for value in [json!(2), json!(true), json!("1"), json!(1.0)] {
        let mut c = installation_contract();
        c["format"] = value;
        assert_eq!(
            choose(&current(), Some(&serde_json::to_vec(&c).unwrap())),
            Ok(Mode::Forwarding)
        );
    }
    let mut c = installation_contract();
    c["unexpected"] = json!(1);
    assert_eq!(
        choose(&current(), Some(&serde_json::to_vec(&c).unwrap())),
        Ok(Mode::Forwarding)
    );
}
#[test]
fn duplicate_root_contract_keys_do_not_enable_ap() {
    let text = String::from_utf8(contract())
        .unwrap()
        .replacen('{', "{\"format\":2,", 1);
    assert_eq!(
        choose(&current(), Some(text.as_bytes())),
        Ok(Mode::Forwarding)
    );
}
#[test]
fn different_core_contract_keeps_forwarding() {
    let mut v = current();
    v["setup_ap_installation_contract"]["material_format"] = json!(2);
    assert_eq!(choose(&v, Some(&contract())), Ok(Mode::Forwarding));
}
#[test]
fn metadata_is_bounded_single_json_with_no_duplicate_fields() {
    assert_eq!(select(&vec![b' '; 16385], None), Err(Error));
    let text = serde_json::to_string(&current()).unwrap();
    assert_eq!(select(format!("{text}{text}").as_bytes(), None), Err(Error));
    let text = text.replacen('{', "{\"database_schema\":6,", 1);
    assert_eq!(select(text.as_bytes(), None), Err(Error));
    let text = serde_json::to_string(&current()).unwrap().replacen(
        "\"setup_ap_installation_contract\":{",
        "\"setup_ap_installation_contract\":{\"format\":1,",
        1,
    );
    assert_eq!(select(text.as_bytes(), Some(&contract())), Err(Error));
}
#[test]
fn arguments_are_exact_and_cannot_come_from_metadata() {
    let base = arguments(Mode::Device);
    assert_eq!(base.len(), 12);
    assert_eq!(base[0], "--serve-device");
    let forwarding = arguments(Mode::Forwarding);
    assert_eq!(forwarding.len(), 13);
    assert_eq!(&forwarding[1..12], &base[1..]);
    assert_eq!(forwarding[12], "983");
    let ap = arguments(Mode::SetupAp);
    assert_eq!(ap.len(), 16);
    assert_eq!(&ap[1..13], &forwarding[1..]);
    assert_eq!(
        &ap[13..],
        [
            "/run/rustsimhub-setup-ap-core",
            "/run/rustsimhub-setup-ap-root",
            "/var/lib/rustsimhub-core/setup-ap-journal"
        ]
    );
    let mut v = current();
    v["command"] = json!("/bin/sh");
    v["args"] = json!(["-c", "anything"]);
    assert_eq!(choose(&v, Some(&contract())), Ok(Mode::SetupAp));
    assert_eq!(arguments(Mode::SetupAp), ap);
}
