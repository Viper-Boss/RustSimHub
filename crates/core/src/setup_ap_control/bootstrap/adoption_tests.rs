// SPDX-License-Identifier: AGPL-3.0-only
//! Real LocalSetup/Auth, files and exclusive guards. Root receipts are fixtures.
use super::*;
async fn ready() -> (Fixture, String) {
    let f = fixture();
    tick(&f).await.unwrap();
    status(&f, State::Ready, |_| {});
    tick(&f).await.unwrap();
    let mut a: Availability =
        serde_json::from_slice(&f.runtime.output.read(Slot::Availability).unwrap().unwrap())
            .unwrap();
    a.station_trial_supported = true;
    a.station_save_supported = true;
    f.runtime.output.publish(Slot::Availability, &a).unwrap();
    let token = claim(&f);
    (f, token)
}
async fn adopt(f: &Fixture, token: &str) -> Result<Value> {
    f.runtime
        .start(
            f.auth.clone(),
            token.into(),
            "adoption-request".into(),
            &idle(),
        )
        .await
}
fn reload(f: Fixture) -> Fixture {
    let Fixture {
        runtime,
        auth,
        gate,
        dirs,
        store,
    } = f;
    let input = runtime.input.clone();
    let output = runtime.output.clone();
    let journal = runtime.journal.clone();
    let coordination = runtime.coordination.clone();
    drop(runtime);
    let runtime = Runtime::with_coordination(input, output, journal, coordination).unwrap();
    Fixture {
        runtime,
        auth,
        gate,
        dirs,
        store,
    }
}
fn private_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
#[tokio::test]
async fn delegated_network_client_cannot_take_the_just_claimed_owner_session() {
    let (f, token) = ready().await;
    let key = SigningKey::from_bytes(&[119; 32]);
    let delegated = {
        let mut a = f.auth.lock().unwrap();
        a.open_pairing(&token).unwrap();
        let scopes = [Scope::NetworkManage];
        let c = a
            .pairing_challenge(key.verifying_key().to_bytes(), Kind::Client, &scopes)
            .unwrap();
        let id = a
            .pairing_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        a.approve(
            &token,
            &id,
            key.verifying_key().to_bytes(),
            Kind::Client,
            &scopes,
        )
        .unwrap();
        let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    let before = hint(&f);
    assert!(adopt(&f, &delegated).await.is_err());
    assert!(hint(&f) == before);
    assert!(!f.runtime.journal.join("pending.json").exists());
    assert!(adopt(&f, &token).await.is_ok());
}
#[tokio::test]
async fn device_claim_to_trial_confirmation_and_saved_proof_keeps_one_original_parent() {
    use rustsimhub_setup_ap::channel::{commit as wire_commit, station as wire_trial};
    use zeroize::Zeroizing;
    let (f, token) = ready().await;
    let before = hint(&f);
    assert!(f.runtime.first_setup_catalogue());
    adopt(&f, &token).await.unwrap();
    assert!(!f.runtime.bootstrap.ready().unwrap());
    assert!(f.runtime.first_setup_catalogue());
    let trial = "22222222-2222-4222-8222-222222222222";
    let commit = "33333333-3333-4333-8333-333333333333";
    let ssid = "synthetic-adopted-wifi";
    let password = "synthetic-adopted-password";
    let operation = before.binding.request_id.clone();
    f.runtime
        .station_begin(
            f.auth.clone(),
            token.clone(),
            operation.clone(),
            trial.into(),
            ssid.into(),
            Zeroizing::new(password.into()),
            &idle(),
        )
        .await
        .unwrap();
    let request = wire_trial::Request::parse(
        &f.runtime.input.read(Slot::StationRequest).unwrap().unwrap(),
        &before.binding,
        now().unwrap(),
    )
    .unwrap();
    assert!(request.binding == before.binding);
    let proof = wire_trial::Status {
        format: 1,
        intent: request.intent().unwrap(),
        phase: wire_trial::Phase::Ready,
        sample_uptime_ms: now().unwrap(),
        ap_https_verified: true,
    };
    f.runtime
        .output
        .publish(Slot::StationStatus, &proof)
        .unwrap();
    assert_eq!(
        f.runtime
            .station_status(
                f.auth.clone(),
                token.clone(),
                operation.clone(),
                trial.into()
            )
            .await
            .unwrap()["state"],
        "ready"
    );
    f.runtime
        .commit_begin(
            f.auth.clone(),
            token.clone(),
            operation.clone(),
            trial.into(),
            commit.into(),
            ssid.into(),
            Zeroizing::new(password.into()),
            &idle(),
        )
        .await
        .unwrap();
    let request = wire_commit::Request::parse(
        &f.runtime
            .input
            .read(Slot::StationCommitRequest)
            .unwrap()
            .unwrap(),
        &before.binding,
        now().unwrap(),
    )
    .unwrap();
    let time = now().unwrap();
    let proof = wire_commit::Status {
        format: 1,
        intent: request.intent().unwrap(),
        phase: wire_commit::Phase::Saved,
        execution_uptime_ms: time,
        observation_boot_id: boot().unwrap(),
        sample_uptime_ms: time,
        settings_saved: true,
        ap_removed: true,
        checkpoints_cleared: true,
        guard_released: true,
        ap_https_verified: false,
    };
    f.runtime
        .output
        .publish(Slot::StationCommitStatus, &proof)
        .unwrap();
    assert_eq!(
        f.runtime
            .commit_status(f.auth.clone(), token.clone(), operation, commit.into())
            .await
            .unwrap()["state"],
        "saved"
    );
    assert!(f.runtime.coordination.network().is_ok());
    assert!(!f.runtime.first_setup_catalogue());
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
    for p in fs::read_dir(&f.runtime.journal).unwrap() {
        let text = fs::read_to_string(p.unwrap().path()).unwrap();
        assert!(!text.contains(ssid) && !text.contains(password) && !text.contains(&token));
    }
    let f = reload(f);
    assert!(f.runtime.session.lock().unwrap().is_none());
    assert!(f.runtime.coordination.network().is_ok());
    tick(&f).await.unwrap();
    assert!(hint(&f).action != Action::Start);
}
#[tokio::test]
async fn pending_write_failure_after_transfer_proof_has_only_metadata_recovery() {
    let (f, token) = ready().await;
    let before = hint(&f);
    let pending = f.runtime.journal.join("pending.json");
    fs::create_dir(&pending).unwrap();
    assert!(adopt(&f, &token).await.is_err());
    assert!(hint(&f) == before);
    assert!(f.runtime.journal.join("first-setup.owner.json").is_file());
    assert!(f.runtime.bootstrap.active.lock().unwrap().is_some());
    fs::remove_dir(pending).unwrap();
    let f = reload(f);
    assert!(hint(&f).action == Action::Cancel);
    assert!(hint(&f).binding == before.binding);
    assert!(f.runtime.coordination.network().is_err());
    assert!(f.runtime.bootstrap.active.lock().unwrap().is_none());
}
#[tokio::test]
async fn interrupted_live_transfer_can_close_with_exact_durable_owner_receipt() {
    let (f, token) = ready().await;
    let path = f.runtime.journal.join("pending.json");
    fs::create_dir(&path).unwrap();
    assert!(adopt(&f, &token).await.is_err());
    fs::remove_dir(path).unwrap();
    f.runtime
        .cancel(f.auth.clone(), token.clone(), hint(&f).binding.request_id)
        .await
        .unwrap();
    status(&f, State::Closed, |_| {});
    tick(&f).await.unwrap();
    let v = f
        .runtime
        .operation_status(f.auth.clone(), token.clone(), hash(b"adoption-request"))
        .await
        .unwrap();
    assert_eq!(v["state"], "closed");
    assert!(f.runtime.coordination.network().is_ok());
    let f = reload(f);
    assert_eq!(
        f.runtime
            .operation_status(f.auth.clone(), token, hash(b"adoption-request"))
            .await
            .unwrap()["state"],
        "closed"
    );
}
#[tokio::test]
async fn an_in_flight_bootstrap_tick_does_not_cancel_the_transferred_owner() {
    struct Paused {
        entered: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
    }
    impl Idle for Paused {
        async fn check(&self, _: &str) -> Result<()> {
            Ok(())
        }
        async fn device(&self, _: Arc<Mutex<LocalSetup>>) -> Result<()> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(())
        }
    }
    let (f, token) = ready().await;
    let before = hint(&f);
    let paused = Paused {
        entered: Arc::new(tokio::sync::Notify::new()),
        release: Arc::new(tokio::sync::Notify::new()),
    };
    let takeover = async {
        paused.entered.notified().await;
        let v = adopt(&f, &token).await;
        paused.release.notify_one();
        v
    };
    let (old, new) = tokio::join!(
        f.runtime
            .bootstrap
            .tick(f.auth.clone(), Some(f.gate.clone()), &paused),
        takeover
    );
    assert!(old.is_err());
    assert_eq!(new.unwrap()["state"], "ready");
    f.runtime.bootstrap.failed();
    assert!(hint(&f) == before);
    assert!(f.runtime.coordination.network().is_err());
    let state = f.runtime.session.lock().unwrap();
    let state = state.as_ref().unwrap();
    assert!(!state.cancelled && state.token.is_some() && state.phase == State::Ready);
}
#[tokio::test]
async fn exact_claim_adopts_once_without_start_deadline_change_or_guard_gap() {
    let (f, token) = ready().await;
    let before = hint(&f);
    let view = f
        .runtime
        .status(f.auth.clone(), token.clone())
        .await
        .unwrap();
    assert_eq!(view["owner_adoption_required"], true);
    assert_eq!(view["station_save_api"], true);
    let v = adopt(&f, &token).await.unwrap();
    assert_eq!(v["state"], "ready");
    assert_eq!(v["operation_id"], before.binding.request_id);
    assert!(hint(&f) == before);
    assert!(f.runtime.coordination.network().is_err());
    assert!(f.runtime.bootstrap.active.lock().unwrap().is_none());
    {
        let s = f.runtime.session.lock().unwrap();
        let s = s.as_ref().unwrap();
        assert!(s.record.binding == before.binding);
        assert_eq!(s.sequence, before.sequence);
        assert!(s.permit.is_some());
    }
    assert!(!f.runtime.journal.join("first-setup.pending.json").exists());
    assert!(f.runtime.journal.join("first-setup.owner.json").exists());
    assert_eq!(adopt(&f, &token).await.unwrap(), v);
    assert!(hint(&f) == before);
    assert!(f
        .runtime
        .start(f.auth.clone(), token.clone(), "another".into(), &idle())
        .await
        .is_err());
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(hint(&f).binding == before.binding);
    assert!(hint(&f).action == Action::KeepAlive);
    status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    assert!(f.runtime.coordination.network().is_ok());
    let f = reload(f);
    assert!(f.runtime.session.lock().unwrap().is_none());
    assert!(f.runtime.coordination.network().is_ok());
}
#[tokio::test]
async fn unavailable_wrong_root_or_cancelled_parent_never_creates_owner_intent() {
    for case in 0..4 {
        let (f, token) = ready().await;
        let original = hint(&f);
        match case {
            0 => status(&f, State::Ready, |s| s.binding.nonce = "f".repeat(64)),
            1 => status(&f, State::Closing, |_| {}),
            2 => {
                let mut a: Availability = serde_json::from_slice(
                    &f.runtime.output.read(Slot::Availability).unwrap().unwrap(),
                )
                .unwrap();
                a.material_sha256 = "e".repeat(64);
                f.runtime.output.publish(Slot::Availability, &a).unwrap();
            }
            _ => {
                f.runtime
                    .bootstrap
                    .cancel(&original.binding.request_id)
                    .unwrap();
            }
        }
        let before = hint(&f);
        assert!(adopt(&f, &token).await.is_err());
        assert!(hint(&f) == before);
        assert!(f.runtime.coordination.network().is_err());
        assert!(!f.runtime.journal.join("pending.json").exists());
        assert!(!f.runtime.journal.join("first-setup.owner.json").exists());
    }
}
#[tokio::test]
async fn signed_owner_without_the_actual_device_claim_cannot_adopt() {
    let f = fixture();
    tick(&f).await.unwrap();
    status(&f, State::Ready, |_| {});
    tick(&f).await.unwrap();
    // A generic Auth bootstrap is not a successful claim through this LocalSetup.
    let token = {
        let mut a = f.auth.lock().unwrap();
        let key = SigningKey::from_bytes(&[118; 32]);
        let c = a
            .bootstrap_challenge(key.verifying_key().to_bytes())
            .unwrap();
        a.bootstrap_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        let c = a.login_challenge(key.verifying_key().to_bytes()).unwrap();
        a.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap()
            .expose()
            .to_owned()
    };
    let before = hint(&f);
    assert!(adopt(&f, &token).await.is_err());
    assert!(hint(&f) == before);
    assert!(!f.runtime.journal.join("pending.json").exists());
}
#[tokio::test]
async fn wrong_token_busy_voice_and_replaced_gate_keep_bootstrap_exclusive() {
    let (f, token) = ready().await;
    let before = hint(&f);
    assert!(adopt(&f, &"0".repeat(64)).await.is_err());
    assert!(f
        .runtime
        .start(
            f.auth.clone(),
            token.clone(),
            "busy".into(),
            &NoCalls { fail: true }
        )
        .await
        .is_err());
    let code = f.dirs[2].path().join("setup.code");
    *f.gate.lock().unwrap() = LocalSetup::load(&code).unwrap();
    assert!(adopt(&f, &token).await.is_err());
    assert!(hint(&f) == before);
    assert!(f.runtime.coordination.network().is_err());
    assert!(!f.runtime.journal.join("pending.json").exists());
}
#[tokio::test]
async fn interrupted_transfer_recovers_metadata_and_only_cancels_original_binding() {
    for point in 0..3 {
        let (f, token) = ready().await;
        let before = hint(&f);
        adopt(&f, &token).await.unwrap();
        let transfer: Value = serde_json::from_slice(
            &rustsimhub_auth::read_private(&f.runtime.journal.join("first-setup.owner.json"), 8192)
                .unwrap(),
        )
        .unwrap();
        private_json(
            &f.runtime.journal.join("first-setup.pending.json"),
            &transfer["bootstrap"],
        );
        if point < 2 {
            fs::remove_file(f.runtime.journal.join("pending.json")).unwrap();
        }
        if point == 0 {
            fs::remove_file(
                f.runtime
                    .journal
                    .join(format!("{}.intent", hash(b"adoption-request"))),
            )
            .unwrap();
        }
        let f = reload(f);
        assert!(hint(&f).action == Action::Cancel);
        assert!(hint(&f).binding == before.binding);
        assert!(f.runtime.bootstrap.active.lock().unwrap().is_none());
        assert!(f.runtime.coordination.network().is_err());
        {
            let s = f.runtime.session.lock().unwrap();
            let s = s.as_ref().unwrap();
            assert!(s.token.is_none() && s.cancelled);
        }
        let v = f
            .runtime
            .operation_status(f.auth.clone(), token.clone(), hash(b"adoption-request"))
            .await
            .unwrap();
        assert_eq!(v["state"], "unknown");
        status(&f, State::Closed, |_| {});
        f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
        assert!(f.runtime.coordination.network().is_ok());
        let f = reload(f);
        assert!(f.runtime.coordination.network().is_ok());
        assert!(f.runtime.session.lock().unwrap().is_none());
    }
}
#[tokio::test]
async fn failure_before_transfer_proof_retains_the_original_bootstrap_cleanup() {
    let (f, token) = ready().await;
    let before = hint(&f);
    fs::create_dir(f.runtime.journal.join("first-setup.owner.json")).unwrap();
    assert!(adopt(&f, &token).await.is_err());
    assert!(hint(&f) == before);
    assert!(f.runtime.bootstrap.active.lock().unwrap().is_some());
    assert!(!f.runtime.journal.join("pending.json").exists());
    fs::remove_dir(f.runtime.journal.join("first-setup.owner.json")).unwrap();
    let f = reload(f);
    assert!(hint(&f).action == Action::Cancel);
    assert!(hint(&f).binding == before.binding);
    assert!(f.runtime.session.lock().unwrap().is_none());
}
#[tokio::test]
async fn mismatched_transfer_or_pending_is_rejected_before_recovery_writes() {
    for case in 0..3 {
        let (f, token) = ready().await;
        adopt(&f, &token).await.unwrap();
        let marker = f.runtime.journal.join("first-setup.owner.json");
        let mut transfer: Value =
            serde_json::from_slice(&rustsimhub_auth::read_private(&marker, 8192).unwrap()).unwrap();
        private_json(
            &f.runtime.journal.join("first-setup.pending.json"),
            &transfer["bootstrap"],
        );
        match case {
            0 => transfer["owner"]["key_sha256"] = "../pending".into(),
            1 => transfer["bootstrap"]["binding"]["nonce"] = "f".repeat(64).into(),
            _ => {
                let mut pending = transfer["owner"].clone();
                pending["owner"]["generation"] = 99.into();
                private_json(&f.runtime.journal.join("pending.json"), &pending);
            }
        }
        private_json(&marker, &transfer);
        let hint_before = hint(&f);
        assert!(bootstrap::adoption::recover(&f.runtime.journal).is_err());
        assert!(hint(&f) == hint_before);
        assert!(f.runtime.journal.join("first-setup.pending.json").exists());
    }
}
#[tokio::test]
async fn adopted_first_setup_round_keeps_the_open_ap_alive_without_a_live_token() {
    let (f, token) = ready().await;
    adopt(&f, &token).await.unwrap();
    // The administrator's session token is gone (page closed, 15-minute expiry): the open
    // first-setup round is still kept alive, with open keep-alives at a bounded rate.
    if let Some(s) = f.runtime.session.try_lock().unwrap().as_mut() {
        s.token = None;
    }
    f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    let first = hint(&f);
    assert!(first.action == Action::KeepAlive && first.open);
    for _ in 0..5 {
        f.runtime.tick(f.auth.clone(), &idle()).await.unwrap();
    }
    assert!(hint(&f) == first, "keep-alives are rate-limited");
    assert!(
        !f.runtime
            .session
            .try_lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .cancelled
    );
}
