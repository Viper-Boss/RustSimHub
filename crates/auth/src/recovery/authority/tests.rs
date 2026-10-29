// SPDX-License-Identifier: AGPL-3.0-only
//! Only the external, not-yet-delivered proof adapter is synthetic. Auth/Store
//! bindings, grants, capacity, replay and gate checks are the production code.
use super::*;
use crate::recovery::tests::{fixture, key, limits, TestKeyRegistry};
use std::{
    cell::Cell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Verifier {
    request: u8,
    action: RecoveryAction,
    body: Vec<u8>,
    session: u8,
    duration: Duration,
    calls: Arc<AtomicUsize>,
    // A test can return a cryptographically rejected request or a wrong binding.
    reject: bool,
    wrong: Option<RecoveryProofBinding>,
    during: Option<Box<dyn FnOnce() + Send>>,
}
impl Verifier {
    fn new(request: u8, action: RecoveryAction) -> Self {
        Self {
            request,
            action,
            body: b"synthetic credentials".to_vec(),
            session: 7,
            duration: Duration::from_secs(30),
            calls: Arc::new(AtomicUsize::new(0)),
            reject: false,
            wrong: None,
            during: None,
        }
    }
}
impl RecoveryRequestVerifierV1 for Verifier {
    fn verify_request(
        &mut self,
        context: &RecoveryProofContext,
        envelope: &[u8],
    ) -> Result<VerifiedRecoveryRequest> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(envelope, b"synthetic sealed request");
        assert_eq!(context.binding().grant().key, key());
        assert!(!context.binding().auth_instance().is_empty());
        assert_ne!(*context.binding().device_key(), [0; 32]);
        assert_ne!(*context.binding().connection_nonce(), [0; 32]);
        if let Some(f) = self.during.take() {
            f();
        }
        if self.reject {
            return Err(Error::Forbidden);
        }
        VerifiedRecoveryRequest::from_verified_adapter(
            self.wrong
                .clone()
                .unwrap_or_else(|| context.binding().clone()),
            vec![self.session; 16],
            [self.request; 16],
            self.action,
            Zeroizing::new(self.body.clone()),
            self.duration,
        )
    }
}
const SEALED: &[u8] = b"synthetic sealed request";
#[test]
fn capability_bound_ledger_reservation_claim_and_owner_reconciliation_use_real_store() {
    use rustsimhub_storage::{RecoveryReceiptState, RecoveryReservation};
    let (_dir, _identity, store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::BeginWifi))));
    let connection = auth.open_recovery_connection().unwrap();
    let r = auth
        .authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    assert!(matches!(
        auth.reserve_recovery_mutation(&r.capability, b"changed", None),
        Err(Error::Forbidden)
    ));
    let receipt = match auth
        .reserve_recovery_mutation(&r.capability, &r.body, None)
        .unwrap()
    {
        RecoveryReservation::New(r) => r,
        _ => panic!("new receipt required"),
    };
    assert_eq!(receipt.request_id, "01010101-0101-0101-0101-010101010101");
    let mut forged = receipt.clone();
    forged.body_digest = [0; 32];
    assert!(matches!(
        auth.claim_recovery_mutation(&r.capability, &r.body, forged),
        Err(Error::Forbidden)
    ));
    let permit = auth
        .claim_recovery_mutation(&r.capability, &r.body, receipt.clone())
        .unwrap();
    assert!(auth
        .claim_recovery_mutation(&r.capability, &r.body, receipt.clone())
        .is_err());
    auth.revoke_wifi_recovery(token.expose(), g.id, 1).unwrap();
    let finished = store
        .handle()
        .finish_recovery_request(permit, RecoveryReceiptState::Ready, Some("a".repeat(64)))
        .unwrap();
    assert!(auth
        .reserve_recovery_mutation(&r.capability, &r.body, None)
        .is_err());
    assert_eq!(
        auth.owner_recovery_receipt(token.expose(), receipt)
            .unwrap(),
        finished
    );
}
#[test]
fn read_only_capabilities_cannot_create_mutation_receipts() {
    for action in [
        RecoveryAction::ReadStatus,
        RecoveryAction::QueryOperation,
        RecoveryAction::Disconnect,
    ] {
        let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
        let g = grant(&mut auth, &token);
        auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, action))));
        let connection = auth.open_recovery_connection().unwrap();
        let r = auth.authorize_recovery(&connection, g, SEALED).unwrap();
        assert!(matches!(
            auth.reserve_recovery_mutation(&r.capability, &r.body, None),
            Err(Error::Forbidden)
        ));
    }
}
fn grant(auth: &mut Auth, token: &crate::Token) -> RecoveryGrant {
    auth.grant_wifi_recovery(
        token.expose(),
        key(),
        limits(),
        0,
        Some(&TestKeyRegistry(Cell::new(0))),
    )
    .unwrap()
}

