// SPDX-License-Identifier: AGPL-3.0-only

use super::{mac, Error, Key};
use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use hkdf::Hkdf;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

pub const SAMPLES: usize = 160;
pub const PCM_BYTES: usize = SAMPLES * 2;
const TAG: usize = 16;
const CONTROL_HEADER: usize = 18;
const AUDIO_HEADER: usize = 18;
const MEDIA_AGE: Duration = Duration::from_millis(60);

pub fn clock_ms() -> Result<i64, Error> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Unavailable)?;
    i64::try_from(elapsed.as_millis()).map_err(|_| Error::Unavailable)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub call: String,
    pub generation: u64,
}

pub struct Command {
    pub name: String,
    pub target: Target,
    pub body: Value,
}

pub struct Channel {
    key: Key,
    session: Option<[u8; 16]>,
    nonces: HashMap<[u8; 12], i64>,
}

impl Channel {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            key: Zeroizing::new(*key),
            session: None,
            nonces: HashMap::new(),
        }
    }

    pub fn established(&self) -> bool {
        self.session.is_some()
    }

    pub fn request(&mut self, raw: &[u8], now: i64) -> Result<Command, Error> {
        if raw.len() < CONTROL_HEADER + TAG || raw.len() > 4096 || raw[..4] != [0xc3, 0x7e, 1, 2] {
            return Err(Error::Invalid);
        }
        let generation = u64::from_be_bytes(raw[4..12].try_into().map_err(|_| Error::Invalid)?);
        if generation >= i64::MAX as u64 || raw[12] != 16 || raw[13] != 12 {
            return Err(Error::Invalid);
        }
        let (name_len, call_len) = (usize::from(raw[14]), usize::from(raw[15]));
        let body_len = usize::from(u16::from_be_bytes([raw[16], raw[17]]));
        let body_at = CONTROL_HEADER + 16 + 12 + name_len + call_len;
        if name_len == 0
            || name_len > 64
            || call_len > 160
            || body_len > 3072
            || raw.len() != body_at + body_len + TAG
        {
            return Err(Error::Invalid);
        }
        let session: [u8; 16] = raw[18..34].try_into().map_err(|_| Error::Invalid)?;
        let nonce: [u8; 12] = raw[34..46].try_into().map_err(|_| Error::Invalid)?;
        if self.session.is_some_and(|known| known != session) {
            return Err(Error::Confirmation);
        }
        let name = std::str::from_utf8(&raw[46..46 + name_len])
            .map_err(|_| Error::Invalid)?
            .to_owned();
        let call = std::str::from_utf8(&raw[46 + name_len..body_at])
            .map_err(|_| Error::Invalid)?
            .to_owned();
        let body: Value = serde_json::from_slice(&raw[body_at..body_at + body_len])
            .map_err(|_| Error::Invalid)?;
        let time = body["timestamp_ms"].as_i64().ok_or(Error::Invalid)?;
        if time < now.saturating_sub(30_000) || time > now.saturating_add(5_000) {
            return Err(Error::Confirmation);
        }
        let target = Target { call, generation };
        let key = derive(&self.key, &session, &target, b"AirSIM/VoBLE/v1/control")?;
        let (prefix, tag) = raw.split_at(raw.len() - TAG);
        use hmac::Mac;
        mac(&key, &auth_prefix(&nonce, time), prefix)
            .verify_truncated_left(tag)
            .map_err(|_| Error::Confirmation)?;
        self.nonces
            .retain(|_, timestamp| *timestamp >= now.saturating_sub(30_000));
        if self.nonces.len() >= 256 || self.nonces.contains_key(&nonce) {
            return Err(Error::Confirmation);
        }
        self.nonces.insert(nonce, time);
        self.session = Some(session);
        Ok(Command { name, target, body })
    }

    pub fn sign(&self, name: &str, target: &Target, mut body: Value) -> Result<Vec<u8>, Error> {
        let session = self.session.ok_or(Error::Confirmation)?;
        let now = clock_ms()?;
        body["timestamp_ms"] = now.into();
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce).map_err(|_| Error::Unavailable)?;
        encode(&self.key, &session, name, target, body, now, &nonce)
    }

    pub fn media(&self, target: &Target) -> Result<Media, Error> {
        let session = self.session.ok_or(Error::Confirmation)?;
        let key = derive(&self.key, &session, target, b"AirSIM/VoBLE/v1/media")?;
        Ok(Media {
            cipher: Aes256Gcm::new_from_slice(&key[..]).map_err(|_| Error::Unavailable)?,
            rx: None,
            partial: None,
            sequence: 0,
        })
    }
}

