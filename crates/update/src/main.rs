// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
fn run() -> Result<(), rustsimhub_update::Error> {
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_update::{
        manifest_hash, signing_bytes,
        store::{load_payloads, read_checked, write_new, Store},
        verify, verify_payload, Entry, Error, Manifest, BOARD, FILES, MAX_MANIFEST, MAX_PAYLOAD,
    };
    use std::{fs, path::Path};
    use zeroize::Zeroizing;
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"system_ota_supported":false,"boot_runtime_selection_supported":true})
        );
        return Ok(());
    }
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    if let [command, public, directory] = args.as_slice() {
        if command == "verify-package" {
            let key: [u8; 32] = read_checked(Path::new(public), 32)?
                .as_slice()
                .try_into()
                .map_err(|_| Error::Invalid)?;
            let directory = Path::new(directory);
            let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
            let signature = read_checked(&directory.join("manifest.sig"), 64)?;
            let verified = verify(&manifest, &signature, &key, 5, 0)?;
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
        if command == "make" {
            let seed = Zeroizing::new(read_checked(Path::new(key), 32)?);
            let seed: &[u8; 32] = seed.as_slice().try_into().map_err(|_| Error::Invalid)?;
            let signing = SigningKey::from_bytes(seed);
            let input = Path::new(input);
            let output = Path::new(output);
            let mut payloads = Vec::new();
            for name in FILES {
                payloads.push(read_checked(&input.join(name), MAX_PAYLOAD as usize)?);
            }
            let m = Manifest {
                format: 1,
                kind: "application".into(),
                board: BOARD.into(),
                arch: "aarch64-linux".into(),
                epoch: epoch.parse().map_err(|_| Error::Invalid)?,
                schema_min: 5,
                schema_max: 5,
                source_sha256: source.clone(),
                development_fixtures: false,
                files: FILES
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
                5,
                0,
            )?;
            verify_payload(&m, &payloads)?;
            write_new(&output.join("manifest.json"), &bytes, false)?;
            write_new(&output.join("manifest.sig"), &sig.to_bytes(), false)?;
            for (name, payload) in FILES.into_iter().zip(payloads) {
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
        let root = Path::new(&args[1]);
        let key: [u8; 32] = read_checked(Path::new(&args[2]), 32)?
            .as_slice()
            .try_into()
            .map_err(|_| Error::Invalid)?;
        let mut store = Store::open(root, key, 5)?;
        match (command.as_str(), args.len()) {
            ("boot-select", 6) => {
                store.check_runtime_paths(Path::new(&args[4]), Path::new(&args[5]))?;
                let directory = Path::new(&args[3]);
                let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
                let signature = read_checked(&directory.join("manifest.sig"), 64)?;
                let m = verify(&manifest, &signature, &key, 5, 0)?;
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
                let m = verify(&manifest, &signature, &key, 5, 0)?;
                let payloads = load_payloads(directory, &m)?;
                let path = store.install_factory(&manifest, &signature, &payloads)?;
                println!("{}", path.display());
                return Ok(());
            }
            ("stage", 4) => {
                let directory = Path::new(&args[3]);
                let manifest = read_checked(&directory.join("manifest.json"), MAX_MANIFEST)?;
                let signature = read_checked(&directory.join("manifest.sig"), 64)?;
                let m = verify(&manifest, &signature, &key, 5, store.floor())?;
                let p = load_payloads(directory, &m)?;
                let _indicator = rustsimhub_update::indicator::Activity::begin();
                let hash = store.stage(&manifest, &signature, &p)?;
                println!(
                    "{}",
                    serde_json::json!({"staged":true,"manifest_sha256":hash})
                );
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