#[test]
fn all_six_real_actions_get_only_a_bound_private_request_capability() {
    for action in [
        RecoveryAction::ReadStatus,
        RecoveryAction::BeginWifi,
        RecoveryAction::QueryOperation,
        RecoveryAction::ConfirmWifi,
        RecoveryAction::CancelWifi,
        RecoveryAction::Disconnect,
    ] {
        let (_dir, _identity, store, mut auth, _owner, token) = fixture(true);
        let before = store.handle().principals().unwrap();
        let grant = grant(&mut auth, &token);
        auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, action))));
        let connection = auth.open_recovery_connection().unwrap();
        let request = auth.authorize_recovery(&connection, grant, SEALED).unwrap();
        assert_eq!(request.capability.action(), action);
        assert_eq!(*request.capability.request_id(), [1; 16]);
        auth.check_recovery_capability(&request.capability, action, &request.body)
            .unwrap();
        assert_eq!(store.handle().principals().unwrap(), before);
        assert!(format!("{:?}", request.capability).contains("(..)"));
    }
}

#[test]
fn missing_backend_default_schema_and_second_connection_are_rejected() {
    for enabled in [false, true] {
        let (_dir, _identity, _store, mut auth, _owner, _token) = fixture(enabled);
        assert!(matches!(
            auth.open_recovery_connection(),
            Err(Error::Forbidden)
        ));
        auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::ReadStatus))));
        if !enabled {
            assert!(matches!(
                auth.open_recovery_connection(),
                Err(Error::Forbidden)
            ));
        } else {
            let connection = auth.open_recovery_connection().unwrap();
            assert!(matches!(auth.open_recovery_connection(), Err(Error::Busy)));
            auth.close_recovery_connection(&connection).unwrap();
            assert!(auth.open_recovery_connection().is_ok());
        }
    }
}

#[test]
fn changed_action_body_or_another_auth_instance_cannot_use_a_capability() {
    let (_dir, identity, store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::BeginWifi))));
    let connection = auth.open_recovery_connection().unwrap();
    let r = auth
        .authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    assert!(matches!(
        auth.check_recovery_capability(&r.capability, RecoveryAction::ConfirmWifi, &r.body),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        auth.check_recovery_capability(&r.capability, RecoveryAction::BeginWifi, b"changed"),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        auth.check_recovery_capability(
            &r.capability,
            RecoveryAction::BeginWifi,
            &vec![1; MAX_BODY + 1]
        ),
        Err(Error::Forbidden)
    ));
    let mut other = Auth::new(
        crate::Identity::load(identity.path()).unwrap(),
        store.handle(),
    )
    .unwrap();
    other.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::BeginWifi))));
    assert!(matches!(
        other.authorize_recovery(&connection, g, SEALED),
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        other.check_recovery_capability(&r.capability, RecoveryAction::BeginWifi, &r.body),
        Err(Error::Forbidden)
    ));
    auth.check_recovery_capability(&r.capability, RecoveryAction::BeginWifi, &r.body)
        .unwrap();
}

