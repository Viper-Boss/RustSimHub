// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed UFI003 runtime NAS voice preference, through the existing Root QMI proxy.
//! Protocol reference: AirSimHub ea019485, node/agent/cmd/nassimhub-voice-domain.
//! No raw device ownership, arbitrary message/port input, persistent NV or APN writes.
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    os::{
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixStream},
    },
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Fields = BTreeMap<u8, Vec<u8>>;
const NAS: u8 = 3;
const PORT: &[u8] = b"/dev/wwan0qmi0";

fn word(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}
fn fields(mut bytes: &[u8]) -> Result<Fields> {
    let mut out = BTreeMap::new();
    while !bytes.is_empty() {
        if bytes.len() < 3 {
            return Err("short QMI TLV".into());
        }
        let size = usize::from(word(&bytes[1..3]));
        if size > bytes.len() - 3 || out.insert(bytes[0], bytes[3..3 + size].to_vec()).is_some() {
            return Err("invalid or duplicate QMI TLV".into());
        }
        bytes = &bytes[3 + size..];
    }
    if out.get(&2).map(Vec::as_slice) != Some(&[0, 0, 0, 0]) {
        return Err("QMI result missing or failed".into());
    }
    Ok(out)
}
fn remaining(end: Instant) -> io::Result<Duration> {
    end.checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "QMI request expired"))
}
struct Transport {
    socket: UnixStream,
    transaction: u16,
    budget: Duration,
}
impl Transport {
    fn read(&mut self, mut bytes: &mut [u8], end: Instant) -> Result<()> {
        while !bytes.is_empty() {
            self.socket.set_read_timeout(Some(remaining(end)?))?;
            let size = self.socket.read(bytes)?;
            if size == 0 {
                return Err("QMI proxy closed".into());
            }
            bytes = &mut bytes[size..];
        }
        Ok(())
    }
    fn request(&mut self, service: u8, client: u8, message: u16, body: &[u8]) -> Result<Fields> {
        if body.len() > 512 {
            return Err("QMI request too large".into());
        }
        self.transaction = self
            .transaction
            .checked_add(1)
            .ok_or("QMI transaction exhausted")?;
        let transaction = if service == 0 {
            self.transaction & 255
        } else {
            self.transaction
        };
        let header = if service == 0 { 6 } else { 7 };
        let mut packet = vec![0; 6 + header + body.len()];
        packet[0] = 1;
        let size = u16::try_from(packet.len() - 1)?;
        packet[1..3].copy_from_slice(&size.to_le_bytes());
        packet[4] = service;
        packet[5] = client;
        let qmi = &mut packet[6..];
        if service == 0 {
            qmi[1] = transaction as u8;
        } else {
            qmi[1..3].copy_from_slice(&transaction.to_le_bytes());
        }
        qmi[header - 4..header - 2].copy_from_slice(&message.to_le_bytes());
        qmi[header - 2..header].copy_from_slice(&(body.len() as u16).to_le_bytes());
        qmi[header..].copy_from_slice(body);
        let end = Instant::now() + self.budget;
        let mut bytes = packet.as_slice();
        while !bytes.is_empty() {
            self.socket.set_write_timeout(Some(remaining(end)?))?;
            let size = self.socket.write(bytes)?;
            if size == 0 {
                return Err("QMI short write".into());
            }
            bytes = &bytes[size..];
        }
        for _ in 0..16 {
            let mut prefix = [0; 3];
            self.read(&mut prefix, end)?;
            let size = usize::from(word(&prefix[1..3]));
            if prefix[0] != 1 || !(11..=4096).contains(&size) {
                return Err("invalid QMUX frame".into());
            }
            let mut frame = vec![0; size + 1];
            frame[..3].copy_from_slice(&prefix);
            self.read(&mut frame[3..], end)?;
            if frame[3] != 0x80 {
                return Err("invalid QMUX direction".into());
            }
            if frame[4] != service || frame[5] != client {
                continue;
            }
            let qmi = &frame[6..];
            if qmi.len() < header {
                return Err("short QMI header".into());
            }
            let got = if service == 0 {
                u16::from(qmi[1])
            } else {
                word(&qmi[1..3])
            };
            if qmi[0] != if service == 0 { 1 } else { 2 }
                || got != transaction
                || word(&qmi[header - 4..header - 2]) != message
            {
                continue;
            }
            if usize::from(word(&qmi[header - 2..header])) != qmi.len() - header {
                return Err("invalid QMI message size".into());
            }
            return fields(&qmi[header..]);
        }
        Err("no matching QMI response".into())
    }
}
fn domain(fields: &Fields) -> Result<u32> {
    let bytes: [u8; 4] = fields
        .get(&0x20)
        .ok_or("voice domain unsupported")?
        .as_slice()
        .try_into()?;
    Ok(u32::from_le_bytes(bytes))
}
fn setting(current: u32, desired: u32) -> Result<Option<Vec<u8>>> {
    if ![2, 3].contains(&current) || ![2, 3].contains(&desired) {
        return Err("unknown voice domain; no change".into());
    }
    if current == desired {
        return Ok(None);
    }
    // NAS change duration POWER_CYCLE=0; only voice-domain preference follows.
    let mut body = vec![0x17, 1, 0, 0, 0x23, 4, 0];
    body.extend_from_slice(&desired.to_le_bytes());
    Ok(Some(body))
}
pub(crate) fn run(operation: &str) -> Result<()> {
    let desired = match operation {
        "--voice-domain-get" => None,
        "--voice-domain-ps-preferred" => Some(3),
        "--voice-domain-cs-preferred" => Some(2),
        _ => return Err("unsupported voice operation".into()),
    };
    if rustix::process::geteuid().as_raw() != 0
        || std::env::consts::ARCH != "aarch64"
        || !std::fs::read("/proc/device-tree/compatible")?
            .split(|b| *b == 0)
            .any(|name| name == b"thwc,ufi003")
    {
        return Err("verified Root UFI003 required".into());
    }
    let address = SocketAddr::from_abstract_name(b"qmi-proxy")?;
    // This mode is a one-shot utility. A timed-out connect returns to main,
    // which exits the utility process; no blocked connector survives there.
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(UnixStream::connect_addr(&address));
    });
    let socket = receiver.recv_timeout(Duration::from_secs(5))??;
    if rustix::net::sockopt::socket_peercred(&socket)?.uid.as_raw() != 0 {
        return Err("untrusted QMI proxy peer".into());
    }
    let mut transport = Transport {
        socket,
        transaction: 0,
        budget: Duration::from_secs(5),
    };
    let mut open = vec![1, PORT.len() as u8, 0];
    open.extend_from_slice(PORT);
    transport.request(0, 0, 0xff00, &open)?;
    let allocation = transport.request(0, 0, 0x22, &[1, 1, 0, NAS])?;
    let allocation = allocation.get(&1).ok_or("missing NAS client")?;
    if allocation.len() != 2 || allocation[0] != NAS || allocation[1] == 0 {
        return Err("invalid NAS client".into());
    }
    let client = allocation[1];
    let result: Result<()> = (|| {
        let current = domain(&transport.request(NAS, client, 0x34, &[])?)?;
        println!("voice-domain={current}");
        if let Some(desired) = desired {
            if let Some(body) = setting(current, desired)? {
                // One mutation only. An uncertain response is never retried.
                transport.request(NAS, client, 0x33, &body)?;
                if domain(&transport.request(NAS, client, 0x34, &[])?)? != desired {
                    return Err("voice domain readback failed".into());
                }
                println!("runtime voice domain requested={desired}");
            }
        }
        Ok(())
    })();
    let released = transport.request(0, 0, 0x23, &[1, 2, 0, NAS, client]);
    result?;
    released?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const OK: &[u8] = &[2, 4, 0, 0, 0, 0, 0];
    #[test]
    fn malformed_duplicate_and_failed_results_are_denied() {
        for bytes in [
            &[][..],
            &[2, 4, 0, 0],
            &[2, 4, 0, 1, 0, 0, 0],
            &[2, 4, 0, 0, 0, 1, 0],
            &[2, 4, 0, 0, 0, 0, 0, 2, 4, 0, 0, 0, 0, 0],
            &[2, 4, 0, 0, 0, 0, 0, 32, 9, 0, 2],
        ] {
            assert!(fields(bytes).is_err());
        }
        assert!(fields(OK).is_ok());
    }
    #[test]
    fn only_power_cycle_voice_tlvs_and_known_baselines_can_be_written() {
        assert_eq!(
            setting(2, 3).unwrap().unwrap(),
            [0x17, 1, 0, 0, 0x23, 4, 0, 3, 0, 0, 0]
        );
        assert_eq!(
            setting(3, 2).unwrap().unwrap(),
            [0x17, 1, 0, 0, 0x23, 4, 0, 2, 0, 0, 0]
        );
        assert!(setting(3, 3).unwrap().is_none());
        for value in [0, 1, 4, u32::MAX] {
            assert!(setting(value, 3).is_err());
            assert!(setting(2, value).is_err());
        }
    }
    #[test]
    fn domain_requires_exact_field_size() {
        assert!(domain(&fields(OK).unwrap()).is_err());
        let mut bytes = OK.to_vec();
        bytes.extend_from_slice(&[0x20, 4, 0, 2, 0, 0, 0]);
        assert_eq!(domain(&fields(&bytes).unwrap()).unwrap(), 2);
        bytes.pop();
        assert!(fields(&bytes).is_err());
    }
    fn reply(tx: u16) -> Vec<u8> {
        let mut p = vec![1, 23, 0, 0x80, 3, 4, 2];
        p.extend_from_slice(&tx.to_le_bytes());
        p.extend_from_slice(&[0x34, 0, 11, 0]);
        p.extend_from_slice(OK);
        p.extend_from_slice(&[0x20, 1, 0, 2]);
        p
    }
    #[test]
    fn fixed_request_ignores_wrong_transaction_and_matches_correct_response() {
        let (socket, mut peer) = UnixStream::pair().unwrap();
        let worker = std::thread::spawn(move || {
            let mut request = [0; 13];
            peer.read_exact(&mut request).unwrap();
            assert_eq!(request, [1, 12, 0, 0, 3, 4, 0, 1, 0, 0x34, 0, 0, 0]);
            peer.write_all(&reply(17)).unwrap();
            peer.write_all(&reply(1)).unwrap();
        });
        let mut t = Transport {
            socket,
            transaction: 0,
            budget: Duration::from_secs(1),
        };
        assert_eq!(
            t.request(3, 4, 0x34, &[]).unwrap().get(&0x20).unwrap(),
            &[2]
        );
        worker.join().unwrap();
    }
    #[test]
    fn trickled_header_cannot_extend_absolute_deadline() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let mut t = Transport {
            socket,
            transaction: 0,
            budget: Duration::from_millis(10),
        };
        let began = Instant::now();
        assert!(t.request(3, 4, 0x34, &[]).is_err());
        assert!(began.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn indication_flood_is_bounded_and_no_mutation_retry_occurs() {
        let (socket, mut peer) = UnixStream::pair().unwrap();
        let worker = std::thread::spawn(move || {
            let mut request = [0; 13];
            peer.read_exact(&mut request).unwrap();
            for _ in 0..16 {
                peer.write_all(&reply(17)).unwrap();
            }
        });
        let mut t = Transport {
            socket,
            transaction: 0,
            budget: Duration::from_secs(1),
        };
        assert!(t.request(3, 4, 0x34, &[]).is_err());
        assert_eq!(t.transaction, 1);
        worker.join().unwrap();
    }
}
