// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use rustsimhub_domain::{CallLeases, LineEpoch, SmsOperation, SmsState};
use serde_json::json;

#[cfg(feature = "dev-fixtures")]
mod auth_check;
#[cfg(target_os = "linux")]
mod authenticated;
#[cfg(target_os = "linux")]
mod calls;
#[cfg(target_os = "linux")]
mod history_api;
#[cfg(feature = "dev-fixtures")]
mod inbox_check;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod media_ipc;
#[cfg(target_os = "linux")]
mod network;
#[cfg(target_os = "linux")]
mod network_trial;
#[cfg(target_os = "linux")]
mod observer;
#[cfg(feature = "dev-fixtures")]
mod recovery_check;
#[cfg(target_os = "linux")]
mod sender;
#[cfg(target_os = "linux")]
mod socket_runtime;
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
    if args == ["--build-info"] {
        println!(
            "{}",
            json!({"source_archive_sha256":rustsimhub_wire::build_source_sha256(),"development_fixtures":cfg!(feature="dev-fixtures")})
        );
        return Ok(());
    }
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
            "--recovery-prepare-fixture" => return recovery_check::prepare(directory),
            "--recovery-verify-fixture" => return recovery_check::verify(directory),
            "--inbox-prepare-fixture" => return inbox_check::prepare(directory),
            "--inbox-verify-fixture" => return inbox_check::verify(directory),
            _ => {}
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let [command, state, identity, key] = args.as_slice() {
            if command == "--provision-owner" {
                return authenticated::provision(state, identity, key);
            }
            if command == "--recover-owner" {
                return authenticated::recover_owner(state, identity, key);
            }
        }
        if let [command, identity, backup] = args.as_slice() {
            if command == "--backup-identity" {
                return authenticated::backup_identity(identity, backup);
            }
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        match args.as_slice() {
            [command, path, uid, state, identity, media_path, media_uid, archive, access]
                if command == "--serve-media-history" =>
            {
                return runtime.block_on(authenticated::serve_media_history(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                    media_path,
                    media_uid.parse()?,
                    (archive, access),
                ));
            }
            #[cfg(feature = "dev-fixtures")]
            [command, path, uid, state, identity, journal]
                if command == "--serve-network-fault-trials" =>
            {
                network_trial::enable_fault(std::path::Path::new(journal))?;
                return runtime.block_on(authenticated::serve(path, uid.parse()?, state, identity));
            }
            [command, path, uid, state, identity, journal]
                if command == "--serve-network-trials" =>
            {
                network_trial::enable(std::path::Path::new(journal))?;
                return runtime.block_on(authenticated::serve(path, uid.parse()?, state, identity));
            }
            [command, path, uid, state, identity, media_path, media_uid]
                if command == "--serve-media-authorizing" =>
            {
                return runtime.block_on(authenticated::serve_media(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                    media_path,
                    media_uid.parse()?,
                ));
            }
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
            [command, path, uid, state, identity] if command == "--serve-observing" => {
                return runtime.block_on(authenticated::serve_observing(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                ));
            }
            [command, path, uid, state, identity, secret_file]
                if command == "--serve-local-setup" =>
            {
                return runtime.block_on(authenticated::serve_setup(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                    secret_file,
                ));
            }
            [command, path, uid, state, identity] if command == "--serve-messaging" => {
                return runtime.block_on(authenticated::serve_messaging(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                ));
            }
            [command, path, uid, state, identity, archive, access]
                if command == "--serve-history" =>
            {
                return runtime.block_on(authenticated::serve_history(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                    archive,
                    access,
                ));
            }
            [command, path, uid, state, identity]
                if command == "--serve-call-observing" || command == "--serve-telephony" =>
            {
                return runtime.block_on(authenticated::serve_calls(
                    path,
                    uid.parse()?,
                    state,
                    identity,
                    command == "--serve-telephony",
                ));
            }
            [command, path] if command == "--request-auth" => {
                return runtime.block_on(authenticated::request(path));
            }
            _ => {}
        }
    }
    Err("usage: --self-check | --probe (Linux) | --serve-readonly SOCKET ALLOWED_UID | --request SOCKET health|capabilities | --serve-authenticated SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR | --serve-observing SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR | --serve-messaging SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR | --serve-call-observing SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR | --serve-telephony SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR | --serve-media-authorizing SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR MEDIA_SOCKET MEDIA_UID | --serve-history SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR ARCHIVE_DIR ACCESS_DIR | --serve-media-history SOCKET ALLOWED_UID STATE_DIR IDENTITY_DIR MEDIA_SOCKET MEDIA_UID ARCHIVE_DIR ACCESS_DIR | --request-auth SOCKET (JSON stdin) | --provision-owner STATE_DIR IDENTITY_DIR CLIENT_PUBLIC_KEY_HEX | --recover-owner STATE_DIR IDENTITY_DIR CLIENT_PUBLIC_KEY_HEX | --backup-identity IDENTITY_DIR PRIVATE_BACKUP_DIR; development fixture commands require dev-fixtures".into())
}
