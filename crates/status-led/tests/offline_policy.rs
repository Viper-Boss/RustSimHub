// SPDX-License-Identifier: AGPL-3.0-only
//! Public policy contracts; no sysfs, subprocesses, network or sleeping.
use rustsimhub_status_led::{staged_update, BusinessInput, Color, Inputs, Policy, UpdateInput};

fn online() -> Inputs {
    Inputs {
        internet: true,
        wifi_connected: true,
        ..Inputs::default()
    }
}

#[test]
fn all_rgb_channels_and_network_combinations_keep_the_fixed_user_palette() {
    for (color, channels) in [
        (Color::Off, [false, false, false]),
        (Color::Red, [true, false, false]),
        (Color::Green, [false, true, false]),
        (Color::Blue, [false, false, true]),
        (Color::Yellow, [true, true, false]),
        (Color::Cyan, [false, true, true]),
        (Color::Purple, [true, false, true]),
        (Color::White, [true, true, true]),
    ] {
        assert_eq!(color.channels(), channels);
    }
    for (internet, wifi, cellular, color) in [
        (false, false, false, Color::Yellow),
        (false, true, false, Color::Yellow),
        (false, false, true, Color::Yellow),
        (false, true, true, Color::Yellow),
        (true, false, false, Color::Yellow),
        (true, true, false, Color::Blue),
        (true, false, true, Color::Green),
        (true, true, true, Color::Green),
    ] {
        let input = Inputs {
            internet,
            wifi_connected: wifi,
            cellular_connected: cellular,
            ..Inputs::default()
        };
        assert_eq!(Policy::default().frame(input, 301, None), color);
    }
}

#[test]
fn lease_boundaries_reject_expired_far_future_unknown_and_malformed_inputs() {
    for (now, expires, valid) in [
        (0, 0, false),
        (1, 1, false),
        (1, 2, true),
        (1, 30_001, true),
        (1, 30_002, false),
        (u64::MAX, u64::MAX, false),
        (u64::MAX - 30_000, u64::MAX, true),
    ] {
        let b = BusinessInput {
            expires_ms: expires,
            configuring: true,
            unread_sms: true,
            missed_calls: true,
            ringing: true,
        };
        let u = UpdateInput {
            expires_ms: expires,
            available: true,
            upgrading: true,
        };
        assert_eq!(b.fresh(now).is_some(), valid);
        assert_eq!(u.fresh(now).is_some(), valid);
    }
    for json in [
        "null",
        "[]",
        "{}",
        r#"{"expires_ms":-1,"configuring":true,"unread_sms":false,"missed_calls":false}"#,
        r#"{"expires_ms":18446744073709551616,"configuring":true,"unread_sms":false,"missed_calls":false}"#,
        r#"{"expires_ms":1,"configuring":1,"unread_sms":false,"missed_calls":false}"#,
        r#"{"expires_ms":1,"configuring":false,"unread_sms":false,"missed_calls":false,"line":1}"#,
        r#"{"expires_ms":1,"expires_ms":2,"configuring":false,"unread_sms":false,"missed_calls":false}"#,
        r#"{"expires_ms":1,"configuring":false,"unread_sms":false,"missed_calls":false,"ringing":"true"}"#,
    ] {
        assert!(serde_json::from_str::<BusinessInput>(json).is_err());
    }
    for bytes in [&b"\xff\xfe"[..], &b"{\"expires_ms\":"[..]] {
        assert!(serde_json::from_slice::<BusinessInput>(bytes).is_err());
    }
    assert!(serde_json::from_str::<UpdateInput>(
        r#"{"expires_ms":1,"available":true,"upgrading":false,"path":"synthetic"}"#
    )
    .is_err());
}

#[test]
fn dominant_modes_cannot_be_overridden_by_any_lower_status_flags() {
    for mask in 0..256 {
        let lower = Inputs {
            unread_sms: mask & 1 != 0,
            missed_calls: mask & 2 != 0,
            update_available: mask & 4 != 0,
            cellular_connected: mask & 8 != 0,
            wifi_connected: mask & 16 != 0,
            internet: mask & 32 != 0,
            configuring: mask & 64 != 0,
            upgrading: mask & 128 != 0,
            ..Inputs::default()
        };
        for time in [0, 199, 200, 3000, 6000, 10_000] {
            let mut p = Policy::default();
            let off = Inputs {
                disabled: true,
                ringing: true,
                error: true,
                ..lower
            };
            assert_eq!(p.frame(off, time, Some(0)), Color::Off);
            let ringing = Inputs {
                ringing: true,
                error: true,
                ..lower
            };
            assert_eq!(
                p.frame(ringing, time, None),
                if time % 400 < 200 {
                    Color::White
                } else {
                    Color::Off
                }
            );
            let error = Inputs {
                error: true,
                ..lower
            };
            assert_eq!(p.frame(error, time, None), Color::Red);
            let upgrading = Inputs {
                upgrading: true,
                ..lower
            };
            assert_eq!(
                p.frame(upgrading, time, None),
                if time % 6000 < 2000 {
                    Color::Purple
                } else {
                    Color::Off
                }
            );
        }
    }
}

