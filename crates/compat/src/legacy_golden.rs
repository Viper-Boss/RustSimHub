// SPDX-License-Identifier: AGPL-3.0-only
// Produced by unmodified legacy Go functions, not by this Rust implementation.
use super::*;
use ed25519_dalek::{Signer, SigningKey};

#[derive(Deserialize)]
struct Fixture {
    source_revision: String,
    signing: Vec<SigningVector>,
    versions: Vec<VersionVector>,
}
#[derive(Deserialize)]
struct SigningVector {
    method: String,
    path: String,
    timestamp: String,
    nonce: String,
    body_hex: String,
    canonical_hex: String,
    public_hex: String,
    signature: String,
    device_id: String,
    fingerprint: String,
}
#[derive(Deserialize)]
struct VersionVector {
    node: Compatibility,
    core: LegacyRequirements,
    result: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LegacyRequirements {
    protocol_major: u16,
    min_protocol_minor: u16,
    min_agent_version: String,
    core_version: String,
}
fn hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn fixture() -> Fixture {
    let fixture: Fixture = serde_json::from_str(include_str!("legacy-golden.json")).unwrap();
    assert_eq!(
        fixture.source_revision,
        "ea019485e5d890a96acae357c52b94c746de837f"
    );
    fixture
}
#[test]
fn go_signing_canonical_signature_and_identity_match() {
    let vectors = fixture().signing;
    assert_eq!(vectors.len(), 5);
    let key = SigningKey::from_bytes(&[77; 32]);
    for v in vectors {
        let public: [u8; 32] = hex(&v.public_hex).try_into().unwrap();
        assert_eq!(public, key.verifying_key().to_bytes());
        let canonical = signing_bytes(
            &v.method,
            &v.path,
            &v.timestamp,
            &v.nonce,
            &hex(&v.body_hex),
        )
        .unwrap();
        assert_eq!(canonical, hex(&v.canonical_hex));
        assert_eq!(
            STANDARD.encode(key.sign(&canonical).to_bytes()),
            v.signature
        );
        verify_signature(&public, &v.signature, &canonical).unwrap();
        assert_eq!(device_id(&public), v.device_id);
        assert_eq!(core_fingerprint(&public), v.fingerprint);
    }
}
#[test]
fn go_compatibility_direction_and_error_reason_match() {
    let vectors = fixture().versions;
    assert_eq!(vectors.len(), 12);
    for v in vectors {
        let core = Requirements {
            protocol_major: v.core.protocol_major,
            minimum_minor: v.core.min_protocol_minor,
            minimum_agent: &v.core.min_agent_version,
            core_version: &v.core.core_version,
        };
        let result = negotiate(&v.node, &core);
        let reason = match result {
            Ok(()) => "ok".to_string(),
            Err(error) => serde_json::to_value(error)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
        };
        assert_eq!(reason, v.result);
    }
}
