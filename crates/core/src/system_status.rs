// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only device facts for the web console. Fixed kernel paths only, bounded
//! reads, no subprocess and no serial/IMEI/SIM identifiers. A missing source
//! becomes `null`: the page must show it as unknown, never as a guessed value.
use serde_json::{json, Value};
use std::{fs::File, io::Read, path::Path};

const MAX_READ: u64 = 64 * 1024;
const MAX_THERMAL_ZONES: usize = 32;

fn read_bounded(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(MAX_READ)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}
/// Device-tree strings are NUL-terminated and unvalidated.
fn clean(text: &str) -> Option<String> {
    let value: String = text
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .chars()
        .filter(|c| !c.is_control())
        .take(128)
        .collect();
    (!value.is_empty()).then_some(value)
}
/// (busy, total) jiffies of the aggregate `cpu` line. The page derives a
/// percentage from two samples, so a single read never claims a load.
fn cpu_ticks(stat: &str) -> Option<(u64, u64)> {
    let fields = stat
        .lines()
        .find(|l| l.split_whitespace().next() == Some("cpu"))?
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(|v| v.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if fields.len() < 5 {
        return None;
    }
    let total = fields.iter().try_fold(0u64, |sum, v| sum.checked_add(*v))?;
    let idle = fields[3].checked_add(fields[4])?;
    Some((total.checked_sub(idle)?, total))
}
fn cpu_cores(stat: &str) -> Option<u64> {
    let cores = stat
        .lines()
        .filter(|l| {
            l.split_whitespace().next().is_some_and(|name| {
                name.len() > 3
                    && name.starts_with("cpu")
                    && name[3..].bytes().all(|b| b.is_ascii_digit())
            })
        })
        .count() as u64;
    (cores > 0).then_some(cores)
}
fn meminfo_bytes(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))?
        .trim()
        .strip_suffix("kB")?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1024)
}
fn first_f64(text: &str) -> Option<f64> {
    text.split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}
fn pretty_name(text: &str) -> Option<String> {
    clean(
        text.lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))?
            .trim_matches('"'),
    )
}
/// Millidegrees to degrees; implausible sensor values are discarded.
fn celsius(text: &str) -> Option<f64> {
    let value = text.trim().parse::<i64>().ok()? as f64 / 1000.0;
    (-40.0..=150.0).contains(&value).then_some(value)
}
/// Prefer a zone named after the CPU; otherwise report the hottest valid zone.
fn pick_temperature(zones: impl IntoIterator<Item = (String, f64)>) -> Option<f64> {
    let (mut cpu, mut hottest) = (None::<f64>, None::<f64>);
    for (name, value) in zones {
        let name = name.to_ascii_lowercase();
        if name.contains("cpu") || name.contains("apc") {
            cpu = Some(cpu.map_or(value, |v| v.max(value)));
        }
        hottest = Some(hottest.map_or(value, |v| v.max(value)));
    }
    cpu.or(hottest)
}
fn temperature() -> Option<f64> {
    let zones = std::fs::read_dir("/sys/class/thermal").ok()?;
    pick_temperature(
        zones
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("thermal_zone"))
            })
            .take(MAX_THERMAL_ZONES)
            .filter_map(|e| {
                let value = celsius(&read_bounded(&e.path().join("temp"))?)?;
                Some((
                    read_bounded(&e.path().join("type")).unwrap_or_default(),
                    value,
                ))
            }),
    )
}
const MAX_PROCESSES: usize = 240;

fn process_row(pid: u32, status: &str) -> Option<Value> {
    let field = |prefix: &str| status.lines().find_map(|line| line.strip_prefix(prefix));
    let name: String = clean(field("Name:")?)?.chars().take(32).collect();
    let state = field("State:")?.trim().chars().next()?;
    let resident_bytes = meminfo_bytes(status, "VmRSS");
    Some(json!({"pid":pid,"name":name,"state":state,"resident_bytes":resident_bytes}))
}

pub(crate) fn processes() -> Option<Value> {
    let proc = Path::new("/proc");
    let mut pids: Vec<u32> = std::fs::read_dir(proc)
        .ok()?
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect();
    pids.sort_unstable();
    let mut rows = pids.into_iter().filter_map(|pid| {
        let status = read_bounded(&proc.join(pid.to_string()).join("status"))?;
        process_row(pid, &status)
    });
    let shown: Vec<_> = rows.by_ref().take(MAX_PROCESSES).collect();
    Some(json!({"processes":shown,"truncated":rows.next().is_some()}))
}

