// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
#[derive(Default)]
struct Model {
    events: Vec<&'static str>,
    fail_begin: bool,
    ready: bool,
    fail_restore: bool,
    revoke_on_restore: Option<Arc<AtomicBool>>,
}
struct Network(Arc<Mutex<Model>>);
impl StationNetwork for Network {
    async fn begin(&mut self, _: &StationCredentials) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        m.events.push("begin");
        if m.fail_begin {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn usable(&self, _: &StationCredentials) -> Result<bool> {
        Ok(self.0.lock().unwrap().ready)
    }
    async fn restore_ap(&mut self, _: &StationCredentials) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        m.events.push("restore");
        if let Some(flag) = &m.revoke_on_restore {
            flag.store(false, Ordering::SeqCst);
        }
        if m.fail_restore {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
struct Guard(AtomicBool);
impl TrialGuard for Guard {
    async fn verify_trial(&self) -> Result<()> {
        if self.0.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Denied)
        }
    }
}
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
fn credentials() -> StationCredentials {
    StationCredentials::new(
        "synthetic AP".into(),
        Zeroizing::new("synthetic-passphrase".into()),
    )
    .unwrap()
}
#[tokio::test]
async fn successful_or_failed_test_returns_to_ap_and_never_saves_or_releases_isolation() {
    for (ready, fail_begin, fail_restore, expected) in [
        (true, false, false, TrialOutcome::UsableAndRestored),
        (false, true, false, TrialOutcome::FailedAndRestored),
        (false, false, false, TrialOutcome::FailedAndRestored),
        (true, false, true, TrialOutcome::Unknown),
    ] {
        let m = Arc::new(Mutex::new(Model {
            ready,
            fail_begin,
            fail_restore,
            ..Default::default()
        }));
        let mut n = Network(m.clone());
        let (_tx, rx) = watch::channel(false);
        let result = run(
            &mut n,
            &Guard(AtomicBool::new(true)),
            &Authority(Arc::new(AtomicBool::new(true))),
            &credentials(),
            Duration::from_millis(20),
            rx,
        )
        .await
        .unwrap();
        assert_eq!(result, expected);
        assert_eq!(m.lock().unwrap().events, ["begin", "restore"]);
    }
}
#[tokio::test]
async fn initial_cancellation_or_invalid_authority_or_wrong_guard_has_no_network_effects() {
    for (cancel, gate, guard) in [
        (true, true, true),
        (false, false, true),
        (false, true, false),
    ] {
        let m = Arc::new(Mutex::new(Model::default()));
        let mut n = Network(m.clone());
        let (_tx, rx) = watch::channel(cancel);
        assert!(run(
            &mut n,
            &Guard(AtomicBool::new(guard)),
            &Authority(Arc::new(AtomicBool::new(gate))),
            &credentials(),
            Duration::from_secs(1),
            rx
        )
        .await
        .is_err());
        assert!(m.lock().unwrap().events.is_empty());
    }
}
#[tokio::test]
async fn caller_loss_after_start_still_awaits_exact_ap_restoration() {
    let m = Arc::new(Mutex::new(Model::default()));
    let mut n = Network(m.clone());
    let (tx, rx) = watch::channel(false);
    let dropper = async {
        loop {
            if !m.lock().unwrap().events.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
        drop(tx);
    };
    let g = Guard(AtomicBool::new(true));
    let a = Authority(Arc::new(AtomicBool::new(true)));
    let c = credentials();
    let (result, _) = tokio::join!(run(&mut n, &g, &a, &c, Duration::from_secs(1), rx), dropper);
    assert_eq!(result.unwrap(), TrialOutcome::FailedAndRestored);
    assert_eq!(m.lock().unwrap().events, ["begin", "restore"]);
}
#[tokio::test]
async fn expired_authority_and_damaged_guard_cannot_report_a_success() {
    for damage_guard in [false, true] {
        let m = Arc::new(Mutex::new(Model::default()));
        let mut n = Network(m.clone());
        let (_tx, rx) = watch::channel(false);
        let g = Guard(AtomicBool::new(true));
        let a = Authority(Arc::new(AtomicBool::new(true)));
        let c = credentials();
        let revoke = async {
            loop {
                if !m.lock().unwrap().events.is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
            if damage_guard {
                g.0.store(false, Ordering::SeqCst);
            } else {
                a.0.store(false, Ordering::SeqCst);
            }
        };
        let (result, _) = tokio::join!(run(&mut n, &g, &a, &c, Duration::from_secs(1), rx), revoke);
        assert_eq!(
            result.unwrap(),
            if damage_guard {
                TrialOutcome::Unknown
            } else {
                TrialOutcome::FailedAndRestored
            }
        );
        assert_eq!(m.lock().unwrap().events, ["begin", "restore"]);
    }
}
#[test]
fn station_credentials_preserve_core_wpa2_contract_and_never_accept_injected_controls() {
    assert!(StationCredentials::new("网络".into(), Zeroizing::new("ab".repeat(32))).is_ok());
    for (ssid, psk) in [
        ("", "12345678"),
        ("bad\nname", "12345678"),
        ("wifi", "short"),
        ("wifi", "1234567\n"),
    ] {
        assert!(StationCredentials::new(ssid.into(), Zeroizing::new(psk.into())).is_err());
    }
    assert_ne!(credentials().uuid, credentials().uuid);
}
#[tokio::test]
async fn authority_lost_during_ap_restore_invalidates_the_earlier_station_success() {
    let valid = Arc::new(AtomicBool::new(true));
    let m = Arc::new(Mutex::new(Model {
        ready: true,
        revoke_on_restore: Some(valid.clone()),
        ..Default::default()
    }));
    let mut n = Network(m.clone());
    let (_tx, rx) = watch::channel(false);
    let result = run(
        &mut n,
        &Guard(AtomicBool::new(true)),
        &Authority(valid),
        &credentials(),
        Duration::from_secs(1),
        rx,
    )
    .await
    .unwrap();
    assert_eq!(result, TrialOutcome::FailedAndRestored);
    assert_eq!(m.lock().unwrap().events, ["begin", "restore"]);
}