fn auth_prefix(nonce: &[u8; 12], time: i64) -> [u8; 22] {
    let mut prefix = [0; 22];
    prefix[..2].copy_from_slice(&12u16.to_be_bytes());
    prefix[2..14].copy_from_slice(nonce);
    prefix[14..].copy_from_slice(&time.to_be_bytes());
    prefix
}

fn derive(key: &Key, session: &[u8; 16], target: &Target, label: &[u8]) -> Result<Key, Error> {
    let call = target.call.as_bytes();
    let length = u16::try_from(call.len()).map_err(|_| Error::Invalid)?;
    let mut hash = Sha256::new();
    hash.update(16u16.to_be_bytes());
    hash.update(session);
    hash.update(length.to_be_bytes());
    hash.update(call);
    hash.update(target.generation.to_be_bytes());
    let mut derived = Zeroizing::new([0; 32]);
    Hkdf::<Sha256>::new(Some(&hash.finalize()), &key[..])
        .expand(label, &mut derived[..])
        .map_err(|_| Error::Unavailable)?;
    Ok(derived)
}

fn encode(
    pairing: &Key,
    session: &[u8; 16],
    name: &str,
    target: &Target,
    body: Value,
    now: i64,
    nonce: &[u8; 12],
) -> Result<Vec<u8>, Error> {
    let json = serde_json::to_vec(&body).map_err(|_| Error::Invalid)?;
    let length = u16::try_from(json.len()).map_err(|_| Error::Invalid)?;
    let name_len = u8::try_from(name.len()).map_err(|_| Error::Invalid)?;
    let call_len = u8::try_from(target.call.len()).map_err(|_| Error::Invalid)?;
    let mut raw =
        Vec::with_capacity(CONTROL_HEADER + 28 + name.len() + target.call.len() + json.len() + TAG);
    raw.extend_from_slice(&[0xc3, 0x7e, 1, 2]);
    raw.extend_from_slice(&target.generation.to_be_bytes());
    raw.extend_from_slice(&[16, 12, name_len, call_len]);
    raw.extend_from_slice(&length.to_be_bytes());
    raw.extend_from_slice(session);
    raw.extend_from_slice(nonce);
    raw.extend_from_slice(name.as_bytes());
    raw.extend_from_slice(target.call.as_bytes());
    raw.extend_from_slice(&json);
    let key = derive(pairing, session, target, b"AirSIM/VoBLE/v1/control")?;
    use hmac::Mac;
    let tag = mac(&key, &auth_prefix(nonce, now), &raw)
        .finalize()
        .into_bytes();
    raw.extend_from_slice(&tag[..TAG]);
    Ok(raw)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Packet {
    flags: u8,
    stream: u8,
    seq: u32,
    stamp: u32,
    count: u8,
}

impl Packet {
    fn nonce(self, direction: u8) -> [u8; 12] {
        let mut bytes = [direction, self.stream, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        bytes[4..8].copy_from_slice(&self.stamp.to_be_bytes());
        bytes[8..].copy_from_slice(&self.seq.to_be_bytes());
        bytes
    }

    fn aad(self, direction: u8) -> [u8; 11] {
        let mut bytes = [direction, 1, self.stream, 0, 0, 0, 0, 0, 0, 0, 0];
        bytes[3..7].copy_from_slice(&self.seq.to_be_bytes());
        bytes[7..].copy_from_slice(&self.stamp.to_be_bytes());
        bytes
    }
}

struct AudioPartial {
    packet: Packet,
    index: u8,
    data: Vec<u8>,
    since: Instant,
}

pub struct Media {
    cipher: Aes256Gcm,
    rx: Option<u32>,
    partial: Option<AudioPartial>,
    sequence: u32,
}

impl Media {
    pub fn uplink(&mut self, bytes: &[u8], now: Instant) -> Result<Option<[u8; PCM_BYTES]>, Error> {
        if bytes.len() <= AUDIO_HEADER
            || bytes[..4] != [0xa5, 0x5a, 1, 1]
            || bytes[5] != 1
            || bytes[6] != 1
            || bytes[17] as usize != bytes.len() - AUDIO_HEADER
        {
            return Err(Error::Invalid);
        }
        let packet = Packet {
            flags: bytes[4],
            stream: bytes[6],
            seq: u32::from_be_bytes(bytes[7..11].try_into().map_err(|_| Error::Invalid)?),
            stamp: u32::from_be_bytes(bytes[11..15].try_into().map_err(|_| Error::Invalid)?),
            count: bytes[16],
        };
        let index = bytes[15];
        if packet.count > 64
            || index >= packet.count
            || self.rx.is_some_and(|seq| packet.seq <= seq)
        {
            return Err(Error::Invalid);
        }
        if index == 0 {
            self.partial = Some(AudioPartial {
                packet,
                index: 0,
                since: now,
                data: Vec::with_capacity(SAMPLES + TAG),
            });
        }
        let mut partial = self.partial.take().ok_or(Error::Invalid)?;
        if partial.packet != packet
            || partial.index != index
            || now.duration_since(partial.since) >= MEDIA_AGE
            || partial.data.len() + bytes.len() - AUDIO_HEADER > SAMPLES + TAG
        {
            return Err(Error::Invalid);
        }
        partial.data.extend_from_slice(&bytes[AUDIO_HEADER..]);
        if index + 1 < packet.count {
            partial.index += 1;
            self.partial = Some(partial);
            return Ok(None);
        }
        let plain = self
            .cipher
            .decrypt(
                Nonce::from_slice(&packet.nonce(1)),
                Payload {
                    msg: &partial.data,
                    aad: &packet.aad(1),
                },
            )
            .map_err(|_| Error::Confirmation)?;
        if plain.len() != SAMPLES {
            return Err(Error::Invalid);
        }
        self.rx = Some(packet.seq);
        let mut pcm = [0; PCM_BYTES];
        for (sample, out) in plain.into_iter().zip(pcm.as_chunks_mut::<2>().0.iter_mut()) {
            out.copy_from_slice(&decode(sample).to_le_bytes());
        }
        Ok(Some(pcm))
    }

    pub fn skip_downlink(&mut self, frames: u64) -> Result<(), Error> {
        let frames = u32::try_from(frames).map_err(|_| Error::Closed)?;
        self.sequence = self.sequence.checked_add(frames).ok_or(Error::Closed)?;
        Ok(())
    }

    pub fn downlink(
        &mut self,
        pcm: &[u8; PCM_BYTES],
        packet_size: usize,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let seq = self.sequence;
        self.sequence = seq.checked_add(1).ok_or(Error::Closed)?;
        let capacity = packet_size.saturating_sub(AUDIO_HEADER).min(255);
        if capacity < 3 {
            return Err(Error::Invalid);
        }
        let count = (SAMPLES + TAG).div_ceil(capacity) as u8;
        let packet = Packet {
            flags: 0,
            stream: 2,
            seq,
            stamp: seq.wrapping_mul(SAMPLES as u32),
            count,
        };
        let mut law = [0; SAMPLES];
        for (sample, out) in pcm.as_chunks::<2>().0.iter().zip(law.iter_mut()) {
            *out = encode_law(i16::from_le_bytes([sample[0], sample[1]]));
        }
        let sealed = self
            .cipher
            .encrypt(
                Nonce::from_slice(&packet.nonce(2)),
                Payload {
                    msg: &law,
                    aad: &packet.aad(2),
                },
            )
            .map_err(|_| Error::Unavailable)?;
        let mut frames = Vec::with_capacity(count.into());
        for (index, data) in sealed.chunks(capacity).enumerate() {
            let mut bytes = vec![0xa5, 0x5a, 1, 1, packet.flags, 1, packet.stream];
            bytes.extend_from_slice(&packet.seq.to_be_bytes());
            bytes.extend_from_slice(&packet.stamp.to_be_bytes());
            bytes.extend_from_slice(&[index as u8, count, data.len() as u8]);
            bytes.extend_from_slice(data);
            frames.push(bytes);
        }
        Ok(frames)
    }
}

fn encode_law(sample: i16) -> u8 {
    let sign = if sample < 0 { 0x80 } else { 0 };
    let sample = u32::from(sample.unsigned_abs()).min(32635) + 132;
    let exponent = (31 - sample.leading_zeros()).saturating_sub(7);
    let mantissa = (sample >> (exponent + 3)) & 15;
    !(sign | (exponent as u8) << 4 | mantissa as u8)
}

fn decode(value: u8) -> i16 {
    let bits = i16::from(!value);
    let magnitude = ((((bits & 15) << 3) + 132) << ((bits >> 4) & 7)) - 132;
    if bits & 0x80 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests;
