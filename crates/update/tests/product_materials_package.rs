// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(target_os = "linux")]
// The shared CLI export is owned by integration. Compile the exact source
// planned for that export; this harness does not replace it with a test model.
#[allow(dead_code, unused_imports)]
#[path = "../src/product_materials/mod.rs"]
mod product_materials;
#[path = "product_materials_support/mod.rs"]
mod support;

use ed25519_dalek::SigningKey;
use product_materials::{
    product_signing_bytes, Error, Profile, TargetId, VerifiedProductMaterials,
};
use std::{
    fs::{self, File},
    os::unix::fs::{symlink, PermissionsExt},
};
use support::{put, Bundle, BLE_GID, NKN_GID};

#[test]
fn every_profile_verifies_actual_signature_all_payloads_and_only_reports_a_plan() {
    for profile in [
        Profile::Legacy,
        Profile::Product,
        Profile::Email,
        Profile::Recovery,
        Profile::EmailRecovery,
        Profile::RecoveryHealth,
    ] {
        let bundle = Bundle::new(profile);
        let verified = bundle.verify().unwrap();
        verified.revalidate().unwrap();
        verified.validate_role_materials(NKN_GID, BLE_GID).unwrap();
        let report = verified.plan_metadata().unwrap();
        assert_eq!(
            serde_json::to_value(verified.plan().unwrap()).unwrap(),
            report
        );
        assert_eq!(report["root_baseline_verified"], false);
        assert_eq!(report["nss_baseline_verified"], false);
        for entry in report["files"].as_array().unwrap() {
            assert_eq!(entry["uid"], 0);
            assert_eq!(entry["gid"], 0);
            assert!(entry.get("path").is_none());
        }
        assert_eq!(report["authorization_verified"], false);
        assert_eq!(report["installed"], false);
        assert_eq!(report["starts_services"], false);
        assert_eq!(
            report["roles"],
            serde_json::to_value(profile.required_roles()).unwrap()
        );
        assert_eq!(
            report["files"].as_array().unwrap().len(),
            profile.targets().len()
        );
        assert!(profile.targets().iter().all(|s| s.path.starts_with('/')
            && !s.path.contains(".seed")
            && !s.path.ends_with("rustsimhub-update")));
    }
}

#[test]
fn external_trust_and_every_live_expected_binding_are_required() {
    let b = Bundle::new(Profile::Recovery);
    let wrong = SigningKey::from_bytes(&[94; 32]).verifying_key().to_bytes();
    assert_eq!(
        VerifiedProductMaterials::open(&b.fd(), &wrong, &b.bindings).err(),
        Some(Error::Signature)
    );
    for item in [
        "profile",
        "commit",
        "archive",
        "application",
        "core",
        "policy",
    ] {
        let mut expected = b.bindings.clone();
        match item {
            "profile" => expected.profile = Profile::EmailRecovery,
            "commit" => expected.source_commit = "f".repeat(40),
            "archive" => expected.source_archive_sha256 = "f".repeat(64),
            "application" => expected.application_manifest_sha256 = "f".repeat(64),
            "core" => expected.core_sha256 = "f".repeat(64),
            _ => expected.approved_role_policy_sha256 = "f".repeat(64),
        }
        assert_eq!(
            VerifiedProductMaterials::open(&b.fd(), &b.public(), &expected).err(),
            Some(Error::Denied),
            "{item}"
        );
    }
}

#[test]
fn old_signature_domains_cannot_be_replayed_and_old_application_parser_rejects_new_kind() {
    let b = Bundle::new(Profile::Legacy);
    let raw = serde_json::to_vec(&b.manifest).unwrap();
    for domain in [
        b"RustSimHub.AppManifest.v1\0".as_slice(),
        b"RustSimHub.SetupApBundle.v1\0".as_slice(),
    ] {
        b.sign_domain(&raw, domain);
        assert_eq!(b.verify().err(), Some(Error::Signature));
    }
    b.sign(&raw);
    let signature = fs::read(b.path().join("manifest.sig")).unwrap();
    assert!(rustsimhub_update::verify(&raw, &signature, &b.public(), 6, 0).is_err());
    b.sign_domain(&raw, b"RustSimHub.AppManifest.v1\0");
    let signature = fs::read(b.path().join("manifest.sig")).unwrap();
    assert!(rustsimhub_update::verify(&raw, &signature, &b.public(), 6, 0).is_err());
}

