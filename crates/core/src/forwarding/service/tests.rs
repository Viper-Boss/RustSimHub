// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::forwarding::tests::{Fixture, CHANGE, REQUEST};
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::net::UnixStream;
use zbus::object_server::SignalEmitter;

#[derive(Clone, Copy)]
enum Mode {
    Done,
    Failed,
    Timeout,
    LostReply,
    WrongPath,
    WrongOwner,
    OldReport,
    ChangePolicy,
}
struct Bus {
    reads: AtomicUsize,
    change: bool,
}
#[zbus::interface(name = "org.freedesktop.DBus")]
impl Bus {
    fn get_name_owner(&self, name: &str) -> zbus::names::OwnedUniqueName {
        assert_eq!(name, NAME);
        let n = self.reads.fetch_add(1, Ordering::SeqCst);
        if self.change && n > 0 {
            ":1.81"
        } else {
            ":1.80"
        }
        .try_into()
        .unwrap()
    }
}
struct Manager {
    calls: Arc<AtomicUsize>,
    mode: Mode,
    report: PathBuf,
    controller: Controller,
    auth: Arc<Mutex<Auth>>,
    token: String,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn subscribe(&self) {}
    async fn start_unit(
        &self,
        unit: &str,
        mode: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert!([RECONCILE, STATUS].contains(&unit));
        assert_eq!(mode, "fail");
        self.calls.fetch_add(1, Ordering::SeqCst);
        // Real re-entrant Core dispatch proves no Auth/Management lock was
        // held across the DBus await. All data is synthetic; not PID1 proof.
        self.controller
            .passive_request(
                self.auth.clone(),
                AuthCommand::ForwardingStatus {
                    token: self.token.clone(),
                },
            )
            .await
            .map_err(|_| zbus::fdo::Error::Failed("locked".into()))?;
        let snapshot = {
            let mut a = self.auth.lock().unwrap();
            let mut s = self.controller.0.lock().unwrap();
            Snapshot::new(&mut a, &mut s, &self.token).unwrap().0
        };
        let now = uptime().unwrap();
        let value = json!({"format":1,"boot_id":boot().unwrap(),"core_instance":snapshot.core,
            "generation":snapshot.generation,"binding_sha256":snapshot.digest().unwrap(),
            "sample_uptime_ms":if matches!(self.mode,Mode::OldReport){0}else{now},
            "state":if snapshot.enabled{"running"}else{"disabled"},"process_state_verified":true,
            "worker_pid":if snapshot.enabled{Some(12345u32)}else{None}});
        fs::write(&self.report, serde_json::to_vec(&value).unwrap()).unwrap();
        fs::set_permissions(&self.report, fs::Permissions::from_mode(0o640)).unwrap();
        if matches!(self.mode, Mode::ChangePolicy) {
            self.controller
                .passive_request(
                    self.auth.clone(),
                    AuthCommand::SetForwardingEnabled {
                        token: self.token.clone(),
                        expected_generation: snapshot.generation,
                        request_id: CHANGE.into(),
                        enabled: !snapshot.enabled,
                    },
                )
                .await
                .unwrap();
        }
        let path: OwnedObjectPath = "/org/freedesktop/systemd1/job/17".try_into().unwrap();
        if !matches!(self.mode, Mode::Timeout) {
            let sent = if matches!(self.mode, Mode::WrongPath) {
                "/org/freedesktop/systemd1/job/18".try_into().unwrap()
            } else {
                path.clone()
            };
            Self::job_removed(
                &emitter,
                17,
                &sent,
                unit,
                if matches!(self.mode, Mode::Failed) {
                    "failed"
                } else {
                    "done"
                },
            )
            .await
            .unwrap();
        }
        if matches!(self.mode, Mode::LostReply) {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        Ok(path)
    }
    #[zbus(signal)]
    async fn job_removed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        job: &OwnedObjectPath,
        unit: &str,
        result: &str,
    ) -> zbus::Result<()>;
}
fn token(f: &Fixture) -> String {
    match f.configure(0, REQUEST, false) {
        AuthCommand::ForwardingConfigure { token, .. } => token,
        _ => unreachable!(),
    }
}
async fn managed(
    f: &mut Fixture,
    mode: Mode,
) -> (zbus::Connection, Arc<AtomicUsize>, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let report = directory.path().join("status.json");
    let calls = Arc::new(AtomicUsize::new(0));
    let (a, b) = UnixStream::pair().unwrap();
    let server = zbus::connection::Builder::unix_stream(a)
        .server(zbus::Guid::generate())
        .unwrap()
        .p2p()
        .serve_at(
            "/org/freedesktop/DBus",
            Bus {
                reads: AtomicUsize::new(0),
                change: matches!(mode, Mode::WrongOwner),
            },
        )
        .unwrap()
        .serve_at(
            PATH,
            Manager {
                calls: calls.clone(),
                mode,
                report: report.clone(),
                controller: f.control.clone(),
                auth: f.auth.clone(),
                token: token(f),
            },
        )
        .unwrap();
    let client = zbus::connection::Builder::unix_stream(b).p2p();
    let (server, client) = tokio::join!(server.build(), client.build());
    let server = server.unwrap();
    server.set_unique_name(":1.80").unwrap();
    f.control.1 = Some(Arc::new(Coordinator {
        slot: AsyncMutex::new(()),
        units: [RECONCILE.into(), STATUS.into()],
        report: REPORT.into(),
        test: Some(TestContext {
            connection: client.unwrap(),
            report,
            uid: rustix::process::geteuid().as_raw(),
            budget: if matches!(mode, Mode::Timeout | Mode::WrongPath | Mode::LostReply) {
                Duration::from_millis(400)
            } else {
                Duration::from_secs(4)
            },
        }),
    }));
    (server, calls, directory)
}
#[tokio::test]
async fn real_core_dispatch_requires_exact_successful_job_and_fresh_report() {
    for mode in [
        Mode::Done,
        Mode::Failed,
        Mode::Timeout,
        Mode::LostReply,
        Mode::WrongPath,
        Mode::WrongOwner,
        Mode::OldReport,
    ] {
        let mut f = Fixture::new();
        let (_server, calls, _directory) = managed(&mut f, mode).await;
        let result = f.request(f.configure(0, REQUEST, true)).await;
        assert_eq!(result["configuration_applied"], true, "{result}");
        assert_eq!(result["request_id"], REQUEST);
        assert_eq!(result["generation"], 1);
        assert_eq!(
            result["process_state_verified"],
            matches!(mode, Mode::Done),
            "{result}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!result.to_string().contains("synthetic-bearer-secret"));
        assert_eq!(
            f.control
                .0
                .lock()
                .unwrap()
                .queue
                .as_ref()
                .unwrap()
                .status()
                .unwrap()
                .generation,
            1
        );
        if matches!(mode, Mode::Done) {
            let retry = f.request(f.configure(0, REQUEST, true)).await;
            assert_eq!(retry["generation"], 1);
            assert_eq!(retry["process_state_verified"], true);
        }
    }
}
#[tokio::test]
async fn paired_client_and_concurrent_writes_cannot_admit_or_replace_a_job() {
    let mut f = Fixture::new();
    let (_server, calls, _directory) = managed(&mut f, Mode::Timeout).await;
    let delegated = crate::forwarding::tests::delegated(&f);
    assert_eq!(
        f.request(AuthCommand::ForwardingStatus {
            token: delegated.clone()
        })
        .await["error"],
        "denied"
    );
    assert_eq!(
        f.request(AuthCommand::SetForwardingEnabled {
            token: delegated,
            expected_generation: 0,
            request_id: REQUEST.into(),
            enabled: false
        })
        .await["error"],
        "denied"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!f.state.path().join("forwarding").exists());
    let c = f.control.clone();
    let a = f.auth.clone();
    let command = f.configure(0, REQUEST, true);
    let pending = tokio::spawn(async move { c.request(a, command).await.unwrap() });
    timeout(Duration::from_secs(1), async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let second = f.request(f.configure(1, CHANGE, false)).await;
    // Authenticated browser wire deliberately uses one generic refusal for
    // Busy and authorization errors; the second intent must remain unapplied.
    assert_eq!(second["error"], "denied");
    let first = pending.await.unwrap();
    assert_eq!(first["configuration_applied"], true);
    assert_eq!(first["service_action"], "unconfirmed");
    assert_eq!(first["process_state_verified"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.control
            .0
            .lock()
            .unwrap()
            .queue
            .as_ref()
            .unwrap()
            .status()
            .unwrap()
            .generation,
        1
    );
}
#[tokio::test]
async fn concurrent_policy_change_and_invalid_commands_never_return_verified() {
    let mut f = Fixture::new();
    let (_server, calls, _directory) = managed(&mut f, Mode::ChangePolicy).await;
    assert_eq!(
        f.request(f.configure(0, REQUEST, true)).await["error"],
        "denied"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let bad = f
        .request(AuthCommand::ForwardingStatus {
            token: "0".repeat(64),
        })
        .await;
    assert_eq!(bad["error"], "denied");
    let mut command = f.configure(2, REQUEST, false);
    if let AuthCommand::ForwardingConfigure { policy, .. } = &mut command {
        *policy = json!({"enabled":true});
    }
    assert_eq!(f.request(command).await["error"], "denied");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn default_off_can_be_verified_without_creating_an_outbox_or_forwarding_process() {
    let mut f = Fixture::new();
    let (_server, calls, _directory) = managed(&mut f, Mode::Done).await;
    let result = f
        .request(AuthCommand::ForwardingStatus { token: token(&f) })
        .await;
    assert_eq!(result["process_state_verified"], true, "{result}");
    assert_eq!(result["state"], "disabled");
    assert!(!f.state.path().join("forwarding").exists());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // The fake DBus manager supplies a report; actual no-PID proof is X1F,
    // not inferred from this fixture's JSON. Rules/receipt reads stay passive.
    f.request(AuthCommand::ForwardingRules { token: token(&f) })
        .await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[test]
fn root_report_metadata_and_all_binding_fields_are_checked() {
    let f = Fixture::new();
    let (snap, _) = Snapshot::new(
        &mut f.auth.lock().unwrap(),
        &mut f.control.0.lock().unwrap(),
        &token(&f),
    )
    .unwrap();
    let value = json!({"format":1,"boot_id":"boot","core_instance":snap.core,"generation":0,
        "binding_sha256":null,"sample_uptime_ms":100,"state":"disabled","process_state_verified":true,"worker_pid":null});
    let valid: Report = serde_json::from_value(value.clone()).unwrap();
    assert!(verified(&snap, &valid, 100, "boot", 101).unwrap());
    for (field, bad) in [
        ("format", json!(2)),
        ("boot_id", json!("other")),
        ("core_instance", json!("other")),
        ("generation", json!(1)),
        ("binding_sha256", json!(vec![0; 32])),
        ("sample_uptime_ms", json!(99)),
        ("sample_uptime_ms", json!(102)),
        ("state", json!("running")),
        ("process_state_verified", json!(false)),
        ("worker_pid", json!(9)),
    ] {
        let mut v = value.clone();
        v[field] = bad;
        assert!(!verified(&snap, &serde_json::from_value(v).unwrap(), 100, "boot", 101).unwrap());
    }
    assert!(!verified(&snap, &valid, 100, "boot", 10_101).unwrap());
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.path().join("status.json");
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let uid = rustix::process::geteuid().as_raw();
    assert!(read_report(&path, uid).is_ok());
    assert!(read_report(&path, uid + 1).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o660)).unwrap();
    assert!(read_report(&path, uid).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let second = directory.path().join("second");
    fs::hard_link(&path, &second).unwrap();
    assert!(read_report(&path, uid).is_err());
    fs::remove_file(&second).unwrap();
    fs::rename(&path, &second).unwrap();
    symlink(&second, &path).unwrap();
    assert!(read_report(&path, uid).is_err());
    assert_eq!(job_id("/org/freedesktop/systemd1/job/17"), Some(17));
    for invalid in [
        "/other/17",
        "/org/freedesktop/systemd1/job/0",
        "/org/freedesktop/systemd1/job/017",
    ] {
        assert_eq!(job_id(invalid), None);
    }
}
