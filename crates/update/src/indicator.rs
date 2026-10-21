// SPDX-License-Identifier: AGPL-3.0-only
//! Best-effort, short-lived LED hint around a verified Root update operation.
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::mpsc,
    thread,
    time::Duration,
};
const DIR: &str = "/run/rustsimhub-led-update";
fn write(active: bool) -> std::io::Result<()> {
    let path = Path::new(DIR);
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != 0 || m.mode() & 0o777 != 0o700 {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    let target = path.join("state.json");
    if let Ok(m) = fs::symlink_metadata(&target) {
        if !m.is_file() || m.uid() != 0 || m.mode() & 0o777 != 0o600 || m.nlink() != 1 {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
    }
    let uptime = fs::read_to_string("/proc/uptime")?;
    let seconds = uptime
        .split('.')
        .next()
        .unwrap_or("")
        .parse::<u64>()
        .map_err(|_| std::io::ErrorKind::InvalidData)?;
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| std::io::ErrorKind::Other)?;
    let name: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
    let temp = path.join(format!(".update-{name}.pending"));
    let data = serde_json::to_vec(
        &serde_json::json!({"expires_ms":seconds.saturating_mul(1000).saturating_add(10_000),"available":false,"upgrading":active}),
    )?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(&data)?;
    fs::rename(temp, target)
}
pub struct Activity {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Activity {
    pub fn begin() -> Self {
        if write(true).is_err() {
            return Self {
                stop: None,
                thread: None,
            };
        }
        let (send, receive) = mpsc::channel();
        let worker = thread::spawn(move || {
            while let Err(mpsc::RecvTimeoutError::Timeout) =
                receive.recv_timeout(Duration::from_secs(3))
            {
                if write(true).is_err() {
                    break;
                }
            }
        });
        Self {
            stop: Some(send),
            thread: Some(worker),
        }
    }
}
impl Drop for Activity {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
            let _ = write(false);
        }
    }
}
