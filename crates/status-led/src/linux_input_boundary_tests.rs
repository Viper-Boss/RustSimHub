// SPDX-License-Identifier: AGPL-3.0-only
//! Private reader tests: temporary files only, never the production run loop.
use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn fifo_reader_child() {
    let Ok(path) = std::env::var("RSH_LED_TEST_FIFO") else {
        return;
    };
    assert!(checked(
        Path::new(&path),
        fs::symlink_metadata(&path).unwrap().uid(),
        0o600,
        512
    )
    .is_err());
}

#[test]
fn fifo_is_rejected_before_open_can_block_the_indicator_loop() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-fifo");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::from_bits_truncate(0o600),
    )
    .unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "linux::input_boundary_tests::fifo_reader_child",
            "--nocapture",
        ])
        .env("RSH_LED_TEST_FIFO", &path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("FIFO open blocked before metadata validation");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn actual_file_reader_keeps_group_owner_link_size_and_parse_boundaries() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("state.json");
    let payload =
        br#"{"expires_ms":10000,"configuring":true,"unread_sms":false,"missed_calls":false}"#;
    fs::write(&path, payload).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let m = fs::metadata(&path).unwrap();
    let bytes = checked_owned(&path, m.uid(), Some(m.gid()), 0o640, 512).unwrap();
    assert!(serde_json::from_slice::<BusinessInput>(&bytes)
        .unwrap()
        .fresh(1000)
        .is_some());
    assert!(checked_owned(&path, m.uid(), Some(m.gid() + 1), 0o640, 512).is_err());
    assert!(checked_owned(&path, m.uid() + 1, Some(m.gid()), 0o640, 512).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(checked(&path, m.uid(), 0o640, 512).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    let alias = d.path().join("alias");
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    assert!(checked(&alias, m.uid(), 0o640, 512).is_err());
    let hard = d.path().join("hard");
    fs::hard_link(&path, &hard).unwrap();
    assert!(checked(&path, m.uid(), 0o640, 512).is_err());
    fs::remove_file(hard).unwrap();
    fs::write(&path, vec![b' '; 513]).unwrap();
    assert!(checked(&path, m.uid(), 0o640, 512).is_err());
    fs::write(&path, b"{truncated").unwrap();
    let bytes = checked(&path, m.uid(), 0o640, 512).unwrap();
    assert!(serde_json::from_slice::<BusinessInput>(&bytes).is_err());
    assert!(checked(d.path(), m.uid(), 0o700, 512).is_err());
}
