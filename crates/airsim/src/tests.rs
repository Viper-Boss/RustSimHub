// SPDX-License-Identifier: AGPL-3.0-only
//! The expected values were produced by AirSIM's own Android gateway code
//! (VoWLANAuth.java compiled unchanged, PairingCrypto.deriveKey copied
//! verbatim) and, for the sealed registration, by an independent
//! implementation of the iOS app's CryptoKit calls; they are the vectors
//! NasSimHub pins. If one fails, an unmodified AirSIM app will no longer talk
//! to the gateway. Every secret here is synthetic.
use super::*;
use x25519_dalek::{PublicKey, StaticSecret};

fn secret() -> [u8; 32] {
    std::array::from_fn(|i| i as u8 + 1)
}
const NOW: i64 = 1_793_600_000;
const SESSION: &str = "11111111-2222-4333-8444-555555555555";

#[test]
fn canonical_and_signature_match_the_android_gateway() {
    let text = canonical(
        "post",
        "/api/calls/dial?x=1",
        br#"{"number":"10086"}"#,
        NOW,
        "nonce-abc-123",
    );
    assert_eq!(
        text,
        "POST\n/api/calls/dial?x=1\na8ef8485bac143ea16241105a3d9aa2b048b678a89abb8fabb15399ed38b249a\n1793600000\nnonce-abc-123"
    );
    let good = "T79O-CqSig21t8LKhZtM0CUmwNO_HNaiqd7z909XuEs";
    assert_eq!(sign(&secret(), &text), good);
    assert!(verify_signature(&secret(), good, &text));
    for bad in [
        "T79O-CqSig21t8LKhZtM0CUmwNO_HNaiqd7z909XuEt",
        "",
        "T79O-CqSig21t8LKhZtM0CUmwNO_HNaiqd7z909XuEs=",
        " T79O-CqSig21t8LKhZtM0CUmwNO_HNaiqd7z909XuEs",
        "T79O-CqSig21t8LKhZtM0CUmwNO_HNaiqd7z909XuE",
        "not base64 !!",
    ] {
        assert!(!verify_signature(&secret(), bad, &text), "{bad:?}");
    }
    assert!(!verify_signature(
        &secret(),
        good,
        &text.replace("nonce-abc-123", "nonce-abc-124")
    ));
    let short = &secret()[..31];
    assert!(
        !verify_signature(short, &sign(short, &text), &text),
        "a secret shorter than 32 bytes is refused"
    );
}

#[test]
fn proofs_match_the_android_gateway() {
    let s = secret();
    assert_eq!(
        health_proof(&s, "nonce-abc-123", "nassimhub-test"),
        "jPBg_ulZx15c32LUNugJESI6JKT1bKa7O3eLIyj65NQ"
    );
    assert_eq!(
        dial_proof(&s, "nonce-abc-123", "op-1", "call-9", "call-9"),
        "v8qE7N6W6VEKFbdZp7g3ndFflglUaggzq1etzOH9l7Y"
    );
    assert_eq!(
        direct_route_proof(&s, "sess-1", "clientkey", "nassimhub-test", 7591, 7592),
        "ZGg0kPk-QS3DUzwREpYuG4UjObwUqFmNDsh55hhKGas"
    );
}

#[test]
fn derived_key_matches_the_android_gateway() {
    let shared: [u8; 32] = std::array::from_fn(|i| 255 - i as u8);
    let key = pairing::hkdf_key(&shared, SESSION, "042917").unwrap();
    assert_eq!(
        hex(&key[..]),
        "10ba379a7ee998d9a60de0ecff1465a5866905e57122865ee516af0f64b170be"
    );
}

