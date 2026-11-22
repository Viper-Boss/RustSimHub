// SPDX-License-Identifier: AGPL-3.0-only
//! Read public installation counts over wlan0. No account, seed, or arbitrary RPC input.
use super::*;
use axum::{body::Body, http::Request as HttpRequest};
use hyper::{body::Body as _, client::conn::http1};
use std::{
    future::{poll_fn, Future},
    net::{Ipv4Addr, SocketAddrV4},
};
use tokio::io::{AsyncRead, AsyncWrite};

const WIFI: &str = "wlan0";
const OFFICIAL: &str = "mainnet-rpc-node-0001.nkn.org";
const DOMESTIC: [Ipv4Addr; 2] = [
    Ipv4Addr::new(183, 53, 109, 99),
    Ipv4Addr::new(183, 53, 109, 45),
];
const TOPIC_PREFIX: &str = "rustsimhub-usage-v1-";
const BODY_MAX: usize = 4096;
static QUERY: Semaphore = Semaphore::const_new(1);
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Clone, Copy)]
enum Node {
    Domestic(Ipv4Addr),
    Official(Ipv4Addr),
}

async fn request(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    host: &str,
    path: &str,
    window: &str,
) -> Result<u64> {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": "rsh-usage", "method": "getsubscriberscount",
        "params": {"topic": format!("{TOPIC_PREFIX}{window}"), "subscriberHashPrefix": ""}
    })
    .to_string();
    let req = HttpRequest::post(path)
        .header(header::HOST, host)
        .header(header::CONNECTION, "close")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::USER_AGENT, "RustSimHub-Progress/1.0")
        .header(header::CONTENT_LENGTH, body.len())
        .body(Body::from(body))
        .map_err(|_| "invalid NKN request")?;
    let (mut sender, connection) = http1::Builder::new()
        .max_buf_size(8192)
        .handshake(TokioIo::new(stream))
        .await
        .map_err(|_| "NKN HTTP handshake failed")?;
    // Both futures are scoped to this request and are dropped at its deadline.
    let read = async {
        let reply = sender
            .send_request(req)
            .await
            .map_err(|_| "NKN request failed")?;
        if reply.status() != StatusCode::OK {
            return Err("NKN HTTP status was not 200");
        }
        let mut body = Box::pin(reply.into_body());
        let mut bytes = Vec::new();
        while let Some(frame) = poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
            let frame = frame.map_err(|_| "NKN response read failed")?;
            if let Ok(data) = frame.into_data() {
                if bytes.len() + data.len() > BODY_MAX {
                    return Err("NKN response too large");
                }
                bytes.extend_from_slice(&data);
            }
        }
        decode(&bytes)
    };
    tokio::pin!(read);
    tokio::select! {
        result = &mut read => result,
        result = connection => {
            result.map_err(|_| "NKN connection failed")?;
            read.await
        }
    }
}

fn decode(bytes: &[u8]) -> Result<u64> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| "invalid NKN JSON")?;
    if value["jsonrpc"] != "2.0" || value["id"] != "rsh-usage" || value.get("error").is_some() {
        return Err("invalid NKN reply");
    }
    value["result"]
        .as_u64()
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or("invalid NKN count")
}

async fn count(source: Ipv4Addr, node: Node, window: &str) -> Result<u64> {
    match node {
        Node::Domestic(ip) => {
            let addr = SocketAddrV4::new(ip, 30003);
            let stream = network_test::connect_ipv4(source, WIFI, addr).await?;
            request(stream, &addr.to_string(), "/", window).await
        }
        Node::Official(ip) => {
            let tcp = network_test::connect_ipv4(source, WIFI, SocketAddrV4::new(ip, 443)).await?;
            let tls = network_test::connect_tls(tcp, OFFICIAL).await?;
            request(tls, OFFICIAL, "/mainnet/api/wallet", window).await
        }
    }
}

async fn window_counts(source: Ipv4Addr, node: Node) -> Result<[u64; 3]> {
    let (daily, monthly, quarterly) = tokio::try_join!(
        count(source, node, "24h"),
        count(source, node, "30d"),
        count(source, node, "90d"),
    )?;
    Ok([daily, monthly, quarterly])
}

fn dns_servers(config: &str) -> Vec<Ipv4Addr> {
    let mut servers = Vec::new();
    for line in config.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() != Some("nameserver") {
            continue;
        }
        let Some(ip) = fields
            .next()
            .and_then(|field| field.parse::<Ipv4Addr>().ok())
        else {
            continue;
        };
        if ip.is_loopback()
            || ip.is_unspecified()
            || ip.is_multicast()
            || ip.is_broadcast()
            || servers.contains(&ip)
        {
            continue;
        }
        servers.push(ip);
        if servers.len() == 3 {
            break;
        }
    }
    servers
}