#[test]
fn revoked_reapproved_or_changed_durable_binding_never_survives_a_gate() {
    let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    let v = Verifier::new(1, RecoveryAction::ConfirmWifi);
    let calls = v.calls.clone();
    auth.set_recovery_verifier(Some(Box::new(v)));
    let connection = auth.open_recovery_connection().unwrap();
    let r = auth
        .authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    let revoked = auth
        .revoke_wifi_recovery(token.expose(), g.id.clone(), 1)
        .unwrap();
    assert!(auth
        .check_recovery_capability(&r.capability, RecoveryAction::ConfirmWifi, &r.body)
        .is_err());
    assert!(auth.authorize_recovery(&connection, g, SEALED).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let new = auth
        .grant_wifi_recovery(
            token.expose(),
            key(),
            limits(),
            revoked.generation,
            Some(&TestKeyRegistry(Cell::new(0))),
        )
        .unwrap();
    assert!(auth.authorize_recovery(&connection, new, SEALED).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn revocation_during_external_verification_is_checked_again_before_issuing_capability() {
    let (_dir, _identity, store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    let principal = auth.recent_owner(token.expose()).unwrap();
    let h = store.handle();
    let id = g.id.clone();
    let mut v = Verifier::new(1, RecoveryAction::BeginWifi);
    v.during = Some(Box::new(move || {
        h.revoke_recovery(principal, id, 1).unwrap();
    }));
    auth.set_recovery_verifier(Some(Box::new(v)));
    let connection = auth.open_recovery_connection().unwrap();
    assert!(auth.authorize_recovery(&connection, g, SEALED).is_err());
    assert!(auth.recovery.connection.as_ref().unwrap().session.is_none());
}

#[test]
fn replay_or_new_session_id_cannot_reset_one_connection_budget() {
    let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
    let mut budget = limits();
    budget.request_budget = 2;
    let g = auth
        .grant_wifi_recovery(
            token.expose(),
            key(),
            budget,
            0,
            Some(&TestKeyRegistry(Cell::new(0))),
        )
        .unwrap();
    auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::BeginWifi))));
    let connection = auth.open_recovery_connection().unwrap();
    let first = auth
        .authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    assert!(matches!(
        auth.authorize_recovery(&connection, g.clone(), SEALED),
        Err(Error::Replay)
    ));
    // Test-only access to the injected adapter changes its next verified output;
    // do not use set_recovery_verifier, which intentionally retires the connection.
    let mut next = Verifier::new(2, RecoveryAction::QueryOperation);
    next.session = 8;
    auth.recovery.verifier = Some(Box::new(next));
    assert!(matches!(
        auth.authorize_recovery(&connection, g.clone(), SEALED),
        Err(Error::Forbidden)
    ));
    auth.recovery.verifier = Some(Box::new(Verifier::new(2, RecoveryAction::QueryOperation)));
    auth.authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    auth.recovery.verifier = Some(Box::new(Verifier::new(3, RecoveryAction::QueryOperation)));
    assert!(matches!(
        auth.authorize_recovery(&connection, g, SEALED),
        Err(Error::Busy)
    ));
    auth.check_recovery_capability(&first.capability, RecoveryAction::BeginWifi, &first.body)
        .unwrap();
    assert_eq!(
        auth.recovery
            .connection
            .as_ref()
            .unwrap()
            .session
            .as_ref()
            .unwrap()
            .requests
            .len(),
        2
    );
}

#[test]
fn session_deadline_never_slides_and_expired_tombstone_is_not_replaced() {
    let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    let mut v = Verifier::new(1, RecoveryAction::BeginWifi);
    v.duration = Duration::from_secs(1);
    auth.set_recovery_verifier(Some(Box::new(v)));
    let connection = auth.open_recovery_connection().unwrap();
    let first = auth
        .authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    let deadline = first.capability.deadline;
    auth.recovery.verifier = Some(Box::new(Verifier::new(2, RecoveryAction::QueryOperation)));
    let second = auth
        .authorize_recovery(&connection, g.clone(), SEALED)
        .unwrap();
    assert_eq!(second.capability.deadline, deadline);
    auth.recovery
        .connection
        .as_mut()
        .unwrap()
        .session
        .as_mut()
        .unwrap()
        .deadline = Instant::now();
    assert!(matches!(
        auth.authorize_recovery(&connection, g, SEALED),
        Err(Error::Expired)
    ));
    assert!(matches!(
        auth.check_recovery_capability(&first.capability, RecoveryAction::BeginWifi, &first.body),
        Err(Error::Expired)
    ));
    assert!(auth.recovery.connection.as_ref().unwrap().session.is_some());
}

