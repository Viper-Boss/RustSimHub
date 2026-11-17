// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[derive(Default)]
struct Memory(Option<Credential>);

impl Store for Memory {
    fn credential(&self) -> Option<&Credential> {
        self.0.as_ref()
    }
    fn save(&mut self, credential: Credential) -> Result<(), Error> {
        self.0 = Some(credential);
        Ok(())
    }
}

fn request(
    pairing: &mut Pairing,
    store: &mut Memory,
    now: Instant,
    value: Value,
) -> Result<Value, Error> {
    pairing.request(value.to_string().as_bytes(), now, store)
}

fn challenge(now: Instant) -> Challenge {
    Challenge {
        id: "challenge".into(),
        secret: StaticSecret::from(std::array::from_fn(|i| (i + 1) as u8)),
        nonce: [3; 32],
        deadline: now + PAIRING_WINDOW,
        prepared: None,
    }
}

fn phone() -> Phone {
    let private = StaticSecret::from(std::array::from_fn(|i| (i + 33) as u8));
    Phone {
        public: PublicKey::from(&private).to_bytes(),
        nonce: [4; 32],
    }
}

fn fields(op: &str, challenge: &str, phone: &Phone) -> Value {
    json!({"op": op, "challenge_id": challenge, "iphone_public": base64url(&phone.public),
           "iphone_nonce": base64url(&phone.nonce)})
}

#[test]
fn v1_transcript_openssl_vector() {
    let now = Instant::now();
    let challenge = challenge(now);
    let phone = phone();
    let identity = "rustsimhub-vector";
    let (key, code) = derive(identity, &challenge, &phone).unwrap();
    // X25519 from OpenSSL pkeyutl; transcript/HKDF/HMAC from Python hashlib/hmac.
    assert_eq!(
        hex(&*key),
        "263701485e574f433c3ca41a868c37a184175aac1657c41e62ba2e21ebc58dcf"
    );
    assert_eq!(code, "154601");
    let ready = mac(&key, b"AirSIM/VoBLE/v1/ready", b"challenge")
        .finalize()
        .into_bytes();
    assert_eq!(hex(&ready[..16]), "f68c75582de19e1c0ffc6ac8f6bd4791");
    let transcript = transcript(identity, &challenge, &phone).unwrap();
    let reconnect = mac(&[9; 32], b"AirSIM/VoBLE/v1/reconnect", &transcript)
        .finalize()
        .into_bytes();
    assert_eq!(hex(&reconnect[..16]), "66a8c675c9a3caf4fc7b08834b399d4c");
}

#[test]
fn enrollment_requires_both_confirmations_and_consumes_challenge() {
    let now = Instant::now();
    let mut pairing = Pairing::new("rustsimhub-vector".into(), Some(now + PAIRING_WINDOW));
    let mut store = Memory::default();
    let phone = phone();
    for approved in [false, true] {
        let begin = request(&mut pairing, &mut store, now, json!({"op":"begin"})).unwrap();
        let id = begin["challenge_id"].as_str().unwrap();
        assert_eq!(
            request(&mut pairing, &mut store, now, json!({"op":"begin"})),
            Err(Error::Busy)
        );
        let preview =
            request(&mut pairing, &mut store, now, fields("prepare", id, &phone)).unwrap();
        let code = preview["sas"].as_str().unwrap();
        assert_eq!(pairing.code(), Some(code));
        if approved {
            pairing.approve(code, now).unwrap();
        }
        let mut confirm = fields("confirm", id, &phone);
        confirm["sas"] = json!(code);
        let result = request(&mut pairing, &mut store, now, confirm.clone());
        if approved {
            let ready = result.unwrap();
            let key = store.credential().unwrap().keys().0;
            let tag = mac(key, b"AirSIM/VoBLE/v1/ready", id.as_bytes())
                .finalize()
                .into_bytes();
            assert_eq!(ready["ready"], true);
            assert_eq!(ready["confirmation_tag"], base64url(&tag[..16]));
        } else {
            assert_eq!(result, Err(Error::Approval));
            assert!(store.credential().is_none());
        }
        assert!(request(&mut pairing, &mut store, now, confirm).is_err());
    }
}

