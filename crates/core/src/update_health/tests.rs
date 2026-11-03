// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_storage::Store;
use std::thread;
use tempfile::TempDir;
struct Inputs {
    ready: std::sync::atomic::AtomicBool,
}
impl RuntimeSources for Inputs {
    fn business_ready(&self) -> bool {
        self.ready.load(std::sync::atomic::Ordering::Acquire)
    }
    fn with_forwarder(
        &self,
        _auth: &Auth,
        work: &mut dyn FnMut(ForwarderLease) -> Result<()>,
    ) -> Result<()> {
        work(ForwarderLease {
            queue_owned: false,
            admitted_generation: None,
            admitted_until: None,
        })
    }
}
struct Fixture {
    _data: TempDir,
    _identity: TempDir,
    store: Store,
    auth: Arc<Mutex<Auth>>,
    inputs: Arc<Inputs>,
}
fn dir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn fixture(owner: bool) -> Fixture {
    let d = dir();
    let i = dir();
    let store = Store::open(d.path()).unwrap();
    let mut auth = Auth::open(i.path(), store.handle()).unwrap();
    if owner {
        let k = SigningKey::from_bytes(&[76; 32]);
        let c = auth
            .bootstrap_challenge(k.verifying_key().to_bytes())
            .unwrap();
        auth.bootstrap_finish(&c.id, &k.sign(&c.transcript).to_bytes())
            .unwrap();
    }
    Fixture {
        _data: d,
        _identity: i,
        store,
        auth: Arc::new(Mutex::new(auth)),
        inputs: Arc::new(Inputs {
            ready: std::sync::atomic::AtomicBool::new(true),
        }),
    }
}
fn start(f: &Fixture) -> (UnixStream, thread::JoinHandle<Result<()>>) {
    let (client, server) = UnixStream::pair().unwrap();
    let a = f.auth.clone();
    let s = f.inputs.clone();
    let h = thread::spawn(move || connection(server, a, s, rustix::process::geteuid().as_raw()));
    (client, h)
}
fn request(s: &mut UnixStream, step: wire::Step, connection: Option<String>) -> Result<()> {
    let bytes = serde_json::to_vec(&wire::Request {
        protocol: wire::VERSION,
        step,
        challenge: "a".repeat(64),
        connection,
    })
    .unwrap();
    io::write(s, &bytes, Instant::now() + Duration::from_secs(1))
}
fn observe(s: &mut UnixStream) -> wire::Snapshot {
    request(s, wire::Step::Observe, None).unwrap();
    wire::parse_snapshot(&io::read(s, Instant::now() + Duration::from_secs(1), false).unwrap())
        .unwrap()
}
#[test]
fn real_auth_store_and_kernel_peer_hold_authority_and_actor_until_finish() {
    let f = fixture(true);
    let instance = f.auth.lock().unwrap().observer_instance();
    let realm = f.auth.lock().unwrap().durable_realm().unwrap();
    let (mut stream, server) = start(&f);
    let snapshot = observe(&mut stream);
    assert_eq!(snapshot.core_instance, instance);
    assert_eq!(snapshot.data_realm, realm);
    assert_eq!(snapshot.data_schema, 6);
    assert_eq!(snapshot.forwarding.policy, wire::Policy::Off);
    assert!(!snapshot.forwarding.worker_admitted);
    assert!(f.auth.try_lock().is_err());
    let (tx, rx) = std::sync::mpsc::channel();
    let h = f.store.handle();
    let reader = thread::spawn(move || {
        tx.send(h.cursor()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(35)).is_err());
    request(
        &mut stream,
        wire::Step::Recheck,
        Some(snapshot.connection.clone()),
    )
    .unwrap();
    let next = wire::parse_snapshot(
        &io::read(&mut stream, Instant::now() + Duration::from_secs(1), false).unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot, next);
    request(&mut stream, wire::Step::Finish, Some(snapshot.connection)).unwrap();
    assert_eq!(
        io::read(&mut stream, Instant::now() + Duration::from_secs(1), false).unwrap(),
        b"{\"protocol\":1,\"released\":true}"
    );
    server.join().unwrap().unwrap();
    assert!(rx.recv_timeout(Duration::from_secs(1)).unwrap().is_ok());
    reader.join().unwrap();
    assert!(f.auth.try_lock().is_ok());
}
#[test]
fn missing_owner_or_unready_business_never_produces_a_snapshot() {
    for owner in [false, true] {
        let f = fixture(owner);
        if owner {
            f.inputs
                .ready
                .store(false, std::sync::atomic::Ordering::Release);
        }
        let (mut s, h) = start(&f);
        request(&mut s, wire::Step::Observe, None).unwrap();
        assert!(io::read(&mut s, Instant::now() + Duration::from_secs(1), false).is_err());
        assert!(h.join().unwrap().is_err());
    }
}
#[test]
fn wrong_peer_unknown_fields_partial_frame_and_root_supplied_pid_are_denied() {
    let f = fixture(true);
    let (mut c, s) = UnixStream::pair().unwrap();
    let a = f.auth.clone();
    let input = f.inputs.clone();
    let h = thread::spawn(move || connection(s, a, input, u32::MAX));
    assert!(h.join().unwrap().is_err());
    for body in [
        br#"{"protocol":1,"step":"observe","challenge":"bad","connection":null,"pid":1}"#
            .as_slice(),
        b"{}",
    ] {
        let (mut s, h) = start(&f);
        io::write(&mut s, body, Instant::now() + Duration::from_secs(1)).unwrap();
        assert!(h.join().unwrap().is_err());
    }
    let (mut s, h) = start(&f);
    use std::io::Write;
    s.write_all(&[0, 0]).unwrap();
    assert!(h.join().unwrap().is_err());
    let _ = &mut c;
}
#[test]
fn foreign_connection_replay_and_business_shutdown_release_locks() {
    let f = fixture(true);
    let (mut s, h) = start(&f);
    let snapshot = observe(&mut s);
    request(&mut s, wire::Step::Recheck, Some("b".repeat(64))).unwrap();
    assert!(h.join().unwrap().is_err());
    assert!(f.auth.try_lock().is_ok());
    let (mut s, h) = start(&f);
    let fresh = observe(&mut s);
    assert_ne!(snapshot.connection, fresh.connection);
    f.inputs
        .ready
        .store(false, std::sync::atomic::Ordering::Release);
    request(&mut s, wire::Step::Recheck, Some(fresh.connection)).unwrap();
    assert!(h.join().unwrap().is_err());
    assert!(f.store.handle().cursor().is_ok());
}
#[test]
fn changed_data_inode_or_external_generation_cannot_pass_recheck() {
    let f = fixture(true);
    let (mut s, h) = start(&f);
    let snapshot = observe(&mut s);
    let p = f._data.path().join("store.sqlite3");
    fs::rename(&p, f._data.path().join("original")).unwrap();
    fs::write(&p, b"not the running database").unwrap();
    request(&mut s, wire::Step::Recheck, Some(snapshot.connection)).unwrap();
    // Identity is rechecked when the real actor releases as well as at observation.
    assert!(h.join().unwrap().is_err());
}
#[test]
fn disconnected_root_and_absolute_deadline_do_not_keep_the_business_writer() {
    let f = fixture(true);
    let (mut s, h) = start(&f);
    let _ = observe(&mut s);
    drop(s);
    assert!(h.join().unwrap().is_err());
    assert!(f.store.handle().cursor().is_ok());
    let until = Instant::now() + Duration::from_millis(80);
    let lease = f
        .auth
        .lock()
        .unwrap()
        .update_health_lease(until, false)
        .unwrap();
    thread::sleep(Duration::from_millis(100));
    assert!(lease.revalidate().is_err());
    drop(lease);
    assert!(f.store.handle().cursor().is_ok());
}

// Explicit private-container tests; never change host identities from ordinary tests.
#[test]
#[ignore = "run only inside the bounded Root UID isolation container"]
fn kernel_root_core_fixed_endpoint() {
    use std::os::unix::process::CommandExt;
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    for p in [
        "/run/rustsimhub",
        "/run/rustsimhub/update-health",
        "/test-state",
    ] {
        fs::create_dir_all(p).unwrap();
    }
    for p in ["/run/rustsimhub/update-health", "/test-state"] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        rustix::fs::chown(
            p,
            Some(rustix::process::Uid::from_raw(986)),
            Some(rustix::process::Gid::from_raw(986)),
        )
        .unwrap();
    }
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "update_health::tests::kernel_core_worker",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("RSH_WEB3_ISOLATED_WORKER", "yes")
        .uid(986)
        .gid(986)
        .spawn()
        .unwrap();
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Child(child);
    let until = Instant::now() + Duration::from_secs(4);
    let mut stream = loop {
        match UnixStream::connect(SOCKET) {
            Ok(s) => break s,
            Err(_) if Instant::now() < until => thread::sleep(Duration::from_millis(10)),
            Err(e) => panic!("fixed endpoint did not start: {e}"),
        }
    };
    let cred = rustix::net::sockopt::socket_peercred(&stream).unwrap();
    assert_eq!(cred.uid.as_raw(), 986);
    let snapshot = observe(&mut stream);
    assert!(snapshot.business_ready);
    assert_eq!(snapshot.data_schema, 6);
    assert_eq!(snapshot.authorization_generation, 1);
    assert_eq!(fs::metadata(SOCKET).unwrap().mode() & 0o7777, 0o600);
    let data = fs::metadata("/test-state/data/store.sqlite3").unwrap();
    assert_eq!(
        (data.dev(), data.ino()),
        (snapshot.data_device, snapshot.data_inode)
    );
    request(
        &mut stream,
        wire::Step::Recheck,
        Some(snapshot.connection.clone()),
    )
    .unwrap();
    assert_eq!(
        wire::parse_snapshot(&io::read(&mut stream, until, false).unwrap()).unwrap(),
        snapshot
    );
    request(&mut stream, wire::Step::Finish, Some(snapshot.connection)).unwrap();
    assert_eq!(
        io::read(&mut stream, until, false).unwrap(),
        b"{\"protocol\":1,\"released\":true}"
    );
    let until = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!Path::new(SOCKET).exists());
}
#[test]
#[ignore = "private worker spawned only by the bounded Root UID test"]
fn kernel_core_worker() {
    assert_eq!(
        std::env::var("RSH_WEB3_ISOLATED_WORKER").as_deref(),
        Ok("yes")
    );
    assert_eq!(rustix::process::geteuid().as_raw(), 986);
    for p in ["/test-state/data", "/test-state/identity"] {
        fs::create_dir(p).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let store = Store::open("/test-state/data").unwrap();
    let mut auth = Auth::open("/test-state/identity", store.handle()).unwrap();
    let key = SigningKey::from_bytes(&[76; 32]);
    let c = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    struct Source {
        done: std::sync::atomic::AtomicBool,
    }
    impl RuntimeSources for Source {
        fn business_ready(&self) -> bool {
            true
        } // Only the external readiness signal is synthetic.
        fn with_forwarder(
            &self,
            _: &Auth,
            work: &mut dyn FnMut(ForwarderLease) -> Result<()>,
        ) -> Result<()> {
            let result = work(ForwarderLease {
                queue_owned: false,
                admitted_generation: None,
                admitted_until: None,
            });
            self.done.store(true, std::sync::atomic::Ordering::Release);
            result
        }
    }
    let source = Arc::new(Source {
        done: std::sync::atomic::AtomicBool::new(false),
    });
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let server=serve(Arc::new(Mutex::new(auth)),source.clone());tokio::pin!(server);
        tokio::select!{r=&mut server=>panic!("server exited: {r:?}"),_=async {let until=tokio::time::Instant::now()+Duration::from_secs(6);while !source.done.load(std::sync::atomic::Ordering::Acquire){assert!(tokio::time::Instant::now()<until);tokio::time::sleep(Duration::from_millis(10)).await;}}=>{}}
    });
    drop(store);
}
