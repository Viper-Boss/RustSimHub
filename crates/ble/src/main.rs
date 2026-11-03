// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    if std::env::args_os().skip(1).collect::<Vec<_>>() != [std::ffi::OsString::from("--service")] {
        eprintln!("usage: rustsimhub-ble --service");
        return std::process::ExitCode::FAILURE;
    }
    match launch() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(_) => {
            eprintln!("BLE recovery unavailable or cleanup unverified");
            std::process::ExitCode::FAILURE
        }
    }
}
#[cfg(target_os = "linux")]
fn launch() -> Result<(), rustsimhub_ble::Error> {
    use rustsimhub_ble::{
        bluez_control::{Adapter, BluezControl},
        core_port::UnixSecurePort,
        service, Error,
    };
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
        time::Duration,
    };
    let dir = std::env::var_os("CREDENTIALS_DIRECTORY").ok_or(Error::Unavailable)?;
    let path = std::path::PathBuf::from(dir).join("bridge-policy.json");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| Error::Unavailable)?;
    let meta = file.metadata().map_err(|_| Error::Unavailable)?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.len() > 8192
        || ![0, rustix::process::geteuid().as_raw()].contains(&meta.uid())
        || meta.mode() & 0o7777 != 0o400
    {
        return Err(Error::Unavailable);
    }
    let mut bytes = Vec::new();
    file.take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Unavailable)?;
    let endpoint = service::endpoint_from_policy(&bytes)?;
    let mut domain = [0; 16];
    getrandom::fill(&mut domain).map_err(|_| Error::Unavailable)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::Unavailable)?;
    runtime.block_on(async move {
        // No advertisement or bus side effect before actual Core-role probe.
        let port = UnixSecurePort::connect(endpoint).await?;
        let control = tokio::time::timeout(Duration::from_secs(5), async {
            let bus = zbus::Connection::system()
                .await
                .map_err(|_| Error::Unavailable)?;
            let proxy = zbus::fdo::DBusProxy::new(&bus)
                .await
                .map_err(|_| Error::Unavailable)?;
            let name =
                zbus::names::BusName::try_from("org.bluez").map_err(|_| Error::Unavailable)?;
            let owner = proxy
                .get_name_owner(name)
                .await
                .map_err(|_| Error::Unavailable)?;
            BluezControl::bind(bus, owner.as_str(), Adapter::Hci0).await
        })
        .await
        .map_err(|_| Error::Deadline)??;
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| Error::Unavailable)?;
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|_| Error::Unavailable)?;
        service::run(control, port, domain, async move {
            tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
        })
        .await
    })
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("BLE recovery service requires Linux");
    std::process::exit(1);
}
