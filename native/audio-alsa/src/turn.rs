// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-revision TURN/TCP behind normally verified TLS. Private I/O is for the
//! owning RTC adapter, not a general proxy, authorization proof or PCM opener.
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{
    client::TlsStream,
    rustls::{self, pki_types::ServerName},
    TlsConnector,
};
use turn_client_proto::{
    api::{Transmit, TurnClientApi, TurnConfig, TurnEvent, TurnPollRet, TurnRecvRet},
    tcp::TurnClientTcp,
    types::{Instant as TurnInstant, TransportType, TurnCredentials},
};

const PREPARE: Duration = Duration::from_secs(5);
const IO: Duration = Duration::from_millis(500);
const CONTROL_BYTES: usize = 8192;
pub(crate) const DATAGRAM_BYTES: usize = 1500;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Certificate,
    Transport,
    Deadline,
    Protocol,
    Bounds,
    Closed,
    Peer,
}
/// Configuration comes from an installed Root registration. No Deserialize,
/// credential Debug, custom verifier, redirect, alternate URL or public fields.
/// This is connection policy, never a Core media authorization.
pub struct Registration {
    address: SocketAddr,
    name: ServerName<'static>,
    roots: rustls::RootCertStore,
    username: String,
    password: String,
    until: Instant,
    test_loopback: bool,
}
fn public_v4(ip: IpAddr) -> bool {
    let IpAddr::V4(ip) = ip else {
        return false;
    };
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip == Ipv4Addr::new(168, 63, 129, 16)
        || a == 0
        || a >= 240
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && (b == 0 || b == 168))
        || (a == 198 && (b == 18 || b == 19 || b == 51 && c == 100))
        || (a == 203 && b == 0 && c == 113))
}
impl Registration {
    /// `address` must be one of the addresses independently resolved/approved
    /// by the installed registration. There is no DNS retry/fallback here.
    pub fn from_installed(
        address: SocketAddr,
        hostname: &str,
        roots: rustls::RootCertStore,
        username: String,
        password: String,
        until: Instant,
    ) -> Result<Self, Error> {
        if !public_v4(address.ip()) || address.port() < 1024 {
            return Err(Error::Configuration);
        }
        Self::build(address, hostname, roots, username, password, until, false)
    }
    fn build(
        address: SocketAddr,
        hostname: &str,
        roots: rustls::RootCertStore,
        username: String,
        password: String,
        until: Instant,
        test_loopback: bool,
    ) -> Result<Self, Error> {
        let remaining = until
            .checked_duration_since(Instant::now())
            .ok_or(Error::Configuration)?;
        if hostname.is_empty()
            || hostname.len() > 253
            || hostname.parse::<IpAddr>().is_ok()
            || hostname
                .bytes()
                .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, b'.' | b'-'))
            || username.is_empty()
            || username.len() > 512
            || password.is_empty()
            || password.len() > 512
            || roots.is_empty()
            || remaining > Duration::from_secs(3600)
        {
            return Err(Error::Configuration);
        }
        let name = ServerName::try_from(hostname.to_owned()).map_err(|_| Error::Configuration)?;
        Ok(Self {
            address,
            name,
            roots,
            username,
            password,
            until,
            test_loopback,
        })
    }
    #[cfg(test)]
    pub(crate) fn loopback(
        address: SocketAddr,
        hostname: &str,
        roots: rustls::RootCertStore,
        username: String,
        password: String,
        until: Instant,
    ) -> Result<Self, Error> {
        if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) || address.port() < 1024 {
            return Err(Error::Configuration);
        }
        Self::build(address, hostname, roots, username, password, until, true)
    }
}
struct Fence {
    fd: rustix::fd::OwnedFd,
    live: AtomicBool,
}
impl Fence {
    fn close(&self) {
        self.live.store(false, Ordering::Release);
        let _ = rustix::net::shutdown(&self.fd, rustix::net::Shutdown::Both);
    }
    fn check(&self) -> Result<(), Error> {
        if self.live.load(Ordering::Acquire) {
            Ok(())
        } else {
            Err(Error::Closed)
        }
    }
}
struct Operation {
    fence: Arc<Fence>,
    complete: bool,
}
impl Drop for Operation {
    fn drop(&mut self) {
        if !self.complete {
            self.fence.close();
        }
    }
}
/// No public constructor, Clone, Deserialize or standalone packet-forwarding API.
/// It records a real authenticated Allocate event, not an SDP claim.
pub(crate) struct Allocation {
    pub(crate) address: SocketAddr,
    fence: Arc<Fence>,
    until: Instant,
}
impl Allocation {
    pub(crate) fn check(&self) -> Result<(), Error> {
        self.fence.check()?;
        if Instant::now() >= self.until {
            self.fence.close();
            return Err(Error::Deadline);
        }
        Ok(())
    }
}
/// Deliberately no Debug: protocol state contains upstream credential Strings.
pub(crate) struct Driver {
    #[cfg(not(test))]
    origin: Instant,
    // Explicit manual Sans-IO timer exercise only. Credential/Core/Frame
    // clocks remain actual monotonic time; production has no clock setter.
    #[cfg(test)]
    pub(crate) origin: Instant,
    // Explicit timer fixture may advance only after actual transactions drain.
    // These opaque IDs never leave test memory and confer no protocol authority.
    #[cfg(test)]
    pub(crate) test_pending: std::collections::HashSet<[u8; 12]>,
    #[cfg(test)]
    pub(crate) test_io: u64,
    tls: TlsStream<TcpStream>,
    proto: TurnClientTcp,
    allocation: Allocation,
    peer: Option<SocketAddr>,
    permission: bool,
    channel: bool,
    test_loopback: bool,
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.allocation.fence.close();
    }
}
impl Driver {
    fn now(&self) -> TurnInstant {
        TurnInstant::from_std(self.origin)
    }
    fn operation(&self) -> Result<Operation, Error> {
        self.allocation.check()?;
        Ok(Operation {
            fence: self.allocation.fence.clone(),
            complete: false,
        })
    }
    pub(crate) async fn allocate(reg: Registration) -> Result<Self, Error> {
        let deadline = tokio::time::Instant::now() + PREPARE;
        let tcp = tokio::time::timeout_at(deadline, TcpStream::connect(reg.address))
            .await
            .map_err(|_| Error::Deadline)?
            .map_err(|_| Error::Transport)?;
        tcp.set_nodelay(true).map_err(|_| Error::Transport)?;
        if tcp.peer_addr().map_err(|_| Error::Transport)? != reg.address {
            return Err(Error::Transport);
        }
        let fence = Arc::new(Fence {
            fd: rustix::io::dup(&tcp).map_err(|_| Error::Transport)?,
            live: AtomicBool::new(true),
        });
        let mut operation = Operation {
            fence: fence.clone(),
            complete: false,
        };
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| Error::Configuration)?
            .with_root_certificates(reg.roots)
            .with_no_client_auth();
        config.max_fragment_size = Some(4096);
        let mut tls = tokio::time::timeout_at(
            deadline,
            TlsConnector::from(Arc::new(config)).connect(reg.name, tcp),
        )
        .await
        .map_err(|_| Error::Deadline)?
        .map_err(|_| Error::Certificate)?;
        tls.get_mut().1.set_buffer_limit(Some(CONTROL_BYTES));
        let local = tls.get_ref().0.local_addr().map_err(|_| Error::Transport)?;
        let mut config = TurnConfig::new(TurnCredentials::new(&reg.username, &reg.password));
        // RFC long-term integrity is implemented by the pinned library. Normal
        // TLS and peer DTLS identity are independently mandatory, no custom hash.
        config.add_supported_integrity(
            turn_client_proto::types::stun::message::IntegrityAlgorithm::Sha256,
        );
        config.set_request_retransmits(
            Duration::from_millis(500),
            Duration::from_secs(1),
            2,
            Duration::from_secs(1),
        );
        let proto = TurnClientTcp::allocate(local, reg.address, config);
        let mut driver = Self {
            origin: Instant::now(),
            #[cfg(test)]
            test_pending: std::collections::HashSet::new(),
            #[cfg(test)]
            test_io: 0,
            tls,
            proto,
            allocation: Allocation {
                address: "0.0.0.0:0".parse().map_err(|_| Error::Configuration)?,
                fence,
                until: reg.until,
            },
            peer: None,
            permission: false,
            channel: false,
            test_loopback: reg.test_loopback,
        };
        tokio::time::timeout_at(deadline, async {
            loop {
                driver.step(Duration::from_millis(10)).await?;
                if driver.allocation.address.port() != 0 {
                    return Ok::<(), Error>(());
                }
            }
        })
        .await
        .map_err(|_| Error::Deadline)??;
        operation.complete = true;
        Ok(driver)
    }
    pub(crate) fn allocation(&self) -> &Allocation {
        &self.allocation
    }
    pub(crate) fn peer(&self) -> Option<SocketAddr> {
        self.peer
    }
    pub(crate) fn base(&self) -> SocketAddr {
        self.proto.local_addr()
    }
    pub(crate) fn fence(&self) {
        self.allocation.fence.close();
    }
    pub(crate) fn ready(&self) -> Result<(), Error> {
        self.allocation.check()?;
        let peer = self.peer.ok_or(Error::Peer)?;
        if !self.permission
            || !self.channel
            || !self.proto.have_permission(TransportType::Udp, peer.ip())
            || !self
                .proto
                .relayed_addresses()
                .any(|(t, a)| t == TransportType::Udp && a == self.allocation.address)
        {
            return Err(Error::Peer);
        }
        Ok(())
    }
    pub(crate) async fn bind_peer(&mut self, peer: SocketAddr) -> Result<(), Error> {
        self.allocation.check()?;
        if self.peer.is_some()
            || peer.port() < 1024
            // Initial single-provider version: only peers on the same actual
            // relay IP authenticated by this Allocate. A public SDP address
            // alone must not turn the registered server into an open proxy.
            || peer.ip() != self.allocation.address.ip()
            || !(public_v4(peer.ip())
                || self.test_loopback && peer.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST))
        {
            return Err(Error::Peer);
        }
        self.peer = Some(peer);
        let mut operation = self.operation()?;
        self.proto
            .create_permission(TransportType::Udp, peer.ip(), self.now())
            .map_err(|_| Error::Protocol)?;
        tokio::time::timeout(PREPARE, async {
            while !self.permission {
                self.step(Duration::from_millis(10)).await?;
            }
            self.proto
                .bind_channel(TransportType::Udp, peer, self.now())
                .map_err(|_| Error::Protocol)?;
            while !self.channel {
                self.step(Duration::from_millis(10)).await?;
            }
            self.ready()
        })
        .await
        .map_err(|_| Error::Deadline)??;
        operation.complete = true;
        Ok(())
    }
    async fn transmit(&mut self, t: Transmit<impl AsRef<[u8]>>) -> Result<(), Error> {
        if t.transport != TransportType::Tcp
            || t.from != self.proto.local_addr()
            || t.to != self.proto.remote_addr()
            || t.data.as_ref().len() > CONTROL_BYTES
        {
            return Err(Error::Bounds);
        }
        tokio::time::timeout(IO, async {
            self.tls.write_all(t.data.as_ref()).await?;
            self.tls.flush().await
        })
        .await
        .map_err(|_| Error::Deadline)?
        .map_err(|_| Error::Transport)?;
        #[cfg(test)]
        {
            let bytes = t.data.as_ref();
            self.test_io += 1;
            if bytes.len() >= 20
                && bytes[0] >> 6 == 0
                && u16::from_be_bytes([bytes[0], bytes[1]]) & 0x0110 == 0
            {
                self.test_pending
                    .insert(bytes[8..20].try_into().map_err(|_| Error::Bounds)?);
            }
        }
        self.allocation.check()
    }
    fn events(&mut self) -> Result<(), Error> {
        for _ in 0..32 {
            let Some(event) = self.proto.poll_event() else {
                return Ok(());
            };
            match event {
                TurnEvent::AllocationCreated(TransportType::Udp, address)
                    if self.allocation.address.port() == 0
                        && address.port() >= 1024
                        && (public_v4(address.ip())
                            || self.test_loopback
                                && address.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST)) =>
                {
                    self.allocation.address = address
                }
                TurnEvent::PermissionCreated(TransportType::Udp, ip)
                    if self.peer.is_some_and(|p| p.ip() == ip) =>
                {
                    self.permission = true
                }
                TurnEvent::ChannelCreated(TransportType::Udp, peer) if self.peer == Some(peer) => {
                    self.channel = true
                }
                _ => return Err(Error::Protocol),
            }
        }
        Err(Error::Bounds)
    }
    /// One complete bounded frame is passed to the Sans-IO parser. TLS/partial
    /// frame errors and cancellation close the actual FD; never reuse leftovers.
    async fn incoming(&mut self, wait: Duration) -> Result<Option<Vec<u8>>, Error> {
        let first = match tokio::time::timeout(wait, self.tls.read_u8()).await {
            Err(_) => return Ok(None),
            Ok(Err(_)) => return Err(Error::Transport),
            Ok(Ok(byte)) => byte,
        };
        tokio::time::timeout(IO, async {
            let mut prefix = [first, 0, 0, 0];
            self.tls
                .read_exact(&mut prefix[1..])
                .await
                .map_err(|_| Error::Transport)?;
            let len = usize::from(u16::from_be_bytes([prefix[2], prefix[3]]));
            let total = match first >> 6 {
                0 if len % 4 == 0 => 20usize
                    .checked_add(len)
                    .filter(|n| *n <= CONTROL_BYTES)
                    .ok_or(Error::Bounds)?,
                1 if len <= DATAGRAM_BYTES => 4 + len.div_ceil(4) * 4,
                _ => return Err(Error::Protocol),
            };
            let mut frame = vec![0; total];
            frame[..4].copy_from_slice(&prefix);
            self.tls
                .read_exact(&mut frame[4..])
                .await
                .map_err(|_| Error::Transport)?;
            if first >> 6 == 0 && frame[4..8] != [0x21, 0x12, 0xa4, 0x42] {
                return Err(Error::Protocol);
            }
            #[cfg(test)]
            {
                self.test_io += 1;
                if first >> 6 == 0 && u16::from_be_bytes([frame[0], frame[1]]) & 0x0100 != 0 {
                    let id: [u8; 12] = frame[8..20].try_into().map_err(|_| Error::Bounds)?;
                    self.test_pending.remove(&id);
                }
            }
            Ok(Some(frame))
        })
        .await
        .map_err(|_| Error::Deadline)?
    }
    pub(crate) async fn step(&mut self, wait: Duration) -> Result<Option<Vec<u8>>, Error> {
        let mut operation = self.operation()?;
        self.events()?;
        let wait = match self.proto.poll(self.now()) {
            TurnPollRet::WaitUntil(until) => wait.min(until.saturating_duration_since(self.now())),
            _ => return Err(Error::Closed),
        };
        for index in 0..32 {
            let Some(packet) = self.proto.poll_transmit(self.now()) else {
                break;
            };
            if index == 31 {
                return Err(Error::Bounds);
            }
            self.transmit(packet).await?;
        }
        let received = match self.incoming(wait).await? {
            None => None,
            Some(frame) => match self.proto.recv(
                Transmit {
                    transport: TransportType::Tcp,
                    from: self.proto.remote_addr(),
                    to: self.proto.local_addr(),
                    data: frame,
                },
                self.now(),
            ) {
                TurnRecvRet::Handled => None,
                TurnRecvRet::PeerData(data) => {
                    if data.transport != TransportType::Udp
                        || Some(data.peer) != self.peer
                        || data.data().is_empty()
                        || data.data().len() > DATAGRAM_BYTES
                    {
                        return Err(Error::Peer);
                    }
                    // The actual peer may start ICE while ChannelBind is still
                    // in flight. Discard early bytes; they must neither grant
                    // readiness nor defeat the original finite preparation.
                    if !self.permission || !self.channel {
                        None
                    } else {
                        self.ready()?;
                        Some(data.data().to_vec())
                    }
                }
                _ => return Err(Error::Protocol),
            },
        };
        self.events()?;
        self.allocation.check()?;
        operation.complete = true;
        Ok(received)
    }
    /// Only RTC-generated ICE/DTLS/SRTP packets reach this private entry point.
    pub(crate) async fn send(&mut self, data: &[u8]) -> Result<(), Error> {
        self.ready()?;
        if data.is_empty() || data.len() > DATAGRAM_BYTES {
            return Err(Error::Bounds);
        }
        let mut operation = self.operation()?;
        let packet = self
            .proto
            .send_to(
                TransportType::Udp,
                self.peer.ok_or(Error::Peer)?,
                data,
                self.now(),
            )
            .map_err(|_| Error::Protocol)?
            .ok_or(Error::Protocol)?
            .build();
        self.transmit(packet).await?;
        self.ready()?;
        operation.complete = true;
        Ok(())
    }
    /// Best-effort lifetime=0 request; upstream exposes no unique DeleteAck.
    /// Always Unknown, including a clean TLS shutdown. Drop/TTL remain bounded.
    pub(crate) async fn close(mut self) {
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            self.proto.delete(self.now()).map_err(|_| Error::Protocol)?;
            for _ in 0..8 {
                let Some(packet) = self.proto.poll_transmit(self.now()) else {
                    break;
                };
                self.transmit(packet).await?;
            }
            self.tls.shutdown().await.map_err(|_| Error::Transport)
        })
        .await;
        self.allocation.fence.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn even_the_explicit_test_registration_refuses_missing_trust_roots() {
        assert!(Registration::loopback(
            "127.0.0.1:5349".parse().unwrap(),
            "turn.fixture.invalid",
            rustls::RootCertStore::empty(),
            "synthetic-user".into(),
            "synthetic-password".into(),
            Instant::now() + Duration::from_secs(60),
        )
        .is_err());
    }
}