#[test]
fn combined_notice_finishes_under_two_seconds_and_acknowledgement_resets_only_the_needed_group() {
    let mut p = Policy::default();
    let mut s = Inputs {
        unread_sms: true,
        missed_calls: true,
        ..online()
    };
    let palette = [
        Color::Red,
        Color::Yellow,
        Color::Purple,
        Color::Red,
        Color::Yellow,
        Color::Purple,
    ];
    for (i, color) in palette.into_iter().enumerate() {
        assert_eq!(p.frame(s, 1000 + i as u64 * 300, None), color);
        assert_eq!(p.frame(s, 1000 + i as u64 * 300 + 279, None), color);
        assert_eq!(p.frame(s, 1000 + i as u64 * 300 + 280, None), Color::Off);
    }
    assert_eq!(p.frame(s, 2800, None), Color::Blue);
    assert_eq!(p.frame(s, 30_999, None), Color::Blue);
    s.unread_sms = false;
    assert_eq!(p.frame(s, 31_000, None), Color::Red);
    assert_eq!(p.frame(s, 31_300, None), Color::Purple);
    assert_eq!(p.frame(s, 31_600, None), Color::Red);
    s.missed_calls = false;
    assert_eq!(p.frame(s, 31_601, None), Color::Blue);
}

#[test]
fn configuration_suppresses_notifications_and_returns_to_current_network_without_a_stuck_white_hint(
) {
    let mut p = Policy::default();
    let mut s = Inputs {
        configuring: true,
        unread_sms: true,
        missed_calls: true,
        ..online()
    };
    assert_eq!(p.frame(s, 0, None), Color::White);
    assert_eq!(p.frame(s, 3000, None), Color::Off);
    s.configuring = false;
    s.cellular_connected = true;
    assert_eq!(p.frame(s, 6000, None), Color::Green);
    s.internet = false;
    assert_eq!(p.frame(s, 6010, None), Color::Yellow);
    s.disabled = true;
    assert_eq!(p.frame(s, 30_000, None), Color::Off);
    s.disabled = false;
    s.unread_sms = false;
    s.missed_calls = false;
    assert_eq!(p.frame(s, 30_001, None), Color::Yellow);
}

#[test]
fn io_and_available_firmware_pulses_keep_their_sparse_fixed_budgets() {
    let mut p = Policy::default();
    let s = online();
    let mut lit_ms = 0;
    for time in (0..10_000).step_by(20) {
        if p.frame(s, time, Some(time / 20)) == Color::Cyan {
            lit_ms += 20;
        }
    }
    assert_eq!(lit_ms, 240);
    assert_eq!(p.frame(s, 10_020, Some(0)), Color::Blue);
    assert_eq!(p.frame(s, 10_040, None), Color::Blue);
    let mut p = Policy::default();
    let s = Inputs {
        update_available: true,
        ..online()
    };
    let mut lit_ms = 0;
    for time in (0..30_000).step_by(20) {
        if p.frame(s, time, None) == Color::Purple {
            lit_ms += 20;
        }
    }
    assert_eq!(lit_ms, 600);
}

#[test]
fn malformed_staged_records_never_claim_an_available_or_running_update() {
    let good = serde_json::json!({"format":1,"key_sha256":"a".repeat(64),"floor":2,"confirmed":{"epoch":1},"candidate":{"epoch":2,"hash":"b".repeat(64)},"attempted":false});
    assert_eq!(staged_update(&good), Some((true, false)));
    for (key, value) in [
        ("format", serde_json::json!(0)),
        ("key_sha256", serde_json::json!("bad")),
        ("floor", serde_json::json!(-1)),
        ("confirmed", serde_json::json!({})),
        ("attempted", serde_json::json!("true")),
        (
            "candidate",
            serde_json::json!({"epoch":0,"hash":"b".repeat(64)}),
        ),
        (
            "candidate",
            serde_json::json!({"epoch":3,"hash":"b".repeat(64)}),
        ),
        (
            "candidate",
            serde_json::json!({"epoch":2,"hash":"B".repeat(64)}),
        ),
    ] {
        let mut altered = good.clone();
        altered[key] = value;
        assert_eq!(staged_update(&altered), None);
    }
}
