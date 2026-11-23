// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(target_os = "linux")]
#![allow(dead_code)]
// Compile the exact proposed modules without touching the shared lib/CLI.
#[allow(unused_imports)]
#[path = "../src/product_materials/mod.rs"]
mod contract;
#[path = "product_materials_support/timestamps.rs"]
mod timestamps;
use contract::Error;
use contract::{nss, rootio};
use rootio::Root;
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};

struct Fixture {
    dir: tempfile::TempDir,
    root: Root,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let root = Root::isolated(dir.path()).unwrap();
        Self { dir, root }
    }
    fn actual(&self, path: &str) -> PathBuf {
        self.dir.path().join(path.trim_start_matches('/'))
    }
    fn put(&self, path: &str, bytes: &[u8], mode: u32) {
        let parent = path.rsplit_once('/').unwrap().0;
        self.root
            .ensure_dir(if parent.is_empty() { "/" } else { parent }, 0o755)
            .unwrap();
        let old = self.root.read(path, 65536).unwrap();
        self.root.replace(path, old.as_ref(), bytes, mode).unwrap();
    }
    fn nss(&self) {
        self.put("/etc/passwd",b"root:x:0:0:root:/root:/bin/sh\nrustsimhub-core:x:986:986::/nonexistent:/usr/sbin/nologin\nrustsimhub-edge:x:985:985::/nonexistent:/usr/sbin/nologin\nrustsimhub-media:x:984:984::/nonexistent:/usr/sbin/nologin\nrustsimhub-forwarder:x:983:983::/nonexistent:/usr/sbin/nologin\n",0o644);
        self.put("/etc/group",b"root:x:0:\nrustsimhub-core:x:986:\nrustsimhub-edge:x:985:\nrustsimhub-media:x:984:\nrustsimhub-forwarder:x:983:\naudio:x:29:rustsimhub-media\nrustsimhub-ipc:x:987:rustsimhub-core,rustsimhub-edge,rustsimhub-media,rustsimhub-forwarder\n",0o644);
        self.put(
            "/etc/nsswitch.conf",
            b"passwd: files systemd\ngroup: files systemd\nhosts: files dns\n",
            0o644,
        );
        self.put(
            "/etc/rustsimhub/product-roles.json",
            br#"{"format":1,"roles":{"ble":{"uid":920,"gid":920},"nkn":{"uid":921,"gid":921}}}"#,
            0o600,
        );
    }
}
#[test]
fn actual_create_replace_empty_restore_remove_and_durable_intent_bind_same_inode() {
    let f = Fixture::new();
    f.root.ensure_dir("/etc/product", 0o755).unwrap();
    assert!(f.root.read("/missing/entire/parent", 16).unwrap().is_none());
    let intent = RefCell::new(None);
    let a = f
        .root
        .replace_with_intent("/etc/product/data", None, b"new", 0o644, |s| {
            *intent.borrow_mut() = Some(s.clone());
            Ok(())
        })
        .unwrap();
    let staged = intent.borrow().clone().unwrap();
    assert_eq!(staged.inode, a.stamp.inode);
    assert_eq!(staged.device, a.stamp.device);
    assert_eq!(fs::read(f.actual("/etc/product/data")).unwrap(), b"new");
    assert_eq!(a.stamp.mode, 0o644);
    assert_eq!(a.stamp.uid, rustix::process::geteuid().as_raw());
    a.recheck(&f.root, "/etc/product/data").unwrap();
    let empty = f
        .root
        .replace("/etc/product/data", Some(&a), b"", 0o600)
        .unwrap();
    assert!(empty.bytes.is_empty());
    assert_eq!(
        f.root.recheck("/etc/product/data", Some(&a)),
        Err(Error::Conflict)
    );
    let restored = f
        .root
        .replace("/etc/product/data", Some(&empty), &a.bytes, a.stamp.mode)
        .unwrap();
    f.root.remove("/etc/product/data", &restored).unwrap();
    assert!(f.root.read("/etc/product/data", 16).unwrap().is_none());
    assert!(fs::read_dir(f.actual("/etc/product"))
        .unwrap()
        .next()
        .is_none());
}
#[test]
fn intent_failure_and_racing_later_writer_leave_target_and_no_owned_stage() {
    let f = Fixture::new();
    f.root.ensure_dir("/etc/product", 0o755).unwrap();
    assert_eq!(
        f.root
            .replace_with_intent("/etc/product/data", None, b"new", 0o644, |_| Err(
                Error::Unsupported
            ))
            .err(),
        Some(Error::Unsupported)
    );
    assert!(fs::read_dir(f.actual("/etc/product"))
        .unwrap()
        .next()
        .is_none());
    let actual = f.actual("/etc/product/data");
    assert_eq!(
        f.root
            .replace_with_intent("/etc/product/data", None, b"new", 0o644, |_| {
                fs::write(&actual, b"later").unwrap();
                Ok(())
            })
            .err(),
        Some(Error::Conflict)
    );
    assert_eq!(fs::read(&actual).unwrap(), b"later");
    assert_eq!(fs::read_dir(f.actual("/etc/product")).unwrap().count(), 1);
    let before = f.root.read("/etc/product/data", 16).unwrap().unwrap();
    assert_eq!(
        f.root
            .replace_with_intent("/etc/product/data", Some(&before), b"new", 0o644, |_| {
                let moved = actual.with_extension("replacement");
                fs::write(&moved, b"later replacement").unwrap();
                fs::rename(moved, &actual).unwrap();
                Ok(())
            })
            .err(),
        Some(Error::Conflict)
    );
    assert_eq!(fs::read(&actual).unwrap(), b"later replacement");
    assert_eq!(
        f.root.remove("/etc/product/data", &before),
        Err(Error::Conflict)
    );
}
#[test]
fn links_special_files_permissions_and_traversal_are_rejected_without_following() {
    let f = Fixture::new();
    f.root.ensure_dir("/etc/product", 0o755).unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("data"), b"outside").unwrap();
    symlink(outside.path(), f.actual("/etc/link")).unwrap();
    assert!(f.root.read("/etc/link/data", 64).is_err());
    assert!(f.root.ensure_dir("/etc/link/more", 0o755).is_err());
    assert!(f.root.read("/etc/../data", 64).is_err());
    assert!(f.root.read("relative", 64).is_err());
    assert!(f.root.read("//etc/data", 64).is_err());
    symlink(outside.path().join("data"), f.actual("/etc/product/link")).unwrap();
    assert!(f.root.read("/etc/product/link", 64).is_err());
    f.put("/etc/product/hard", b"hard", 0o644);
    fs::hard_link(
        f.actual("/etc/product/hard"),
        f.actual("/etc/product/alias"),
    )
    .unwrap();
    assert!(f.root.read("/etc/product/hard", 64).is_err());
    rustix::fs::mknodat(
        rustix::fs::CWD,
        f.actual("/etc/product/fifo"),
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR,
        0,
    )
    .unwrap();
    assert!(f.root.read("/etc/product/fifo", 64).is_err());
    f.put("/etc/product/writable", b"bad", 0o644);
    fs::set_permissions(
        f.actual("/etc/product/writable"),
        fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    assert!(f.root.read("/etc/product/writable", 64).is_err());
    assert_eq!(fs::read(outside.path().join("data")).unwrap(), b"outside");
    assert!(Root::isolated(Path::new("/")).is_err());
}
#[test]
fn fixed_lock_is_real_exclusive_and_released_and_unsafe_alias_refused() {
    let f = Fixture::new();
    f.root.ensure_dir("/journal", 0o700).unwrap();
    let first = f.root.lock_fixed("/journal/.install.lock").unwrap();
    assert_eq!(
        f.root.lock_fixed("/journal/.install.lock").err(),
        Some(Error::Busy)
    );
    drop(first);
    let again = f.root.lock_fixed("/journal/.install.lock").unwrap();
    drop(again);
    fs::hard_link(
        f.actual("/journal/.install.lock"),
        f.actual("/journal/alias"),
    )
    .unwrap();
    assert!(f.root.lock_fixed("/journal/.install.lock").is_err());
}
#[test]
fn approved_policy_creates_only_finite_private_root_accounts_and_is_idempotent() {
    let f = Fixture::new();
    f.nss();
    assert!(nss::NssObservation::observe_isolated(&f.root).is_err());
    let base = nss::NssObservation::observe_isolated_for(&f.root, false).unwrap();
    assert_eq!(base.ble_uid(), 920);
    assert_eq!(base.nkn_uid(), 921);
    assert!(
        nss::NssObservation::observe(&f.root).is_err(),
        "isolated adapter never executes host getent"
    );
    nss::prepare_accounts(&f.root).unwrap();
    let observed = nss::NssObservation::observe_isolated(&f.root).unwrap();
    let pwd = f.root.read("/etc/passwd", 65536).unwrap().unwrap();
    let grp = f.root.read("/etc/group", 65536).unwrap().unwrap();
    assert!(String::from_utf8(pwd.bytes.clone())
        .unwrap()
        .contains("rustsimhub-ble:x:920:920:"));
    nss::prepare_accounts(&f.root).unwrap();
    pwd.recheck(&f.root, "/etc/passwd").unwrap();
    grp.recheck(&f.root, "/etc/group").unwrap();
    assert_eq!(observed.policy_sha256().len(), 64);
    assert_eq!(observed.baseline_sha256().len(), 64);
    observed.revalidate(&f.root).unwrap();
    f.put(
        "/etc/rustsimhub/product-roles.json",
        br#"{"format":1,"roles":{"ble":{"uid":922,"gid":922},"nkn":{"uid":923,"gid":923}}}"#,
        0o600,
    );
    assert!(observed.revalidate(&f.root).is_err());
}
#[test]
fn nss_alias_supplementary_privilege_wrong_policy_and_remote_sources_refuse() {
    for case in [
        "uid-alias",
        "group-alias",
        "supplementary",
        "nss-alias",
        "remote",
        "policy-base",
        "policy-duplicate",
        "policy-key",
        "policy-mode",
    ] {
        let f = Fixture::new();
        f.nss();
        nss::prepare_accounts(&f.root).unwrap();
        match case {
            "uid-alias"=> {let old=f.root.read("/etc/passwd",65536).unwrap().unwrap();let mut data=old.bytes.clone();data.extend_from_slice(b"alias:x:920:100::/nonexistent:/bin/false\n");f.put("/etc/passwd",&data,0o644);},
            "group-alias"=> {let old=f.root.read("/etc/group",65536).unwrap().unwrap();let mut data=old.bytes.clone();data.extend_from_slice(b"alias:x:920:\n");f.put("/etc/group",&data,0o644);},
            "supplementary"=> {let old=f.root.read("/etc/group",65536).unwrap().unwrap();let mut data=old.bytes.clone();data.extend_from_slice(b"privileged:x:88:rustsimhub-ble\n");f.put("/etc/group",&data,0o644);},
            "nss-alias"=> {let old=f.root.read("/etc/passwd",65536).unwrap().unwrap();let mut data=old.bytes.clone();data.extend_from_slice(b"external:x:921:100::/nonexistent:/bin/false\n");f.put("/run/product-install-test/nss-passwd",&data,0o644);},
            "remote"=> f.put("/etc/nsswitch.conf",b"passwd: files ldap\ngroup: files\n",0o644),
            "policy-base"=> f.put("/etc/rustsimhub/product-roles.json",br#"{"format":1,"roles":{"ble":{"uid":986,"gid":986},"nkn":{"uid":921,"gid":921}}}"#,0o600),
            "policy-duplicate"=> f.put("/etc/rustsimhub/product-roles.json",br#"{"format":1,"format":1,"roles":{"ble":{"uid":920,"gid":920},"nkn":{"uid":921,"gid":921}}}"#,0o600),
            "policy-key"=> f.put("/etc/rustsimhub/product-roles.json",br#"{"format":1,"healthy":true,"roles":{"ble":{"uid":920,"gid":920},"nkn":{"uid":921,"gid":921}}}"#,0o600),
            _=> {fs::set_permissions(f.actual("/etc/rustsimhub/product-roles.json"),fs::Permissions::from_mode(0o644)).unwrap();},
        }
        assert!(
            nss::NssObservation::observe_isolated(&f.root).is_err(),
            "{case}"
        );
    }
}
#[test]
fn account_conflict_never_allocates_another_uid_or_partially_changes_files() {
    let f = Fixture::new();
    f.nss();
    let old = f.root.read("/etc/passwd", 65536).unwrap().unwrap();
    let mut bytes = old.bytes.clone();
    bytes.extend_from_slice(b"existing:x:920:100::/nonexistent:/bin/false\n");
    f.put("/etc/passwd", &bytes, 0o644);
    let before_passwd = fs::read(f.actual("/etc/passwd")).unwrap();
    let before_group = fs::read(f.actual("/etc/group")).unwrap();
    assert!(nss::prepare_accounts(&f.root).is_err());
    assert_eq!(fs::read(f.actual("/etc/passwd")).unwrap(), before_passwd);
    assert_eq!(fs::read(f.actual("/etc/group")).unwrap(), before_group);
}

fn private_mount(variable: &str, readonly: bool) -> PathBuf {
    let path =
        PathBuf::from(std::env::var_os(variable).expect("explicit private fixture env required"));
    assert!(path.is_absolute() && path != Path::new("/"));
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").unwrap();
    let parts = mountinfo
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .find(|p| p[4] == path.to_str().unwrap())
        .expect("dedicated private mount required");
    assert_eq!(
        parts[parts.iter().position(|v| *v == "-").unwrap() + 1],
        "tmpfs"
    );
    assert_eq!(parts[5].split(',').any(|v| v == "ro"), readonly);
    path
}
#[test]
#[ignore = "requires explicit dedicated private <=4MiB tmpfs, never fills an ordinary filesystem"]
fn dedicated_private_tmpfs_enospc_cleans_own_stage_without_target_write() {
    let path = private_mount("PRODUCT_INSTALL_ENOSPC_ROOT", false);
    let root = Root::isolated(&path).unwrap();
    let dir = root.open_dir("/").unwrap();
    let stat = rustix::fs::fstatvfs(&dir).unwrap();
    assert!(u128::from(stat.f_blocks) * u128::from(stat.f_frsize) <= 4 * 1024 * 1024);
    assert!(root
        .replace("/target", None, &vec![9; 8 * 1024 * 1024], 0o600)
        .is_err());
    assert!(root.read("/target", 16).unwrap().is_none());
    assert!(fs::read_dir(&path).unwrap().next().is_none());
}
#[test]
#[ignore = "requires explicit dedicated private read-only tmpfs with baseline target"]
fn dedicated_readonly_tmpfs_rejects_write_and_retains_baseline() {
    let path = private_mount("PRODUCT_INSTALL_READONLY_ROOT", true);
    let root = Root::isolated(&path).unwrap();
    let before = root.read("/target", 65536).unwrap().unwrap();
    assert!(root
        .replace("/target", Some(&before), b"changed", 0o600)
        .is_err());
    before.recheck(&root, "/target").unwrap();
}

#[test]
fn nss_query_transport_is_bounded_env_clear_and_reaps_failed_children() {
    use std::{
        process::Command,
        time::{Duration, Instant},
    };
    let python = ["/usr/bin/python3", "/usr/local/bin/python3"]
        .into_iter()
        .find(|path| Path::new(path).exists())
        .expect("explicit absolute Python path for the isolated test child");
    for (script, error) in [
        (
            "import os;assert 'DBUS_SYSTEM_BUS_ADDRESS' not in os.environ;print('fixture')",
            None,
        ),
        ("import time;time.sleep(20)", Some(Error::Busy)),
        ("print('x'*70000)", Some(Error::Invalid)),
        ("raise SystemExit(9)", Some(Error::Io)),
    ] {
        let mut command = Command::new(python);
        command
            .args(["-c", script])
            .env("DBUS_SYSTEM_BUS_ADDRESS", "synthetic-invalid-target");
        let start = Instant::now();
        let result = nss::test_getent_capture(command);
        if let Some(error) = error {
            assert_eq!(result.err(), Some(error));
        } else {
            assert_eq!(result.unwrap(), b"fixture\n");
        }
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[test]
fn readonly_running_fd_guard_rejects_replacement_restored_metadata_and_parent_alias() {
    for mutation in ["replace", "content", "mode", "link", "parent"] {
        let f = Fixture::new();
        let path = "/usr/libexec/product/program";
        f.put(path, b"original executable bytes", 0o555);
        let snapshot = f.root.read(path, 65536).unwrap().unwrap();
        snapshot.recheck_unchanged(&f.root, path).unwrap();
        let actual = f.actual(path);
        let before = fs::metadata(&actual).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        match mutation {
            "replace" => {
                let replacement = actual.with_extension("replacement");
                fs::copy(&actual, &replacement).unwrap();
                fs::rename(&replacement, &actual).unwrap();
            }
            "content" => {
                fs::set_permissions(&actual, fs::Permissions::from_mode(0o755)).unwrap();
                fs::write(&actual, b"changed! executable bytes").unwrap();
                fs::File::open(&actual)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(before.modified().unwrap()))
                    .unwrap();
                fs::set_permissions(&actual, before.permissions()).unwrap();
            }
            "mode" => {
                timestamps::mutate_until_ctime_changes(&actual, || {
                    fs::set_permissions(&actual, fs::Permissions::from_mode(0o755)).unwrap();
                    fs::set_permissions(&actual, before.permissions()).unwrap();
                });
            }
            "link" => fs::hard_link(&actual, actual.with_extension("alias")).unwrap(),
            "parent" => {
                let parent = actual.parent().unwrap();
                let moved = parent.with_extension("old");
                fs::rename(parent, &moved).unwrap();
                symlink(&moved, parent).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            snapshot.recheck_unchanged(&f.root, path).is_err(),
            "{mutation}"
        );
        assert!(snapshot.recheck(&f.root, path).is_err(), "{mutation}");
    }
}