#[test]
fn signed_duplicate_unknown_and_authority_claim_fields_are_rejected() {
    let b = Bundle::new(Profile::Recovery);
    let raw = serde_json::to_string(&b.manifest).unwrap();
    let mut cases = vec![
        raw.replacen("\"format\":1", "\"format\":1,\"format\":1", 1),
        raw.replacen(
            "\"source_commit\":",
            "\"source_commit\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"source_commit\":",
            1,
        ),
        raw.replacen("\"bytes\":", "\"bytes\":64,\"bytes\":", 1),
    ];
    for field in [
        "verified", "healthy", "approved", "uid", "path", "shell", "unit",
    ] {
        let value = if ["uid", "path", "shell", "unit"].contains(&field) {
            serde_json::json!("untrusted")
        } else {
            serde_json::json!(true)
        };
        let mut json = serde_json::to_value(&b.manifest).unwrap();
        json[field] = value;
        cases.push(serde_json::to_string(&json).unwrap());
    }
    for raw in cases {
        b.sign(raw.as_bytes());
        assert!(b.verify().is_err());
    }
}

#[test]
fn platform_kind_version_roles_and_finite_file_set_are_not_caller_extensible() {
    for case in [
        "board",
        "arch",
        "kind",
        "version",
        "hash",
        "roles",
        "unknown-role",
        "target",
        "missing",
        "duplicate",
        "order",
        "length",
        "total",
    ] {
        let mut b = Bundle::new(Profile::Recovery);
        let mut value = serde_json::to_value(&b.manifest).unwrap();
        match case {
            "board" => value["board"] = "other-board".into(),
            "arch" => value["arch"] = "x86_64-linux".into(),
            "kind" => value["kind"] = "application".into(),
            "version" => value["format"] = 2.into(),
            "hash" => value["bindings"]["source_commit"] = "A".repeat(40).into(),
            "roles" => value["roles"][0] = "ble".into(),
            "unknown-role" => value["roles"][0] = "root".into(),
            "target" => value["files"][0]["id"] = "../etc/passwd".into(),
            "missing" => {
                value["files"].as_array_mut().unwrap().pop();
            }
            "duplicate" => value["files"][1] = value["files"][0].clone(),
            "order" => value["files"].as_array_mut().unwrap().swap(0, 1),
            "length" => value["files"][0]["bytes"] = 0.into(),
            _ => {
                for e in value["files"].as_array_mut().unwrap() {
                    e["bytes"] = (32 * 1024 * 1024).into();
                }
            }
        }
        b.sign(&serde_json::to_vec(&value).unwrap());
        assert!(b.verify().is_err(), "{case}");
        // Keep manifest mutations local; no permissions or helper bypasses.
        b.manifest.format = 1;
    }
}

#[test]
fn digest_length_elf_identity_and_exact_source_templates_are_verified() {
    for case in [
        "digest", "length", "elf", "profile", "dropin", "nkn", "ble", "bus", "tmpfiles",
    ] {
        let mut b = Bundle::new(Profile::Recovery);
        let id = match case {
            "profile" => TargetId::ProfileConfig,
            "dropin" => TargetId::CoreDropin,
            "nkn" => TargetId::NknUnit,
            "ble" => TargetId::BleUnit,
            "bus" => TargetId::BlePolicy,
            "tmpfiles" => TargetId::ProductTmpfiles,
            _ => TargetId::Launcher,
        };
        let path = b.path().join(id.payload_name());
        let mut raw = fs::read(&path).unwrap();
        match case {
            "digest" => {
                raw[20] ^= 1;
                put(&path, &raw);
            }
            "length" => {
                raw.push(0);
                put(&path, &raw);
            }
            "elf" => {
                raw[18] = 62;
                b.replace_payload(id, &raw);
            }
            "profile" => {
                let mut v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                v["profile"] = "legacy-v1".into();
                b.replace_payload(id, &serde_json::to_vec(&v).unwrap());
            }
            "tmpfiles" => {
                let text = String::from_utf8(raw)
                    .unwrap()
                    .replace("/run/rustsimhub/nkn", "/tmp/elsewhere");
                b.replace_payload(id, text.as_bytes());
            }
            _ => {
                raw.push(b'\n');
                b.replace_payload(id, &raw);
            }
        }
        assert!(b.verify().is_err(), "{case}");
    }
}

