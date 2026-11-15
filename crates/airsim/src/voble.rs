// SPDX-License-Identifier: AGPL-3.0-only
//! AirSIM VoBLE v1 pairing. The phone's BLE control and media protocols are separate.

use crate::{base64url, decode_base64url, hex};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

pub const PAIRING_WINDOW: Duration = Duration::from_secs(120);
const FRAGMENT_LIFETIME: Duration = Duration::from_secs(2);
const HEADER: usize = 8;
const PREFIX: [u8; 4] = [0xd4, 0x6b, 1, 1];
const MAX_MESSAGE: usize = 4096;
const MAX_FRAGMENTS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Busy,
    Closed,
    AlreadyPaired,
    Unpaired,
    Reconnect,
    Confirmation,
    Approval,
    Unavailable,
}

impl Error {
    pub fn response(self) -> Value {
        let message = match self {
            Self::Invalid => "invalid pairing message",
            Self::Busy => "pairing already in progress",
            Self::Closed => "pairing window closed",
            Self::AlreadyPaired => "peer is already paired",
            Self::Unpaired => "peer is not paired",
            Self::Reconnect => "reconnect authentication failed",
            Self::Confirmation => "pairing confirmation failed",
            Self::Approval => "confirm the code on RustSimHub first",
            Self::Unavailable => "pairing unavailable",
        };
        json!({"version": 1, "ok": false, "error": message})
    }
}

/// VoBLE uses a different fragment envelope from RustSimHub recovery GATT.
/// iOS writes each fragment with response and serializes its write batches.
#[derive(Default)]
pub struct Fragments(Option<Partial>);

struct Partial {
    id: u16,
    count: usize,
    next: usize,
    started: Instant,
    data: Vec<u8>,
}

impl Fragments {
    pub fn push(&mut self, frame: &[u8], now: Instant) -> Result<Option<Vec<u8>>, Error> {
        let mut pending = self.0.take();
        if frame.len() <= HEADER || !frame.starts_with(&PREFIX) {
            return Err(Error::Invalid);
        }
        let id = u16::from_be_bytes([frame[4], frame[5]]);
        let index = usize::from(frame[6]);
        let count = usize::from(frame[7]);
        if count == 0 || count > MAX_FRAGMENTS || index >= count {
            return Err(Error::Invalid);
        }
        if index == 0 {
            pending = Some(Partial {
                id,
                count,
                next: 0,
                started: now,
                data: Vec::new(),
            });
        }
        let mut partial = pending.ok_or(Error::Invalid)?;
        if (partial.id, partial.count, partial.next) != (id, count, index)
            || now.duration_since(partial.started) >= FRAGMENT_LIFETIME
            || partial.data.len() + frame.len() - HEADER > MAX_MESSAGE
        {
            return Err(Error::Invalid);
        }
        partial.data.extend_from_slice(&frame[HEADER..]);
        partial.next += 1;
        if partial.next == count {
            return Ok(Some(partial.data));
        }
        self.0 = Some(partial);
        Ok(None)
    }
}

pub fn fragment(payload: &[u8], id: u16, att_payload: usize) -> Result<Vec<Vec<u8>>, Error> {
    let capacity = att_payload
        .checked_sub(HEADER)
        .filter(|n| *n > 0)
        .ok_or(Error::Invalid)?;
    let count = payload.len().div_ceil(capacity);
    if count == 0 || count > MAX_FRAGMENTS || payload.len() > MAX_MESSAGE {
        return Err(Error::Invalid);
    }
    Ok(payload
        .chunks(capacity)
        .enumerate()
        .map(|(index, data)| {
            let mut frame = Vec::with_capacity(HEADER + data.len());
            frame.extend_from_slice(&PREFIX);
            frame.extend_from_slice(&id.to_be_bytes());
            frame.extend_from_slice(&[index as u8, count as u8]);
            frame.extend_from_slice(data);
            frame
        })
        .collect())
}

type Key = Zeroizing<[u8; 32]>;

pub struct Credential {
    current: Key,
    // During reconnect the phone replaces its key only after it receives ready.
    // Retain the key that authenticated this rotation across an interrupted reply.
    previous: Option<Key>,
}

impl Credential {
    pub fn from_bytes(current: [u8; 32], previous: Option<[u8; 32]>) -> Self {
        Self {
            current: Zeroizing::new(current),
            previous: previous.map(Zeroizing::new),
        }
    }

    pub fn keys(&self) -> (&[u8; 32], Option<&[u8; 32]>) {
        (&self.current, self.previous.as_deref())
    }

