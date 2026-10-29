// SPDX-License-Identifier: AGPL-3.0-only
//! Real signed Auth and protected production slots; Root status is explicitly emulated.
use super::super::tests::{fixture, hint, root_status, Fixture};
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use std::os::unix::fs::PermissionsExt;
pub(in crate::setup_ap_control) const ID: &str = "11111111-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";
pub(in crate::setup_ap_control) const SSID: &str = "SYNTHETIC_PRIVATE_SSID";
pub(in crate::setup_ap_control) const PASS: &str = "SYNTHETIC_PRIVATE_PASSWORD";
struct Quiet;
impl Idle for Quiet {
    async fn check(&self, _: &str) -> Result<()> {
        Ok(())
    }
    async fn device(&self, _: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
        Ok(())
    }
}
pub(super) async fn ready(supported: bool) -> Fixture {
    let f = fixture();
    let mut a: Availability =
        serde_json::from_slice(&f.runtime.output.read(Slot::Availability).unwrap().unwrap())
            .unwrap();
    a.station_trial_supported = supported;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    f.runtime
        .start(
            f.auth.clone(),
            f.token.clone(),
            "station-parent".into(),
            &Quiet,
        )
        .await
        .unwrap();
    root_status(&f, State::Ready, |_| {});
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    // During an active AP the worker advertises no new-start availability.
    // Station capability was captured from the exact admitted parent.
    a.tls_origin_ready = false;
    a.gate_ready = false;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    f
}
pub(in crate::setup_ap_control) async fn begin(
    f: &Fixture,
    id: &str,
    password: &str,
) -> Result<Value> {
    f.runtime
        .station_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(f).binding.request_id,
            id.into(),
            SSID.into(),
            Zeroizing::new(password.into()),
            &Quiet,
        )
        .await
}
fn request(f: &Fixture) -> Request {
    let raw = f.runtime.input.read(Slot::StationRequest).unwrap().unwrap();
    Request::parse(&raw, &hint(f).binding, now().unwrap()).unwrap()
}
pub(in crate::setup_ap_control) fn root(
    f: &Fixture,
    phase: Phase,
    change: impl FnOnce(&mut TrialStatus),
) {
    let active = f.runtime.session.lock().unwrap();
    let intent = active
        .as_ref()
        .unwrap()
        .trial
        .as_ref()
        .unwrap()
        .record
        .intent
        .clone();
    let mut s = TrialStatus {
        format: 1,
        intent,
        phase,
        sample_uptime_ms: now().unwrap(),
        ap_https_verified: matches!(phase, Phase::Ready | Phase::Failed),
    };
    change(&mut s);
    f.runtime.output.publish(Slot::StationStatus, &s).unwrap();
}
pub(in crate::setup_ap_control) async fn query(f: &Fixture, id: &str) -> Result<Value> {
    f.runtime
        .station_status(
            f.auth.clone(),
            f.token.clone(),
            hint(f).binding.request_id,
            id.into(),
        )
        .await
}
fn assert_no_secrets(f: &Fixture) {
    for p in fs::read_dir(&f.runtime.journal).unwrap() {
        let p = p.unwrap().path();
        let text = fs::read_to_string(p).unwrap();
        assert!(!text.contains(SSID) && !text.contains(PASS) && !text.contains(&f.token));
    }
}
#[tokio::test]
async fn signed_owner_emits_a_root_valid_request_after_durable_password_free_intent() {
    let f = ready(true).await;
    let result = begin(&f, ID, PASS).await.unwrap();
    assert_eq!(result["state"], "testing");
    let q = request(&f);
    assert!(matches!(q.operation, Operation::Test { .. }));
    let c = owner(f.auth.clone(), f.token.clone(), true).await.unwrap();
    assert_eq!(q.owner_generation, c.principal.generation);
    assert_eq!(q.owner_key_sha256, hash(&c.principal.public_key));
    assert_eq!(q.deadline_uptime_ms - q.requested_uptime_ms, 40_000);
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    assert_no_secrets(&f);
    let before = f.runtime.input.read(Slot::StationRequest).unwrap().unwrap();
    assert_eq!(begin(&f, ID, PASS).await.unwrap()["state"], "testing");
    assert_eq!(
        f.runtime.input.read(Slot::StationRequest).unwrap().unwrap(),
        before
    );
    assert!(begin(&f, ID, "changed-password").await.is_err());
    assert!(begin(&f, OTHER, PASS).await.is_err());
}
#[tokio::test]
async fn old_worker_and_parent_not_ready_refuse_before_any_station_intent() {
    let f = ready(false).await;
    assert!(begin(&f, ID, PASS).await.is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
    let f = ready(true).await;
    root_status(&f, State::Starting, |_| {});
    assert!(begin(&f, ID, PASS).await.is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
}
#[tokio::test]
async fn testing_ack_scrubs_password_but_cannot_erase_a_later_cancel() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    let original = request(&f).intent().unwrap();
    root(&f, Phase::Testing, |_| {});
    assert_eq!(query(&f, ID).await.unwrap()["state"], "testing");
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
    f.runtime
        .station_cancel(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
        )
        .await
        .unwrap();
    assert!(request(&f).cancels(&original));
    query(&f, ID).await.unwrap();
    assert!(request(&f).cancels(&original));
    root(&f, Phase::Failed, |_| {});
    let result = query(&f, ID).await.unwrap();
    assert_eq!(result["state"], "cancelled");
    assert_eq!(result["ap_return_verified"], true);
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert_no_secrets(&f);
}
#[tokio::test]
async fn ready_result_is_exact_and_durable_without_resubmitting_credentials() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    root(&f, Phase::Ready, |s| s.intent.owner_generation += 1);
    assert_eq!(query(&f, ID).await.unwrap()["state"], "testing");
    root(&f, Phase::Ready, |s| {
        s.sample_uptime_ms = s.intent.requested_uptime_ms - 1
    });
    assert_eq!(query(&f, ID).await.unwrap()["state"], "testing");
    root(&f, Phase::Ready, |s| s.ap_https_verified = false);
    assert_eq!(query(&f, ID).await.unwrap()["state"], "testing");
    root(&f, Phase::Ready, |_| {});
    let result = query(&f, ID).await.unwrap();
    assert_eq!(result["state"], "ready");
    assert_eq!(result["authoritative_result"], true);
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert_eq!(begin(&f, ID, PASS).await.unwrap()["state"], "ready");
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
    assert_no_secrets(&f);
}
#[tokio::test]
async fn cancellation_of_a_finished_trial_cannot_replace_the_next_trial() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    root(&f, Phase::Ready, |_| {});
    query(&f, ID).await.unwrap();
    begin(&f, OTHER, PASS).await.unwrap();
    let current = f.runtime.input.read(Slot::StationRequest).unwrap().unwrap();
    f.runtime
        .station_cancel(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
        )
        .await
        .unwrap();
    assert_eq!(
        f.runtime.input.read(Slot::StationRequest).unwrap().unwrap(),
        current
    );
    assert_eq!(
        pending(&f.runtime.journal)
            .unwrap()
            .unwrap()
            .intent
            .trial_id,
        OTHER
    );
}
#[tokio::test]
async fn occupied_call_or_stale_parent_proof_cannot_admit_credentials() {
    struct Busy;
    impl Idle for Busy {
        async fn check(&self, _: &str) -> Result<()> {
            Err(Error::Busy)
        }
        async fn device(&self, _: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
            Err(Error::Busy)
        }
    }
    let f = ready(true).await;
    assert!(f
        .runtime
        .station_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
            SSID.into(),
            Zeroizing::new(PASS.into()),
            &Busy
        )
        .await
        .is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
    root_status(&f, State::Ready, |s| s.sample_uptime_ms = 0);
    assert!(begin(&f, ID, PASS).await.is_err());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
}
#[tokio::test]
async fn signed_paired_network_client_and_wrong_owner_token_cannot_publish() {
    let f = ready(true).await;
    let key = SigningKey::from_bytes(&[113; 32]);
    let token = {
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
    for token in [token, "0".repeat(64)] {
        assert!(f
            .runtime
            .station_begin(
                f.auth.clone(),
                token,
                hint(&f).binding.request_id,
                ID.into(),
                SSID.into(),
                Zeroizing::new(PASS.into()),
                &Quiet
            )
            .await
            .is_err());
    }
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
    assert!(load(&f.runtime.journal, ID).unwrap().is_none());
}
#[tokio::test]
async fn unacknowledged_timeout_requests_cleanup_and_never_replays_test() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    let intent = request(&f).intent().unwrap();
    {
        let mut active = f.runtime.session.lock().unwrap();
        f.runtime
            .station_tick(
                active.as_mut().unwrap(),
                intent.requested_uptime_ms + channel::HEARTBEAT_MS + 1,
            )
            .unwrap();
    }
    assert!(request(&f).cancels(&intent));
    assert!(hint(&f).action == Action::Cancel);
    assert_eq!(begin(&f, ID, PASS).await.unwrap()["state"], "unknown");
    assert!(request(&f).cancels(&intent));
    assert!(begin(&f, OTHER, PASS).await.is_err());
}
#[tokio::test]
async fn restart_produces_only_exact_cancel_and_keeps_network_gate_until_parent_closed() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    let original = request(&f).intent().unwrap();
    let runtime = Runtime::with_coordination(
        f.runtime.input.clone(),
        f.runtime.output.clone(),
        f.runtime.journal.clone(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    let raw = runtime.input.read(Slot::StationRequest).unwrap().unwrap();
    let q = Request::parse(&raw, &original.binding, now().unwrap()).unwrap();
    assert!(q.cancels(&original));
    assert!(runtime.coordination.network().is_err());
    assert_eq!(
        runtime
            .station_begin(
                f.auth.clone(),
                f.token.clone(),
                original.binding.request_id.clone(),
                ID.into(),
                SSID.into(),
                Zeroizing::new(PASS.into()),
                &Quiet
            )
            .await
            .unwrap()["state"],
        "unknown"
    );
    assert_no_secrets(&f);
}
#[tokio::test]
async fn verified_parent_close_cleans_pending_unknown_without_inventing_trial_success() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
    let result = query(&f, ID).await.unwrap();
    assert_eq!(result["state"], "unknown");
    assert_eq!(result["ap_return_verified"], false);
    assert!(f.runtime.coordination.network().is_ok());
}
#[tokio::test]
async fn old_core_closed_proof_permits_scoped_secret_cleanup_after_rollback() {
    let f = ready(true).await;
    begin(&f, ID, PASS).await.unwrap();
    let active = f.runtime.session.lock().unwrap();
    let record = active.as_ref().unwrap().record.clone();
    drop(active);
    let closed = Status {
        format: 1,
        binding: record.binding,
        state: State::Closed,
        sequence: 1,
        sample_uptime_ms: now().unwrap(),
        rollback_verified: true,
        guard_released: true,
        refusal: None,
    };
    write(
        &f.runtime
            .journal
            .join(format!("{}.closed", record.key_sha256)),
        &closed,
    )
    .unwrap();
    fs::remove_file(f.runtime.journal.join("pending.json")).unwrap();
    let runtime = Runtime::with_coordination(
        f.runtime.input.clone(),
        f.runtime.output.clone(),
        f.runtime.journal.clone(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    assert!(runtime.input.read(Slot::StationRequest).unwrap().is_none());
    assert!(pending(&runtime.journal).unwrap().is_none());
    let record = load(&runtime.journal, ID).unwrap().unwrap();
    assert!(
        saved(&runtime.journal, &record)
            .unwrap()
            .unwrap()
            .parent_closed
    );
}
#[tokio::test]
async fn publication_failure_consumes_intent_and_does_not_persist_password() {
    let f = ready(true).await;
    let operation = hint(&f).binding.request_id;
    fs::set_permissions(f.dirs[2].path(), fs::Permissions::from_mode(0o750)).unwrap();
    assert!(f
        .runtime
        .station_begin(
            f.auth.clone(),
            f.token.clone(),
            operation,
            ID.into(),
            SSID.into(),
            Zeroizing::new(PASS.into()),
            &Quiet,
        )
        .await
        .is_err());
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    assert_no_secrets(&f);
    fs::set_permissions(f.dirs[2].path(), fs::Permissions::from_mode(0o2750)).unwrap();
    assert_eq!(begin(&f, ID, PASS).await.unwrap()["state"], "unknown");
    assert!(f
        .runtime
        .input
        .read(Slot::StationRequest)
        .unwrap()
        .is_none());
}
