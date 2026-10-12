// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use rustsimhub_domain::{CallLeases, LineEpoch, SmsOperation, SmsState};
use serde_json::json;

#[cfg(feature = "dev-fixtures")]
mod auth_check;
#[cfg(target_os = "linux")]
mod authenticated;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(feature = "dev-fixtures")]
mod storage_check;

fn self_check() -> Result<(), Box<dyn std::error::Error>> {
    let mut calls = CallLeases::new("self-check".into(), LineEpoch(1));
    let grant = calls
        .acquire("mock-client".into(), 0, false)
        .map_err(|_| "lease failure")?;
    calls
        .validate(&grant, "mock-client", 1)
        .map_err(|_| "validation failure")?;
    let mut sms = SmsOperation::prepare("mock-request".into(), LineEpoch(1));
    sms.begin_submit(LineEpoch(1))
        .map_err(|_| "SMS transition failure")?;
    sms.recover_uncertain();
    if sms.state() != SmsState::Unknown {
        return Err("SMS recovery failure".into());
    }
    println!(
        "{}",
        json!({"ok":true,"backend":"policy-self-check","protocol":1,"hardware_mutations":false})
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        // Never include raw D-Bus payloads or private request values in logs.
        eprintln!("rustsimhub-core: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--self-check"] {
        return self_check();
    }
    #[cfg(feature = "dev-fixtures")]
    if let [command, directory] = args.as_slice() {
        match command.as_str() {
            "--storage-check" => return storage_check::check(directory),
            "--storage-crash-fixture" => return storage_check::crash(directory),
            "--storage-recover-fixture" => return storage_check::recover(directory),
            "--auth-check" => return auth_check::check(directory),
            "--auth-reopen-fixture" => return auth_check::reopen(directory),
            _ => {}
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let [command, state, identity, key] = args.as_slice() {
            if command == "--provision-owner" {
                return authenticated::provision(state, identity, key);
            }
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match args.as_slice() {
            #[cfg(feature = "dev-fixtures")]
            [command, directory, socket] if command == "--auth-ipc-fixture" => {
                return runtime.block_on(auth_check::ipc(directory, socket));
            }
            [probe] if probe == "--probe" => return runtime.block_on(linux::probe_and_print()),
            [serve, path, uid] if serve == "--serve-readonly" => {
                return runtime.block_on(linux::serve(path, uid.parse()?));
            }
            [request, path, command] if request == "--request" => {
                return runtime.block_on(linux::request(path, command));
            }
            [command, path, uid, state, identity] if command == "--serve-authenticated" => {
                return runtime.block_on(authenticated::serve(path, uid.parse()?, state, identity));
            }
            [command, path] if command == "--request-auth" => {
                return runtime.block_on(authenticated::request(path));
            }
            _ => {}
        }
    }
    Err("usage: --self-check | --probe (Linux) | --serve-readonly SOCKET ALLOWED_UID | --request SOCKET health|capabilities | --serve-authenticated SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR | --request-auth SOCKET (JSON stdin) | --provision-owner STATE_DIR IDENTITY_DIR CLIENT_PUBLIC_KEY_HEX; development fixture commands require dev-fixtures".into())
}