#[test]
fn disconnect_or_verifier_reconfiguration_immediately_retires_old_capabilities() {
    for remove in [false, true] {
        let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
        let g = grant(&mut auth, &token);
        auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::ReadStatus))));
        let connection = auth.open_recovery_connection().unwrap();
        let r = auth
            .authorize_recovery(&connection, g.clone(), SEALED)
            .unwrap();
        if remove {
            auth.set_recovery_verifier(None);
        } else {
            auth.close_recovery_connection(&connection).unwrap();
        }
        assert!(matches!(
            auth.check_recovery_capability(&r.capability, RecoveryAction::ReadStatus, &r.body),
            Err(Error::Forbidden)
        ));
        auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::ReadStatus))));
        let new_connection = auth.open_recovery_connection().unwrap();
        assert_ne!(connection.nonce, new_connection.nonce);
        assert!(matches!(
            auth.authorize_recovery(&connection, g.clone(), SEALED),
            Err(Error::Forbidden)
        ));
        auth.authorize_recovery(&new_connection, g, SEALED).unwrap();
        assert!(matches!(
            auth.close_recovery_connection(&connection),
            Err(Error::Forbidden)
        ));
    }
}

#[test]
fn invalid_or_rejected_proofs_never_create_session_or_consume_request_ids() {
    let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    let mut v = Verifier::new(1, RecoveryAction::ReadStatus);
    v.reject = true;
    let calls = v.calls.clone();
    auth.set_recovery_verifier(Some(Box::new(v)));
    let connection = auth.open_recovery_connection().unwrap();
    for envelope in [vec![], vec![0; MAX_ENVELOPE + 1]] {
        assert!(matches!(
            auth.authorize_recovery(&connection, g.clone(), &envelope),
            Err(Error::InvalidInput)
        ));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(matches!(
        auth.authorize_recovery(&connection, g.clone(), SEALED),
        Err(Error::Forbidden)
    ));
    assert!(auth.recovery.connection.as_ref().unwrap().session.is_none());
    auth.recovery.verifier = Some(Box::new(Verifier::new(1, RecoveryAction::ReadStatus)));
    let r = auth.authorize_recovery(&connection, g, SEALED).unwrap();
    auth.check_recovery_capability(&r.capability, RecoveryAction::ReadStatus, &r.body)
        .unwrap();
}

#[test]
fn proof_output_with_another_binding_or_invalid_shape_is_rejected() {
    let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    auth.set_recovery_verifier(Some(Box::new(Verifier::new(1, RecoveryAction::ReadStatus))));
    let connection = auth.open_recovery_connection().unwrap();
    let binding = RecoveryProofBinding {
        instance: auth.instance.clone(),
        device_key: auth.public_key(),
        connection: connection.nonce,
        grant: g.clone(),
    };
    let mut wrong = binding.clone();
    wrong.connection = [9; 32];
    let mut v = Verifier::new(1, RecoveryAction::ReadStatus);
    v.wrong = Some(wrong);
    auth.recovery.verifier = Some(Box::new(v));
    assert!(matches!(
        auth.authorize_recovery(&connection, g, SEALED),
        Err(Error::Forbidden)
    ));
    assert!(auth.recovery.connection.as_ref().unwrap().session.is_none());
    for (session, request, body, duration) in [
        (vec![7; 15], [1; 16], vec![], Duration::from_secs(1)),
        (vec![7; 65], [1; 16], vec![], Duration::from_secs(1)),
        (vec![0; 16], [1; 16], vec![], Duration::from_secs(1)),
        (vec![7; 16], [0; 16], vec![], Duration::from_secs(1)),
        (
            vec![7; 16],
            [1; 16],
            vec![0; MAX_BODY + 1],
            Duration::from_secs(1),
        ),
        (vec![7; 16], [1; 16], vec![], Duration::ZERO),
        (vec![7; 16], [1; 16], vec![], Duration::from_secs(301)),
    ] {
        assert!(matches!(
            VerifiedRecoveryRequest::from_verified_adapter(
                binding.clone(),
                session,
                request,
                RecoveryAction::ReadStatus,
                Zeroizing::new(body),
                duration
            ),
            Err(Error::InvalidInput)
        ));
    }
}

#[test]
fn slow_proof_cannot_mint_an_already_expired_capability() {
    let (_dir, _identity, _store, mut auth, _owner, token) = fixture(true);
    let g = grant(&mut auth, &token);
    let mut v = Verifier::new(1, RecoveryAction::ReadStatus);
    v.duration = Duration::from_millis(1);
    v.during = Some(Box::new(|| std::thread::sleep(Duration::from_millis(3))));
    auth.set_recovery_verifier(Some(Box::new(v)));
    let connection = auth.open_recovery_connection().unwrap();
    assert!(matches!(
        auth.authorize_recovery(&connection, g, SEALED),
        Err(Error::Expired)
    ));
    assert!(auth.recovery.connection.as_ref().unwrap().session.is_none());
}
