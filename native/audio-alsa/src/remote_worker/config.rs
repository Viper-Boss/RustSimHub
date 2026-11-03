// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde::Deserialize;
use std::net::SocketAddr;
use tokio_rustls::rustls::{pki_types::CertificateDer, RootCertStore};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Installed {
    protocol: u16,
    relay_handle: String,
    address: SocketAddr,
    hostname: String,
    ca_der: Vec<u8>,
    username: String,
    password: String,
    // Absolute CLOCK_BOOTTIME, not a renewable TTL supplied by an Edge request.
    until_ms: u64,
}
pub(super) type Registry = Arc<dyn Fn(&str) -> Result<crate::remote::Registration> + Send + Sync>;
pub(super) fn installed(bytes: &[u8]) -> Result<Registry> {
    if bytes.len() > wire::MAX_FRAME
        || bytes.iter().copied().find(|c| !c.is_ascii_whitespace()) != Some(b'{')
    {
        return Err(Failure::BadRequest);
    }
    let config: Installed = serde_json::from_slice(bytes).map_err(|_| Failure::BadRequest)?;
    if config.protocol != wire::PROTOCOL
        || !wire::relay_handle(&config.relay_handle)
        || config.ca_der.is_empty()
        || config.ca_der.len() > 8192
    {
        return Err(Failure::BadRequest);
    }
    let now = local::now_ms();
    let remaining = config
        .until_ms
        .checked_sub(now)
        .filter(|v| (1..=3_600_000).contains(v))
        .ok_or(Failure::Denied)?;
    let until = Instant::now() + Duration::from_millis(remaining);
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(config.ca_der))
        .map_err(|_| Failure::Denied)?;
    // Validate immediately, before creating any control socket. No DNS or network I/O.
    crate::remote::Registration::from_installed(
        config.address,
        &config.hostname,
        roots.clone(),
        config.username.clone(),
        config.password.clone(),
        until,
    )
    .map_err(|_| Failure::Denied)?;
    Ok(Arc::new(move |handle| {
        if handle != config.relay_handle || local::now_ms() >= config.until_ms {
            return Err(Failure::Denied);
        }
        crate::remote::Registration::from_installed(
            config.address,
            &config.hostname,
            roots.clone(),
            config.username.clone(),
            config.password.clone(),
            until,
        )
        .map_err(|_| Failure::Denied)
    }))
}
