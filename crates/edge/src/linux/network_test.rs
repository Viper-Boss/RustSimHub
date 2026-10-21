// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed HTTPS targets, interface-bound DNS/TCP, bounded bytes/time. No URL input.
use super::*;
use std::{
    io::BufReader,
    net::{Ipv4Addr, SocketAddrV4},
    sync::OnceLock,
};
use tokio::net::{TcpSocket, UdpSocket};
use tokio_rustls::{
    rustls::{
        pki_types::{pem::PemObject, ServerName},
        ClientConfig, RootCertStore,
    },
    TlsConnector,
};
static TESTS: Semaphore = Semaphore::const_new(1);
static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
fn public(ip: Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 240
        || (ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1])))
}
fn query(host: &str, id: [u8; 2]) -> Vec<u8> {
    let mut b = vec![id[0], id[1], 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in host.split('.') {
        b.push(label.len() as u8);
        b.extend(label.as_bytes());
    }
    b.extend([0, 0, 1, 0, 1]);
    b
}
fn skip_name(b: &[u8], at: &mut usize) -> Result<()> {
    for _ in 0..128 {
        let n = *b.get(*at).ok_or("invalid DNS response")?;
        *at += 1;
        if n == 0 {
            return Ok(());
        }
        if n & 0xc0 == 0xc0 {
            b.get(*at).ok_or("invalid DNS pointer")?;
            *at += 1;
            return Ok(());
        }
        if n > 63 {
            return Err("invalid DNS label");
        }
        *at += usize::from(n);
        if *at > b.len() {
            return Err("invalid DNS name");
        }
    }
    Err("DNS name too long")
}
fn answer(b: &[u8], q: &[u8]) -> Result<Ipv4Addr> {
    if b.len() < q.len()
        || b[..2] != q[..2]
        || b[2] & 0x80 == 0
        || b[2] & 0x78 != 0
        || b[2] & 2 != 0
        || b[3] & 15 != 0
        || b[4..6] != [0, 1]
        || b[12..q.len()] != q[12..]
    {
        return Err("invalid DNS response");
    }
    let count = u16::from_be_bytes([b[6], b[7]]);
    if count > 64 {
        return Err("DNS answer catalogue too large");
    }
    let mut at = q.len();
    for _ in 0..count {
        skip_name(b, &mut at)?;
        let h = b.get(at..at + 10).ok_or("invalid DNS record")?;
        at += 10;
        let len = usize::from(u16::from_be_bytes([h[8], h[9]]));
        let data = b.get(at..at + len).ok_or("invalid DNS payload")?;
        at += len;
        if h[..4] == [0, 1, 0, 1] && len == 4 {
            let ip = Ipv4Addr::new(data[0], data[1], data[2], data[3]);
            if public(ip) {
                return Ok(ip);
            }
        }
    }
    Err("no public DNS address")
}
fn interface(v: &serde_json::Value, name: &str) -> Result<(Ipv4Addr, Ipv4Addr, bool)> {
    if name.is_empty()
        || name.len() > 15
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err("invalid interface");
    }
    let dev = v["devices"]
        .as_array()
        .ok_or("network unavailable")?
        .iter()
        .find(|d| {
            d["interface"] == name && d["state"].as_str().is_some_and(|s| s.starts_with("100 "))
        })
        .ok_or("selected interface disconnected")?;
    let source = dev["ipv4"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|a| a.as_str())
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.parse::<Ipv4Addr>().ok())
        .filter(|ip| !ip.is_unspecified() && !ip.is_loopback())
        .ok_or("IPv4 unavailable")?;
    let dns = dev["dns_ipv4"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|a| a.as_str())
        .and_then(|s| s.parse::<Ipv4Addr>().ok())
        .filter(|ip| !ip.is_unspecified() && !ip.is_multicast())
        .ok_or("DNS unavailable")?;
    Ok((source, dns, dev["kind"] == "cellular"))
}
async fn probe(
    snapshot: &serde_json::Value,
    iface: &str,
    profile: &str,
    allow_cellular: bool,
) -> Result<serde_json::Value> {
    let (source, dns, cellular) = interface(snapshot, iface)?;
    if cellular && !allow_cellular {
        return Err("cellular consent required");
    }
    if cellular && snapshot.pointer("/usage/limit_reached") == Some(&serde_json::Value::Bool(true))
    {
        return Err("cellular budget exhausted");
    }
    let (host, path, max) = match profile {
        "connectivity" => ("www.baidu.com", "/robots.txt", 65536usize),
        "download" => ("speed.cloudflare.com", "/__down?bytes=1048576", 1_100_000),
        _ => return Err("unknown test profile"),
    };
    let start = Instant::now();
    let std_udp =
        std::net::UdpSocket::bind(SocketAddrV4::new(source, 0)).map_err(|_| "DNS bind failed")?;
    socket2::SockRef::from(&std_udp)
        .bind_device(Some(iface.as_bytes()))
        .map_err(|_| "interface binding unavailable")?;
    std_udp
        .set_nonblocking(true)
        .map_err(|_| "DNS socket unavailable")?;
    let udp = UdpSocket::from_std(std_udp).map_err(|_| "DNS socket unavailable")?;
    udp.connect(SocketAddrV4::new(dns, 53))
        .await
        .map_err(|_| "DNS connect failed")?;
    let mut id = [0u8; 2];
    getrandom::fill(&mut id).map_err(|_| "entropy unavailable")?;
    let q = query(host, id);
    udp.send(&q).await.map_err(|_| "DNS send failed")?;
    let mut packet = [0u8; 4096];
    let n = timeout(Duration::from_secs(4), udp.recv(&mut packet))
        .await
        .map_err(|_| "DNS timeout")?
        .map_err(|_| "DNS receive failed")?;
    let ip = answer(&packet[..n], &q)?;
    let dns_ms = start.elapsed().as_millis();
    let socket = TcpSocket::new_v4().map_err(|_| "TCP unavailable")?;
    socket2::SockRef::from(&socket)
        .bind_device(Some(iface.as_bytes()))
        .map_err(|_| "interface binding unavailable")?;
    socket
        .bind(SocketAddrV4::new(source, 0).into())
        .map_err(|_| "TCP source bind failed")?;
    let tcp_start = Instant::now();
    let tcp = socket
        .connect(SocketAddrV4::new(ip, 443).into())
        .await
        .map_err(|_| "TCP connect failed")?;
    let tcp_ms = tcp_start.elapsed().as_millis();
    let ca = std::fs::File::open("/etc/ssl/certs/ca-certificates.crt")
        .map_err(|_| "TLS trust store unavailable")?;
    let mut roots = RootCertStore::empty();
    for cert in CertificateDer::pem_reader_iter(&mut BufReader::new(ca)) {
        roots
            .add(cert.map_err(|_| "invalid TLS trust store")?)
            .map_err(|_| "invalid TLS root")?;
    }
    let cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tls_start = Instant::now();
    let mut tls = TlsConnector::from(Arc::new(cfg))
        .connect(
            ServerName::try_from(host.to_owned()).map_err(|_| "invalid TLS name")?,
            tcp,
        )
        .await
        .map_err(|_| "TLS verification failed")?;
    let tls_ms = tls_start.elapsed().as_millis();
    let request=format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: RustSimHub-network-test/1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n");
    tls.write_all(request.as_bytes())
        .await
        .map_err(|_| "HTTPS send failed")?;
    let receive = Instant::now();
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    let mut first = None;
    loop {
        let n = tls
            .read(&mut buf)
            .await
            .map_err(|_| "HTTPS receive failed")?;
        if n == 0 {
            break;
        }
        if first.is_none() {
            first = Some(receive.elapsed().as_millis());
        }
        if bytes.len() + n > max {
            return Err("HTTPS response exceeds byte limit");
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    let header = bytes
        .windows(4)
        .position(|v| v == b"\r\n\r\n")
        .ok_or("invalid HTTPS response")?;
    if header > 16384
        || !(bytes.starts_with(b"HTTP/1.1 200 ") || bytes.starts_with(b"HTTP/1.0 200 "))
    {
        return Err("HTTPS target did not return 200");
    }
    let payload = bytes.len().saturating_sub(header + 4);
    let seconds = receive.elapsed().as_secs_f64().max(0.001);
    Ok(
        serde_json::json!({"ok":true,"profile":profile,"target":host,"interface":iface,"dns_ms":dns_ms,"tcp_ms":tcp_ms,"tls_ms":tls_ms,"first_byte_ms":first,"total_ms":start.elapsed().as_millis(),"received_bytes":bytes.len(),"payload_bytes":payload,"sample_download_mbps":payload as f64*8./seconds/1_000_000.,"maximum_capacity_test":false,"tls_verified":true,"interface_bound":true,"ordinary_data_activated":false}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TestRequest {
    token: String,
    interface: String,
    profile: String,
    #[serde(default)]
    allow_cellular: bool,
}
async fn auth_snapshot(app: &App, token: String) -> Result<serde_json::Value> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|_| "entropy unavailable")?;
    let id: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let body = serde_json::to_vec(&rustsimhub_wire::AuthRequest {
        protocol: AUTH_PROTOCOL,
        id: id.clone(),
        command: AuthCommand::NetworkDiagnostics { token },
    })
    .map_err(|_| "invalid request")?;
    let reply = forward(app, &body, &id).await?;
    let parsed: CoreResponse =
        serde_json::from_slice(&reply).map_err(|_| "invalid core response")?;
    if parsed.result.get("error").is_some() || parsed.result["available"] != true {
        return Err("network permission or monitor unavailable");
    }
    Ok(parsed.result)
}
pub(super) async fn api(State(app): State<App>, body: Bytes) -> Response {
    let Ok(_edge_permit) = app.limit.try_acquire() else {
        return denied(StatusCode::SERVICE_UNAVAILABLE);
    };
    if !app.rate.lock().is_ok_and(|mut r| r.take()) {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    }
    let request: TestRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return denied(StatusCode::BAD_REQUEST),
    };
    let snapshot = match auth_snapshot(&app, request.token.clone()).await {
        Ok(v) => v,
        Err(_) => return denied(StatusCode::FORBIDDEN),
    };
    let Ok(_permit) = TESTS.try_acquire() else {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    };
    {
        let Ok(mut last) = LAST.get_or_init(|| Mutex::new(None)).lock() else {
            return denied(StatusCode::SERVICE_UNAVAILABLE);
        };
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(15)) {
            return denied(StatusCode::TOO_MANY_REQUESTS);
        }
        *last = Some(Instant::now());
    }
    let outcome = timeout(
        Duration::from_secs(15),
        probe(
            &snapshot,
            &request.interface,
            &request.profile,
            request.allow_cellular,
        ),
    )
    .await;
    // Do not return diagnostic details into a session revoked while testing.
    if auth_snapshot(&app, request.token).await.is_err() {
        return denied(StatusCode::FORBIDDEN);
    }
    let v = match outcome {
        Ok(Ok(v)) => v,
        Ok(Err(reason)) => {
            serde_json::json!({"ok":false,"reason":reason,"ordinary_data_activated":false})
        }
        Err(_) => {
            serde_json::json!({"ok":false,"reason":"15 second test timeout","ordinary_data_activated":false})
        }
    };
    axum::Json(v).into_response()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_dns_and_private_destinations_are_rejected() {
        let q = query("www.baidu.com", [1, 2]);
        assert!(answer(&q, &q).is_err());
        for n in 0..q.len() {
            assert!(answer(&q[..n], &q).is_err());
        }
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "224.0.0.1",
            "0.1.2.3",
        ] {
            assert!(!public(ip.parse().unwrap()));
        }
        assert!(public("223.109.82.16".parse().unwrap()));
    }
    #[test]
    fn test_plan_requires_exact_connected_interface_and_addresses() {
        let v = serde_json::json!({"devices":[{"interface":"wwan0","kind":"cellular","state":"100 (connected)","ipv4":["10.0.0.2/30"],"dns_ipv4":["211.140.11.66"]}]});
        assert!(interface(&v, "wwan0").unwrap().2);
        for bad in ["usb0", "../wwan0", "wwan0;id", ""] {
            assert!(interface(&v, bad).is_err());
        }
    }
}
