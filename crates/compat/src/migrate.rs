// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
// Keep the subprocess fixture in this separate test executable. Spawning Go
// concurrently with the library's history/SQLite lock fixtures can retain open
// descriptors in a child while those fixtures are checking immediate unlocks.
#[cfg(all(test, target_os = "linux"))]
mod nas_adapter_tests;
#[cfg(all(test, target_os = "linux"))]
mod preflight_cli_tests;
#[cfg(all(test, target_os = "linux"))]
static OFFLINE_CLI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn run() -> Result<(), Box<dyn std::error::Error>> {
    run_cli(std::env::args().skip(1).collect(), &mut std::io::stdout())
}
fn run_cli(
    args: Vec<String>,
    output: &mut dyn std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    if args == ["--build-info"] {
        writeln!(
            output,
            "{}",
            serde_json::json!({"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"offline_history_only":true,"hardware_operations":false,"credentials_imported":false})
        )?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use rustsimhub_compat::history;
        use std::path::Path;
        let report = match args.as_slice() {
            [op, snapshot, source] if op == "preflight" => return preflight_output(history::preflight::preflight(Path::new(snapshot), source, None), output),
            [op, snapshot, source, destination] if op == "preflight" => return preflight_output(history::preflight::preflight(Path::new(snapshot), source, Some(Path::new(destination))), output),
            [op, directory] if op=="init" => history::initialize(Path::new(directory))?,
            [op, snapshot, source] if op == "validate" => history::validate(Path::new(snapshot),source)?,
            [op, snapshot, source, destination] if op == "import" => history::import(Path::new(snapshot),source,Path::new(destination))?,
            [op, directory] if op == "inspect" => history::inspect(Path::new(directory))?,
            _ => return Err("usage: init PRIVATE_EMPTY_ARCHIVE_DIR | preflight PRIVATE_OFFLINE_SNAPSHOT SOURCE_ID [PRIVATE_ARCHIVE_DIR] | validate PRIVATE_OFFLINE_SNAPSHOT SOURCE_ID | import PRIVATE_OFFLINE_SNAPSHOT SOURCE_ID PRIVATE_ARCHIVE_DIR | inspect PRIVATE_ARCHIVE_DIR; history only, no credentials or hardware replay".into()),
        };
        writeln!(output, "{}", serde_json::to_string(&report)?)?;
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    Err("offline history tool requires Linux private filesystem checks".into())
}
#[cfg(target_os = "linux")]
fn preflight_output(
    report: rustsimhub_compat::history::preflight::PreflightReport,
    output: &mut dyn std::io::Write,
) -> Result<(), Box<dyn std::error::Error>> {
    writeln!(output, "{}", serde_json::to_string(&report)?)?;
    if report.accepted {
        Ok(())
    } else {
        Err("history preflight rejected".into())
    }
}
fn main() {
    if run().is_err() {
        // Never print SQLite/path/row errors: old phone numbers and text are private.
        eprintln!("history migration denied: verify offline snapshot, supported schema, private ownership and stable source namespace");
        std::process::exit(1);
    }
}
