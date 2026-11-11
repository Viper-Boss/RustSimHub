// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::{symlink, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    rc::Rc,
};

const BOOT: &str = "12345678-abcd-1234-abcd-123456789abc";
struct Fixture {
    dir: tempfile::TempDir,
    root: PathBuf,
    trust: PathBuf,
    versions: PathBuf,
    selected: PathBuf,
    hash: String,
}
impl Fixture {
    fn new(format: u16) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("app");
        let runtime = dir.path().join("runtime");
        let versions = dir.path().join("versions");
        for d in [&root, &runtime, &versions] {
            fs::create_dir(d).unwrap();
            fs::set_permissions(d, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let trust = dir.path().join("release.public");
        let (key, mut m, payloads) = if format == 2 {
            crate::tests::forwarding_fixture()
        } else {
            crate::tests::fixture()
        };
        m.schema_max = 6;
        let bytes = serde_json::to_vec(&m).unwrap();
        let hash = crate::manifest_hash(&bytes);
        fs::write(&trust, key.verifying_key().to_bytes()).unwrap();
        fs::set_permissions(&trust, fs::Permissions::from_mode(0o644)).unwrap();
        let slot = versions.join(&hash);
        fs::create_dir(&slot).unwrap();
        fs::write(slot.join("manifest.json"), &bytes).unwrap();
        fs::write(
            slot.join("manifest.sig"),
            key.sign(&crate::signing_bytes(&bytes).unwrap()).to_bytes(),
        )
        .unwrap();
        for (entry, bytes) in m.files.iter().zip(payloads) {
            fs::write(slot.join(&entry.name), bytes).unwrap();
        }
        fs::write(root.join(".update.lock"), b"").unwrap();
        fs::set_permissions(root.join(".update.lock"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&slot, fs::Permissions::from_mode(0o755)).unwrap();
        for e in fs::read_dir(&slot).unwrap() {
            fs::set_permissions(e.unwrap().path(), fs::Permissions::from_mode(0o644)).unwrap();
        }
        let selected = runtime.join("selected");
        symlink(&slot, &selected).unwrap();
        let f = Self {
            dir,
            root,
            trust,
            versions,
            selected,
            hash,
        };
        f.state(serde_json::json!({"format":1,"key_sha256":crate::manifest_hash(&key.verifying_key().to_bytes()),
            "floor":1,"confirmed":{"hash":f.hash,"epoch":1},"candidate":null,"attempted":false}));
        f
    }
    fn state(&self, state: serde_json::Value) {
        fs::write(
            self.root.join("state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        fs::set_permissions(
            self.root.join("state.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
    }
    fn edit_state(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let mut value =
            serde_json::from_slice(&fs::read(self.root.join("state.json")).unwrap()).unwrap();
        edit(&mut value);
        self.state(value);
    }
    fn ledger(&self) -> Result<ReadOnlyState> {
        ReadOnlyState::runtime_test_at(&self.root, &self.trust)
    }
    fn slot(&self, ledger: &ReadOnlyState) -> Result<Slot> {
        Slot::at(ledger, &self.selected, &self.versions, &[self.dir.path()])
    }
    fn observe(&self, port: Rc<Port>, started: Instant) -> Result<RuntimeObservation> {
        let ledger = self.ledger()?;
        let slot = self.slot(&ledger)?;
        RuntimeObservation::capture(ledger, slot, BOOT.into(), started, Box::new(port))
    }
    fn inventory(&self) -> BTreeMap<PathBuf, (u32, u64, Vec<u8>)> {
        fn walk(root: &Path, map: &mut BTreeMap<PathBuf, (u32, u64, Vec<u8>)>) {
            for e in fs::read_dir(root).unwrap() {
                let path = e.unwrap().path();
                let m = fs::symlink_metadata(&path).unwrap();
                let bytes = if m.file_type().is_symlink() {
                    fs::read_link(&path)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else if m.is_file() {
                    fs::read(&path).unwrap()
                } else {
                    Vec::new()
                };
                map.insert(
                    path.clone(),
                    (m.mode(), std::os::unix::fs::MetadataExt::ino(&m), bytes),
                );
                if m.is_dir() {
                    walk(&path, map);
                }
            }
        }
        let mut map = BTreeMap::new();
        walk(self.dir.path(), &mut map);
        map
    }
}
fn unit_bytes(index: usize, pid: u32) -> Vec<u8> {
    let (name, _, user, _) = ROLES[index];
    let (active, sub, cg, invocation) = if pid == 0 {
        ("inactive", "dead", String::new(), String::new())
    } else {
        (
            "active",
            "running",
            format!("/system.slice/{name}"),
            "a".repeat(32),
        )
    };
    format!("Id={name}\nLoadState=loaded\nActiveState={active}\nSubState={sub}\nMainPID={pid}\nUser={user}\nControlGroup={cg}\nNeedDaemonReload=no\nInvocationID={invocation}\n").into_bytes()
}
#[derive(Default)]
struct Port {
    fault: Cell<u8>,
    calls: Cell<u32>,
    race: Cell<bool>,
    worker: Cell<bool>,
}
impl Sources for Rc<Port> {
    fn boot(&self) -> Result<String> {
        Ok(if self.fault.get() == 1 {
            "87654321-abcd-1234-abcd-123456789abc"
        } else {
            BOOT
        }
        .into())
    }
    fn units(&self) -> Result<Vec<Unit>> {
        self.calls.set(self.calls.get() + 1);
        (0..4)
            .map(|i| {
                let mut pid = if i == 3 && !self.worker.get() {
                    0
                } else {
                    i as u32 + 100
                };
                if i == 0 && (self.fault.get() == 2 || self.race.get() && self.calls.get() >= 3) {
                    pid += 10;
                }
                units::parse(i, &unit_bytes(i, pid))
            })
            .collect()
    }
    fn processes(&self, units: &[Unit], slot: &Slot) -> Result<(Vec<Process>, WorkerState)> {
        if self.fault.get() == 3 {
            return Err(Error::Integrity);
        }
        let processes = units
            .iter()
            .filter(|u| u.active)
            .map(|u| {
                Process::synthetic(u.index, u.pid, &slot.directory.join(ROLES[u.index].1))
                    .with_test_start(if self.fault.get() == 4 { 2 } else { 1 })
            })
            .collect();
        Ok((
            processes,
            if self.worker.get() {
                WorkerState::On
            } else {
                WorkerState::Off
            },
        ))
    }
}

#[test]
fn real_signed_slots_and_synthetic_units_report_no_health_or_authority_and_no_private_ids() {
    for format in [1, 2] {
        let f = Fixture::new(format);
        let before = f.inventory();
        let observation = f.observe(Rc::new(Port::default()), Instant::now()).unwrap();
        for _ in 0..2 {
            let bytes = serde_json::to_vec(&observation.report().unwrap()).unwrap();
            assert!(bytes.len() < 4096);
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["read_only"], true);
            assert_eq!(json["forwarder_worker"], "off");
            assert_eq!(json["forwarder_policy"], "unknown");
            assert_eq!(json["processes"].as_array().unwrap().len(), 3);
            for field in [
                "health_verified",
                "authorization_verified",
                "may_confirm",
                "core_authorization_boot_verified",
                "business_ready_verified",
                "data_generation_verified",
                "forwarder_policy_relation_verified",
            ] {
                assert_eq!(json[field], false, "{field}");
            }
            let text = String::from_utf8(bytes).unwrap();
            for forbidden in [BOOT, "pid", "start_ticks", "cgroup", "identity", "settings"] {
                assert!(!text.contains(forbidden));
            }
        }
        assert_eq!(before, f.inventory());
    }
}
#[test]
fn attempted_candidate_and_merely_staged_candidate_cannot_cross_roles() {
    for attempted in [false, true] {
        let f = Fixture::new(1);
        f.edit_state(|v| {
            v["candidate"] = v["confirmed"].clone();
            v["confirmed"]["hash"] = "b".repeat(64).into();
            v["attempted"] = attempted.into();
        });
        let before = f.inventory();
        let ledger = f.ledger().unwrap();
        assert_eq!(f.slot(&ledger).is_ok(), attempted);
        assert_eq!(before, f.inventory());
    }
    let f = Fixture::new(2);
    f.edit_state(|v| v["candidate"] = serde_json::json!({"hash":"b".repeat(64),"epoch":1}));
    let o = f.observe(Rc::new(Port::default()), Instant::now()).unwrap();
    assert_eq!(
        serde_json::to_value(o.report().unwrap()).unwrap()["selection"],
        "confirmed"
    );
}
#[test]
fn missing_wrong_trust_pending_state_and_lock_competition_leave_no_writes() {
    for case in [
        "state",
        "trust",
        "lock",
        "next",
        "wrong-key",
        "lock-contention",
    ] {
        let f = Fixture::new(1);
        match case {
            "state" => fs::remove_file(f.root.join("state.json")).unwrap(),
            "trust" => fs::remove_file(&f.trust).unwrap(),
            "lock" => fs::remove_file(f.root.join(".update.lock")).unwrap(),
            "next" => fs::write(f.root.join("state.next"), b"pending").unwrap(),
            "wrong-key" => fs::write(
                &f.trust,
                SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes(),
            )
            .unwrap(),
            _ => (),
        }
        let lock = fs::File::open(f.root.join(".update.lock")).ok();
        if case == "lock-contention" {
            fs2::FileExt::try_lock_exclusive(lock.as_ref().unwrap()).unwrap();
        }
        let before = f.inventory();
        assert!(f.ledger().is_err(), "{case}");
        assert_eq!(before, f.inventory());
    }
}
#[test]
fn selected_escape_next_wrong_signature_payload_source_and_extra_files_fail_read_only() {
    for case in [
        "escape",
        "relative",
        "next",
        "signature",
        "payload",
        "source",
        "extra",
        "symlink",
        "fifo",
        "oversize",
    ] {
        let f = Fixture::new(2);
        let directory = f.versions.join(&f.hash);
        match case {
            "escape" | "relative" => {
                fs::remove_file(&f.selected).unwrap();
                symlink(
                    if case == "escape" {
                        Path::new("/etc")
                    } else {
                        Path::new("../versions")
                    },
                    &f.selected,
                )
                .unwrap();
            }
            "next" => symlink(&directory, f.selected.with_file_name("selected.next")).unwrap(),
            "signature" => fs::write(directory.join("manifest.sig"), [0; 64]).unwrap(),
            "payload" => fs::write(directory.join(ROLES[0].1), [0; 64]).unwrap(),
            "source" => {
                let p = directory.join("manifest.json");
                let value = String::from_utf8(fs::read(&p).unwrap())
                    .unwrap()
                    .replace(&"a".repeat(64), &"b".repeat(64));
                fs::write(p, value).unwrap();
            }
            "extra" => fs::write(directory.join("extra"), b"unlisted").unwrap(),
            "symlink" => {
                fs::remove_file(directory.join(ROLES[0].1)).unwrap();
                symlink("/etc/passwd", directory.join(ROLES[0].1)).unwrap();
            }
            "fifo" => {
                let p = directory.join(ROLES[0].1);
                fs::remove_file(&p).unwrap();
                rustix::fs::mknodat(
                    rustix::fs::CWD,
                    p,
                    rustix::fs::FileType::Fifo,
                    rustix::fs::Mode::RUSR,
                    0,
                )
                .unwrap();
            }
            _ => fs::write(
                directory.join("manifest.json"),
                vec![0; crate::MAX_MANIFEST + 1],
            )
            .unwrap(),
        }
        // FIFO is excluded from inventory's file-content reads.
        let before = f.inventory();
        let ledger = f.ledger().unwrap();
        assert!(f.slot(&ledger).is_err(), "{case}");
        assert_eq!(before, f.inventory());
    }
}
#[test]
fn held_slot_detects_link_inode_payload_directory_and_pending_replacement() {
    for case in ["link", "payload", "extra", "next", "state", "lock"] {
        let f = Fixture::new(1);
        let ledger = f.ledger().unwrap();
        let slot = f.slot(&ledger).unwrap();
        match case {
            "link" => {
                fs::remove_file(&f.selected).unwrap();
                symlink(&slot.directory, &f.selected).unwrap();
            }
            "payload" => fs::write(slot.directory.join(ROLES[0].1), [0; 64]).unwrap(),
            "extra" => fs::write(slot.directory.join("other"), b"").unwrap(),
            "next" => fs::write(f.root.join("state.next"), b"").unwrap(),
            "state" => f.edit_state(|v| v["floor"] = 2.into()),
            _ => {
                fs::rename(f.root.join(".update.lock"), f.root.join("old.lock")).unwrap();
                fs::write(f.root.join(".update.lock"), b"").unwrap();
            }
        }
        assert!(slot.revalidate(&ledger).is_err(), "{case}");
    }
}
#[test]
fn boot_unit_pid_process_and_mid_collection_change_permanently_invalidate() {
    for fault in 1..=4 {
        let f = Fixture::new(2);
        let port = Rc::new(Port::default());
        let o = f.observe(port.clone(), Instant::now()).unwrap();
        port.fault.set(fault);
        assert!(o.report().is_err());
        port.fault.set(0);
        assert!(o.report().is_err());
    }
    let f = Fixture::new(2);
    let port = Rc::new(Port::default());
    port.race.set(true);
    assert!(f.observe(port, Instant::now()).is_err());
}
#[test]
fn future_expired_and_initial_deadline_are_not_renewed() {
    let f = Fixture::new(1);
    for start in [
        Instant::now() + Duration::from_secs(10),
        Instant::now() - MAX_AGE,
    ] {
        assert!(f.observe(Rc::new(Port::default()), start).is_err());
    }
    let mut o = f.observe(Rc::new(Port::default()), Instant::now()).unwrap();
    let original = o.started;
    o.report().unwrap();
    assert_eq!(o.started, original);
    o.started = Instant::now() - MAX_AGE;
    assert!(o.report().is_err());
    o.started = Instant::now();
    assert!(o.report().is_err());
}
#[test]
fn strict_systemd_parser_rejects_wrong_mapping_duplicate_foreign_and_bad_output() {
    let good = String::from_utf8(unit_bytes(0, 42)).unwrap();
    units::parse(0, good.as_bytes()).unwrap();
    for bad in [
        good.replace("MainPID=42", "MainPID=0"),
        good.replace("MainPID=42", "MainPID=042"),
        good.replace("User=rustsimhub-core", "User=root"),
        good.replace("Id=rustsimhub-core", "Id=rustsimhub-edge"),
        good.replace("/system.slice/rustsimhub-core", "/foreign/rustsimhub-core"),
        good.replace("NeedDaemonReload=no", "NeedDaemonReload=yes"),
        good.replace("LoadState=loaded", "LoadState=not-found"),
        good.replace("InvocationID=", "Invalid="),
        format!("{good}MainPID=42\n"),
        good.replace("User=rustsimhub-core\n", ""),
        format!("{good}ExecStart=/bin/sh\n"),
        String::new(),
    ] {
        assert!(units::parse(0, bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(units::parse(0, &[255]).is_err());
    assert!(!units::parse(3, &unit_bytes(3, 0)).unwrap().active);
    units::parse(3, &unit_bytes(3, 43)).unwrap();
}
#[test]
fn malformed_security_cgroup_and_residual_subgroups_are_rejected() {
    let good = b"Uid:\t986 986 986 986\nNoNewPrivs:\t1\nCapInh:\t0\nCapPrm:\t0\nCapEff:\t0\nCapAmb:\t0\nCapBnd:\t0\n";
    linux::check_security(good, 986).unwrap();
    let text = std::str::from_utf8(good).unwrap();
    for bad in [
        text.replace("986", "985"),
        text.replace("NoNewPrivs:\t1", "NoNewPrivs:\t0"),
        text.replace("CapEff:\t0", "CapEff:\t1"),
        text.replace("CapPrm:\t0", "CapPrm:\t1"),
        text.replace("CapInh:\t0", "CapInh:\t1"),
        text.replace("CapAmb:\t0", "CapAmb:\t1"),
        format!("{text}CapEff:\t0\n"),
        text.replace("CapAmb:\t0\n", ""),
    ] {
        assert!(linux::check_security(bad.as_bytes(), 986).is_err());
    }
    linux::check_cgroups("0::/system.slice/unit\n", "/system.slice/unit").unwrap();
    linux::check_cgroups(
        "1:name=systemd:/system.slice/unit\n2:cpu:/system.slice/unit\n",
        "/system.slice/unit",
    )
    .unwrap();
    for bad in [
        "0::/other\n",
        "0::/system.slice/unit\n0::/system.slice/unit\n",
        "2:cpu:/system.slice/unit\n",
        "0::/system.slice/unit/../unit\n",
        "bad",
        "0::/system.slice/unit\n1:name=systemd:/system.slice/unit\n",
    ] {
        assert!(linux::check_cgroups(bad, "/system.slice/unit").is_err());
    }
    let d = tempfile::tempdir().unwrap();
    fs::write(d.path().join("cgroup.procs"), b"").unwrap();
    assert!(linux::read_members(d.path(), true).unwrap().is_empty());
    let child = d.path().join("residual");
    fs::create_dir(&child).unwrap();
    fs::write(child.join("cgroup.procs"), b"42\n").unwrap();
    assert_eq!(
        linux::read_members(d.path(), true).unwrap(),
        std::collections::BTreeSet::from([42])
    );
    fs::write(d.path().join("cgroup.procs"), b"42\n").unwrap();
    assert!(linux::read_members(d.path(), true).is_err());
}
#[test]
fn linux_boot_uuid_is_not_a_core_authorization_boot() {
    assert_eq!(
        linux::parse_boot(format!("{BOOT}\n").as_bytes()).unwrap(),
        BOOT
    );
    for bad in [
        "a".repeat(64),
        format!("{BOOT}\nextra"),
        BOOT.to_string(),
        "x".repeat(36),
    ] {
        assert!(linux::parse_boot(bad.as_bytes()).is_err());
    }
    linux::boot().unwrap();
}

struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn real_linux_child_exe_uid_nnp_and_exit_are_checked_using_existing_probe() {
    let mut command = Command::new(if Path::new("/usr/bin/python3").exists() {
        "/usr/bin/python3"
    } else {
        "python3"
    });
    command.args(["-c", "import ctypes,time; assert ctypes.CDLL(None).prctl(38,1,0,0,0)==0; print('ready',flush=True); time.sleep(20)"])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = Reap(command.spawn().unwrap());
    let mut ready = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    let pid = child.0.id();
    let exe = fs::read_link(format!("/proc/{pid}/exe")).unwrap();
    let digest = crate::manifest_hash(&fs::read(&exe).unwrap());
    let uid = rustix::process::geteuid().as_raw();
    let cgroups = fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap();
    let cg = cgroups
        .lines()
        .find(|l| l.starts_with("0::") || l.contains(":name=systemd:"))
        .unwrap()
        .splitn(3, ':')
        .nth(2)
        .unwrap();
    let existing = crate::health::probe_process(pid, ROLES[0].1).unwrap();
    assert_eq!(existing.uid, uid);
    assert_eq!(existing.exe_sha256, digest);
    assert!(existing.no_new_privileges);
    // The unprivileged outer host retains its bounding set. Root/cap-drop ALL
    // private container runs exercise the stricter complete positive as well.
    let status = fs::read(format!("/proc/{pid}/status")).unwrap();
    if linux::check_security(&status, uid).is_ok() {
        let p = linux::probe(0, pid, uid, &exe, &digest, cg).unwrap();
        assert_eq!(p, linux::probe(0, pid, uid, &exe, &digest, cg).unwrap());
    } else {
        assert!(linux::probe(0, pid, uid, &exe, &digest, cg).is_err());
    }
    assert!(linux::probe(0, pid, uid + 1, &exe, &digest, cg).is_err());
    assert!(linux::probe(0, pid, uid, &exe, &"0".repeat(64), cg).is_err());
    assert!(linux::probe(0, pid, uid, Path::new("/bin/sleep"), &digest, cg).is_err());
    assert!(linux::probe(0, pid, uid, &exe, &digest, "/foreign").is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert!(linux::probe(0, pid, uid, &exe, &digest, cg).is_err());
}
#[test]
fn real_subprocess_timeout_overflow_exit_and_valid_capture_are_bounded_and_reaped() {
    for (script, expected) in [
        ("print('ok')", None),
        ("import time; time.sleep(20)", Some(Error::Busy)),
        ("print('x'*5000)", Some(Error::Invalid)),
        ("raise SystemExit(7)", Some(Error::Integrity)),
    ] {
        let mut command = Command::new("python3");
        command
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let start = Instant::now();
        let result = crate::admission::MaintenanceSnapshot::runtime_test_capture(
            command,
            Duration::from_millis(150),
        );
        if let Some(error) = expected {
            assert_eq!(result.err(), Some(error));
        } else {
            assert_eq!(result.unwrap(), b"ok\n");
        }
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[test]
fn worker_off_on_unknown_residual_old_generation_and_duplicate_mainpid_matrix() {
    use std::collections::BTreeSet;
    for state in [
        "off",
        "on",
        "unknown",
        "residual",
        "old",
        "duplicate",
        "foreign-member",
    ] {
        let f = Fixture::new(2);
        let ledger = f.ledger().unwrap();
        let slot = f.slot(&ledger).unwrap();
        let mut units = (0..4)
            .map(|i| {
                units::parse(
                    i,
                    &unit_bytes(
                        i,
                        if i == 3 && state != "on" && state != "old" {
                            0
                        } else {
                            i as u32 + 100
                        },
                    ),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        if state == "unknown" {
            let bytes = String::from_utf8(unit_bytes(3, 0))
                .unwrap()
                .replace("inactive", "failed")
                .replace("SubState=dead", "SubState=failed");
            units[3] = units::parse(3, bytes.as_bytes()).unwrap();
        }
        if state == "duplicate" {
            units[1] = units::parse(1, &unit_bytes(1, 100)).unwrap();
        }
        let before = f.inventory();
        let result = linux::collect_using(
            &units,
            &slot,
            |u| {
                if u.index == 3 && state == "residual" || u.index == 0 && state == "foreign-member"
                {
                    return Ok(BTreeSet::from([999]));
                }
                Ok(if u.active {
                    BTreeSet::from([u.pid])
                } else {
                    BTreeSet::new()
                })
            },
            |index, pid, _, path, _, _| Ok(Process::synthetic(index, pid, path)),
            || {
                Ok(match state {
                    "old" => BTreeSet::from([999]),
                    "on" => BTreeSet::from([103]),
                    _ => BTreeSet::new(),
                })
            },
        );
        match state {
            "off" => assert_eq!(result.unwrap().1, WorkerState::Off),
            "on" => assert_eq!(result.unwrap().1, WorkerState::On),
            "unknown" => assert_eq!(result.unwrap().1, WorkerState::Unknown),
            _ => assert!(result.is_err(), "{state}"),
        }
        assert_eq!(before, f.inventory());
    }
    // A worker running against a format1 package cannot have a signed executable.
    let f = Fixture::new(1);
    let ledger = f.ledger().unwrap();
    let slot = f.slot(&ledger).unwrap();
    let units = (0..4)
        .map(|i| units::parse(i, &unit_bytes(i, i as u32 + 100)).unwrap())
        .collect::<Vec<_>>();
    assert!(linux::collect_using(
        &units,
        &slot,
        |u| Ok(BTreeSet::from([u.pid])),
        |i, p, _, path, _, _| Ok(Process::synthetic(i, p, path)),
        || Ok(BTreeSet::from([103]))
    )
    .is_err());
}

#[test]
fn real_exec_replaces_executable_and_old_binding_is_rejected() {
    let d = tempfile::tempdir().unwrap();
    let trigger = d.path().join("exec");
    let script = "import ctypes,os,sys,time; assert ctypes.CDLL(None).prctl(38,1,0,0,0)==0; print('ready',flush=True)\nwhile not os.path.exists(sys.argv[1]): time.sleep(.005)\nos.execv('/bin/sleep',['sleep','20'])";
    let mut cmd = Command::new(if Path::new("/usr/bin/python3").exists() {
        "/usr/bin/python3"
    } else {
        "python3"
    });
    cmd.args(["-c", script])
        .arg(&trigger)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = Reap(cmd.spawn().unwrap());
    let mut ready = String::new();
    BufReader::new(child.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready, "ready\n");
    let pid = child.0.id();
    let old = fs::read_link(format!("/proc/{pid}/exe")).unwrap();
    let before = crate::health::probe_process(pid, ROLES[0].1).unwrap();
    fs::write(&trigger, b"exec").unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let now = fs::read_link(format!("/proc/{pid}/exe")).unwrap();
        if now != old {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let after = crate::health::probe_process(pid, ROLES[0].1).unwrap();
    assert_eq!(before.start_ticks, after.start_ticks);
    assert_ne!(before.exe_sha256, after.exe_sha256);
    assert!(linux::probe(
        0,
        pid,
        rustix::process::geteuid().as_raw(),
        &old,
        &before.exe_sha256,
        "/"
    )
    .is_err());
}

#[test]
fn verified_executable_retains_original_fd_and_rejects_equal_byte_replacement() {
    let f = Fixture::new(2);
    let ledger = f.ledger().unwrap();
    let slot = f.slot(&ledger).unwrap();
    let path = slot.executable(ROLES[0].1).unwrap().0;
    let original = fs::File::open(&path).unwrap();
    let binding = slot.bound_executable(ROLES[0].1).unwrap();
    binding.check(&original).unwrap();
    let replacement = path.with_extension("replacement");
    fs::copy(&path, &replacement).unwrap();
    fs::rename(&replacement, &path).unwrap();
    // A retained fd alone still points to old equal bytes; its path generation
    // must also remain the original one. Neither fd is accepted after replace.
    assert!(binding.check(&original).is_err());
    assert!(binding.check(&fs::File::open(&path).unwrap()).is_err());
    assert!(slot.bound_executable(ROLES[0].1).is_err());
    assert!(slot.revalidate(&ledger).is_err());
}

#[test]
fn verified_executable_rejects_mutation_with_restored_mtime_and_mode() {
    for mutation in ["bytes", "mode", "link"] {
        let f = Fixture::new(2);
        let ledger = f.ledger().unwrap();
        let slot = f.slot(&ledger).unwrap();
        let path = slot.executable(ROLES[0].1).unwrap().0;
        let original = fs::File::open(&path).unwrap();
        let before = original.metadata().unwrap();
        let binding = slot.bound_executable(ROLES[0].1).unwrap();
        binding.check(&original).unwrap();
        // Ensure a distinct kernel ctime even on coarser filesystems.
        std::thread::sleep(Duration::from_millis(2));
        match mutation {
            "bytes" => {
                let mut bytes = fs::read(&path).unwrap();
                *bytes.last_mut().unwrap() ^= 1;
                fs::write(&path, bytes).unwrap();
                original
                    .set_times(std::fs::FileTimes::new().set_modified(before.modified().unwrap()))
                    .unwrap();
                assert_eq!(
                    original.metadata().unwrap().mtime_nsec(),
                    before.mtime_nsec()
                );
            }
            "mode" => {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
                fs::set_permissions(&path, before.permissions()).unwrap();
            }
            "link" => fs::hard_link(&path, path.with_extension("alias")).unwrap(),
            _ => unreachable!(),
        }
        assert!(binding.check(&original).is_err(), "{mutation}");
        assert!(slot.revalidate(&ledger).is_err(), "{mutation}");
    }
}

#[test]
fn verified_executable_never_accepts_another_process_image_or_unknown_role() {
    let f = Fixture::new(2);
    let ledger = f.ledger().unwrap();
    let slot = f.slot(&ledger).unwrap();
    let binding = slot.bound_executable(ROLES[0].1).unwrap();
    let actual = fs::File::open("/proc/self/exe").unwrap();
    assert!(binding.check(&actual).is_err());
    assert!(slot.bound_executable("not-a-signed-executable").is_err());
    assert!(slot.bound_executable("Opus-COPYING.txt").is_err());
}

#[test]
#[ignore = "private disposable ARM guest readonly performance diagnostic; never confirms"]
fn profile_original_runtime_sources() {
    assert_eq!(
        std::env::var("RSH_WEB4_PRIVATE_CONTAINER").as_deref(),
        Ok("yes")
    );
    assert!(Path::new("/fixture/FORMAL_ARM_GUEST").is_file());
    fn timed<T>(name: &str, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let start = Instant::now();
        let result = work();
        eprintln!(
            "PRIVATE READONLY STAGE {name}: {:?} error={:?}",
            start.elapsed(),
            result.as_ref().err()
        );
        result
    }
    let ledger = timed("ledger", ReadOnlyState::observe).unwrap();
    let slot = timed("signed slot", || Slot::observe(&ledger)).unwrap();
    let units = timed("units", units::observe).unwrap();
    timed("processes first", || linux::collect(&units, &slot)).unwrap();
    timed("processes recheck", || linux::collect(&units, &slot)).unwrap();
    drop(slot);
    drop(ledger);
    let _ = timed(
        "original full runtime observation",
        RuntimeObservation::observe,
    );
}

#[test]
fn private_collection_keeps_original_deadline_and_cannot_validate_a_changed_runtime() {
    for expired in [false, true] {
        let f = Fixture::new(2);
        let port = Rc::new(Port::default());
        let ledger = f.ledger().unwrap();
        let slot = f.slot(&ledger).unwrap();
        let started = if expired {
            Instant::now() - MAX_AGE
        } else {
            Instant::now()
        };
        let sample =
            RuntimeObservation::sample(ledger, slot, BOOT.into(), started, Box::new(port.clone()))
                .unwrap();
        if !expired {
            port.fault.set(2);
        }
        assert_eq!(
            sample.revalidate(),
            Err(if expired {
                Error::Busy
            } else {
                Error::Integrity
            })
        );
        port.fault.set(0);
        // Failed observations cannot be repaired by another sample or deadline.
        assert_eq!(sample.revalidate(), Err(Error::Busy));
        assert!(sample.report().is_err());
    }
}