async fn resolve_official<F>(
    servers: Vec<Ipv4Addr>,
    resolve: impl Fn(Ipv4Addr) -> F,
) -> Result<Ipv4Addr>
where
    F: Future<Output = Result<Ipv4Addr>> + Send + 'static,
{
    let mut queries = JoinSet::new();
    for dns in servers {
        queries.spawn(resolve(dns));
    }
    while let Some(result) = queries.join_next().await {
        if let Ok(Ok(address)) = result {
            return Ok(address);
        }
    }
    Err("Wi-Fi DNS unavailable")
}

async fn fetch_counts(source: Ipv4Addr) -> Result<(&'static str, [u64; 3])> {
    for ip in DOMESTIC {
        if let Ok(Ok(counts)) = timeout(
            Duration::from_millis(1500),
            window_counts(source, Node::Domestic(ip)),
        )
        .await
        {
            return Ok(("domestic", counts));
        }
    }
    let config = std::fs::read_to_string("/etc/resolv.conf").map_err(|_| "DNS unavailable")?;
    let ip = resolve_official(dns_servers(&config), |dns| {
        network_test::resolve_ipv4(source, dns, WIFI, OFFICIAL)
    })
    .await?;
    Ok(("official", window_counts(source, Node::Official(ip)).await?))
}

pub(super) async fn api(body: Bytes) -> Response {
    if body.as_ref() != b"{}" {
        return denied(StatusCode::BAD_REQUEST);
    }
    let Some(source) = addresses::wlan0().into_iter().next() else {
        return denied(StatusCode::SERVICE_UNAVAILABLE);
    };
    let Ok(_permit) = QUERY.try_acquire() else {
        return denied(StatusCode::TOO_MANY_REQUESTS);
    };
    {
        let Ok(mut last) = LAST.lock() else {
            return denied(StatusCode::SERVICE_UNAVAILABLE);
        };
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(5)) {
            return denied(StatusCode::TOO_MANY_REQUESTS);
        }
        *last = Some(Instant::now());
    }
    match timeout(Duration::from_secs(9), fetch_counts(source)).await {
        Ok(Ok((node, counts))) => axum::Json(serde_json::json!({
            "source": node, "counts": counts
        }))
        .into_response(),
        _ => denied(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_up_to_three_distinct_usable_ipv4_resolvers() {
        let config = "search test.invalid\n\
            nameserver 127.0.0.53\nnameserver ::1\nnameserver 0.0.0.0\n\
            nameserver 224.0.0.1\nnameserver 255.255.255.255\n\
            nameserver 10.0.0.1\nnameserver 10.0.0.1\n\
            nameserver 10.0.0.2\nnameserver 10.0.0.3\nnameserver 10.0.0.4";
        assert_eq!(
            dns_servers(config),
            [1, 2, 3].map(|last| Ipv4Addr::new(10, 0, 0, last))
        );
    }

    #[tokio::test]
    async fn an_unresponsive_primary_cannot_block_the_working_resolver() {
        let servers = dns_servers("nameserver 10.0.0.1\nnameserver 10.0.0.2\n");
        let address = Ipv4Addr::new(203, 0, 113, 1);
        let resolution = resolve_official(servers, |dns| async move {
            if dns.octets()[3] == 1 {
                std::future::pending::<()>().await;
            }
            Ok(address)
        });
        assert_eq!(
            timeout(Duration::from_millis(200), resolution).await,
            Ok(Ok(address))
        );
    }

    #[tokio::test]
    async fn reports_failure_without_making_counts_when_no_resolver_succeeds() {
        let servers = dns_servers("nameserver 10.0.0.1\nnameserver 10.0.0.2\n");
        let resolve = |_| async { Err("invalid DNS response") };
        for config in [servers, Vec::new()] {
            assert_eq!(
                resolve_official(config, resolve).await,
                Err("Wi-Fi DNS unavailable")
            );
        }
    }

    #[test]
    fn accepts_only_correlated_nonnegative_exact_counts() {
        assert_eq!(
            decode(br#"{"jsonrpc":"2.0","id":"rsh-usage","result":0}"#),
            Ok(0)
        );
        for body in [
            br#"{"id":"other","jsonrpc":"2.0","result":1}"#.as_slice(),
            br#"{"id":"rsh-usage","jsonrpc":"2.0","result":9007199254740992}"#,
            br#"{"id":"rsh-usage","jsonrpc":"2.0","error":{},"result":1}"#,
            br#"{"id":"rsh-usage","jsonrpc":"2.0","result":-1}"#,
            br#"{"id":"rsh-usage","jsonrpc":"2.0","result":"0"}"#,
        ] {
            assert!(decode(body).is_err());
        }
    }
}
