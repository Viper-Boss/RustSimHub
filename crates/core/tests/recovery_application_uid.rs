// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(all(test, target_os = "linux"))]
//! Explicit isolated real-UID fixture. Only A verification/sealing is synthetic.
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_auth::{
    recovery_capability::{
        RecoveryAction, RecoveryProofContext, RecoveryRequestVerifierV1, VerifiedRecoveryRequest,
    },
    Auth, Token,
};
use rustsimhub_core::{
    recovery_channel::{
        ApplicationReplySealerV1, ApplicationStep, RecoveryChannel, SealedReply, SealingRequest,
    },
    recovery_transport::{Kind, Roles, SourceGuard},
};
use rustsimhub_storage::{RecoveryGrant, Store};
use std::{
    fs,
    future::{poll_fn, Future},
    os::unix::fs::{MetadataExt, PermissionsExt},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc, Mutex,
    },
    task::Poll,
    time::{Duration, Instant},
};
use tokio::{net::UnixListener, sync::Notify};
use zeroize::Zeroizing;
const BODY: &[u8] = br#"{"target_request_id":"00000000-0000-4000-8000-000000000099"}"#;
type Plan = Arc<Mutex<Vec<u8>>>;
struct Verifier {
    plan: Plan,
    calls: Arc<AtomicUsize>,
}
impl RecoveryRequestVerifierV1 for Verifier {
    fn verify_request(
        &self,
        c: &RecoveryProofContext,
        e: &[u8],
    ) -> rustsimhub_auth::Result<VerifiedRecoveryRequest> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let n = *e.first().ok_or(rustsimhub_auth::Error::InvalidInput)?;
        VerifiedRecoveryRequest::from_verified_adapter(
            c.binding().clone(),
            vec![9; 16],
            [n; 16],
            RecoveryAction::QueryOperation,
            Zeroizing::new(self.plan.lock().unwrap().clone()),
            Duration::from_secs(120),
        )
    }
}
struct Fixture {
    auth: Arc<Mutex<Auth>>,
    grant: RecoveryGrant,
    owner: Token,
    plan: Plan,
    calls: Arc<AtomicUsize>,
    _store: Store,
    _dirs: Vec<tempfile::TempDir>,
}
fn dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn fixture() -> Fixture {
    let d = dir();
    let i = dir();
    let b = dir();
    let b9 = dir();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[61; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let grant = a
        .grant_recovery_key(
            token.expose(),
            SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes(),
            40000,
            0,
            "00000000-0000-4000-8000-000000000001".into(),
        )
        .unwrap();
    drop(a);
    drop(s);
    Store::migrate_recovery_execution(d.path(), b9.path()).unwrap();
    let s = Store::open_recovery_execution(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let plan = Arc::new(Mutex::new(BODY.to_vec()));
    let calls = Arc::new(AtomicUsize::new(0));
    a.set_recovery_verifier(Some(Arc::new(Verifier {
        plan: plan.clone(),
        calls: calls.clone(),
    })))
    .unwrap();
    Fixture {
        auth: Arc::new(Mutex::new(a)),
        grant,
        owner: token,
        plan,
        calls,
        _store: s,
        _dirs: vec![d, i, b, b9],
    }
}
struct Hold {
    entered: Notify,
    release: Mutex<mpsc::Receiver<()>>,
    after_release_denied: AtomicBool,
}
struct Sealer {
    marker: u8,
    invalid: bool,
    hold: Option<Arc<Hold>>,
    revoke: Option<(Arc<Mutex<Auth>>, Zeroizing<String>, RecoveryGrant)>,
}
impl ApplicationReplySealerV1 for Sealer {
    fn seal(&self, r: SealingRequest) -> rustsimhub_auth::Result<SealedReply> {
        assert_eq!(r.scope().action(), RecoveryAction::QueryOperation);
        assert_eq!(r.scope().request_id(), &[self.marker; 16]);
        let v: serde_json::Value = serde_json::from_slice(r.plaintext()).unwrap();
        if self.invalid {
            assert_eq!(v["code"], "invalid_input");
        } else {
            assert_eq!(v["lookup"], "not_found");
            assert_eq!(v["operation"], serde_json::Value::Null);
        }
        if let Some(h) = &self.hold {
            h.entered.notify_one();
            let _ = h.release.lock().unwrap().recv();
            h.after_release_denied
                .store(r.scope().check_current().is_err(), Ordering::Release);
        }
        if let Some((a, t, g)) = &self.revoke {
            a.lock()
                .unwrap()
                .revoke_recovery_key(
                    t,
                    g.independent_public_key,
                    g.generation,
                    "00000000-0000-4000-8000-000000000009".into(),
                )
                .unwrap();
        }
        // Synthetic A output, NOT an actual cipher or an interoperability claim.
        r.finish_from_trusted_session(Zeroizing::new(vec![0xa5, self.marker]))
    }
}
fn sealer(marker: u8) -> Sealer {
    Sealer {
        marker,
        invalid: false,
        hold: None,
        revoke: None,
    }
}
async fn channel(l: &UnixListener, g: &SourceGuard, f: &Fixture) -> RecoveryChannel {
    let (s, _) = tokio::time::timeout(Duration::from_secs(10), l.accept())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(s.peer_cred().unwrap().uid(), 1002);
    RecoveryChannel::attach(&mut f.auth.lock().unwrap(), g.accept(s).unwrap()).unwrap()
}
async fn drained(g: &SourceGuard) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while g.is_busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}
#[test]
fn isolated_real_uid_application_dispatch_and_reply_fences() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();
    rt.block_on(async {
        assert_eq!(rustix::process::geteuid().as_raw(), 1001);
        let root = std::path::PathBuf::from(
            std::env::var_os("RUSTSIMHUB_UID_FIXTURE_DIR")
                .expect("explicit isolated runner required"),
        );
        let guard = SourceGuard::new(Roles::new(1003, 1004, 1005, 1002).unwrap()).unwrap();
        let listener = UnixListener::bind(root.join("core.sock")).unwrap();
        fs::set_permissions(root.join("core.sock"), fs::Permissions::from_mode(0o660)).unwrap();
        let m = fs::metadata(root.join("core.sock")).unwrap();
        assert_eq!((m.uid(), m.gid(), m.mode() & 0o777), (1001, 1002, 0o660));
        let f = fixture();
        let audit = dir();
        rustsimhub_core::network_trial::provision::enable(audit.path()).unwrap();
        println!("APP_READY");
        for uid in [0, 1001, 1003, 1004, 1005, 1006] {
            let (s, _) = listener.accept().await.unwrap();
            assert_eq!(s.peer_cred().unwrap().uid(), uid);
            assert!(guard.accept(s).is_err());
            println!("UID_CASE denied_{uid}");
        }
        let c = channel(&listener, &guard, &f).await;
        assert!(c
            .dispatch_application_v1(f.auth.clone(), f.grant.clone(), None)
            .await
            .is_err());
        assert_eq!(f.calls.load(Ordering::Acquire), 0);
        assert!(!guard.is_busy());
        println!("UID_CASE no_sealer");
        for (marker, invalid) in [(2, false), (3, true)] {
            *f.plan.lock().unwrap() = if invalid {
                br#"["00000000-0000-4000-8000-000000000099"]"#.to_vec()
            } else {
                BODY.to_vec()
            };
            let c = channel(&listener, &guard, &f).await;
            let mut s = sealer(marker);
            s.invalid = invalid;
            let ApplicationStep::Continue(c) = c
                .dispatch_application_v1(f.auth.clone(), f.grant.clone(), Some(Arc::new(s)))
                .await
                .unwrap()
            else {
                panic!("reply")
            };
            drop(c);
            assert!(!guard.is_busy());
            println!(
                "UID_CASE {}",
                if invalid {
                    "sealed_invalid_input"
                } else {
                    "sealed_lookup"
                }
            );
        }
        *f.plan.lock().unwrap() = BODY.to_vec();
        for queue_timeout in [false, true] {
            let c = channel(&listener, &guard, &f).await;
            // First actual frame establishes a private same-channel request scope;
            // the fixture has already sent a second frame in the same write.
            let (c, p) = c.read().await.unwrap();
            assert_eq!(p.kind(), Kind::EncryptedRequest);
            let pending = c
                .prepare(&mut f.auth.lock().unwrap(), f.grant.clone(), p)
                .unwrap();
            let v = pending.verify().await.unwrap();
            let (c, r) = v.finish(&mut f.auth.lock().unwrap()).unwrap();
            let scope = f
                .auth
                .lock()
                .unwrap()
                .prepare_recovery_response(&r)
                .unwrap();
            scope.check_current().unwrap();
            let (release, held) = mpsc::channel();
            let entered = Arc::new(Notify::new());
            let notify = entered.clone();
            let blocker = tokio::task::spawn_blocking(move || {
                notify.notify_one();
                let _ = held.recv();
            });
            entered.notified().await;
            let mut future = Box::pin(c.dispatch_application_v1(
                f.auth.clone(),
                f.grant.clone(),
                Some(Arc::new(sealer(if queue_timeout { 0x24 } else { 0x22 }))),
            ));
            for _ in 0..8 {
                poll_fn(|cx| {
                    assert!(future.as_mut().poll(cx).is_pending());
                    Poll::Ready(())
                })
                .await;
                tokio::task::yield_now().await;
            }
            if queue_timeout {
                assert!(matches!(
                    future.as_mut().await,
                    Err(rustsimhub_core::recovery_channel::Error::Transport(
                        rustsimhub_core::recovery_transport::Error::Deadline
                    ))
                ));
            }
            drop(future);
            assert!(
                scope.check_current().is_err(),
                "waiter cancellation must immediately fence queued ownership"
            );
            assert!(guard.is_busy(), "queued worker retains actual source slot");
            println!(
                "{}",
                if queue_timeout {
                    "APP_QUEUED_TIMEOUT"
                } else {
                    "APP_QUEUED"
                }
            );
            let (s, _) = listener.accept().await.unwrap();
            assert!(guard.accept(s).is_err());
            release.send(()).unwrap();
            blocker.await.unwrap();
            drained(&guard).await;
            println!(
                "UID_CASE {}",
                if queue_timeout {
                    "queued_timeout"
                } else {
                    "queued_cancel"
                }
            );
        }
        for (marker, timeout) in [(5, false), (6, true)] {
            let c = channel(&listener, &guard, &f).await;
            let (release, held) = mpsc::channel();
            let hold = Arc::new(Hold {
                entered: Notify::new(),
                release: Mutex::new(held),
                after_release_denied: AtomicBool::new(false),
            });
            let mut s = sealer(marker);
            s.hold = Some(hold.clone());
            let auth = f.auth.clone();
            let grant = f.grant.clone();
            let task = tokio::spawn(async move {
                c.dispatch_application_v1(auth, grant, Some(Arc::new(s)))
                    .await
            });
            hold.entered.notified().await;
            let now = Instant::now();
            if timeout {
                assert!(task.await.unwrap().is_err());
                assert!(now.elapsed() >= Duration::from_secs(4));
            } else {
                task.abort();
                assert!(matches!(task.await,Err(e) if e.is_cancelled()));
            }
            assert!(guard.is_busy());
            println!("APP_SEAL_HELD_{marker}");
            let (s, _) = listener.accept().await.unwrap();
            assert!(guard.accept(s).is_err());
            release.send(()).unwrap();
            drained(&guard).await;
            assert!(hold.after_release_denied.load(Ordering::Acquire));
            println!(
                "UID_CASE {}",
                if timeout {
                    "sealer_timeout"
                } else {
                    "sealer_cancel"
                }
            );
        }
        let c = channel(&listener, &guard, &f).await;
        let (release, held) = mpsc::channel();
        let hold = Arc::new(Hold {
            entered: Notify::new(),
            release: Mutex::new(held),
            after_release_denied: AtomicBool::new(false),
        });
        let mut s = sealer(9);
        s.hold = Some(hold.clone());
        let a = f.auth.clone();
        let grant = f.grant.clone();
        let task =
            tokio::spawn(
                async move { c.dispatch_application_v1(a, grant, Some(Arc::new(s))).await },
            );
        hold.entered.notified().await;
        println!("APP_PEER_SEAL_ENTERED");
        assert!(tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("actual peer EOF must fence before the adapter deadline")
            .unwrap()
            .is_err());
        assert!(guard.is_busy());
        release.send(()).unwrap();
        drained(&guard).await;
        assert!(hold.after_release_denied.load(Ordering::Acquire));
        println!("UID_CASE peer_disconnect");
        let c = channel(&listener, &guard, &f).await;
        assert!(matches!(
            c.dispatch_application_v1(f.auth.clone(), f.grant.clone(), Some(Arc::new(sealer(8))))
                .await,
            Ok(ApplicationStep::Closed)
        ));
        assert!(!guard.is_busy());
        println!("UID_CASE close_only");
        let c = channel(&listener, &guard, &f).await;
        let mut s = sealer(7);
        s.revoke = Some((
            f.auth.clone(),
            Zeroizing::new(f.owner.expose().into()),
            f.grant.clone(),
        ));
        assert!(c
            .dispatch_application_v1(f.auth.clone(), f.grant.clone(), Some(Arc::new(s)))
            .await
            .is_err());
        drained(&guard).await;
        println!("UID_CASE revoke_before_reply");
    });
}