#[test]
fn closed_expired_low_order_and_mismatched_transcripts_fail() {
    let now = Instant::now();
    let mut store = Memory::default();
    let mut pairing = Pairing::new("rustsimhub-vector".into(), None);
    assert_eq!(
        request(&mut pairing, &mut store, now, json!({"op":"begin"})),
        Err(Error::Closed)
    );
    pairing.enrollment_deadline = Some(now + Duration::from_secs(1));
    let challenge = request(&mut pairing, &mut store, now, json!({"op":"begin"})).unwrap();
    let id = challenge["challenge_id"].as_str().unwrap();
    let mut invalid = phone();
    invalid.public = [0; 32];
    assert_eq!(
        request(
            &mut pairing,
            &mut store,
            now,
            fields("prepare", id, &invalid)
        ),
        Err(Error::Confirmation)
    );
    let phone = phone();
    let preview = request(&mut pairing, &mut store, now, fields("prepare", id, &phone)).unwrap();
    let code = preview["sas"].as_str().unwrap();
    pairing.approve(code, now).unwrap();
    let mut wrong = fields("confirm", id, &phone);
    wrong["iphone_nonce"] = json!(base64url(&[1; 32]));
    wrong["sas"] = json!(code);
    assert_eq!(
        request(&mut pairing, &mut store, now, wrong),
        Err(Error::Confirmation)
    );
    let begin = request(&mut pairing, &mut store, now, json!({"op":"begin"})).unwrap();
    let id = begin["challenge_id"].as_str().unwrap();
    assert_eq!(
        request(
            &mut pairing,
            &mut store,
            now + Duration::from_secs(1),
            fields("prepare", id, &phone)
        ),
        Err(Error::Closed)
    );
    assert!(store.credential().is_none());
}

#[test]
fn reconnect_rotates_only_on_valid_proof_and_recovers_after_dropped_ready() {
    let now = Instant::now();
    let mut store = Memory(Some(Credential::from_bytes([9; 32], None)));
    let identity = "rustsimhub-vector";
    let phone = phone();
    for key in [
        [6; 32],
        [9; 32],
        [9; 32],
        *derive(identity, &challenge(now), &phone).unwrap().0,
    ] {
        let mut pairing = Pairing::new(identity.into(), None);
        pairing.challenge = Some(challenge(now));
        let transcript = transcript(identity, pairing.challenge.as_ref().unwrap(), &phone).unwrap();
        let tag = mac(&key, b"AirSIM/VoBLE/v1/reconnect", &transcript)
            .finalize()
            .into_bytes();
        let mut confirm = fields("confirm", "challenge", &phone);
        confirm["reconnect_tag"] = json!(base64url(&tag[..16]));
        let result = request(&mut pairing, &mut store, now, confirm);
        if key == [6; 32] {
            assert_eq!(result, Err(Error::Reconnect));
            assert_eq!(store.credential().unwrap().keys(), (&[9; 32], None));
        } else {
            assert_eq!(result.unwrap()["ready"], true);
            assert_eq!(store.credential().unwrap().keys().1, Some(&key));
        }
    }
}

#[test]
fn small_mtu_fragment_round_trip_and_stale_frame_rejection() {
    let now = Instant::now();
    let data = vec![b'a'; 140];
    let fragments = fragment(&data, 0xabcd, 20).unwrap();
    assert_eq!(&fragments[0][..8], &[0xd4, 0x6b, 1, 1, 0xab, 0xcd, 0, 12]);
    let mut receiving = Fragments::default();
    assert_eq!(receiving.push(&fragments[1], now), Err(Error::Invalid));
    assert_eq!(receiving.push(&fragments[0], now), Ok(None));
    assert_eq!(
        receiving.push(&fragments[1], now + FRAGMENT_LIFETIME),
        Err(Error::Invalid)
    );
    for frame in &fragments[..fragments.len() - 1] {
        assert_eq!(receiving.push(frame, now), Ok(None));
    }
    assert_eq!(
        receiving.push(fragments.last().unwrap(), now),
        Ok(Some(data))
    );
}