    fn authenticated_key(&self, transcript: &[u8], tag: &[u8]) -> Option<Key> {
        let verifies = |key: &Key| {
            mac(key, b"AirSIM/VoBLE/v1/reconnect", transcript)
                .verify_truncated_left(tag)
                .is_ok()
        };
        if verifies(&self.current) {
            return Some(self.current.clone());
        }
        self.previous.as_ref().filter(|key| verifies(key)).cloned()
    }
}

pub trait Store {
    fn credential(&self) -> Option<&Credential>;
    /// Make the credential durable before the phone receives ready.
    fn save(&mut self, credential: Credential) -> Result<(), Error>;
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", deny_unknown_fields)]
enum Request {
    Begin,
    Prepare {
        challenge_id: String,
        iphone_public: String,
        iphone_nonce: String,
    },
    Confirm {
        challenge_id: String,
        iphone_public: String,
        iphone_nonce: String,
        sas: Option<String>,
        reconnect_tag: Option<String>,
    },
}

#[derive(PartialEq, Eq)]
struct Phone {
    public: [u8; 32],
    nonce: [u8; 32],
}

impl Phone {
    fn decode(public: &str, nonce: &str) -> Result<Self, Error> {
        let decode = |value| {
            decode_base64url(value)
                .map_err(|_| Error::Invalid)?
                .try_into()
                .map_err(|_| Error::Invalid)
        };
        Ok(Self {
            public: decode(public)?,
            nonce: decode(nonce)?,
        })
    }
}

struct Challenge {
    id: String,
    secret: StaticSecret,
    nonce: [u8; 32],
    deadline: Instant,
    prepared: Option<Prepared>,
}

struct Prepared {
    phone: Phone,
    key: Key,
    code: String,
    approved: bool,
}

/// Create a session for one transport peer. Destroy it when that peer disconnects.
pub struct Pairing {
    identity: String,
    enrollment_deadline: Option<Instant>,
    challenge: Option<Challenge>,
    paired: bool,
}

impl Pairing {
    pub fn new(identity: String, enrollment_deadline: Option<Instant>) -> Self {
        Self {
            identity,
            enrollment_deadline,
            challenge: None,
            paired: false,
        }
    }

    pub fn code(&self) -> Option<&str> {
        self.challenge
            .as_ref()?
            .prepared
            .as_ref()
            .filter(|prepared| !prepared.approved)
            .map(|prepared| prepared.code.as_str())
    }

    pub fn approve(&mut self, code: &str, now: Instant) -> Result<(), Error> {
        let challenge = self.challenge.as_mut().ok_or(Error::Closed)?;
        if now >= challenge.deadline {
            self.challenge = None;
            return Err(Error::Closed);
        }
        let prepared = challenge.prepared.as_mut().ok_or(Error::Confirmation)?;
        if code != prepared.code {
            self.challenge = None;
            return Err(Error::Confirmation);
        }
        prepared.approved = true;
        Ok(())
    }

    pub fn request(
        &mut self,
        data: &[u8],
        now: Instant,
        store: &mut impl Store,
    ) -> Result<Value, Error> {
        if self.paired {
            return Err(Error::Busy);
        }
        let request = serde_json::from_slice(data).map_err(|_| Error::Invalid)?;
        if self
            .challenge
            .as_ref()
            .is_some_and(|challenge| now >= challenge.deadline)
        {
            self.challenge = None;
        }
        match request {
            Request::Begin => self.begin(now, store.credential().is_some()),
            Request::Prepare {
                challenge_id,
                iphone_public,
                iphone_nonce,
            } => {
                if store.credential().is_some() {
                    return Err(Error::AlreadyPaired);
                }
                let challenge = self
                    .challenge
                    .as_mut()
                    .filter(|c| c.id == challenge_id)
                    .ok_or(Error::Closed)?;
                if challenge.prepared.is_some() {
                    return Err(Error::Busy);
                }
                let phone = Phone::decode(&iphone_public, &iphone_nonce)?;
                let (key, code) = derive(&self.identity, challenge, &phone)?;
                let response = json!({"version": 1, "op": "prepare", "sas": code});
                challenge.prepared = Some(Prepared {
                    phone,
                    key,
                    code,
                    approved: false,
                });
                Ok(response)
            }
            Request::Confirm {
                challenge_id,
                iphone_public,
                iphone_nonce,
                sas,
                reconnect_tag,
            } => {
                if self.challenge.as_ref().is_none_or(|c| c.id != challenge_id) {
                    return Err(Error::Closed);
                }
                let phone = Phone::decode(&iphone_public, &iphone_nonce)?;
                let mut challenge = self.challenge.take().ok_or(Error::Closed)?;
                let (key, code, previous) = match (sas, reconnect_tag, store.credential()) {
                    (None, Some(tag), Some(credential)) => {
                        let tag = decode_base64url(&tag).map_err(|_| Error::Reconnect)?;
                        if tag.len() != 16 {
                            return Err(Error::Reconnect);
                        }
                        let transcript = transcript(&self.identity, &challenge, &phone)?;
                        let previous = credential
                            .authenticated_key(&transcript, &tag)
                            .ok_or(Error::Reconnect)?;
                        let (key, code) = derive(&self.identity, &challenge, &phone)?;
                        (key, code, Some(previous))
                    }
                    (None, Some(_), None) => return Err(Error::Unpaired),
                    (Some(_), None, Some(_)) => return Err(Error::AlreadyPaired),
                    (Some(sas), None, None) => {
                        let prepared = challenge.prepared.take().ok_or(Error::Confirmation)?;
                        if prepared.phone != phone || prepared.code != sas {
                            return Err(Error::Confirmation);
                        }
                        if !prepared.approved {
                            return Err(Error::Approval);
                        }
                        (prepared.key, prepared.code, None)
                    }
                    _ => return Err(Error::Invalid),
                };
                let tag = mac(&key, b"AirSIM/VoBLE/v1/ready", challenge_id.as_bytes())
                    .finalize()
                    .into_bytes();
                store.save(Credential {
                    current: key,
                    previous,
                })?;
                self.paired = true;
                Ok(
                    json!({"version": 1, "op": "confirm", "ready": true, "sas": code,
                          "confirmation_tag": base64url(&tag[..16])}),
                )
            }
        }
    }

