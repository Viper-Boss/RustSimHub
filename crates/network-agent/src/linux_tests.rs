// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::os::unix::fs::symlink;

#[test]
fn counter_source_accepts_exact_u64_values_and_rejects_missing_or_malformed_input() {
    let source = json!({"nftables":[
        {"metainfo":{"version":"synthetic"}},
        {"counter":{"family":"inet","table":"rustsimhub_net","name":"rx","bytes":u64::MAX}},
        {"counter":{"family":"inet","table":"rustsimhub_net","name":"tx","bytes":0}}
    ]});
    assert_eq!(parse_counters(&source.to_string()).unwrap(), (u64::MAX, 0));
    for input in ["", "{", "null", "{}", r#"{"nftables":[]}"#] {
        assert!(parse_counters(input).is_err());
    }
    for direction in [1, 2] {
        for bad in [
            Value::Null,
            json!(-1),
            json!(1.5),
            json!("1"),
            json!(18446744073709551616u128.to_string()),
        ] {
            let mut input = source.clone();
            input["nftables"][direction]["counter"]["bytes"] = bad;
            assert!(parse_counters(&input.to_string()).is_err());
        }
        let mut missing = source.clone();
        missing["nftables"]
            .as_array_mut()
            .unwrap()
            .remove(direction);
        assert!(parse_counters(&missing.to_string()).is_err());
    }
    for (field, wrong) in [("family", "ip"), ("table", "foreign"), ("name", "other")] {
        let mut input = source.clone();
        input["nftables"][1]["counter"][field] = json!(wrong);
        assert!(parse_counters(&input.to_string()).is_err());
    }
}

fn read_state(path: &Path) -> Result<State> {
    let state: State =
        serde_json::from_slice(&checked(path, rustix::process::geteuid().as_raw(), 0o600)?)?;
    if !state.valid() {
        return Err("invalid accounting".into());
    }
    Ok(state)
}

#[test]
fn private_ledger_publish_reopen_and_replace_preserve_actual_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meter.json");
    let mut state = State::new("a".repeat(64));
    state
        .sample("boot", &"b".repeat(64), 1793203200, 10, 20)
        .unwrap();
    publish(dir.path().to_str().unwrap(), "meter.json", &state, 0o600).unwrap();
    let mut reopened = read_state(&path).unwrap();
    assert_eq!(
        serde_json::to_value(&reopened).unwrap(),
        serde_json::to_value(&state).unwrap()
    );
    reopened
        .sample("boot", &"b".repeat(64), 1793203200, 11, 22)
        .unwrap();
    publish(dir.path().to_str().unwrap(), "meter.json", &reopened, 0o600).unwrap();
    assert_eq!(
        read_state(&path)
            .unwrap()
            .usage(1793203200)
            .unwrap()
            .total_bytes,
        33
    );
    assert!(!dir.path().join(".meter.json.pending").exists());
}

#[test]
fn corrupt_version_truncation_and_oversize_never_replace_ledger_with_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meter.json");
    let mut invalid = State::new("a".repeat(64));
    invalid.schema = 2;
    for bytes in [
        b"{\"schema\":".to_vec(),
        serde_json::to_vec(&invalid).unwrap(),
        vec![b' '; LIMIT + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(read_state(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn checked_rejects_wrong_mode_owner_links_and_nonregular_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("meter.json");
    let state = State::new("a".repeat(64));
    publish(dir.path().to_str().unwrap(), "meter.json", &state, 0o600).unwrap();
    let uid = rustix::process::geteuid().as_raw();
    assert!(checked(&path, uid + 1, 0o600).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(checked(&path, uid, 0o600).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let alias = dir.path().join("alias");
    symlink(&path, &alias).unwrap();
    assert!(checked(&alias, uid, 0o600).is_err());
    fs::remove_file(&alias).unwrap();
    fs::hard_link(&path, &alias).unwrap();
    assert!(checked(&path, uid, 0o600).is_err());
    assert!(checked(&alias, uid, 0o600).is_err());
    assert!(checked(dir.path(), uid, 0o700).is_err());
}

#[test]
fn failed_publish_retains_old_ledger_and_unexpected_pending_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().to_str().unwrap();
    let path = dir.path().join("meter.json");
    let pending = dir.path().join(".meter.json.pending");
    let state = State::new("a".repeat(64));
    publish(base, "meter.json", &state, 0o600).unwrap();
    let original = fs::read(&path).unwrap();
    fs::write(&pending, b"interrupted checkpoint").unwrap();
    assert!(publish(base, "meter.json", &state, 0o600).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(&pending).unwrap(), b"interrupted checkpoint");
    fs::remove_file(&pending).unwrap();
    symlink(&path, &pending).unwrap();
    assert!(publish(base, "meter.json", &state, 0o600).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::remove_file(&pending).unwrap();
    assert!(publish(base, "meter.json", &"x".repeat(LIMIT), 0o600).is_err());
    assert!(!pending.exists());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn failed_atomic_rename_keeps_destination_and_new_pending_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("meter.json");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("evidence"), b"retained").unwrap();
    let state = State::new("a".repeat(64));
    assert!(publish(dir.path().to_str().unwrap(), "meter.json", &state, 0o600).is_err());
    assert_eq!(fs::read(destination.join("evidence")).unwrap(), b"retained");
    read_state(&dir.path().join(".meter.json.pending")).unwrap();
}
