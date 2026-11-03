// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use std::{collections::HashSet, fs, os::unix::fs::MetadataExt};
fn roles() -> Result<u32, &'static str> {
    let m = fs::symlink_metadata("/etc/passwd").map_err(|_| "installation roles unavailable")?;
    if !m.is_file() || m.uid() != 0 || m.mode() & 0o022 != 0 || m.nlink() != 1 || m.len() > 65536 {
        return Err("untrusted installation roles");
    }
    let text = fs::read_to_string("/etc/passwd").map_err(|_| "installation roles unavailable")?;
    if text.len() > 65536 {
        return Err("roles limit");
    }
    let mut core = None;
    let mut ble = None;
    let mut ids = HashSet::new();
    let mut duplicates = HashSet::new();
    for line in text.lines() {
        let c = line.split(':').collect::<Vec<_>>();
        if c.len() != 7 {
            return Err("invalid roles");
        };
        let uid = c[2].parse::<u32>().map_err(|_| "invalid UID")?;
        if !ids.insert(uid) {
            duplicates.insert(uid);
        }
        if c[0] == "rustsimhub-core" && core.replace(uid).is_some() {
            return Err("duplicate role");
        }
        if c[0] == "rustsimhub-ble" && ble.replace(uid).is_some() {
            return Err("duplicate role");
        }
    }
    let (core, ble) = (core.ok_or("missing Core")?, ble.ok_or("missing BLE")?);
    if core == 0
        || core == u32::MAX
        || core == ble
        || duplicates.contains(&core)
        || duplicates.contains(&ble)
        || rustix::process::geteuid().as_raw() != ble
    {
        return Err("wrong installed identity");
    }
    Ok(core)
}
fn main() {
    if run().is_err() {
        eprintln!("explicit BLE recovery candidate stopped; cleanup may require inspection");
        std::process::exit(1)
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--serve-fixed-candidate"] {
        return Err("usage: --serve-fixed-candidate".into());
    }
    let core = roles()?;
    let mut domain = [0; 16];
    getrandom::fill(&mut domain)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let connection = zbus::Connection::system().await?;
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let result = rustsimhub_ble::product_host::serve(connection, core, domain, async {
            tokio::select! {_=term.recv()=>{},_=interrupt.recv()=>{}}
        })
        .await;
        result.map_err(|_| "BLE recovery epoch ended".into())
    })
}
