// SPDX-License-Identifier: AGPL-3.0-only
//! Owned, kernel-admitted candidate host. No listener, daemon activation,
//! arbitrary address, Owner creation, Root invocation or default integration.
use crate::{
    nkn_business::{CorePorts, NknBusinessPort},
    nkn_transport::{NknAdmission, NknTransportPort, SourceGate},
};
use rustsimhub_auth::nkn_session::{MembershipKey, NknSession};
use rustsimhub_core::recovery_transport::{
    self as transport, Kind, Packet, PeerConnection, PeerContext, Roles, SourceGuard,
};
use rustsimhub_nkn_agent::{routing::Router, SystemClock};
use rustsimhub_recovery_session::{ClientHello, Purpose};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{net::UnixStream, runtime::Handle};
#[derive(Debug, Clone, Copy)]
pub enum Error {
    Transport(transport::Error),
    Authentication,
    Protocol,
    Deadline,
    OutcomeUnknown,
}
type Result<T> = std::result::Result<T, Error>;
impl From<transport::Error> for Error {
    fn from(e: transport::Error) -> Self {
        Self::Transport(e)
    }
}
pub struct NknAccepted {
    pub(crate) peer: PeerConnection,
}
pub struct NknSources {
    guard: SourceGuard,
}
impl NknSources {
    /// Actual trusted installation roles, not fields accepted from a client.
    /// The NKN account MUST be separate from BLE, network, forwarder and Edge.
    pub fn new(edge: u32, forwarder: u32, media: u32, ble: u32, nkn: u32) -> Result<Self> {
        let ids = [
            edge,
            forwarder,
            media,
            ble,
            nkn,
            rustix::process::geteuid().as_raw(),
        ];
        if ids.contains(&0)
            || ids.contains(&u32::MAX)
            || ids.into_iter().collect::<HashSet<_>>().len() != 6
        {
            return Err(Error::Authentication);
        }
        Ok(Self {
            guard: SourceGuard::new(Roles::new(edge, forwarder, media, nkn)?)?,
        })
    }
    pub fn accept(&self, stream: UnixStream) -> Result<NknAccepted> {
        Ok(NknAccepted {
            peer: self.guard.accept(stream)?,
        })
    }
    pub fn is_busy(&self) -> bool {
        self.guard.is_busy()
    }
}
struct Fence(SourceGate);
impl Drop for Fence {
    fn drop(&mut self) {
        self.0.close();
    }
}
pub(crate) async fn stage<T, F>(
    peer: PeerConnection,
    packet: Packet,
    action: F,
) -> Result<(PeerConnection, T)>
where
    T: Send + 'static,
    F: FnOnce(&PeerContext, &Packet) -> Result<T> + Send + 'static,
{
    let (peer, result) = peer
        .verify_blocking(packet, move |context, packet| Ok(action(context, packet)))
        .await?;
    Ok((peer, result?))
}
fn auth_packet(packet: &Packet) -> Result<&[u8]> {
    if packet.kind() != Kind::Handshake {
        return Err(Error::Protocol);
    }
    Ok(packet.payload())
}
async fn handshake(
    mut peer: PeerConnection,
    ports: CorePorts,
    key: Arc<MembershipKey>,
    gate: SourceGate,
    parent_deadline: Instant,
) -> Result<(PeerConnection, NknSession)> {
    let first = peer.read().await?;
    let auth = ports.auth.clone();
    let g = gate.clone();
    let (mut peer, (mut session, noise)) = stage(peer, first, move |context, packet| {
        g.check().map_err(|_| Error::Authentication)?;
        let hello = ClientHello::parse(auth_packet(packet)?).map_err(|_| Error::Protocol)?;
        if hello.purpose != Purpose::NknControl {
            return Err(Error::Authentication);
        }
        let preparation = auth
            .try_lock()
            .map_err(|_| Error::Authentication)?
            .prepare_nkn_session(
                *context.connection_nonce(),
                hello.client_key,
                hello.generation,
                hello.nonce,
                parent_deadline,
            )
            .map_err(|_| Error::Authentication)?;
        let mut session = NknSession::begin(preparation).map_err(|_| Error::Authentication)?;
        session
            .read_handshake(&hello.noise)
            .map_err(|_| Error::Authentication)?;
        let wire = session
            .write_handshake()
            .map_err(|_| Error::Authentication)?;
        g.check().map_err(|_| Error::Authentication)?;
        Ok((session, wire))
    })
    .await?;
    peer.write(&Packet::new(Kind::Handshake, noise)?).await?;
    let next = peer.read().await?;
    let auth = ports.auth.clone();
    let g = gate.clone();
    let (mut peer, (next_session, binding)) = stage(peer, next, move |_, packet| {
        g.check().map_err(|_| Error::Authentication)?;
        session
            .read_handshake(auth_packet(packet)?)
            .map_err(|_| Error::Authentication)?;
        let work = session
            .device_binding_work()
            .map_err(|_| Error::Authentication)?;
        let signature = auth
            .try_lock()
            .map_err(|_| Error::Authentication)?
            .sign_nkn_device_binding(&work)
            .map_err(|_| Error::Authentication)?;
        let binding = session
            .seal_device_binding(work, &signature)
            .map_err(|_| Error::Authentication)?;
        g.check().map_err(|_| Error::Authentication)?;
        Ok((session, binding))
    })
    .await?;
    session = next_session;
    peer.write(&Packet::new(Kind::Handshake, binding)?).await?;
    let next = peer.read().await?;
    let auth = ports.auth.clone();
    let g = gate.clone();
    let (mut peer, (next_session, ack)) = stage(peer, next, move |_, packet| {
        g.check().map_err(|_| Error::Authentication)?;
        session
            .read_client_binding(auth_packet(packet)?)
            .map_err(|_| Error::Authentication)?;
        auth.try_lock()
            .map_err(|_| Error::Authentication)?
            .check_nkn_session(&session)
            .map_err(|_| Error::Authentication)?;
        let ack = session
            .write_binding_ack()
            .map_err(|_| Error::Authentication)?;
        g.check().map_err(|_| Error::Authentication)?;
        Ok((session, ack))
    })
    .await?;
    session = next_session;
    peer.write(&Packet::new(Kind::Handshake, ack)?).await?;
    let next = peer.read().await?;
    let auth = ports.auth.clone();
    let g = gate.clone();
    let (mut peer,(next_session,reply))=stage(peer,next,move|_,packet| {
        g.check().map_err(|_|Error::Authentication)?;
        let admission=session.verify_membership(&key,auth_packet(packet)?).map_err(|_|Error::Authentication)?;
        let scope=session.scope().map_err(|_|Error::Authentication)?;
        g.publish(scope.clone()).map_err(|_|Error::Authentication)?;
        let token=auth.try_lock().map_err(|_|Error::Authentication)?.issue_nkn_token(&scope).map_err(|_|Error::Authentication)?;
        let body=zeroize::Zeroizing::new(serde_json::to_vec(&serde_json::json!({"contract":1,"auth_protocol":2,"token":token.expose(),"expires_in_ms":scope.deadline().saturating_duration_since(Instant::now()).as_millis()})).map_err(|_|Error::Protocol)?);
        let reply=session.seal_membership_reply(admission,&body).map_err(|_|Error::Authentication)?;
        auth.try_lock().map_err(|_|Error::Authentication)?.check_nkn_scope(&scope).map_err(|_|Error::Authentication)?;
        g.check().map_err(|_|Error::Authentication)?;Ok((session,reply))
    }).await?;
    session = next_session;
    peer.write(&Packet::new(Kind::Handshake, reply)?).await?;
    Ok((peer, session))
}
async fn run(
    peer: PeerConnection,
    ports: CorePorts,
    key: Arc<MembershipKey>,
    gate: SourceGate,
    parent_deadline: Instant,
) -> Result<()> {
    let (peer, session) = tokio::time::timeout(
        Duration::from_secs(10),
        handshake(peer, ports.clone(), key, gate.clone(), parent_deadline),
    )
    .await
    .map_err(|_| Error::Deadline)??;
    let runtime = Handle::current();
    let g = gate.clone();
    // This local carrier only moves owned resources to the dedicated worker;
    // it is never parsed as a peer handshake or authorization evidence.
    let (mut peer, mut router) = stage(
        peer,
        Packet::new(Kind::Handshake, vec![])?,
        move |context, _| {
            g.check().map_err(|_| Error::Authentication)?;
            let scope = session.scope().map_err(|_| Error::Authentication)?;
            if scope.transport_nonce() != context.connection_nonce() {
                return Err(Error::Authentication);
            }
            let business = NknBusinessPort::new(ports.clone(), scope, runtime)
                .map_err(|_| Error::Authentication)?;
            let transport = NknTransportPort::new(ports.auth.clone(), g.clone());
            let mut router = Router::new(transport, business, SystemClock);
            router
                .open(NknAdmission::new(session, g))
                .map_err(|_| Error::Authentication)?;
            Ok(router)
        },
    )
    .await?;
    loop {
        gate.check().map_err(|_| Error::Authentication)?;
        let packet = peer.read().await?;
        if packet.kind() == Kind::CloseTransport {
            return Ok(());
        }
        if packet.kind() != Kind::EncryptedRequest {
            return Err(Error::Protocol);
        }
        let g = gate.clone();
        let (next_peer, (next_router, wire)) = stage(peer, packet, move |_, packet| {
            g.check().map_err(|_| Error::Authentication)?;
            let wire = router.dispatch(packet.payload()).map_err(|e| {
                if e == rustsimhub_nkn_agent::routing::Error::OutcomeUnknown {
                    Error::OutcomeUnknown
                } else {
                    Error::Authentication
                }
            })?;
            g.check().map_err(|_| Error::OutcomeUnknown)?;
            Ok((router, wire))
        })
        .await?;
        peer = next_peer;
        router = next_router;
        peer.write(&Packet::new(Kind::EncryptedRequest, wire)?)
            .await?;
    }
}
/// Caller supplies the original verified Root lease deadline; this function
/// cannot obtain/renew it or start an SDK. Dropping the waiter fences Auth and
/// media immediately; the dedicated worker retains the actual source slot
/// until it drains. All late output is discarded by the existing owned worker.
pub async fn serve_owned(
    accepted: NknAccepted,
    ports: CorePorts,
    membership: Arc<MembershipKey>,
    parent_deadline: Instant,
) -> Result<()> {
    let peer = accepted.peer;
    let nonce = *peer
        .context()
        .ok_or(Error::Authentication)?
        .connection_nonce();
    let deadline = parent_deadline.min(Instant::now() + Duration::from_secs(300));
    if Instant::now() >= deadline {
        return Err(Error::Deadline);
    }
    let gate = SourceGate::new(nonce, deadline);
    serve_gate(peer, ports, membership, gate, deadline).await
}

