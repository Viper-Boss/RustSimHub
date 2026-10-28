// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Minimal mDNS responder logic: answers IPv4 (A) questions for exactly one fixed
//! `<device-name>.local` and nothing else. Pure functions, no sockets; the binary
//! supplies the address facing the querier.
use std::net::Ipv4Addr;

pub const PORT: u16 = 5353;
pub const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// Bounded input: anything larger than one Ethernet payload is ignored.
pub const MAX_PACKET: usize = 1500;
const MAX_QUESTIONS: u16 = 16;
const MAX_POINTERS: usize = 8;
const TTL_SECONDS: u32 = 120;
const TYPE_A: u16 = 1;
const TYPE_ANY: u16 = 255;
const CLASS_IN: u16 = 1;
const UNICAST_RESPONSE: u16 = 0x8000;
const CACHE_FLUSH: u16 = 0x8000;

/// `rustsimhub-` followed by 4 lowercase alphanumerics, e.g. `rustsimhub-0087`.
pub fn valid_device_name(name: &str) -> bool {
    name.strip_prefix("rustsimhub-").is_some_and(|tail| {
        tail.len() == 4
            && tail
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
    })
}

/// What to send for one query packet.
#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub packet: Vec<u8>,
    /// True when the querier asked for a unicast reply (QU bit) on every matching question.
    pub unicast: bool,
}

/// Parses one mDNS packet and, if it asks for `<name>.local` A/ANY, builds the answer
/// carrying `address`. Responses, malformed or unrelated packets yield `None`.
pub fn answer(packet: &[u8], name: &str, address: Ipv4Addr) -> Option<Reply> {
    if !valid_device_name(name) || packet.len() < 12 || packet.len() > MAX_PACKET {
        return None;
    }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    // Only standard queries: QR=0, opcode 0, no truncation games.
    if flags & 0xF800 != 0 {
        return None;
    }
    let questions = u16::from_be_bytes([packet[4], packet[5]]);
    if questions == 0 || questions > MAX_QUESTIONS {
        return None;
    }
    let mut offset = 12;
    let mut matched = false;
    let mut unicast = true;
    for _ in 0..questions {
        let (labels, next) = read_name(packet, offset)?;
        let fixed = packet.get(next..next + 4)?;
        offset = next + 4;
        let qtype = u16::from_be_bytes([fixed[0], fixed[1]]);
        let qclass = u16::from_be_bytes([fixed[2], fixed[3]]);
        if is_ours(&labels, name)
            && (qtype == TYPE_A || qtype == TYPE_ANY)
            && qclass & !UNICAST_RESPONSE == CLASS_IN
        {
            matched = true;
            unicast &= qclass & UNICAST_RESPONSE != 0;
        }
    }
    if !matched || address.is_unspecified() || address.is_loopback() {
        return None;
    }
    Some(Reply {
        packet: response(name, address),
        unicast,
    })
}

/// Reads a (possibly compressed) name at `offset`; returns lowercase labels and the
/// offset just after the name in the original position. Bounded pointer chasing.
fn read_name(packet: &[u8], mut offset: usize) -> Option<(Vec<String>, usize)> {
    let mut labels = Vec::new();
    let mut end = None;
    let mut jumps = 0;
    let mut total = 0;
    loop {
        let len = *packet.get(offset)? as usize;
        match len & 0xC0 {
            0x00 => {
                if len == 0 {
                    return Some((labels, end.unwrap_or(offset + 1)));
                }
                let label = packet.get(offset + 1..offset + 1 + len)?;
                total += len + 1;
                if total > 255 || labels.len() >= 8 {
                    return None;
                }
                labels.push(String::from_utf8(label.to_ascii_lowercase()).ok()?);
                offset += 1 + len;
            }
            0xC0 => {
                let low = *packet.get(offset + 1)? as usize;
                let target = ((len & 0x3F) << 8) | low;
                jumps += 1;
                // Pointers must go strictly backwards: no loops, no forward references.
                if jumps > MAX_POINTERS || target >= offset {
                    return None;
                }
                end.get_or_insert(offset + 2);
                offset = target;
            }
            _ => return None,
        }
    }
}

fn is_ours(labels: &[String], name: &str) -> bool {
    labels.len() == 2 && labels[0] == name && labels[1] == "local"
}

fn response(name: &str, address: Ipv4Addr) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    // ID 0, QR=1 + AA=1, no questions, one answer.
    out.extend_from_slice(&[0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0]);
    for label in [name, "local"] {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    out.extend_from_slice(&TYPE_A.to_be_bytes());
    out.extend_from_slice(&(CLASS_IN | CACHE_FLUSH).to_be_bytes());
    out.extend_from_slice(&TTL_SECONDS.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&address.octets());
    out
}

/// Addresses we are willing to announce: private, link-local or the fixed setup AP.
pub fn announceable(address: Ipv4Addr) -> bool {
    address.is_private() || address.is_link_local()
}

#[cfg(test)]
mod tests;
