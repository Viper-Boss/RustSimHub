// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
#[test]
fn strict_snapshot_and_actual_schema_nine_never_gain_schema_six_admission() {
    let snapshot = wire::Snapshot {
        protocol: 1,
        challenge: "a".repeat(64),
        connection: "b".repeat(64),
        core_instance: "c".repeat(64),
        source_sha256: Some("d".repeat(64)),
        business_ready: true,
        data_schema: 6,
        data_realm: "12345678-abcd-1234-abcd-123456789abc".into(),
        line_generation: 1,
        data_device: 0,
        data_inode: 1,
        authorization_generation: 1,
        authorization_binding: "e".repeat(64),
        forwarding: wire::Forwarding {
            policy: wire::Policy::Off,
            generation: 0,
            outbox_schema: None,
            worker_admitted: false,
        },
    };
    let raw = serde_json::to_vec(&snapshot).unwrap();
    assert_eq!(wire::parse_snapshot(&raw).unwrap(), snapshot);
    let duplicate = String::from_utf8(raw)
        .unwrap()
        .replace("\"protocol\":1", "\"protocol\":1,\"protocol\":1");
    assert!(wire::parse_snapshot(duplicate.as_bytes()).is_err());
    let mut bad = serde_json::to_value(&snapshot).unwrap();
    bad["may_confirm"] = true.into();
    assert!(wire::parse_snapshot(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut receipt: crate::admission::WebHealthReceipt =
        serde_json::from_value(serde_json::json!({
            "format": 1,
            "request_id": "12345678-1234-4234-8234-123456789abc",
            "manifest_sha256": "a".repeat(64),
            "baseline_sha256": "b".repeat(64),
            "epoch": 115,
            "business_schema": 6,
            "authorization_binding": snapshot.authorization_binding,
            "business_sha256": "c".repeat(64),
            "outbox_sha256": null,
        }))
        .unwrap();
    assert_eq!(web_matches_health(&receipt, &snapshot), Ok(()));
    receipt.authorization_binding = Some("4".repeat(64));
    assert_eq!(web_matches_health(&receipt, &snapshot), Err(Error::Denied));
    receipt.authorization_binding = None;
    assert_eq!(web_matches_health(&receipt, &snapshot), Err(Error::Denied));
    let mut nine = snapshot;
    nine.data_schema = 9;
    assert!(wire::parse_snapshot(&serde_json::to_vec(&nine).unwrap()).is_ok());
    assert_eq!(running::Data::observe(&nine).err(), Some(Error::Schema));
}
#[test]
fn root_client_frame_read_has_one_budget_and_rejects_eof_or_oversize() {
    use std::io::Write;
    let (mut root, mut peer) = UnixStream::pair().unwrap();
    peer.write_all(&((wire::MAX_FRAME + 1) as u32).to_be_bytes())
        .unwrap();
    assert!(io::read(&mut root, Instant::now() + MAX_AGE, false).is_err());
    let (mut root, peer) = UnixStream::pair().unwrap();
    drop(peer);
    assert!(io::read(&mut root, Instant::now() + MAX_AGE, false).is_err());
}
mod live;
