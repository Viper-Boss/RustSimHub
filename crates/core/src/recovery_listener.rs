// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit original Core/Auth/Store recovery listener, separate from NKN.
use rustsimhub_auth::{secure_session::SecureRecoverySessions, Auth};
use rustsimhub_core::{
    nkn_root::InstalledRoles,
    recovery_channel::{RecoveryChannel, SecureApplicationStep},
    recovery_transport::{Roles, SourceGuard},
};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::UnixListener, task::JoinSet};
const SOCKET: &str = "/run/rustsimhub/recovery/core.sock";
static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn activate() {
    ACTIVE.store(true, std::sync::atomic::Ordering::Release)
}
pub fn active() -> bool {
    ACTIVE.load(std::sync::atomic::Ordering::Acquire)
}
pub async fn run(auth: Arc<Mutex<Auth>>) -> Result<(), Box<dyn std::error::Error>> {
    if !active() {
        return std::future::pending().await;
    }
    let roles = InstalledRoles::read().map_err(|_| "installation roles unavailable")?;
    for p in ["/run", "/run/rustsimhub"] {
        let m = fs::symlink_metadata(p)?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err("untrusted recovery parent".into());
        }
    }
    let m = fs::symlink_metadata("/run/rustsimhub/recovery")?;
    if !m.is_dir()
        || m.uid() != roles.core
        || m.gid() != roles.ble
        || m.mode() & 0o7777 != 0o2750
        || fs::symlink_metadata(SOCKET).is_ok()
    {
        return Err("untrusted recovery socket directory".into());
    }
    let guard = SourceGuard::new(Roles::new(
        roles.edge,
        roles.forwarder,
        roles.media,
        roles.ble,
    )?)?;
    let sessions = Arc::new(SecureRecoverySessions::new());
    auth.lock()
        .map_err(|_| "Auth busy")?
        .set_recovery_verifier(Some(sessions.clone()))?;
    let listener = UnixListener::bind(SOCKET)?;
    let m = fs::symlink_metadata(SOCKET)?;
    struct PathGuard(u64, u64);
    impl Drop for PathGuard {
        fn drop(&mut self) {
            if fs::symlink_metadata(SOCKET).is_ok_and(|m| m.dev() == self.0 && m.ino() == self.1) {
                let _ = fs::remove_file(SOCKET);
            }
        }
    }
    let _path = PathGuard(m.dev(), m.ino());
    fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o660))?;
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {biased;_=tasks.join_next(),if !tasks.is_empty()=>{},result=listener.accept()=>{
            let (stream,_)=result?;let Ok(peer)=guard.accept(stream) else{continue};let a=auth.clone();let s=sessions.clone();
            tasks.spawn(async move{
                let channel=tokio::task::spawn_blocking({let a=a.clone();move||{let mut auth=a.try_lock().map_err(|_|rustsimhub_auth::Error::Busy)?;RecoveryChannel::attach(&mut auth,peer)}}).await.ok()?.ok()?;
                let mut channel=channel.establish_secure(a.clone(),s).await.ok()?;
                // Original cipher enforces 128 messages/300s. No per-request renewals.
                for _ in 0..128{match tokio::time::timeout(Duration::from_secs(300),channel.dispatch(a.clone())).await.ok()?.ok()?{SecureApplicationStep::Continue(next)=>channel=next,SecureApplicationStep::Closed=>return Some(())}}
                Some(())
            });
        }}
    }
}
