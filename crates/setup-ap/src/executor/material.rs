// SPDX-License-Identifier: AGPL-3.0-only
//! Root-private material and a real pinned TLS + exact AP Host/Origin identity probe.
use super::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, ServerName},
    },
    TlsConnector,
};
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateConfig {
    format: u16,
    realm: String,
    core_uid: u32,
    device_public_key: [u8; 32],
    ssid: String,
    passphrase: String,
    uuid: String,
}
pub(super) struct Material {
    directory: Directory,
    pub(super) credentials: Credentials,
    pub(super) core_uid: u32,
    pub(super) realm: String,
    pub(super) digest: String,
    public_key: [u8; 32],
    certificate: CertificateDer<'static>,
    last_probe: Mutex<Option<Instant>>,
    last_ap_probe: Mutex<Option<Instant>>,
    endpoint: std::net::SocketAddr,
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn digest(config: &[u8], certificate: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b"RustSimHub.SetupMaterial.v1\0");
    h.update((config.len() as u64).to_be_bytes());
    h.update(config);
    h.update((certificate.len() as u64).to_be_bytes());
    h.update(certificate);
    hex(&h.finalize())
}
impl Material {
    pub(super) fn root(path: &Path) -> Result<Self> {
        Self::open(Directory::open(path, 0, 0, 0o700)?)
    }
    pub(super) fn open(directory: Directory) -> Result<Self> {
        let raw = Zeroizing::new(
            directory
                .read_name("config.json", 0o600)?
                .ok_or(Error::Denied)?,
        );
        let cert = directory
            .read_name("cert.der", 0o600)?
            .ok_or(Error::Denied)?;
        let config: PrivateConfig = serde_json::from_slice(&raw).map_err(|_| Error::Invalid)?;
        let password = Zeroizing::new(config.passphrase);
        let valid_realm = config.realm.len() == 36
            && config.realm.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
                }
            });
        if config.format != 1
            || !valid_realm
            || config.core_uid == 0
            || config.device_public_key == [0; 32]
            || cert.is_empty()
        {
            return Err(Error::Denied);
        }
        Ok(Self {
            digest: digest(&raw, &cert),
            directory,
            credentials: Credentials::from_private_parts(config.ssid, password, config.uuid)?,
            core_uid: config.core_uid,
            realm: config.realm,
            public_key: config.device_public_key,
            certificate: CertificateDer::from(cert),
            last_probe: Mutex::new(None),
            last_ap_probe: Mutex::new(None),
            endpoint: std::net::SocketAddr::from(([127, 0, 0, 1], crate::HTTPS_PORT)),
        })
    }
    pub(super) fn device_hash(&self) -> String {
        hex(&Sha256::digest(self.public_key))
    }
    pub(super) fn device_public_key_hex(&self) -> String {
        hex(&self.public_key)
    }
    pub(super) fn matches(&self, realm: &str, uid: u32, key: &[u8; 32]) -> bool {
        self.realm == realm && self.core_uid == uid && self.public_key == *key
    }
    fn unchanged(&self) -> Result<()> {
        let raw = Zeroizing::new(
            self.directory
                .read_name("config.json", 0o600)?
                .ok_or(Error::Denied)?,
        );
        let cert = self
            .directory
            .read_name("cert.der", 0o600)?
            .ok_or(Error::Denied)?;
        if digest(&raw, &cert) != self.digest {
            return Err(Error::Changed);
        }
        Ok(())
    }
    async fn probe(&self, endpoint: std::net::SocketAddr) -> Result<()> {
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(self.certificate.clone())
            .map_err(|_| Error::Invalid)?;
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));
        let tcp = TcpStream::connect(endpoint).await.map_err(|_| Error::Io)?;
        // TCP is loopback only; name/validity/chain verification is for the actual fixed AP IP.
        let name = ServerName::IpAddress(
            crate::ADDRESS
                .parse::<std::net::IpAddr>()
                .map_err(|_| Error::Invalid)?
                .into(),
        );
        let mut stream = connector
            .connect(name, tcp)
            .await
            .map_err(|_| Error::Denied)?;
        let certificates = stream
            .get_ref()
            .1
            .peer_certificates()
            .ok_or(Error::Denied)?;
        if certificates.len() != 1 || certificates[0].as_ref() != self.certificate.as_ref() {
            return Err(Error::Changed);
        }
        let mut random = [0u8; 12];
        getrandom::fill(&mut random).map_err(|_| Error::Io)?;
        let id = format!("setup-{}", hex(&random));
        let body = serde_json::to_vec(
            &serde_json::json!({"protocol":2,"id":id,"command":{"op":"identity"}}),
        )
        .map_err(|_| Error::Invalid)?;
        let request = format!("POST /api/v2 HTTP/1.1\r\nHost: {}:{}\r\nOrigin: https://{}:{}\r\nContent-Type: application/json\r\nX-RustSimHub-Request: 2\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", crate::ADDRESS,crate::HTTPS_PORT,crate::ADDRESS,crate::HTTPS_PORT,body.len());
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|_| Error::Io)?;
        stream.write_all(&body).await.map_err(|_| Error::Io)?;
        let mut raw = Vec::new();
        loop {
            if let Some(body) = response(&raw)? {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Reply {
                    protocol: u16,
                    id: String,
                    result: serde_json::Value,
                }
                let reply: Reply = serde_json::from_slice(body).map_err(|_| Error::Invalid)?;
                let key: [u8; 32] = serde_json::from_value(
                    reply
                        .result
                        .get("public_key")
                        .cloned()
                        .ok_or(Error::Denied)?,
                )
                .map_err(|_| Error::Denied)?;
                if reply.protocol != 2
                    || reply.id != id
                    || key != self.public_key
                    || reply
                        .result
                        .get("protocol")
                        .and_then(serde_json::Value::as_u64)
                        != Some(2)
                    || reply
                        .result
                        .get("signature_algorithm")
                        .and_then(serde_json::Value::as_str)
                        != Some("ed25519")
                    || reply.result.get("error").is_some()
                {
                    return Err(Error::Changed);
                }
                return Ok(());
            }
            if raw.len() >= 8192 {
                return Err(Error::Invalid);
            }
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).await.map_err(|_| Error::Io)?;
            if n == 0 {
                return Err(Error::Io);
            }
            raw.extend_from_slice(&chunk[..n]);
        }
    }
}
impl Material {
    async fn cached_probe(
        &self,
        cache: &Mutex<Option<Instant>>,
        endpoint: std::net::SocketAddr,
    ) -> Result<()> {
        self.unchanged()?;
        let mut last = cache.lock().await;
        if last.is_some_and(|v| v.elapsed() < Duration::from_secs(4)) {
            return Ok(());
        }
        *last = None;
        timeout(Duration::from_secs(2), self.probe(endpoint))
            .await
            .map_err(|_| Error::Busy)??;
        self.unchanged()?;
        *last = Some(Instant::now());
        Ok(())
    }
}
impl Ready for Material {
    async fn check(&self) -> Result<()> {
        self.cached_probe(&self.last_probe, self.endpoint).await
    }
    async fn ap_check(&self) -> Result<()> {
        let endpoint = std::net::SocketAddr::new(
            crate::ADDRESS.parse().map_err(|_| Error::Invalid)?,
            crate::HTTPS_PORT,
        );
        self.cached_probe(&self.last_ap_probe, endpoint).await
    }
}
fn response(raw: &[u8]) -> Result<Option<&[u8]>> {
    if raw.len() > 8192 {
        return Err(Error::Invalid);
    }
    let mut headers = [httparse::EMPTY_HEADER; 24];
    let mut response = httparse::Response::new(&mut headers);
    let size = match response.parse(raw).map_err(|_| Error::Invalid)? {
        httparse::Status::Partial => return Ok(None),
        httparse::Status::Complete(n) => n,
    };
    if size > 4096 || response.code != Some(200) || response.version != Some(1) {
        return Err(Error::Denied);
    }
    let mut length = None;
    let mut content_type = None;
    for h in response.headers {
        if h.name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(Error::Denied);
        }
        if h.name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || h.value.is_empty() || !h.value.iter().all(u8::is_ascii_digit) {
                return Err(Error::Invalid);
            }
            length = Some(
                std::str::from_utf8(h.value)
                    .map_err(|_| Error::Invalid)?
                    .parse::<usize>()
                    .map_err(|_| Error::Invalid)?,
            );
        }
        if h.name.eq_ignore_ascii_case("content-type") {
            if content_type.is_some() {
                return Err(Error::Invalid);
            }
            content_type = Some(h.value);
        }
    }
    let length = length.ok_or(Error::Invalid)?;
    if length == 0
        || length > 4096
        || content_type != Some(b"application/json".as_slice())
        || raw.len() > size + length
    {
        return Err(Error::Invalid);
    }
    if raw.len() < size + length {
        return Ok(None);
    }
    Ok(Some(&raw[size..]))
}
#[cfg(test)]
mod tests;
