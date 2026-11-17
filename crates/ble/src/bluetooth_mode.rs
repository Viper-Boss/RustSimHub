// SPDX-License-Identifier: AGPL-3.0-only
//! Root mode broker. The abstract socket accepts only the installed Core UID.
use crate::nas_bluetooth::peer_path;
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Read, Write},
    os::{
        linux::net::SocketAddrExt,
        unix::{
            fs::OpenOptionsExt,
            net::{SocketAddr, UnixListener as StdListener},
        },
    },
    process::{Command, Output},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    time::timeout,
};

const STATE: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/bluetooth-mode";
const NAS_CONFIG: &str = "/etc/rustsimhub/nas-bluetooth.conf";
const TIMEOUT: &str = "/usr/bin/timeout";
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const MMCLI: &str = "/usr/bin/mmcli";
const MAIN_SOCKET: &[u8] = b"rustsimhub-bluetooth-mode-v1";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Airsim,
    Nas,
}

impl Mode {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "airsim" => Some(Self::Airsim),
            "nassimhub" => Some(Self::Nas),
            _ => None,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Airsim => "airsim",
            Self::Nas => "nassimhub",
        }
    }
    fn unit(self) -> &'static str {
        match self {
            Self::Airsim => "rustsimhub-airsim-voble.service",
            Self::Nas => "rustsimhub-nas-bluetooth.service",
        }
    }
    fn other(self) -> Self {
        match self {
            Self::Airsim => Self::Nas,
            Self::Nas => Self::Airsim,
        }
    }
}

fn run(program: &str, sec: &str, args: &[&str]) -> io::Result<Output> {
    Command::new(TIMEOUT)
        .args(["-k", "1", sec, program])
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .output()
}

fn small_file(path: &str) -> io::Result<String> {
    let mut value = String::new();
    fs::File::open(path)?.take(257).read_to_string(&mut value)?;
    if value.len() > 256 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(value)
}

fn nas_configured() -> bool {
    small_file(NAS_CONFIG)
        .ok()
        .and_then(|text| {
            let mut lines = text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'));
            let value = lines.next()?.strip_prefix("NAS_BLUETOOTH_PEER=")?;
            if lines.next().is_some() {
                return None;
            }
            peer_path(value)
        })
        .is_some()
}

fn selected() -> io::Result<Mode> {
    match small_file(STATE) {
        Ok(s) => Mode::parse(s.trim()).ok_or_else(|| io::ErrorKind::InvalidData.into()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Mode::Airsim),
        Err(e) => Err(e),
    }
}

fn save(mode: Mode) -> io::Result<()> {
    let temporary = format!("{STATE}.{}", std::process::id());
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        let mut file = file;
        writeln!(file, "{}", mode.name())?;
        file.sync_all()?;
        fs::rename(&temporary, STATE)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn active(mode: Mode) -> io::Result<bool> {
    let output = run(SYSTEMCTL, "2s", &["is-active", mode.unit()])?;
    match std::str::from_utf8(&output.stdout).ok().map(str::trim) {
        Some("active") if output.status.success() => Ok(true),
        Some("inactive" | "failed" | "unknown") => Ok(false),
        _ => Err(io::Error::other("systemd unit status unknown")),
    }
}

fn execute(action: &str, mode: Mode) -> bool {
    run(SYSTEMCTL, "18s", &[action, mode.unit()]).is_ok_and(|v| v.status.success())
}

fn quiescent() -> bool {
    let check = || -> Option<bool> {
        let listed = run(MMCLI, "2s", &["-L"]).ok()?;
        if !listed.status.success() {
            return None;
        }
        let mut modems = std::str::from_utf8(&listed.stdout)
            .ok()?
            .split_whitespace()
            .filter_map(|item| item.strip_prefix("/org/freedesktop/ModemManager1/Modem/"));
        let modem = modems
            .next()
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))?;
        if modems.next().is_some() {
            return None;
        }
        let result = run(MMCLI, "2s", &["-m", modem, "--voice-list-calls"]).ok()?;
        if !result.status.success() {
            return None;
        }
        let text = std::str::from_utf8(&result.stdout).ok()?.trim();
        Some(
            matches!(text, "No calls were found" | "No calls were found.")
                || !text.is_empty()
                    && text
                        .lines()
                        .filter(|line| !line.trim().is_empty())
                        .all(|line| {
                            let mut fields = line.split_whitespace();
                            let call = fields.next().and_then(|x| {
                                x.strip_prefix("/org/freedesktop/ModemManager1/Call/")
                            });
                            call.is_some_and(|v| {
                                !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())
                            }) && matches!(fields.next(), Some("incoming" | "outgoing"))
                                && fields.next() == Some("(terminated)")
                                && fields.next().is_none()
                        }),
        )
    };
    check().unwrap_or(false)
}

fn view(mode: Mode, pending: Option<Mode>, failure: Option<&str>) -> io::Result<Value> {
    let nas = nas_configured();
    if let Some(wanted) = pending {
        return Ok(
            json!({"mode":mode.name(),"effective":null,"pending":wanted.name(),"nas_configured":nas}),
        );
    }
    let now = active(mode)?;
    let other = active(mode.other())?;
    let effective = match (now, other) {
        (true, false) => Some(mode.name()),
        (false, true) => Some(mode.other().name()),
        _ => None,
    };
    Ok(
        json!({"mode":mode.name(),"effective":effective,"healthy":now&&!other,"can_switch":!other,"nas_configured":nas,"last_error":failure}),
    )
}

