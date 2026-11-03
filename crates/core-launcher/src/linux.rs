// SPDX-License-Identifier: AGPL-3.0-only
use crate::{profile_arguments, Error, CONTRACT, CORE, PRODUCT_PROFILE};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    os::unix::{fs::MetadataExt, fs::OpenOptionsExt, process::CommandExt},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn owned_file(path: &Path, limit: u64) -> Result<fs::File, Error> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|_| Error)?;
    let meta = file.metadata().map_err(|_| Error)?;
    if !meta.is_file()
        || meta.uid() != 0
        || meta.gid() != 0
        || meta.mode() & 0o6022 != 0
        || meta.nlink() != 1
        || meta.len() > limit
    {
        return Err(Error);
    }
    Ok(file)
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
fn metadata(program: &Path, budget: Duration) -> Result<Vec<u8>, Error> {
    let mut command = Command::new(program);
    command.arg("--build-info");
    metadata_command(command, budget)
}
fn metadata_command(mut command: Command, budget: Duration) -> Result<Vec<u8>, Error> {
    let mut child = Reap(
        command
            .env_clear()
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error)?,
    );
    let mut out = child.0.stdout.take().ok_or(Error)?;
    let flags = rustix::fs::fcntl_getfl(&out).map_err(|_| Error)?;
    rustix::fs::fcntl_setfl(&out, flags | rustix::fs::OFlags::NONBLOCK).map_err(|_| Error)?;
    let deadline = Instant::now() + budget;
    let mut bytes = Vec::new();
    let mut eof = false;
    loop {
        let mut part = [0; 4096];
        loop {
            match out.read(&mut part) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => {
                    if bytes.len() + n > 16384 {
                        return Err(Error);
                    }
                    bytes.extend_from_slice(&part[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error),
            }
        }
        if let Some(status) = child.0.try_wait().map_err(|_| Error)? {
            if !status.success() {
                return Err(Error);
            }
            if eof {
                return Ok(bytes);
            }
        }
        if Instant::now() >= deadline {
            return Err(Error);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
pub fn launch() -> Result<(), Error> {
    if rustix::process::geteuid().as_raw() != 986 || rustix::process::getegid().as_raw() != 986 {
        return Err(Error);
    }
    let mut core = owned_file(Path::new(CORE), 24 * 1024 * 1024)?;
    if core.metadata().map_err(|_| Error)?.mode() & 0o111 == 0 {
        return Err(Error);
    }
    let info = metadata(Path::new(CORE), Duration::from_secs(3))?;
    // A missing, malformed or unreadable optional AP contract cannot promote
    // into AP mode and does not prevent basic communication fallback.
    let contract = owned_file(Path::new(CONTRACT), 4096)
        .ok()
        .and_then(|mut file| {
            let mut bytes = Vec::new();
            file.by_ref().take(4097).read_to_end(&mut bytes).ok()?;
            (bytes.len() <= 4096).then_some(bytes)
        });
    // A present invalid opt-in profile is an error, never silent promotion or fallback.
    let profile = match fs::symlink_metadata(PRODUCT_PROFILE) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(Error),
        Ok(_) => {
            let mut file = owned_file(Path::new(PRODUCT_PROFILE), 4096)?;
            let mut bytes = Vec::new();
            file.by_ref()
                .take(4097)
                .read_to_end(&mut bytes)
                .map_err(|_| Error)?;
            if bytes.len() > 4096 {
                return Err(Error);
            }
            Some(bytes)
        }
    };
    let digest = if profile.is_some() {
        let mut bytes = Vec::new();
        core.by_ref()
            .take(24 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error)?;
        if bytes.len() as u64 != core.metadata().map_err(|_| Error)?.len() {
            return Err(Error);
        }
        format!("{:x}", Sha256::digest(&bytes))
    } else {
        String::new()
    };
    let args = profile_arguments(&info, contract.as_deref(), profile.as_deref(), &digest)?;
    let end = args
        .iter()
        .position(|v| v.starts_with("--serve-"))
        .ok_or(Error)?;
    if end > 0 {
        let mut probe = Command::new(CORE);
        probe.args(&args[..end]).arg("--build-info");
        // The actual binary must accept every selected prefix; metadata alone is insufficient.
        if metadata_command(probe, Duration::from_secs(3))? != info {
            return Err(Error);
        }
    }
    // Replace the launcher PID; no shell and no second long-lived process.
    let _error = Command::new(CORE).args(args).exec();
    Err(Error)
}
#[cfg(test)]
mod tests;
