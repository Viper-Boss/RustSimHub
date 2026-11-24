// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::{signing_bytes, Entry, FORWARDING_FILES};
use ed25519_dalek::Signer;
use std::{cell::Cell, fs};

fn package() -> (ed25519_dalek::SigningKey, Vec<u8>, Vec<u8>, Vec<Vec<u8>>) {
    let (key, mut manifest, initial) = crate::tests::fixture();
    let mut payloads = initial[..4].to_vec();
    payloads.push(initial[0].clone());
    payloads.push(initial[4].clone());
    manifest.format = 2;
    manifest.schema_min = 6;
    manifest.schema_max = 6;
    manifest.files = FORWARDING_FILES
        .iter()
        .zip(&payloads)
        .map(|(n, b)| Entry {
            name: (*n).into(),
            bytes: b.len() as u64,
            sha256: manifest_hash(b),
        })
        .collect();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let signature = key
        .sign(&signing_bytes(&bytes).unwrap())
        .to_bytes()
        .to_vec();
    (key, bytes, signature, payloads)
}

fn info(worker: bool, source: &str) -> Vec<u8> {
    let runtime = serde_json::json!({"format":1,"worker_protocol":4,"outbox":{"application_id":0x52534657,
        "read_format":3,"write_format":3,"migrate_from":[1,2]}});
    if worker {
        serde_json::to_vec(&serde_json::json!({"worker":true,"protocol":4,
            "source_archive_sha256":source,"development_fixtures":false,
            "forwarding_runtime_contract":runtime}))
        .unwrap()
    } else {
        serde_json::to_vec(
            &serde_json::json!({"component":"core","target_arch":"aarch64",
            "database_schema":6,"development_fixtures":false,"source_archive_sha256":source,
            "service_launch":{"format":1,"modes":["device","forwarding_managed","setup_ap"]},
            "setup_ap_installation_contract":rustsimhub_core_launcher::installation_contract(),
            "forwarding_runtime_contract":runtime}),
        )
        .unwrap()
    }
}

fn inspected() -> SignedPrograms {
    let (key, manifest, signature, payloads) = package();
    let mut worker = false;
    SignedPrograms::inspect_with(
        &manifest,
        &signature,
        &key.verifying_key().to_bytes(),
        &payloads,
        6,
        |_| {
            let result = info(worker, &"a".repeat(64));
            worker = true;
            Ok(result)
        },
    )
    .unwrap()
}

#[test]
fn signature_and_every_payload_are_checked_before_any_executable_is_started() {
    let (key, manifest, signature, mut payloads) = package();
    let calls = Cell::new(0);
    let mut read = |_: &[u8]| {
        calls.set(calls.get() + 1);
        Ok(info(false, &"a".repeat(64)))
    };
    assert!(SignedPrograms::inspect_with(
        &manifest,
        &[0; 64],
        &key.verifying_key().to_bytes(),
        &payloads,
        6,
        &mut read
    )
    .is_err());
    payloads[2][10] ^= 1;
    assert!(SignedPrograms::inspect_with(
        &manifest,
        &signature,
        &key.verifying_key().to_bytes(),
        &payloads,
        6,
        &mut read
    )
    .is_err());
    assert_eq!(calls.get(), 0);
}

#[test]
fn metadata_must_match_the_signed_source_arch_role_and_production_features() {
    let (key, manifest, signature, payloads) = package();
    for (field, value) in [
        ("source_archive_sha256", serde_json::json!("b".repeat(64))),
        ("target_arch", serde_json::json!("x86_64")),
        ("development_fixtures", serde_json::json!(true)),
        ("component", serde_json::json!("edge")),
        ("worker", serde_json::json!(true)),
    ] {
        let mut bad: serde_json::Value =
            serde_json::from_slice(&info(false, &"a".repeat(64))).unwrap();
        bad[field] = value;
        assert!(SignedPrograms::inspect_with(
            &manifest,
            &signature,
            &key.verifying_key().to_bytes(),
            &payloads,
            6,
            |_| Ok(serde_json::to_vec(&bad).unwrap())
        )
        .is_err());
    }
    let mut calls = 0;
    assert!(SignedPrograms::inspect_with(
        &manifest,
        &signature,
        &key.verifying_key().to_bytes(),
        &payloads,
        6,
        |_| {
            calls += 1;
            let source = if calls == 2 { "b" } else { "a" }.repeat(64);
            Ok(info(calls == 2, &source))
        }
    )
    .is_err());
}

