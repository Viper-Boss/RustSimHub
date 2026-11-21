// SPDX-License-Identifier: AGPL-3.0-only
//! Read only the allocation byte of each EF_SMS record on the supported modem.
use super::{
    bounded_command, checked, collect_modem, directory, publish, sim_key, text, uptime_ms,
};
use super::{Result, BASE, OUTPUT};
use rustsimhub_network_agent::State;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::FileTypeExt,
    path::Path,
    time::{Duration, Instant},
};

const FILE: &str = "sim-sms-storage.json";
const PORT: &str = "wwan0qmi0";
const DEVICE: &str = "/dev/wwan0qmi0";
const MAX_RECORDS: u16 = 254;
const MAX_AGE_MS: u64 = 300_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capacity {
    used_slots: u16,
    total_slots: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sample {
    boot: String,
    sim: String,
    sampled_at_ms: u64,
    capacity: Capacity,
}

fn field<'a>(response: &'a str, key: &str) -> Option<&'a str> {
    response.lines().find_map(|line| {
        let (field, value) = line.trim().split_once(':')?;
        (field == key).then(|| value.trim())
    })
}

fn slots(response: &str) -> Option<u16> {
    if field(response, "File ID")? != "28476"
        || field(response, "File type")? != "linear-fixed"
        || field(response, "Record size")? != "176"
    {
        return None;
    }
    let count = field(response, "Record count")?.parse().ok()?;
    (1..=MAX_RECORDS).contains(&count).then_some(count)
}

fn record_used(response: &str) -> Option<bool> {
    let status = response
        .lines()
        .skip_while(|line| line.trim() != "Read result:")
        .nth(1)?
        .trim();
    if status.len() != 2 {
        return None;
    }
    // EF_SMS bit 1 is set for allocated records, irrespective of read/send state.
    Some(u8::from_str_radix(status, 16).ok()? & 1 != 0)
}

fn read_capacity(
    total_slots: u16,
    mut record: impl FnMut(u16) -> Result<String>,
) -> Result<Capacity> {
    let start = Instant::now();
    let mut used_slots = 0;
    for index in 1..=total_slots {
        if start.elapsed() > Duration::from_secs(35) {
            return Err("SIM storage sampling deadline".into());
        }
        let used = record_used(&record(index)?).ok_or("invalid SIM record status")?;
        used_slots += u16::from(used);
    }
    Ok(Capacity {
        used_slots,
        total_slots,
    })
}

fn qmi(query: &str) -> Result<String> {
    bounded_command("/usr/bin/qmicli", &["-d", DEVICE, "-p", query], None)
}

fn sim_binding(salt: &str) -> Result<(String, String)> {
    let (modem, info) = collect_modem()?;
    if text(&info, "/modem/generic/primary-port", 32).as_deref() != Some(PORT) {
        return Err("SIM storage port is unsupported".into());
    }
    Ok((modem, sim_key(&info, salt)?))
}

pub(super) fn collect() -> Result<()> {
    if !fs::symlink_metadata(DEVICE)?.file_type().is_char_device() {
        return Err("SIM storage device is unavailable".into());
    }
    directory(OUTPUT, 0o750)?;
    let state: State =
        serde_json::from_slice(&checked(&Path::new(BASE).join("meter.json"), 0, 0o600)?)?;
    if !state.valid() {
        return Err("invalid meter identity".into());
    }
    let binding = sim_binding(&state.salt)?;
    let sampled_at_ms = uptime_ms()?;
    let attrs = qmi("--uim-get-file-attributes=0x3F00,0x7FFF,0x6F3C")?;
    let total_slots = slots(&attrs).ok_or("invalid EF_SMS attributes")?;
    let capacity = read_capacity(total_slots, |index| {
        qmi(&format!(
            "--uim-read-record=record-number={index},record-length=1,file=0x3F00-0x7FFF-0x6F3C"
        ))
    })?;
    if sim_binding(&state.salt)? != binding {
        return Err("SIM changed during capacity sample".into());
    }
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    publish(
        OUTPUT,
        FILE,
        &Sample {
            boot: boot.trim().to_owned(),
            sim: binding.1,
            sampled_at_ms,
            capacity,
        },
        0o640,
    )
}

fn valid_sample(sample: Sample, boot: &str, sim: &str, now: u64) -> Option<Capacity> {
    let age = now.checked_sub(sample.sampled_at_ms)?;
    (sample.boot == boot
        && !sim.is_empty()
        && sample.sim == sim
        && age <= MAX_AGE_MS
        && (1..=MAX_RECORDS).contains(&sample.capacity.total_slots)
        && sample.capacity.used_slots <= sample.capacity.total_slots)
        .then_some(sample.capacity)
}

pub(super) fn current(boot: &str, sim: &str) -> Option<Capacity> {
    let bytes = checked(&Path::new(OUTPUT).join(FILE), 0, 0o640).ok()?;
    valid_sample(
        serde_json::from_slice(&bytes).ok()?,
        boot,
        sim,
        uptime_ms().ok()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE_ATTRIBUTES: &str =
        "File attributes:\n  File ID: 28476\n  File type: linear-fixed\n  Record size: 176\n  Record count: 50\n";

    #[test]
    fn accepts_only_bounded_linear_fixed_sms_storage() {
        assert_eq!(slots(LIVE_ATTRIBUTES), Some(50));
        assert_eq!(slots(&LIVE_ATTRIBUTES.replace("50", "255")), None);
        assert_eq!(slots(&LIVE_ATTRIBUTES.replace("28476", "28477")), None);
        assert_eq!(slots(&LIVE_ATTRIBUTES.replace("176", "177")), None);
    }

    #[test]
    fn full_partial_and_invalid_slot_responses() {
        assert_eq!(
            read_capacity(50, |_| Ok("Read result:\n\t01\n".into())).unwrap(),
            Capacity {
                used_slots: 50,
                total_slots: 50
            },
        );
        let partial = read_capacity(4, |n| {
            Ok(format!(
                "Read result:\n\t{:02x}\n",
                [0, 1, 3, 4][usize::from(n - 1)]
            ))
        })
        .unwrap();
        assert_eq!(partial.used_slots, 2);
        assert!(read_capacity(1, |_| Ok("Read result:\n\t00:01\n".into())).is_err());
        assert!(read_capacity(1, |_| Ok("no status bytes".into())).is_err());
    }

    #[test]
    fn stale_or_different_line_reports_do_not_leak_into_status() {
        let sample = |capacity| Sample {
            boot: "boot".into(),
            sim: "sim".into(),
            sampled_at_ms: 100,
            capacity,
        };
        let cap = Capacity {
            used_slots: 50,
            total_slots: 50,
        };
        assert_eq!(
            valid_sample(sample(cap.clone()), "boot", "sim", 500),
            Some(cap.clone())
        );
        for (boot, sim, now) in [
            ("other", "sim", 500),
            ("boot", "other", 500),
            ("boot", "", 500),
            ("boot", "sim", 99),
            ("boot", "sim", MAX_AGE_MS + 101),
        ] {
            assert_eq!(valid_sample(sample(cap.clone()), boot, sim, now), None);
        }
        assert!(!serde_json::to_string(&cap).unwrap().contains("sim"));
    }
}