fn switch(previous: Mode, target: Mode) -> Result<(), &'static str> {
    let unknown = |_| "bluetooth_mode_unavailable";
    if selected().map_err(unknown)? != previous {
        return Err("bluetooth_mode_stale");
    }
    if target == Mode::Nas && !nas_configured() {
        return Err("bluetooth_nas_not_ready");
    }
    let was_active = active(previous).map_err(unknown)?;
    if active(previous.other()).map_err(unknown)? {
        return Err("bluetooth_mode_unavailable");
    }
    if was_active && previous == target {
        return Ok(());
    }
    if !quiescent() {
        return Err("bluetooth_call_busy");
    }
    let stopped = !was_active || execute("stop", previous) && matches!(active(previous), Ok(false));
    if !stopped {
        return Err("bluetooth_recovery_required");
    }
    let ready = quiescent()
        && execute("start", target)
        && matches!(active(target), Ok(true))
        && matches!(active(target.other()), Ok(false));
    if ready && save(target).is_ok() {
        return Ok(());
    }
    execute("stop", target);
    if !matches!(active(target), Ok(false)) || was_active && !execute("start", previous) {
        return Err("bluetooth_recovery_required");
    }
    Err("bluetooth_switch_failed")
}

#[derive(Default)]
struct Work {
    pending: Option<Mode>,
    failure: Option<&'static str>,
}

fn handle(payload: &str, work: Arc<Mutex<Work>>) -> io::Result<Value> {
    let fields: Vec<_> = payload.split_ascii_whitespace().collect();
    let mut status = work
        .lock()
        .map_err(|_| io::Error::other("mode controller unavailable"))?;
    let mode = selected()?;
    if fields == ["S"] {
        return view(mode, status.pending, status.failure);
    }
    let ["C", previous, selected] = fields.as_slice() else {
        return Ok(json!({"error":"bluetooth_invalid_request"}));
    };
    let Some((expected, wanted)) = Mode::parse(previous).zip(Mode::parse(selected)) else {
        return Ok(json!({"error":"bluetooth_invalid_request"}));
    };
    if expected != mode {
        return Ok(json!({"error":"bluetooth_mode_stale"}));
    }
    if status.pending.is_some() {
        return Ok(json!({"error":"bluetooth_switch_busy"}));
    }
    if wanted == Mode::Nas && !nas_configured() {
        return Ok(json!({"error":"bluetooth_nas_not_ready"}));
    }
    status.pending = Some(wanted);
    status.failure = None;
    drop(status);
    std::thread::spawn(move || {
        let result = switch(expected, wanted);
        if let Ok(mut status) = work.lock() {
            status.pending = None;
            status.failure = result.err();
        }
    });
    view(mode, Some(wanted), None)
}

async fn request(mut stream: UnixStream, work: Arc<Mutex<Work>>) -> io::Result<()> {
    let mut payload = Vec::new();
    timeout(Duration::from_secs(2), async {
        for _ in 0..96 {
            let byte = stream.read_u8().await?;
            if byte == b'\n' {
                return Ok::<_, io::Error>(());
            }
            payload.push(byte);
        }
        Err(io::ErrorKind::InvalidData.into())
    })
    .await??;
    let result = tokio::task::spawn_blocking(move || {
        std::str::from_utf8(&payload)
            .map_err(io::Error::other)
            .and_then(|s| handle(s, work))
    })
    .await
    .map_err(io::Error::other)?;
    let result = result.unwrap_or_else(|_| json!({"error":"bluetooth_mode_unavailable"}));
    timeout(
        Duration::from_secs(1),
        stream.write_all(result.to_string().as_bytes()),
    )
    .await??;
    Ok(())
}

pub async fn serve() -> io::Result<()> {
    let password = run("/usr/bin/getent", "2s", &["passwd", "rustsimhub-core"])?;
    if !password.status.success() {
        return Err(io::Error::other("Core account unavailable"));
    }
    let uid = std::str::from_utf8(&password.stdout)
        .ok()
        .and_then(|line| line.split(':').nth(2))
        .and_then(|uid| uid.parse::<u32>().ok())
        .filter(|uid| *uid > 0)
        .ok_or_else(|| io::Error::other("Core account unavailable"))?;
    let mode = selected()?;
    if active(mode.other())?
        && (!quiescent() || !execute("stop", mode.other()) || active(mode.other())?)
    {
        return Err(io::Error::other("unexpected active Bluetooth role"));
    }
    if !active(mode)? {
        execute("start", mode);
    }
    let raw = StdListener::bind_addr(&SocketAddr::from_abstract_name(MAIN_SOCKET)?)?;
    raw.set_nonblocking(true)?;
    let listener = UnixListener::from_std(raw)?;
    let work = Arc::new(Mutex::new(Work::default()));
    let slots = Arc::new(tokio::sync::Semaphore::new(4));
    loop {
        let (stream, _) = listener.accept().await?;
        if !stream
            .peer_cred()
            .is_ok_and(|credential| credential.uid() == uid)
        {
            continue;
        }
        let Ok(permit) = slots.clone().try_acquire_owned() else {
            continue;
        };
        let work = work.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let _ = request(stream, work).await;
        });
    }
}
