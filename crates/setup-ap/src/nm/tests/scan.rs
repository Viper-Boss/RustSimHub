// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use std::time::Duration;

#[tokio::test]
async fn actual_scan_waits_for_completion_and_reads_bounded_display_rows_once() {
    let model = Arc::new(Mutex::new(Model {
        scan_paths: vec!["/org/freedesktop/NetworkManager/AccessPoint/1"
            .try_into()
            .unwrap()],
        ..Default::default()
    }));
    let (_server, mut nm) = fixture(model.clone()).await;
    assert!(nm.scan_before_ap().await.is_err()); // no preflight, no bus request
    nm.preflight().await.unwrap();
    let report = nm.scan_before_ap().await.unwrap();
    assert!(report.valid());
    assert_eq!(report.networks.len(), 1);
    assert_eq!(report.networks[0].ssid, "Synthetic nearby Wi-Fi");
    assert_eq!(report.networks[0].strength, 81);
    assert!(nm.scan_before_ap().await.is_err());
    assert_eq!(model.lock().unwrap().events, ["scan", "scan-list"]);
}
#[tokio::test]
async fn completed_empty_scan_is_not_replaced_by_an_old_ssid() {
    let model = Arc::new(Mutex::new(Model::default()));
    let (_server, mut nm) = fixture(model.clone()).await;
    nm.preflight().await.unwrap();
    let report = nm.scan_before_ap().await.unwrap();
    assert!(report.valid());
    assert!(report.networks.is_empty());
    assert_eq!(report.omitted, 0);
    assert_eq!(model.lock().unwrap().events, ["scan", "scan-list"]);
}
#[tokio::test]
async fn denied_and_lost_scan_replies_never_retry_or_return_cached_rows() {
    for scan_result in [ScanResult::Denied, ScanResult::LostReply] {
        let model = Arc::new(Mutex::new(Model {
            scan_result,
            ..Default::default()
        }));
        let (_server, mut nm) = fixture(model.clone()).await;
        nm.preflight().await.unwrap();
        assert!(nm.scan_before_ap().await.is_err());
        assert!(nm.scan_before_ap().await.is_err());
        assert_eq!(model.lock().unwrap().events, ["scan"]);
    }
}
#[tokio::test]
async fn accepted_request_without_completion_times_out_once() {
    let model = Arc::new(Mutex::new(Model {
        scan_result: ScanResult::NoCompletion,
        ..Default::default()
    }));
    let (_server, mut nm) = fixture(model.clone()).await;
    nm.preflight().await.unwrap();
    assert!(matches!(
        nm.scan_with_budget(Duration::from_millis(100)).await,
        Err(Error::Busy)
    ));
    assert!(nm.scan_before_ap().await.is_err());
    assert_eq!(model.lock().unwrap().events, ["scan"]);
}
#[tokio::test]
async fn owner_connection_and_timestamp_changes_do_not_publish_scan_success() {
    for scan_result in [
        ScanResult::OwnerChanged,
        ScanResult::ConnectionChanged,
        ScanResult::FutureTimestamp,
        ScanResult::CheckpointChanged,
        ScanResult::RadioDisabled,
    ] {
        let model = Arc::new(Mutex::new(Model {
            scan_result,
            ..Default::default()
        }));
        let (_server, mut nm) = fixture(model.clone()).await;
        nm.preflight().await.unwrap();
        assert!(nm.scan_before_ap().await.is_err());
        assert!(nm.scan_before_ap().await.is_err());
        assert_eq!(model.lock().unwrap().events, ["scan"]);
    }
}
#[tokio::test]
async fn unsupported_scan_falls_back_without_claiming_a_snapshot_but_changed_context_is_fatal() {
    for scan_result in [
        ScanResult::Denied,
        ScanResult::LostReply,
        ScanResult::CheckpointChanged,
        ScanResult::RadioDisabled,
    ] {
        let model = Arc::new(Mutex::new(Model {
            scan_result,
            ..Default::default()
        }));
        let (_server, mut nm) = fixture(model.clone()).await;
        nm.preflight().await.unwrap();
        let result = Network::scan_before_start(&mut nm).await;
        if matches!(scan_result, ScanResult::Denied | ScanResult::LostReply) {
            assert!(matches!(result, Ok(None)));
        } else {
            assert!(result.is_err());
        }
        assert_eq!(model.lock().unwrap().events, ["scan"]);
    }
}
#[tokio::test]
async fn duplicate_foreign_or_excessive_ap_paths_are_refused_before_reading_them() {
    let ap: OwnedObjectPath = "/org/freedesktop/NetworkManager/AccessPoint/1"
        .try_into()
        .unwrap();
    for scan_paths in [
        vec![ap.clone(), ap],
        vec!["/unrelated/Object/1".try_into().unwrap()],
        (0..129)
            .map(|n| {
                format!("/org/freedesktop/NetworkManager/AccessPoint/{n}")
                    .try_into()
                    .unwrap()
            })
            .collect(),
    ] {
        let model = Arc::new(Mutex::new(Model {
            scan_paths,
            ..Default::default()
        }));
        let (_server, mut nm) = fixture(model.clone()).await;
        nm.preflight().await.unwrap();
        assert!(nm.scan_before_ap().await.is_err());
        assert!(nm.scan_before_ap().await.is_err());
        assert_eq!(model.lock().unwrap().events, ["scan", "scan-list"]);
    }
}
#[tokio::test]
async fn checkpoint_or_existing_ap_cannot_trigger_a_scan() {
    for checkpoint in [true, false] {
        let model = Arc::new(Mutex::new(Model::default()));
        let (_server, mut nm) = fixture(model.clone()).await;
        nm.preflight().await.unwrap();
        if checkpoint {
            nm.checkpoint(330).await.unwrap();
        } else {
            model.lock().unwrap().active = true;
        }
        assert!(nm.scan_before_ap().await.is_err());
        assert!(!model.lock().unwrap().events.contains(&"scan"));
    }
}
