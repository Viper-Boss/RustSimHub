// SPDX-License-Identifier: AGPL-3.0-only
//! Private native fixture. PID1/cgroup membership and signed-metadata capture
//! are synthetic; actual signatures/exe/UIDs/Unix/Core/Auth/Store are exercised.
use super::*;
use crate::{manifest_hash, signing_bytes, Entry, Manifest, BOARD};
use ed25519_dalek::{Signer, SigningKey};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command},
    sync::{Arc, Mutex},
};
const APP: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/app";
const DATA: &str = "/var/lib/rustsimhub/rustsimhub-v1/core/state";
const IDENTITY: &str = "/var/lib/rustsimhub/rustsimhub-v1/core/identity";
const VERSIONS: &str = "/opt/rustsimhub/versions";
const SELECTED: &str = "/run/rustsimhub-app/selected";
const TRUST: &str = "/usr/share/rustsimhub/release.public";
fn isolated() {
    assert_eq!(rustix::process::geteuid().as_raw(), 0);
    assert_eq!(
        fs::read("/fixture/RSH_WEB3_ISOLATED").unwrap(),
        b"private-native-health-only\n"
    );
    assert_eq!(
        std::env::var("RSH_WEB3_PRIVATE_CONTAINER").as_deref(),
        Ok("yes")
    );
    let info: serde_json::Value = serde_json::from_slice(
        &Command::new("/fixture/updater")
            .arg("--build-info")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert_eq!(
        info["source_archive_sha256"].as_str(),
        rustsimhub_wire::build_source_sha256()
    );
    assert_eq!(info["running_candidate_confirmation_protocol"], 1);
}
fn mkdir(path: &Path, uid: u32, mode: u32) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    rustix::fs::chown(
        path,
        Some(rustix::process::Uid::from_raw(uid)),
        Some(rustix::process::Gid::from_raw(uid)),
    )
    .unwrap();
}
struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn launch(exe: &Path, uid: u32, arguments: &[&str]) -> Reap {
    Reap(
        Command::new("/fixture/setpriv")
            .args(["--bounding-set=-all", "--clear-groups", "--no-new-privs"])
            .arg(format!("--reuid={uid}"))
            .arg(format!("--regid={uid}"))
            .arg(exe)
            .args(arguments)
            .env("RSH_WEB3_CORE_FIXTURE", "yes")
            .spawn()
            .unwrap(),
    )
}
struct KernelSources {
    pids: [u32; 3],
    cgroup: String,
}
impl Sources for KernelSources {
    fn boot(&self) -> Result<String> {
        linux::boot()
    }
    fn units(&self) -> Result<Vec<Unit>> {
        (0..4).map(|i|{
            let active=i<3;let pid=if active{self.pids[i]}else{0};let (name,_,user,_)=ROLES[i];
            units::parse(i,format!("Id={name}\nLoadState=loaded\nActiveState={}\nSubState={}\nMainPID={pid}\nUser={user}\nControlGroup={}\nNeedDaemonReload=no\nInvocationID={}\n",if active{"active"}else{"inactive"},if active{"running"}else{"dead"},if active{format!("/system.slice/{name}")}else{String::new()},if active{"a".repeat(32)}else{String::new()}).as_bytes())
        }).collect()
    }
    fn processes(&self, _units: &[Unit], slot: &Slot) -> Result<(Vec<Process>, WorkerState)> {
        let p = (0..3)
            .map(|i| linux::probe_verified(i, self.pids[i], ROLES[i].3, slot, &self.cgroup))
            .collect::<Result<Vec<_>>>()?;
        Ok((p, WorkerState::Off))
    }
}
struct Fixture {
    root: running::Root,
    core: Reap,
    edge: Reap,
    media: Reap,
    hash: String,
    key: [u8; 32],
}
impl Fixture {
    fn new() -> Self {
        isolated();
        for p in [
            APP,
            "/var/lib/rustsimhub/rustsimhub-v1/core",
            VERSIONS,
            "/run/rustsimhub-app",
            "/run/rustsimhub/update-health",
            running::SCRATCH,
        ] {
            if Path::new(p).exists() {
                fs::remove_dir_all(p).unwrap();
            }
        }
        for p in [
            "/test-state/stop",
            "/test-state/not-ready",
            "/test-state/exec",
        ] {
            let _ = fs::remove_file(p);
        }
        for p in [
            APP,
            VERSIONS,
            "/run/rustsimhub-app",
            "/usr/share/rustsimhub",
            "/run/rustsimhub",
        ] {
            mkdir(Path::new(p), 0, 0o755);
        }
        mkdir(
            Path::new("/var/lib/rustsimhub/rustsimhub-v1/core"),
            0,
            0o755,
        );
        for p in [DATA, IDENTITY, "/run/rustsimhub/update-health"] {
            mkdir(Path::new(p), 986, 0o700);
        }
        mkdir(&Path::new(APP).join("versions"), 0, 0o755);
        let signing = SigningKey::from_bytes(&[79; 32]);
        let key = signing.verifying_key().to_bytes();
        let source = rustsimhub_wire::build_source_sha256()
            .expect("compile with recorded source archive SHA");
        let binary = fs::read(std::env::current_exe().unwrap()).unwrap();
        let helper = fs::read("/fixture/sleep").unwrap();
        let payloads = crate::FORWARDING_FILES
            .iter()
            .map(|n| {
                if *n == "rustsimhub-core" {
                    binary.clone()
                } else if *n == "Opus-COPYING.txt" {
                    b"private synthetic license".to_vec()
                } else {
                    helper.clone()
                }
            })
            .collect::<Vec<_>>();
        let mut hashes = vec![];
        for epoch in [1, 2] {
            let manifest = Manifest {
                format: 2,
                kind: "application".into(),
                board: BOARD.into(),
                arch: "aarch64-linux".into(),
                epoch,
                schema_min: 6,
                schema_max: 6,
                source_sha256: source.into(),
                development_fixtures: false,
                files: crate::FORWARDING_FILES
                    .iter()
                    .zip(&payloads)
                    .map(|(name, p)| Entry {
                        name: (*name).into(),
                        bytes: p.len() as u64,
                        sha256: manifest_hash(p),
                    })
                    .collect(),
            };
            let bytes = serde_json::to_vec(&manifest).unwrap();
            let hash = manifest_hash(&bytes);
            if cfg!(target_arch = "x86_64") {
                assert_eq!(
                    crate::verify_payload(&manifest, &payloads).err(),
                    Some(Error::Board)
                );
            }
            for directory in [
                Path::new(VERSIONS).join(&hash),
                Path::new(APP).join("versions").join(&hash),
            ] {
                mkdir(&directory, 0, 0o755);
                for (name, value, mode) in [
                    ("manifest.json", bytes.clone(), 0o600),
                    (
                        "manifest.sig",
                        signing
                            .sign(&signing_bytes(&bytes).unwrap())
                            .to_bytes()
                            .to_vec(),
                        0o600,
                    ),
                ] {
                    fs::write(directory.join(name), value).unwrap();
                    fs::set_permissions(directory.join(name), fs::Permissions::from_mode(mode))
                        .unwrap();
                }
                for (entry, p) in manifest.files.iter().zip(&payloads) {
                    fs::write(directory.join(&entry.name), p).unwrap();
                    fs::set_permissions(
                        directory.join(&entry.name),
                        fs::Permissions::from_mode(0o555),
                    )
                    .unwrap();
                }
            }
            hashes.push(hash);
        }
        let hash = hashes[1].clone();
        fs::write(TRUST, key).unwrap();
        fs::set_permissions(TRUST, fs::Permissions::from_mode(0o444)).unwrap();
        let state = serde_json::json!({"format":1,"key_sha256":manifest_hash(&key),"floor":2,"confirmed":{"hash":hashes[0],"epoch":1},"candidate":{"hash":hash,"epoch":2},"attempted":true});
        for (name, value) in [
            ("state.json", serde_json::to_vec(&state).unwrap()),
            (".update.lock", vec![]),
        ] {
            let p = Path::new(APP).join(name);
            fs::write(&p, value).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
        }
        std::os::unix::fs::symlink(Path::new(VERSIONS).join(&hash), SELECTED).unwrap();
        let root = running::Root::native_fixture_install().unwrap();
        let directory = Path::new(VERSIONS).join(&hash);
        let core = launch(
            &directory.join("rustsimhub-core"),
            986,
            &[
                "--exact",
                "runtime_observation::confirmation::tests::live::kernel_core_worker",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ],
        );
        let edge = launch(&directory.join("rustsimhub-edge"), 985, &["30"]);
        let media = launch(&directory.join("rustsimhub-audio-alsa"), 984, &["30"]);
        let until = Instant::now() + Duration::from_secs(4);
        while !Path::new(wire::SOCKET).exists() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        Self {
            root,
            core,
            edge,
            media,
            hash,
            key,
        }
    }
    fn observe(self) -> (RunningCandidate, Reap, Reap, Reap) {
        let ledger = ReadOnlyState::observe().unwrap();
        let slot = Slot::native_fixture(&ledger, Path::new(SELECTED), Path::new(VERSIONS)).unwrap();
        let cgroups = fs::read_to_string("/proc/self/cgroup").unwrap();
        let cgroup = cgroups
            .lines()
            .find_map(|l| l.strip_prefix("0::"))
            .expect("private cgroup2")
            .to_owned();
        let sources = KernelSources {
            pids: [self.core.0.id(), self.edge.0.id(), self.media.0.id()],
            cgroup,
        };
        let runtime = RuntimeObservation::capture(
            ledger,
            slot,
            linux::boot().unwrap(),
            Instant::now(),
            Box::new(sources),
        )
        .unwrap();
        let plan = runtime.ledger.native_fixture_plan().unwrap();
        // Explicitly synthetic build-info subprocess output, not a production
        // attestation. Native signed bytes and process binding are still real.
        let metadata = |group: &crate::store::SignedGroup| {
            let contract = rustsimhub_forwarder::runtime_compatibility();
            let core=serde_json::to_vec(&serde_json::json!({"component":"core","target_arch":"aarch64","source_archive_sha256":rustsimhub_wire::build_source_sha256(),"development_fixtures":false,"database_schema":6,"update_health_protocol":1,"update_health_default_enabled":false,"service_launch":{"format":1,"modes":["device","forwarding_managed"]},"forwarding_runtime_contract":contract})).unwrap();
            let worker=serde_json::to_vec(&serde_json::json!({"worker":true,"protocol":4,"source_archive_sha256":rustsimhub_wire::build_source_sha256(),"development_fixtures":false,"forwarding_runtime_contract":contract})).unwrap();
            SignedPrograms::native_fixture(group, &self.key, core, worker).unwrap()
        };
        let candidate = metadata(&runtime.slot.signed().unwrap());
        let baseline = metadata(plan.baseline());
        let core = Core::connect(&runtime).unwrap();
        let data = running::Data::observe(&core.snapshot).unwrap();
        running::compatible(&core.snapshot, &candidate, &baseline).unwrap();
        let cap = RunningCandidate {
            runtime,
            root: self.root,
            data,
            core,
            plan,
            invalid: Cell::new(false),
        };
        cap.revalidate().unwrap();
        (cap, self.core, self.edge, self.media)
    }
}
fn stop(mut child: Reap) {
    fs::write("/test-state/stop", b"stop").unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
#[ignore = "private Root native fixture: synthetic PID1 and metadata capture adapter"]
fn kernel_real_core_confirm_once_and_failures() {
    isolated();
    let f = Fixture::new();
    let hash = f.hash.clone();
    let (cap, core, edge, media) = f.observe();
    assert_eq!(cap.core.snapshot.forwarding.policy, wire::Policy::Off);
    assert_eq!(cap.runtime.slot.manifest.format, 2);
    let result = cap.confirm().unwrap();
    assert_eq!(result.manifest_sha256, hash);
    assert_eq!(result.floor_epoch, 2);
    let ledger = ReadOnlyState::observe().unwrap();
    let value = ledger.report().unwrap();
    assert_eq!(value["state"]["confirmed"]["hash"], hash);
    assert_eq!(value["state"]["floor"], 2);
    assert_eq!(value["state"]["attempted"], false);
    assert!(value["state"]["candidate"].is_null());
    assert!(ledger.confirmation_plan().is_err());
    drop(ledger);
    let first = Command::new("/fixture/updater")
        .arg("inspect-state")
        .output()
        .unwrap();
    let second = Command::new("/fixture/updater")
        .arg("inspect-state")
        .output()
        .unwrap();
    assert!(first.status.success() && second.status.success());
    assert_eq!(first.stdout, second.stdout);
    let queried: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(queried["state"]["confirmed"]["hash"], hash);
    assert_eq!(queried["state"]["floor"], 2);
    stop(core);
    assert!(!Path::new(wire::SOCKET).exists());
    drop((edge, media));
    // Real CLI rejects exact-command extras/nonroot before writable recovery.
    let f = Fixture::new();
    let old = fs::read(Path::new(APP).join("state.json")).unwrap();
    let pending = Path::new(APP).join("state.next");
    fs::write(&pending, &old).unwrap();
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o600)).unwrap();
    for args in [
        vec!["confirm-running-candidate"],
        vec!["confirm-running-candidate", "--healthy", "true"],
        vec!["confirm-running-candidate", APP, TRUST],
        vec!["confirm-running-candidate", &f.hash],
    ] {
        let output = Command::new("/fixture/updater")
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read(&pending).unwrap(), old);
        assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), old);
    }
    let output = Command::new("/fixture/setpriv")
        .args([
            "--bounding-set=-all",
            "--clear-groups",
            "--no-new-privs",
            "--reuid=986",
            "--regid=986",
            "/fixture/updater",
            "confirm-running-candidate",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&pending).unwrap(), old);
    stop(f.core);
    drop((f.edge, f.media));
    // Actual process exit after scratch fsync, BEFORE any authorized intent.
    let f = Fixture::new();
    let old = fs::read(Path::new(APP).join("state.json")).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime_observation::confirmation::tests::live::kernel_abort_after_prepare",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(91));
    assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), old);
    assert!(Path::new(running::SCRATCH)
        .join("confirmation.next")
        .exists());
    assert!(!Path::new(APP).join("state.next").exists());
    let store = crate::store::Store::open(Path::new(APP), f.key, 6).unwrap();
    assert_eq!(store.status()["candidate_attempted"], true);
    assert_eq!(store.floor(), 2);
    drop(store);
    assert_eq!(fs::read(Path::new(APP).join("state.json")).unwrap(), old);
    stop(f.core);
    drop((f.edge, f.media));

    for failure in [
        "ready",
        "root",
        "data",
        "revoke",
        "schema",
        "identity",
        "payload",
        "signature",
        "policy",
        "instance",
        "peer",
        "death",
        "exec",
        "expired",
        "socket",
    ] {
        let f = Fixture::new();
        let (mut cap, mut core, edge, media) = f.observe();
        let old = fs::read(Path::new(APP).join("state.json")).unwrap();
        match failure {
            "ready" => fs::write("/test-state/not-ready", b"no").unwrap(),
            "root" => fs::write(
                "/etc/systemd/system/rustsimhub-core.service.d/55-update-health.conf",
                b"changed",
            )
            .unwrap(),
            "data" => {
                let c = rusqlite::Connection::open(Path::new(DATA).join("store.sqlite3")).unwrap();
                c.execute(
                    "UPDATE meta SET line_epoch=line_epoch+1 WHERE singleton=1",
                    [],
                )
                .unwrap();
            }
            "revoke" => {
                let c = rusqlite::Connection::open(Path::new(DATA).join("store.sqlite3")).unwrap();
                c.execute("UPDATE principals SET active=0 WHERE kind='owner'", [])
                    .unwrap();
            }
            "schema" => {
                let c = rusqlite::Connection::open(Path::new(DATA).join("store.sqlite3")).unwrap();
                c.pragma_update(None, "user_version", 9).unwrap();
            }
            "identity" => {
                fs::rename(
                    Path::new(IDENTITY).join("identity.seed"),
                    Path::new(IDENTITY).join("identity.old"),
                )
                .unwrap();
                fs::write(Path::new(IDENTITY).join("identity.seed"), b"replaced").unwrap();
            }
            "payload" => {
                let p = cap.runtime.slot.directory.join("rustsimhub-edge");
                let next = p.with_extension("next");
                fs::write(&next, b"changed").unwrap();
                fs::rename(next, p).unwrap();
            }
            "signature" => fs::write(
                cap.runtime.slot.directory.join("manifest.sig"),
                b"changed signature",
            )
            .unwrap(),
            "policy" => {
                cap.core.snapshot.forwarding.policy = wire::Policy::On;
                cap.core.snapshot.forwarding.worker_admitted = true;
            }
            "instance" => cap.core.snapshot.core_instance = "c".repeat(64),
            "exec" => {
                fs::write("/test-state/exec", b"exec").unwrap();
                let until = Instant::now() + Duration::from_secs(1);
                while fs::read_link(format!("/proc/{}/exe", core.0.id())).unwrap()
                    != Path::new("/fixture/sleep")
                {
                    assert!(Instant::now() < until);
                    std::thread::sleep(Duration::from_millis(5));
                }
                assert!(core.0.try_wait().unwrap().is_none());
            }
            "peer" => {
                assert!(Core::connect(&cap.runtime).is_err());
            }
            "death" => {
                core.0.kill().unwrap();
                core.0.wait().unwrap();
            }
            "expired" => std::thread::sleep(Duration::from_millis(wire::MAX_OBSERVATION_MS + 40)),
            "socket" => {
                fs::remove_file(wire::SOCKET).unwrap();
                fs::write(wire::SOCKET, b"foreign Root node").unwrap();
            }
            _ => unreachable!(),
        }
        if failure == "peer" {
            drop(cap);
        } else {
            assert!(cap.confirm().is_err(), "{failure}");
        }
        assert_eq!(
            fs::read(Path::new(APP).join("state.json")).unwrap(),
            old,
            "{failure}"
        );
        assert!(!Path::new(APP).join("state.next").exists());
        if matches!(failure, "death" | "exec") {
            if failure == "exec" {
                core.0.kill().unwrap();
                core.0.wait().unwrap();
            }
            drop(core);
        } else {
            stop(core);
        }
        if failure == "socket" {
            assert_eq!(fs::read(wire::SOCKET).unwrap(), b"foreign Root node");
            fs::remove_file(wire::SOCKET).unwrap();
        }
        if matches!(failure, "death" | "exec") {
            let _ = fs::remove_file(wire::SOCKET);
        }
        drop((edge, media));
    }
}
#[test]
#[ignore = "private UID986 Core worker spawned only by confirmation fixture"]
fn kernel_core_worker() {
    assert_eq!(std::env::var("RSH_WEB3_CORE_FIXTURE").as_deref(), Ok("yes"));
    assert_eq!(rustix::process::geteuid().as_raw(), 986);
    let store = rustsimhub_storage::Store::open(DATA).unwrap();
    let mut auth = rustsimhub_auth::Auth::open(IDENTITY, store.handle()).unwrap();
    let key = SigningKey::from_bytes(&[76; 32]);
    let c = auth
        .bootstrap_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap();
    struct External;
    impl rustsimhub_core::update_health::RuntimeSources for External {
        fn business_ready(&self) -> bool {
            !Path::new("/test-state/not-ready").exists()
        }
        fn with_forwarder(
            &self,
            _: &rustsimhub_auth::Auth,
            work: &mut dyn FnMut(
                rustsimhub_core::update_health::ForwarderLease,
            ) -> rustsimhub_core::update_health::Result<()>,
        ) -> rustsimhub_core::update_health::Result<()> {
            work(rustsimhub_core::update_health::ForwarderLease {
                queue_owned: false,
                admitted_generation: None,
                admitted_until: None,
            })
        }
    }
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
        let server=rustsimhub_core::update_health::serve(Arc::new(Mutex::new(auth)),Arc::new(External));tokio::pin!(server);
        tokio::select!{r=&mut server=>panic!("health server {r:?}"),_=async{let until=tokio::time::Instant::now()+Duration::from_secs(20);while !Path::new("/test-state/stop").exists(){
            if Path::new("/test-state/exec").exists() {
                use std::os::unix::process::CommandExt;
                panic!("exec failed: {}",Command::new("/fixture/sleep").arg("30").exec());
            }
            assert!(tokio::time::Instant::now()<until);tokio::time::sleep(Duration::from_millis(10)).await;}}=>{}}
    });
    drop(store);
}

#[test]
#[ignore = "private crash child: exit91 after durable scratch, before authority gate"]
fn kernel_abort_after_prepare() {
    isolated();
    let ledger = ReadOnlyState::observe().unwrap();
    let plan = ledger.native_fixture_plan().unwrap();
    let mut store = ledger.exclusive(&plan).unwrap();
    let _ = store.confirm_live(&plan, Path::new(running::SCRATCH), || {
        std::process::exit(91)
    });
    panic!("crash point was not reached");
}
