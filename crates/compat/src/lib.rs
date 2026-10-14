// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! AirSimHub protocol 1.5 contract primitives, not an enabled legacy gateway.
//! An adapter must use a separately approved NAS principal and enforce live v2 scopes.
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use rustsimhub_policy::{Kind, Principal, Scope};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    InvalidInput,
    Signature,
    ProtocolMajorMismatch,
    NodeProtocolTooOld,
    AgentTooOld,
    CoreTooOld,
    VersionUnreadable,
    OwnerApprovalRequired,
    NotSupported,
    Forbidden,
}
type Result<T> = std::result::Result<T, Error>;

/// Exact legacy bytes: method, path, original timestamp, original nonce, Base64 SHA256.
/// Validation excludes canonical-field newline injection; time/replay policy is separate.
pub fn signing_bytes(
    method: &str,
    path: &str,
    timestamp: &str,
    nonce: &str,
    body: &[u8],
) -> Result<Vec<u8>> {
    if method.is_empty()
        || method.len() > 16
        || !method.bytes().all(|b| b.is_ascii_alphabetic())
        || !path.starts_with("/v1/")
        || path.len() > 512
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_.".contains(&b))
        || timestamp.is_empty()
        || timestamp.len() > 64
        || !timestamp.bytes().all(|b| b.is_ascii_graphic())
        || nonce.is_empty()
        || nonce.len() > 128
        || !nonce.bytes().all(|b| b.is_ascii_graphic())
        || body.len() > 65536
    {
        return Err(Error::InvalidInput);
    }
    Ok(format!(
        "{}\n{}\n{}\n{}\n{}",
        method.to_ascii_uppercase(),
        path,
        timestamp,
        nonce,
        STANDARD.encode(Sha256::digest(body))
    )
    .into_bytes())
}
pub fn verify_signature(public: &[u8; 32], signature: &str, canonical: &[u8]) -> Result<()> {
    if signature.len() != 88 || canonical.len() > 66000 {
        return Err(Error::InvalidInput);
    }
    let decoded = STANDARD.decode(signature).map_err(|_| Error::Signature)?;
    // Reject noncanonical alternate encodings, including unpadded and ignored whitespace.
    if STANDARD.encode(&decoded) != signature {
        return Err(Error::Signature);
    }
    let signature = Signature::from_slice(&decoded).map_err(|_| Error::Signature)?;
    let key = VerifyingKey::from_bytes(public).map_err(|_| Error::Signature)?;
    if key.is_weak() {
        return Err(Error::Signature);
    }
    key.verify_strict(canonical, &signature)
        .map_err(|_| Error::Signature)
}
pub fn device_id(public: &[u8; 32]) -> String {
    let digest = Sha256::digest(public);
    format!(
        "NSH-410-{:02X}{:02X}{:02X}",
        digest[0], digest[1], digest[2]
    )
}
pub fn core_fingerprint(public: &[u8; 32]) -> String {
    Sha256::digest(public)[..4]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);
pub fn parse_version(value: &str) -> Result<Version> {
    if value.len() > 128 {
        return Err(Error::VersionUnreadable);
    }
    let value = value.trim().strip_prefix('v').unwrap_or(value.trim());
    let numeric = value.split(['-', '+']).next().unwrap_or("");
    let parts = numeric.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 {
        return Err(Error::VersionUnreadable);
    }
    let mut result = [0; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::VersionUnreadable);
        }
        result[i] = p.parse().map_err(|_| Error::VersionUnreadable)?;
        if result[i] > i64::MAX as u64 {
            return Err(Error::VersionUnreadable);
        }
    }
    Ok(Version(result[0], result[1], result[2]))
}
fn development(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "dev" | "devel" | "unknown"
    )
}
#[derive(Deserialize)]
pub struct Compatibility {
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub agent_version: String,
    pub min_core_version: String,
}
pub struct Requirements<'a> {
    pub protocol_major: u16,
    pub minimum_minor: u16,
    pub minimum_agent: &'a str,
    pub core_version: &'a str,
}
pub fn negotiate(node: &Compatibility, core: &Requirements<'_>) -> Result<()> {
    if node.protocol_major != core.protocol_major {
        return Err(Error::ProtocolMajorMismatch);
    }
    if node.protocol_minor < core.minimum_minor {
        return Err(Error::NodeProtocolTooOld);
    }
    match parse_version(&node.agent_version) {
        Ok(version) if !development(&node.agent_version) => {
            if version < parse_version(core.minimum_agent)? {
                return Err(Error::AgentTooOld);
            }
        }
        _ => {}
    }
    if !node.min_core_version.is_empty()
        && !development(core.core_version)
        && parse_version(core.core_version)? < parse_version(&node.min_core_version)?
    {
        return Err(Error::CoreTooOld);
    }
    Ok(())
}

#[cfg(test)]
mod legacy_golden;

