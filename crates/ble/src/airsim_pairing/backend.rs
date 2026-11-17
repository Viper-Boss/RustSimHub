// SPDX-License-Identifier: AGPL-3.0-only

use super::DynError;
use rustsimhub_airsim::{
    base64url, canonical, decode_base64url, direct_route_proof, pcm_canonical, seal_registration,
    sign,
    voble::{call::clock_ms, EDGE_UID, LOCAL_CONTROL, LOCAL_PAIRING, LOCAL_PCM},
    PcmPreface, HEADER_NONCE, HEADER_SIGNATURE, HEADER_TIMESTAMP, HEADER_VERSION, PCM_MAGIC,
    PCM_READY,
};
use serde_json::{json, Value};
use std::{io, os::linux::net::SocketAddrExt, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{unix::SocketAddr, UnixStream},
    time::timeout,
};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REPLY: usize = 96 * 1024;

pub(super) type Secret = Zeroizing<[u8; 32]>;

#[derive(Clone)]
pub(super) struct Backend(pub Secret);

pub(super) enum Status {
    Refused,
    Unknown,
    Revoked,
}

impl Backend {
    pub(super) async fn call(
        &self,
        method: &str,
        target: &str,
        body: Value,
    ) -> Result<Value, Status> {
        let payload = if body.is_null() {
            Vec::new()
        } else {
            serde_json::to_vec(&body).map_err(|_| Status::Unknown)?
        };
        let nonce = random_nonce().map_err(|_| Status::Unknown)?;
        let now = clock_ms().map_err(|_| Status::Unknown)? / 1_000;
        let signature = sign(
            &self.0[..],
            &canonical(method, target, &payload, now, &nonce),
        );
        let header = format!("{HEADER_VERSION}: 1\r\n{HEADER_NONCE}: {nonce}\r\n{HEADER_TIMESTAMP}: {now}\r\n{HEADER_SIGNATURE}: {signature}\r\n");
        match request(LOCAL_CONTROL, method, target, &header, &payload).await {
            Ok((200..=299, value)) => Ok(value),
            Ok((401 | 403, _)) => Err(Status::Revoked),
            Ok((400..=499, _)) => Err(Status::Refused),
            Ok((503, value)) if value["error"] == "gateway_disabled" => Err(Status::Revoked),
            _ => Err(Status::Unknown),
        }
    }

    pub(super) async fn media(&self) -> io::Result<(UnixStream, usize)> {
        let now = clock_ms().map_err(|_| io::Error::other("clock unavailable"))? / 1_000;
        let mut preface = PcmPreface {
            timestamp: now,
            nonce: random_nonce()?,
            signature: String::new(),
        };
        preface.signature = sign(&self.0[..], &pcm_canonical(&preface));
        let hello = format!(
            "{PCM_MAGIC} {now} {} {}\n",
            preface.nonce, preface.signature
        );
        timeout(Duration::from_secs(3), async {
            let mut stream = connect(LOCAL_PCM).await?;
            stream.write_all(hello.as_bytes()).await?;
            let size = match stream.read_u32().await? {
                8_000 => 320,
                16_000 => 640,
                _ => return Err(io::ErrorKind::InvalidData.into()),
            };
            let mut marker = [0; PCM_READY.len()];
            stream.read_exact(&mut marker).await?;
            if marker != PCM_READY.as_bytes() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            Ok((stream, size))
        })
        .await?
    }
}

pub(super) async fn enroll(code: &str) -> Result<Secret, DynError> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(io::Error::other(
            "expected the six-digit code from the RustSimHub AirSIM web panel",
        )
        .into());
    }
    let (200, info) = request(LOCAL_PAIRING, "GET", "/pair/v1/info", "", &[]).await? else {
        return Err(io::Error::other("the RustSimHub AirSIM pairing window is closed").into());
    };
    let invalid = || io::Error::other("invalid local AirSIM pairing response");
    let session = info["session_id"].as_str().ok_or_else(invalid)?;
    let server_public = decode_base64url(info["server_public_key"].as_str().ok_or_else(invalid)?)?;
    let server_public: [u8; 32] = server_public.try_into().map_err(|_| invalid())?;
    let mut private_bytes = Zeroizing::new([0; 32]);
    let mut secret = Zeroizing::new([0; 32]);
    let mut nonce = [0; 12];
    fill(&mut *private_bytes)?;
    fill(&mut *secret)?;
    fill(&mut nonce)?;
    let private = StaticSecret::from(*private_bytes);
    if !private
        .diffie_hellman(&PublicKey::from(server_public))
        .was_contributory()
    {
        return Err(invalid().into());
    }
    let client_public = base64url(PublicKey::from(&private).as_bytes());
    let registration = Zeroizing::new(serde_json::to_vec(&json!({
        "vowlan_secret":base64url(&secret[..]), "device_id":"RustSimHub-VoBLE"
    }))?);
    let sealed = seal_registration(
        &private,
        &server_public,
        &registration,
        session,
        code,
        nonce,
    )?;
    let claim = serde_json::to_vec(&json!({
        "version":1, "session_id":session, "client_public_key":client_public, "sealed_registration":base64url(&sealed),
    }))?;
    let (200, reply) = request(LOCAL_PAIRING, "POST", "/pair/v1/claim", "", &claim).await? else {
        return Err(io::Error::other("the RustSimHub AirSIM pairing claim was refused").into());
    };
    let agent = reply["agent_id"].as_str().ok_or_else(invalid)?;
    if reply["route_proof"].as_str()
        != Some(&direct_route_proof(
            &secret[..],
            session,
            &client_public,
            agent,
            7592,
            7593,
        ))
    {
        return Err(invalid().into());
    }
    Ok(secret)
}

async fn connect(name: &[u8]) -> io::Result<UnixStream> {
    let address = SocketAddr::from(std::os::unix::net::SocketAddr::from_abstract_name(name)?);
    let stream = UnixStream::connect_addr(&address).await?;
    if stream.peer_cred()?.uid() != EDGE_UID {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(stream)
}

async fn request(
    name: &[u8],
    method: &str,
    route: &str,
    headers: &str,
    payload: &[u8],
) -> io::Result<(u16, Value)> {
    timeout(REQUEST_TIMEOUT, async {
        let mut conn = connect(name).await?;
        let head = format!(
            "{method} {route} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n",
            payload.len()
        );
        conn.write_all(head.as_bytes()).await?;
        conn.write_all(payload).await?;
        let mut bytes = Vec::new();
        conn.take(MAX_REPLY as u64 + 1).read_to_end(&mut bytes).await?;
        let invalid = || io::Error::from(io::ErrorKind::InvalidData);
        if bytes.len() > MAX_REPLY {
            return Err(invalid());
        }
        let end = bytes.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(invalid)?;
        let mut lines = std::str::from_utf8(&bytes[..end]).map_err(|_| invalid())?.split("\r\n");
        let mut status = lines.next().ok_or_else(invalid)?.split_ascii_whitespace();
        if status.next() != Some("HTTP/1.1") {
            return Err(invalid());
        }
        let status = status.next().and_then(|s| s.parse().ok()).ok_or_else(invalid)?;
        let data = serde_json::from_slice(&bytes[end + 4..]).map_err(|_| invalid())?;
        Ok((status, data))
    }).await?
}

fn fill(dst: &mut [u8]) -> io::Result<()> {
    getrandom::fill(dst).map_err(|_| io::Error::other("random source unavailable"))
}

fn random_nonce() -> io::Result<String> {
    let mut bytes = [0; 16];
    fill(&mut bytes)?;
    Ok(base64url(&bytes))
}
