// SPDX-License-Identifier: AGPL-3.0-only
use super::*;

fn handshake() -> (EvalSession, EvalSession) {
    let mut init = EvalSession::new(true).unwrap();
    let mut resp = EvalSession::new(false).unwrap();
    let first = init
        .write_handshake(b"public synthetic first payload")
        .unwrap();
    assert_eq!(
        resp.read_handshake(&first).unwrap(),
        b"public synthetic first payload"
    );
    let second = resp.write_handshake(b"synthetic second").unwrap();
    assert_eq!(init.read_handshake(&second).unwrap(), b"synthetic second");
    let third = init.write_handshake(b"synthetic third").unwrap();
    assert_eq!(resp.read_handshake(&third).unwrap(), b"synthetic third");
    init.finish().unwrap();
    resp.finish().unwrap();
    assert_eq!(
        init.handshake_hash().unwrap(),
        resp.handshake_hash().unwrap()
    );
    (init, resp)
}
#[test]
fn upstream_known_answer_matches_all_five_messages() {
    assert_eq!(verify_public_vectors().unwrap(), 5);
}
#[test]
fn real_handshake_and_independent_directions_exchange_plaintext() {
    let (mut init, mut resp) = handshake();
    for size in [0, 1, MAX_BODY] {
        let body = vec![37; size];
        let wire = init.write_transport(&body).unwrap();
        assert_eq!(wire.len(), body.len() + 16);
        assert_eq!(resp.read_transport(&wire).unwrap(), body);
        let other = resp.write_transport(b"reply").unwrap();
        assert_eq!(init.read_transport(&other).unwrap(), b"reply");
    }
}
#[test]
fn fresh_key_generation_changes_handshake_and_transcript() {
    let (a, _) = handshake();
    let (b, _) = handshake();
    assert_ne!(a.handshake_hash().unwrap(), b.handshake_hash().unwrap());
    let mut a = EvalSession::new(true).unwrap();
    let mut b = EvalSession::new(true).unwrap();
    assert_ne!(
        a.write_handshake(&[]).unwrap(),
        b.write_handshake(&[]).unwrap()
    );
}
#[test]
fn first_xx_payload_is_public_and_must_not_carry_credentials() {
    let mut init = EvalSession::new(true).unwrap();
    let wire = init.write_handshake(b"public probe").unwrap();
    assert!(wire.ends_with(b"public probe"));
}
#[test]
fn unfinished_handshake_cannot_enter_transport_and_is_closed() {
    let mut init = EvalSession::new(true).unwrap();
    assert!(init.handshake_hash().is_err());
    assert!(init.finish().is_err());
    assert!(init.closed());
    assert!(init.write_handshake(&[]).is_err());
    let mut init = EvalSession::new(true).unwrap();
    assert!(init.write_transport(b"forbidden early data").is_err());
    assert!(init.closed());
}
#[test]
fn encrypted_handshake_tamper_fails_and_retires() {
    let mut init = EvalSession::new(true).unwrap();
    let mut resp = EvalSession::new(false).unwrap();
    resp.read_handshake(&init.write_handshake(&[]).unwrap())
        .unwrap();
    let mut second = resp.write_handshake(&[]).unwrap();
    second[40] ^= 1;
    assert!(init.read_handshake(&second).is_err());
    assert!(init.closed());
}
#[test]
fn transport_tamper_truncation_extra_byte_and_direction_swap_close() {
    for case in 0..4 {
        let (mut init, mut resp) = handshake();
        let mut wire = init.write_transport(b"synthetic secret").unwrap();
        match case {
            0 => wire[0] ^= 1,
            1 => {
                wire.pop();
            }
            2 => wire.push(0),
            _ => {
                assert!(init.read_transport(&wire).is_err());
                assert!(init.closed());
                continue;
            }
        }
        assert!(resp.read_transport(&wire).is_err());
        assert!(resp.closed());
    }
}
#[test]
fn replay_and_out_of_order_ciphertexts_fail_closed() {
    let (mut init, mut resp) = handshake();
    let first = init.write_transport(b"first").unwrap();
    resp.read_transport(&first).unwrap();
    assert!(resp.read_transport(&first).is_err());
    assert!(resp.closed());
    let (mut init, mut resp) = handshake();
    init.write_transport(b"first").unwrap();
    let second = init.write_transport(b"second").unwrap();
    assert!(resp.read_transport(&second).is_err());
    assert!(resp.closed());
}
#[test]
fn ciphertext_from_other_session_is_rejected() {
    let (mut init, _) = handshake();
    let (_, mut resp) = handshake();
    assert!(resp
        .read_transport(&init.write_transport(b"old context").unwrap())
        .is_err());
    assert!(resp.closed());
}
#[test]
fn input_and_output_limits_close_without_partial_success() {
    let mut init = EvalSession::new(true).unwrap();
    assert!(init.write_handshake(&vec![0; MAX_BODY + 1]).is_err());
    assert!(init.closed());
    for wire in [Vec::new(), vec![0; MAX_WIRE + 1]] {
        let mut resp = EvalSession::new(false).unwrap();
        assert!(resp.read_handshake(&wire).is_err());
        assert!(resp.closed());
    }
    let (mut init, mut resp) = handshake();
    assert!(init.write_transport(&vec![0; MAX_BODY + 1]).is_err());
    assert!(init.closed());
    assert!(resp.read_transport(&[0; 15]).is_err());
    assert!(resp.closed());
}
#[test]
fn explicit_close_erases_local_access_without_claiming_memory_wiping() {
    let (mut init, mut resp) = handshake();
    init.close();
    init.close();
    assert!(init.closed());
    assert!(init.handshake_hash().is_err());
    assert!(init
        .read_transport(&resp.write_transport(b"late").unwrap())
        .is_err());
    assert!(init.write_transport(b"late").is_err());
    assert!(init.finish().is_err());
}
