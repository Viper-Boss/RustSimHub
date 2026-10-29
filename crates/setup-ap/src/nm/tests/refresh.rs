// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::ap_rescan::{RescanNetwork, RescanProvider};
use std::time::Duration;

async fn ready(model: Shared) -> (zbus::Connection, NetworkManager, Credentials) {
    let (server, mut nm) = fixture(model).await;
    nm.retain_ap_for_station_trial().unwrap();
    let ap = Credentials::generate().unwrap();
    nm.preflight().await.unwrap();
    nm.checkpoint(330).await.unwrap();
    nm.activate(&ap).await.unwrap();
    assert!(nm.ready(&ap).await.unwrap());
    (server, nm, ap)
}
struct TestLease;
impl crate::transaction::Gate for TestLease {
    async fn check(&self) -> Result<()> {
        Ok(())
    }
}
struct TestGuard;
impl crate::station_trial::TrialGuard for TestGuard {
    async fn verify_trial(&self) -> Result<()> {
        Ok(())
    }
}
#[tokio::test]
async fn real_dbus_rescan_backend_and_bounded_transaction_agree_on_restoration_outcomes() {
    use crate::ap_rescan::{self, RescanOutcome};
    for mode in [
        "ok",
        "lost-pause",
        "scan-refused",
        "lost-checkpoint",
        "restore-failed",
    ] {
        let model = Arc::new(Mutex::new(Model {
            retain_ap: true,
            lost_deactivate_reply: mode == "lost-pause",
            lost_station_cp_reply: mode == "lost-checkpoint",
            fail_station_rollback: mode == "restore-failed",
            scan_result: if mode == "scan-refused" {
                ScanResult::Denied
            } else {
                ScanResult::Complete
            },
            ..Default::default()
        }));
        let (_server, mut nm, ap) = ready(model.clone()).await;
        let (_stop, rx) = tokio::sync::watch::channel(false);
        let result = {
            let mut session = nm.rescan_session(&ap).unwrap();
            ap_rescan::run(
                &mut session,
                &TestGuard,
                &TestLease,
                Duration::from_secs(3),
                rx,
            )
            .await
            .unwrap()
        };
        match mode {
            "ok" => assert!(matches!(result, RescanOutcome::ScannedAndRestored(_))),
            "lost-pause" | "scan-refused" => {
                assert!(matches!(result, RescanOutcome::FailedAndRestored))
            }
            _ => assert!(matches!(result, RescanOutcome::Unknown)),
        }
        if matches!(mode, "ok" | "lost-pause" | "scan-refused") {
            assert!(nm.ready(&ap).await.unwrap());
            assert!(nm.refresh.is_none());
        } else {
            assert!(nm.refresh.is_some());
            assert!(nm.rollback(&ap).await.is_err());
        }
        let m = model.lock().unwrap();
        assert!(m.events.iter().filter(|v| **v == "deactivate").count() <= 1);
        assert!(m.events.iter().filter(|v| **v == "scan").count() <= 1);
        assert!(
            !m.events.contains(&"rollback")
                && !m.events.contains(&"station-save")
                && !m.events.contains(&"delete")
        );
    }
}
#[tokio::test]
async fn rescan_pauses_only_owned_ap_scans_once_and_restores_the_same_ram_profile() {
    for (rollback_consumes, home_during_refresh) in [(false, false), (true, false), (false, true)] {
        let model = Arc::new(Mutex::new(Model {
            retain_ap: true,
            rollback_consumes,
            home: home_during_refresh,
            home_active: home_during_refresh,
            home_during_refresh,
            ..Default::default()
        }));
        let (_server, mut nm, ap) = ready(model.clone()).await;
        let old_profiles = nm.connections().await.unwrap();
        let old_profile = nm.profile.clone();
        {
            let mut scan = nm.rescan_session(&ap).unwrap();
            assert!(scan.scan().await.is_err());
            scan.pause_ap().await.unwrap();
            assert!(scan.pause_ap().await.is_err());
            let result = scan.scan().await.unwrap();
            assert!(result.valid());
            assert!(scan.scan().await.is_err());
            scan.restore_ap().await.unwrap();
            assert!(scan.pause_ap().await.is_err());
            assert!(scan.scan().await.is_err());
        }
        assert!(nm.ready(&ap).await.unwrap());
        assert!(nm.profile == old_profile);
        assert!(nm.connections().await.unwrap() == old_profiles);
        assert!(nm.refresh.is_none());
        let m = model.lock().unwrap();
        assert!(m.active && m.cp && !m.station_cp);
        assert!(m.station_profile.is_none());
        assert!(!m.saved);
        assert_eq!(m.events.iter().filter(|v| **v == "deactivate").count(), 1);
        assert_eq!(m.events.iter().filter(|v| **v == "scan").count(), 1);
        assert_eq!(
            m.events
                .iter()
                .filter(|v| **v == "station-rollback")
                .count(),
            1
        );
        assert!(
            !m.events.contains(&"station-save")
                && !m.events.contains(&"station-enable")
                && !m.events.contains(&"delete")
        );
    }
}
#[tokio::test]
async fn lost_pause_reply_or_scan_failure_still_restores_without_replaying_a_mutation() {
    for mode in ["lost-pause", "failed-pause", "denied-scan", "lost-scan"] {
        let model = Arc::new(Mutex::new(Model {
            retain_ap: true,
            lost_deactivate_reply: mode == "lost-pause",
            failed_deactivate: mode == "failed-pause",
            scan_result: match mode {
                "denied-scan" => ScanResult::Denied,
                "lost-scan" => ScanResult::LostReply,
                _ => ScanResult::Complete,
            },
            ..Default::default()
        }));
        let (_server, mut nm, ap) = ready(model.clone()).await;
        {
            let mut scan = nm.rescan_session(&ap).unwrap();
            let paused = scan.pause_ap().await;
            if mode.ends_with("pause") {
                assert!(paused.is_err());
                assert!(scan.scan().await.is_err());
            } else {
                paused.unwrap();
                assert!(scan.scan().await.is_err());
                assert!(scan.scan().await.is_err());
            }
            scan.restore_ap().await.unwrap();
        }
        assert!(nm.ready(&ap).await.unwrap());
        let m = model.lock().unwrap();
        assert_eq!(m.events.iter().filter(|v| **v == "deactivate").count(), 1);
        assert!(m.events.iter().filter(|v| **v == "scan").count() <= 1);
        assert_eq!(
            m.events
                .iter()
                .filter(|v| **v == "station-rollback")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn uncertain_checkpoint_blocks_parent_cleanup_and_new_trials_until_read_only_reconciliation()
{
    for mode in [
        "lost-checkpoint",
        "failed-rollback",
        "lost-rollback",
        "lost-destroy",
    ] {
        let model = Arc::new(Mutex::new(Model {
            retain_ap: true,
            lost_station_cp_reply: mode == "lost-checkpoint",
            fail_station_rollback: mode == "failed-rollback",
            lost_inner_rollback_reply: mode == "lost-rollback",
            lost_inner_destroy_reply: mode == "lost-destroy",
            ..Default::default()
        }));
        let (_server, mut nm, ap) = ready(model.clone()).await;
        {
            let mut scan = nm.rescan_session(&ap).unwrap();
            let pause = scan.pause_ap().await;
            if mode == "lost-checkpoint" {
                assert!(pause.is_err());
            } else {
                pause.unwrap();
                scan.scan().await.unwrap();
            }
            assert!(scan.restore_ap().await.is_err());
        }
        assert!(nm.refresh.is_some());
        assert!(nm.rollback(&ap).await.is_err());
        assert!(nm.station_trial(&ap).is_err());
        assert!(crate::station_commit::CommitProvider::commit_session(&mut nm, &ap).is_err());
        let before = model.lock().unwrap().events.clone();
        {
            let mut cleanup = nm.rescan_session(&ap).unwrap();
            assert!(cleanup.pause_ap().await.is_err());
            assert!(cleanup.scan().await.is_err());
            if mode == "lost-destroy" {
                cleanup.restore_ap().await.unwrap();
            } else {
                assert!(cleanup.restore_ap().await.is_err());
            }
        }
        assert_eq!(
            model.lock().unwrap().events,
            before,
            "uncertain writes must not be retried"
        );
        if mode != "lost-destroy" {
            // Simulate the checkpoint's own finite NM rollback; subject performs no write.
            {
                let mut m = model.lock().unwrap();
                m.active = true;
                m.station_cp = false;
            }
            let mut cleanup = nm.rescan_session(&ap).unwrap();
            cleanup.restore_ap().await.unwrap();
        }
        assert_eq!(model.lock().unwrap().events, before);
        assert!(nm.refresh.is_none());
        assert!(nm.ready(&ap).await.unwrap());
    }
}
#[tokio::test]
async fn foreign_checkpoint_and_owner_change_are_not_treated_as_owned_cleanup() {
    for scan_result in [ScanResult::ForeignCheckpoint, ScanResult::OwnerChanged] {
        let model = Arc::new(Mutex::new(Model {
            retain_ap: true,
            scan_result,
            ..Default::default()
        }));
        let (_server, mut nm, ap) = ready(model.clone()).await;
        {
            let mut scan = nm.rescan_session(&ap).unwrap();
            scan.pause_ap().await.unwrap();
            assert!(scan.scan().await.is_err());
            assert!(scan.restore_ap().await.is_err());
        }
        assert!(nm.refresh.is_some());
        assert!(nm.rollback(&ap).await.is_err());
        assert!(!model.lock().unwrap().events.contains(&"station-rollback"));
        assert!(!model.lock().unwrap().events.contains(&"rollback"));
    }
}
