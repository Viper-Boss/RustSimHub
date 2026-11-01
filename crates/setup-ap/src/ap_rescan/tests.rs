// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

struct Authority(Arc<AtomicBool>);
impl Gate for Authority {
    async fn check(&self) -> Result<()> {
        if self.0.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
struct Guard(Arc<AtomicBool>);
impl TrialGuard for Guard {
    async fn verify_trial(&self) -> Result<()> {
        if self.0.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Changed)
        }
    }
}
struct Network {
    events: Arc<Mutex<Vec<&'static str>>>,
    mode: &'static str,
    authority: Arc<AtomicBool>,
    guard: Arc<AtomicBool>,
    stop: watch::Sender<bool>,
}
impl RescanNetwork for Network {
    async fn pause_ap(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("pause");
        match self.mode {
            "lost-pause" => Err(Error::Io),
            "blocked" => std::future::pending().await,
            "revoke" => {
                self.authority.store(false, Ordering::SeqCst);
                Ok(())
            }
            "cancel" => {
                self.stop.send_replace(true);
                tokio::task::yield_now().await;
                Ok(())
            }
            _ => Ok(()),
        }
    }
    async fn scan(&mut self) -> Result<Snapshot> {
        self.events.lock().unwrap().push("scan");
        if self.mode == "scan-failed" {
            return Err(Error::Io);
        }
        if self.mode == "guard-lost" {
            self.guard.store(false, Ordering::SeqCst);
        }
        let t = crate::channel::uptime_ms()?;
        Ok(Snapshot {
            requested_uptime_ms: if self.mode == "old-snapshot" { 0 } else { t },
            completed_uptime_ms: if self.mode == "old-snapshot" { 0 } else { t },
            last_scan_uptime_ms: if self.mode == "old-snapshot" { 0 } else { t },
            networks: vec![],
            omitted: 0,
            truncated: false,
        })
    }
    async fn restore_ap(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("restore");
        if self.mode == "restore-blocked" {
            return std::future::pending().await;
        }
        if self.mode == "restore-failed" {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
struct SlowAuthority;
impl Gate for SlowAuthority {
    async fn check(&self) -> Result<()> {
        tokio::time::sleep(Duration::from_millis(30)).await;
        Ok(())
    }
}
#[tokio::test]
async fn initial_checks_consume_the_original_work_deadline_without_pausing() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(AtomicBool::new(true));
    let guard = Arc::new(AtomicBool::new(true));
    let (stop, rx) = watch::channel(false);
    let mut network = Network {
        events: events.clone(),
        mode: "ok",
        authority,
        guard: guard.clone(),
        stop,
    };
    assert!(matches!(
        run(
            &mut network,
            &Guard(guard),
            &SlowAuthority,
            Duration::from_millis(10),
            rx
        )
        .await,
        Err(Error::Busy)
    ));
    assert!(events.lock().unwrap().is_empty());
}
#[tokio::test]
async fn blocked_cleanup_is_bounded_and_keeps_the_parent_isolation_unknown() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let authority = Arc::new(AtomicBool::new(true));
    let guard = Arc::new(AtomicBool::new(true));
    let (stop, rx) = watch::channel(false);
    let mut network = Network {
        events: events.clone(),
        mode: "restore-blocked",
        authority: authority.clone(),
        guard: guard.clone(),
        stop,
    };
    let began = tokio::time::Instant::now();
    let result = run(
        &mut network,
        &Guard(guard.clone()),
        &Authority(authority),
        Duration::from_millis(100),
        rx,
    )
    .await
    .unwrap();
    assert!(matches!(result, RescanOutcome::Unknown));
    assert!(began.elapsed() < Duration::from_secs(12));
    assert!(guard.load(Ordering::SeqCst));
    assert_eq!(*events.lock().unwrap(), ["pause", "scan", "restore"]);
}
#[tokio::test]
async fn scan_outcome_never_substitutes_for_exact_cleanup_and_current_authority() {
    for mode in [
        "ok",
        "lost-pause",
        "blocked",
        "revoke",
        "cancel",
        "scan-failed",
        "guard-lost",
        "restore-failed",
        "old-snapshot",
    ] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let authority = Arc::new(AtomicBool::new(true));
        let guard = Arc::new(AtomicBool::new(true));
        let (stop, rx) = watch::channel(false);
        let mut network = Network {
            events: events.clone(),
            mode,
            authority: authority.clone(),
            guard: guard.clone(),
            stop,
        };
        let result = run(
            &mut network,
            &Guard(guard),
            &Authority(authority),
            Duration::from_millis(100),
            rx,
        )
        .await
        .unwrap();
        if mode == "ok" {
            assert!(matches!(result, RescanOutcome::ScannedAndRestored(_)));
        } else if ["guard-lost", "restore-failed"].contains(&mode) {
            assert!(matches!(result, RescanOutcome::Unknown));
        } else {
            assert!(matches!(result, RescanOutcome::FailedAndRestored));
        }
        let log = events.lock().unwrap();
        assert_eq!(log.first(), Some(&"pause"));
        assert_eq!(log.last(), Some(&"restore"));
        assert_eq!(log.iter().filter(|v| **v == "pause").count(), 1);
        assert!(log.iter().filter(|v| **v == "scan").count() <= 1);
        if ["revoke", "cancel", "blocked", "lost-pause"].contains(&mode) {
            assert!(!log.contains(&"scan"));
        }
    }
}
#[tokio::test]
async fn initial_denial_invalid_window_and_lost_caller_have_no_network_effects() {
    for mode in [
        "authority",
        "guard",
        "cancelled",
        "gone",
        "zero",
        "too-long",
    ] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let authority = Arc::new(AtomicBool::new(mode != "authority"));
        let guard = Arc::new(AtomicBool::new(mode != "guard"));
        let (stop, rx) = watch::channel(mode == "cancelled");
        let mut network = Network {
            events: events.clone(),
            mode: "ok",
            authority: authority.clone(),
            guard: guard.clone(),
            stop,
        };
        if mode == "gone" {
            let (new, _) = watch::channel(false);
            network.stop = new;
        }
        let window = if mode == "zero" {
            Duration::ZERO
        } else if mode == "too-long" {
            Duration::from_secs(MAX_WORK_SECONDS + 1)
        } else {
            Duration::from_secs(1)
        };
        assert!(run(
            &mut network,
            &Guard(guard),
            &Authority(authority),
            window,
            rx
        )
        .await
        .is_err());
        assert!(events.lock().unwrap().is_empty());
    }
}
