// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use std::path::Path;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"offline_recovery":true})
        );
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    match args.as_slice() {
        [command, directory] if command == "keygen" => {
            rustsimhub_recovery::keygen(Path::new(directory))?;
            println!(
                "{}",
                serde_json::json!({"ok":true,"recovery_key_created":true})
            );
            return Ok(());
        }
        [command, identity, recipient, output] if command == "backup" => {
            rustsimhub_recovery::backup(
                Path::new(identity),
                Path::new(recipient),
                Path::new(output),
            )?;
            println!(
                "{}",
                serde_json::json!({"ok":true,"encrypted_identity_backup":true})
            );
            return Ok(());
        }
        [command, key, backup, pin, destination] if command == "restore" => {
            if pin.len() != 64 || !pin.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid identity pin".into());
            }
            let mut expected = [0u8; 32];
            for (i, b) in expected.iter_mut().enumerate() {
                *b = u8::from_str_radix(&pin[i * 2..i * 2 + 2], 16)?;
            }
            rustsimhub_recovery::restore(
                Path::new(key),
                Path::new(backup),
                expected,
                Path::new(destination),
            )?;
            println!(
                "{}",
                serde_json::json!({"ok":true,"encrypted_identity_restored":true})
            );
            return Ok(());
        }
        _ => {}
    }
    Err("offline Linux recovery tool: keygen PRIVATE_DIR | backup IDENTITY_DIR RECIPIENT_FILE NEW_AGE_FILE | restore PRIVATE_KEY AGE_FILE EXPECTED_DEVICE_PIN EMPTY_PRIVATE_DIR".into())
}
fn main() {
    if run().is_err() {
        eprintln!("recovery denied: check private ownership, recipient, expected device pin and empty destination");
        std::process::exit(1);
    }
}
