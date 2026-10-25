// SPDX-License-Identifier: AGPL-3.0-only
//! Real zbus calls to a strict NM model, not hardware or a real NetworkManager daemon.
use super::*;
use crate::{
    station_commit::{CommitNetwork, CommitProvider},
    station_trial::StationNetwork,
    transaction::Gate,
};
struct Allowed;
async fn prepared(m: Shared) -> (zbus::Connection, NetworkManager, Credentials) {
    let f = prepared_station_fixture(m.clone()).await;
    m.lock().unwrap().commit_mode = true;
    f
}
impl Gate for Allowed {
    async fn check(&self) -> Result<()> {
        Ok(())
    }
}
struct Once(std::sync::atomic::AtomicBool);
impl crate::station_commit::CommitAudit for Once {
    fn consume(&self) -> Result<()> {
        if self.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
            Err(Error::Denied)
        } else {
            Ok(())
        }
    }
}
struct Isolation(Shared);
impl crate::station_trial::TrialGuard for Isolation {
    async fn verify_trial(&self) -> Result<()> {
        Ok(())
    }
}
impl crate::transaction::Guard for Isolation {
    async fn install(&mut self) -> Result<()> {
        panic!("parent already owns guard")
    }
    async fn verify(&self) -> Result<()> {
        Ok(())
    }
    async fn release(&mut self) -> Result<()> {
        let mut m = self.0.lock().unwrap();
        assert!(m.saved && m.station_active && m.profile.is_none() && !m.cp && !m.station_cp);
        m.events.push("release");
        Ok(())
    }
}
#[tokio::test]
async fn complete_engine_uses_real_dbus_backend_and_lost_enable_reply_never_releases_guard() {
    for lost_enable in [false, true] {
        let m = Arc::new(Mutex::new(Model {
            lost_enable_reply: lost_enable,
            ..Default::default()
        }));
        let (_s, mut nm, ap) = prepared(m.clone()).await;
        let c = station_credentials();
        let mut session = nm.commit_session(&ap).unwrap();
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let outcome = crate::station_commit::run(
            &mut session,
            &mut Isolation(m.clone()),
            &Allowed,
            &Once(std::sync::atomic::AtomicBool::new(false)),
            &c,
            std::time::Duration::from_secs(2),
            rx,
        )
        .await
        .unwrap();
        assert_eq!(
            outcome,
            if lost_enable {
                crate::station_commit::CommitOutcome::Unknown
            } else {
                crate::station_commit::CommitOutcome::Saved
            }
        );
        let model = m.lock().unwrap();
        assert_eq!(model.events.contains(&"release"), !lost_enable);
        assert!(!model.events.contains(&"station-rollback"));
        assert_eq!(
            model
                .events
                .iter()
                .filter(|e| **e == "station-save")
                .count(),
            1
        );
        assert_eq!(
            model
                .events
                .iter()
                .filter(|e| **e == "station-enable")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn final_sta_is_saved_unbound_from_worker_and_exact_ap_resources_are_removed() {
    let m = Arc::new(Mutex::new(Model::default()));
    let (_s, mut nm, ap) = prepared(m.clone()).await;
    let c = station_credentials();
    {
        let mut session = nm.commit_session(&ap).unwrap();
        session.begin(&c).await.unwrap();
        assert!(session.usable(&c).await.unwrap());
        session.persist(&c).await.unwrap();
        assert!(session.persist(&c).await.is_err());
        session.finish(&c, &Allowed).await.unwrap();
        assert!(session.finish(&c, &Allowed).await.is_err());
    }
    let model = m.lock().unwrap();
    assert!(model.station_active && model.saved && model.station_profile.is_some());
    assert!(!model.active && !model.cp && !model.station_cp && model.profile.is_none());
    assert_eq!(
        model.events,
        [
            "checkpoint",
            "activate",
            "station-checkpoint",
            "station-activate",
            "station-save",
            "destroy",
            "station-destroy",
            "station-enable",
            "delete"
        ]
    );
    assert!(boolean(
        &model.station_profile.as_ref().unwrap()["connection"],
        "autoconnect"
    )
    .unwrap());
}
#[tokio::test]
async fn lost_or_unverified_save_never_commits_and_exact_checkpoint_restores_ap() {
    for mode in 0..3 {
        let m = Arc::new(Mutex::new(Model {
            lost_save_reply: mode == 0,
            unsaved_after_save: mode == 1,
            volatile_after_save: mode == 2,
            ..Default::default()
        }));
        let (_s, mut nm, ap) = prepared(m.clone()).await;
        let c = station_credentials();
        {
            let mut session = nm.commit_session(&ap).unwrap();
            session.begin(&c).await.unwrap();
            assert!(session.persist(&c).await.is_err());
            assert!(session.persist(&c).await.is_err());
            session.restore_ap(&c).await.unwrap();
        }
        assert!(nm.ready(&ap).await.unwrap());
        let model = m.lock().unwrap();
        assert!(!model.saved && model.station_profile.is_none() && !model.station_active);
        assert!(model.cp && model.profile.is_some());
        assert!(!model.events.contains(&"destroy"));
        assert_eq!(
            model
                .events
                .iter()
                .filter(|e| **e == "station-save")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn lost_checkpoint_or_delete_reply_stops_at_that_exact_boundary_without_guessing() {
    for mode in 0..3 {
        let m = Arc::new(Mutex::new(Model {
            lost_outer_destroy_reply: mode == 0,
            lost_inner_destroy_reply: mode == 1,
            lost_ap_delete_reply: mode == 2,
            ..Default::default()
        }));
        let (_s, mut nm, ap) = prepared(m.clone()).await;
        let c = station_credentials();
        let mut session = nm.commit_session(&ap).unwrap();
        session.begin(&c).await.unwrap();
        session.persist(&c).await.unwrap();
        assert!(session.finish(&c, &Allowed).await.is_err());
        let model = m.lock().unwrap();
        assert!(model.saved && model.station_active);
        assert_eq!(model.events.contains(&"station-destroy"), mode > 0);
        assert_eq!(model.events.contains(&"delete"), mode > 1);
        assert!(!model.events.contains(&"station-rollback"));
    }
}
#[tokio::test]
async fn trial_mode_and_wrong_candidate_never_gain_persistence() {
    let m = Arc::new(Mutex::new(Model::default()));
    let (_s, mut nm, ap) = prepared_station_fixture(m.clone()).await;
    let c = station_credentials();
    {
        let mut session = nm.station_trial(&ap).unwrap();
        session.begin(&c).await.unwrap();
        assert!(session.persist(&c).await.is_err());
        session.restore_ap(&c).await.unwrap();
    }
    m.lock().unwrap().commit_mode = true;
    let mut session = nm.commit_session(&ap).unwrap();
    session.begin(&c).await.unwrap();
    assert!(session.persist(&station_credentials()).await.is_err());
    assert!(!m.lock().unwrap().events.contains(&"station-save"));
    session.restore_ap(&c).await.unwrap();
}
#[tokio::test]
async fn revoked_gate_before_final_checkpoint_keeps_automatic_inner_recovery() {
    struct Revoked;
    impl Gate for Revoked {
        async fn check(&self) -> Result<()> {
            Err(Error::Denied)
        }
    }
    let m = Arc::new(Mutex::new(Model::default()));
    let (_s, mut nm, ap) = prepared(m.clone()).await;
    let c = station_credentials();
    let mut session = nm.commit_session(&ap).unwrap();
    session.begin(&c).await.unwrap();
    session.persist(&c).await.unwrap();
    assert!(session.finish(&c, &Revoked).await.is_err());
    assert!(!m.lock().unwrap().events.contains(&"destroy"));
    session.restore_ap(&c).await.unwrap();
    assert!(!m.lock().unwrap().saved);
}
