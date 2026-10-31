// SPDX-License-Identifier: AGPL-3.0-only
//! Real Auth/Store gates with a synthetic external verifier, not crypto vectors.
use super::*;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_storage::{RecoveryChange, Store};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{mpsc, Mutex},
};
use tempfile::TempDir;

fn dir() -> TempDir {
    let d = tempfile::tempdir().unwrap();
    fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
    d
}
fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
struct Fixture {
    _d: TempDir,
    i: TempDir,
    s: Store,
    a: Auth,
    token: super::super::Token,
    grant: RecoveryGrant,
}
fn fixture() -> Fixture {
    let d = dir();
    let i = dir();
    let b = dir();
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[61; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    drop(a);
    drop(s);
    let s = Store::migrate_recovery_grants(d.path(), b.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let token = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let key = SigningKey::from_bytes(&[62; 32]).verifying_key().to_bytes();
    let grant = a
        .grant_recovery_key(token.expose(), key, 40000, 0, id(1))
        .unwrap();
    Fixture {
        _d: d,
        i,
        s,
        a,
        token,
        grant,
    }
}
struct Adapter<F>(F);
impl<F> RecoveryRequestVerifierV1 for Adapter<F>
where
    F: Fn(&RecoveryProofContext, &[u8]) -> Result<VerifiedRecoveryRequest> + Send + Sync,
{
    fn verify_request(
        &self,
        c: &RecoveryProofContext,
        e: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        (self.0)(c, e)
    }
}
fn synthetic(c: &RecoveryProofContext, e: &[u8]) -> Result<VerifiedRecoveryRequest> {
    VerifiedRecoveryRequest::from_verified_adapter(
        c.binding().clone(),
        vec![*e.get(1).unwrap_or(&8); 16],
        [e[0]; 16],
        RecoveryAction::BeginWifi,
        Zeroizing::new(b"synthetic".to_vec()),
        SESSION_TTL,
    )
}
fn install(a: &mut Auth) -> RecoveryConnection {
    a.set_recovery_verifier(Some(Arc::new(Adapter(synthetic))))
        .unwrap();
    a.open_recovery_connection_from_trusted_transport([7; 32])
        .unwrap()
}
fn authorize(
    a: &mut Auth,
    c: &RecoveryConnection,
    g: &RecoveryGrant,
    e: &[u8],
) -> Result<AuthorizedRecoveryRequest> {
    let work = a.prepare_recovery_verification(c, g.clone(), e)?;
    a.finish_recovery_verification(c, work.verify())
}

#[test]
fn no_default_verifier_no_capability_and_zero_transport_is_rejected() {
    let mut f = fixture();
    assert!(matches!(
        f.a.open_recovery_connection_from_trusted_transport([7; 32]),
        Err(Error::Forbidden)
    ));
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(synthetic))))
        .unwrap();
    assert!(matches!(
        f.a.open_recovery_connection_from_trusted_transport([0; 32]),
        Err(Error::Forbidden)
    ));
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    let before = f.s.handle().principals().unwrap();
    let r = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    f.a.check_recovery_capability(&r.capability, RecoveryAction::BeginWifi, &r.body)
        .unwrap();
    assert!(matches!(
        f.a.check_recovery_capability(&r.capability, RecoveryAction::ReadStatus, &r.body),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        f.a.check_recovery_capability(&r.capability, RecoveryAction::BeginWifi, b"changed"),
        Err(Error::Forbidden)
    ));
    assert_eq!(r.capability.request_id(), &[1; 16]);
    assert_eq!(f.s.handle().principals().unwrap(), before);
    assert!(f.a.login_challenge(f.grant.independent_public_key).is_err());
}

