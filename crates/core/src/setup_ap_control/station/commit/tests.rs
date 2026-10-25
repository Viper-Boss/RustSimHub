// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth/journal/protected files; Root receipts are explicitly emulated.
use super::*;
use crate::setup_ap_control::station::tests::{begin, query, ready, root, ID, PASS, SSID};
use crate::setup_ap_control::tests::{hint, root_status, Fixture};
use std::os::unix::fs::PermissionsExt;
const COMMIT: &str = "33333333-3333-4333-8333-333333333333";
const OTHER: &str = "44444444-4444-4444-8444-444444444444";
#[tokio::test]
async fn signed_network_client_or_wrong_token_cannot_confirm_even_an_owner_ready_trial() {
    use ed25519_dalek::{Signer, SigningKey};
    let f = fixture().await;
    let key = SigningKey::from_bytes(&[115; 32]);
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
        let id = a
            .pairing_finish(&c.id, &key.sign(&c.transcript).to_bytes())
            .unwrap();
        a.approve(
            &f.token,
            &id,
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
            .commit_begin(
                f.auth.clone(),
                token,
                hint(&f).binding.request_id,
                ID.into(),
                COMMIT.into(),
                SSID.into(),
                Zeroizing::new(PASS.into()),
                &Quiet
            )
            .await
            .is_err());
    }
    assert!(load(&f.runtime.journal, COMMIT).unwrap().is_none());
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
}
#[tokio::test]
async fn busy_call_refuses_before_intent_and_restored_result_allows_only_a_new_explicit_confirmation(
) {
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
        .commit_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
            COMMIT.into(),
            SSID.into(),
            Zeroizing::new(PASS.into()),
            &Busy
        )
        .await
        .is_err());
    assert!(load(&f.runtime.journal, COMMIT).unwrap().is_none());
    confirm(&f, COMMIT, PASS).await.unwrap();
    receipt(&f, CommitPhase::RestoredAp, |_| {});
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "restored_ap");
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert!(f.runtime.coordination.network().is_err());
    assert_eq!(
        confirm(&f, COMMIT, PASS).await.unwrap()["state"],
        "restored_ap"
    );
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
    assert_eq!(
        confirm(&f, OTHER, PASS).await.unwrap()["state"],
        "submitted"
    );
}
struct Quiet;
impl Idle for Quiet {
    async fn check(&self, _: &str) -> Result<()> {
        Ok(())
    }
    async fn device(&self, _: Arc<Mutex<rustsimhub_auth::LocalSetup>>) -> Result<()> {
        Ok(())
    }
}
async fn fixture() -> Fixture {
    let f = ready(true).await;
    f.runtime
        .session
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .save_supported = true;
    begin(&f, ID, PASS).await.unwrap();
    root(&f, Phase::Ready, |_| {});
    query(&f, ID).await.unwrap();
    f
}
async fn confirm(f: &Fixture, id: &str, password: &str) -> Result<Value> {
    f.runtime
        .commit_begin(
            f.auth.clone(),
            f.token.clone(),
            hint(f).binding.request_id,
            ID.into(),
            id.into(),
            SSID.into(),
            Zeroizing::new(password.into()),
            &Quiet,
        )
        .await
}
async fn result(f: &Fixture, id: &str) -> Result<Value> {
    f.runtime
        .commit_status(
            f.auth.clone(),
            f.token.clone(),
            hint(f).binding.request_id,
            id.into(),
        )
        .await
}
fn receipt(f: &Fixture, phase: CommitPhase, mutate: impl FnOnce(&mut CommitStatus)) {
    let r = pending(&f.runtime.journal).unwrap().unwrap();
    let time = now().unwrap();
    let saved = phase == CommitPhase::Saved;
    let mut status = CommitStatus {
        format: 1,
        intent: r.intent,
        phase,
        execution_uptime_ms: time,
        observation_boot_id: boot().unwrap(),
        sample_uptime_ms: time,
        settings_saved: saved,
        ap_removed: saved,
        checkpoints_cleared: saved,
        guard_released: saved,
        ap_https_verified: phase == CommitPhase::RestoredAp,
    };
    mutate(&mut status);
    f.runtime
        .output
        .publish(Slot::StationCommitStatus, &status)
        .unwrap();
}
fn no_secrets(f: &Fixture) {
    for p in fs::read_dir(&f.runtime.journal).unwrap() {
        let text = fs::read_to_string(p.unwrap().path()).unwrap();
        assert!(!text.contains(SSID) && !text.contains(PASS) && !text.contains(&f.token));
    }
}
#[tokio::test]
async fn current_owner_confirmation_is_durable_exact_and_never_resubmitted() {
    let f = fixture().await;
    assert_eq!(
        confirm(&f, COMMIT, PASS).await.unwrap()["state"],
        "submitted"
    );
    let before = f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .unwrap();
    let q = denied(protocol::Request::parse(
        &before,
        &hint(&f).binding,
        now().unwrap(),
    ))
    .unwrap();
    assert_eq!(q.commit_id, COMMIT);
    assert_eq!(
        q.trial.owner_generation,
        pending(&f.runtime.journal)
            .unwrap()
            .unwrap()
            .trial
            .owner
            .generation
    );
    assert_eq!(
        confirm(&f, COMMIT, PASS).await.unwrap()["state"],
        "submitted"
    );
    assert_eq!(
        f.runtime
            .input
            .read(Slot::StationCommitRequest)
            .unwrap()
            .unwrap(),
        before
    );
    assert!(confirm(&f, COMMIT, "changed-password").await.is_err());
    assert!(confirm(&f, OTHER, PASS).await.is_err());
    assert!(begin(&f, OTHER, PASS).await.is_err());
    no_secrets(&f);
}
#[tokio::test]
async fn root_ack_scrubs_password_and_complete_saved_proof_releases_parent_then_survives_runtime_restart(
) {
    let f = fixture().await;
    confirm(&f, COMMIT, PASS).await.unwrap();
    receipt(&f, CommitPhase::Saving, |_| {});
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "saving");
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
    assert!(f.runtime.coordination.network().is_err());
    receipt(&f, CommitPhase::Saved, |_| {});
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "saved");
    assert!(f.runtime.coordination.network().is_ok());
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert!(!f.runtime.journal.join("pending.json").exists());
    let runtime = Runtime::with_coordination(
        f.runtime.input.clone(),
        f.runtime.output.clone(),
        f.runtime.journal.clone(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    assert!(runtime.session.lock().unwrap().is_none());
    let v = runtime
        .commit_status(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            COMMIT.into(),
        )
        .await
        .unwrap();
    assert_eq!(v["state"], "saved");
    assert_eq!(v["historical"], true);
    assert_eq!(confirm(&f, COMMIT, PASS).await.unwrap()["state"], "saved");
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
    no_secrets(&f);
}
#[tokio::test]
async fn wrong_generation_missing_cleanup_or_foreign_observation_cannot_confirm_save() {
    let f = fixture().await;
    confirm(&f, COMMIT, PASS).await.unwrap();
    for mode in 0..3 {
        receipt(&f, CommitPhase::Saved, |s| match mode {
            0 => s.intent.trial.owner_generation += 1,
            1 => s.guard_released = false,
            _ => s.observation_boot_id = "55555555-5555-4555-8555-555555555555".into(),
        });
        assert_ne!(result(&f, COMMIT).await.unwrap()["state"], "saved");
        assert!(f.runtime.coordination.network().is_err());
    }
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_some());
    receipt(&f, CommitPhase::Saved, |_| {});
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "saved");
}
#[tokio::test]
async fn recovered_core_withdraws_secret_and_only_queries_existing_root_intent() {
    let f = fixture().await;
    confirm(&f, COMMIT, PASS).await.unwrap();
    let r = pending(&f.runtime.journal).unwrap().unwrap();
    let runtime = Runtime::with_coordination(
        f.runtime.input.clone(),
        f.runtime.output.clone(),
        f.runtime.journal.clone(),
        Arc::new(crate::coordination::Coordination::default()),
    )
    .unwrap();
    assert!(runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
    let query: CommitIntent = serde_json::from_slice(
        &runtime
            .input
            .read(Slot::StationCommitQuery)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert!(query == r.intent);
    assert!(runtime.coordination.network().is_err());
    assert!(hint(&f).action == Action::Cancel);
    receipt(&f, CommitPhase::Saved, |_| {});
    assert_eq!(
        runtime
            .commit_status(
                f.auth.clone(),
                f.token.clone(),
                hint(&f).binding.request_id,
                COMMIT.into()
            )
            .await
            .unwrap()["state"],
        "saved"
    );
    assert!(runtime.coordination.network().is_ok());
    no_secrets(&f);
}
#[tokio::test]
async fn unknown_and_legacy_closed_never_prove_cleanup_of_a_confirmation() {
    let f = fixture().await;
    confirm(&f, COMMIT, PASS).await.unwrap();
    receipt(&f, CommitPhase::Unknown, |_| {});
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "unknown");
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    root_status(&f, State::Closed, |_| {});
    f.runtime.tick(f.auth.clone(), &Quiet).await.unwrap();
    assert!(f.runtime.coordination.network().is_err());
    assert!(f.runtime.journal.join("pending.json").exists());
    assert!(confirm(&f, OTHER, PASS).await.is_err());
}
#[tokio::test]
async fn no_save_capability_bad_password_and_stale_parent_have_no_confirmation_intent() {
    let f = fixture().await;
    assert!(confirm(&f, COMMIT, "changed-password").await.is_err());
    assert!(load(&f.runtime.journal, COMMIT).unwrap().is_none());
    f.runtime
        .session
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .save_supported = false;
    assert!(confirm(&f, COMMIT, PASS).await.is_err());
    assert!(load(&f.runtime.journal, COMMIT).unwrap().is_none());
    f.runtime
        .session
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .save_supported = true;
    root_status(&f, State::Ready, |s| s.sample_uptime_ms = 0);
    assert!(confirm(&f, COMMIT, PASS).await.is_err());
    assert!(load(&f.runtime.journal, COMMIT).unwrap().is_none());
}
#[tokio::test]
async fn invalid_channel_is_rejected_before_intent_and_cancel_never_reissues_confirmation() {
    let f = fixture().await;
    let operation = hint(&f).binding.request_id;
    fs::set_permissions(f.dirs[2].path(), fs::Permissions::from_mode(0o750)).unwrap();
    assert!(f
        .runtime
        .commit_begin(
            f.auth.clone(),
            f.token.clone(),
            operation,
            ID.into(),
            COMMIT.into(),
            SSID.into(),
            Zeroizing::new(PASS.into()),
            &Quiet
        )
        .await
        .is_err());
    assert!(pending(&f.runtime.journal).unwrap().is_none());
    assert!(load(&f.runtime.journal, COMMIT).unwrap().is_none());
    no_secrets(&f);
    fs::set_permissions(f.dirs[2].path(), fs::Permissions::from_mode(0o2750)).unwrap();
    assert_eq!(
        confirm(&f, COMMIT, PASS).await.unwrap()["state"],
        "submitted"
    );
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_some());
    let f = fixture().await;
    confirm(&f, COMMIT, PASS).await.unwrap();
    f.runtime
        .station_cancel(
            f.auth.clone(),
            f.token.clone(),
            hint(&f).binding.request_id,
            ID.into(),
        )
        .await
        .unwrap();
    assert!(hint(&f).action == Action::Cancel);
    assert_eq!(confirm(&f, COMMIT, PASS).await.unwrap()["state"], "unknown");
    assert!(f
        .runtime
        .input
        .read(Slot::StationCommitRequest)
        .unwrap()
        .is_none());
    receipt(&f, CommitPhase::Saved, |_| {});
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "saved");
}
#[tokio::test]
async fn writing_completion_proof_must_succeed_before_unlocking_network() {
    let f = fixture().await;
    confirm(&f, COMMIT, PASS).await.unwrap();
    let r = pending(&f.runtime.journal).unwrap().unwrap();
    let path = f
        .runtime
        .journal
        .join(format!("{}.committed", r.trial.parent_key_sha256));
    fs::create_dir(&path).unwrap();
    receipt(&f, CommitPhase::Saved, |_| {});
    assert!(result(&f, COMMIT).await.is_err());
    assert!(f.runtime.coordination.network().is_err());
    assert!(pending(&f.runtime.journal).unwrap().is_some());
    fs::remove_dir(&path).unwrap();
    assert_eq!(result(&f, COMMIT).await.unwrap()["state"], "saved");
    assert!(f.runtime.coordination.network().is_ok());
}
