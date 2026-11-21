// SPDX-License-Identifier: AGPL-3.0-only
#![cfg(feature = "transaction-candidate")]
use ed25519_dalek::{Signature, VerifyingKey};
use rustsimhub_usage_stats::{
    identity::StatisticsIdentity,
    protocol::{
        CheckInIntent, Clock, ClockError, DisclosureAcknowledgement, EnabledPolicy, OwnerConsent,
        Window, RENEW_INTERVAL_SECS,
    },
    transaction::{Error, MAX_TRANSACTION_BYTES},
};
use sha2::{Digest, Sha256};
fn enabled() -> EnabledPolicy {
    EnabledPolicy::from_owner_consent(OwnerConsent::Granted, DisclosureAcknowledgement::Current)
        .unwrap()
}
struct At(u64);
impl Clock for At {
    fn now_secs(&self) -> Result<u64, ClockError> {
        Ok(self.0)
    }
}
struct Never;
impl Clock for Never {
    fn now_secs(&self) -> Result<u64, ClockError> {
        panic!("disabled must not read clock or prepare bytes")
    }
}
struct Broken;
impl Clock for Broken {
    fn now_secs(&self) -> Result<u64, ClockError> {
        Err(ClockError::Unavailable)
    }
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn candidate_matches_all_fixed_sdk_transaction_signature_and_chain_hash_vectors() {
    let raw = include_bytes!("../../../tools/nkn-build/usage-vectors/vectors.json");
    assert_eq!(
        hex(&Sha256::digest(raw)),
        "4e3b33a2beee10c61c3ba08df0f6938ddf6c73e33ddf2890f02c368e6242550e",
        "fixed SDK reference artifact changed"
    );
    let vectors: serde_json::Value = serde_json::from_slice(raw).unwrap();
    let identity = StatisticsIdentity::from_seed(std::array::from_fn(|i| i as u8));
    let rows = vectors["vectors"].as_array().unwrap();
    assert_eq!(rows.len(), 15);
    for row in rows {
        let intent = CheckInIntent::parse(&serde_json::to_vec(&row["intent"]).unwrap()).unwrap();
        let nonce = row["nonce"].as_u64().unwrap();
        let prepared = identity
            .prepare_subscription(enabled(), intent, nonce, &At(20000 * RENEW_INTERVAL_SECS))
            .unwrap()
            .unwrap();
        assert_eq!(prepared.intent(), intent);
        assert_eq!(prepared.nonce(), nonce);
        assert!(prepared.transaction_bytes().len() <= MAX_TRANSACTION_BYTES);
        assert_eq!(
            String::from_utf8(intent.to_json().unwrap()).unwrap(),
            row["meta"]
        );
        let digest = prepared.digest();
        for (key, actual) in [
            ("unsigned_hex", prepared.unsigned_bytes()),
            ("signing_hash_hex", prepared.signing_hash().as_slice()),
            ("signature_hex", prepared.signature().as_slice()),
            ("transaction_hex", prepared.transaction_bytes()),
            ("digest_hex", digest.as_slice()),
        ] {
            assert_eq!(
                hex(actual),
                row[key].as_str().unwrap(),
                "SDK mismatch {key}, nonce={nonce}"
            );
        }
        let public = VerifyingKey::from_bytes(identity.public_key().as_bytes()).unwrap();
        let signature = Signature::from_bytes(prepared.signature());
        public
            .verify_strict(prepared.signing_hash(), &signature)
            .unwrap();
        let mut wrong = *prepared.signing_hash();
        wrong[0] ^= 1;
        assert!(public.verify_strict(&wrong, &signature).is_err());
    }
}
#[test]
fn off_wrong_identity_stale_period_and_clock_failure_never_return_signed_bytes() {
    let identity = StatisticsIdentity::from_seed([42; 32]);
    let clock = At(20000 * RENEW_INTERVAL_SECS);
    let intent = enabled()
        .check_in_intent(Window::Last24Hours, identity.public_key(), &clock)
        .unwrap()
        .unwrap();
    assert!(identity
        .prepare_subscription(EnabledPolicy::disabled(), intent, 0, &Never)
        .unwrap()
        .is_none());
    let foreign = StatisticsIdentity::from_seed([43; 32]);
    assert!(matches!(
        foreign.prepare_subscription(enabled(), intent, 0, &Never),
        Err(Error::WrongIdentity)
    ));
    assert!(matches!(
        identity.prepare_subscription(enabled(), intent, 0, &At(clock.0 + RENEW_INTERVAL_SECS)),
        Err(Error::StaleIntent)
    ));
    assert!(matches!(
        identity.prepare_subscription(enabled(), intent, 0, &At(clock.0 - 1)),
        Err(Error::StaleIntent)
    ));
    assert!(matches!(
        identity.prepare_subscription(enabled(), intent, 0, &Broken),
        Err(Error::Clock)
    ));
    let valid = identity
        .prepare_subscription(enabled(), intent, u64::MAX, &clock)
        .unwrap()
        .unwrap();
    let intent_hash: [u8; 32] = Sha256::digest(intent.to_json().unwrap()).into();
    assert_ne!(valid.digest(), intent_hash);
}
