// SPDX-License-Identifier: AGPL-3.0-only
//! Actual random Noise + actual Identity signatures + actual Store/Auth gates.
use super::*;
use crate::secure_session::SecureRecoverySessions;
use rustsimhub_recovery_session::{Action, Context, Purpose, Session as Cipher};

fn connect(f: &mut Fixture) -> (Arc<SecureRecoverySessions>, RecoveryConnection, Cipher) {
    let sessions = Arc::new(SecureRecoverySessions::new());
    f.a.set_recovery_verifier(Some(sessions.clone())).unwrap();
    let connection =
        f.a.open_recovery_connection_from_trusted_transport([7; 32])
            .unwrap();
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).unwrap();
    let grant =
        f.a.secure_recovery_grant(
            &connection,
            f.grant.independent_public_key,
            f.grant.generation,
        )
        .unwrap();
    let preparation =
        f.a.prepare_secure_recovery_handshake(&connection, grant, nonce)
            .unwrap();
    sessions.begin_prepared(&connection, preparation).unwrap();
    let context = Context::new(
        Purpose::BleRecovery,
        f.a.public_key(),
        f.grant.independent_public_key,
        f.grant.generation,
        nonce,
    )
    .unwrap();
    let mut client = Cipher::client(context, 0, 300_000).unwrap();
    sessions
        .read_handshake(&connection, &client.write_handshake(1).unwrap())
        .unwrap();
    client
        .read_handshake(&sessions.write_handshake(&connection).unwrap(), 2)
        .unwrap();
    sessions
        .read_handshake(&connection, &client.write_handshake(3).unwrap())
        .unwrap();
    let work = sessions.device_signing_work(&connection).unwrap();
    let signature =
        f.a.sign_secure_recovery_binding(&connection, &work)
            .unwrap();
    let wire = sessions
        .seal_device_binding(&connection, work, &signature)
        .unwrap();
    client.open_binding(&wire, 4).unwrap();
    let key = SigningKey::from_bytes(&[62; 32]); // Existing fixture's synthetic grant key only.
    let signature = key.sign(&client.binding_message(5).unwrap()).to_bytes();
    sessions
        .read_client_binding(&connection, &client.seal_binding(&signature, 5).unwrap())
        .unwrap();
    client
        .open_binding_ack(&sessions.write_binding_ack(&connection).unwrap(), 5)
        .unwrap();
    f.a.check_secure_recovery_context(&connection, &sessions.binding(&connection).unwrap())
        .unwrap();
    (sessions, connection, client)
}

#[test]
fn real_cipher_request_reaches_same_auth_and_real_scoped_encrypted_reply() {
    let mut f = fixture();
    let (sessions, connection, mut client) = connect(&mut f);
    let wire = client
        .seal_request([9; 16], Action::ReadStatus, b"{}", 6)
        .unwrap();
    let work =
        f.a.prepare_recovery_verification(&connection, f.grant.clone(), &wire)
            .unwrap();
    let request =
        f.a.finish_recovery_verification(&connection, work.verify())
            .unwrap();
    f.a.check_recovery_capability(
        &request.capability,
        RecoveryAction::ReadStatus,
        &request.body,
    )
    .unwrap();
    assert_eq!(&*request.body, b"{}");
    let scope = f.a.prepare_recovery_response(&request).unwrap();
    let sealed = sessions
        .seal_response(&scope, b"synthetic actual authorized reply")
        .unwrap();
    assert_eq!(
        &*client.open_response(&sealed, 7).unwrap(),
        b"synthetic actual authorized reply"
    );
    assert!(f
        .a
        .check_recovery_capability(
            &request.capability,
            RecoveryAction::BeginWifi,
            &request.body
        )
        .is_err());
}

#[test]
fn revoked_between_decrypt_and_auth_finish_cannot_mint_capability() {
    let mut f = fixture();
    let (_, connection, mut client) = connect(&mut f);
    let wire = client
        .seal_request([10; 16], Action::ReadStatus, b"{}", 6)
        .unwrap();
    let work =
        f.a.prepare_recovery_verification(&connection, f.grant.clone(), &wire)
            .unwrap();
    let decrypted = work.verify();
    f.a.revoke_recovery_key(
        f.token.expose(),
        f.grant.independent_public_key,
        f.grant.generation,
        id(2),
    )
    .unwrap();
    assert!(f
        .a
        .finish_recovery_verification(&connection, decrypted)
        .is_err());
}

#[test]
fn revoked_after_dispatch_prevents_scoped_response_encryption() {
    let mut f = fixture();
    let (sessions, connection, mut client) = connect(&mut f);
    let wire = client
        .seal_request([11; 16], Action::ReadStatus, b"{}", 6)
        .unwrap();
    let work =
        f.a.prepare_recovery_verification(&connection, f.grant.clone(), &wire)
            .unwrap();
    let request =
        f.a.finish_recovery_verification(&connection, work.verify())
            .unwrap();
    let scope = f.a.prepare_recovery_response(&request).unwrap();
    f.a.revoke_recovery_key(
        f.token.expose(),
        f.grant.independent_public_key,
        f.grant.generation,
        id(2),
    )
    .unwrap();
    assert!(sessions
        .seal_response(&scope, b"must not seal after revoke")
        .is_err());
}

#[test]
fn old_connection_cipher_and_cross_auth_registry_are_rejected() {
    let mut f = fixture();
    let (sessions, connection, mut client) = connect(&mut f);
    let preparation =
        f.a.prepare_secure_recovery_handshake(&connection, f.grant.clone(), [8; 32])
            .unwrap();
    assert!(sessions.begin_prepared(&connection, preparation).is_err());
    let old = client
        .seal_request([12; 16], Action::ReadStatus, &[], 6)
        .unwrap();
    drop(connection);
    let next =
        f.a.open_recovery_connection_from_trusted_transport([8; 32])
            .unwrap();
    let preparation =
        f.a.prepare_secure_recovery_handshake(&next, f.grant.clone(), [9; 32])
            .unwrap();
    sessions.begin_prepared(&next, preparation).unwrap();
    let work =
        f.a.prepare_recovery_verification(&next, f.grant.clone(), &old)
            .unwrap();
    assert!(f
        .a
        .finish_recovery_verification(&next, work.verify())
        .is_err());
    let mut foreign = fixture();
    foreign
        .a
        .set_recovery_verifier(Some(sessions.clone()))
        .unwrap();
    let token = foreign
        .a
        .open_recovery_connection_from_trusted_transport([10; 32])
        .unwrap();
    let preparation = foreign
        .a
        .prepare_secure_recovery_handshake(&token, foreign.grant.clone(), [11; 32])
        .unwrap();
    assert!(sessions.begin_prepared(&token, preparation).is_err());
}

#[test]
fn tampered_ciphertext_and_disconnect_never_authorize() {
    for disconnected in [false, true] {
        let mut f = fixture();
        let (_, connection, mut client) = connect(&mut f);
        let mut wire = client
            .seal_request([13; 16], Action::ReadStatus, b"{}", 6)
            .unwrap();
        if disconnected {
            connection.invalidate();
        } else {
            wire[0] ^= 1;
        }
        let result =
            f.a.prepare_recovery_verification(&connection, f.grant.clone(), &wire);
        if let Ok(work) = result {
            assert!(f
                .a
                .finish_recovery_verification(&connection, work.verify())
                .is_err());
        }
    }
}
