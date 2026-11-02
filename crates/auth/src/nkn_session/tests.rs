// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use crate::token_digest;
use ed25519_dalek::{Signer, SigningKey};
use rustsimhub_policy::{Kind, Scope};
use rustsimhub_storage::Store;
use std::fs;
use std::os::unix::fs::PermissionsExt;
struct Fixture {
    a: Auth,
    s: Store,
    owner: Token,
    client: SigningKey,
    _dirs: Vec<tempfile::TempDir>,
}
fn fixture() -> Fixture {
    let d = tempfile::tempdir().unwrap();
    let i = tempfile::tempdir().unwrap();
    for p in [d.path(), i.path()] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let s = Store::open(d.path()).unwrap();
    let mut a = Auth::open(i.path(), s.handle()).unwrap();
    let owner = SigningKey::from_bytes(&[71; 32]);
    let c = a
        .bootstrap_challenge(owner.verifying_key().to_bytes())
        .unwrap();
    a.bootstrap_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let c = a.login_challenge(owner.verifying_key().to_bytes()).unwrap();
    let owner = a
        .login_finish(&c.id, &owner.sign(&c.transcript).to_bytes())
        .unwrap();
    let client = SigningKey::from_bytes(&[72; 32]);
    let key = client.verifying_key().to_bytes();
    a.open_pairing(owner.expose()).unwrap();
    let c = a
        .pairing_challenge(key, Kind::Client, &[Scope::StatusRead])
        .unwrap();
    let id = a
        .pairing_finish(&c.id, &client.sign(&c.transcript).to_bytes())
        .unwrap();
    a.approve(owner.expose(), &id, key, Kind::Client, &[Scope::StatusRead])
        .unwrap();
    Fixture {
        a,
        s,
        owner,
        client,
        _dirs: vec![d, i],
    }
}
fn random() -> [u8; 32] {
    let mut n = [0; 32];
    getrandom::fill(&mut n).unwrap();
    n
}
fn key() -> MembershipKey {
    MembershipKey::from_secret([5; 16], Zeroizing::new([7; 32]), 1).unwrap()
}
#[test]
fn fixed_upstream_go_membership_vector_matches_rust_and_rejects_modified_root() {
    let v: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/nkn-membership-upstream.json"
    ))
    .unwrap();
    let decode = |s: &str| -> Vec<u8> {
        s.as_bytes()
            .as_chunks::<2>().0.iter()
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    };
    let root: [u8; 32] = decode(v["root_public_key"].as_str().unwrap())
        .try_into()
        .unwrap();
    let expected: [u8; 32] = decode(v["proof"].as_str().unwrap()).try_into().unwrap();
    let key = key();
    assert_eq!(key.network_id(), v["network"].as_str().unwrap());
    assert_eq!(
        MembershipKey::device_id(&root),
        v["device"].as_str().unwrap()
    );
    assert_eq!(key.proof(&root).unwrap(), expected);
    assert!(key.verify(root, &expected).is_ok());
    let other = SigningKey::from_bytes(&[99; 32]).verifying_key().to_bytes();
    assert!(key.verify(other, &expected).is_err());
}
fn bound_pair(f: &Fixture) -> (NknSession, Cipher, Vec<u8>) {
    let client = f.client.verifying_key().to_bytes();
    let generation = f.a.current_principal(client).unwrap().generation;
    let nonce = random();
    let prepared =
        f.a.prepare_nkn_session(
            random(),
            client,
            generation,
            nonce,
            Instant::now() + Duration::from_secs(120),
        )
        .unwrap();
    let mut server = NknSession::begin(prepared).unwrap();
    let context = Context::new(
        Purpose::NknControl,
        f.a.public_key(),
        client,
        generation,
        nonce,
    )
    .unwrap();
    let mut peer = Cipher::client(context, 0, 120_000).unwrap();
    server
        .read_handshake(&peer.write_handshake(1).unwrap())
        .unwrap();
    peer.read_handshake(&server.write_handshake().unwrap(), 2)
        .unwrap();
    server
        .read_handshake(&peer.write_handshake(3).unwrap())
        .unwrap();
    let work = server.device_binding_work().unwrap();
    let transcript = work.message.clone();
    let signature = f.a.sign_nkn_device_binding(&work).unwrap();
    let wire = server.seal_device_binding(work, &signature).unwrap();
    peer.open_binding(&wire, 4).unwrap();
    let signature = f.client.sign(&peer.binding_message(5).unwrap()).to_bytes();
    server
        .read_client_binding(&peer.seal_binding(&signature, 5).unwrap())
        .unwrap();
    peer.open_binding_ack(&server.write_binding_ack().unwrap(), 6)
        .unwrap();
    assert_eq!(peer.verified_peer_binding_message(6).unwrap(), transcript);
    (server, peer, transcript)
}
fn member_wire(
    peer: &mut Cipher,
    transcript: &[u8],
    key: &MembershipKey,
    root: &SigningKey,
) -> Vec<u8> {
    let public = root.verifying_key().to_bytes();
    let proof = key.proof(&public).unwrap();
    let message =
        NknSession::membership_message(transcript, key.network_id(), &public, &proof).unwrap();
    let mut body = vec![1];
    body.extend_from_slice(&public);
    body.extend_from_slice(&proof);
    body.extend_from_slice(&root.sign(&message).to_bytes());
    peer.seal_request([1; 16], Action::NormalCore, &body, 7)
        .unwrap()
}
fn ready(f: &mut Fixture, key: &MembershipKey) -> (NknSession, Cipher, Token) {
    let (mut server, mut peer, transcript) = bound_pair(f);
    assert!(server.scope().is_err());
    let root = SigningKey::from_bytes(&[73; 32]);
    let wire = member_wire(&mut peer, &transcript, key, &root);
    let admission = server.verify_membership(key, &wire).unwrap();
    let scope = server.scope().unwrap();
    let token = f.a.issue_nkn_token(&scope).unwrap();
    assert!(f.a.issue_nkn_token(&scope).is_err());
    let body=serde_json::to_vec(&serde_json::json!({"contract":1,"auth_protocol":2,"token":token.expose(),"expires_in_seconds":120})).unwrap();
    let reply = server.seal_membership_reply(admission, &body).unwrap();
    let returned: serde_json::Value =
        serde_json::from_slice(&peer.open_response(&reply, 8).unwrap()).unwrap();
    assert_eq!(returned["token"], token.expose());
    (server, peer, token)
}
#[test]
fn actual_membership_and_noise_preserve_normal_principal_scopes_and_original_auth() {
    let mut f = fixture();
    let key = key();
    let (mut server, mut peer, token) = ready(&mut f, &key);
    let scope = server.scope().unwrap();
    assert_eq!(
        f.a.authorize(token.expose(), Scope::StatusRead)
            .unwrap()
            .kind,
        Kind::Client
    );
    assert!(f.a.authorize(token.expose(), Scope::SmsSend).is_err());
    assert!(f.a.recent_owner(token.expose()).is_err());
    f.a.check_nkn_token(&scope, token.expose()).unwrap();
    assert!(f.a.check_nkn_token(&scope, f.owner.expose()).is_err());
    let wire = peer
        .seal_request([2; 16], Action::NormalCore, b"real encrypted request", 9)
        .unwrap();
    let request = server.open_record(&wire).unwrap();
    assert_eq!(request.body.as_slice(), b"real encrypted request");
    let wire = server
        .seal_reply(request.id, b"actual normal response bytes")
        .unwrap();
    assert_eq!(
        peer.open_response(&wire, 10).unwrap().as_slice(),
        b"actual normal response bytes"
    );
}
#[test]
fn cross_connection_bearer_is_rejected_and_renewal_cannot_escape_source_lifetime() {
    let mut f = fixture();
    let key = key();
    let (first, _, token) = ready(&mut f, &key);
    let (second, _, other) = ready(&mut f, &key);
    assert!(f
        .a
        .check_nkn_token(&second.scope().unwrap(), token.expose())
        .is_err());
    assert!(f
        .a
        .check_nkn_token(&first.scope().unwrap(), other.expose())
        .is_err());
    let c = f.a.session_challenge(token.expose()).unwrap();
    let sig = f.client.sign(&c.transcript).to_bytes();
    let next = f.a.renew_session(token.expose(), &c.id, &sig).unwrap();
    drop(first);
    assert!(f.a.authorize(token.expose(), Scope::StatusRead).is_err());
    assert!(f.a.authorize(next.expose(), Scope::StatusRead).is_err());
    assert!(f.a.authorize(other.expose(), Scope::StatusRead).is_ok());
    assert!(f.a.recent_owner(f.owner.expose()).is_ok());
    // The original normal authenticated timestamp is preserved by renewal.
    assert_eq!(
        f.a.sessions[&token_digest(token.expose()).unwrap()].authenticated,
        f.a.sessions[&token_digest(next.expose()).unwrap()].authenticated
    );
}
#[test]
fn real_client_revocation_line_change_and_membership_retirement_fence_normal_tokens() {
    for cause in 0..3 {
        let mut f = fixture();
        let key = key();
        let (server, _, token) = ready(&mut f, &key);
        let scope = server.scope().unwrap();
        match cause {
            0 => {
                f.a.revoke(f.owner.expose(), f.client.verifying_key().to_bytes())
                    .unwrap()
            }
            1 => {
                f.s.handle()
                    .change_line(rustsimhub_domain::LineEpoch(2))
                    .unwrap()
            }
            _ => key.revoke(),
        }
        assert!(f.a.check_nkn_scope(&scope).is_err());
        assert!(f.a.authorize(token.expose(), Scope::StatusRead).is_err());
    }
}
#[test]
fn valid_membership_does_not_admit_unregistered_key_wrong_generation_or_cross_auth() {
    let f = fixture();
    let unknown = SigningKey::from_bytes(&[74; 32]).verifying_key().to_bytes();
    let key = key();
    assert!(key.proof(&unknown).is_ok());
    assert!(f
        .a
        .prepare_nkn_session(
            random(),
            unknown,
            1,
            random(),
            Instant::now() + Duration::from_secs(120)
        )
        .is_err());
    assert!(f
        .a
        .prepare_nkn_session(
            random(),
            f.client.verifying_key().to_bytes(),
            2,
            random(),
            Instant::now() + Duration::from_secs(120)
        )
        .is_err());
    let (mut server, _, _) = bound_pair(&f);
    let work = server.device_binding_work();
    assert!(work.is_err()); // signing cannot be reopened after encrypted ACK
    let mut f = fixture();
    let (server, _, _) = ready(&mut f, &key);
    let foreign = fixture();
    assert!(foreign.a.check_nkn_scope(&server.scope().unwrap()).is_err());
}
#[test]
fn root_proof_must_match_membership_secret_actual_signature_and_exact_noise_transcript() {
    for cause in 0..3 {
        let f = fixture();
        let key = key();
        let (mut server, mut peer, mut transcript) = bound_pair(&f);
        let root = SigningKey::from_bytes(&[73; 32]);
        if cause == 0 {
            transcript[0] ^= 1;
        }
        let wrong = MembershipKey::from_secret([5; 16], Zeroizing::new([8; 32]), 1).unwrap();
        let selected = if cause == 1 { &wrong } else { &key };
        let mut wire = member_wire(&mut peer, &transcript, selected, &root);
        if cause == 2 {
            wire[0] ^= 1;
        }
        assert!(server.verify_membership(&key, &wire).is_err());
        assert!(server.scope().is_err());
    }
}
