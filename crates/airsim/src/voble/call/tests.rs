// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const NOW: i64 = 1_700_000_000_500;
// Generated independently with Python hashlib/hmac and openssl EVP AES-GCM.
const IPHONE_CONTROL: &str = include_str!("../../../tests/data/voble-control.hex");
const IPHONE_AUDIO: &str = include_str!("../../../tests/data/voble-microphone.hex");

fn decode_hex(data: &str) -> Vec<u8> {
    let data = data.trim();
    (0..data.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&data[i..i + 2], 16).unwrap())
        .collect()
}

fn channel() -> Channel {
    Channel::new(&std::array::from_fn(|i| i as u8))
}

#[test]
fn iphone_control_envelope_authenticates_before_it_spends_a_nonce() {
    let mut channel = channel();
    let raw = decode_hex(IPHONE_CONTROL);
    let mut tampered = raw.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        channel.request(&tampered, NOW),
        Err(Error::Confirmation)
    ));
    assert!(!channel.established());
    assert!(matches!(
        channel.request(&raw, NOW + 30_001),
        Err(Error::Confirmation)
    ));
    let command = channel.request(&raw, NOW).unwrap();
    assert!(channel.established());
    assert_eq!(command.name, "start_media");
    assert_eq!(command.target.generation, 5);
    assert_eq!(command.target.call, "call-test");
    assert!(matches!(
        channel.request(&raw, NOW),
        Err(Error::Confirmation)
    ));
}

#[test]
fn encrypted_iphone_microphone_and_gateway_speaker_use_separate_crypto_directions() {
    let mut channel = channel();
    let command = channel.request(&decode_hex(IPHONE_CONTROL), NOW).unwrap();
    let mut media = channel.media(&command.target).unwrap();
    let wire = decode_hex(IPHONE_AUDIO);
    let now = Instant::now();
    let mut corrupted = wire.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(matches!(
        media.uplink(&corrupted, now),
        Err(Error::Confirmation)
    ));
    let output = media.uplink(&wire, now).unwrap().unwrap();
    let samples: Vec<_> = output
        .as_chunks::<2>()
        .0
        .iter()
        .take(4)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    assert_eq!(samples, [988, -988, 988, -988]);
    assert!(media.uplink(&wire, now).is_err());
    let frames = media.downlink(&output, 100).unwrap();
    assert_eq!(frames.len(), 3);
    assert!(frames.iter().all(|f| f.len() <= 100));
    let sealed: Vec<u8> = frames
        .iter()
        .flat_map(|f| f[AUDIO_HEADER..].iter().copied())
        .collect();
    let packet = Packet {
        flags: 0,
        stream: 2,
        seq: 0,
        stamp: 0,
        count: 3,
    };
    let plaintext = media
        .cipher
        .decrypt(
            Nonce::from_slice(&packet.nonce(2)),
            Payload {
                msg: &sealed,
                aad: &packet.aad(2),
            },
        )
        .unwrap();
    assert_eq!(plaintext, [0xce, 0x4e].repeat(80));
    assert!(media
        .cipher
        .decrypt(
            Nonce::from_slice(&packet.nonce(1)),
            Payload {
                msg: &sealed,
                aad: &packet.aad(1)
            }
        )
        .is_err());
}

#[test]
fn pairing_and_control_fragment_envelopes_do_not_interchange() {
    let now = Instant::now();
    let body = decode_hex(IPHONE_CONTROL);
    let nested = super::super::fragment_control(&body, 5, 20).unwrap();
    let mut pairing = super::super::Fragments::default();
    assert!(pairing.push(&nested[0], now).is_err());
    let mut control = super::super::Fragments::control();
    for outer in &nested[..nested.len() - 1] {
        assert!(control.push(outer, now).unwrap().is_none());
    }
    assert_eq!(
        control.push(nested.last().unwrap(), now).unwrap(),
        Some(body)
    );
}