pub(crate) fn snapshot() -> Value {
    let stat = read_bounded(Path::new("/proc/stat")).unwrap_or_default();
    let mem = read_bounded(Path::new("/proc/meminfo")).unwrap_or_default();
    let ticks = cpu_ticks(&stat);
    let storage = rustix::fs::statvfs("/").ok().map(|s| {
        let unit = s.f_frsize;
        (
            s.f_blocks.saturating_mul(unit),
            s.f_bavail.saturating_mul(unit),
        )
    });
    let string = |path: &str| read_bounded(Path::new(path)).and_then(|t| clean(&t));
    json!({
        "schema": 1,
        "firmware_version": env!("CARGO_PKG_VERSION"),
        "model": string("/proc/device-tree/model"),
        "kernel": string("/proc/sys/kernel/osrelease"),
        "os": read_bounded(Path::new("/etc/os-release")).and_then(|t| pretty_name(&t)),
        "cpu_cores": cpu_cores(&stat),
        "cpu_busy_ticks": ticks.map(|t| t.0),
        "cpu_total_ticks": ticks.map(|t| t.1),
        "load_1m": read_bounded(Path::new("/proc/loadavg")).and_then(|t| first_f64(&t)),
        "memory_total_bytes": meminfo_bytes(&mem, "MemTotal"),
        "memory_available_bytes": meminfo_bytes(&mem, "MemAvailable"),
        "storage_total_bytes": storage.map(|s| s.0),
        "storage_free_bytes": storage.map(|s| s.1),
        "temperature_c": temperature(),
        "uptime_seconds": read_bounded(Path::new("/proc/uptime")).and_then(|t| first_f64(&t)).map(|v| v as u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_ticks_count_idle_and_iowait_as_not_busy() {
        let stat = "cpu  10 0 20 60 10 0 0 0 0 0\ncpu0 5 0 10 30 5 0 0 0 0 0\ncpu1 5 0 10 30 5 0 0 0 0 0\nintr 1\n";
        assert_eq!(cpu_ticks(stat), Some((30, 100)));
        assert_eq!(cpu_cores(stat), Some(2));
    }
    #[test]
    fn malformed_cpu_sources_are_unknown_not_zero() {
        for bad in [
            "",
            "cpu  1 2",
            "cpu  a b c d e",
            "cpux 1 2 3 4 5",
            "cpu0 1 2 3 4 5",
        ] {
            assert_eq!(cpu_ticks(bad), None, "{bad}");
        }
        assert_eq!(cpu_cores("cpu  1 2 3 4 5\ncpux 1\n"), None);
        let overflow = format!("cpu  {} {} 0 0 0", u64::MAX, u64::MAX);
        assert_eq!(cpu_ticks(&overflow), None);
    }
    #[test]
    fn meminfo_needs_the_exact_key_and_unit() {
        let text =
            "MemTotal:        1024 kB\nMemAvailable:     512 kB\nMemTotalX: 9 kB\nBad: 1 MB\n";
        assert_eq!(meminfo_bytes(text, "MemTotal"), Some(1024 * 1024));
        assert_eq!(meminfo_bytes(text, "MemAvailable"), Some(512 * 1024));
        assert_eq!(meminfo_bytes(text, "Mem"), None);
        assert_eq!(meminfo_bytes(text, "Bad"), None);
        assert_eq!(
            meminfo_bytes("MemTotal: 99999999999999999999 kB", "MemTotal"),
            None
        );
    }
    #[test]
    fn strings_are_trimmed_bounded_and_control_free() {
        assert_eq!(clean("Board X\0\n").as_deref(), Some("Board X"));
        assert_eq!(clean("\0 \n"), None);
        assert_eq!(clean("a\u{1b}[2Jb").as_deref(), Some("a[2Jb"));
        assert_eq!(clean(&"x".repeat(500)).unwrap().len(), 128);
        assert_eq!(
            pretty_name("NAME=x\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\n").as_deref(),
            Some("Debian GNU/Linux 13 (trixie)")
        );
        assert_eq!(pretty_name("NAME=x\n"), None);
    }
    #[test]
    fn temperature_rejects_implausible_values_and_prefers_cpu_zones() {
        assert_eq!(celsius("46000\n"), Some(46.0));
        for bad in ["", "abc", "200000", "-50000"] {
            assert_eq!(celsius(bad), None, "{bad}");
        }
        assert_eq!(
            pick_temperature([
                ("pa-therm0".to_owned(), 60.0),
                ("cpu0-thermal".to_owned(), 45.0)
            ]),
            Some(45.0)
        );
        assert_eq!(
            pick_temperature([("battery".to_owned(), 31.0), ("pa".to_owned(), 38.0)]),
            Some(38.0)
        );
        assert_eq!(pick_temperature([]), None);
    }
    #[test]
    fn loadavg_and_uptime_reject_non_numbers() {
        assert_eq!(first_f64("0.42 0.3 0.2 1/90 1234\n"), Some(0.42));
        for bad in ["", "NaN 1", "inf", "-1"] {
            assert_eq!(first_f64(bad), None, "{bad}");
        }
    }
    #[test]
    fn snapshot_has_a_fixed_shape_without_identifiers() {
        let value = snapshot();
        let keys: std::collections::BTreeSet<_> =
            value.as_object().unwrap().keys().cloned().collect();
        let expected: std::collections::BTreeSet<_> = [
            "schema",
            "firmware_version",
            "model",
            "kernel",
            "os",
            "cpu_cores",
            "cpu_busy_ticks",
            "cpu_total_ticks",
            "load_1m",
            "memory_total_bytes",
            "memory_available_bytes",
            "storage_total_bytes",
            "storage_free_bytes",
            "temperature_c",
            "uptime_seconds",
        ]
        .map(str::to_owned)
        .into_iter()
        .collect();
        assert_eq!(keys, expected);
        assert_eq!(value["firmware_version"], env!("CARGO_PKG_VERSION"));
    }
}
