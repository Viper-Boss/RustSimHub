// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed stop/query/reload transport. Never start/enable, never a generic shell.
use super::{Error, Result};
use std::{
    io::Read,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const UNITS: [&str; 6] = [
    "rustsimhub-core.service",
    "rustsimhub-edge.service",
    "rustsimhub-media.service",
    "rustsimhub-forwarder.service",
    "rustsimhub-nkn-root.service",
    "rustsimhub-ble-recovery.service",
];
const PROPS: [&str; 4] = ["LoadState", "ActiveState", "SubState", "MainPID"];

pub(super) trait Services {
    fn prepare_off(&self) -> Result<()>;
    fn revalidate_off(&self) -> Result<()>;
    fn reload(&self) -> Result<()>;
}
pub(super) struct Systemd;
impl Services for Systemd {
    fn prepare_off(&self) -> Result<()> {
        // Core/Edge/Media/forwarder must already be stopped by the retained
        // maintenance lease. Only optional workers are stopped by this layer.
        for index in 0..4 {
            if !query(index)? {
                return Err(Error::Denied);
            }
        }
        for (index, unit) in UNITS.iter().enumerate().skip(4) {
            if !query(index)? {
                let (code, _) = capture(&["--system", "stop", unit], Duration::from_secs(5))?;
                if code != 0 {
                    return Err(Error::Unknown);
                }
                if !query(index)? {
                    return Err(Error::Unknown);
                }
            }
        }
        Ok(())
    }
    fn revalidate_off(&self) -> Result<()> {
        for index in 0..6 {
            if !query(index)? {
                return Err(Error::Denied);
            }
        }
        Ok(())
    }
    fn reload(&self) -> Result<()> {
        let (code, _) = capture(&["--system", "daemon-reload"], Duration::from_secs(5))?;
        if code != 0 {
            return Err(Error::Unknown);
        }
        self.revalidate_off()
    }
}
fn query(index: usize) -> Result<bool> {
    let unit = UNITS.get(index).ok_or(Error::Invalid)?;
    let mut arguments = vec!["--system", "show", "--all", "--no-pager"];
    for name in PROPS {
        arguments.extend(["-p", name]);
    }
    arguments.push(unit);
    let (code, bytes) = capture(&arguments, Duration::from_secs(1))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid)?;
    let mut fields = std::collections::BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or(Error::Invalid)?;
        if !PROPS.contains(&key) || fields.insert(key, value).is_some() {
            return Err(Error::Invalid);
        }
    }
    if fields.len() != PROPS.len() {
        return Err(Error::Invalid);
    }
    let absent = fields["LoadState"] == "not-found"
        && fields["ActiveState"] == "inactive"
        && fields["SubState"] == "dead"
        && fields["MainPID"] == "0";
    if index >= 4 && absent && matches!(code, 0 | 1) {
        return Ok(true);
    }
    if code != 0 || fields["LoadState"] != "loaded" {
        return Err(Error::Denied);
    }
    Ok(fields["ActiveState"] == "inactive"
        && fields["SubState"] == "dead"
        && fields["MainPID"] == "0")
}
struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn capture(arguments: &[&str], budget: Duration) -> Result<(i32, Vec<u8>)> {
    let mut child = Reap(
        Command::new("/usr/bin/systemctl")
            .args(arguments)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error::Io)?,
    );
    let mut pipe = child.0.stdout.take().ok_or(Error::Io)?;
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(|_| Error::Io)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK).map_err(|_| Error::Io)?;
    let deadline = Instant::now() + budget;
    let mut output = Vec::new();
    let mut eof = false;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        let mut buffer = [0; 1024];
        match pipe.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(n) => {
                if output.len() + n > 4096 {
                    return Err(Error::Invalid);
                }
                output.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err(Error::Io),
        }
        if let Some(status) = child.0.try_wait().map_err(|_| Error::Io)? {
            if eof {
                return Ok((status.code().ok_or(Error::Unknown)?, output));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
