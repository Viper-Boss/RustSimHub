use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn signing_key() -> SigningKey {
    let mut seed = Zeroizing::new([0; 32]);
    getrandom::fill(seed.as_mut()).expect("synthetic test entropy");
    SigningKey::from_bytes(&seed)
}
fn contexts(purpose: Purpose) -> (Context, CoreBinding, SigningKey, SigningKey) {
    let device = signing_key();
    let client = signing_key();
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).unwrap();
    let context = Context::new(
        purpose,
        device.verifying_key().to_bytes(),
        client.verifying_key().to_bytes(),
        7,
        nonce,
    )
    .unwrap();
    let binding = CoreBinding::new([1; 32], [2; 32], [3; 32]).unwrap();
    (context, binding, device, client)
}
fn handshake(client: &mut Session, device: &mut Session) {
    let one = client.write_handshake(1).unwrap();
    assert_eq!(one.len(), 32); // No application bytes in the plaintext XX first message.
    device.read_handshake(&one, 2).unwrap();
    let two = device.write_handshake(3).unwrap();
    client.read_handshake(&two, 4).unwrap();
    let three = client.write_handshake(5).unwrap();
    device.read_handshake(&three, 6).unwrap();
    assert!(!client.is_bound() && !device.is_bound());
}
fn pair(purpose: Purpose) -> (Session, Session) {
    let (context, binding, device_key, client_key) = contexts(purpose);
    let mut client = Session::client(context.clone(), 0, 100_000).unwrap();
    let mut device = Session::device(context, binding, 0, 100_000).unwrap();
    handshake(&mut client, &mut device);
    let sig = device_key
        .sign(&device.binding_message(7).unwrap())
        .to_bytes();
    let wire = device.seal_binding(&sig, 7).unwrap();
    client.open_binding(&wire, 7).unwrap();
    let sig = client_key
        .sign(&client.binding_message(8).unwrap())
        .to_bytes();
    let wire = client.seal_binding(&sig, 8).unwrap();
    device.open_binding(&wire, 8).unwrap();
    assert!(client.is_bound() && device.is_bound());
    assert_eq!(client.session_id().unwrap(), device.session_id().unwrap());
    (client, device)
}
#[test]
fn real_handshake_signatures_and_both_application_purposes() {
    for (purpose, action) in [
        (Purpose::BleRecovery, Action::BeginWifi),
        (Purpose::NknControl, Action::NormalCore),
    ] {
        let (mut client, mut device) = pair(purpose);
        let request = client
            .seal_request([1; 16], action, "合成数据".as_bytes(), 10)
            .unwrap();
        let opened = device.open_request(&request, 10).unwrap();
        assert_eq!(opened.action, action);
        assert_eq!(&*opened.body, "合成数据".as_bytes());
        let response = device
            .seal_response(opened.id, b"synthetic accepted", 11)
            .unwrap();
        assert_eq!(
            &*client.open_response(&response, 11).unwrap(),
            b"synthetic accepted"
        );
    }
}
#[test]
fn early_application_and_early_signing_fail_closed() {
    let (context, binding, _, _) = contexts(Purpose::BleRecovery);
    let mut client = Session::client(context.clone(), 0, 1000).unwrap();
    assert!(client
        .seal_request([1; 16], Action::ReadStatus, &[], 0)
        .is_err());
    assert!(client.is_closed());
    let mut device = Session::device(context, binding, 0, 1000).unwrap();
    assert!(device.binding_message(0).is_err());
    assert!(device.is_closed());
}
#[test]
fn wrong_pin_peer_generation_nonce_or_purpose_cannot_share_transcript() {
    for variation in 0..5 {
        let (context, binding, _, _) = contexts(Purpose::BleRecovery);
        let mut wrong = context.clone();
        match variation {
            0 => wrong.device = signing_key().verifying_key().to_bytes(),
            1 => wrong.client = signing_key().verifying_key().to_bytes(),
            2 => wrong.generation += 1,
            3 => wrong.nonce[0] ^= 1,
            _ => wrong.purpose = Purpose::NknControl,
        }
        let mut client = Session::client(wrong, 0, 1000).unwrap();
        let mut device = Session::device(context, binding, 0, 1000).unwrap();
        let one = client.write_handshake(1).unwrap();
        device.read_handshake(&one, 2).unwrap();
        let two = device.write_handshake(3).unwrap();
        assert!(client.read_handshake(&two, 4).is_err());
        assert!(client.is_closed());
    }
}
#[test]
fn real_signature_rejects_wrong_signer_role_and_modified_core_binding() {
    for variation in 0..3 {
        let (context, binding, device_key, client_key) = contexts(Purpose::BleRecovery);
        let mut client = Session::client(context.clone(), 0, 1000).unwrap();
        let mut device = Session::device(context, binding, 0, 1000).unwrap();
        handshake(&mut client, &mut device);
        let mut message = device.binding_message(7).unwrap();
        let signature = match variation {
            0 => client_key.sign(&message),
            1 => {
                message[31] ^= 1;
                device_key.sign(&message)
            }
            _ => {
                let n = message.len();
                message[n - 33] ^= 1;
                device_key.sign(&message)
            }
        }
        .to_bytes();
        assert!(device.seal_binding(&signature, 7).is_err());
        assert!(device.is_closed());
    }
}
#[test]
fn request_tamper_truncate_append_reflect_and_foreign_session_close() {
    for variation in 0..5 {
        let (mut client, mut device) = pair(Purpose::BleRecovery);
        let mut wire = client
            .seal_request([2; 16], Action::ReadStatus, b"synthetic", 10)
            .unwrap();
        match variation {
            0 => wire[0] ^= 1,
            1 => {
                wire.pop();
            }
            2 => wire.push(0),
            3 => {
                assert!(client.open_response(&wire, 10).is_err());
                continue;
            }
            _ => {
                let (_, foreign) = pair(Purpose::BleRecovery);
                device = foreign;
            }
        }
        assert!(device.open_request(&wire, 10).is_err());
        assert!(device.is_closed());
    }
}
#[test]
fn one_inflight_request_exact_response_and_duplicate_ids() {
    let (mut client, mut device) = pair(Purpose::BleRecovery);
    let wire = client
        .seal_request([2; 16], Action::ReadStatus, &[], 10)
        .unwrap();
    let request = device.open_request(&wire, 10).unwrap();
    let response = device.seal_response(request.id, &[], 11).unwrap();
    client.open_response(&response, 11).unwrap();
    assert_eq!(
        client
            .seal_request([2; 16], Action::ReadStatus, &[], 12)
            .err(),
        Some(Error::Replay)
    );
    let (mut client, _) = pair(Purpose::BleRecovery);
    client
        .seal_request([1; 16], Action::ReadStatus, &[], 10)
        .unwrap();
    assert_eq!(
        client
            .seal_request([3; 16], Action::ReadStatus, &[], 11)
            .err(),
        Some(Error::State)
    );
}
#[test]
fn replay_reorder_and_wrong_response_ids_are_rejected() {
    let (mut client, mut device) = pair(Purpose::BleRecovery);
    let first = client
        .seal_request([1; 16], Action::ReadStatus, &[], 10)
        .unwrap();
    device.open_request(&first, 10).unwrap();
    let reply = device.seal_response([1; 16], &[], 11).unwrap();
    client.open_response(&reply, 11).unwrap();
    assert!(device.open_request(&first, 12).is_err());
    let (mut client, mut device) = pair(Purpose::BleRecovery);
    let one = client
        .seal_request([1; 16], Action::ReadStatus, &[], 10)
        .unwrap();
    device.open_request(&one, 10).unwrap();
    assert!(device.seal_response([2; 16], &[], 11).is_err());
}
#[test]
fn recovery_and_normal_core_actions_never_cross_purpose() {
    for (purpose, wrong) in [
        (Purpose::BleRecovery, Action::NormalCore),
        (Purpose::NknControl, Action::BeginWifi),
    ] {
        let (mut client, _) = pair(purpose);
        assert!(client.seal_request([1; 16], wrong, &[], 10).is_err());
        assert!(client.is_closed());
    }
}
#[test]
fn original_deadlines_clock_rollback_and_close_are_terminal() {
    let (context, binding, _, _) = contexts(Purpose::BleRecovery);
    let mut device = Session::device(context.clone(), binding, 0, 100_000).unwrap();
    assert_eq!(device.write_handshake(10_000).err(), Some(Error::Expired));
    let mut client = Session::client(context, 100, 10_000).unwrap();
    assert_eq!(client.write_handshake(99).err(), Some(Error::Clock));
    let (mut client, _) = pair(Purpose::BleRecovery);
    assert_eq!(
        client
            .seal_request([1; 16], Action::ReadStatus, &[], 100_000)
            .err(),
        Some(Error::Expired)
    );
    let (mut client, _) = pair(Purpose::BleRecovery);
    client.close();
    assert!(client
        .seal_request([1; 16], Action::ReadStatus, &[], 10)
        .is_err());
}
#[test]
fn wire_body_and_request_quota_are_real_bounds() {
    let (mut client, mut device) = pair(Purpose::BleRecovery);
    for index in 1..=MAX_REQUESTS {
        let mut id = [0; 16];
        id[..8].copy_from_slice(&(index as u64).to_be_bytes());
        let wire = client
            .seal_request(id, Action::ReadStatus, &[7; MAX_BODY], 10)
            .unwrap();
        assert_eq!(wire.len(), 1103);
        let request = device.open_request(&wire, 10).unwrap();
        let response = device
            .seal_response(request.id, &[8; MAX_BODY], 10)
            .unwrap();
        assert_eq!(response.len(), 1102);
        client.open_response(&response, 10).unwrap();
    }
    assert_eq!(
        client
            .seal_request([255; 16], Action::ReadStatus, &[], 10)
            .err(),
        Some(Error::Limit)
    );
    let (mut client, _) = pair(Purpose::BleRecovery);
    assert!(client
        .seal_request([1; 16], Action::ReadStatus, &[0; MAX_BODY + 1], 10)
        .is_err());
}
#[test]
fn plaintext_in_noise_first_message_is_rejected() {
    let (context, binding, _, _) = contexts(Purpose::BleRecovery);
    let mut client = Session::client(context.clone(), 0, 1000).unwrap();
    let mut device = Session::device(context, binding, 0, 1000).unwrap();
    let mut one = client.write_handshake(1).unwrap();
    one.extend_from_slice(b"synthetic must not become handshake application data");
    assert_eq!(device.read_handshake(&one, 2).err(), Some(Error::Record));
    assert!(device.is_closed());
}
