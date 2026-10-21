// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Deterministic indicator policy. Inputs contain no message content or identifiers.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Off,
    Red,
    Green,
    Blue,
    Yellow,
    Cyan,
    Purple,
    White,
}
impl Color {
    pub fn channels(self) -> [bool; 3] {
        use Color::*;
        match self {
            Off => [false, false, false],
            Red => [true, false, false],
            Green => [false, true, false],
            Blue => [false, false, true],
            Yellow => [true, true, false],
            Cyan => [false, true, true],
            Purple => [true, false, true],
            White => [true, true, true],
        }
    }
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessInput {
    pub expires_ms: u64,
    pub configuring: bool,
    pub unread_sms: bool,
    pub missed_calls: bool,
}
impl BusinessInput {
    pub fn fresh(self, now: u64) -> Option<Self> {
        (self.expires_ms > now && self.expires_ms.saturating_sub(now) <= 30_000).then_some(self)
    }
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateInput {
    pub expires_ms: u64,
    pub available: bool,
    pub upgrading: bool,
}
impl UpdateInput {
    pub fn fresh(self, now: u64) -> Option<Self> {
        (self.expires_ms > now && self.expires_ms.saturating_sub(now) <= 30_000).then_some(self)
    }
}

#[derive(Clone, Copy, Default)]
pub struct Inputs {
    pub disabled: bool,
    pub error: bool,
    pub configuring: bool,
    pub upgrading: bool,
    pub update_available: bool,
    pub unread_sms: bool,
    pub missed_calls: bool,
    pub cellular_connected: bool,
    pub wifi_connected: bool,
    pub internet: bool,
}

/// The Root updater publishes this record only after signature/payload validation.
/// The caller must first validate the fixed file's Root ownership and metadata.
pub fn staged_update(value: &serde_json::Value) -> Option<(bool, bool)> {
    fn hash(v: &serde_json::Value) -> bool {
        v.as_str().is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
    }
    if value.get("format")?.as_u64()? != 1 || !hash(value.get("key_sha256")?) {
        return None;
    }
    let candidate = value.get("candidate")?;
    if candidate.is_null() {
        return Some((false, false));
    }
    let epoch = candidate.get("epoch")?.as_u64()?;
    let confirmed = value.get("confirmed")?;
    let floor = value.get("floor")?.as_u64()?;
    let old = if confirmed.is_null() {
        0
    } else {
        confirmed.get("epoch")?.as_u64()?
    };
    if epoch == 0 || epoch > floor || epoch <= old || !hash(candidate.get("hash")?) {
        return None;
    }
    let attempted = value.get("attempted")?.as_bool()?;
    Some((!attempted, attempted))
}

#[derive(Default)]
pub struct Policy {
    io_counter: Option<u64>,
    io_started: Option<u64>,
    last_io: Option<u64>,
    unread_anchor: u64,
    unread: (bool, bool),
}
fn pulse(color: Color, time: u64) -> Color {
    // This GPIO board has max_brightness=1: slow pulses, not fake PWM fading.
    if time % 6_000 < 2_000 {
        color
    } else {
        Color::Off
    }
}
impl Policy {
    pub fn frame(&mut self, input: Inputs, now: u64, counter: Option<u64>) -> Color {
        use Color::*;
        if let Some(value) = counter {
            if self.io_counter.is_some_and(|old| value > old)
                && self
                    .last_io
                    .is_none_or(|old| now.saturating_sub(old) >= 5_000)
            {
                self.io_started = Some(now);
                self.last_io = Some(now);
            }
            self.io_counter = Some(value);
        }
        let unread = (input.unread_sms, input.missed_calls);
        if unread != self.unread {
            self.unread_anchor = now;
            self.unread = unread;
        }
        if input.disabled {
            return Off;
        }
        if input.error {
            return Red;
        }
        if input.upgrading {
            return pulse(Purple, now);
        }
        if input.configuring {
            return pulse(White, now);
        }
        let base = if !input.internet {
            Yellow
        } else if input.cellular_connected {
            Green
        } else if input.wifi_connected {
            Blue
        } else {
            Yellow
        };
        let t = now.saturating_sub(self.unread_anchor) % 30_000;
        let mut sequence = [Off; 6];
        let mut len = 0;
        if unread.0 {
            sequence[..3].copy_from_slice(&[Blue, Cyan, Green]);
            len += 3;
        }
        if unread.1 {
            sequence[len..len + 3].copy_from_slice(&[Red, Yellow, Purple]);
            len += 3;
        }
        if t < (len as u64) * 300 {
            return if t % 300 < 280 {
                sequence[(t / 300) as usize]
            } else {
                Off
            };
        }
        if input.update_available && now % 10_000 < 200 {
            return Purple;
        }
        if self
            .io_started
            .is_some_and(|old| now.saturating_sub(old) < 120)
        {
            return Cyan;
        }
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn online() -> Inputs {
        Inputs {
            wifi_connected: true,
            internet: true,
            ..Inputs::default()
        }
    }
    #[test]
    fn link_priority_and_offline_internet() {
        let mut p = Policy::default();
        let mut s = online();
        assert_eq!(p.frame(s, 0, None), Color::Blue);
        s.cellular_connected = true;
        assert_eq!(p.frame(s, 0, None), Color::Green);
        s.internet = false;
        assert_eq!(p.frame(s, 0, None), Color::Yellow);
        s.wifi_connected = false;
        s.cellular_connected = false;
        assert_eq!(p.frame(s, 0, None), Color::Yellow);
    }
    #[test]
    fn error_upgrade_configuration_priority() {
        let mut p = Policy::default();
        let mut s = online();
        s.configuring = true;
        s.upgrading = true;
        s.error = true;
        assert_eq!(p.frame(s, 0, None), Color::Red);
        s.error = false;
        assert_eq!(p.frame(s, 0, None), Color::Purple);
        s.upgrading = false;
        assert_eq!(p.frame(s, 0, None), Color::White);
        assert_eq!(p.frame(s, 3_000, None), Color::Off);
        s.disabled = true;
        assert_eq!(p.frame(s, 0, None), Color::Off);
    }
    #[test]
    fn io_is_short_and_throttled_to_five_seconds() {
        let mut p = Policy::default();
        let s = online();
        assert_eq!(p.frame(s, 0, Some(10)), Color::Blue);
        assert_eq!(p.frame(s, 1_000, Some(11)), Color::Cyan);
        assert_eq!(p.frame(s, 1_121, Some(12)), Color::Blue);
        assert_eq!(p.frame(s, 5_999, Some(13)), Color::Blue);
        assert_eq!(p.frame(s, 6_000, Some(14)), Color::Cyan);
        // A counter reset cannot create a false activity pulse.
        assert_eq!(p.frame(s, 8_000, Some(0)), Color::Blue);
    }
    #[test]
    fn notifications_are_nine_tenths_and_one_point_eight_seconds() {
        let mut p = Policy::default();
        let mut s = online();
        s.unread_sms = true;
        s.missed_calls = true;
        let colors = [
            Color::Blue,
            Color::Cyan,
            Color::Green,
            Color::Red,
            Color::Yellow,
            Color::Purple,
        ];
        for (i, color) in colors.iter().enumerate() {
            assert_eq!(p.frame(s, (i as u64) * 300, None), *color);
        }
        assert_eq!(p.frame(s, 1_800, None), Color::Blue);
        assert_eq!(p.frame(s, 29_999, None), Color::Blue);
        assert_eq!(p.frame(s, 30_300, None), Color::Cyan);
        s.unread_sms = false;
        s.missed_calls = false;
        assert_eq!(p.frame(s, 30_310, None), Color::Blue);
    }
    #[test]
    fn individual_sequences_and_dark_separators() {
        for (sms, colors) in [
            (true, [Color::Blue, Color::Cyan, Color::Green]),
            (false, [Color::Red, Color::Yellow, Color::Purple]),
        ] {
            let mut p = Policy::default();
            let s = Inputs {
                unread_sms: sms,
                missed_calls: !sms,
                ..online()
            };
            for (i, color) in colors.iter().enumerate() {
                assert_eq!(p.frame(s, i as u64 * 300, None), *color);
                assert_eq!(p.frame(s, i as u64 * 300 + 280, None), Color::Off);
            }
            assert_eq!(p.frame(s, 900, None), Color::Blue);
        }
    }
    #[test]
    fn stale_future_and_malformed_leases_are_not_accepted() {
        let mut s = BusinessInput {
            expires_ms: 20_000,
            ..BusinessInput::default()
        };
        assert!(s.fresh(1).is_some());
        assert!(s.fresh(20_000).is_none());
        s.expires_ms = 40_001;
        assert!(s.fresh(10_000).is_none());
        assert!(serde_json::from_str::<BusinessInput>(r#"{"expires_ms":1,"configuring":true,"unread_sms":false,"missed_calls":false,"path":"/etc/passwd"}"#).is_err());
    }
    #[test]
    fn new_firmware_hint_requires_valid_newer_staged_root_record() {
        let hash = "a".repeat(64);
        let mut value = serde_json::json!({"format":1,"key_sha256":hash,"floor":3,"confirmed":{"epoch":2,"hash":"b".repeat(64)},"candidate":{"epoch":3,"hash":"c".repeat(64)},"attempted":false});
        assert_eq!(staged_update(&value), Some((true, false)));
        value["attempted"] = true.into();
        assert_eq!(staged_update(&value), Some((false, true)));
        value["candidate"]["epoch"] = 2.into();
        assert_eq!(staged_update(&value), None);
        value["candidate"]["epoch"] = 4.into();
        assert_eq!(staged_update(&value), None);
        value["candidate"] = serde_json::Value::Null;
        assert_eq!(staged_update(&value), Some((false, false)));
    }
    #[test]
    fn available_notice_is_infrequent_and_does_not_replace_configuration() {
        let mut p = Policy::default();
        let mut s = online();
        s.update_available = true;
        assert_eq!(p.frame(s, 0, None), Color::Purple);
        assert_eq!(p.frame(s, 200, None), Color::Blue);
        assert_eq!(p.frame(s, 9_999, None), Color::Blue);
        assert_eq!(p.frame(s, 10_000, None), Color::Purple);
        s.configuring = true;
        assert_eq!(p.frame(s, 12_000, None), Color::White);
    }
}
