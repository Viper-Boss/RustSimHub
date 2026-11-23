// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::PermissionsExt;
fn output() -> Vec<u8> {
    b"LoadState=loaded\nFragmentPath=/etc/systemd/system/rustsimhub-core.service\nDropInPaths=/etc/systemd/system/rustsimhub-core.service.d/30-forwarding.conf /etc/systemd/system/rustsimhub-core.service.d/55-update-health.conf\nNeedDaemonReload=no\n".to_vec()
}
fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for spec in materials() {
        let p = temp.path().join(spec.path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, spec.expected.unwrap()).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(spec.mode)).unwrap();
    }
    let scratch = temp.path().join(SCRATCH.trim_start_matches('/'));
    fs::create_dir_all(&scratch).unwrap();
    fs::set_permissions(scratch, fs::Permissions::from_mode(0o700)).unwrap();
    temp
}
#[test]
fn exact_opt_in_root_profile_rechecks_bytes_loaded_contract_and_scratch_inode() {
    let temp = fixture();
    let uid = rustix::process::geteuid().as_raw();
    let root = Root::at(temp.path(), uid, Box::new(|| Ok(output()))).unwrap();
    root.revalidate().unwrap();
    fs::write(root.scratch.join("confirmation.next"), b"preparation").unwrap();
    root.revalidate().unwrap();
    fs::write(temp.path().join(DROP), b"different Core command").unwrap();
    assert!(root.revalidate().is_err());
}
#[test]
fn ap_or_unknown_override_pending_reload_and_wrong_root_ownership_are_rejected() {
    let temp = fixture();
    let uid = rustix::process::geteuid().as_raw();
    fs::write(
        temp.path()
            .join("etc/systemd/system/rustsimhub-core.service.d/50-setup-ap.conf"),
        b"ap",
    )
    .unwrap();
    assert!(Root::at(temp.path(), uid, Box::new(|| Ok(output()))).is_err());
    for bad in [
        String::from_utf8(output())
            .unwrap()
            .replace("NeedDaemonReload=no", "NeedDaemonReload=yes"),
        String::from_utf8(output())
            .unwrap()
            .replace("55-update-health.conf", "foreign.conf"),
        String::from_utf8(output()).unwrap() + "LoadState=loaded\n",
    ] {
        assert!(effective(bad.as_bytes()).is_err());
    }
}
#[test]
fn replaced_or_world_writable_scratch_is_refused() {
    let temp = fixture();
    let uid = rustix::process::geteuid().as_raw();
    let root = Root::at(temp.path(), uid, Box::new(|| Ok(output()))).unwrap();
    fs::set_permissions(root.scratch(), fs::Permissions::from_mode(0o777)).unwrap();
    assert!(root.revalidate().is_err());
}

#[test]
fn web_ap_health_requires_exact_root_bytes_and_loaded_config() {
    let temp = super::super::tests::fixture(Mode::SetupAp, true);
    let uid = rustix::process::geteuid().as_raw();
    for spec in web_health_materials() {
        let path = temp.path().join(spec.path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, spec.expected.unwrap()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(spec.mode)).unwrap();
    }
    let scratch = temp.path().join(SCRATCH.trim_start_matches('/'));
    fs::create_dir_all(&scratch).unwrap();
    fs::set_permissions(scratch, fs::Permissions::from_mode(0o700)).unwrap();
    let effective = || {
        Ok(String::from_utf8(output()).unwrap()
            .replace("55-update-health.conf", "50-setup-ap.conf /etc/systemd/system/rustsimhub-core.service.d/56-web-update-health.conf")
            .into_bytes())
    };
    let snapshot = super::super::tests::snapshot(temp.path());
    assert!(snapshot.web_health_ap);
    drop(snapshot);
    let root = Root::at_profile(temp.path(), uid, Box::new(effective), None, true).unwrap();
    root.revalidate().unwrap();
    assert!(Root::at_profile(temp.path(), uid, Box::new(|| Ok(output())), None, true).is_err());
    fs::write(temp.path().join(WEB_HEALTH_DROP), b"changed policy").unwrap();
    assert!(root.revalidate().is_err());
    assert!(Root::at_profile(temp.path(), uid, Box::new(effective), None, true).is_err());
}
