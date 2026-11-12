// SPDX-License-Identifier: AGPL-3.0-only
//! Host side of the AirSIM iPhone app's local ("VoWLAN") protocol, so the
//! stick can stand in for the Android phone the app was written for.
//!
//! The wire formats are fixed by the AirSIM app and its Android gateway
//! (github.com/kai-wu-cortex/AirSIM: iOS/AirSIM/VoWLANAuth.swift,
//! SamsungPairingCrypto.swift, android/.../VoWLAN*.java). Nothing here may
//! change a byte of them: an unmodified app has to accept every response.
//! Ported from NasSimHub `internal/airsimproto`, whose test vectors (taken
//! from the Android gateway code itself) are kept in `tests.rs`.
#![forbid(unsafe_code)]

mod pairing;
pub use pairing::*;

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// How far a signed request's timestamp may be from the gateway clock.
pub const CLOCK_SKEW_SECONDS: i64 = 30;
/// The shortest VoWLAN secret the app ever generates.
pub const MIN_SECRET_BYTES: usize = 32;

pub const HEADER_VERSION: &str = "X-AirSIM-VoWLAN-Version";
pub const HEADER_TIMESTAMP: &str = "X-AirSIM-VoWLAN-Timestamp";
pub const HEADER_NONCE: &str = "X-AirSIM-VoWLAN-Nonce";
pub const HEADER_SIGNATURE: &str = "X-AirSIM-VoWLAN-Signature";

/// Opens a PCM connection; `PCM_READY` is the gateway's reply.
pub const PCM_MAGIC: &str = "AIRSIMVWL1";
pub const PCM_READY: &str = "AIRSIMREADY";

/// Mono signed 16-bit little-endian audio in 20 ms frames. The released app
/// speaks 8 kHz; 16 kHz is kept selectable for a future app version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmRate {
    Narrow8k,
    Wide16k,
}
impl PcmRate {
    pub const fn hertz(self) -> u32 {
        match self {
            Self::Narrow8k => 8_000,
            Self::Wide16k => 16_000,
        }
    }
    /// Bytes in one 20 ms frame.
    pub const fn frame_bytes(self) -> usize {
        (self.hertz() as usize / 50) * 2
    }
    pub fn from_hertz(hertz: u32) -> Option<Self> {
        match hertz {
            8_000 => Some(Self::Narrow8k),
            16_000 => Some(Self::Wide16k),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// A signature, code or sealed payload did not verify.
    Authentication,
    /// The input is malformed.
    Invalid,
    PairingExpired,
    PairingUsed,
    /// The system random source failed.
    Random,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Authentication => "airsim: authentication failed",
            Self::Invalid => "airsim: invalid request",
            Self::PairingExpired => "airsim: pairing window expired",
            Self::PairingUsed => "airsim: pairing window already used",
            Self::Random => "airsim: random source unavailable",
        })
    }
}
impl std::error::Error for Error {}

/// Unpadded base64url, the encoding every AirSIM field uses.
pub fn base64url(value: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)
}

/// Accepts the unpadded form and, like the app, padded input.
pub fn decode_base64url(value: &str) -> Result<Vec<u8>, Error> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value.trim().trim_end_matches('='))
        .map_err(|_| Error::Invalid)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The string a VoWLAN request signature covers:
/// `METHOD\npath?query\nsha256-hex(body)\ntimestamp\nnonce`.
pub fn canonical(
    method: &str,
    path_and_query: &str,
    body: &[u8],
    timestamp: i64,
    nonce: &str,
) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        method.to_uppercase(),
        path_and_query,
        hex(&Sha256::digest(body)),
        timestamp,
        nonce
    )
}

fn mac(secret: &[u8], message: &str) -> Hmac<Sha256> {
    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(message.as_bytes());
    mac
}

/// `base64url(HMAC-SHA256(secret, message))`.
pub fn sign(secret: &[u8], message: &str) -> String {
    base64url(&mac(secret, message).finalize().into_bytes())
}

/// Constant-time comparison; secrets shorter than the app's are refused.
pub fn verify_signature(secret: &[u8], signature: &str, message: &str) -> bool {
    if secret.len() < MIN_SECRET_BYTES || signature.is_empty() {
        return false;
    }
    match decode_base64url(signature) {
        // The app sends the canonical unpadded text; anything else is not its signature.
        Ok(bytes) if base64url(&bytes) == signature => {
            mac(secret, message).verify_slice(&bytes).is_ok()
        }
        _ => false,
    }
}

