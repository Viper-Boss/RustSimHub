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
    checked_owned(path, uid, None, mode, limit)
}
fn checked_owned(
    path: &Path,
    uid: u32,
    gid: Option<u32>,
    mode: u32,
    limit: u64,
) -> Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.uid() != uid
        || gid.is_some_and(|g| m.gid() != g)
        || m.mode() & 0o7777 != mode
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
const BASE_UNITS: [&str; 8] = [
    "rustsimhub-firstboot.service",
    "rustsimhub-usb.service",
    "rustsimhub-device-firmware.service",
    "rustsimhub-audio-drivers.service",
    "rustsimhub-radio-resume.service",
    "rmtfs.service",
    "ModemManager.service",
    "NetworkManager.service",
];
const BUSINESS_UNITS: [&str; 2] = ["rustsimhub-core.service", "rustsimhub-edge.service"];
/// (failed, activating) from one `systemctl show -p ActiveState` over all
/// units. Paragraphs follow argument order (an alias would report another Id),
/// so they are matched by position. Business units count only once
/// installation is complete; activating ones never mean "configuring".
fn unit_states(text: &str, complete: bool) -> Result<(bool, bool)> {
    let states = text
        .split(
            "

",
        )
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let mut lines = p.lines();
            match (
                lines.next().and_then(|l| l.strip_prefix("ActiveState=")),
                lines.next(),
            ) {
                (Some(state), None) => Ok(state),
                _ => Err("malformed unit status"),
            }
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if states.len() != BASE_UNITS.len() + BUSINESS_UNITS.len() {
        return Err("incomplete unit status".into());
    }
    let (base, business) = states.split_at(BASE_UNITS.len());
    let failed = base.contains(&"failed") || (complete && business.contains(&"failed"));
    Ok((failed, base.contains(&"activating")))
}
fn refresh() -> Result<Inputs> {
    let complete =
        Path::new("/var/lib/rustsimhub/rustsimhub-v1/system/installation.complete").is_file();
    let mut args = vec!["show"];
    args.extend(BASE_UNITS);
    args.extend(BUSINESS_UNITS);
    args.extend(["-p", "ActiveState"]);
    let (failed, activating) = unit_states(&command("/usr/bin/systemctl", &args)?, complete)?;
    let mut s = Inputs {
        error: failed,
        configuring: !complete || activating,
        ..Inputs::default()
    };
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
    // Root here has no capabilities: Core shares the hint through group root.
    let dir = fs::symlink_metadata("/run/rustsimhub-led-input").ok()?;
    if !dir.is_dir() || dir.uid() != 986 || dir.gid() != 0 || dir.mode() & 0o7777 != 0o2750 {
        return None;
    }
    let bytes = checked_owned(
        Path::new("/run/rustsimhub-led-input/state.json"),
        986,
        Some(0),
        0o640,
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
        // Unit and NM state change rarely; every child process costs CPU.
        thread::sleep(Duration::from_secs(15));
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
            input.ringing = b.ringing;
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
    /// Unit state text in argument order, as `systemctl show` prints it.
    fn with(id: &str, state: &str) -> String {
        BASE_UNITS
            .iter()
            .chain(&BUSINESS_UNITS)
            .map(|u| {
                format!(
                    "ActiveState={}
",
                    if *u == id { state } else { "active" }
                )
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }
    #[test]
    fn one_status_query_keeps_the_previous_red_and_white_rules() {
        assert_eq!(unit_states(&with("", ""), true).unwrap(), (false, false));
        assert_eq!(
            unit_states(&with("ModemManager.service", "failed"), true).unwrap(),
            (true, false)
        );
        assert_eq!(
            unit_states(&with("rmtfs.service", "activating"), true).unwrap(),
            (false, true)
        );
        assert_eq!(
            unit_states(&with("rustsimhub-core.service", "failed"), true).unwrap(),
            (true, false)
        );
        // Before installation completes, business units neither turn red nor white.
        assert_eq!(
            unit_states(&with("rustsimhub-edge.service", "failed"), false).unwrap(),
            (false, false)
        );
        assert_eq!(
            unit_states(&with("rustsimhub-core.service", "activating"), true).unwrap(),
            (false, false)
        );
        assert_eq!(
            unit_states(&with("rustsimhub-usb.service", "failed"), false).unwrap(),
            (true, false)
        );
    }
    #[test]
    fn malformed_partial_or_extra_status_is_an_error_not_green() {
        let good = with("", "");
        let mut parts: Vec<_> = good
            .split(
                "

",
            )
            .collect();
        parts.pop();
        assert!(unit_states(
            &parts.join(
                "

"
            ),
            true
        )
        .is_err());
        assert!(unit_states(&good.replacen("ActiveState=", "SubState=", 1), true).is_err());
        assert!(unit_states(
            &good.replacen(
                "ActiveState=active
",
                "ActiveState=active
Id=x
",
                1
            ),
            true
        )
        .is_err());
        assert!(unit_states(
            &format!(
                "{good}
ActiveState=active
"
            ),
            true
        )
        .is_err());
        assert!(unit_states("", true).is_err());
    }
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
