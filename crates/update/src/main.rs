// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
fn check_maintenance_versions(
    paths: &[String],
    schema: u32,
) -> Result<i32, rustsimhub_update::Error> {
    use rustsimhub_update::{
        admission::{MaintenanceSnapshot, SignedPrograms},
        store::{load_payloads, read_checked},
        verify, Error, MAX_MANIFEST,
    };
    use std::path::Path;
    let key: [u8; 32] = read_checked(Path::new(&paths[0]), 32)?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Invalid)?;
    let snapshot = MaintenanceSnapshot::observe()?;
    let inspect = |directory: &str| {
        let directory = Path::new(directory);
        let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
        let signature = read_checked(&directory.join("manifest.sig"), 64)?;
        let verified = verify(&manifest, &signature, &key, schema, 0)?;
        SignedPrograms::inspect(
            &manifest,
            &signature,
            &key,
            &load_payloads(directory, &verified)?,
            schema,
        )
    };
    let selected = inspect(&paths[1])?;
    let candidate = inspect(&paths[2])?;
    let fallback = inspect(&paths[3])?;
    let check = snapshot.compare_signed(&selected, &candidate, &fallback)?;
    check.revalidate()?;
    println!(
        "{}",
        serde_json::to_string(&check.report()).map_err(|_| Error::Invalid)?
    );
    Ok(check.exit_code())
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), rustsimhub_update::Error> {
    const SCHEMA: u32 = 6;
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_update::{
        application_files, manifest_hash, signing_bytes,
        store::{load_payloads, read_checked, write_new, Store},
        verify, verify_payload, Entry, Error, Manifest, BOARD, MAX_MANIFEST, MAX_PAYLOAD,
    };
    use std::{fs, path::Path};
    use zeroize::Zeroizing;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"system_ota_supported":false,"boot_runtime_selection_supported":true,"forwarding_application_format_supported":true,"signed_application_formats":[1,2],"database_schema":SCHEMA,"maintenance_observation_protocol":1,"read_only_state_inspection_protocol":1,"read_only_runtime_inspection_protocol":1,"running_candidate_confirmation_protocol":1,"running_candidate_confirmation_default_enabled":false})
        );
        return Ok(());
    }
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    if args
        .first()
        .is_some_and(|command| command.starts_with("product-"))
    {
        let report = rustsimhub_update::admission::product::dispatch(&args)?;
        println!(
            "{}",
            serde_json::to_string(&report).map_err(|_| Error::Invalid)?
        );
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|command| command == "confirm-running-candidate")
    {
        if args.len() != 1 {
            return Err(Error::Invalid);
        }
        let confirmation =
            rustsimhub_update::runtime_observation::confirmation::confirm_running_candidate()?;
        println!(
            "{}",
            serde_json::to_string(&confirmation).map_err(|_| Error::Invalid)?
        );
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|command| command == "inspect-runtime")
    {
        // This exact command never reaches the writable Store dispatch, even
        // when additional arguments happen to be valid Store/trust paths.
        if args.len() != 1 {
            return Err(Error::Invalid);
        }
        let observation = rustsimhub_update::runtime_observation::RuntimeObservation::observe()?;
        let report = observation.report()?;
        let output = serde_json::to_vec(&report).map_err(|_| Error::Invalid)?;
        if output.len() >= 4096 {
            return Err(Error::Invalid);
        }
        // Retain the live guard until the entire bounded report is emitted.
        println!(
            "{}",
            std::str::from_utf8(&output).map_err(|_| Error::Invalid)?
        );
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|command| command == "inspect-state")
    {
        // Reject extra arguments BEFORE the generic writable Store dispatch.
        // There is no caller-supplied root/trust path or recovery operation.
        if args.len() != 1 {
            return Err(Error::Invalid);
        }
        let observation = rustsimhub_update::store::inspection::ReadOnlyState::observe()?;
        println!("{}", observation.report()?);
        return Ok(());
    }
    if args
        .first()
        .is_some_and(|command| command == "admit-web-upload")
    {
        let [_, request_id, manifest_sha256] = args.as_slice() else {
            return Err(Error::Invalid);
        };
        let admission =
            rustsimhub_update::admission::admit_web_upload(request_id, manifest_sha256)?;
        println!(
            "{}",
            serde_json::json!({"request_id":request_id,"admission":admission.report})
        );
        if admission.exit_code != 0 {
            std::process::exit(admission.exit_code);
        }
        return Ok(());
    }
    if args == ["inspect-maintenance"] {
        let snapshot = rustsimhub_update::admission::MaintenanceSnapshot::observe()?;
        snapshot.revalidate()?;
        println!(
            "{}",
            serde_json::to_string(&snapshot.report()).map_err(|_| Error::Invalid)?
        );
        return Ok(());
    }
    if args.len() == 5 && args[0] == "check-maintenance-versions" {
        // All guards in this finite diagnostic are dropped before exit. The
        // provided trust/roles are not promoted to installed slot identities.
        let code = check_maintenance_versions(&args[1..], SCHEMA)?;
        std::process::exit(code);
    }
    if let [command, public, directory, file] = args.as_slice() {
        if command == "pack-application" {
            let key: [u8; 32] = read_checked(Path::new(public), 32)?
                .as_slice()
                .try_into()
                .map_err(|_| Error::Invalid)?;
            let directory = Path::new(directory);
            let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
            let signature = read_checked(&directory.join("manifest.sig"), 64)?;
            let verified = verify(&manifest, &signature, &key, SCHEMA, 0)?;
            let payloads = load_payloads(directory, &verified)?;
            let bytes =
                rustsimhub_update::bundle::pack(&manifest, &signature, &payloads, &key, SCHEMA)?;
            write_new(Path::new(file), &bytes, false)?;
            return Ok(());
        }
    }
    if let [command, public, directory] = args.as_slice() {
        if command == "verify-app-file" {
            let key: [u8; 32] = read_checked(Path::new(public), 32)?
                .as_slice()
                .try_into()
                .map_err(|_| Error::Invalid)?;
            let bytes = read_checked(Path::new(directory), rustsimhub_update::bundle::MAX_BUNDLE)?;
            let result = rustsimhub_update::bundle::inspect(&bytes, &key, SCHEMA, 0)?;
            println!(
                "{}",
                serde_json::to_string(&result).map_err(|_| Error::Invalid)?
            );
            return Ok(());
        }
        if command == "verify-package" {
            let key: [u8; 32] = read_checked(Path::new(public), 32)?
                .as_slice()
                .try_into()
                .map_err(|_| Error::Invalid)?;
            let directory = Path::new(directory);
            let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
            let signature = read_checked(&directory.join("manifest.sig"), 64)?;
            let verified = verify(&manifest, &signature, &key, SCHEMA, 0)?;
            verify_payload(&verified, &load_payloads(directory, &verified)?)?;
            println!(
                "{}",
                serde_json::json!({"verified":true,"manifest_sha256":manifest_hash(&manifest),"source_sha256":verified.source_sha256,"epoch":verified.epoch})
            );
            return Ok(());
        }
    }
    if let [command, directory] = args.as_slice() {
        if command == "keygen" {
            let directory = Path::new(directory);
            let mut seed = Zeroizing::new([0; 32]);
            getrandom::fill(seed.as_mut()).map_err(|_| Error::Io)?;
            let key = SigningKey::from_bytes(&seed);
            write_new(&directory.join("release.seed"), seed.as_ref(), false)?;
            write_new(
                &directory.join("release.public"),
                &key.verifying_key().to_bytes(),
                false,
            )?;
            println!("release signing key created in private directory");
            return Ok(());
        }
    }
    if let [command, key, source, epoch, input, output] = args.as_slice() {
        if command == "make"
            || command == "make-forwarding"
            || matches!(
                command.as_str(),
                "make-product-recovery" | "make-product-email-recovery"
            )
        {
            let format = if command == "make" { 1 } else { 2 };
            let names = application_files(format)?;
            let seed = Zeroizing::new(read_checked(Path::new(key), 32)?);
            let seed: &[u8; 32] = seed.as_slice().try_into().map_err(|_| Error::Invalid)?;
            let signing = SigningKey::from_bytes(seed);
            let input = Path::new(input);
            let output = Path::new(output);
            let mut payloads = Vec::new();
            for name in names {
                payloads.push(read_checked(&input.join(name), MAX_PAYLOAD as usize)?);
            }
            let m = Manifest {
                format,
                kind: "application".into(),
                board: BOARD.into(),
                arch: "aarch64-linux".into(),
                epoch: epoch.parse().map_err(|_| Error::Invalid)?,
                // Schema 6 is a maintenance transition: v5 updaters must refuse it.
                schema_min: SCHEMA,
                schema_max: if matches!(
                    command.as_str(),
                    "make-product-recovery" | "make-product-email-recovery"
                ) {
                    9
                } else {
                    SCHEMA
                },
                source_sha256: source.clone(),
                development_fixtures: false,
                files: names
                    .iter()
                    .zip(&payloads)
                    .map(|(name, b)| Entry {
                        name: (*name).into(),
                        bytes: b.len() as u64,
                        sha256: manifest_hash(b),
                    })
                    .collect(),
            };
            let bytes = serde_json::to_vec(&m).map_err(|_| Error::Invalid)?;
            let sig = signing.sign(&signing_bytes(&bytes)?);
            let m = verify(
                &bytes,
                &sig.to_bytes(),
                &signing.verifying_key().to_bytes(),
                SCHEMA,
                0,
            )?;
            verify_payload(&m, &payloads)?;
            if matches!(
                command.as_str(),
                "make-product-recovery" | "make-product-email-recovery"
            ) {
                // Offline signer only; the future installer still validates
                // installed trust, fallback and actual data. Reject a Core
                // that cannot implement this exact finite profile BEFORE IO.
                rustsimhub_update::product_admission::SignedProductPrograms::inspect(
                    &bytes,
                    &sig.to_bytes(),
                    &signing.verifying_key().to_bytes(),
                    &payloads,
                    SCHEMA,
                    if command == "make-product-email-recovery" {
                        rustsimhub_update::product_admission::ProductRuntimeModeV1::EmailRecovery
                    } else {
                        rustsimhub_update::product_admission::ProductRuntimeModeV1::RecoveryHealth
                    },
                )?;
            }
            write_new(&output.join("manifest.json"), &bytes, false)?;
            write_new(&output.join("manifest.sig"), &sig.to_bytes(), false)?;
            for (name, payload) in names.iter().zip(payloads) {
                write_new(&output.join(name), &payload, false)?;
            }
            fs::File::open(output)
                .and_then(|f| f.sync_all())
                .map_err(|_| Error::Io)?;
            println!(
                "{}",
                serde_json::json!({"signed":true,"manifest_sha256":manifest_hash(&bytes),"epoch":m.epoch})
            );
            return Ok(());
        }
    }
    if args.len() >= 3 {
        let command = &args[0];
        // Store::open can recover state.next or create a lock/versions.
        // A typo or wrong arity must be rejected before those side effects.
        if !matches!(
            (command.as_str(), args.len()),
            ("boot-select", 6)
                | (
                    "install-factory"
                        | "stage"
                        | "admit-stage"
                        | "admit-stage-file"
                        | "migrate-email4"
                        | "confirm",
                    4
                )
                | ("choose" | "status", 3)
        ) {
            return Err(Error::Invalid);
        }
        if command == "migrate-email4" && !cfg!(feature = "email-production-candidate") {
            return Err(Error::Invalid);
        }
        let root = Path::new(&args[1]);
        let key: [u8; 32] = read_checked(Path::new(&args[2]), 32)?
            .as_slice()
            .try_into()
            .map_err(|_| Error::Invalid)?;
        // Reject malformed, untrusted inputs before opening a writable ledger.
        let packed = if command == "admit-stage-file" {
            Some(read_checked(
                Path::new(&args[3]),
                rustsimhub_update::bundle::MAX_BUNDLE,
            )?)
        } else {
            None
        };
        let signed_file = packed
            .as_deref()
            .map(|bytes| rustsimhub_update::bundle::verify_for_admission(bytes, &key, SCHEMA, 0))
            .transpose()?;
        let mut store = Store::open(root, key, SCHEMA)?;
        match (command.as_str(), args.len()) {
            ("boot-select", 6) => {
                store.check_runtime_paths(Path::new(&args[4]), Path::new(&args[5]))?;
                let directory = Path::new(&args[3]);
                let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
                let signature = read_checked(&directory.join("manifest.sig"), 64)?;
                let m = verify(&manifest, &signature, &key, SCHEMA, 0)?;
                let payloads = load_payloads(directory, &m)?;
                store.install_factory(&manifest, &signature, &payloads)?;
                let selected = store.select_runtime(Path::new(&args[4]), Path::new(&args[5]))?;
                println!("{}", selected.display());
                return Ok(());
            }
            ("install-factory", 4) => {
                let directory = Path::new(&args[3]);
                let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
                let signature = read_checked(&directory.join("manifest.sig"), 64)?;
                let m = verify(&manifest, &signature, &key, SCHEMA, 0)?;
                let payloads = load_payloads(directory, &m)?;
                let path = store.install_factory(&manifest, &signature, &payloads)?;
                println!("{}", path.display());
                return Ok(());
            }
            ("stage", 4) => {
                let directory = Path::new(&args[3]);
                let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
                let signature = read_checked(&directory.join("manifest.sig"), 64)?;
                let m = verify(&manifest, &signature, &key, SCHEMA, store.floor())?;
                let p = load_payloads(directory, &m)?;
                let _indicator = rustsimhub_update::indicator::Activity::begin();
                let hash = store.stage(&manifest, &signature, &p)?;
                println!(
                    "{}",
                    serde_json::json!({"staged":true,"manifest_sha256":hash})
                );
                return Ok(());
            }
            #[cfg(feature = "email-production-candidate")]
            ("migrate-email4", 4) => {
                let directory = Path::new(&args[3]);
                let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
                let signature = read_checked(&directory.join("manifest.sig"), 64)?;
                let m = verify(&manifest, &signature, &key, SCHEMA, store.floor())?;
                let payloads = load_payloads(directory, &m)?;
                let snapshot = rustsimhub_update::admission::MaintenanceSnapshot::observe()?;
                rustsimhub_update::admission::migrate_email4(
                    &mut store, snapshot, &manifest, &signature, &payloads,
                )?;
                println!("explicit outbox migration completed; worker remains stopped");
                return Ok(());
            }
            ("admit-stage" | "admit-stage-file", 4) => {
                // Opt-in maintenance path; `stage` is unchanged. Core must be
                // stopped: the snapshot takes the data writer fences.
                let input = Path::new(&args[3]);
                let (manifest, signature, payloads) = if let Some(signed) = signed_file {
                    if signed.epoch() <= store.floor() {
                        return Err(Error::Rollback);
                    }
                    (
                        signed.manifest.to_vec(),
                        signed.signature.to_vec(),
                        signed.payloads.into_iter().map(<[u8]>::to_vec).collect(),
                    )
                } else {
                    let manifest = read_checked(&input.join("manifest.json"), MAX_MANIFEST)?;
                    let signature = read_checked(&input.join("manifest.sig"), 64)?;
                    let m = verify(&manifest, &signature, &key, SCHEMA, store.floor())?;
                    let payloads = load_payloads(input, &m)?;
                    (manifest, signature, payloads)
                };
                drop(packed);
                let code = {
                    // Release fences and the indicator before a classified exit.
                    let snapshot = rustsimhub_update::admission::MaintenanceSnapshot::observe()?;
                    let _indicator = rustsimhub_update::indicator::Activity::begin();
                    let admission = rustsimhub_update::admission::admit_stage(
                        &mut store, &snapshot, &manifest, &signature, &payloads,
                    )?;
                    println!("{}", admission.report);
                    admission.exit_code
                };
                if code != 0 {
                    std::process::exit(code);
                }
                return Ok(());
            }
            ("choose", 3) => {
                let _indicator = rustsimhub_update::indicator::Activity::begin();
                let path = store.choose()?;
                println!("{}", path.display());
                return Ok(());
            }
            ("confirm", 4) => {
                store.confirm(&args[3])?;
                println!("application health confirmation recorded");
                return Ok(());
            }
            ("status", 3) => {
                println!("{}", store.status());
                return Ok(());
            }
            _ => {}
        }
    }
    Err(Error::Invalid)
}
fn main() {
    #[cfg(target_os = "linux")]
    if let Err(reason) = run() {
        eprintln!("application update denied: {reason:?}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("application selector requires Linux");
        std::process::exit(2);
    }
}
