// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_status_led::{BusinessInput, Color, Inputs, Policy, UpdateInput};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const LEDS: [&str; 3] = ["red:", "green:wan", "blue:"];
fn checked(path: &Path, uid: u32, mode: u32, limit: u64) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.uid() != uid
        || m.mode() & 0o777 != mode
        || m.nlink() != 1
        || m.len() > limit
    {
        return Err("invalid indicator input metadata".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err("indicator input exceeds bound".into());
    }
    Ok(bytes)
}
fn uptime() -> Result<u64> {
    let text = fs::read_to_string("/proc/uptime")?;
    Ok(text
        .split('.')
        .next()
        .ok_or("uptime")?
        .parse::<u64>()?
        .checked_mul(1000)
        .ok_or("uptime")?)
}
fn validate_board() -> Result<()> {
    if !fs::read("/sys/firmware/devicetree/base/compatible")?
        .split(|b| *b == 0)
        .any(|s| s == b"thwc,ufi003")
    {
        return Err("board mismatch".into());
    }
    for name in LEDS {
        let path = Path::new("/sys/class/leds").join(name);
        if fs::canonicalize(&path)? != Path::new("/sys/devices/platform/leds/leds").join(name)
            || fs::canonicalize(path.join("device/driver"))?
                != Path::new("/sys/bus/platform/drivers/leds-gpio")
            || fs::read_to_string(path.join("max_brightness"))?.trim() != "1"
        {
            return Err("LED identity mismatch".into());
        }
    }
    Ok(())
}
fn apply(color: Color) -> Result<()> {
    for (name, on) in LEDS.iter().zip(color.channels()) {
        fs::write(
            Path::new("/sys/class/leds").join(name).join("brightness"),
            if on { b"1\n" } else { b"0\n" },
        )?;
    }
    Ok(())
}
fn command(program: &str, args: &[&str]) -> Result<String> {
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait()? {
            let mut bytes = Vec::new();
            child
                .stdout
                .take()
                .ok_or("missing output")?
                .take(16_385)
                .read_to_end(&mut bytes)?;
            if !status.success() || bytes.len() > 16_384 {
                return Err("status query failed".into());
            }
            return Ok(String::from_utf8(bytes)?);
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            return Err("status query timed out".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}
fn refresh() -> Result<Inputs> {
    let complete =
        Path::new("/var/lib/rustsimhub/rustsimhub-v1/system/installation.complete").is_file();
    let units = command(
        "/usr/bin/systemctl",
        &[
            "show",
            "rustsimhub-firstboot.service",
            "rustsimhub-usb.service",
            "rustsimhub-device-firmware.service",
            "rustsimhub-audio-drivers.service",
            "rustsimhub-radio-resume.service",
            "rmtfs.service",
            "ModemManager.service",
            "NetworkManager.service",
            "-p",
            "ActiveState",
        ],
    )?;
    let mut s = Inputs {
        error: units.lines().any(|v| v == "ActiveState=failed"),
        configuring: !complete || units.lines().any(|v| v == "ActiveState=activating"),
        ..Inputs::default()
    };
    if complete {
        let business = command(
            "/usr/bin/systemctl",
            &[
                "show",
                "rustsimhub-core.service",
                "rustsimhub-edge.service",
                "-p",
                "ActiveState",
            ],
        )?;
        s.error |= business.lines().any(|v| v == "ActiveState=failed");
    }
    let devices = command(
        "/usr/bin/nmcli",
        &["-t", "-f", "DEVICE,TYPE,STATE", "device"],
    )?;
    for row in devices.lines() {
        let fields: Vec<_> = row.split(':').collect();
        if fields.len() != 3 {
            continue;
        }
        if fields[1] == "gsm" && fields[2] == "connected" {
            s.cellular_connected = true;
        }
        if fields[0] == "wlan0" && fields[1] == "wifi" && fields[2] == "connected" {
            s.wifi_connected = true;
        }
    }
    // Read cached per-device connectivity only. This program never sends a probe,
    // enables an APN, changes routes, or asks NM to probe a cellular interface.
    if s.wifi_connected {
        let value = command(
            "/usr/bin/nmcli",
            &[
                "-g",
                "GENERAL.IP4-CONNECTIVITY,GENERAL.IP6-CONNECTIVITY",
                "device",
                "show",
                "wlan0",
            ],
        )?;
        s.internet = value.lines().any(|v| v.trim().starts_with("4 (full)"));
    }
    let root = Path::new("/var/lib/rustsimhub/rustsimhub-v1/system/app");
    if fs::symlink_metadata(root)
        .is_ok_and(|m| m.is_dir() && m.uid() == 0 && m.mode() & 0o777 == 0o700)
    {
        if let Ok(bytes) = checked(&root.join("state.json"), 0, 0o600, 8192) {
            if let Ok(v) = serde_json::from_slice(&bytes) {
                if let Some((available, upgrading)) = rustsimhub_status_led::staged_update(&v) {
                    s.update_available = available;
                    s.upgrading = upgrading;
                }
            }
        }
    }
    Ok(s)
}

fn io_count() -> Option<u64> {
    let text = fs::read_to_string("/sys/block/mmcblk0/stat").ok()?;
    let fields: Vec<_> = text.split_whitespace().collect();
    fields
        .first()?
        .parse::<u64>()
        .ok()?
        .checked_add(fields.get(4)?.parse::<u64>().ok()?)
}
fn business(now: u64) -> Option<BusinessInput> {
    let dir = fs::symlink_metadata("/run/rustsimhub-led-input").ok()?;
    if !dir.is_dir() || dir.uid() != 986 || dir.mode() & 0o777 != 0o700 {
        return None;
    }
    let bytes = checked(
        Path::new("/run/rustsimhub-led-input/state.json"),
        986,
        0o600,
        512,
    )
    .ok()?;
    serde_json::from_slice::<BusinessInput>(&bytes)
        .ok()?
        .fresh(now)
}
fn update(now: u64) -> Option<UpdateInput> {
    let dir = fs::symlink_metadata("/run/rustsimhub-led-update").ok()?;
    if !dir.is_dir() || dir.uid() != 0 || dir.mode() & 0o777 != 0o700 {
        return None;
    }
    let bytes = checked(
        Path::new("/run/rustsimhub-led-update/state.json"),
        0,
        0o600,
        512,
    )
    .ok()?;
    serde_json::from_slice::<UpdateInput>(&bytes)
        .ok()?
        .fresh(now)
}
pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"component":"status-led","source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"board":"thwc,ufi003","policy":1,"cellular_probes":false,"smooth_breathing":false})
        );
        return Ok(());
    }
    if !args.is_empty() && args != ["--off"] {
        return Err("invalid action".into());
    }
    validate_board()?;
    for name in LEDS {
        fs::write(
            Path::new("/sys/class/leds").join(name).join("trigger"),
            b"none\n",
        )?;
    }
    if args == ["--off"] {
        return apply(Color::Off);
    }
    let mode = checked(Path::new("/etc/rustsimhub/status-led.conf"), 0, 0o644, 16)?;
    let disabled = match mode.as_slice() {
        b"auto\n" | b"auto" => false,
        b"off\n" | b"off" => true,
        _ => return Err("invalid mode".into()),
    };
    let mut policy = Policy::default();
    let snapshot = Arc::new(Mutex::new(Inputs {
        configuring: true,
        ..Inputs::default()
    }));
    let sampler = snapshot.clone();
    thread::spawn(move || loop {
        let value = refresh().unwrap_or(Inputs {
            error: true,
            ..Inputs::default()
        });
        if let Ok(mut slot) = sampler.lock() {
            *slot = value;
        }
        thread::sleep(Duration::from_secs(5));
    });
    let mut previous = None;
    let start = Instant::now();
    let boot = uptime()?;
    let mut io_next = Instant::now();
    let mut count = None;
    let mut business_hint = None;
    let mut update_hint = None;
    loop {
        let now = boot.saturating_add(start.elapsed().as_millis() as u64);
        let mut input = *snapshot.lock().map_err(|_| "snapshot lock")?;
        input.disabled = disabled;
        if Instant::now() >= io_next {
            count = io_count();
            business_hint = business(now);
            update_hint = update(now);
            io_next = Instant::now() + Duration::from_millis(200);
        }
        if let Some(b) = business_hint.and_then(|b: BusinessInput| b.fresh(now)) {
            input.configuring |= b.configuring;
            input.unread_sms = b.unread_sms;
            input.missed_calls = b.missed_calls;
        }
        if let Some(u) = update_hint.and_then(|u: UpdateInput| u.fresh(now)) {
            input.upgrading |= u.upgrading;
            input.update_available |= u.available;
        }
        let color = policy.frame(input, now, count);
        if previous != Some(color) {
            apply(color)?;
            previous = Some(color);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn input_metadata_refuses_symlinks_hardlinks_permissions_owner_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("input");
        fs::write(&file, b"safe").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let uid = fs::metadata(&file).unwrap().uid();
        assert_eq!(checked(&file, uid, 0o600, 4).unwrap(), b"safe");
        assert!(checked(&file, uid + 1, 0o600, 4).is_err());
        assert!(checked(&file, uid, 0o644, 4).is_err());
        assert!(checked(&file, uid, 0o600, 3).is_err());
        let link = dir.path().join("alias");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(checked(&link, uid, 0o600, 4).is_err());
        let hard = dir.path().join("hard");
        fs::hard_link(&file, &hard).unwrap();
        assert!(checked(&file, uid, 0o600, 4).is_err());
    }
}
