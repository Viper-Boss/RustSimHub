// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

fn midnight(year: i32, month: Month, day: u8) -> i64 {
    Date::from_calendar_date(year, month, day)
        .unwrap()
        .with_hms(0, 0, 0)
        .unwrap()
        .assume_offset(UtcOffset::from_hms(8, 0, 0).unwrap())
        .unix_timestamp()
}
fn state() -> State {
    State::new("c".repeat(64))
}
fn reopen(s: &State) -> State {
    let s: State = serde_json::from_slice(&serde_json::to_vec(s).unwrap()).unwrap();
    assert!(s.valid());
    s
}
fn snapshot(s: &State) -> serde_json::Value {
    serde_json::to_value(s).unwrap()
}

#[test]
fn policy_boundaries_and_disabled_quota_are_independent() {
    for reset_day in [1, 28, 29, 30, 31] {
        for limit_bytes in [1, MAX_LIMIT] {
            assert!(Policy {
                enabled: false,
                limit_bytes,
                reset_day
            }
            .valid());
        }
    }
    for (limit_bytes, reset_day) in [(0, 1), (MAX_LIMIT + 1, 1), (1, 0), (1, 32)] {
        assert!(!Policy {
            enabled: false,
            limit_bytes,
            reset_day
        }
        .valid());
    }
    let now = midnight(2026, Month::October, 29);
    let mut s = state();
    s.sample("boot", &"a".repeat(64), now, 1, 0).unwrap();
    s.policy.limit_bytes = 1;
    s.uncertain = true;
    assert!(!s.usage(now).unwrap().limit_reached);
    s.policy.enabled = true;
    assert!(s.usage(now).unwrap().limit_reached);
    assert_eq!(s.days.len(), 1);
}

#[test]
fn every_reset_day_clamps_at_local_midnight_across_short_months_and_years() {
    for (year, month, last, next_year, next_month, next_last) in [
        (2027, Month::February, 28, 2027, Month::March, 31),
        (2028, Month::February, 29, 2028, Month::March, 31),
        (2026, Month::April, 30, 2026, Month::May, 31),
        (2026, Month::December, 31, 2027, Month::January, 31),
    ] {
        for reset in [1, 28, 29, 30, 31] {
            let start = Date::from_calendar_date(year, month, reset.min(last)).unwrap();
            let end =
                Date::from_calendar_date(next_year, next_month, reset.min(next_last)).unwrap();
            let at = midnight(year, month, start.day());
            assert_eq!(period(at - 1, reset).unwrap().1, start);
            for delta in [0, 1] {
                assert_eq!(period(at + delta, reset).unwrap(), (start, end));
            }
            assert_eq!(
                period(midnight(next_year, next_month, end.day()) - 1, reset).unwrap(),
                (start, end)
            );
        }
    }
}

#[test]
fn invalid_clocks_do_not_reset_a_retained_ledger() {
    let now = midnight(2026, Month::October, 29);
    let mut s = state();
    s.sample("boot", &"a".repeat(64), now, 7, 9).unwrap();
    let before = snapshot(&s);
    for invalid in [
        i64::MIN,
        i64::MAX,
        0,
        midnight(2023, Month::December, 31),
        midnight(2201, Month::January, 1),
    ] {
        assert!(s.sample("boot", &"a".repeat(64), invalid, 8, 10).is_err());
        assert!(s.usage(invalid).is_err());
        assert_eq!(snapshot(&s), before);
    }
}

#[test]
fn backward_day_is_fenced_but_same_day_and_future_samples_keep_existing_semantics() {
    let now = midnight(2026, Month::October, 29);
    let sim = "a".repeat(64);
    let mut s = state();
    s.sample("boot", &sim, now + 60, 10, 20).unwrap();
    s.sample("boot", &sim, now + 1, 11, 21).unwrap();
    assert_eq!(s.usage(now).unwrap().total_bytes, 32);
    let mut expected = reopen(&s);
    expected.uncertain = true;
    assert_eq!(
        s.sample("boot", &sim, now - 1, 12, 22),
        Err("clock moved backwards")
    );
    assert_eq!(snapshot(&s), snapshot(&expected));
    let future = midnight(2027, Month::October, 29);
    s.sample("boot", &sim, future, 12, 22).unwrap();
    assert!(s.uncertain);
    assert_eq!(s.usage(future).unwrap().total_bytes, 2);
}

