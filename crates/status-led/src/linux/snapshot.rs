// SPDX-License-Identifier: AGPL-3.0-only
//! The renderer must not keep reporting a live network after its sampler dies.
//! This age check runs in the renderer, independently of the query runtime.
use rustsimhub_status_led::Inputs;
use std::time::{Duration, Instant};

pub(super) const MAX_AGE_MS: u64 = 30_000;

pub(super) struct Snapshot {
    input: Inputs,
    observed_at: Instant,
}

impl Snapshot {
    pub(super) fn new(input: Inputs, observed_at: Instant) -> Self {
        Self { input, observed_at }
    }

    pub(super) fn current(&self, now: Instant) -> Inputs {
        if now
            .checked_duration_since(self.observed_at)
            .is_some_and(|age| age < Duration::from_millis(MAX_AGE_MS))
        {
            self.input
        } else {
            // Drop all stale connectivity/configuration/update assertions.
            // Fresh Core hints and the explicit OFF mode are applied later
            // by the renderer, retaining the existing policy priorities.
            Inputs {
                error: true,
                ..Inputs::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustsimhub_status_led::{Color, Policy};
    use std::sync::{Arc, Mutex};

    fn online() -> Inputs {
        Inputs {
            wifi_connected: true,
            internet: true,
            ..Inputs::default()
        }
    }

    #[test]
    fn a_finished_sampler_cannot_leave_a_permanent_connected_light() {
        let start = Instant::now();
        let shared = Arc::new(Mutex::new(Snapshot::new(Inputs::default(), start)));
        let writer = shared.clone();
        std::thread::spawn(move || *writer.lock().unwrap() = Snapshot::new(online(), start))
            .join()
            .unwrap();
        let snapshot = shared.lock().unwrap();
        let mut policy = Policy::default();
        // No writer remains. Normal query+sleep gaps remain usable, while
        // expiry is enforced by the render side without another bus query.
        let fresh = snapshot.current(start + Duration::from_secs(18));
        assert_eq!(policy.frame(fresh, 18_000, None), Color::Blue);
        let stale = snapshot.current(start + Duration::from_secs(30));
        assert!(stale.error && !stale.wifi_connected && !stale.internet);
        assert_eq!(policy.frame(stale, 30_000, None), Color::Red);
    }

    #[test]
    fn the_initial_configuration_indication_has_a_bounded_lifetime() {
        let start = Instant::now();
        let initial = Snapshot::new(
            Inputs {
                configuring: true,
                ..Inputs::default()
            },
            start,
        );
        assert!(
            initial
                .current(start + Duration::from_millis(29_999))
                .configuring
        );
        let expired = initial.current(start + Duration::from_millis(30_000));
        assert!(expired.error && !expired.configuring);
        // A future observation must not acquire an unlimited lifetime.
        assert!(initial.current(start - Duration::from_millis(1)).error);
    }

    #[test]
    fn a_new_observation_recovers_but_its_query_time_already_counts() {
        let start = Instant::now();
        let old = Snapshot::new(online(), start);
        assert!(old.current(start + Duration::from_secs(31)).error);
        let query_started = start + Duration::from_secs(35);
        let renewed = Snapshot::new(
            Inputs {
                cellular_connected: true,
                internet: true,
                ..Inputs::default()
            },
            query_started,
        );
        let mut policy = Policy::default();
        let current = renewed.current(query_started + Duration::from_secs(3));
        assert_eq!(policy.frame(current, 38_000, None), Color::Green);
        assert!(
            renewed
                .current(query_started + Duration::from_secs(30))
                .error
        );
    }

    #[test]
    fn stale_flags_do_not_mask_failure_but_current_off_and_ringing_keep_priority() {
        let start = Instant::now();
        let old = Snapshot::new(
            Inputs {
                configuring: true,
                upgrading: true,
                update_available: true,
                unread_sms: true,
                missed_calls: true,
                ..online()
            },
            start,
        );
        let mut current = old.current(start + Duration::from_secs(30));
        assert!(!current.configuring && !current.upgrading && !current.update_available);
        assert!(!current.unread_sms && !current.missed_calls);
        let mut policy = Policy::default();
        assert_eq!(policy.frame(current, 30_000, None), Color::Red);
        current.ringing = true; // Supplied by a separately fresh Core hint.
        assert_eq!(policy.frame(current, 30_000, None), Color::White);
        current.disabled = true; // Supplied by the actual local OFF setting.
        assert_eq!(policy.frame(current, 30_000, None), Color::Off);
    }
}