#[test]
fn package_gids_do_not_approve_their_own_nss_policy() {
    let b = Bundle::new(Profile::Product);
    let verified = b.verify().unwrap();
    verified.validate_role_materials(NKN_GID, BLE_GID).unwrap();
    assert_eq!(
        verified
            .validate_role_materials(NKN_GID + 10, BLE_GID)
            .err(),
        Some(Error::Integrity)
    );
    for id in [0, 29, 983, 984, 985, 986, 987, 65534, u32::MAX] {
        assert_eq!(
            verified.validate_role_materials(id, BLE_GID).err(),
            Some(Error::Denied)
        );
    }
}

#[test]
fn package_directory_contains_only_the_signed_finite_set() {
    for case in ["extra", "missing", "directory", "writable"] {
        let b = Bundle::new(Profile::Legacy);
        match case {
            "extra" => put(&b.path().join("release.public"), &b.public()),
            "missing" => fs::remove_file(b.path().join(TargetId::Launcher.payload_name())).unwrap(),
            "directory" => {
                let p = b.path().join(TargetId::Launcher.payload_name());
                fs::remove_file(&p).unwrap();
                fs::create_dir(p).unwrap();
            }
            _ => fs::set_permissions(b.path(), fs::Permissions::from_mode(0o777)).unwrap(),
        }
        assert!(b.verify().is_err(), "{case}");
    }
}

#[test]
fn source_links_special_files_permissions_and_oversized_objects_fail_before_reading() {
    for case in [
        "symlink",
        "hardlink",
        "fifo",
        "writable",
        "oversized-manifest",
        "oversized-program",
    ] {
        let b = Bundle::new(Profile::Legacy);
        let p = b.path().join(TargetId::Launcher.payload_name());
        match case {
            "symlink" => {
                let saved = b.path().join("saved");
                fs::rename(&p, &saved).unwrap();
                symlink(&saved, &p).unwrap();
            }
            "hardlink" => fs::hard_link(&p, b.path().join("extra-link")).unwrap(),
            "fifo" => {
                fs::remove_file(&p).unwrap();
                rustix::fs::mknodat(
                    rustix::fs::CWD,
                    &p,
                    rustix::fs::FileType::Fifo,
                    rustix::fs::Mode::RUSR,
                    0,
                )
                .unwrap();
            }
            "writable" => fs::set_permissions(&p, fs::Permissions::from_mode(0o666)).unwrap(),
            "oversized-manifest" => File::options()
                .write(true)
                .open(b.path().join("manifest.json"))
                .unwrap()
                .set_len(32769)
                .unwrap(),
            _ => File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_len(32 * 1024 * 1024 + 1)
                .unwrap(),
        }
        assert!(b.verify().is_err(), "{case}");
    }
}