async fn serve_gate(
    peer: PeerConnection,
    ports: CorePorts,
    membership: Arc<MembershipKey>,
    gate: SourceGate,
    deadline: Instant,
) -> Result<()> {
    let _fence = Fence(gate.clone());
    let monitor = peer.disconnect_monitor()?;
    tokio::select! {biased;
        _=monitor.wait_closed()=>Err(Error::Transport(transport::Error::Closed)),
        result=tokio::time::timeout_at(deadline.into(),run(peer,ports,membership,gate,deadline))=>result.map_err(|_|Error::Deadline)?,
    }
}

/// Product candidate entry requires the Root provider's opaque admission for
/// this exact kernel PID, current owned child and fresh Core source nonce.
/// The earlier deadline-only entry remains an isolated UID fixture interface.
#[cfg(feature = "root-provider-candidate")]
pub async fn serve_with_root(
    accepted: NknAccepted,
    ports: CorePorts,
    membership: Arc<MembershipKey>,
    admission: rustsimhub_core::nkn_root::Admission,
) -> Result<()> {
    let peer = accepted.peer;
    let context = peer.context().ok_or(Error::Authentication)?;
    if !admission.matches(context) {
        return Err(Error::Authentication);
    }
    let nonce = *context.connection_nonce();
    let admission = Arc::new(admission);
    let gate = SourceGate::from_root(nonce, admission.clone());
    let deadline = admission.deadline();
    let parent = async move {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        loop {
            tick.tick().await;
            let original = admission.clone();
            let valid = tokio::task::spawn_blocking(move || original.refresh())
                .await
                .unwrap_or(false);
            if !valid {
                return Err(Error::Authentication);
            }
        }
    };
    tokio::select! {biased;result=parent=>result,result=serve_gate(peer,ports,membership,gate,deadline)=>result}
}
