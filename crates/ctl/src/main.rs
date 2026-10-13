// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use rustsimhub_auth::{Challenge, ClientKey, ProofExpectation};
use serde_json::json;
use std::{io::Read, path::Path};
#[cfg(target_os = "linux")]
mod linux;
mod session;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn public_hex(key: [u8; 32]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
fn parse_key(hex: &str) -> Result<[u8; 32]> {
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid public key".into());
    }
    let mut key = [0; 32];
    for (i, b) in key.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| "invalid public key")?;
    }
    Ok(key)
}
fn private_directory(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)?;
        }
        Err(_) => return Err("private directory unavailable".into()),
    }
    Ok(()) // Key/session I/O enforces directory ownership, mode and non-symlink itself.
}
fn main() {
    if run().is_err() {
        eprintln!("simhubctl: operation failed; check permissions, pinned key, proof, session and daemon state");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] {
        println!("keygen PRIVATE_KEY_DIR | sign PRIVATE_KEY_DIR DEVICE_PUBLIC_KEY_HEX bootstrap|recover_owner|login (challenge JSON stdin)\nlogin SOCKET PRIVATE_KEY_DIR DEVICE_PUBLIC_KEY_HEX NEW_SESSION_FILE\nstatus|capabilities|line|inbox|inbox-events|open-pairing|pending|clients SOCKET SESSION_FILE\nsms SOCKET SESSION_FILE SMS_ID\npair SOCKET PRIVATE_KEY_DIR DEVICE_PUBLIC_KEY_HEX client|nas COMMA_SCOPES\napprove SOCKET OWNER_SESSION PAIRING_ID CLIENT_PUBLIC_KEY_HEX client|nas COMMA_SCOPES\nrevoke SOCKET OWNER_SESSION CLIENT_PUBLIC_KEY_HEX\nprepare-sms SOCKET SESSION_FILE PRIVATE_DRAFT_JSON\nsubmit-sms|sms-operation SOCKET SESSION_FILE OPERATION_ID\ncalls|call-events|call-status SOCKET SESSION_FILE\ncall|call-operation SOCKET SESSION_FILE ID\nprepare-call SOCKET SESSION_FILE PRIVATE_DRAFT_JSON\nsubmit-call SOCKET SESSION_FILE OPERATION_ID LEASE_FILE|none normal|hangup\nrenew-call SOCKET SESSION_FILE PRIVATE_LEASE_FILE\nsave-call-lease PRIVATE_OPERATION_JSON NEW_PRIVATE_LEASE_FILE\nLocal Unix sockets only. Calling requires calls_control and --serve-telephony. Sending requires sms_send and --serve-messaging. Unknown is never retried automatically.");
        return Ok(());
    }
    if let [command, directory] = args.as_slice() {
        if command == "keygen" {
            private_directory(Path::new(directory))?;
            let key = ClientKey::create(directory)?;
            println!(
                "{}",
                json!({"public_key":key.public_key(),"public_key_hex":public_hex(key.public_key())})
            );
            return Ok(());
        }
    }
    if let [command, directory, pin, purpose] = args.as_slice() {
        if command == "sign" {
            let expectation = match purpose.as_str() {
                "bootstrap" => ProofExpectation::Bootstrap,
                "recover_owner" => ProofExpectation::RecoverOwner,
                "login" => ProofExpectation::Login,
                _ => return Err("invalid purpose".into()),
            };
            let mut bytes = Vec::new();
            std::io::stdin().take(65537).read_to_end(&mut bytes)?;
            if bytes.len() > 65536 {
                return Err("challenge too large".into());
            }
            let challenge: Challenge =
                serde_json::from_slice(&bytes).map_err(|_| "invalid challenge")?;
            let proof =
                ClientKey::load(directory)?.prove(&challenge, parse_key(pin)?, expectation)?;
            println!("{}", serde_json::to_string(&proof)?);
            return Ok(());
        }
    }
    #[cfg(target_os = "linux")]
    return tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(linux::run(&args));
    #[cfg(not(target_os = "linux"))]
    Err("use --help; socket commands require Linux".into())
}
