// SPDX-License-Identifier: AGPL-3.0-only
//! Exercise actual ledger helpers without touching fixed production paths.
use super::*;

#[test]
fn fifo_reader_child() {
    let Ok(path) = std::env::var("RSH_CORE_LED_TEST_FIFO") else {
        return;
    };
    assert!(checked_read(Path::new(&path)).is_err());
}
#[test]
fn fifo_is_rejected_before_core_read_can_block_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-fifo");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::from_bits_truncate(0o600),
    )
    .unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "status_led::input_boundary_tests::fifo_reader_child",
            "--nocapture",
        ])
        .env("RSH_CORE_LED_TEST_FIFO", &path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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
            panic!("FIFO open blocked before Core ledger metadata validation");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn read_ledger_rejects_malformed_fields_paths_and_capacity_without_marking_notifications() {
    for bytes in [
        &b"\xff"[..],
        &b"{}"[..],
        &br#"{"sms":[],"calls":[],"connected":[],"x":1}"#[..],
        &br#"{"sms":"not-a-set","calls":[],"connected":[]}"#[..],
    ] {
        assert!(serde_json::from_slice::<Ledger>(bytes).is_err());
    }
    let mut ledger = Ledger::default();
    ledger.sms.insert("/etc/passwd".into());
    assert!(validate(&ledger).is_err());
    ledger.sms.clear();
    ledger.calls = (0..32768)
        .map(|n| format!("00000000-0000-4000-8000-{n:012x}"))
        .collect();
    assert!(validate(&ledger).is_ok());
    ledger
        .calls
        .insert("11111111-1111-4111-8111-111111111111".into());
    assert!(validate(&ledger).is_err());
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("synthetic-ledger");
    fs::write(&p, b"{}").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(checked_read(&p).unwrap(), b"{}");
    std::fs::File::options()
        .write(true)
        .open(&p)
        .unwrap()
        .set_len(4_194_305)
        .unwrap();
    assert!(checked_read(&p).is_err());
}
