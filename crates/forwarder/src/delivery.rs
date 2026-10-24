// SPDX-License-Identifier: AGPL-3.0-only
//! One bounded HTTPS attempt. No ambient DNS, proxy, redirect, connection pool,
//! background driver, timer loop or retry. The caller MUST supply the actual
//! approved egress connector and a live Core fence, not a cached Owner boolean.
use crate::{
    model::{endpoint, uuid},
    Claim, Error, Result, State,
};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::rt::TokioIo;
use std::{cell::Cell, sync::Arc, time::Duration};
use tokio::{net::TcpStream, sync::watch, time::timeout};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, ServerName},
        ClientConfig, RootCertStore,
    },
    TlsConnector,
};

const BUDGET: Duration = Duration::from_secs(10);
const MAX_RESPONSE: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    BeforeConnect,
    BeforeTls,
    BeforeSubmit,
}

/// Production must implement this using its approved, explicit network path.
/// In particular it cannot replace a disconnected Wi-Fi/USB path with a
/// default-route cellular socket. No fallback connector is provided here.
#[allow(async_fn_in_trait)]
pub trait Connector {
    async fn connect(&mut self, host: &str, port: u16) -> Result<TcpStream>;
}
/// Each call is a NEW live Core check of Owner, realm, configuration, attempt
/// and SIM. A Root-owned one-way cancellation channel also covers operations
/// which are already awaiting TLS/HTTP or a stalled fence/connector.
#[allow(async_fn_in_trait)]
pub trait Fence {
    async fn check(&mut self, stage: Stage, claim: &Claim) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    InvalidRequest,
    Denied,
    Connect,
    Tls,
    Protocol,
    TooLarge,
    Unconfirmed,
    Cancelled,
    Deadline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeliveryOutcome {
    NotSubmitted {
        reason: Failure,
    },
    Confirmed {
        status: u16,
    },
    Unknown {
        status: Option<u16>,
        reason: Failure,
    },
}
impl DeliveryOutcome {
    pub fn queue_state(self) -> State {
        match self {
            Self::Confirmed { .. } => State::Confirmed,
            Self::NotSubmitted { .. } => State::Failed,
            Self::Unknown { .. } => State::Unknown,
        }
    }
}

// Only verified roots enter the TLS builder; arbitrary ClientConfig/dangerous
// verifier objects are not accepted as a substitute for certificate validation.
pub struct HttpsSender {
    tls: Arc<ClientConfig>,
    budget: Duration,
}
impl HttpsSender {
    pub fn new(certificates: Vec<CertificateDer<'static>>) -> Result<Self> {
        if certificates.is_empty()
            || certificates.len() > 512
            || certificates.iter().map(|c| c.len()).sum::<usize>() > 1024 * 1024
        {
            return Err(Error::Invalid);
        }
        let mut roots = RootCertStore::empty();
        for certificate in certificates {
            roots.add(certificate).map_err(|_| Error::Invalid)?;
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut tls = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| Error::Invalid)?
            .with_root_certificates(roots)
            .with_no_client_auth();
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(Self {
            tls: Arc::new(tls),
            budget: BUDGET,
        })
    }
    /// Cancellation is one-way: true OR loss of the supervising sender stops
    /// the attempt. It never launches a cleanup request, reconnect or retry.
    pub async fn deliver(
        &self,
        claim: &Claim,
        connector: &mut impl Connector,
        fence: &mut impl Fence,
        mut cancel: watch::Receiver<bool>,
    ) -> DeliveryOutcome {
        let attempted = Cell::new(false);
        let observed_status = Cell::new(None);
        if *cancel.borrow() {
            return DeliveryOutcome::NotSubmitted {
                reason: Failure::Cancelled,
            };
        }
        let work = async {
            let request = &claim.request;
            if !uuid(&claim.id)
                || !uuid(&claim.attempt_id)
                || claim.generation == 0
                || claim.line.0 == 0
                || request.body.expose().len() > 32768
                || endpoint(request.url.expose()).is_err()
                || request.bearer.as_ref().is_some_and(|s| {
                    s.expose().is_empty()
                        || s.expose().len() > 2048
                        || !s.expose().bytes().all(|b| (33..=126).contains(&b))
                })
                || !serde_json::from_str::<serde_json::Value>(request.body.expose())
                    .is_ok_and(|v| v.is_object())
            {
                return Err(Failure::InvalidRequest);
            }
            let uri: http::Uri = request
                .url
                .expose()
                .parse()
                .map_err(|_| Failure::InvalidRequest)?;
            let authority = uri.authority().ok_or(Failure::InvalidRequest)?;
            let raw_host = uri.host().ok_or(Failure::InvalidRequest)?;
            let host = raw_host
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(raw_host);
            let name =
                ServerName::try_from(host.to_owned()).map_err(|_| Failure::InvalidRequest)?;
            let path = uri.path_and_query().map(|p| p.as_str()).unwrap_or("/");
            let mut builder = http::Request::builder()
                .method("POST")
                .uri(path)
                .header("Host", authority.as_str())
                .header("Content-Type", "application/json; charset=utf-8")
                .header("Accept", "application/json")
                .header("Accept-Encoding", "identity")
                .header("Connection", "close")
                .header("User-Agent", "RustSimHub-forwarder/1")
                .header("X-RustSimHub-Delivery-ID", claim.id.as_str());
            if let Some(bearer) = &request.bearer {
                builder = builder.header("Authorization", format!("Bearer {}", bearer.expose()));
            }
            let body = builder
                .body(Full::new(Bytes::copy_from_slice(
                    request.body.expose().as_bytes(),
                )))
                .map_err(|_| Failure::InvalidRequest)?;
            fence
                .check(Stage::BeforeConnect, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            let tcp = connector
                .connect(host, uri.port_u16().unwrap_or(443))
                .await
                .map_err(|_| Failure::Connect)?;
            fence
                .check(Stage::BeforeTls, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            let tls = TlsConnector::from(self.tls.clone())
                .connect(name, tcp)
                .await
                .map_err(|_| Failure::Tls)?;
            let (mut sender, connection) = hyper::client::conn::http1::Builder::new()
                .max_headers(32)
                .max_buf_size(8192)
                .handshake::<_, Full<Bytes>>(TokioIo::new(tls))
                .await
                .map_err(|_| Failure::Protocol)?;
            fence
                .check(Stage::BeforeSubmit, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            // Commit-to-attempt occurs BEFORE polling send_request. A lost reply
            // or failure from here on is conservatively Unknown, never retried.
            let exchange = async {
                attempted.set(true);
                let mut response = sender
                    .send_request(body)
                    .await
                    .map_err(|_| Failure::Protocol)?;
                let status = response.status().as_u16();
                observed_status.set(Some(status));
                if response
                    .headers()
                    .get("Content-Encoding")
                    .is_some_and(|h| !h.as_bytes().eq_ignore_ascii_case(b"identity"))
                {
                    return Err(Failure::Protocol);
                }
                if response.headers().get("Content-Length").is_some_and(|h| {
                    h.to_str()
                        .ok()
                        .and_then(|s| s.parse::<u64>().ok())
                        .is_none_or(|n| n > MAX_RESPONSE as u64)
                }) {
                    return Err(Failure::TooLarge);
                }
                let mut bytes = Vec::new();
                while let Some(frame) = response.body_mut().frame().await {
                    let frame = frame.map_err(|_| Failure::Protocol)?;
                    if let Some(data) = frame.data_ref() {
                        if data.len() > MAX_RESPONSE - bytes.len() {
                            return Err(Failure::TooLarge);
                        }
                        bytes.extend_from_slice(data);
                    }
                }
                if !request.acknowledged(status, &bytes) {
                    return Err(Failure::Unconfirmed);
                }
                Ok(status)
            };
            // Both futures remain owned HERE. Dropping/cancelling this attempt
            // drops its TLS socket too; no detached Hyper driver survives it.
            tokio::pin!(exchange);
            tokio::pin!(connection);
            tokio::select! {biased;
                result=&mut exchange=>result,
                result=&mut connection=>{result.map_err(|_|Failure::Protocol)?;exchange.await}
            }
        };
        let result = tokio::select! {biased;
            _=cancel.changed()=>Err(Failure::Cancelled),
            result=timeout(self.budget,work)=>result.unwrap_or(Err(Failure::Deadline)),
        };
        match result {
            Ok(status) => DeliveryOutcome::Confirmed { status },
            Err(reason) if attempted.get() => DeliveryOutcome::Unknown {
                status: observed_status.get(),
                reason,
            },
            Err(reason) => DeliveryOutcome::NotSubmitted { reason },
        }
    }
}

#[cfg(test)]
mod tests;