#[test]
fn envelope_and_trusted_adapter_output_budgets_are_enforced() {
    let mut f = fixture();
    let c = install(&mut f.a);
    for data in [vec![], vec![1; 2049]] {
        assert!(matches!(
            f.a.prepare_recovery_verification(&c, f.grant.clone(), &data),
            Err(Error::InvalidInput)
        ));
    }
    let w =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[1; 2048])
            .unwrap();
    for (session, request, size, ttl) in [
        (vec![1; 15], [1; 16], 1, SESSION_TTL),
        (vec![1; 65], [1; 16], 1, SESSION_TTL),
        (vec![0; 16], [1; 16], 1, SESSION_TTL),
        (vec![1; 16], [0; 16], 1, SESSION_TTL),
        (vec![1; 16], [1; 16], 1025, SESSION_TTL),
        (vec![1; 16], [1; 16], 1, Duration::ZERO),
        (
            vec![1; 16],
            [1; 16],
            1,
            SESSION_TTL + Duration::from_secs(1),
        ),
    ] {
        assert!(matches!(
            VerifiedRecoveryRequest::from_verified_adapter(
                w.context.binding.clone(),
                session,
                request,
                RecoveryAction::BeginWifi,
                Zeroizing::new(vec![0; size]),
                ttl
            ),
            Err(Error::InvalidInput)
        ));
    }
    drop(w);
}

#[test]
fn changed_proof_binding_is_rejected_even_from_a_trusted_adapter() {
    let mut f = fixture();
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(
        |c: &RecoveryProofContext, e: &[u8]| {
            let mut r = synthetic(c, e)?;
            r.binding.grant.realm = id(91);
            Ok(r)
        },
    ))))
    .unwrap();
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    assert!(matches!(
        authorize(&mut f.a, &c, &f.grant, &[1]),
        Err(Error::Forbidden)
    ));
    assert!(!f.a.recovery.busy.load(Ordering::Acquire));
}

#[test]
fn revoke_during_external_verification_rejects_late_success_and_active_receipt() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[1])
            .unwrap();
    f.a.revoke_recovery_key(f.token.expose(), f.grant.independent_public_key, 1, id(2))
        .unwrap();
    assert!(matches!(
        f.a.finish_recovery_verification(&c, work.verify()),
        Err(Error::Storage)
    ));
    let historical =
        f.a.recovery_key_request(f.token.expose(), id(1))
            .unwrap()
            .unwrap();
    assert!(historical.active);
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, historical, &[2]),
        Err(Error::Storage)
    ));
    assert!(!f.a.recovery.busy.load(Ordering::Acquire));
}

#[test]
fn revoke_and_regrant_never_revives_an_old_capability_or_session() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let r = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    f.a.revoke_recovery_key(f.token.expose(), f.grant.independent_public_key, 1, id(2))
        .unwrap();
    let next =
        f.a.grant_recovery_key(
            f.token.expose(),
            f.grant.independent_public_key,
            20000,
            2,
            id(3),
        )
        .unwrap();
    assert_eq!(next.generation, 3);
    assert!(matches!(
        f.a.check_recovery_capability(&r.capability, r.capability.action(), &r.body),
        Err(Error::Storage)
    ));
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, next.clone(), &[2]),
        Err(Error::Forbidden)
    ));
    f.a.close_recovery_connection(&c).unwrap();
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    authorize(&mut f.a, &c, &next, &[2]).unwrap();
}

#[test]
fn disconnect_and_verifier_replacement_fence_results_without_resetting_work_occupancy() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[1])
            .unwrap();
    f.a.close_recovery_connection(&c).unwrap();
    assert!(matches!(
        f.a.open_recovery_connection_from_trusted_transport([8; 32]),
        Err(Error::Busy)
    ));
    f.a.set_recovery_verifier(None).unwrap();
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(synthetic))))
        .unwrap();
    assert!(matches!(
        f.a.open_recovery_connection_from_trusted_transport([8; 32]),
        Err(Error::Busy)
    ));
    assert!(matches!(
        f.a.finish_recovery_verification(&c, work.verify()),
        Err(Error::Forbidden)
    ));
    let next =
        f.a.open_recovery_connection_from_trusted_transport([8; 32])
            .unwrap();
    assert_ne!(next.nonce, c.nonce);
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2]),
        Err(Error::Forbidden)
    ));
    authorize(&mut f.a, &next, &f.grant, &[2]).unwrap();
}

