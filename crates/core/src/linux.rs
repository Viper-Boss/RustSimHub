// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_wire::{check_len, parse_request, Command, Response, MAX_FRAME, PROTOCOL};
use serde::Serialize;
use serde_json::json;
use std::{
    error::Error,
    io,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::Semaphore,
    time::timeout,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize)]
pub struct ModemSummary {
    state: Option<i32>,
    access_technologies: Option<u32>,
    voice_supported: bool,
    messaging_supported: bool,
}

#[derive(Debug, Serialize)]
pub struct Capabilities {
    schema: u16,
    arch: &'static str,
    kernel: String,
    modem_manager_available: bool,
    modems: Vec<ModemSummary>,
    hardware_mutations: bool,
}

/// Whitelisted capability output. Identifiers, numbers and SMS contents are never serialized.
pub async fn probe() -> Result<Capabilities> {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")?
        .trim()
        .to_string();
    let conn = timeout(Duration::from_secs(5), zbus::Connection::system())
        .await?
        .map_err(|_| "system D-Bus unavailable")?;
    let manager = zbus::fdo::ObjectManagerProxy::new(
        &conn,
        "org.freedesktop.ModemManager1",
        "/org/freedesktop/ModemManager1",
    )
    .await
    .map_err(|_| "ModemManager proxy unavailable")?;
    let managed = timeout(Duration::from_secs(5), manager.get_managed_objects())
        .await?
        .map_err(|_| "ModemManager enumeration failed")?;
    let mut modems = Vec::new();
    for interfaces in managed.values() {
        if let Some(properties) = interfaces.get("org.freedesktop.ModemManager1.Modem") {
            modems.push(ModemSummary {
                state: properties.get("State").and_then(|v| i32::try_from(v).ok()),
                access_technologies: properties
                    .get("AccessTechnologies")
                    .and_then(|v| u32::try_from(v).ok()),
                voice_supported: interfaces
                    .contains_key("org.freedesktop.ModemManager1.Modem.Voice"),
                messaging_supported: interfaces
                    .contains_key("org.freedesktop.ModemManager1.Modem.Messaging"),
            });
        }
    }
    // Stable output independent of HashMap iteration order.
    modems.sort_by_key(|m| {
        (
            m.state,
            m.access_technologies,
            m.voice_supported,
            m.messaging_supported,
        )
    });
    Ok(Capabilities {
        schema: 1,
        arch: std::env::consts::ARCH,
        kernel,
        modem_manager_available: true,
        modems,
        hardware_mutations: false,
    })
}

pub async fn probe_and_print() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&probe().await?)?);
    Ok(())
}

pub(crate) async fn read_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let len = check_len(stream.read_u32().await?)?;
    let mut body = vec![0; len];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

pub(crate) async fn write_frame(stream: &mut UnixStream, body: &[u8]) -> io::Result<()> {
    if body.is_empty() || body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame size",
        ));
    }
    stream.write_u32(body.len() as u32).await?;
    stream.write_all(body).await
}

async fn handle(stream: &mut UnixStream) -> Result<()> {
    let request = parse_request(&timeout(Duration::from_secs(5), read_frame(stream)).await??)?;
    let result = match request.command {
        Command::Health => {
            json!({"ok":true,"version":env!("CARGO_PKG_VERSION"),"mode":"read_only","write_api":false})
        }
        Command::Capabilities => match probe().await {
            Ok(value) => serde_json::to_value(value)?,
            Err(_) => json!({"ok":false,"error":"modem_unavailable"}),
        },
    };
    let response = serde_json::to_vec(&Response {
        protocol: PROTOCOL,
        id: request.id,
        result,
    })?;
    timeout(Duration::from_secs(5), write_frame(stream, &response)).await??;
    Ok(())
}

/// One request per connection. Restrictive private directory + SO_PEERCRED.
/// No stale socket removal: a second daemon must not unlink a live socket.
pub(crate) fn bind_listener(socket: &str) -> Result<UnixListener> {
    let parent = Path::new(socket).parent().ok_or("socket has no parent")?;
    let meta = std::fs::symlink_metadata(parent)?;
    if !meta.is_dir() || meta.permissions().mode() & 0o022 != 0 {
        return Err(
            "socket directory must be a non-symlink directory without group/other write permission"
                .into(),
        );
    }
    let listener = UnixListener::bind(socket)?;
    let socket_meta = std::fs::symlink_metadata(socket)?;
    // The directory must belong to the daemon, not an unrelated user.
    if meta.uid() != socket_meta.uid() {
        std::fs::remove_file(socket)?;
        return Err("socket directory owner differs from daemon".into());
    }
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}
pub async fn serve(socket: &str, allowed_uid: u32) -> Result<()> {
    let listener = bind_listener(socket)?;
    let limit = std::sync::Arc::new(Semaphore::new(8));
    loop {
        let (mut stream, _) = listener.accept().await?;
        if stream.peer_cred()?.uid() != allowed_uid {
            continue;
        }
        let permit = match limit.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => continue,
        };
        tokio::spawn(async move {
            let _ = timeout(Duration::from_secs(15), handle(&mut stream)).await;
            drop(permit);
        });
    }
}

pub async fn request(socket: &str, command: &str) -> Result<()> {
    let command = match command {
        "health" => Command::Health,
        "capabilities" => Command::Capabilities,
        _ => return Err("unknown command".into()),
    };
    let mut stream = timeout(Duration::from_secs(5), UnixStream::connect(socket)).await??;
    let body = serde_json::to_vec(&rustsimhub_wire::Request {
        protocol: PROTOCOL,
        id: "cli".into(),
        command,
    })?;
    timeout(Duration::from_secs(5), write_frame(&mut stream, &body)).await??;
    let body = timeout(Duration::from_secs(15), read_frame(&mut stream)).await??;
    let response: Response = serde_json::from_slice(&body)?;
    if response.protocol != PROTOCOL || response.id != "cli" {
        return Err("unexpected response".into());
    }
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn malicious_frame_is_rejected_on_real_unix_socket() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        sender.write_u32(u32::MAX).await.unwrap();
        assert_eq!(
            read_frame(&mut receiver).await.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[tokio::test]
    async fn health_round_trip_contains_no_write_api() {
        let (mut sender, mut receiver) = UnixStream::pair().unwrap();
        let task = tokio::spawn(async move {
            handle(&mut receiver).await.unwrap();
        });
        write_frame(
            &mut sender,
            br#"{"protocol":1,"id":"test","command":"health"}"#,
        )
        .await
        .unwrap();
        let response: Response =
            serde_json::from_slice(&read_frame(&mut sender).await.unwrap()).unwrap();
        assert_eq!(response.id, "test");
        assert_eq!(response.result["write_api"], false);
        task.await.unwrap();
    }
}
