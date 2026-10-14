// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"offline_history_only":true,"hardware_operations":false,"credentials_imported":false})
        );
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use rustsimhub_compat::history;
        use std::path::Path;
        let report = match args.as_slice() {
            [op, snapshot, source] if op == "validate" => history::validate(Path::new(snapshot),source)?,
            [op, snapshot, source, destination] if op == "import" => history::import(Path::new(snapshot),source,Path::new(destination))?,
            [op, directory] if op == "inspect" => history::inspect(Path::new(directory))?,
            _ => return Err("usage: validate PRIVATE_OFFLINE_SNAPSHOT SOURCE_ID | import PRIVATE_OFFLINE_SNAPSHOT SOURCE_ID PRIVATE_ARCHIVE_DIR | inspect PRIVATE_ARCHIVE_DIR; history only, no credentials or hardware replay".into()),
        };
        println!("{}", serde_json::to_string(&report)?);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err("offline history tool requires Linux private filesystem checks".into())
}
fn main() {
    if run().is_err() {
        // Never print SQLite/path/row errors: old phone numbers and text are private.
        eprintln!("history migration denied: verify offline snapshot, supported schema, private ownership and stable source namespace");
        std::process::exit(1);
    }
}