#[test]
fn live_capability_is_fenced_by_disconnect_and_verifier_replacement() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let r = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    f.a.close_recovery_connection(&c).unwrap();
    assert!(matches!(
        f.a.check_recovery_capability(&r.capability, r.capability.action(), &r.body),
        Err(Error::Forbidden)
    ));
    let next =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    assert_ne!(next.nonce, c.nonce);
    let r = authorize(&mut f.a, &next, &f.grant, &[2]).unwrap();
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(synthetic))))
        .unwrap();
    assert!(matches!(
        f.a.check_recovery_capability(&r.capability, r.capability.action(), &r.body),
        Err(Error::Forbidden)
    ));
}

#[test]
fn another_auth_instance_cannot_finish_or_use_the_original_request() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let r = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    let mut other = Auth::open(f.i.path(), f.s.handle()).unwrap();
    let _next = install(&mut other);
    assert!(matches!(
        other.check_recovery_capability(&r.capability, r.capability.action(), &r.body),
        Err(Error::Forbidden)
    ));
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2])
            .unwrap();
    assert!(matches!(
        other.finish_recovery_verification(&c, work.verify()),
        Err(Error::Forbidden)
    ));
}

#[test]
fn replay_or_new_session_id_cannot_reset_one_connection() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let _r = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    assert!(matches!(
        authorize(&mut f.a, &c, &f.grant, &[1]),
        Err(Error::Replay)
    ));
    assert!(matches!(
        authorize(&mut f.a, &c, &f.grant, &[2, 9]),
        Err(Error::Forbidden)
    ));
    authorize(&mut f.a, &c, &f.grant, &[2]).unwrap();
}

#[test]
fn expired_capability_session_and_proof_do_not_refresh_their_deadlines() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let mut r = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    r.capability.deadline = Instant::now();
    assert!(matches!(
        f.a.check_recovery_capability(&r.capability, r.capability.action(), &r.body),
        Err(Error::Expired)
    ));
    let original =
        f.a.recovery
            .connection
            .as_ref()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .deadline;
    authorize(&mut f.a, &c, &f.grant, &[2]).unwrap();
    assert_eq!(
        f.a.recovery
            .connection
            .as_ref()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .deadline,
        original
    );
    let mut work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[3])
            .unwrap();
    work.started = Instant::now() - PROOF_TTL;
    let result = work.verify();
    assert!(matches!(result.verified, Err(Error::Expired)));
    assert!(matches!(
        f.a.finish_recovery_verification(&c, result),
        Err(Error::Expired)
    ));
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[3])
            .unwrap();
    let mut result = work.verify();
    result.started = Instant::now() - PROOF_TTL;
    assert!(matches!(
        f.a.finish_recovery_verification(&c, result),
        Err(Error::Expired)
    ));
    f.a.recovery
        .connection
        .as_mut()
        .unwrap()
        .session
        .as_mut()
        .unwrap()
        .deadline = Instant::now();
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[4, 9]),
        Err(Error::Expired)
    ));
}

#[test]
fn request_budget_tombstone_cannot_be_reset_by_a_new_session_id() {
    let mut f = fixture();
    let c = install(&mut f.a);
    for n in 1..=MAX_REQUESTS {
        authorize(&mut f.a, &c, &f.grant, &[n as u8]).unwrap();
    }
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[129, 9]),
        Err(Error::Busy)
    ));
    assert_eq!(
        f.a.recovery
            .connection
            .as_ref()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .requests
            .len(),
        128
    );
}

#[test]
fn outstanding_work_and_unconsumed_result_hold_the_same_single_permit() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[1])
            .unwrap();
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2]),
        Err(Error::Busy)
    ));
    drop(work);
    let result =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2])
            .unwrap()
            .verify();
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[3]),
        Err(Error::Busy)
    ));
    drop(result);
    authorize(&mut f.a, &c, &f.grant, &[3]).unwrap();
}