#[test]
fn held_sources_reject_mutation_replacement_and_extra_materials() {
    for case in ["mutation", "replacement", "extra", "signature", "manifest"] {
        let b = Bundle::new(Profile::Recovery);
        let verified = b.verify().unwrap();
        let path = b.path().join(TargetId::Launcher.payload_name());
        match case {
            "mutation" => {
                let mut raw = fs::read(&path).unwrap();
                raw[20] ^= 1;
                put(&path, &raw);
            }
            "replacement" => {
                let bytes = fs::read(&path).unwrap();
                let new = b.path().join("replacement");
                put(&new, &bytes);
                fs::rename(new, &path).unwrap();
            }
            "extra" => put(&b.path().join("extra"), b"not in signed file set"),
            "signature" => put(&b.path().join("manifest.sig"), &[0; 64]),
            _ => support::timestamps::mutate_until_ctime_changes(
                &b.path().join("manifest.json"),
                || b.sign(&serde_json::to_vec(&b.manifest).unwrap()),
            ),
        }
        assert!(verified.revalidate().is_err(), "{case}");
        assert!(verified.plan_metadata().is_err(), "{case}");
    }
}

#[test]
fn source_size_and_signature_input_are_bounded() {
    assert_eq!(product_signing_bytes(&[]).err(), Some(Error::Invalid));
    assert_eq!(
        product_signing_bytes(&vec![0; 32769]).err(),
        Some(Error::Invalid)
    );
    let b = Bundle::new(Profile::Legacy);
    put(&b.path().join("manifest.sig"), &[0; 65]);
    assert!(b.verify().is_err());
}

