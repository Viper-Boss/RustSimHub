// SPDX-License-Identifier: AGPL-3.0-only
//! Signed production Auth, real protected slots and journal; Root observations are explicit fixtures.
use super::super::station::tests::{
    begin as station_begin, query as station_query, root as station_root, ID as TRIAL_ID, PASS,
    SSID,
};
use super::super::tests::{hint, root_status, Fixture};
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::os::unix::fs::PermissionsExt;
const ID: &str = "44444444-4444-4444-8444-444444444444";
const OTHER: &str = "55555555-5555-4555-8555-555555555555";
struct Quiet;
impl Idle for Quiet {
    async fn check(&self, _: &str) -> Result<()> {
        Ok(())
    }
    async fn device(&self, _: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
        Ok(())
    }
}
fn capability(f: &Fixture, change: impl FnOnce(&mut protocol::Capability)) {
    let mut c = protocol::Capability {
        format: 1,
        binding: hint(f).binding,
        sample_uptime_ms: now().unwrap(),
        available: true,
        max_per_parent: protocol::MAX_PER_PARENT,
        cooldown_ms: protocol::COOLDOWN_MS,
        remaining: 8,
        refusal: None,
        retry_after_uptime_ms: None,
    };
    change(&mut c);
    f.runtime
        .output
        .publish(Slot::RescanCapability, &c)
        .unwrap();
}
async fn fixture() -> Fixture {
    let f = super::super::tests::fixture();
    let mut a: Availability =
        serde_json::from_slice(&f.runtime.output.read(Slot::Availability).unwrap().unwrap())
            .unwrap();
    a.station_trial_supported = true;
    a.station_save_supported = true;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    f.runtime
        .start(
            f.auth.clone(),
            f.token.clone(),
            "scan-parent".into(),
            &Quiet,
        )
        .await
        .unwrap();
    root_status(&f, State::Ready, |_| {});
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    capability(&f, |_| {});
    f
}
async fn begin(f: &Fixture, id: &str) -> Result<Value> {
    f.runtime
        .rescan_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(f).binding.request_id,
            id.into(),
            &Quiet,
        )
        .await
}
async fn query(f: &Fixture, id: &str) -> Result<Value> {
    f.runtime
        .rescan_status(
            f.auth.clone(),
            f.token.clone(),
            hint(f).binding.request_id,
            id.into(),
        )
        .await
}
fn payload(f: &Fixture) -> Request {
    serde_json::from_slice(&f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap()).unwrap()
}
fn root(f: &Fixture, phase: Phase, change: impl FnOnce(&mut ScanStatus)) {
    let record = pending(&f.runtime.journal).unwrap().unwrap();
    let t = now().unwrap();
    let snapshot = (phase == Phase::Ready).then_some(rustsimhub_setup_ap::nm::scan::Snapshot {
        requested_uptime_ms: t,
        completed_uptime_ms: t,
        last_scan_uptime_ms: t,
        networks: vec![],
        omitted: 0,
        truncated: false,
    });
    let mut s = ScanStatus {
        format: 1,
        intent: record.intent,
        phase,
        sample_boot_id: boot().unwrap(),
        sample_uptime_ms: t,
        finished_uptime_ms: if phase == Phase::Scanning {
            None
        } else {
            Some(t)
        },
        ap_https_verified: matches!(phase, Phase::Ready | Phase::Failed),
        snapshot,
        refusal: (phase == Phase::Refused).then_some(protocol::Refusal::Conflict),
    };
    change(&mut s);
    f.runtime.output.publish(Slot::RescanStatus, &s).unwrap();
}
#[tokio::test]
async fn owner_begin_is_durable_password_free_single_dispatch_and_scanning_keeps_the_scan_slot() {
    let f = fixture().await;
    let original = hint(&f).binding;
    assert_eq!(begin(&f, ID).await.unwrap()["state"], "submitting");
    let q = payload(&f);
    denied(q.validate(&original, now().unwrap())).unwrap();
    let r = load(&f.runtime.journal, ID).unwrap().unwrap();
    assert!(q.intent().unwrap() == r.intent);
    assert!(marker(&f.runtime.journal, &r, "dispatch").unwrap());
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    let before = f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap();
    assert_eq!(begin(&f, ID).await.unwrap()["state"], "submitting");
    assert_eq!(
        f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap(),
        before
    );
    assert!(begin(&f, OTHER).await.is_err());
    root(&f, Phase::Scanning, |_| {});
    assert_eq!(query(&f, ID).await.unwrap()["state"], "scanning");
    assert_eq!(
        f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap(),
        before
    );
    assert_eq!(
        hint(&f).binding.deadline_uptime_ms,
        original.deadline_uptime_ms
    );
    for entry in fs::read_dir(&f.runtime.journal).unwrap() {
        let text = fs::read_to_string(entry.unwrap().path()).unwrap();
        assert!(!text.contains(&f.token) && !text.contains(PASS));
    }
    let v = f
        .runtime
        .status(f.auth.clone(), f.token.clone())
        .await
        .unwrap();
    assert_eq!(v["scan"]["available"], false);
    assert_eq!(v["station_trial_api"], false);
    assert_eq!(v["station_save_api"], false);
    assert!(station_begin(&f, TRIAL_ID, PASS).await.is_err());
}
#[tokio::test]
async fn unsupported_stale_foreign_capability_or_parent_never_creates_an_intent() {
    for failure in 0..5 {
        let f = fixture().await;
        match failure {
            0 => {
                f.runtime.output.clear(Slot::RescanCapability).unwrap();
            }
            1 => capability(&f, |c| c.sample_uptime_ms = 0),
            2 => capability(&f, |c| c.binding.nonce = "f".repeat(64)),
            3 => root_status(&f, State::Starting, |_| {}),
            _ => root_status(&f, State::Ready, |p| p.sequence += 100),
        }
        assert!(begin(&f, ID).await.is_err());
        assert!(load(&f.runtime.journal, ID).unwrap().is_none());
        assert!(f.runtime.input.read(Slot::RescanRequest).unwrap().is_none());
    }
}
#[tokio::test]
async fn signed_network_client_and_invalid_bearer_cannot_begin_or_query_owner_rescans() {
    let f = fixture().await;
    let key = SigningKey::from_bytes(&[113; 32]);
    let client = {
        let mut a = f.auth.lock().unwrap();
        a.open_pairing(&f.token).unwrap();
        let c = a
            .pairing_challenge(
                key.verifying_key().to_bytes(),
                Kind::Client,
                &[Scope::NetworkManage],
            )
            .unwrap();
        let pair = a
            .pairing_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        a.approve(
            &f.token,
            &pair,
            key.verifying_key().to_bytes(),
            Kind::Client,
            &[Scope::NetworkManage],
        )
        .unwrap();
        let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    for token in [client, "0".repeat(64)] {
        assert!(f
            .runtime
            .rescan_begin(
                f.auth.clone(),
                token.clone(),
                hint(&f).binding.request_id,
                ID.into(),
                &Quiet
            )
            .await
            .is_err());
        assert!(f
            .runtime
            .rescan_status(
                f.auth.clone(),
                token,
                hint(&f).binding.request_id,
                ID.into()
            )
            .await
            .is_err());
    }
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
}
#[tokio::test]
async fn empty_success_and_explicit_refusal_have_distinct_authoritative_views() {
    for phase in [Phase::Ready, Phase::Refused] {
        let f = fixture().await;
        begin(&f, ID).await.unwrap();
        root(&f, phase, |_| {});
        let v = query(&f, ID).await.unwrap();
        assert_eq!(v["authoritative_result"], true);
        assert_eq!(v["execution_started"], phase == Phase::Ready);
        assert_eq!(v["ap_return_verified"], phase == Phase::Ready);
        if phase == Phase::Ready {
            assert_eq!(v["snapshot"]["networks"], json!([]));
        } else {
            assert!(v["snapshot"].is_null());
        }
        assert!(pending(&f.runtime.journal).unwrap().is_none());
        assert!(f.runtime.input.read(Slot::RescanRequest).unwrap().is_none());
        let text = v.to_string();
        assert!(
            !text.contains("nonce")
                && !text.contains("owner_key_sha256")
                && !text.contains(&f.token)
        );
        let first = fs::read(file(&f.runtime.journal, ID, "result").unwrap()).unwrap();
        begin(&f, ID).await.unwrap();
        assert_eq!(
            fs::read(file(&f.runtime.journal, ID, "result").unwrap()).unwrap(),
            first
        );
    }
}
#[tokio::test]
async fn altered_nonce_owner_generation_and_future_proof_do_not_finish_or_clear_the_scan() {
    for failure in 0..3 {
        let f = fixture().await;
        begin(&f, ID).await.unwrap();
        let before = f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap();
        root(&f, Phase::Ready, |s| match failure {
            0 => s.intent.binding.nonce = "f".repeat(64),
            1 => s.intent.owner_generation += 1,
            _ => s.sample_uptime_ms += 100000,
        });
        let v = query(&f, ID).await.unwrap();
        assert_eq!(v["authoritative_result"], false);
        assert!(pending(&f.runtime.journal).unwrap().is_some());
        assert_eq!(
            f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap(),
            before
        );
    }
}
#[tokio::test]
async fn exact_cancel_survives_scanning_heartbeats_until_verified_failure() {
    let f = fixture().await;
    begin(&f, ID).await.unwrap();
    let intent = payload(&f).intent().unwrap();
    f.runtime
        .rescan_cancel(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
        )
        .await
        .unwrap();
    let cancel = payload(&f);
    assert!(cancel.cancels(&intent));
    root(&f, Phase::Scanning, |_| {});
    query(&f, ID).await.unwrap();
    assert!(payload(&f).cancels(&intent));
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    root(&f, Phase::Failed, |_| {});
    let v = query(&f, ID).await.unwrap();
    assert_eq!(v["state"], "cancelled");
    assert_eq!(v["ap_return_verified"], true);
    assert!(pending(&f.runtime.journal).unwrap().is_none());
}
#[tokio::test]
async fn unknown_result_is_preserved_and_even_parent_closed_cannot_silently_archive_it() {
    let f = fixture().await;
    begin(&f, ID).await.unwrap();
    root(&f, Phase::Unknown, |_| {});
    let v = query(&f, ID).await.unwrap();
    assert_eq!(v["state"], "unknown");
    assert_eq!(v["authoritative_result"], false);
    assert!(hint(&f).action == Action::Cancel);
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    assert!(begin(&f, OTHER).await.is_err());
}
#[tokio::test]
async fn a_closed_parent_waits_for_exact_non_execution_proof_before_releasing_the_permit() {
    let f = fixture().await;
    begin(&f, ID).await.unwrap();
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    root(&f, Phase::Refused, |s| {
        s.refusal = Some(protocol::Refusal::ParentClosed)
    });
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert!(f.runtime.coordination.network().is_ok());
    assert_eq!(query(&f, ID).await.unwrap()["historical"], true);
}
#[tokio::test]
async fn runtime_restart_cancels_and_queries_original_intent_without_replaying_scan() {
    let f = fixture().await;
    begin(&f, ID).await.unwrap();
    let intent = payload(&f).intent().unwrap();
    let runtime = Runtime::with_coordination(
        f.runtime.input.clone(),
        f.runtime.output.clone(),
        f.runtime.journal.clone(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    assert!(payload(&f).cancels(&intent));
    assert!(hint(&f).action == Action::Cancel);
    let before = f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap();
    let v = runtime
        .rescan_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
            &Quiet,
        )
        .await
        .unwrap();
    assert_eq!(v["state"], "unknown");
    assert_eq!(
        f.runtime.input.read(Slot::RescanRequest).unwrap().unwrap(),
        before
    );
    assert!(runtime.coordination.network().is_err());
}
#[tokio::test]
async fn payload_publish_failure_keeps_intent_pending_and_still_cancels_the_parent() {
    let f = fixture().await;
    let link = f.dirs[2].path().join("rescan-request.json");
    std::os::unix::fs::symlink(f.dirs[2].path().join("missing-target"), &link).unwrap();
    assert!(begin(&f, ID).await.is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_some());
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    assert!(hint(&f).action == Action::Cancel);
    assert!(f.runtime.coordination.network().is_err());
    assert!(!f.dirs[2].path().join("missing-target").exists());
}
#[tokio::test]
async fn ready_trial_is_explicitly_ended_before_scan_and_its_confirmation_is_blocked() {
    let f = fixture().await;
    station_begin(&f, TRIAL_ID, PASS).await.unwrap();
    station_root(
        &f,
        rustsimhub_setup_ap::channel::station::Phase::Ready,
        |_| {},
    );
    station_query(&f, TRIAL_ID).await.unwrap();
    capability(&f, |c| {
        c.available = false;
        c.refusal = Some(protocol::Refusal::Conflict);
    });
    let status = f
        .runtime
        .status(f.auth.clone(), f.token.clone())
        .await
        .unwrap();
    assert_eq!(status["scan"]["available"], true);
    assert_eq!(status["scan"]["will_end_unsaved_trial"], true);
    begin(&f, ID).await.unwrap();
    let scan = payload(&f);
    let raw = f.runtime.input.read(Slot::StationRequest).unwrap().unwrap();
    let ended: rustsimhub_setup_ap::channel::station::Request =
        serde_json::from_slice(&raw).unwrap();
    assert!(matches!(
        ended.operation,
        rustsimhub_setup_ap::channel::station::Operation::Cancel
    ));
    assert_eq!(scan.binding.request_id, ended.binding.request_id);
    assert!(f
        .runtime
        .commit_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            TRIAL_ID.into(),
            OTHER.into(),
            SSID.into(),
            zeroize::Zeroizing::new(PASS.into()),
            &Quiet
        )
        .await
        .is_err());
    assert!(!f
        .runtime
        .journal
        .join(format!("commit-{OTHER}.intent"))
        .exists());
}
#[tokio::test]
async fn live_station_trial_blocks_scan_before_rescan_intent_is_written() {
    let f = fixture().await;
    station_begin(&f, TRIAL_ID, PASS).await.unwrap();
    assert!(begin(&f, ID).await.is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
}
#[tokio::test]
async fn worst_escaped_snapshot_fits_private_saved_record_and_links_are_rejected() {
    let f = fixture().await;
    begin(&f, ID).await.unwrap();
    root(&f, Phase::Ready, |s| {
        let snapshot = s.snapshot.as_mut().unwrap();
        snapshot.networks = (0..32)
            .map(|i| rustsimhub_setup_ap::nm::scan::Choice {
                ssid: format!("{i:02}{}", "\\".repeat(30)),
                strength: 100,
                frequency_mhz: 2412,
                security: rustsimhub_setup_ap::nm::scan::Security::Wpa2Personal,
            })
            .collect();
        snapshot.truncated = true;
        snapshot.omitted = 128;
    });
    query(&f, ID).await.unwrap();
    assert!(
        fs::read(file(&f.runtime.journal, ID, "result").unwrap())
            .unwrap()
            .len()
            <= 8192
    );
    fs::set_permissions(
        file(&f.runtime.journal, ID, "result").unwrap(),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(query(&f, ID).await.is_err());
}
#[tokio::test]
async fn call_activity_and_authority_change_during_idle_check_prevent_dispatch() {
    struct Busy;
    impl Idle for Busy {
        async fn check(&self, _: &str) -> Result<()> {
            Err(Error::Busy)
        }
        async fn device(&self, _: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
            Err(Error::Busy)
        }
    }
    let f = fixture().await;
    assert!(f
        .runtime
        .rescan_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
            &Busy
        )
        .await
        .is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
    struct Reset {
        auth: Arc<Mutex<Auth>>,
        replacement: Mutex<Option<Auth>>,
    }
    impl Idle for Reset {
        async fn check(&self, _: &str) -> Result<()> {
            *self.auth.lock().unwrap() = self.replacement.lock().unwrap().take().unwrap();
            Ok(())
        }
        async fn device(&self, _: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
            Ok(())
        }
    }
    let replacement = Auth::new(
        rustsimhub_auth::Identity::load(f.dirs[1].path()).unwrap(),
        f._store.handle(),
    )
    .unwrap();
    let reset = Reset {
        auth: f.auth.clone(),
        replacement: Mutex::new(Some(replacement)),
    };
    assert!(f
        .runtime
        .rescan_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
            &reset
        )
        .await
        .is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
    assert!(f.runtime.input.read(Slot::RescanRequest).unwrap().is_none());
}

#[tokio::test]
async fn pending_confirmation_blocks_rescan_before_any_scan_intent_or_payload() {
    let f = fixture().await;
    station_begin(&f, TRIAL_ID, PASS).await.unwrap();
    station_root(
        &f,
        rustsimhub_setup_ap::channel::station::Phase::Ready,
        |_| {},
    );
    station_query(&f, TRIAL_ID).await.unwrap();
    f.runtime
        .commit_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            TRIAL_ID.into(),
            OTHER.into(),
            SSID.into(),
            zeroize::Zeroizing::new(PASS.into()),
            &Quiet,
        )
        .await
        .unwrap();
    assert!(begin(&f, ID).await.is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
    assert!(f.runtime.input.read(Slot::RescanRequest).unwrap().is_none());
}
