// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit candidate listener assembly; never called by the default Core.
use crate::{
    nkn_business::CorePorts,
    nkn_host::{self, stage, Error, NknSources},
};
use rustsimhub_auth::nkn_session::MembershipKey;
use rustsimhub_core::{
    nkn_root::{Controller, InstalledRoles},
    recovery_transport::{Kind, Packet},
};
use rustsimhub_nkn_agent::{Phase, PortError};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{net::UnixListener, task::JoinSet};
const SOCKET: &str = "/run/rustsimhub/nkn/core.sock";
static SOURCES: OnceLock<Result<Arc<NknSources>, Error>> = OnceLock::new();
fn registry() -> Result<Arc<NknSources>, Error> {
    SOURCES
        .get_or_init(|| {
            let r = InstalledRoles::read().map_err(|_| Error::Authentication)?;
            Ok(Arc::new(NknSources::new(
                r.edge,
                r.forwarder,
                r.media,
                r.ble,
                r.engine,
            )?))
        })
        .clone()
}
struct SocketGuard {
    device: u64,
    inode: u64,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        if fs::symlink_metadata(SOCKET)
            .is_ok_and(|m| m.dev() == self.device && m.ino() == self.inode)
        {
            let _ = fs::remove_file(SOCKET);
        }
    }
}
fn parent() -> Result<(), Error> {
    let r = InstalledRoles::read().map_err(|_| Error::Authentication)?;
    for path in ["/run", "/run/rustsimhub"] {
        let m = fs::symlink_metadata(path).map_err(|_| Error::Authentication)?;
        if !m.is_dir() || m.uid() != 0 || m.mode() & 0o022 != 0 {
            return Err(Error::Authentication);
        }
    }
    let m = fs::symlink_metadata("/run/rustsimhub/nkn").map_err(|_| Error::Authentication)?;
    if !m.is_dir() || m.uid() != r.core || m.gid() != r.engine_gid || m.mode() & 0o7777 != 0o2750 {
        return Err(Error::Authentication);
    }
    match fs::symlink_metadata(SOCKET) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(Error::Authentication),
    }
}

/// Caller must already have explicitly enabled Remote with a real recent
/// Owner. The directory/group is installed separately; this function does not
/// create accounts, paths, secrets, routes, Root services or SDKs. Off/status
/// must not call it. The single registry outlives every listener reopen and
/// retained original blocking worker, so rapid disable/enable cannot free an
/// occupied admission slot or run two old/new sources concurrently.
pub async fn serve(
    controller: Arc<Controller>,
    ports: CorePorts,
    membership: Arc<MembershipKey>,
) -> Result<(), Error> {
    controller.source().map_err(|_| Error::Authentication)?;
    let sources = registry()?;
    parent()?;
    let listener = UnixListener::bind(SOCKET).map_err(|_| Error::Protocol)?;
    let m = fs::symlink_metadata(SOCKET).map_err(|_| Error::Protocol)?;
    let _path = SocketGuard {
        device: m.dev(),
        inode: m.ino(),
    };
    fs::set_permissions(SOCKET, fs::Permissions::from_mode(0o660)).map_err(|_| Error::Protocol)?;
    let mut workers = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {biased;
            _=tick.tick()=>{
                let state=controller.snapshot();
                match state {Ok(Some(s)) if s.phase==Phase::Disabled=>return Ok(()),Ok(_)|Err(PortError::Unavailable)=>{},Err(_)=>return Err(Error::Authentication)}
                if matches!(controller.source(),Err(PortError::Denied)){return Ok(())}
            }
            _=workers.join_next(),if !workers.is_empty()=>{}
            accepted=listener.accept()=>{
                let (stream,_)=accepted.map_err(|_|Error::Protocol)?;
                let Ok(peer)=sources.accept(stream) else{#[cfg(test)] eprintln!("candidate fixture source UID rejected");continue};
                let Ok(source)=controller.source() else{continue};
                let Ok((peer,admission))=stage(peer.peer,Packet::new(Kind::Handshake,vec![]).map_err(Error::from)?,move|context,_|source.admit(context).map_err(|_error|{#[cfg(test)] eprintln!("candidate fixture Root admission failed: {_error:?}");Error::Authentication})).await else{continue};
                let ports=ports.clone();let membership=membership.clone();
                workers.spawn(async move{let result=nkn_host::serve_with_root(nkn_host::NknAccepted{peer},ports,membership,admission).await;#[cfg(test)]eprintln!("candidate fixture channel ended: {result:?}");result});
            }
        }
    }
}