#[test]
fn unchanged_old_ap_parser_refuses_the_new_finite_package() {
    let b = Bundle::new(Profile::Legacy);
    let trust = tempfile::tempdir().unwrap();
    let public = trust.path().join("external.public");
    put(&public, &b.public());
    let script="import sys;sys.path.insert(0,sys.argv[1]);import setup_ap_package as old\ntry: old.verify_bundle(sys.argv[2],sys.argv[3],sys.argv[4])\nexcept (ValueError,AssertionError): pass\nelse: raise SystemExit('old AP parser accepted product materials')";
    let output = std::process::Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .args(["-c", script])
        .arg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools"))
        .arg(b.path())
        .arg(public)
        .arg(&b.bindings.source_archive_sha256)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn actual_offline_encoder_binds_git_archive_and_signed_application_then_rust_verifies_output() {
    use ed25519_dalek::Signer;
    let input = tempfile::tempdir().unwrap();
    let base = input.path();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let revision = String::from_utf8(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let archive = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["archive", "--format=tar", &revision])
        .output()
        .unwrap();
    assert!(archive.status.success());
    let source = product_materials::material_sha256(&archive.stdout);
    let mut template = Bundle::new(Profile::Legacy);
    template.bindings.source_commit = revision.clone();
    template.bindings.source_archive_sha256 = source.clone();
    let programs = base.join("build/programs");
    let materials = base.join("build/materials");
    let app = base.join("application");
    for p in [&programs, &materials, &app] {
        fs::create_dir_all(p).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut inventories = serde_json::Map::new();
    let mut app_entries = Vec::new();
    let app_names = rustsimhub_update::FORWARDING_FILES;
    for name in app_names {
        let raw = if name == "Opus-COPYING.txt" {
            b"synthetic license".to_vec()
        } else {
            support::elf()
        };
        put(&app.join(name), &raw);
        if name != "Opus-COPYING.txt" {
            put(&programs.join(name), &raw);
            inventories.insert(name.into(),serde_json::json!({"sha256":product_materials::material_sha256(&raw),"bytes":raw.len()}));
        }
        app_entries.push(serde_json::json!({"name":name,"bytes":raw.len(),"sha256":product_materials::material_sha256(&raw)}));
    }
    let application=serde_json::to_vec(&serde_json::json!({"format":2,"kind":"application","board":"ufi003-v02","arch":"aarch64-linux","epoch":1,"schema_min":6,"schema_max":6,"source_sha256":source,"development_fixtures":false,"files":app_entries})).unwrap();
    let app_key = SigningKey::from_bytes(&[91; 32]);
    put(&app.join("manifest.json"), &application);
    put(
        &app.join("manifest.sig"),
        &app_key
            .sign(&rustsimhub_update::signing_bytes(&application).unwrap())
            .to_bytes(),
    );
    put(
        &base.join("app.public"),
        &app_key.verifying_key().to_bytes(),
    );
    template.bindings.application_manifest_sha256 =
        product_materials::material_sha256(&application);
    template.bindings.core_sha256 = product_materials::material_sha256(&support::elf());
    let mut hashes = serde_json::Map::new();
    for (id, name) in [
        (TargetId::ProfileConfig, "product-profile.json"),
        (TargetId::CoreDropin, "50-product-profile.conf"),
        (TargetId::Launcher, "rustsimhub-core-launcher"),
    ] {
        let raw = support::payload(id, &template.bindings);
        if id.spec().executable {
            put(&programs.join(name), &raw);
            inventories.insert(name.into(),serde_json::json!({"sha256":product_materials::material_sha256(&raw),"bytes":raw.len()}));
        } else {
            put(&materials.join(name), &raw);
            hashes.insert(name.into(), product_materials::material_sha256(&raw).into());
        }
    }
    put(&base.join("build/profile-build.json"),&serde_json::to_vec(&serde_json::json!({"format":1,"kind":"unsigned-product-profile","profile":"legacy-v1","revision":revision,"source_archive_sha256":source,"target":"aarch64-unknown-linux-gnu","signed":false,"installable":false,"epoch_assigned":false,"programs":inventories,"materials":hashes})).unwrap());
    put(&base.join("roles.json"), support::FIXTURE_ROLE_POLICY);
    put(&base.join("product.seed"), &[93; 32]);
    let tool = repo.join("tools/product-install/package.py");
    let out = base.join("new-package");
    let invoke = |output: &std::path::Path| {
        std::process::Command::new("python3")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .arg(&tool)
            .arg("--build-root")
            .arg(base.join("build"))
            .arg("--application")
            .arg(&app)
            .arg("--app-public")
            .arg(base.join("app.public"))
            .arg("--roles-policy")
            .arg(base.join("roles.json"))
            .arg("--seed")
            .arg(base.join("product.seed"))
            .arg("--output")
            .arg(output)
            .output()
            .unwrap()
    };
    let result = invoke(&out);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let verified = VerifiedProductMaterials::open(
        &File::open(&out).unwrap(),
        &template.public(),
        &template.bindings,
    )
    .unwrap();
    verified.revalidate().unwrap();
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["authorization_verified"], false);
    assert_eq!(report["installed"], false);
    assert!(
        !invoke(&out).status.success(),
        "existing output must not be overwritten"
    );
    let mut bad: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("build/profile-build.json")).unwrap()).unwrap();
    bad["revision"] = "f".repeat(40).into();
    put(
        &base.join("build/profile-build.json"),
        &serde_json::to_vec(&bad).unwrap(),
    );
    let refused = base.join("refused-output");
    assert!(!invoke(&refused).status.success());
    assert!(!refused.exists());
}

#[test]
fn running_material_proof_requires_signature_and_rejects_changed_program_or_contract() {
    for id in [TargetId::NknSdk, TargetId::ProfileConfig] {
        let bundle = Bundle::new(Profile::RecoveryHealth);
        let key = bundle.key.verifying_key().to_bytes();
        assert!(matches!(
            VerifiedProductMaterials::open_running(&bundle.fd(), &[0; 32], &bundle.bindings),
            Err(Error::Signature)
        ));
        let verified =
            VerifiedProductMaterials::open_running(&bundle.fd(), &key, &bundle.bindings).unwrap();
        verified.revalidate_running().unwrap();
        let path = bundle.directory.path().join(id.payload_name());
        let before = fs::metadata(&path).unwrap();
        let mut bytes = fs::read(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        *bytes.last_mut().unwrap() ^= 1;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, bytes).unwrap();
        File::open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(before.modified().unwrap()))
            .unwrap();
        fs::set_permissions(&path, before.permissions()).unwrap();
        assert!(verified.revalidate_running().is_err(), "{id:?}");
        assert!(verified.revalidate().is_err(), "{id:?}");
        assert!(
            VerifiedProductMaterials::open_running(&bundle.fd(), &key, &bundle.bindings).is_err()
        );
    }
}