/// Whether `timestamp` (Unix seconds) is within the clock skew of `now`.
pub fn fresh_timestamp(timestamp: i64, now: i64) -> bool {
    now.abs_diff(timestamp) <= CLOCK_SKEW_SECONDS as u64
}

/// The Android gateway's nonce length limits.
pub fn valid_nonce(nonce: &str) -> bool {
    (7..=128).contains(&nonce.len())
}

/// Proves to the app that the gateway holds its pairing secret.
pub fn health_proof(secret: &[u8], nonce: &str, agent_id: &str) -> String {
    sign(secret, &format!("AIRSIM-HEALTH1\n{nonce}\n{agent_id}"))
}

/// Binds an owned outgoing call to the request that created it.
pub fn dial_proof(
    secret: &[u8],
    nonce: &str,
    operation_id: &str,
    android_id: &str,
    backend_id: &str,
) -> String {
    sign(
        secret,
        &format!("AIRSIM-DIAL1\n{nonce}\n{operation_id}\n{android_id}\n{backend_id}"),
    )
}

/// Lets a manually paired app trust the returned ports.
pub fn direct_route_proof(
    secret: &[u8],
    session_id: &str,
    client_key: &str,
    agent_id: &str,
    control_port: u16,
    pcm_port: u16,
) -> String {
    sign(
        secret,
        &format!(
            "AIRSIM-DIRECT1\n{session_id}\n{client_key}\n{agent_id}\n{control_port}\n{pcm_port}"
        ),
    )
}

/// Remembers nonces for twice the clock skew. Bounded: when full, new nonces
/// are refused rather than old ones forgotten.
pub struct ReplayCache {
    seen: HashMap<String, i64>,
    capacity: usize,
}
impl Default for ReplayCache {
    fn default() -> Self {
        Self::with_capacity(4096)
    }
}
impl ReplayCache {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            seen: HashMap::new(),
            capacity,
        }
    }
    /// Records `nonce` and reports whether it was new. `now` is Unix seconds.
    pub fn accept(&mut self, nonce: &str, now: i64) -> bool {
        if !valid_nonce(nonce) {
            return false;
        }
        self.seen
            .retain(|_, at| now.saturating_sub(*at) <= 2 * CLOCK_SKEW_SECONDS);
        if self.seen.contains_key(nonce) || self.seen.len() >= self.capacity {
            return false;
        }
        self.seen.insert(nonce.to_owned(), now);
        true
    }
}

/// The first line of a PCM connection:
/// `AIRSIMVWL1 <timestamp> <nonce> <signature>\n`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmPreface {
    pub timestamp: i64,
    pub nonce: String,
    pub signature: String,
}

pub fn parse_pcm_preface(line: &str) -> Result<PcmPreface, Error> {
    if line.len() > 1024 {
        return Err(Error::Invalid);
    }
    let body = line.strip_suffix('\n').ok_or(Error::Invalid)?;
    let fields: Vec<&str> = body.split(' ').collect();
    // No control or non-ASCII bytes anywhere: a second newline must not ride in a field.
    if !body.bytes().all(|b| b == b' ' || b.is_ascii_graphic()) {
        return Err(Error::Invalid);
    }
    if fields.len() != 4
        || fields[0] != PCM_MAGIC
        || !valid_nonce(fields[2])
        || !(40..=128).contains(&fields[3].len())
    {
        return Err(Error::Invalid);
    }
    // Digits only: `i64::from_str` would also take a leading '+'.
    if fields[1].is_empty()
        || !fields[1]
            .strip_prefix('-')
            .unwrap_or(fields[1])
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        return Err(Error::Invalid);
    }
    Ok(PcmPreface {
        timestamp: fields[1].parse().map_err(|_| Error::Invalid)?,
        nonce: fields[2].to_owned(),
        signature: fields[3].to_owned(),
    })
}

/// What a PCM preface signature covers.
pub fn pcm_canonical(preface: &PcmPreface) -> String {
    canonical("PCM", "/v1/pcm", b"", preface.timestamp, &preface.nonce)
}

#[cfg(test)]
mod tests;