#[test]
fn malformed_duplicate_empty_and_oversized_metadata_cannot_become_verified_claims() {
    let (key, manifest, signature, payloads) = package();
    for bytes in [vec![],vec![b'x';LIMIT+1],vec![255],b"{}".to_vec(),
        format!("{{\"source_archive_sha256\":\"{}\",\"source_archive_sha256\":\"{}\",\"development_fixtures\":false}}","a".repeat(64),"a".repeat(64)).into_bytes()] {
        assert!(SignedPrograms::inspect_with(&manifest,&signature,&key.verifying_key().to_bytes(),&payloads,6,|_|Ok(bytes.clone())).is_err());
    }
}

#[test]
fn verified_role_report_still_retains_actual_data_fences_and_denies_mutation_authority() {
    let root = super::super::tests::fixture(rustsimhub_core_launcher::Mode::SetupAp, true);
    let snapshot = super::super::tests::snapshot(root.path());
    let programs = inspected();
    let check = snapshot
        .compare_signed(&programs, &programs, &programs)
        .unwrap();
    assert_eq!(check.exit_code(), 0);
    let report = serde_json::to_value(check.report()).unwrap();
    assert_eq!(report["signed_programs_verified"], true);
    assert_eq!(report["authorization_verified"], false);
    assert_eq!(report["effective_systemd_verified"], false);
    assert_eq!(report["comparison"]["actual_data"]["outbox"]["format"], 3);
    assert!(rustsimhub_storage::Store::open(root.path().join(super::super::STATE)).is_err());
    check.revalidate().unwrap();
    fs::write(
        root.path().join(super::super::BASE_PATH),
        b"changed after comparison",
    )
    .unwrap();
    assert!(check.revalidate().is_err());
}

#[test]
fn executable_copy_is_really_sealed_against_writes_and_truncation() {
    use rustix::fs::{fcntl_get_seals, SealFlags};
    let mut file = sealed(b"fixed signed bytes").unwrap();
    let flags = fcntl_get_seals(&file).unwrap();
    assert!(
        flags.contains(SealFlags::WRITE | SealFlags::SHRINK | SealFlags::GROW | SealFlags::SEAL)
    );
    assert!(file.write_all(b"replacement").is_err());
    assert!(file.set_len(0).is_err());
}

#[test]
fn malformed_executable_bytes_are_refused() {
    let uid = rustix::process::geteuid().as_raw();
    assert!(capture(b"not an executable", uid).is_err());
}

#[test]
#[ignore = "requires isolated Root with CAP_SETUID/CAP_SETGID and compiled synthetic fixtures"]
fn actual_root_capture_drops_to_core_uid_clears_groups_and_enforces_deadline() {
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    let fixture = std::env::var("RUSTSIMHUB_METADATA_FIXTURE").unwrap();
    let waiting = std::env::var("RUSTSIMHUB_METADATA_WAIT_FIXTURE").unwrap();
    let flooding = std::env::var("RUSTSIMHUB_METADATA_FLOOD_FIXTURE").unwrap();
    assert!(!rustix::process::getgroups().unwrap().is_empty());
    let output = capture(&fs::read(fixture).unwrap(), 986).unwrap();
    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        result,
        serde_json::json!({"uid":986,"gid":986,"groups":0,"clean_env":true})
    );
    assert_eq!(
        capture(&fs::read(flooding).unwrap(), 986).err(),
        Some(Error::Invalid)
    );
    let start = Instant::now();
    assert_eq!(
        capture(&fs::read(waiting).unwrap(), 986).err(),
        Some(Error::Busy)
    );
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
#[ignore = "requires isolated Root with CAP_SETUID/CAP_SETGID but without CAP_KILL"]
fn actual_root_capture_requires_the_ability_to_terminate_its_dropped_uid_child() {
    use rustix::thread::{capabilities, CapabilitySet};
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    let effective = capabilities(None).unwrap().effective;
    assert!(effective.contains(CapabilitySet::SETUID | CapabilitySet::SETGID));
    assert!(!effective.contains(CapabilitySet::KILL));
    let waiting = std::env::var("RUSTSIMHUB_METADATA_WAIT_FIXTURE").unwrap();
    let start = Instant::now();
    assert_eq!(
        capture(&fs::read(waiting).unwrap(), 986).err(),
        Some(Error::Denied)
    );
    assert!(start.elapsed() < Duration::from_secs(1));
}
