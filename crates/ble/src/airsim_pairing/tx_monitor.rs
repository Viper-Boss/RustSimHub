// SPDX-License-Identifier: AGPL-3.0-only

use std::{io, process::Stdio};
use tokio::{
    io::{AsyncReadExt, BufReader},
    process::{Child, ChildStderr, Command},
};

const MONITOR_HEADER: &[u8; 16] = b"btsnoop\0\0\0\0\x01\0\0\x07\xd1";
const HCI0_ACL_TX: [u8; 4] = [0, 0, 0, 4];
const PREFIX_LEN: usize = 20;

pub(super) struct TxMonitor {
    reader: BufReader<ChildStderr>,
    _child: Child,
}

impl TxMonitor {
    pub(super) async fn open() -> io::Result<Self> {
        // BlueZ writes binary records to the pipe. The human-readable output
        // goes to /dev/null and no capture file is created.
        let mut child = Command::new("/usr/bin/btmon")
            .args(["-P", "-i", "0", "-w", "/dev/stderr"])
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let pipe = child.stderr.take().ok_or(io::ErrorKind::BrokenPipe)?;
        let mut reader = BufReader::new(pipe);
        let mut header = [0; 16];
        reader.read_exact(&mut header).await?;
        if &header != MONITOR_HEADER {
            return Err(io::ErrorKind::InvalidData.into());
        }
        Ok(Self {
            reader,
            _child: child,
        })
    }

    pub(super) async fn submitted(&mut self, fragment: &[u8]) -> io::Result<()> {
        let mut header = [0; 24];
        let mut buf = [0; 4096];
        loop {
            self.reader.read_exact(&mut header).await?;
            let size = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
            let payload = buf.get_mut(..size).ok_or(io::ErrorKind::InvalidData)?;
            self.reader.read_exact(payload).await?;
            if header[8..12] == HCI0_ACL_TX && is_submission(payload, fragment) {
                return Ok(());
            }
        }
    }
}

fn is_submission(packet: &[u8], fragment: &[u8]) -> bool {
    if fragment.len() <= 18 {
        return false;
    }
    let prefix = &fragment[..fragment.len().min(PREFIX_LEN)];
    let Some(header) = packet.get(..11) else {
        return false;
    };
    // ACL continuation packets have no L2CAP or ATT header. Match the
    // connection's notification by its frame header and encrypted prefix.
    header[1] & 0x70 != 0x10
        && header[1] & 0xc0 == 0
        && usize::from(u16::from_le_bytes([header[2], header[3]])) + 4 == packet.len()
        && usize::from(u16::from_le_bytes([header[4], header[5]])) == fragment.len() + 3
        && header[6..9] == [4, 0, 0x1b]
        && packet[11..].starts_with(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_submitted_notification() {
        let fragment = [0xff; 182];
        // A fragmented ACL packet starts with the L2CAP and ATT headers.
        let mut acl = [0xff; 68];
        acl[..11].copy_from_slice(&[1, 0, 64, 0, 185, 0, 4, 0, 27, 23, 0]);
        assert!(is_submission(&acl, &fragment));
        for (index, value) in [(1, 0x10), (6, 5), (8, 0x52), (30, 0)] {
            let mut other = acl;
            other[index] = value;
            assert!(!is_submission(&other, &fragment));
        }
        assert!(!is_submission(&acl[..30], &fragment));
    }
}
