// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::{
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Mock {
    effects: Arc<Mutex<Vec<&'static str>>>,
    ready: bool,
    fail_start: bool,
    fail_commit: bool,
    fail_rollback: bool,
}
impl ProvisionBackend for Mock {
    async fn start(&mut self, _: &Credentials, _: &str) -> Result<()> {
        self.effects.lock().unwrap().push("temporary");
        if self.fail_start {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn ready(&self) -> Result<bool> {
        Ok(self.ready)
    }
    async fn commit(&mut self, _: &Credentials) -> Result<()> {
        self.effects.lock().unwrap().push("persist");
        if self.fail_commit {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
    async fn rollback(&mut self) -> Result<()> {
        self.effects.lock().unwrap().push("restore");
        if self.fail_rollback {
            Err(Error::Io)
        } else {
            Ok(())
        }
    }
}
struct Check {
    count: AtomicUsize,
    revoke: usize,
}
impl Gate for Check {
    async fn check(&self) -> Result<()> {
        if self.count.fetch_add(1, Ordering::SeqCst) >= self.revoke {
            Err(Error::Forbidden)
        } else {
            Ok(())
        }
    }
}
fn principal() -> Principal {
    Principal {
        id: "synthetic-owner".into(),
        public_key: [7; 32],
        kind: rustsimhub_policy::Kind::Owner,
        scopes: rustsimhub_policy::ALL_SCOPES,
        generation: 1,
        active: true,
    }
}
fn credentials() -> Credentials {
    Credentials {
        ssid: "synthetic-network".into(),
        passphrase: Zeroizing::new("synthetic-secret".into()),
    }
}
fn mock() -> Mock {
    Mock {
        effects: Arc::new(Mutex::new(vec![])),
        ready: true,
        fail_start: false,
        fail_commit: false,
        fail_rollback: false,
    }
}
fn directory() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    d
}
#[tokio::test]
async fn no_confirmation_or_failed_auth_never_persists() {
    for revoke in [usize::MAX, 0, 2] {
        let d = directory();
        let audit = Audit::new(d.path(), "key", &principal()).unwrap();
        audit.begin().unwrap();
        let mut backend = mock();
        let gate = Check {
            count: AtomicUsize::new(0),
            revoke,
        };
        let state = tokio::sync::Mutex::new(State::Starting);
        let (tx, mut rx) = mpsc::channel(1);
        let (reply, _) = oneshot::channel();
        tx.send(Control {
            confirm: false,
            reply,
        })
        .await
        .unwrap();
        run_trial(
            &mut backend,
            &gate,
            credentials(),
            &"a".repeat(64),
            &audit,
            &state,
            &mut rx,
        )
        .await;
        assert!(!backend.effects.lock().unwrap().contains(&"persist"));
        assert!(!audit.confirmation.exists());
        assert_eq!(*state.lock().await, State::RolledBack);
        assert!(Audit::new(d.path(), "key", &principal())
            .unwrap()
            .begin()
            .is_err());
        let data = std::fs::read_to_string(audit.started).unwrap();
        assert!(!data.contains("synthetic-secret"));
        assert!(!data.contains("synthetic-network"));
    }
}
#[tokio::test]
async fn ready_and_current_owner_confirmation_is_durable_before_persistence() {
    let d = directory();
    let audit = Audit::new(d.path(), "once", &principal()).unwrap();
    audit.begin().unwrap();
    let mut b = mock();
    let gate = Check {
        count: AtomicUsize::new(0),
        revoke: usize::MAX,
    };
    let state = tokio::sync::Mutex::new(State::Starting);
    let (tx, mut rx) = mpsc::channel(1);
    let (reply, replied) = oneshot::channel();
    tx.send(Control {
        confirm: true,
        reply,
    })
    .await
    .unwrap();
    run_trial(
        &mut b,
        &gate,
        credentials(),
        &"b".repeat(64),
        &audit,
        &state,
        &mut rx,
    )
    .await;
    assert_eq!(replied.await.unwrap(), State::Saved);
    assert_eq!(*b.effects.lock().unwrap(), ["temporary", "persist"]);
    assert!(audit.confirmation.exists());
    assert!(audit.result.exists());
}
#[tokio::test]
async fn partial_save_rolls_back_and_unproved_recovery_reports_unknown() {
    for recovery_fails in [false, true] {
        let d = directory();
        let audit = Audit::new(d.path(), "once", &principal()).unwrap();
        audit.begin().unwrap();
        let mut b = mock();
        b.fail_commit = true;
        b.fail_rollback = recovery_fails;
        let gate = Check {
            count: AtomicUsize::new(0),
            revoke: usize::MAX,
        };
        let state = tokio::sync::Mutex::new(State::Starting);
        let (tx, mut rx) = mpsc::channel(1);
        let (reply, _) = oneshot::channel();
        tx.send(Control {
            confirm: true,
            reply,
        })
        .await
        .unwrap();
        run_trial(
            &mut b,
            &gate,
            credentials(),
            &"c".repeat(64),
            &audit,
            &state,
            &mut rx,
        )
        .await;
        assert_eq!(
            *b.effects.lock().unwrap(),
            ["temporary", "persist", "restore"]
        );
        assert_eq!(
            *state.lock().await,
            if recovery_fails {
                State::Unknown
            } else {
                State::RolledBack
            }
        );
        assert!(audit.confirmation.exists());
        assert!(Audit::new(d.path(), "once", &principal())
            .unwrap()
            .begin()
            .is_err());
    }
}
#[tokio::test]
async fn failed_connection_and_revocation_before_save_only_restore() {
    for (not_ready, revoke) in [(true, usize::MAX), (false, 2)] {
        let d = directory();
        let audit = Audit::new(d.path(), "once", &principal()).unwrap();
        audit.begin().unwrap();
        let mut b = mock();
        b.ready = !not_ready;
        let gate = Check {
            count: AtomicUsize::new(0),
            revoke,
        };
        let state = tokio::sync::Mutex::new(State::Starting);
        let (tx, mut rx) = mpsc::channel(1);
        let (reply, _) = oneshot::channel();
        tx.send(Control {
            confirm: true,
            reply,
        })
        .await
        .unwrap();
        run_trial(
            &mut b,
            &gate,
            credentials(),
            &"c".repeat(64),
            &audit,
            &state,
            &mut rx,
        )
        .await;
        assert!(!b.effects.lock().unwrap().contains(&"persist"));
        assert_eq!(*state.lock().await, State::RolledBack);
    }
}
#[test]
fn settings_cannot_become_apn_or_access_point_or_expose_secret_in_view() {
    let c = credentials();
    let unconfirmed = settings(&c, &"a".repeat(64), false).unwrap();
    assert!(!bool::try_from(unconfirmed["connection"].get("autoconnect").unwrap()).unwrap());
    assert_eq!(
        <&str>::try_from(&unconfirmed["connection"]["type"]).unwrap(),
        "802-11-wireless"
    );
    assert_eq!(
        <&str>::try_from(&unconfirmed["802-11-wireless"]["mode"]).unwrap(),
        "infrastructure"
    );
    assert!(!unconfirmed.contains_key("gsm"));
    let summary = view(&"a".repeat(64), State::Ready, Instant::now() + DEADLINE).to_string();
    assert!(!summary.contains(&c.ssid));
    assert!(!summary.contains(c.passphrase.as_str()));
    let mut changed = credentials();
    changed.passphrase = Zeroizing::new("different-secret".into());
    assert_ne!(changed.fingerprint(), c.fingerprint());
}
#[tokio::test]
async fn unconfirmed_timeout_and_revocation_after_partial_commit_restore() {
    for confirm in [false, true] {
        let d = directory();
        let mut audit = Audit::new(d.path(), "once", &principal()).unwrap();
        audit.limit = Duration::from_millis(5);
        audit.begin().unwrap();
        let mut b = mock();
        let gate = Check {
            count: AtomicUsize::new(0),
            revoke: if confirm { 3 } else { usize::MAX },
        };
        let state = tokio::sync::Mutex::new(State::Starting);
        let (tx, mut rx) = mpsc::channel(1);
        if confirm {
            let (reply, _) = oneshot::channel();
            tx.send(Control {
                confirm: true,
                reply,
            })
            .await
            .unwrap();
        }
        run_trial(
            &mut b,
            &gate,
            credentials(),
            &"d".repeat(64),
            &audit,
            &state,
            &mut rx,
        )
        .await;
        assert_eq!(*state.lock().await, State::RolledBack);
        assert_eq!(
            *b.effects.lock().unwrap(),
            if confirm {
                vec!["temporary", "persist", "restore"]
            } else {
                vec!["temporary", "restore"]
            }
        );
    }
}
#[tokio::test]
async fn approved_client_trials_stay_bound_to_their_principal_and_end_on_revoke() {
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_policy::Kind;
    let (d, i) = (directory(), directory());
    let store = rustsimhub_storage::Store::open(d.path()).unwrap();
    let mut a = Auth::new(
        rustsimhub_auth::Identity::open(i.path()).unwrap(),
        store.handle(),
    )
    .unwrap();
    let login = |a: &mut Auth, k: &SigningKey| {
        let c = a.login_challenge(k.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &k.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    let o = SigningKey::from_bytes(&[131; 32]);
    let c = a.bootstrap_challenge(o.verifying_key().to_bytes()).unwrap();
    a.bootstrap_finish(&c.id, &o.sign(&c.transcript).to_bytes())
        .unwrap();
    let owner_token = login(&mut a, &o);
    let scopes = [Scope::StatusRead, Scope::NetworkManage];
    let mut tools = vec![];
    for seed in [132u8, 133] {
        let k = SigningKey::from_bytes(&[seed; 32]);
        a.open_pairing(&owner_token).unwrap();
        let c = a
            .pairing_challenge(k.verifying_key().to_bytes(), Kind::Client, &scopes)
            .unwrap();
        let id = a
            .pairing_finish(&c.id, &k.sign(&c.transcript).to_bytes())
            .unwrap();
        a.approve(
            &owner_token,
            &id,
            k.verifying_key().to_bytes(),
            Kind::Client,
            &scopes,
        )
        .unwrap();
        let token = login(&mut a, &k);
        tools.push((k, token));
    }
    let auth = Arc::new(Mutex::new(a));
    let owner_p = manager(auth.clone(), owner_token.clone()).await.unwrap();
    let tool_p = manager(auth.clone(), tools[0].1.clone()).await.unwrap();
    let other_p = manager(auth.clone(), tools[1].1.clone()).await.unwrap();
    assert_eq!(tool_p.kind, Kind::Client);
    let (control, _) = mpsc::channel(1);
    let session = Session {
        id: "a".repeat(64),
        key: "once".into(),
        principal: tool_p.clone(),
        fingerprint: credentials().fingerprint(),
        state: Arc::new(tokio::sync::Mutex::new(State::Ready)),
        control,
        deadline: Instant::now() + DEADLINE,
    };
    // Status/confirm/cancel: the tool's own trial id only; never another client's or the owner's.
    session.owned_by(&"a".repeat(64), &tool_p).unwrap();
    assert_eq!(
        session.owned_by(&"b".repeat(64), &tool_p),
        Err(Error::Forbidden)
    );
    assert_eq!(
        session.owned_by(&"a".repeat(64), &other_p),
        Err(Error::Forbidden)
    );
    assert_eq!(
        session.owned_by(&"a".repeat(64), &owner_p),
        Err(Error::Forbidden)
    );
    // Idempotency: same principal and same input replay; anything else under the key is refused.
    session
        .replays(&tool_p, &credentials().fingerprint())
        .unwrap();
    let changed = Credentials {
        ssid: "synthetic-network".into(),
        passphrase: Zeroizing::new("other-secret".into()),
    };
    assert_eq!(
        session.replays(&tool_p, &changed.fingerprint()),
        Err(Error::Forbidden)
    );
    assert_eq!(
        session.replays(&other_p, &credentials().fingerprint()),
        Err(Error::Forbidden)
    );
    assert_eq!(
        session.replays(&owner_p, &credentials().fingerprint()),
        Err(Error::Forbidden)
    );
    // The in-trial gate passes for the tool, then fails once the owner revokes it.
    let gate = ManagerGate {
        auth: auth.clone(),
        token: tools[0].1.clone(),
        expected: tool_p.clone(),
    };
    gate.check().await.unwrap();
    let wrong = ManagerGate {
        auth: auth.clone(),
        token: tools[1].1.clone(),
        expected: tool_p,
    };
    assert_eq!(wrong.check().await, Err(Error::Forbidden));
    auth.lock()
        .unwrap()
        .revoke(&owner_token, tools[0].0.verifying_key().to_bytes())
        .unwrap();
    assert_eq!(gate.check().await, Err(Error::Forbidden));
    assert_eq!(
        manager(auth, tools[0].1.clone()).await,
        Err(Error::Forbidden)
    );
}
