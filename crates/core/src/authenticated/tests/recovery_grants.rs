// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn owner(a: &mut Auth) -> String {
    let k = SigningKey::from_bytes(&[61; 32]);
    let c = a.bootstrap_challenge(k.verifying_key().to_bytes()).unwrap();
    a.bootstrap_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap();
    let c = a.login_challenge(k.verifying_key().to_bytes()).unwrap();
    a.login_finish(&c.id, &k.sign(&c.transcript).to_bytes())
        .unwrap()
        .expose()
        .to_owned()
}
#[tokio::test]
async fn default_schema6_core_rejects_grant_administration_without_migration() {
    let d = directory();
    let i = directory();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let token = owner(&mut a);
    let public_key = SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes();
    let a = Arc::new(Mutex::new(a));
    let result = round_trip(
        AuthCommand::RecoveryGrant {
            token,
            public_key,
            max_trial_ms: 40000,
            expected_generation: "0".into(),
            request_id: id(1),
        },
        a,
    )
    .await;
    assert!(result.get("error").is_some());
    assert_eq!(s.handle().principals().unwrap().len(), 1);
}
#[cfg(feature = "recovery-grants")]
#[tokio::test]
async fn exact_owner_grant_query_and_revocation_use_real_core_ipc_and_durable_store() {
    let d = directory();
    let i = directory();
    let b = directory();
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let token = owner(&mut a);
    let public_key = SigningKey::from_bytes(&[63; 32]).verifying_key().to_bytes();
    let a = Arc::new(Mutex::new(a));
    let command = || AuthCommand::RecoveryGrant {
        token: token.clone(),
        public_key,
        max_trial_ms: 40000,
        expected_generation: "0".into(),
        request_id: id(1),
    };
    let granted = round_trip(command(), a.clone()).await;
    assert_eq!(granted["active"], true);
    assert_eq!(granted["generation"], "1");
    assert_eq!(round_trip(command(), a.clone()).await, granted);
    let query = round_trip(
        AuthCommand::RecoveryRequest {
            token: token.clone(),
            request_id: id(1),
        },
        a.clone(),
    )
    .await;
    assert_eq!(query, granted);
    let denied = round_trip(
        AuthCommand::RecoveryKey {
            token: "0".repeat(64),
            public_key,
        },
        a.clone(),
    )
    .await;
    assert!(denied.get("error").is_some());
    let revoked = round_trip(
        AuthCommand::RecoveryRevoke {
            token: token.clone(),
            public_key,
            expected_generation: "1".into(),
            request_id: id(2),
        },
        a.clone(),
    )
    .await;
    assert_eq!(revoked["active"], false);
    assert_eq!(revoked["generation"], "2");
    let read = round_trip(AuthCommand::RecoveryKey { token, public_key }, a.clone()).await;
    assert_eq!(read, revoked);
    assert!(a.lock().unwrap().login_challenge(public_key).is_err());
    assert_eq!(s.handle().principals().unwrap().len(), 1);
}