#[test]
fn failed_proofs_and_abandoned_work_cannot_get_unlimited_verification_attempts() {
    let mut f = fixture();
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(
        |_: &RecoveryProofContext, _: &[u8]| Err(Error::Forbidden),
    ))))
    .unwrap();
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    for n in 1..=MAX_REQUESTS {
        let work =
            f.a.prepare_recovery_verification(&c, f.grant.clone(), &[n as u8])
                .unwrap();
        if n % 2 == 0 {
            drop(work);
        } else {
            assert!(matches!(
                f.a.finish_recovery_verification(&c, work.verify()),
                Err(Error::Forbidden)
            ));
        }
    }
    assert!(f.a.recovery.connection.as_ref().unwrap().session.is_none());
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[129]),
        Err(Error::Busy)
    ));
}

#[test]
fn real_blocking_adapter_leaves_auth_mutex_usable_and_rechecks_revocation_after_return() {
    let mut f = fixture();
    let (started_tx, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(
        move |c: &RecoveryProofContext, e: &[u8]| {
            started_tx.send(()).unwrap();
            wait.lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap();
            synthetic(c, e)
        },
    ))))
    .unwrap();
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    let auth = Mutex::new(f.a);
    let work = auth
        .lock()
        .unwrap()
        .prepare_recovery_verification(&c, f.grant.clone(), &[1])
        .unwrap();
    let worker = std::thread::spawn(move || work.verify());
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    {
        let mut a = auth
            .try_lock()
            .expect("blocking adapter retained the global Auth lock");
        assert!(
            a.recovery_key(f.token.expose(), f.grant.independent_public_key)
                .unwrap()
                .unwrap()
                .active
        );
        assert!(matches!(
            a.prepare_recovery_verification(&c, f.grant.clone(), &[2]),
            Err(Error::Busy)
        ));
        a.revoke_recovery_key(f.token.expose(), f.grant.independent_public_key, 1, id(2))
            .unwrap();
    }
    release.send(()).unwrap();
    let result = worker.join().unwrap();
    assert!(matches!(
        auth.lock()
            .unwrap()
            .finish_recovery_verification(&c, result),
        Err(Error::Storage)
    ));
}

#[test]
fn schema6_and_restored_realm_cannot_use_active_grant_metadata() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let d = dir();
    let i = dir();
    let six = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), six.handle()).unwrap();
    let sc = install(&mut a);
    assert!(matches!(
        a.prepare_recovery_verification(&sc, f.grant.clone(), &[1]),
        Err(Error::Storage)
    ));
    let backup = dir();
    let target = dir();
    f.s.handle().backup(backup.path()).unwrap();
    let restored = Store::restore_recovery_grants(backup.path(), target.path()).unwrap();
    let mut a = Auth::open(f.i.path(), restored.handle()).unwrap();
    let rc = install(&mut a);
    assert!(matches!(
        a.prepare_recovery_verification(&rc, f.grant.clone(), &[1]),
        Err(Error::Storage)
    ));
    authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
}

#[test]
fn verifier_error_drops_work_permit_without_minting_a_session() {
    let mut f = fixture();
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(
        |_: &RecoveryProofContext, _: &[u8]| Err(Error::Forbidden),
    ))))
    .unwrap();
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    assert!(matches!(
        authorize(&mut f.a, &c, &f.grant, &[1]),
        Err(Error::Forbidden)
    ));
    assert!(!f.a.recovery.busy.load(Ordering::Acquire));
    assert!(f.a.recovery.connection.as_ref().unwrap().session.is_none());
    let owner = f.a.recent_owner(f.token.expose()).unwrap();
    f.s.handle()
        .change_recovery_grant(
            owner,
            f.grant.independent_public_key,
            1,
            id(2),
            RecoveryChange::Revoke,
        )
        .unwrap();
}

mod lifecycle;