/// Read-only subset; signing primitives alone never authorize ownership or mint v2 tokens.
pub fn endpoint_scope(method: &str, path: &str) -> Result<Scope> {
    match (method, path) {
        ("GET", "/v1/health" | "/v1/status" | "/v1/modem" | "/v1/capabilities") => {
            Ok(Scope::StatusRead)
        }
        ("GET", "/v1/sms") => Ok(Scope::SmsRead),
        ("GET", "/v1/calls") => Ok(Scope::CallsRead),
        ("POST", "/v1/pair") => Err(Error::OwnerApprovalRequired),
        _ => Err(Error::NotSupported),
    }
}
pub fn check_nas_read(current: &Principal, method: &str, path: &str) -> Result<()> {
    let scope = endpoint_scope(method, path)?;
    if current.kind != Kind::Nas
        || !current.active
        || !rustsimhub_policy::allows(current.scopes, scope)
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn signatures_bind_exact_method_path_stamp_nonce_and_body() {
        let key = SigningKey::from_bytes(&[77; 32]);
        let public = key.verifying_key().to_bytes();
        let bytes = signing_bytes(
            "post",
            "/v1/session",
            "2026-10-14T02:00:00Z",
            "synthetic-nonce-0123456789",
            b"{\"core_id\":\"synthetic\"}",
        )
        .unwrap();
        let sig = STANDARD.encode(key.sign(&bytes).to_bytes());
        verify_signature(&public, &sig, &bytes).unwrap();
        for (method, path, stamp, nonce, body) in [
            (
                "get",
                "/v1/session",
                "2026-10-14T02:00:00Z",
                "synthetic-nonce-0123456789",
                b"{\"core_id\":\"synthetic\"}".as_slice(),
            ),
            (
                "post",
                "/v1/pair",
                "2026-10-14T02:00:00Z",
                "synthetic-nonce-0123456789",
                b"{\"core_id\":\"synthetic\"}".as_slice(),
            ),
            (
                "post",
                "/v1/session",
                "2026-10-14T02:00:01Z",
                "synthetic-nonce-0123456789",
                b"{\"core_id\":\"synthetic\"}".as_slice(),
            ),
            (
                "post",
                "/v1/session",
                "2026-10-14T02:00:00Z",
                "different-nonce-0123456789",
                b"{\"core_id\":\"synthetic\"}".as_slice(),
            ),
            (
                "post",
                "/v1/session",
                "2026-10-14T02:00:00Z",
                "synthetic-nonce-0123456789",
                b"{}".as_slice(),
            ),
        ] {
            assert!(verify_signature(
                &public,
                &sig,
                &signing_bytes(method, path, stamp, nonce, body).unwrap()
            )
            .is_err());
        }
        assert!(signing_bytes("post", "/v1/session", "stamp\nGET", "nonce", b"").is_err());
        assert!(signing_bytes("post", "/v1/session", "stamp", "nonce\nGET", b"").is_err());
        assert!(verify_signature(&public, sig.trim_end_matches('='), &bytes).is_err());
    }
    #[test]
    fn version_contract_separates_offline_from_incompatible_and_preserves_legacy_labels() {
        assert_eq!(
            parse_version(" v1.2.3-rc1+build7 ").unwrap(),
            Version(1, 2, 3)
        );
        for bad in [
            "",
            "abc",
            "1.x.3",
            "1.2.3.4",
            "-1.0.0",
            "1..2",
            "9223372036854775808",
        ] {
            assert!(parse_version(bad).is_err());
        }
        let mut node = Compatibility {
            protocol_major: 1,
            protocol_minor: 5,
            agent_version: "0.1.0".into(),
            min_core_version: "0.1.0".into(),
        };
        let core = Requirements {
            protocol_major: 1,
            minimum_minor: 5,
            minimum_agent: "0.1.0",
            core_version: "0.1.0",
        };
        negotiate(&node, &core).unwrap();
        node.protocol_major = 2;
        assert_eq!(negotiate(&node, &core), Err(Error::ProtocolMajorMismatch));
        node.protocol_major = 1;
        node.protocol_minor = 4;
        assert_eq!(negotiate(&node, &core), Err(Error::NodeProtocolTooOld));
        node.protocol_minor = 5;
        node.min_core_version = "0.2.0".into();
        assert_eq!(negotiate(&node, &core), Err(Error::CoreTooOld));
        node.min_core_version = "nonsense".into();
        assert_eq!(negotiate(&node, &core), Err(Error::VersionUnreadable));
        node.min_core_version = "0.1.0".into();
        node.agent_version = "unversioned-local-label".into();
        negotiate(&node, &core).unwrap();
    }
    #[test]
    fn legacy_pair_writes_and_non_nas_or_revoked_authority_are_never_accepted() {
        let mut p = Principal {
            id: "synthetic".into(),
            public_key: [1; 32],
            kind: Kind::Nas,
            scopes: 1,
            generation: 1,
            active: true,
        };
        check_nas_read(&p, "GET", "/v1/status").unwrap();
        assert!(check_nas_read(&p, "GET", "/v1/sms").is_err());
        p.kind = Kind::Owner;
        assert!(check_nas_read(&p, "GET", "/v1/status").is_err());
        p.kind = Kind::Nas;
        p.active = false;
        assert!(check_nas_read(&p, "GET", "/v1/status").is_err());
        for (method, path) in [
            ("POST", "/v1/sms"),
            ("DELETE", "/v1/sms"),
            ("POST", "/v1/calls"),
            ("POST", "/v1/factory-reset"),
            ("POST", "/v1/ota/stage"),
            ("POST", "/v1/transport/key"),
        ] {
            assert_eq!(endpoint_scope(method, path), Err(Error::NotSupported));
        }
        assert_eq!(
            endpoint_scope("POST", "/v1/pair"),
            Err(Error::OwnerApprovalRequired)
        );
    }
}
