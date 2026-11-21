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
const WALLET_HOST: &str = "mainnet-rpc-node-0001.nkn.org";
const DOMESTIC: [Node; 2] = [
    Node::Domestic(Ipv4Addr::new(183, 53, 109, 99)),
    Node::Domestic(Ipv4Addr::new(183, 53, 109, 45)),
];
// Default mainnet seed list from nkn-sdk-go. Each endpoint was probed from wlan0.
const OFFICIAL_SEEDS: [Node; 8] = [
    Node::Seed("mainnet-seed-0001.nkn.org"),
    Node::Seed("mainnet-seed-0002.nkn.org"),
    Node::Seed("mainnet-seed-0003.nkn.org"),
    Node::Seed("mainnet-seed-0004.nkn.org"),
    Node::Seed("mainnet-seed-0005.nkn.org"),
    Node::Seed("mainnet-seed-0006.nkn.org"),
    Node::Seed("mainnet-seed-0007.nkn.org"),
    Node::Seed("mainnet-seed-0008.nkn.org"),
];
const TOPIC_PREFIX: &str = "rustsimhub-usage-v1-";
const BODY_MAX: usize = 4096;
static QUERY: Semaphore = Semaphore::const_new(1);
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Node {
    Domestic(Ipv4Addr),
    Wallet,
    Seed(&'static str),
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

async fn first_ok<T, F>(attempts: impl IntoIterator<Item = F>) -> Option<T>
where
    T: Send + 'static,
    F: Future<Output = Result<T>> + Send + 'static,
{
    let mut active = JoinSet::new();
    for attempt in attempts {
        active.spawn(attempt);
    }
    while let Some(result) = active.join_next().await {
        if let Ok(Ok(value)) = result {
            return Some(value);
        }
    }
    None
}

fn dns_servers(config: &str) -> [Option<Ipv4Addr>; 2] {
    let mut servers = config.lines().filter_map(|line| {
        let mut parts = line.split_whitespace();
        if parts.next()? != "nameserver" {
            return None;
        }
        let ip = parts.next()?.parse::<Ipv4Addr>().ok()?;
        (!(ip.is_loopback() || ip.is_unspecified() || ip.is_multicast())).then_some(ip)
    });
    let first = servers.next();
    let second = servers.find(|ip| Some(*ip) != first);
    [first, second]
}

async fn node_counts(source: Ipv4Addr, dns: [Option<Ipv4Addr>; 2], node: Node) -> Result<[u64; 3]> {
    let resolve = |host| async move {
        first_ok(dns.into_iter().flatten().map(|server| async move {
            network_test::resolve_ipv4(source, server, WIFI, host).await
        }))
        .await
        .ok_or("NKN DNS unavailable")
    };
    let (addr, authority, path) = match node {
        Node::Domestic(ip) => {
            let addr = SocketAddrV4::new(ip, 30003);
            (addr, addr.to_string(), "/")
        }
        Node::Wallet => (
            SocketAddrV4::new(resolve(WALLET_HOST).await?, 443),
            WALLET_HOST.to_owned(),
            "/mainnet/api/wallet",
        ),
        Node::Seed(host) => (
            SocketAddrV4::new(resolve(host).await?, 30003),
            format!("{host}:30003"),
            "/",
        ),
    };
    let count = |window| {
        let authority = authority.as_str();
        async move {
            let stream = network_test::connect_ipv4(source, WIFI, addr).await?;
            if node == Node::Wallet {
                let tls = network_test::connect_tls(stream, WALLET_HOST).await?;
                request(tls, authority, path, window).await
            } else {
                request(stream, authority, path, window).await
            }
        }
    };
    let (daily, monthly, quarterly) = tokio::try_join!(count("24h"), count("30d"), count("90d"))?;
    Ok([daily, monthly, quarterly])
}

async fn query_counts<F>(probe: impl Fn(Node) -> F) -> Result<(&'static str, [u64; 3])>
where
    F: Future<Output = Result<[u64; 3]>> + Send + 'static,
{
    if let Ok(Some(counts)) = timeout(
        Duration::from_millis(1500),
        first_ok(DOMESTIC.into_iter().map(&probe)),
    )
    .await
    {
        return Ok(("domestic", counts));
    }
    if let Ok(Ok(counts)) = timeout(Duration::from_millis(2200), probe(Node::Wallet)).await {
        return Ok(("official", counts));
    }
    for batch in OFFICIAL_SEEDS.chunks(4) {
        if let Ok(Some(counts)) = timeout(
            Duration::from_millis(2400),
            first_ok(batch.iter().copied().map(&probe)),
        )
        .await
        {
            return Ok(("official", counts));
        }
    }
    Err("no NKN count endpoint available")
}

async fn fetch_counts(source: Ipv4Addr) -> Result<(&'static str, [u64; 3])> {
    let config = std::fs::read_to_string("/etc/resolv.conf").unwrap_or_default();
    let dns = dns_servers(&config);
    query_counts(move |node| node_counts(source, dns, node)).await
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
    fn uses_two_distinct_usable_ipv4_resolvers() {
        assert_eq!(
            dns_servers("nameserver 127.0.0.53\nnameserver ::1\nnameserver 8.8.8.8\nnameserver 8.8.8.8\nnameserver 192.168.2.1\n"),
            [Some(Ipv4Addr::new(8, 8, 8, 8)), Some(Ipv4Addr::new(192,168,2,1))]
        );
    }

    #[tokio::test]
    async fn tries_domestic_before_wallet_then_both_seed_batches() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let result = query_counts({
            let seen = Arc::clone(&seen);
            move |node| {
                seen.lock().unwrap().push(node);
                async move {
                    if node == OFFICIAL_SEEDS[7] {
                        Ok([0, 2, 3])
                    } else {
                        Err("offline")
                    }
                }
            }
        })
        .await;
        assert_eq!(result, Ok(("official", [0, 2, 3])));
        let expected: Vec<_> = DOMESTIC
            .into_iter()
            .chain([Node::Wallet])
            .chain(OFFICIAL_SEEDS)
            .collect();
        assert_eq!(*seen.lock().unwrap(), expected);
    }

    #[tokio::test]
    async fn zero_from_one_domestic_node_is_success() {
        let result = query_counts(|node| async move {
            match node {
                Node::Domestic(_) if node == DOMESTIC[1] => Ok([0, 0, 0]),
                Node::Domestic(_) => Err("offline"),
                _ => panic!("domestic success must stop before fallback"),
            }
        })
        .await;
        assert_eq!(result, Ok(("domestic", [0, 0, 0])));
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