#[test]
fn exact_quota_single_byte_over_and_directional_deltas_survive_reopen() {
    let now = midnight(2026, Month::October, 29);
    let sim = "a".repeat(64);
    let mut s = state();
    s.policy = Policy {
        enabled: true,
        limit_bytes: 10,
        reset_day: 1,
    };
    for (rx, tx, total) in [(3, 4, 7), (3, 4, 7), (6, 4, 10), (6, 5, 11)] {
        s.sample("boot", &sim, now, rx, tx).unwrap();
        s = reopen(&s);
        let u = s.usage(now).unwrap();
        assert_eq!((u.rx_bytes, u.tx_bytes, u.total_bytes), (rx, tx, total));
        assert_eq!(u.remaining_bytes, 10u64.saturating_sub(total));
        assert_eq!(u.limit_reached, total >= 10);
    }
}

#[test]
fn each_direction_counter_reset_preserves_checkpoint_and_blocks_enabled_quota() {
    let now = midnight(2026, Month::October, 29);
    for (rx, tx) in [(9, 20), (10, 19), (0, 0)] {
        let mut s = state();
        s.policy.enabled = true;
        s.sample("boot", &"a".repeat(64), now, 10, 20).unwrap();
        let mut expected = reopen(&s);
        expected.uncertain = true;
        assert_eq!(
            s.sample("boot", &"a".repeat(64), now, rx, tx),
            Err("counter decreased")
        );
        assert_eq!(snapshot(&s), snapshot(&expected));
        assert!(reopen(&s).usage(now).unwrap().limit_reached);
    }
}

#[test]
fn boot_and_sim_changes_keep_accounts_separate_without_recounting() {
    let now = midnight(2026, Month::October, 29);
    let (a, b) = ("a".repeat(64), "b".repeat(64));
    let mut s = state();
    s.sample("one", &a, now, 10, 20).unwrap();
    s = reopen(&s);
    s.sample("two", &a, now, 2, 3).unwrap();
    s.sample("two", &b, now, 5, 7).unwrap(); // final deltas belong to A
    assert_eq!(s.usage(now).unwrap().total_bytes, 0);
    s = reopen(&s);
    s.sample("two", &b, now, 6, 9).unwrap();
    assert_eq!(s.usage(now).unwrap().total_bytes, 3);
    s.sample("two", &a, now, 6, 9).unwrap();
    assert_eq!(s.usage(now).unwrap().total_bytes, 42);
    s.sample("two", "", now, 7, 10).unwrap();
    assert_eq!(s.usage(now).unwrap().total_bytes, 44);
}

#[test]
fn failed_directional_addition_never_partially_charges_or_advances_checkpoint() {
    let now = midnight(2026, Month::October, 29);
    let sim = "a".repeat(64);
    for (rx, tx) in [(10, u64::MAX), (u64::MAX, 10)] {
        let mut s = state();
        s.sample("one", &sim, now, rx, tx).unwrap();
        let mut expected = reopen(&s);
        expected.uncertain = true;
        for _ in 0..2 {
            assert_eq!(s.sample("two", &sim, now, 1, 1), Err("counter overflow"));
            assert_eq!((s.days[0].rx, s.days[0].tx), (rx, tx));
            assert_eq!(snapshot(&s), snapshot(&expected));
            s = reopen(&s);
        }
    }
}

#[test]
fn usage_overflow_is_an_error_instead_of_wrapping_into_free_quota() {
    let now = midnight(2026, Month::October, 29);
    let mut s = state();
    s.sample("one", &"a".repeat(64), now, u64::MAX, 1).unwrap();
    assert!(s.usage(now).is_err());
    s.days[0].tx = 0;
    let mut extra = s.days[0].clone();
    extra.rx = 1;
    s.days.push(extra);
    assert!(s.usage(now).is_err());
}

#[test]
fn full_catalogue_rejection_preserves_a_valid_reopenable_ledger() {
    let now = midnight(2026, Month::October, 29);
    let day = local_date(now).unwrap().to_julian_day();
    let mut s = state();
    s.days = (0..3200)
        .map(|n| Day {
            sim: format!("{n:064x}"),
            day,
            rx: 1,
            tx: 1,
        })
        .collect();
    let mut expected = reopen(&s);
    expected.uncertain = true;
    assert_eq!(
        s.sample("boot", &"f".repeat(64), now, 1, 1),
        Err("accounting catalogue full")
    );
    assert!(s.valid());
    assert_eq!(snapshot(&s), snapshot(&expected));
    reopen(&s);
    // Retention can free capacity without clearing the current account.
    s.days[0].day = day - 400;
    s.sample("boot", &"f".repeat(64), now, 1, 1).unwrap();
    assert_eq!(s.days.len(), 3200);
    assert_eq!(s.usage(now).unwrap().total_bytes, 2);
}
