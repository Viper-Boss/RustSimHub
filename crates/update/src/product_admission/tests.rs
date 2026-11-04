// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
fn fixture() -> (SigningKey, Vec<u8>, Vec<u8>, Vec<Vec<u8>>) {
    let key = SigningKey::from_bytes(&[51; 32]);
    let names = crate::FORWARDING_FILES;
    let payloads = names
        .iter()
        .map(|name| {
            if *name == "Opus-COPYING.txt" {
                return b"synthetic license".to_vec();
            }
            let mut b = vec![0; 64];
            b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
            b[16] = 2;
            b[18] = 183;
            b[52] = 64;
            b
        })
        .collect::<Vec<_>>();
    let manifest = json!({"format":2,"kind":"application","board":"ufi003-v02","arch":"aarch64-linux","epoch":1,"schema_min":6,"schema_max":9,"source_sha256":"a".repeat(64),"development_fixtures":false,
        "files":names.iter().zip(&payloads).map(|(n,b)|json!({"name":n,"bytes":b.len(),"sha256":manifest_hash(b)})).collect::<Vec<_>>()});
    let raw = serde_json::to_vec(&manifest).unwrap();
    let sig = key
        .sign(&crate::signing_bytes(&raw).unwrap())
        .to_bytes()
        .to_vec();
    (key, raw, sig, payloads)
}
fn info(worker: bool) -> Vec<u8> {
    let runtime = json!({"format":1,"worker_protocol":4,"outbox":{"application_id":1381189207_i64,"read_format":3,"write_format":3,"migrate_from":[1,2]}});
    serde_json::to_vec(&if worker {json!({"worker":true,"finite_root_actions":true,"protocol":4,"source_archive_sha256":"a".repeat(64),"development_fixtures":false,"forwarding_runtime_contract":runtime})}
        else {json!({"format":1,"kind":"product-runtime-compatibility","component":"core","mode":"product-recovery-v1","source_archive_sha256":"a".repeat(64),"target_arch":"aarch64","development_fixtures":false,
            "prefixes":["--enable-product-providers-candidate","--enable-secure-recovery-candidate"],"business":{"application_id":1381189698_i64,"read":[9],"write":9,"maintenance_from":[6,7],"automatic_migration":false},"forwarding_runtime_contract":runtime,"features":{"product_providers":true,"email":false,"recovery_execution":true},"opens_database":false,"starts_services":false,"authorization_verified":false})}).unwrap()
}
#[test]
fn signature_payload_mode_source_and_finite_protocol_are_bound() {
    let (k, m, s, p) = fixture();
    let mode = ProductRuntimeModeV1::Recovery;
    let inspected = SignedProductPrograms::finish(
        OriginalVerification::SyntheticTransport,
        SignedInput {
            manifest: &m,
            signature: &s,
            key: &k.verifying_key().to_bytes(),
            payloads: &p,
        },
        6,
        mode,
        |_, args| Ok(info(args == ["--build-info"])),
    )
    .unwrap();
    assert_eq!(inspected.manifest_sha256(), manifest_hash(&m));
    let mut restricted: serde_json::Value = serde_json::from_slice(&m).unwrap();
    restricted["schema_max"] = 6.into();
    let restricted = serde_json::to_vec(&restricted).unwrap();
    let restricted_sig = k
        .sign(&crate::signing_bytes(&restricted).unwrap())
        .to_bytes();
    assert!(matches!(
        SignedProductPrograms::finish(
            OriginalVerification::SyntheticTransport,
            SignedInput {
                manifest: &restricted,
                signature: &restricted_sig,
                key: &k.verifying_key().to_bytes(),
                payloads: &p
            },
            6,
            mode,
            |_, args| Ok(info(args == ["--build-info"]))
        ),
        Err(Error::Schema)
    ));

    for case in [
        "key",
        "signature",
        "payload",
        "source",
        "mode",
        "extra",
        "duplicate",
        "worker",
    ] {
        let mut key = k.verifying_key().to_bytes();
        let mut signature = s.clone();
        let mut payloads = p.clone();
        if case == "key" {
            key = [0; 32];
        }
        if case == "signature" {
            signature[0] ^= 1;
        }
        if case == "payload" {
            payloads[0][0] ^= 1;
        }
        let result = SignedProductPrograms::finish(
            OriginalVerification::SyntheticTransport,
            SignedInput {
                manifest: &m,
                signature: &signature,
                key: &key,
                payloads: &payloads,
            },
            6,
            mode,
            |_, args| {
                let worker = args == ["--build-info"];
                let raw = info(worker);
                let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                if !worker {
                    match case {
                        "source" => value["source_archive_sha256"] = "b".repeat(64).into(),
                        "mode" => value["mode"] = "legacy-v1".into(),
                        "extra" => value["healthy"] = true.into(),
                        _ => (),
                    }
                }
                if worker && case == "worker" {
                    value["protocol"] = 5.into();
                }
                if !worker && case == "duplicate" {
                    let text = String::from_utf8(raw).unwrap();
                    return Ok(
                        format!("{{\"mode\":\"product-recovery-v1\",{}", &text[1..]).into_bytes()
                    );
                }
                Ok(serde_json::to_vec(&value).unwrap())
            },
        );
        assert!(result.is_err(), "{case}");
    }
}
#[test]
fn actual_linux_sealed_probe_is_bounded_without_accepting_a_path_or_stdout_claim() {
    let executable = std::fs::read("/usr/bin/python3").unwrap();
    let uid = rustix::process::geteuid().as_raw();
    let result = capture(&executable, &["-c", "print('actual-exec')"], uid).unwrap();
    assert_eq!(result, b"actual-exec\n");
    assert!(capture(&executable, &["-c", "print('x'*20000)"], uid).is_err());
    assert!(capture(&executable, &["-c", "raise SystemExit(7)"], uid).is_err());
}

