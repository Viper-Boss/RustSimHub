// SPDX-License-Identifier: AGPL-3.0-only
//! A2 ordinary Auth boundaries and explicit real-Root/SDK-only lifecycle checks.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_policy::{Kind, ALL_SCOPES};
use rustsimhub_wire::NknOwner;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};

struct Fixture {
    _dirs: [tempfile::TempDir; 2],
    _store: rustsimhub_storage::Store,
    auth: Arc<Mutex<Auth>>,
    owner: String,
    client: String,
    nas: String,
}
fn login(auth: &mut Auth, key: &SigningKey) -> String {
    let c = auth
        .login_challenge(key.verifying_key().to_bytes())
        .unwrap();
    auth.login_finish(&c.id, &key.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
impl Fixture {
    fn new() -> Self {
        let state = tempfile::tempdir().unwrap();
        let identity = tempfile::tempdir().unwrap();
        for d in [&state, &identity] {
            fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = rustsimhub_storage::Store::open(state.path()).unwrap();
        let mut auth = Auth::open(identity.path(), store.handle()).unwrap();
        let owner = SigningKey::from_bytes(&[71; 32]);
        let c = auth
            .bootstrap_challenge(owner.verifying_key().to_bytes())
            .unwrap();
        let principal = auth
            .bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
            .unwrap();
        let owner = login(&mut auth, &owner);
        let mut delegated = Vec::new();
        for (seed, kind) in [(72, Kind::Client), (74, Kind::Nas)] {
            let key = SigningKey::from_bytes(&[seed; 32]);
            store
                .handle()
                .approve_client(
                    principal.clone(),
                    key.verifying_key().to_bytes(),
                    kind,
                    ALL_SCOPES,
                )
                .unwrap();
            delegated.push(login(&mut auth, &key));
        }
        Self {
            _dirs: [state, identity],
            _store: store,
            auth: Arc::new(Mutex::new(auth)),
            owner,
            client: delegated.remove(0),
            nas: delegated.remove(0),
        }
    }
}
fn change(
    token: &str,
    instance: &str,
    owner: NknOwner,
    enabled: bool,
    revision: u64,
) -> AuthCommand {
    AuthCommand::SetNknEnabled {
        token: token.into(),
        owner,
        enabled,
        expected_revision: revision.to_string(),
        expected_core_instance: instance.into(),
        request_id: format!("{revision:08x}-1111-4111-8111-111111111111"),
    }
}
#[tokio::test]
async fn all_scope_client_and_nas_tokens_cannot_read_control_or_mint_root() {
    let f = Fixture::new();
    let c = Controller::new(f.auth.clone());
    let instance = f.auth.lock().unwrap().observer_instance();
    for token in [&f.client, &f.nas] {
        assert!(f
            .auth
            .lock()
            .unwrap()
            .authorize(token, rustsimhub_policy::Scope::SystemManage)
            .is_ok());
        assert!(f.auth.lock().unwrap().nkn_root_authority(token).is_err());
        assert!(c
            .request(
                f.auth.clone(),
                AuthCommand::NknStatus {
                    token: token.clone()
                }
            )
            .await
            .is_err());
        for owner in [
            NknOwner::ManualEngine,
            NknOwner::Remote,
            NknOwner::Statistics,
        ] {
            for enabled in [false, true] {
                assert!(c
                    .request(f.auth.clone(), change(token, &instance, owner, enabled, 0))
                    .await
                    .is_err());
            }
        }
        assert!(c
            .request(
                f.auth.clone(),
                AuthCommand::NknRequest {
                    token: token.clone(),
                    request_id: "00000000-1111-4111-8111-111111111111".into(),
                    expected_core_instance: instance.clone()
                }
            )
            .await
            .is_err());
    }
    assert!(c.root().snapshot().unwrap().is_none());
    let status = c
        .request(
            f.auth.clone(),
            AuthCommand::NknStatus {
                token: f.owner.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(status["revision"], "0");
    assert_eq!(
        status["provider"]["owners"],
        serde_json::json!({"manual":false,"remote":false,"statistics":false})
    );
    assert!(c.root().snapshot().unwrap().is_none());
}

async fn wait(path: &str, present: bool, seconds: u64) {
    let end = Instant::now() + Duration::from_secs(seconds);
    while std::path::Path::new(path).exists() != present {
        assert!(
            Instant::now() < end,
            "A2 expected path present={present}: {path}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn snapshot(c: &Controller) -> rustsimhub_nkn_agent::Snapshot {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        match c.root().snapshot() {
            Ok(Some(value)) => return value,
            Err(rustsimhub_nkn_agent::PortError::Unavailable) if Instant::now() < end => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            other => panic!("A2 original supervisor snapshot: {other:?}"),
        }
    }
}
#[tokio::test]
#[ignore = "explicit disposable private namespace/cgroup with actual fixed SDK only"]
async fn kernel_remote() {
    assert_eq!(std::env::var("RSH_A2_KERNEL").unwrap(), "1");
    assert_eq!(rustix::process::geteuid().as_raw(), 1001);
    let f = Fixture::new();
    let c = Controller::new(f.auth.clone());
    let instance = f.auth.lock().unwrap().observer_instance();
    let pin = f.auth.lock().unwrap().public_key();
    fs::write(
        "/tmp/a2-public.json",
        serde_json::to_vec(&serde_json::json!({"pin":pin})).unwrap(),
    )
    .unwrap();
    wait("/tmp/a2-root-ready", true, 10).await;
    let before = c
        .request(
            f.auth.clone(),
            AuthCommand::NknStatus {
                token: f.owner.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(before["revision"], "0");
    assert!(c.root().snapshot().unwrap().is_none());
    for (revision, owner) in [
        NknOwner::ManualEngine,
        NknOwner::Statistics,
        NknOwner::Remote,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            c.request(
                f.auth.clone(),
                change(&f.owner, &instance, owner, true, revision as u64)
            )
            .await
            .unwrap()["state"],
            "accepted"
        );
    }
    let driver = tokio::spawn(run(
        Some(c.clone()),
        f.auth.clone(),
        None,
        None,
        None,
        crate::authenticated::Providers {
            nkn: Some(c.clone()),
            ..Default::default()
        },
    ));
    wait("/run/rustsimhub/nkn/core.sock", true, 10).await;
    let original_epoch = snapshot(&c).await.epoch;
    let status = c
        .request(
            f.auth.clone(),
            AuthCommand::NknStatus {
                token: f.owner.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(status["provider"]["status"]["service"], "running");
    for field in ["rtt_ms", "peer_count", "rx_bytes", "tx_bytes"] {
        assert!(status["provider"]["status"][field].is_null());
    }
    fs::write("/tmp/a2-status.json", serde_json::to_vec(&status).unwrap()).unwrap();
    fs::write("/tmp/a2-break-membership", b"1").unwrap();
    wait("/run/rustsimhub/nkn/core.sock", false, 3).await;
    fs::write("/tmp/a2-restore-membership", b"1").unwrap();
    wait("/tmp/a2-membership-restored", true, 3).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(
        !std::path::Path::new("/run/rustsimhub/nkn/core.sock").exists(),
        "credential failure reopened without explicit OFF/ON"
    );
    // Original synchronous action handler on this current-thread executor:
    // no driver poll can observe the intermediate OFF between these calls.
    assert_eq!(
        c.inner
            .request(change(&f.owner, &instance, NknOwner::Remote, false, 3))
            .unwrap()["state"],
        "accepted"
    );
    assert_eq!(
        c.inner
            .request(change(&f.owner, &instance, NknOwner::Remote, true, 4))
            .unwrap()["state"],
        "accepted"
    );
    wait("/run/rustsimhub/nkn/core.sock", true, 3).await;
    assert_eq!(
        snapshot(&c).await.epoch,
        original_epoch,
        "explicit reference replaced original Root scope"
    );
    fs::write("/tmp/a2-peer-ready", b"1").unwrap();
    let end = Instant::now() + Duration::from_secs(45);
    while !std::path::Path::new("/tmp/a2-peers-done").exists() {
        assert!(Instant::now() < end, "A2 peers did not finish");
        if std::path::Path::new("/tmp/a2-revoke-request").exists()
            && !std::path::Path::new("/tmp/a2-client-revoked").exists()
        {
            f.auth
                .lock()
                .unwrap()
                .revoke(
                    &f.owner,
                    SigningKey::from_bytes(&[72; 32]).verifying_key().to_bytes(),
                )
                .unwrap();
            fs::write("/tmp/a2-client-revoked", b"1").unwrap();
        }
        if !crate::nkn_listener::source_busy() {
            if let Ok(sequence) = fs::read_to_string("/tmp/a2-drain-request") {
                fs::write("/tmp/a2-drain-ack", sequence).unwrap();
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for (revision, owner) in [
        (5, NknOwner::Remote),
        (6, NknOwner::Statistics),
        (7, NknOwner::ManualEngine),
    ] {
        assert_eq!(
            c.request(
                f.auth.clone(),
                change(&f.owner, &instance, owner, false, revision)
            )
            .await
            .unwrap()["state"],
            "accepted"
        );
        if revision == 5 {
            assert_eq!(snapshot(&c).await.references, 2);
        }
    }
    let end = Instant::now() + Duration::from_secs(8);
    while snapshot(&c).await.phase != rustsimhub_nkn_agent::Phase::Disabled {
        assert!(Instant::now() < end);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    driver.abort();
    let _ = driver.await;
    fs::write("/tmp/a2-core-result.json",serde_json::to_vec(&serde_json::json!({"passive_off":true,"unknown_fields_preserved":true,"membership_failure_no_auto_reopen":true,"rapid_explicit_reopen":true,"original_root_epoch_preserved":true,"original_scope_disabled":true})).unwrap()).unwrap();
}
