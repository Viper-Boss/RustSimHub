// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

#[test]
fn dropping_transport_token_fences_capability_even_while_auth_mutex_is_held() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let request = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    let auth = Mutex::new(f.a);
    let mut locked = auth.lock().unwrap();
    drop(c); // Must never try to acquire this held mutex.
    assert!(matches!(
        locked.check_recovery_capability(
            &request.capability,
            request.capability.action(),
            &request.body
        ),
        Err(Error::Forbidden)
    ));
    let next = locked
        .open_recovery_connection_from_trusted_transport([7; 32])
        .unwrap();
    authorize(&mut locked, &next, &f.grant, &[2]).unwrap();
}

#[test]
fn invalidation_is_monotonic_and_pending_work_must_still_drain() {
    let mut f = fixture();
    let c = install(&mut f.a);
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[1])
            .unwrap();
    assert!(c.is_live());
    c.invalidate();
    c.invalidate();
    assert!(!c.is_live());
    assert!(matches!(
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2]),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        f.a.open_recovery_connection_from_trusted_transport([7; 32]),
        Err(Error::Busy)
    ));
    assert!(matches!(
        f.a.finish_recovery_verification(&c, work.verify()),
        Err(Error::Forbidden)
    ));
    let next =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    assert_ne!(next.nonce, c.nonce);
    authorize(&mut f.a, &next, &f.grant, &[2]).unwrap();
}

#[test]
fn old_token_drop_cannot_invalidate_a_replacement_connection() {
    let mut f = fixture();
    let old = install(&mut f.a);
    f.a.close_recovery_connection(&old).unwrap();
    assert!(!old.is_live());
    let next =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    let request = authorize(&mut f.a, &next, &f.grant, &[1]).unwrap();
    drop(old);
    assert!(next.is_live());
    f.a.check_recovery_capability(
        &request.capability,
        request.capability.action(),
        &request.body,
    )
    .unwrap();
}

#[test]
fn auth_drop_and_configuration_replacement_fence_external_tokens() {
    let mut f = fixture();
    let old = install(&mut f.a);
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(synthetic))))
        .unwrap();
    assert!(!old.is_live());
    let next =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    assert!(next.is_live());
    drop(f.a);
    assert!(!next.is_live());
}

#[test]
fn token_drop_fences_old_capability_while_real_verifier_is_still_running() {
    let mut f = fixture();
    let (started_tx, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    f.a.set_recovery_verifier(Some(Arc::new(Adapter(
        move |ctx: &RecoveryProofContext, e: &[u8]| {
            if e[0] == 2 {
                started_tx.send(()).unwrap();
                wait.lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
            synthetic(ctx, e)
        },
    ))))
    .unwrap();
    let c =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    let prior = authorize(&mut f.a, &c, &f.grant, &[1]).unwrap();
    let work =
        f.a.prepare_recovery_verification(&c, f.grant.clone(), &[2])
            .unwrap();
    let worker = std::thread::spawn(move || work.verify());
    started.recv_timeout(Duration::from_secs(2)).unwrap();
    let auth = Mutex::new(f.a);
    let mut a = auth.lock().unwrap();
    drop(c);
    assert!(matches!(
        a.check_recovery_capability(&prior.capability, prior.capability.action(), &prior.body),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        a.open_recovery_connection_from_trusted_transport([7; 32]),
        Err(Error::Busy)
    ));
    release.send(()).unwrap();
    drop(worker.join().unwrap());
    let next = a
        .open_recovery_connection_from_trusted_transport([7; 32])
        .unwrap();
    assert!(next.is_live());
}
