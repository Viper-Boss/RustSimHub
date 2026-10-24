// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
type Events = Arc<Mutex<Vec<&'static str>>>;
struct Authority {
    checks: AtomicUsize,
    fail_at: usize,
}
impl Gate for Authority {
    async fn check(&self) -> Result<()> {
        if self.checks.fetch_add(1, Ordering::SeqCst) >= self.fail_at {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
}
struct Firewall {
    events: Events,
    bad: bool,
    released: bool,
}
impl Guard for Firewall {
    async fn install(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("guard-install");
        Ok(())
    }
    async fn verify(&self) -> Result<()> {
        if self.bad {
            Err(Error::Changed)
        } else {
            Ok(())
        }
    }
    async fn release(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("guard-release");
        self.released = true;
        Ok(())
    }
}
struct Wifi {
    events: Events,
    stop: watch::Sender<bool>,
    rollback_fails: bool,
    activate_fails: bool,
    ready_error: bool,
}
impl Network for Wifi {
    async fn preflight(&mut self) -> Result<()> {
        self.events.lock().unwrap().push("preflight");
        Ok(())
    }
    async fn checkpoint(&mut self, _s: u32) -> Result<()> {
        self.events.lock().unwrap().push("checkpoint");
        Ok(())
    }
    async fn activate(&mut self, _c: &Credentials) -> Result<()> {
        self.events.lock().unwrap().push("activate");
        if self.activate_fails {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn ready(&self, _c: &Credentials) -> Result<bool> {
        self.events.lock().unwrap().push("ready");
        self.stop.send_replace(true);
        if self.ready_error {
            Err(Error::Changed)
        } else {
            Ok(true)
        }
    }
    async fn rollback(&mut self, _c: &Credentials) -> Result<()> {
        self.events.lock().unwrap().push("rollback");
        if self.rollback_fails {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
#[tokio::test]
async fn authority_guard_and_network_order_remain_finite_and_never_retry_activation() {
    for failure in [
        "none",
        "policy",
        "guard",
        "activate",
        "ready",
        "rollback",
        "cancelled",
        "gone",
    ] {
        let events = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = watch::channel(failure == "cancelled");
        let (status, view) = watch::channel(Phase::Preflight);
        let mut wifi = Wifi {
            events: events.clone(),
            stop: tx.clone(),
            rollback_fails: failure == "rollback",
            activate_fails: failure == "activate",
            ready_error: failure == "ready",
        };
        let mut firewall = Firewall {
            events: events.clone(),
            bad: failure == "guard",
            released: false,
        };
        let authority = Authority {
            checks: AtomicUsize::new(0),
            fail_at: if failure == "policy" { 0 } else { usize::MAX },
        };
        if failure == "gone" {
            drop(wifi.stop);
            let (replacement, _) = watch::channel(false);
            wifi.stop = replacement;
            drop(tx);
        }
        let result = run(
            &mut wifi,
            &mut firewall,
            &authority,
            &Credentials::generate().unwrap(),
            Duration::from_secs(3),
            rx,
            status,
        )
        .await;
        let log = events.lock().unwrap();
        if ["policy", "cancelled", "gone"].contains(&failure) {
            assert!(result.is_err());
            assert!(log.is_empty());
            continue;
        }
        assert_eq!(log.first(), Some(&"preflight"));
        assert_eq!(log[1], "guard-install");
        assert!(log.iter().filter(|x| **x == "activate").count() <= 1);
        assert!(log.contains(&"rollback"));
        if failure == "rollback" {
            assert_eq!(result.unwrap(), Outcome::Unknown);
            assert!(!firewall.released);
            assert_eq!(*view.borrow(), Phase::Unknown);
        } else {
            assert_eq!(result.unwrap(), Outcome::RolledBack);
            assert_eq!(log.last(), Some(&"guard-release"));
            assert_eq!(*view.borrow(), Phase::Closed);
        }
    }
}
#[tokio::test]
async fn gate_rechecks_prevent_late_revocation_from_reaching_later_mutations() {
    for fail_at in 1..6 {
        let events = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = watch::channel(false);
        let (status, _) = watch::channel(Phase::Preflight);
        let mut wifi = Wifi {
            events: events.clone(),
            stop: tx,
            rollback_fails: false,
            activate_fails: false,
            ready_error: false,
        };
        let mut guard = Firewall {
            events: events.clone(),
            bad: false,
            released: false,
        };
        let result = run(
            &mut wifi,
            &mut guard,
            &Authority {
                checks: AtomicUsize::new(0),
                fail_at,
            },
            &Credentials::generate().unwrap(),
            Duration::from_secs(3),
            rx,
            status,
        )
        .await;
        let log = events.lock().unwrap();
        if fail_at == 1 {
            assert!(result.is_err());
            assert_eq!(*log, ["preflight"]);
        } else {
            assert_eq!(result.unwrap(), Outcome::RolledBack);
            assert!(guard.released);
        }
        if fail_at <= 3 {
            assert!(!log.contains(&"activate"));
        }
    }
}
