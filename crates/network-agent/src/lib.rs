// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Calendar accounting and fixed nftables rules. No shell or arbitrary commands.
use serde::{Deserialize, Serialize};
use time::{Date, Month, OffsetDateTime, UtcOffset};

pub const MAX_LIMIT: u64 = 100 * 1024 * 1024 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub enabled: bool,
    pub limit_bytes: u64,
    pub reset_day: u8,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            limit_bytes: 10 * 1024 * 1024 * 1024,
            reset_day: 1,
        }
    }
}
impl Policy {
    pub fn valid(&self) -> bool {
        (1..=31).contains(&self.reset_day) && (1..=MAX_LIMIT).contains(&self.limit_bytes)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Day {
    pub sim: String,
    pub day: i32,
    pub rx: u64,
    pub tx: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub schema: u16,
    pub policy: Policy,
    pub days: Vec<Day>,
    pub counter_boot: String,
    pub last_rx: u64,
    pub last_tx: u64,
    pub last_day: i32,
    pub active_sim: String,
    pub last_request: String,
    pub uncertain: bool,
    pub was_running: bool,
    pub salt: String,
}
impl State {
    pub fn new(salt: String) -> Self {
        Self {
            schema: 1,
            policy: Policy::default(),
            days: vec![],
            counter_boot: String::new(),
            last_rx: 0,
            last_tx: 0,
            last_day: 0,
            active_sim: String::new(),
            last_request: String::new(),
            uncertain: false,
            was_running: false,
            salt,
        }
    }
    pub fn valid(&self) -> bool {
        self.schema == 1
            && self.policy.valid()
            && self.days.len() <= 3200
            && self.salt.len() == 64
            && self.salt.bytes().all(|b| b.is_ascii_hexdigit())
            && self
                .days
                .iter()
                .all(|d| d.sim.len() == 64 && d.sim.bytes().all(|b| b.is_ascii_hexdigit()))
            && self.active_sim.len() <= 64
            && self.last_request.len() <= 64
            && self.counter_boot.len() <= 36
    }
    // New boots / replaced tables must not subtract counters or grant a fresh quota.
    pub fn sample(
        &mut self,
        boot: &str,
        sim: &str,
        now: i64,
        rx: u64,
        tx: u64,
    ) -> Result<(), &'static str> {
        let date = local_date(now)?;
        let day = date.to_julian_day();
        if self.last_day > day {
            self.uncertain = true;
            return Err("clock moved backwards");
        }
        let same = self.counter_boot == boot;
        if same && (rx < self.last_rx || tx < self.last_tx) {
            self.uncertain = true;
            return Err("counter decreased");
        }
        let (dr, dt) = if same {
            (rx - self.last_rx, tx - self.last_tx)
        } else {
            (rx, tx)
        };
        // Attribute final deltas to the previous SIM before changing to a new one.
        let key = if self.active_sim.is_empty() || !same {
            sim
        } else {
            &self.active_sim
        };
        if !key.is_empty() && (dr > 0 || dt > 0) {
            if let Some(row) = self.days.iter_mut().find(|d| d.sim == key && d.day == day) {
                row.rx = row.rx.checked_add(dr).ok_or("counter overflow")?;
                row.tx = row.tx.checked_add(dt).ok_or("counter overflow")?;
            } else {
                self.days.push(Day {
                    sim: key.to_owned(),
                    day,
                    rx: dr,
                    tx: dt,
                });
            }
        }
        self.days.retain(|d| d.day >= day - 399);
        if self.days.len() > 3200 {
            self.uncertain = true;
            return Err("accounting catalogue full");
        }
        self.last_day = day;
        self.counter_boot = boot.to_owned();
        self.last_rx = rx;
        self.last_tx = tx;
        if !sim.is_empty() {
            self.active_sim = sim.to_owned();
        }
        Ok(())
    }
    pub fn usage(&self, now: i64) -> Result<Usage, &'static str> {
        let (start, end) = period(now, self.policy.reset_day)?;
        let mut rx = 0u64;
        let mut tx = 0u64;
        for d in self.days.iter().filter(|d| {
            d.sim == self.active_sim
                && d.day >= start.to_julian_day()
                && d.day < end.to_julian_day()
        }) {
            rx = rx.checked_add(d.rx).ok_or("usage overflow")?;
            tx = tx.checked_add(d.tx).ok_or("usage overflow")?;
        }
        let total = rx.checked_add(tx).ok_or("usage overflow")?;
        Ok(Usage {
            period_start: start.to_string(),
            period_end: end.to_string(),
            timezone: "Asia/Shanghai",
            rx_bytes: rx,
            tx_bytes: tx,
            total_bytes: total,
            remaining_bytes: self.policy.limit_bytes.saturating_sub(total),
            limit_reached: self.policy.enabled
                && (total >= self.policy.limit_bytes || self.uncertain),
            uncertain: self.uncertain,
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Usage {
    pub period_start: String,
    pub period_end: String,
    pub timezone: &'static str,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub total_bytes: u64,
    pub remaining_bytes: u64,
    pub limit_reached: bool,
    pub uncertain: bool,
}
fn local_date(now: i64) -> Result<Date, &'static str> {
    let date = OffsetDateTime::from_unix_timestamp(now)
        .map_err(|_| "invalid clock")?
        .to_offset(UtcOffset::from_hms(8, 0, 0).unwrap())
        .date();
    if date.year() < 2024 || date.year() > 2200 {
        return Err("clock not synchronized");
    }
    Ok(date)
}
fn boundary(year: i32, month: Month, day: u8) -> Date {
    let mut d = day;
    loop {
        if let Ok(v) = Date::from_calendar_date(year, month, d) {
            return v;
        }
        d -= 1;
    }
}
pub fn period(now: i64, reset: u8) -> Result<(Date, Date), &'static str> {
    if !(1..=31).contains(&reset) {
        return Err("invalid reset day");
    }
    let date = local_date(now)?;
    let (mut y, mut m) = (date.year(), date.month());
    if date < boundary(y, m, reset) {
        if m == Month::January {
            y -= 1;
        }
        m = m.previous();
    }
    let start = boundary(y, m, reset);
    let end = boundary(
        if m == Month::December { y + 1 } else { y },
        m.next(),
        reset,
    );
    Ok((start, end))
}
pub fn interface_valid(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 15
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
pub fn cellular_interface(s: &str) -> bool {
    s.len() == 5 && s.starts_with("wwan") && (b'0'..=b'7').contains(&s.as_bytes()[4])
}
/// Only our named table is touched. Named counters survive quota / interface changes.
pub fn rules(
    ifaces: &[String],
    remaining: Option<u64>,
    create: bool,
) -> Result<String, &'static str> {
    if ifaces.len() > 8 || ifaces.iter().any(|s| !cellular_interface(s)) {
        return Err("invalid cellular interface");
    }
    let mut s = String::new();
    if create {
        s.push_str("add table inet rustsimhub_net\nadd counter inet rustsimhub_net rx\nadd counter inet rustsimhub_net tx\nadd chain inet rustsimhub_net rx { type filter hook prerouting priority -10; policy accept; }\nadd chain inet rustsimhub_net tx { type filter hook postrouting priority -10; policy accept; }\n");
    } else {
        s.push_str("flush chain inet rustsimhub_net rx\nflush chain inet rustsimhub_net tx\ndelete quota inet rustsimhub_net cap\n");
    }
    s.push_str(&format!(
        "add quota inet rustsimhub_net cap {{ over {} bytes; }}\n",
        remaining.unwrap_or(MAX_LIMIT).max(1)
    ));
    for iface in ifaces {
        for (chain, meta) in [("rx", "iifname"), ("tx", "oifname")] {
            if remaining == Some(0) {
                s.push_str(&format!(
                    "add rule inet rustsimhub_net {chain} {meta} \"{iface}\" drop\n"
                ));
            } else {
                if remaining.is_some() {
                    s.push_str(&format!("add rule inet rustsimhub_net {chain} {meta} \"{iface}\" quota name cap drop\n"));
                }
                s.push_str(&format!("add rule inet rustsimhub_net {chain} {meta} \"{iface}\" counter name {chain}\n"));
            }
        }
    }
    Ok(s)
}
#[derive(Debug, Serialize)]
pub struct LteChannel {
    pub band: u32,
    pub downlink_mhz: f64,
    pub uplink_mhz: Option<f64>,
}
pub fn lte_channel(earfcn: u32) -> Option<LteChannel> {
    // 3GPP TS 36.101 table 5.7.3-1, limited explicitly to these ranges.
    for (band, lo, hi, base, duplex) in [
        (1, 0, 599, 2110., Some(190.)),
        (3, 1200, 1949, 1805., Some(95.)),
        (5, 2400, 2649, 869., Some(45.)),
        (8, 3450, 3799, 925., Some(45.)),
        (40, 38650, 39649, 2300., None),
        (41, 39650, 41589, 2496., None),
    ] {
        if (lo..=hi).contains(&earfcn) {
            let down = base + f64::from(earfcn - lo) * 0.1;
            return Some(LteChannel {
                band,
                downlink_mhz: down,
                uplink_mhz: duplex.map(|d| down - d),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stamp(y: i32, m: Month, d: u8) -> i64 {
        Date::from_calendar_date(y, m, d)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_utc()
            .unix_timestamp()
    }
    #[test]
    fn calendar_clamps_31_and_handles_year_leap_and_local_midnight() {
        let (a, b) = period(stamp(2028, Month::February, 29), 31).unwrap();
        assert_eq!(a.to_string(), "2028-02-29");
        assert_eq!(b.to_string(), "2028-03-31");
        let (a, b) = period(stamp(2027, Month::January, 1), 15).unwrap();
        assert_eq!(a.to_string(), "2026-12-15");
        assert_eq!(b.to_string(), "2027-01-15");
        let now = Date::from_calendar_date(2026, Month::October, 31)
            .unwrap()
            .with_hms(17, 0, 0)
            .unwrap()
            .assume_utc()
            .unix_timestamp();
        assert_eq!(period(now, 1).unwrap().0.to_string(), "2026-11-01");
        assert!(period(0, 1).is_err());
        assert!(period(now, 0).is_err());
    }
    #[test]
    fn durable_counters_do_not_recount_or_reset_on_reload_and_sim_change() {
        let now = stamp(2026, Month::October, 22);
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let mut s = State::new("c".repeat(64));
        s.sample("one", &a, now, 100, 50).unwrap();
        s.sample("one", &a, now, 100, 50).unwrap();
        assert_eq!(s.usage(now).unwrap().total_bytes, 150);
        let mut s: State = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        s.sample("two", &a, now, 10, 10).unwrap();
        assert_eq!(s.usage(now).unwrap().total_bytes, 170);
        s.sample("two", &b, now, 20, 20).unwrap();
        assert_eq!(s.usage(now).unwrap().total_bytes, 0);
        s.sample("two", &b, now, 30, 30).unwrap();
        assert_eq!(s.usage(now).unwrap().total_bytes, 20);
        assert!(s.sample("two", &b, now, 1, 1).is_err());
        assert!(s.uncertain);
    }
    #[test]
    fn cap_clock_and_cycle_changes_preserve_history() {
        let now = stamp(2026, Month::October, 22);
        let mut s = State::new("f".repeat(64));
        s.sample("boot", &"a".repeat(64), now, 200, 50).unwrap();
        s.policy = Policy {
            enabled: true,
            limit_bytes: 200,
            reset_day: 1,
        };
        assert!(s.usage(now).unwrap().limit_reached);
        s.policy.reset_day = 15;
        assert_eq!(s.usage(now).unwrap().total_bytes, 250);
        assert_eq!(
            s.usage(stamp(2026, Month::November, 15))
                .unwrap()
                .total_bytes,
            0
        );
        assert!(s
            .sample("boot", &"a".repeat(64), now - 86400, 200, 50)
            .is_err());
        assert!(s.usage(now).unwrap().limit_reached);
    }
    #[test]
    fn rules_are_confined_and_quota_shared_across_directions() {
        for bad in ["usb0", "wlan0", "wwan8", "wwan0\nflush ruleset", "../wwan0"] {
            assert!(rules(&[bad.into()], Some(10), true).is_err());
        }
        let s = rules(&["wwan0".into()], Some(100), false).unwrap();
        assert_eq!(s.matches("quota name cap drop").count(), 2);
        assert!(!s.contains("flush ruleset"));
        assert!(!s.contains("reset counter"));
        assert!(rules(&["wwan0".into()], Some(0), true)
            .unwrap()
            .contains("\"wwan0\" drop"));
        assert!(!rules(&["wwan0".into()], None, true)
            .unwrap()
            .contains("quota name"));
    }
    #[test]
    fn lte_frequency_is_derived_from_observed_channel_not_allowed_band_list() {
        let c = lte_channel(1300).unwrap();
        assert_eq!(c.band, 3);
        assert_eq!(c.downlink_mhz, 1815.);
        assert_eq!(c.uplink_mhz, Some(1720.));
        assert!(lte_channel(999999).is_none());
    }
}
