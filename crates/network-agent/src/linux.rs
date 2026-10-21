// SPDX-License-Identifier: AGPL-3.0-only
use fs2::FileExt;
use rustsimhub_network_agent::{
    cellular_interface, interface_valid, lte_channel, rules, Policy, State,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/network";
const OUTPUT: &str = "/run/rustsimhub-network";
const INPUT: &str = "/run/rustsimhub-network-input";
const LIMIT: usize = rustsimhub_network_agent::MAX_STATE_BYTES;

fn checked(p: &Path, uid: u32, mode: u32) -> Result<Vec<u8>> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(p)?;
    let m = f.metadata()?;
    if !m.is_file()
        || m.uid() != uid
        || m.mode() & 0o777 != mode
        || m.nlink() != 1
        || m.len() > LIMIT as u64
    {
        return Err("unsafe network metadata".into());
    }
    let mut b = vec![];
    f.take(LIMIT as u64 + 1).read_to_end(&mut b)?;
    if b.len() > LIMIT {
        return Err("network file too large".into());
    }
    Ok(b)
}
fn directory(p: &str, mode: u32) -> Result<()> {
    let m = fs::symlink_metadata(p)?;
    if !m.is_dir() || m.uid() != 0 || m.mode() & 0o777 != mode {
        return Err("unsafe network directory".into());
    }
    Ok(())
}
fn publish(dir: &str, name: &str, value: &impl serde::Serialize, mode: u32) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > LIMIT {
        return Err("network report too large".into());
    }
    let tmp = Path::new(dir).join(format!(".{name}.pending"));
    // The only directory writers are this singleton and Root. Retain unexpected pending data.
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)?;
    f.set_permissions(fs::Permissions::from_mode(mode))?;
    f.write_all(&bytes)?;
    f.sync_all()?;
    drop(f);
    fs::rename(tmp, Path::new(dir).join(name))?;
    File::open(dir)?.sync_all()?;
    Ok(())
}
fn command(program: &str, args: &[&str], input: Option<&str>) -> Result<String> {
    if !["/usr/bin/nmcli", "/usr/bin/mmcli", "/usr/sbin/nft"].contains(&program) {
        return Err("unknown network tool".into());
    }
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let reader = thread::spawn(move || {
        let mut b = vec![];
        stdout.take(LIMIT as u64 + 1).read_to_end(&mut b).map(|_| b)
    });
    if let Some(s) = input {
        child
            .stdin
            .take()
            .ok_or("missing stdin")?
            .write_all(s.as_bytes())?;
    }
    let end = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break Some(s);
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let b = reader.join().map_err(|_| "reader failed")??;
    if b.len() > LIMIT || !status.is_some_and(|s| s.success()) {
        return Err("bounded network tool failed".into());
    }
    Ok(String::from_utf8(b)?)
}
fn nft(script: &str) -> Result<()> {
    command("/usr/sbin/nft", &["-f", "-"], Some(script))?;
    Ok(())
}
fn counters() -> Result<(u64, u64)> {
    let v: Value = serde_json::from_str(&command(
        "/usr/sbin/nft",
        &["-j", "list", "table", "inet", "rustsimhub_net"],
        None,
    )?)?;
    let mut rx = None;
    let mut tx = None;
    for row in v["nftables"].as_array().ok_or("nft response")? {
        if let Some(c) = row.get("counter") {
            if c["family"] != "inet" || c["table"] != "rustsimhub_net" {
                return Err("foreign counter".into());
            }
            match c["name"].as_str() {
                Some("rx") => rx = c["bytes"].as_u64(),
                Some("tx") => tx = c["bytes"].as_u64(),
                _ => return Err("unexpected counter".into()),
            }
        }
    }
    Ok((rx.ok_or("missing rx")?, tx.ok_or("missing tx")?))
}
fn quota_exhausted() -> Result<bool> {
    let v: Value = serde_json::from_str(&command(
        "/usr/sbin/nft",
        &["-j", "list", "quota", "inet", "rustsimhub_net", "cap"],
        None,
    )?)?;
    for row in v["nftables"].as_array().ok_or("quota response")? {
        if let Some(q) = row.get("quota") {
            if q["name"] == "cap" && q["table"] == "rustsimhub_net" {
                return Ok(
                    q["used"].as_u64().unwrap_or(0) >= q["bytes"].as_u64().ok_or("quota bytes")?
                );
            }
        }
    }
    Err("missing quota".into())
}
fn lifecycle(stop: bool) -> Result<()> {
    directory(BASE, 0o700)?;
    // Gate ordinary data before reading any ledger. Corrupt state must not take USB/NM down.
    let fresh = counters().is_err();
    if !stop {
        nft(&rules(
            &[
                "wwan0".into(),
                "wwan1".into(),
                "wwan2".into(),
                "wwan3".into(),
                "wwan4".into(),
                "wwan5".into(),
                "wwan6".into(),
                "wwan7".into(),
            ],
            Some(0),
            fresh,
        )?)?;
    }
    let file = Path::new(BASE).join("meter.json");
    let mut state: Option<State> = if file.try_exists()? {
        let s: State = serde_json::from_slice(&checked(&file, 0, 0o600)?)?;
        if !s.valid() {
            return Err("invalid accounting".into());
        }
        Some(s)
    } else {
        None
    };
    if stop {
        // Stop packets before the final durable checkpoint. Preserve named counters.
        nft(&rules(
            &[
                "wwan0".into(),
                "wwan1".into(),
                "wwan2".into(),
                "wwan3".into(),
                "wwan4".into(),
                "wwan5".into(),
                "wwan6".into(),
                "wwan7".into(),
            ],
            Some(0),
            false,
        )?)?;
        if let Some(ref mut s) = state {
            let (rx, tx) = counters()?;
            let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")?
                .trim()
                .to_owned();
            let sim = s.active_sim.clone();
            if s.sample(&boot, &sim, now()?, rx, tx).is_err() {
                s.uncertain = true;
            }
            s.was_running = false;
            publish(BASE, "meter.json", s, 0o600)?;
        }
    } else if fresh {
        if let Some(ref mut s) = state {
            if s.was_running && s.policy.enabled {
                s.uncertain = true;
            }
            s.counter_boot.clear();
            s.last_rx = 0;
            s.last_tx = 0;
            publish(BASE, "meter.json", s, 0o600)?;
        }
    }
    Ok(())
}
fn now() -> Result<i64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .try_into()?)
}
fn uptime_ms() -> Result<u64> {
    Ok((fs::read_to_string("/proc/uptime")?
        .split_whitespace()
        .next()
        .ok_or("uptime")?
        .parse::<f64>()?
        * 1000.) as u64)
}
fn text(v: &Value, path: &str, max: usize) -> Option<String> {
    let s = v.pointer(path)?.as_str()?;
    if s.is_empty() || s == "--" || s.len() > max || s.chars().any(char::is_control) {
        None
    } else {
        Some(s.to_owned())
    }
}
fn numeric(v: &Value, path: &str) -> Option<f64> {
    let n = v.pointer(path)?.as_str()?.parse::<f64>().ok()?;
    n.is_finite().then_some(n)
}
fn collect_radio(salt: &str) -> Result<(Value, String)> {
    let list = command("/usr/bin/mmcli", &["-L"], None)?;
    let paths: Vec<_> = list
        .split_whitespace()
        .filter(|s| s.starts_with("/org/freedesktop/ModemManager1/Modem/"))
        .collect();
    if paths.len() != 1
        || !paths[0]
            .rsplit('/')
            .next()
            .unwrap_or("")
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        return Err("one modem required".into());
    }
    let p = paths[0];
    let all: Value = serde_json::from_str(&command("/usr/bin/mmcli", &["-m", p, "-J"], None)?)?;
    let sim = text(&all, "/modem/generic/sim", 100).ok_or("SIM unavailable")?;
    if !sim.starts_with("/org/freedesktop/ModemManager1/SIM/")
        || !sim
            .rsplit('/')
            .next()
            .unwrap_or("")
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        return Err("SIM path".into());
    }
    let simdata: Value =
        serde_json::from_str(&command("/usr/bin/mmcli", &["-i", &sim, "-J"], None)?)?;
    let iccid = rustsimhub_domain::canonical_sim_identifier(
        &text(&simdata, "/sim/properties/iccid", 32).ok_or("SIM unavailable")?,
    )
    .ok_or("invalid SIM")?;
    let key = format!(
        "{:x}",
        Sha256::digest(format!("RustSimHub.NetworkMeter.v1\0{salt}\0{iccid}"))
    );
    let cells = command("/usr/bin/mmcli", &["-m", p, "--get-cell-info", "-J"], None)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok());
    let mut cell = json!({"available":false,"bandwidth_mhz":null,"bandwidth_note":"ModemManager未提供当前带宽"});
    if let Some(rows) = cells
        .as_ref()
        .and_then(|v| v.pointer("/modem/generic/cell-info"))
        .and_then(Value::as_array)
    {
        for row in rows.iter().take(32) {
            let Some(s) = row.as_str().filter(|s| s.len() <= 2048) else {
                continue;
            };
            let fields: std::collections::HashMap<_, _> =
                s.split(", ").filter_map(|s| s.split_once(": ")).collect();
            if fields.get("serving") != Some(&"yes") {
                continue;
            }
            cell["available"] = true.into();
            for (from, to) in [
                ("cell type", "technology"),
                ("operator id", "operator_code"),
                ("tac", "tracking_area_code"),
                ("ci", "cell_id"),
                ("physical ci", "physical_cell_id"),
            ] {
                if let Some(v) = fields.get(from).filter(|v| {
                    v.len() <= 32
                        && v.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-".contains(&b))
                }) {
                    cell[to] = (*v).into();
                }
            }
            for field in ["earfcn", "rsrp", "rsrq", "timing advance", "sinr", "rssi"] {
                if let Some(n) = fields
                    .get(field)
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                {
                    cell[field.replace(' ', "_")] = json!(n);
                }
            }
            if fields.get("cell type") == Some(&"lte") {
                if let Some(n) = fields
                    .get("earfcn")
                    .and_then(|s| s.parse::<u32>().ok())
                    .and_then(lte_channel)
                {
                    cell["channel_derived"] = serde_json::to_value(n)?;
                }
            }
            break;
        }
    }
    let list_field = |path: &str| -> Vec<String> {
        all.pointer(path)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .take(64)
                    .filter_map(|v| {
                        v.as_str()
                            .filter(|s| s.len() <= 40 && !s.chars().any(char::is_control))
                            .map(str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok((
        json!({"available":true,"operator":text(&all,"/modem/3gpp/operator-name",128),"operator_code":text(&all,"/modem/3gpp/operator-code",8),"registration":text(&all,"/modem/3gpp/registration-state",32),"state":text(&all,"/modem/generic/state",32),"access_technologies":list_field("/modem/generic/access-technologies"),"signal_percent":numeric(&all,"/modem/generic/signal-quality/value"),"configured_bands":list_field("/modem/generic/current-bands"),"supported_bands":list_field("/modem/generic/supported-bands"),"serving_cell":cell}),
        key,
    ))
}
fn collect_devices() -> Result<(Vec<Value>, Vec<String>, Vec<String>)> {
    let raw = command(
        "/usr/bin/nmcli",
        &["-t", "-f", "GENERAL,IP4,IP6", "device", "show"],
        None,
    )?;
    let mut devices = vec![];
    let mut map = serde_json::Map::new();
    let flush = |m: &mut serde_json::Map<String, Value>, out: &mut Vec<Value>| {
        if !m.is_empty() {
            out.push(Value::Object(std::mem::take(m)));
        }
    };
    for line in raw.lines() {
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.replace("\\:", ":").replace("\\\\", "\\");
        if field == "GENERAL.DEVICE" {
            flush(&mut map, &mut devices);
        }
        if value.len() > 255 || value.chars().any(char::is_control) {
            return Err("device value out of bounds".into());
        }
        let key = match field {
            "GENERAL.DEVICE" => Some("device"),
            "GENERAL.TYPE" => Some("kind"),
            "GENERAL.IP-IFACE" => Some("interface"),
            "GENERAL.STATE" => Some("state"),
            "GENERAL.CON-UUID" => Some("connection_uuid"),
            "IP4.GATEWAY" => Some("gateway_ipv4"),
            "IP6.GATEWAY" => Some("gateway_ipv6"),
            _ => None,
        };
        if let Some(k) = key {
            map.insert(
                k.into(),
                if value.is_empty() || value == "--" {
                    Value::Null
                } else {
                    value.clone().into()
                },
            );
        }
        for (prefix, key) in [
            ("IP4.ADDRESS[", "ipv4"),
            ("IP6.ADDRESS[", "ipv6"),
            ("IP4.DNS[", "dns_ipv4"),
            ("IP6.DNS[", "dns_ipv6"),
        ] {
            if field.starts_with(prefix) {
                let a = map
                    .entry(key)
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .ok_or("bad address vector")?;
                if a.len() >= 16 {
                    return Err("address catalogue too large".into());
                }
                a.push(value.clone().into());
            }
        }
    }
    flush(&mut map, &mut devices);
    if devices.len() > 16 {
        return Err("device catalogue too large".into());
    }
    let mut ifaces = vec![];
    let mut active = vec![];
    for dev in &mut devices {
        let iface = dev["interface"]
            .as_str()
            .or_else(|| dev["device"].as_str())
            .unwrap_or("")
            .to_owned();
        if !interface_valid(&iface) {
            continue;
        }
        if dev["kind"] == "gsm" {
            dev["kind"] = "cellular".into();
            let uuid = dev["connection_uuid"].as_str().unwrap_or("").to_owned();
            if uuid.len() == 36
                && uuid.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
                && dev["state"].as_str().is_some_and(|s| s.starts_with("100 "))
            {
                let apn = command(
                    "/usr/bin/nmcli",
                    &["-g", "gsm.apn", "connection", "show", "uuid", &uuid],
                    None,
                )
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| {
                    s.len() <= 100
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
                });
                dev["apn"] = json!(apn);
                if cellular_interface(&iface)
                    && !matches!(apn.as_deref(), Some("ims" | "sos" | "emergency"))
                {
                    ifaces.push(iface);
                    active.push(uuid);
                }
            }
            dev["counter_source"] = "nftables-ip-packets".into();
        } else {
            for (name, key) in [("rx_bytes", "rx_bytes"), ("tx_bytes", "tx_bytes")] {
                dev[key] = json!(fs::read_to_string(format!(
                    "/sys/class/net/{iface}/statistics/{name}"
                ))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok()));
            }
            dev["counter_source"] = "kernel-interface".into();
        }
        dev.as_object_mut()
            .ok_or("device")?
            .remove("connection_uuid");
    }
    ifaces.sort();
    ifaces.dedup();
    if ifaces.is_empty() {
        ifaces.push("wwan0".into());
    }
    Ok((devices, ifaces, active))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    schema: u16,
    request_id: String,
    boot_id: String,
    expires_uptime_ms: u64,
    policy: Policy,
    #[serde(default)]
    acknowledge_meter_gap: bool,
}
fn input(state: &mut State, boot: &str) -> Result<()> {
    let file = Path::new(INPUT).join("policy.json");
    if !file.try_exists()? {
        return Ok(());
    }
    let r: Request = serde_json::from_slice(&checked(&file, 986, 0o640)?)?;
    let now = uptime_ms()?;
    if r.schema != 1
        || r.request_id.len() != 36
        || !r
            .request_id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
        || !r.policy.valid()
        || r.boot_id != boot
        || r.expires_uptime_ms < now
        || r.expires_uptime_ms > now + 20_000
    {
        return Ok(());
    }
    if r.request_id == state.last_request {
        return Ok(());
    }
    if r.acknowledge_meter_gap {
        state.uncertain = false;
    }
    state.policy = r.policy;
    state.last_request = r.request_id;
    Ok(())
}
pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!(
            "{}",
            json!({"component":"network-agent","schema":1,"source_archive_sha256":option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256"),"arbitrary_commands":false,"apn_autoactivation":false})
        );
        return Ok(());
    }
    if rustix::process::geteuid().as_raw() != 0 {
        return Err("Root service required".into());
    }
    if args == ["--early-guard"] {
        return lifecycle(false);
    }
    if args == ["--checkpoint-stop"] {
        return lifecycle(true);
    }
    if !args.is_empty() {
        return Err("unknown operation".into());
    }
    directory(BASE, 0o700)?;
    directory(OUTPUT, 0o750)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(Path::new(BASE).join(".lock"))?;
    lock.try_lock_exclusive()?;
    let file = Path::new(BASE).join("meter.json");
    let mut state: State = if file.try_exists()? {
        serde_json::from_slice(&checked(&file, 0, 0o600)?)?
    } else {
        let mut salt = [0u8; 32];
        getrandom::fill(&mut salt).map_err(|_| "entropy unavailable")?;
        State::new(salt.iter().map(|b| format!("{b:02x}")).collect())
    };
    if !state.valid() {
        return Err("corrupt accounting; retained".into());
    }
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_owned();
    let mut fresh = false;
    if counters().is_err() {
        nft(&rules(
            &["wwan0".into()],
            state.policy.enabled.then_some(0),
            true,
        )?)?;
        fresh = true;
    }
    if fresh && state.was_running && state.policy.enabled {
        state.uncertain = true;
    }
    if fresh {
        state.counter_boot.clear();
        state.last_rx = 0;
        state.last_tx = 0;
    }
    state.was_running = true;
    publish(BASE, "meter.json", &state, 0o600)?;
    let mut previous = Instant::now();
    let mut last_counter = (0u64, 0u64);
    let mut last_radio = Instant::now() - Duration::from_secs(30);
    let mut radio = json!({"available":false});
    let mut sim = String::new();
    let mut applied: Option<(Vec<String>, Option<u64>, String)> = None;
    let mut last_save = Instant::now();
    loop {
        let timestamp = now()?;
        let tick = Instant::now();
        if last_radio.elapsed() >= Duration::from_secs(15) {
            match collect_radio(&state.salt) {
                Ok((r, s)) => {
                    radio = r;
                    sim = s;
                }
                Err(_) => {
                    radio = json!({"available":false});
                    sim.clear();
                }
            }
            last_radio = Instant::now();
        }
        let (devices, ifaces, active) = match collect_devices() {
            Ok(v) => v,
            Err(_) => {
                thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let (rx, tx) = counters()?;
        let sample_ok = state.sample(&boot, &sim, timestamp, rx, tx).is_ok();
        if !sample_ok {
            state.uncertain = true;
        }
        let old = state.policy.clone();
        let request_before = state.last_request.clone();
        let _ = input(&mut state, &boot);
        let changed = old != state.policy || request_before != state.last_request;
        if changed {
            publish(BASE, "meter.json", &state, 0o600)?;
            last_save = tick;
        }
        let mut usage = state.usage(timestamp)?;
        if state.policy.enabled
            && !changed
            && applied.is_some()
            && quota_exhausted().unwrap_or(true)
        {
            usage.limit_reached = true;
        }
        // A quota is replenished only by a new calendar, explicit policy, SIM, or interface.
        let quota_key = format!(
            "{}:{}:{}:{}:{}",
            usage.period_start,
            state.last_request,
            state.active_sim,
            state.uncertain,
            sim.is_empty()
        );
        let should_apply = applied
            .as_ref()
            .is_none_or(|(i, _, k)| i != &ifaces || k != &quota_key);
        let remaining = state
            .policy
            .enabled
            .then_some(if state.uncertain || sim.is_empty() {
                0
            } else {
                usage.remaining_bytes
            });
        if should_apply {
            nft(&rules(&ifaces, remaining, false)?)?;
            applied = Some((ifaces.clone(), remaining, quota_key));
        }
        if state.policy.enabled && (usage.limit_reached || sim.is_empty()) {
            // Disconnect only the NM ordinary-data profiles observed this tick. Never radio/IMS.
            for uuid in &active {
                let _ = command(
                    "/usr/bin/nmcli",
                    &["-w", "3", "connection", "down", "uuid", uuid],
                    None,
                );
            }
        }
        let elapsed = previous.elapsed().as_secs_f64().max(0.001);
        let rate_rx = rx.checked_sub(last_counter.0).map(|n| n as f64 / elapsed);
        let rate_tx = tx.checked_sub(last_counter.1).map(|n| n as f64 / elapsed);
        let status = json!({"schema":1,"available":true,"sample_unix_seconds":timestamp,"boot_id":boot,"devices":devices,"cellular":radio,"policy":state.policy,"usage":usage,"rx_bytes":rx,"tx_bytes":tx,"rx_bytes_per_second":if last_counter==(0,0){None}else{rate_rx},"tx_bytes_per_second":if last_counter==(0,0){None}else{rate_tx},"counter_source":"nftables-ip-packets","limit_enforcement":"shared-kernel-quota-and-NM-disconnect","accounting_note":"按当前SIM分开，Asia/Shanghai自然月；本机IP层估算，运营商账单为准；异常掉电可能缺最后5秒，限额开启时计量不确定会阻断普通数据","last_request_id":state.last_request,"policy_applied":applied.is_some(),"hardware_mutations":state.policy.enabled,"apn_autoactivation":false});
        publish(OUTPUT, "status.json", &status, 0o640)?;
        if last_save.elapsed() >= Duration::from_secs(5) {
            publish(BASE, "meter.json", &state, 0o600)?;
            last_save = Instant::now();
        }
        previous = tick;
        last_counter = (rx, tx);
        thread::sleep(Duration::from_secs(2));
    }
}
