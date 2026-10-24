// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use hickory_proto::rr::{
    rdata::{A, AAAA, CNAME},
    Record,
};
use tokio::{net::TcpListener, sync::oneshot};

fn route() -> Route {
    Route {
        interface: "lo".into(),
        ifindex: 1,
        ipv4: Some(Ipv4Addr::LOCALHOST),
        ipv6: None,
        dns_servers: vec![],
    }
}
fn response(query: &Message) -> Message {
    let mut m = Message::new(query.metadata.id, MessageType::Response, OpCode::Query);
    m.queries = query.queries.clone();
    m
}
async fn listener() -> (TcpListener, SocketAddr) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    (l, a)
}
fn question() -> Query {
    Query::query(Name::from_ascii("fixture.test.").unwrap(), RecordType::A)
}
fn query() -> Message {
    let mut m = Message::new(7, MessageType::Query, OpCode::Query);
    m.queries.push(question());
    m
}
#[test]
fn route_requires_explicit_usable_sources_interface_index_and_finite_resolvers() {
    let r = route();
    r.validate().unwrap();
    for bad in ["", "../lo", "bad\0name", "sixteencharacters"] {
        let mut r = r.clone();
        r.interface = bad.into();
        assert!(r.validate().is_err());
    }
    let mut r = route();
    r.ifindex = 0;
    assert!(r.validate().is_err());
    r = route();
    r.ipv4 = None;
    assert!(r.validate().is_err());
    r = route();
    r.ipv4 = Some(Ipv4Addr::UNSPECIFIED);
    assert!(r.validate().is_err());
    r = route();
    r.ipv6 = Some("::ffff:127.0.0.1".parse().unwrap());
    assert!(r.validate().is_err());
    r = route();
    r.dns_servers = vec!["127.0.0.1:53".parse().unwrap(); 2];
    assert!(r.validate().is_err());
    r = route();
    r.dns_servers = vec!["[::1]:53".parse().unwrap()];
    assert!(r.validate().is_err());
}
#[tokio::test]
async fn numeric_tcp_is_actually_bound_to_device_index_and_source_without_dns() {
    let (server, addr) = listener().await;
    let mut c = BoundConnector::new(route()).unwrap();
    c.probe().unwrap();
    let stream = c.connect("127.0.0.1", addr.port()).await.unwrap();
    let (_, peer) = server.accept().await.unwrap();
    assert_eq!(peer.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    let sock = socket2::SockRef::from(&stream);
    assert_eq!(sock.device().unwrap().as_deref(), Some(b"lo".as_slice()));
    assert_eq!(sock.device_index_v4().unwrap().unwrap().get(), 1);
}
#[tokio::test]
async fn missing_or_changed_interface_cannot_fall_back_to_an_available_unbound_listener() {
    let (server, addr) = listener().await;
    for name in ["lo", "no-such-link"] {
        let mut r = route();
        r.interface = name.into();
        r.ifindex = 77;
        let mut c = BoundConnector::new(r).unwrap();
        assert!(c.connect("127.0.0.1", addr.port()).await.is_err());
    }
    assert!(timeout(Duration::from_millis(40), server.accept())
        .await
        .is_err());
}
#[tokio::test]
async fn explicit_dns_resolves_alias_and_ignores_unrelated_answers() {
    let (server, addr) = listener().await;
    let dns = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut r = route();
    r.dns_servers.push(dns.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut bytes = [0; 1024];
        let (len, peer) = dns.recv_from(&mut bytes).await.unwrap();
        assert_eq!(peer.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        let q = Message::from_vec(&bytes[..len]).unwrap();
        let mut a = response(&q);
        let alias = Name::from_ascii("alias.test.").unwrap();
        a.answers.push(Record::from_rdata(
            q.queries[0].name.clone(),
            1,
            RData::CNAME(CNAME(alias.clone())),
        ));
        a.answers.push(Record::from_rdata(
            alias,
            1,
            RData::A(A(Ipv4Addr::LOCALHOST)),
        ));
        a.answers.push(Record::from_rdata(
            Name::from_ascii("unrelated.test.").unwrap(),
            1,
            RData::A(A("192.0.2.99".parse().unwrap())),
        ));
        dns.send_to(&a.to_vec().unwrap(), peer).await.unwrap();
    });
    let mut c = BoundConnector::new(r).unwrap();
    let _stream = c.connect("fixture.test", addr.port()).await.unwrap();
    server.accept().await.unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn cname_only_reply_gets_a_bounded_fresh_query_to_same_explicit_resolver() {
    let (server, addr) = listener().await;
    let dns = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut r = route();
    r.dns_servers.push(dns.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut bytes = [0; 1024];
        for i in 0..2 {
            let (len, peer) = dns.recv_from(&mut bytes).await.unwrap();
            let q = Message::from_vec(&bytes[..len]).unwrap();
            let mut a = response(&q);
            if i == 0 {
                a.answers.push(Record::from_rdata(
                    q.queries[0].name.clone(),
                    1,
                    RData::CNAME(CNAME(Name::from_ascii("alias.test.").unwrap())),
                ));
            } else {
                assert_eq!(q.queries[0].name, Name::from_ascii("alias.test.").unwrap());
                a.answers.push(Record::from_rdata(
                    q.queries[0].name.clone(),
                    1,
                    RData::A(A(Ipv4Addr::LOCALHOST)),
                ));
            }
            dns.send_to(&a.to_vec().unwrap(), peer).await.unwrap();
        }
    });
    let _stream = BoundConnector::new(r)
        .unwrap()
        .connect("fixture.test", addr.port())
        .await
        .unwrap();
    server.accept().await.unwrap();
    task.await.unwrap();
}
#[tokio::test]
async fn truncated_udp_uses_tcp_dns_on_the_same_bound_interface_and_source() {
    let dns = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let dns_addr = dns.local_addr().unwrap();
    let tcp = TcpListener::bind(dns_addr).await.unwrap();
    let (server, addr) = listener().await;
    let mut r = route();
    r.dns_servers.push(dns_addr);
    let task = tokio::spawn(async move {
        let mut bytes = [0; 1024];
        let (len, peer) = dns.recv_from(&mut bytes).await.unwrap();
        let q = Message::from_vec(&bytes[..len]).unwrap();
        let mut a = response(&q);
        a.metadata.truncation = true;
        dns.send_to(&a.to_vec().unwrap(), peer).await.unwrap();
        let (mut stream, peer) = tcp.accept().await.unwrap();
        assert_eq!(peer.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
        let len = stream.read_u16().await.unwrap() as usize;
        let mut bytes = vec![0; len];
        stream.read_exact(&mut bytes).await.unwrap();
        let q = Message::from_vec(&bytes).unwrap();
        let mut a = response(&q);
        a.answers.push(Record::from_rdata(
            q.queries[0].name.clone(),
            1,
            RData::A(A(Ipv4Addr::LOCALHOST)),
        ));
        let bytes = a.to_vec().unwrap();
        stream.write_u16(bytes.len() as u16).await.unwrap();
        stream.write_all(&bytes).await.unwrap();
    });
    let _stream = BoundConnector::new(r)
        .unwrap()
        .connect("fixture.test", addr.port())
        .await
        .unwrap();
    server.accept().await.unwrap();
    task.await.unwrap();
}
#[test]
fn dns_identity_counts_trailing_bytes_and_compression_are_not_treated_as_addresses() {
    let q = query();
    let mut a = response(&q);
    a.answers.push(Record::from_rdata(
        question().name,
        1,
        RData::A(A(Ipv4Addr::LOCALHOST)),
    ));
    let bytes = a.to_vec().unwrap();
    assert!(decode(&bytes, 7, &question(), true).unwrap().is_some());
    assert!(decode(&bytes, 8, &question(), true).is_err());
    let mut wrong = question();
    wrong.query_type = RecordType::AAAA;
    assert!(decode(&bytes, 7, &wrong, true).is_err());
    let mut extra = bytes.clone();
    extra.push(0);
    assert!(decode(&extra, 7, &question(), true).is_err());
    let mut truncated = a.clone();
    truncated.metadata.truncation = true;
    assert!(decode(&truncated.to_vec().unwrap(), 7, &question(), true)
        .unwrap()
        .is_none());
    assert!(decode(&truncated.to_vec().unwrap(), 7, &question(), false).is_err());
    a.metadata.id = 7;
    a.answers = vec![a.answers[0].clone(); 65];
    assert!(decode(&a.to_vec().unwrap(), 7, &question(), true).is_err());
    for bad in [vec![0; 11], vec![0; MAX_DNS + 1], vec![0xc0; 32]] {
        assert!(decode(&bad, 7, &question(), true).is_err());
    }
    let mut looped = vec![0, 7, 0x81, 0x80, 0, 1, 0, 0, 0, 0, 0, 0];
    looped.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1]);
    assert!(decode(&looped, 7, &question(), true).is_err());
}
#[test]
fn cname_cycles_and_mixed_alias_addresses_are_refused() {
    let q = query();
    let mut a = response(&q);
    let name = q.queries[0].name.clone();
    a.answers.push(Record::from_rdata(
        name.clone(),
        1,
        RData::CNAME(CNAME(name.clone())),
    ));
    assert!(answers(&a, &name, RecordType::A, &mut HashSet::from([name.clone()])).is_err());
    a.answers[0] = Record::from_rdata(
        name.clone(),
        1,
        RData::CNAME(CNAME(Name::from_ascii("other.test.").unwrap())),
    );
    a.answers.push(Record::from_rdata(
        name.clone(),
        1,
        RData::A(A(Ipv4Addr::LOCALHOST)),
    ));
    assert!(answers(&a, &name, RecordType::A, &mut HashSet::from([name.clone()])).is_err());
}
#[tokio::test]
async fn ipv6_literal_and_aaaa_answers_use_only_the_explicit_ipv6_source() {
    let server = TcpListener::bind("[::1]:0").await.unwrap();
    let addr = server.local_addr().unwrap();
    let dns = UdpSocket::bind("[::1]:0").await.unwrap();
    let mut r = route();
    r.ipv4 = None;
    r.ipv6 = Some(Ipv6Addr::LOCALHOST);
    r.dns_servers.push(dns.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let mut bytes = [0; 1024];
        let (len, peer) = dns.recv_from(&mut bytes).await.unwrap();
        let q = Message::from_vec(&bytes[..len]).unwrap();
        assert_eq!(q.queries[0].query_type, RecordType::AAAA);
        let mut a = response(&q);
        a.answers.push(Record::from_rdata(
            q.queries[0].name.clone(),
            1,
            RData::AAAA(AAAA(Ipv6Addr::LOCALHOST)),
        ));
        dns.send_to(&a.to_vec().unwrap(), peer).await.unwrap();
    });
    let _stream = BoundConnector::new(r)
        .unwrap()
        .connect("fixture.test", addr.port())
        .await
        .unwrap();
    assert_eq!(
        server.accept().await.unwrap().1.ip(),
        IpAddr::V6(Ipv6Addr::LOCALHOST)
    );
    task.await.unwrap();
}
#[tokio::test]
async fn connector_deadline_drops_a_stalled_tcp_dns_socket_without_background_driver() {
    let dns = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = dns.local_addr().unwrap();
    let tcp = TcpListener::bind(address).await.unwrap();
    let mut r = route();
    r.dns_servers.push(address);
    let (ready_tx, ready_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut bytes = [0; 1024];
        let (len, peer) = dns.recv_from(&mut bytes).await.unwrap();
        let q = Message::from_vec(&bytes[..len]).unwrap();
        let mut a = response(&q);
        a.metadata.truncation = true;
        dns.send_to(&a.to_vec().unwrap(), peer).await.unwrap();
        let (mut stream, _) = tcp.accept().await.unwrap();
        let len = stream.read_u16().await.unwrap() as usize;
        let mut query = vec![0; len];
        stream.read_exact(&mut query).await.unwrap();
        let _ = ready_tx.send(());
        assert_eq!(
            timeout(Duration::from_secs(2), stream.read(&mut bytes))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let mut c = BoundConnector::new(r).unwrap();
    c.budget = Duration::from_millis(200);
    assert!(c.connect("fixture.test", 443).await.is_err());
    timeout(Duration::from_secs(2), ready_rx)
        .await
        .unwrap()
        .unwrap();
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
}
