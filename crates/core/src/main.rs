// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use rustsimhub_domain::{CallLeases, LineEpoch, SmsOperation, SmsState};
use serde_json::json;

#[cfg(target_os = "linux")]
mod linux;

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
    #[cfg(target_os = "linux")]
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match args.as_slice() {
            [probe] if probe == "--probe" => return runtime.block_on(linux::probe_and_print()),
            [serve, path, uid] if serve == "--serve-readonly" => {
                return runtime.block_on(linux::serve(path, uid.parse()?));
            }
            [request, path, command] if request == "--request" => {
                return runtime.block_on(linux::request(path, command));
            }
            _ => {}
        }
    }
    Err("usage: --self-check | --probe (Linux) | --serve-readonly SOCKET ALLOWED_UID | --request SOCKET health|capabilities".into())
}
