// SPDX-License-Identifier: AGPL-3.0-only
use super::{Error, Result};
use rustsimhub_wire::update_health::{MAX_FRAME, READ_MS};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};
fn fresh(until: Instant) -> Result<Duration> {
    let remaining = until
        .checked_duration_since(Instant::now())
        .ok_or(Error::Busy)?;
    if remaining.is_zero() {
        return Err(Error::Busy);
    }
    Ok(remaining.min(Duration::from_millis(READ_MS)))
}
/// Idle polls never extend the absolute observation. Once a frame starts, the
/// entire header/body must complete inside one 250ms budget, not 250ms per byte.
pub(super) fn read(stream: &mut UnixStream, until: Instant, idle: bool) -> Result<Vec<u8>> {
    let initial = if idle {
        until
    } else {
        until.min(Instant::now() + Duration::from_millis(READ_MS))
    };
    let mut header = [0u8; 4];
    loop {
        stream
            .set_read_timeout(Some(fresh(initial)?))
            .map_err(|_| Error::Io)?;
        match stream.read(&mut header[..1]) {
            Ok(0) => return Err(Error::Integrity),
            Ok(_) => break,
            Err(e)
                if idle
                    && matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
            {
                continue
            }
            Err(_) => return Err(Error::Io),
        }
    }
    let frame_until = initial.min(Instant::now() + Duration::from_millis(READ_MS));
    exact(stream, &mut header[1..], frame_until)?;
    let n = u32::from_be_bytes(header) as usize;
    if n == 0 || n > MAX_FRAME {
        return Err(Error::Invalid);
    }
    let mut body = vec![0; n];
    exact(stream, &mut body, frame_until)?;
    Ok(body)
}
fn exact(stream: &mut UnixStream, mut bytes: &mut [u8], until: Instant) -> Result<()> {
    while !bytes.is_empty() {
        stream
            .set_read_timeout(Some(fresh(until)?))
            .map_err(|_| Error::Io)?;
        let n = stream.read(bytes).map_err(|_| Error::Io)?;
        if n == 0 {
            return Err(Error::Integrity);
        }
        bytes = &mut bytes[n..];
    }
    fresh(until)?;
    Ok(())
}
pub(super) fn write(stream: &mut UnixStream, body: &[u8], until: Instant) -> Result<()> {
    if body.is_empty() || body.len() > MAX_FRAME {
        return Err(Error::Invalid);
    }
    let until = until.min(Instant::now() + Duration::from_millis(READ_MS));
    let mut frame = (body.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(body);
    let mut bytes = frame.as_slice();
    while !bytes.is_empty() {
        stream
            .set_write_timeout(Some(fresh(until)?))
            .map_err(|_| Error::Io)?;
        let n = stream.write(bytes).map_err(|_| Error::Io)?;
        if n == 0 {
            return Err(Error::Integrity);
        }
        bytes = &bytes[n..];
    }
    fresh(until)?;
    Ok(())
}
