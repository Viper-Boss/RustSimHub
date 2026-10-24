// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit Root-selected egress. Every DNS and TCP socket is bound to the
//! selected interface AND source address. No ambient resolver, proxy, pooled
//! socket, unbound fallback, background task or interface/routing mutation.
use crate::{Connector, Error, Result};
use hickory_proto::{
    op::{Header, Message, MessageType, OpCode, Query, ResponseCode},
    rr::{DNSClass, Name, RData, RecordType},
    serialize::binary::{BinDecodable, BinDecoder},
};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpSocket, TcpStream, UdpSocket},
    time::{timeout, timeout_at, Instant},
};

const MAX_DNS: usize = 4096;
const MAX_ADDRESSES: usize = 8;
const BUDGET: Duration = Duration::from_secs(6);
const DNS_WAIT: Duration = Duration::from_millis(750);
const TCP_WAIT: Duration = Duration::from_millis(1200);

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub interface: String,
    pub ifindex: u32,
    pub ipv4: Option<Ipv4Addr>,
    pub ipv6: Option<Ipv6Addr>,
    pub dns_servers: Vec<SocketAddr>,
}
fn unicast(ip: IpAddr) -> bool {
    !ip.is_unspecified()
        && !ip.is_multicast()
        && match ip {
            IpAddr::V4(v) => !v.is_broadcast(),
            IpAddr::V6(v) => v.to_ipv4_mapped().is_none(),
        }
}
impl Route {
    pub fn validate(&self) -> Result<()> {
        if self.interface.is_empty()
            || self.interface.len() > 15
            || self.ifindex == 0
            || !self
                .interface
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            || self.ipv4.is_none() && self.ipv6.is_none()
            || self.ipv4.is_some_and(|v| !unicast(v.into()))
            || self.ipv6.is_some_and(|v| !unicast(v.into()))
            || self.dns_servers.len() > 3
        {
            return Err(Error::Invalid);
        }
        let mut seen = HashSet::new();
        for server in &self.dns_servers {
            if server.port() == 0
                || !unicast(server.ip())
                || !seen.insert(server)
                || self.source(server.ip()).is_err()
                || matches!(server,SocketAddr::V6(s) if s.scope_id()!=0 && s.scope_id()!=self.ifindex)
            {
                return Err(Error::Invalid);
            }
        }
        Ok(())
    }
    fn source(&self, destination: IpAddr) -> Result<IpAddr> {
        match destination {
            IpAddr::V4(_) => self.ipv4.map(IpAddr::V4),
            IpAddr::V6(_) => self.ipv6.map(IpAddr::V6),
        }
        .ok_or(Error::Forbidden)
    }
    fn address(&self, ip: IpAddr, port: u16) -> SocketAddr {
        match ip {
            IpAddr::V6(v) if v.is_unicast_link_local() => {
                SocketAddr::V6(SocketAddrV6::new(v, port, 0, self.ifindex))
            }
            _ => SocketAddr::new(ip, port),
        }
    }
    fn socket(&self, destination: SocketAddr, udp: bool) -> Result<Socket> {
        if !unicast(destination.ip()) || destination.port() == 0 {
            return Err(Error::Invalid);
        }
        let source = self.source(destination.ip())?;
        let socket = Socket::new(
            Domain::for_address(destination),
            if udp { Type::DGRAM } else { Type::STREAM },
            Some(if udp { Protocol::UDP } else { Protocol::TCP }),
        )?;
        socket.set_nonblocking(true)?;
        if destination.is_ipv6() {
            socket.set_only_v6(true)?;
        }
        socket.bind_device(Some(self.interface.as_bytes()))?;
        // Name and index must BOTH still identify the approved device. A
        // removed/renamed interface or unsupported kernel option fails closed.
        if socket.device()?.as_deref() != Some(self.interface.as_bytes())
            || socket.device_index_v4()?.map(|n| n.get()) != Some(self.ifindex)
        {
            return Err(Error::Forbidden);
        }
        socket.bind(&SockAddr::from(self.address(source, 0)))?;
        Ok(socket)
    }
    async fn tcp(&self, destination: SocketAddr) -> Result<TcpStream> {
        timeout(TCP_WAIT, async {
            let socket = self.socket(destination, false)?;
            let stream = TcpSocket::from_std_stream(socket.into())
                .connect(destination)
                .await?;
            if stream.local_addr()?.ip() != self.source(destination.ip())? {
                return Err(Error::Forbidden);
            }
            let socket = socket2::SockRef::from(&stream);
            if socket.device_index_v4()?.map(|n| n.get()) != Some(self.ifindex) {
                return Err(Error::Forbidden);
            }
            Ok(stream)
        })
        .await
        .map_err(|_| Error::Busy)?
    }
}
pub struct BoundConnector {
    route: Route,
    budget: Duration,
}
impl BoundConnector {
    pub fn new(route: Route) -> Result<Self> {
        route.validate()?;
        Ok(Self {
            route,
            budget: BUDGET,
        })
    }
    pub fn probe(&self) -> Result<()> {
        for source in [
            self.route.ipv4.map(IpAddr::V4),
            self.route.ipv6.map(IpAddr::V6),
        ]
        .into_iter()
        .flatten()
        {
            self.route.socket(self.route.address(source, 443), false)?;
        }
        Ok(())
    }
    async fn exchange(&self, server: SocketAddr, name: &Name, kind: RecordType) -> Result<Message> {
        let mut random = [0; 2];
        getrandom::fill(&mut random).map_err(|_| Error::Forbidden)?;
        let id = u16::from_be_bytes(random);
        let question = Query::query(name.clone(), kind);
        let mut query = Message::new(id, MessageType::Query, OpCode::Query);
        query.metadata.recursion_desired = true;
        query.queries.push(question.clone());
        let bytes = query.to_vec().map_err(|_| Error::Invalid)?;
        let server = self.route.address(server.ip(), server.port());
        let udp = timeout(DNS_WAIT, async {
            let socket = self.route.socket(server, true)?;
            socket.connect(&server.into())?;
            let socket = UdpSocket::from_std(socket.into())?;
            socket.send(&bytes).await?;
            let mut buffer = vec![0; MAX_DNS + 1];
            let len = socket.recv(&mut buffer).await?;
            buffer.truncate(len);
            Ok::<_, Error>(buffer)
        })
        .await
        .map_err(|_| Error::Busy)??;
        match decode(&udp, id, &question, true)? {
            Some(message) => Ok(message),
            None => {
                // TC is a DNS transport fallback, never an HTTP retry. It is
                // pinned to the SAME resolver/interface/source as this query.
                let mut stream = self.route.tcp(server).await?;
                stream.write_u16(bytes.len() as u16).await?;
                stream.write_all(&bytes).await?;
                let len = stream.read_u16().await? as usize;
                if !(12..=MAX_DNS).contains(&len) {
                    return Err(Error::Invalid);
                }
                let mut buffer = vec![0; len];
                stream.read_exact(&mut buffer).await?;
                decode(&buffer, id, &question, false)?.ok_or(Error::Invalid)
            }
        }
    }
    async fn resolve_type(
        &self,
        server: SocketAddr,
        name: &Name,
        kind: RecordType,
    ) -> Result<Vec<IpAddr>> {
        let mut current = name.clone();
        let mut seen = HashSet::from([current.clone()]);
        for _ in 0..4 {
            let response = self.exchange(server, &current, kind).await?;
            if response.metadata.response_code == ResponseCode::NXDomain {
                return Ok(vec![]);
            }
            let (addresses, next) = answers(&response, &current, kind, &mut seen)?;
            if !addresses.is_empty() || next == current {
                return Ok(addresses);
            }
            current = next;
        }
        Err(Error::Invalid)
    }
    async fn resolve(&self, host: &str) -> Result<Vec<IpAddr>> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return if unicast(ip) && self.route.source(ip).is_ok() {
                Ok(vec![ip])
            } else {
                Err(Error::Invalid)
            };
        }
        let host = host.strip_suffix('.').unwrap_or(host);
        if host.is_empty()
            || host.len() > 253
            || !host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err(Error::Invalid);
        }
        let name = Name::from_ascii(format!("{host}.")).map_err(|_| Error::Invalid)?;
        let mut addresses = vec![];
        let deadline = Instant::now() + Duration::from_secs(3);
        'servers: for &server in &self.route.dns_servers {
            for (present, kind) in [
                (self.route.ipv4.is_some(), RecordType::A),
                (self.route.ipv6.is_some(), RecordType::AAAA),
            ] {
                if !present {
                    continue;
                }
                let result = timeout_at(deadline, self.resolve_type(server, &name, kind)).await;
                let Ok(result) = result else {
                    break 'servers;
                };
                if let Ok(found) = result {
                    for ip in found {
                        if !addresses.contains(&ip) {
                            addresses.push(ip);
                        }
                    }
                }
                if addresses.len() >= MAX_ADDRESSES {
                    addresses.truncate(MAX_ADDRESSES);
                    break;
                }
            }
            if !addresses.is_empty() {
                break;
            }
        }
        if addresses.is_empty() {
            return Err(Error::Forbidden);
        }
        Ok(addresses)
    }
}
impl Connector for BoundConnector {
    async fn connect(&mut self, host: &str, port: u16) -> Result<TcpStream> {
        if port == 0 {
            return Err(Error::Invalid);
        }
        timeout(self.budget, async {
            // Reserve a separate bound budget for TCP so a slow DNS family
            // cannot consume every opportunity to connect to a known address.
            let addresses = self.resolve(host).await?;
            for ip in addresses {
                if let Ok(stream) = self.route.tcp(self.route.address(ip, port)).await {
                    return Ok(stream);
                }
            }
            Err(Error::Forbidden)
        })
        .await
        .map_err(|_| Error::Busy)?
    }
}
fn decode(bytes: &[u8], id: u16, question: &Query, udp: bool) -> Result<Option<Message>> {
    if !(12..=MAX_DNS).contains(&bytes.len()) {
        return Err(Error::Invalid);
    }
    let mut decoder = BinDecoder::new(bytes);
    let header = Header::read(&mut decoder).map_err(|_| Error::Invalid)?;
    if header.counts.queries != 1
        || u32::from(header.counts.answers)
            + u32::from(header.counts.authorities)
            + u32::from(header.counts.additionals)
            > 64
        || header.metadata.id != id
        || header.metadata.message_type != MessageType::Response
        || header.metadata.op_code != OpCode::Query
        || !matches!(
            header.metadata.response_code,
            ResponseCode::NoError | ResponseCode::NXDomain
        )
    {
        return Err(Error::Invalid);
    }
    if Query::read(&mut decoder).map_err(|_| Error::Invalid)? != *question {
        return Err(Error::Invalid);
    }
    if header.metadata.truncation {
        return if udp { Ok(None) } else { Err(Error::Invalid) };
    }
    let mut decoder = BinDecoder::new(bytes);
    let message = Message::read(&mut decoder).map_err(|_| Error::Invalid)?;
    if !decoder.is_empty() {
        return Err(Error::Invalid);
    }
    Ok(Some(message))
}
fn answers(
    message: &Message,
    name: &Name,
    kind: RecordType,
    seen: &mut HashSet<Name>,
) -> Result<(Vec<IpAddr>, Name)> {
    let mut current = name.clone();
    for _ in 0..8 {
        let mut addresses = vec![];
        let mut alias = None;
        for record in &message.answers {
            if record.dns_class != DNSClass::IN || record.name != current {
                continue;
            }
            match &record.data {
                RData::A(a) if kind == RecordType::A => addresses.push(IpAddr::V4(a.0)),
                RData::AAAA(a) if kind == RecordType::AAAA => addresses.push(IpAddr::V6(a.0)),
                RData::CNAME(cname) => {
                    if alias.as_ref().is_some_and(|n| *n != cname.0) {
                        return Err(Error::Invalid);
                    }
                    alias = Some(cname.0.clone());
                }
                _ => {}
            }
        }
        if addresses.iter().any(|a| !unicast(*a)) || !addresses.is_empty() && alias.is_some() {
            return Err(Error::Invalid);
        }
        if !addresses.is_empty() {
            addresses.truncate(MAX_ADDRESSES);
            return Ok((addresses, current));
        }
        match alias {
            None => return Ok((vec![], current)),
            Some(next) => {
                if !seen.insert(next.clone()) || seen.len() > 8 {
                    return Err(Error::Invalid);
                }
                current = next;
            }
        }
    }
    Err(Error::Invalid)
}

#[cfg(test)]
mod tests;