#[test]
fn opens_a_registration_sealed_by_the_app_and_refuses_wrong_inputs() {
    let server = StaticSecret::from(secret());
    assert_eq!(
        base64url(PublicKey::from(&server).as_bytes()),
        "B6N8vBQgk8i3VdwbEOhstCY3StFqqFPtC9_AsrhtHHw"
    );
    let client = decode_base64url("WGmv9FBUlzLLqu1eXfmzCm2jHLDldCutWtShp2jxpns").unwrap();
    let sealed = decode_base64url("ZGVmZ2hpamtsbW5vfhj6oAs_3IeZg1DpRoTfp760rc8JM3B4SO3EIzTzNhoPCoUBpyoGQwN8fIHkcc8noCG22E9PYl0Oi1YRzBF7fpq1rMmEhyNGKwNwOMLMvLfOw8veZE7Ol0TzgytrDcgmrQ").unwrap();
    let plaintext = open_sealed_registration(&server, &client, &sealed, SESSION, "042917").unwrap();
    assert!(String::from_utf8(plaintext)
        .unwrap()
        .contains(r#""vowlan_secret":"AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyA""#));
    let open = |client: &[u8], sealed: &[u8], session: &str, code: &str| {
        open_sealed_registration(&server, client, sealed, session, code)
    };
    assert_eq!(
        open(&client, &sealed, SESSION, "042918"),
        Err(Error::Authentication)
    );
    assert_eq!(
        open(&client, &sealed, "other-session", "042917"),
        Err(Error::Authentication)
    );
    let mut tampered = sealed.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert_eq!(
        open(&client, &tampered, SESSION, "042917"),
        Err(Error::Authentication)
    );
    assert_eq!(
        open(&client, &sealed[..27], SESSION, "042917"),
        Err(Error::Authentication)
    );
    assert_eq!(
        open(&client[..31], &sealed, SESSION, "042917"),
        Err(Error::Authentication)
    );
    // An all-zero public key is a low-order point: the shared secret would not depend on our key.
    assert_eq!(
        open(&[0; 32], &sealed, SESSION, "042917"),
        Err(Error::Authentication)
    );
    for code in ["42917", "0429170", "04291a", ""] {
        assert_eq!(
            open(&client, &sealed, SESSION, code),
            Err(Error::Invalid),
            "{code:?}"
        );
    }
    assert_eq!(open(&client, &sealed, "", "042917"), Err(Error::Invalid));
}

#[test]
fn a_pairing_window_opens_once_survives_a_wrong_code_and_expires() {
    let mut session = PairingSession::new(NOW).unwrap();
    assert_eq!(session.code().len(), 6);
    assert!(session.code().bytes().all(|b| b.is_ascii_digit()));
    assert_eq!(session.id().len(), 36);
    assert_eq!(session.expires_at(), NOW + 120);
    let other = PairingSession::new(NOW).unwrap();
    assert_ne!(session.id(), other.id());
    assert_ne!(session.public_key(), other.public_key());

    let client = StaticSecret::from([7u8; 32]);
    let client_public = PublicKey::from(&client).to_bytes();
    let seal = |code: &str, session: &PairingSession| {
        seal_registration(
            &client,
            &session.public_key(),
            br#"{"x":1}"#,
            session.id(),
            code,
            [9; 12],
        )
        .unwrap()
    };
    let wrong = if session.code() == "000000" {
        "000001"
    } else {
        "000000"
    };
    let bad = seal(wrong, &session);
    let code = session.code().to_string();
    let good = seal(&code, &session);
    assert_eq!(
        session.open(&client_public, &bad, NOW),
        Err(Error::Authentication)
    );
    assert_eq!(
        session.open(&client_public, &good, NOW + 121),
        Err(Error::PairingExpired)
    );
    assert_eq!(
        session.open(&client_public, &good, NOW + 120).as_deref(),
        Ok(&br#"{"x":1}"#[..]),
        "a wrong code and a late attempt do not consume the window"
    );
    assert_eq!(
        session.open(&client_public, &good, NOW),
        Err(Error::PairingUsed)
    );
}

#[test]
fn nonces_are_single_use_and_the_cache_is_bounded() {
    let mut cache = ReplayCache::default();
    assert!(cache.accept("nonce-1234", NOW));
    assert!(!cache.accept("nonce-1234", NOW + 1));
    assert!(!cache.accept("short", NOW));
    assert!(!cache.accept(&"n".repeat(129), NOW));
    assert!(
        !cache.accept("nonce-1234", NOW + 60),
        "still inside the retention window"
    );
    assert!(
        cache.accept("nonce-1234", NOW + 61),
        "an expired nonce is not blocked forever"
    );

    let mut small = ReplayCache::with_capacity(2);
    assert!(small.accept("nonce-aaaa", NOW) && small.accept("nonce-bbbb", NOW));
    assert!(
        !small.accept("nonce-cccc", NOW),
        "a full cache refuses instead of forgetting"
    );
    assert!(
        !small.accept("nonce-aaaa", NOW),
        "and still knows the old nonces"
    );
    assert!(small.accept("nonce-cccc", NOW + 61));

    assert!(fresh_timestamp(NOW - 30, NOW) && fresh_timestamp(NOW + 30, NOW));
    assert!(!fresh_timestamp(NOW - 31, NOW) && !fresh_timestamp(NOW + 31, NOW));
    assert!(!fresh_timestamp(i64::MIN, NOW) && !fresh_timestamp(i64::MAX, i64::MIN));
}

#[test]
fn pcm_preface_is_parsed_strictly() {
    let signature = sign(
        &secret(),
        &canonical("PCM", "/v1/pcm", b"", NOW, "nonce-abc-123"),
    );
    let line = format!("AIRSIMVWL1 1793600000 nonce-abc-123 {signature}\n");
    let preface = parse_pcm_preface(&line).unwrap();
    assert_eq!(preface.timestamp, NOW);
    assert!(verify_signature(
        &secret(),
        &preface.signature,
        &pcm_canonical(&preface)
    ));
    for bad in [
        format!("AIRSIMVWL1 1793600000 nonce-abc-123 {signature}"),
        format!("AIRSIMVWL2 1793600000 nonce-abc-123 {signature}\n"),
        format!("AIRSIMVWL1 x nonce-abc-123 {signature}\n"),
        format!("AIRSIMVWL1 +1793600000 nonce-abc-123 {signature}\n"),
        format!("AIRSIMVWL1  nonce-abc-123 {signature}\n"),
        format!("AIRSIMVWL1 1793600000 n {signature}\n"),
        "AIRSIMVWL1 1793600000 nonce-abc-123 short\n".to_string(),
        format!("AIRSIMVWL1 1793600000 nonce-abc-123 {signature} extra\n"),
        format!("AIRSIMVWL1 1793600000 nonce-abc-123 {signature}\n\n"),
        format!("AIRSIMVWL1 1793600000 nonce-abc-123 {}\n", "s".repeat(1100)),
        String::new(),
    ] {
        assert_eq!(parse_pcm_preface(&bad), Err(Error::Invalid), "{bad:?}");
    }
}

#[test]
fn base64url_round_trips_and_accepts_padding() {
    assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    assert_eq!(decode_base64url("-_8").unwrap(), [0xfb, 0xff]);
    assert_eq!(decode_base64url(" -_8= ").unwrap(), [0xfb, 0xff]);
    assert_eq!(decode_base64url("+/8"), Err(Error::Invalid));
}

#[test]
fn both_audio_rates_use_twenty_millisecond_frames() {
    assert_eq!(PcmRate::Narrow8k.frame_bytes(), 320);
    assert_eq!(PcmRate::Wide16k.frame_bytes(), 640);
    assert_eq!(PcmRate::from_hertz(8000), Some(PcmRate::Narrow8k));
    assert_eq!(PcmRate::from_hertz(16000), Some(PcmRate::Wide16k));
    assert_eq!(PcmRate::from_hertz(44100), None);
}