/// Run explicitly in the owned Root container after installing its synthetic
/// default Root/Store fixture. The metadata transport remains synthetic here;
/// the ORIGINAL MaintenanceSnapshot, DB and writer flocks are real.
#[test]
#[ignore = "requires disposable Root container with fixed-path synthetic Store6 fixture"]
fn actual_original_guard_borrows_real_writer_and_rejects_root_generation_change() {
    use fs2::FileExt;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    assert!(Path::new("/.dockerenv").exists());
    let (key, manifest, signature, payloads) = fixture();
    let program = SignedProductPrograms::finish(
        OriginalVerification::SyntheticTransport,
        SignedInput {
            manifest: &manifest,
            signature: &signature,
            key: &key.verifying_key().to_bytes(),
            payloads: &payloads,
        },
        6,
        ProductRuntimeModeV1::Legacy,
        |_, args| {
            if args == ["--build-info"] {
                return Ok(info(true));
            }
            let mut core: serde_json::Value = serde_json::from_slice(&info(false)).unwrap();
            core["mode"] = "legacy-v1".into();
            core["prefixes"] = json!([]);
            core["business"]["read"] = json!([6]);
            core["business"]["write"] = 6.into();
            core["business"]["maintenance_from"] = json!([]);
            core["features"] =
                json!({"product_providers":false,"email":false,"recovery_execution":false});
            Ok(serde_json::to_vec(&core).unwrap())
        },
    )
    .unwrap();
    let data = Path::new("/var/lib/rustsimhub/rustsimhub-v1/core/state");
    let before = fs::read(data.join("store.sqlite3")).unwrap();
    let writer = fs::File::open(data.join(".writer.lock")).unwrap();
    writer.try_lock_exclusive().unwrap();
    assert!(MaintenanceSnapshot::observe().is_err());
    FileExt::unlock(&writer).unwrap();
    let snapshot = MaintenanceSnapshot::observe().unwrap();
    let checked = ProductDataAdmission::compare(&snapshot, &program, &program, &program).unwrap();
    assert_eq!(checked.exit_code(), 0);
    assert_eq!(checked.report().unwrap()["authorization_verified"], false);
    assert!(writer.try_lock_exclusive().is_err());
    let base = Path::new("/etc/systemd/system/rustsimhub-core.service");
    let mut original = fs::read(base).unwrap();
    original.push(b'\n');
    fs::write(base, original).unwrap();
    fs::set_permissions(base, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(checked.revalidate().is_err());
    assert!(checked.report().is_err());
    drop(checked);
    assert!(writer.try_lock_exclusive().is_err());
    drop(snapshot);
    writer.try_lock_exclusive().unwrap();
    assert_eq!(fs::read(data.join("store.sqlite3")).unwrap(), before);
    assert!(!data.join("store.sqlite3-wal").exists());
}