    fn begin(&mut self, now: Instant, reconnect: bool) -> Result<Value, Error> {
        if self.challenge.is_some() {
            return Err(Error::Busy);
        }
        let deadline = now.checked_add(PAIRING_WINDOW).ok_or(Error::Unavailable)?;
        let deadline = if reconnect {
            deadline
        } else {
            deadline.min(
                self.enrollment_deadline
                    .filter(|d| now < *d)
                    .ok_or(Error::Closed)?,
            )
        };
        let mut private = Zeroizing::new([0; 32]);
        let mut nonce = [0; 32];
        let mut id = [0; 16];
        getrandom::fill(&mut *private).map_err(|_| Error::Unavailable)?;
        getrandom::fill(&mut nonce).map_err(|_| Error::Unavailable)?;
        getrandom::fill(&mut id).map_err(|_| Error::Unavailable)?;
        let secret = StaticSecret::from(*private);
        let id = hex(&id);
        let result = json!({"version": 1, "op": "begin", "challenge_id": id,
                            "android_identity": self.identity, "android_public": base64url(PublicKey::from(&secret).as_bytes()),
                            "android_nonce": base64url(&nonce)});
        self.challenge = Some(Challenge {
            id,
            secret,
            nonce,
            deadline,
            prepared: None,
        });
        Ok(result)
    }
}

fn mac(key: &[u8; 32], label: &[u8], data: &[u8]) -> Hmac<Sha256> {
    let mut mac = Hmac::new_from_slice(key).expect("HMAC accepts 32-byte keys");
    mac.update(label);
    mac.update(data);
    mac
}

fn transcript(identity: &str, challenge: &Challenge, phone: &Phone) -> Result<Vec<u8>, Error> {
    let public = PublicKey::from(&challenge.secret);
    let mut transcript = Vec::new();
    if identity.is_empty() {
        return Err(Error::Invalid);
    }
    for field in [
        &[1][..],
        identity.as_bytes(),
        public.as_bytes(),
        &phone.public,
        &challenge.nonce,
        &phone.nonce,
    ] {
        let len = u16::try_from(field.len()).map_err(|_| Error::Invalid)?;
        transcript.extend_from_slice(&len.to_be_bytes());
        transcript.extend_from_slice(field);
    }
    Ok(transcript)
}

fn derive(identity: &str, challenge: &Challenge, phone: &Phone) -> Result<(Key, String), Error> {
    let transcript = transcript(identity, challenge, phone)?;
    let shared = challenge
        .secret
        .diffie_hellman(&PublicKey::from(phone.public));
    if !shared.was_contributory() {
        return Err(Error::Confirmation);
    }
    let mut key = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(&Sha256::digest(&transcript)), shared.as_bytes())
        .expand(b"AirSIM/VoBLE/v1/pairing", &mut *key)
        .map_err(|_| Error::Unavailable)?;
    let digest = mac(&key, b"AirSIM/VoBLE/v1/sas", &transcript)
        .finalize()
        .into_bytes();
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000;
    Ok((key, format!("{number:06}")))
}

#[cfg(test)]
mod tests;
