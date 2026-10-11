// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_domain::{LineEpoch, SmsState};
use rustsimhub_storage::{Cursor, SmsRequest, Store};
use serde_json::json;
use std::{error::Error, fs, io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn fixture() -> SmsRequest {
    SmsRequest {
        client_id: "development-fixture".into(),
        idempotency_key: "fixture-only".into(),
        line: LineEpoch(1),
        recipient: "+12025550123".into(),
        text: "synthetic fixture; no modem send".into(),
    }
}
fn require_fresh(directory: &Path) -> Result<()> {
    if directory.join("store.sqlite3").exists() {
        return Err("storage fixture requires a fresh directory".into());
    }
    Ok(())
}
fn mkdir(directory: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory)?;
    Ok(())
}
pub fn check(directory: &str) -> Result<()> {
    let directory = Path::new(directory);
    require_fresh(directory)?;
    let started = std::time::Instant::now();
    let store = Store::open(directory)?;
    let handle = store.handle();
    let op = handle.prepare_sms(fixture())?;
    if handle.prepare_sms(fixture())? != op {
        return Err("idempotency fixture failed".into());
    }
    handle.begin_submit(&op.id, "development-fixture")?;
    let cursor = handle.cursor()?;
    let backup = directory.join("backup");
    mkdir(&backup)?;
    handle.backup(&backup)?;
    drop(store);
    let restarted = Store::open(directory)?;
    let recovered = restarted
        .handle()
        .operation(&op.id, "development-fixture")?;
    if recovered.state != SmsState::Unknown {
        return Err("recovery fixture failed".into());
    }
    if restarted
        .handle()
        .begin_submit(&op.id, "development-fixture")
        .is_ok()
    {
        return Err("uncertain fixture became retryable".into());
    }
    let restored_dir = directory.join("restored");
    mkdir(&restored_dir)?;
    let restored = Store::restore(&backup, &restored_dir)?;
    if restored.handle().cursor()?.db_epoch == cursor.db_epoch {
        return Err("restore epoch fixture failed".into());
    }
    let mut after: Cursor = restarted.handle().cursor()?;
    after.seq = "0".into();
    let events = restarted.handle().events(after, 256)?.events.len();
    println!(
        "{}",
        json!({"ok":true,"backend":"sqlite-fixture","hardware_mutations":false,"events":events,"recovery":"unknown","restore_epoch_rotated":true,"elapsed_ms":started.elapsed().as_millis()})
    );
    Ok(())
}
pub fn crash(directory: &str) -> Result<()> {
    require_fresh(Path::new(directory))?;
    let store = Store::open(directory)?;
    let handle = store.handle();
    let op = handle.prepare_sms(fixture())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut marker = options.open(Path::new(directory).join(".fixture-operation"))?;
    marker.write_all(op.id.as_bytes())?;
    marker.sync_all()?;
    handle.begin_submit(&op.id, "development-fixture")?;
    println!(
        "{}",
        json!({"fixture":true,"committed_submitting":true,"hardware_mutations":false})
    );
    std::io::stdout().flush()?;
    std::process::exit(73)
}
pub fn recover(directory: &str) -> Result<()> {
    if !Path::new(directory).join("store.sqlite3").is_file() {
        return Err("crash fixture database missing".into());
    }
    let id = fs::read_to_string(Path::new(directory).join(".fixture-operation"))?;
    let store = Store::open(directory)?;
    let handle = store.handle();
    let op = handle.operation(&id, "development-fixture")?;
    if op.state != SmsState::Unknown || handle.begin_submit(&op.id, "development-fixture").is_ok() {
        return Err("crash recovery fixture failed".into());
    }
    println!(
        "{}",
        json!({"ok":true,"backend":"sqlite-crash-fixture","recovery":"unknown","automatic_retry":false,"hardware_mutations":false})
    );
    Ok(())
}
