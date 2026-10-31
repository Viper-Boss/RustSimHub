// SPDX-License-Identifier: AGPL-3.0-only
//! One explicit implicit-TLS SMTP attempt, with no production queue/IPC adapter.
//! The caller supplies the approved Connector and a fresh, stage-bound fence.
//! Binding/claim metadata and an immutable fingerprint are NOT an auth proof.
use crate::{model::uuid, Binding, Connector, Error, Failure, Message, Result, Secret, Share};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    cell::Cell,
    sync::Arc,
    time::{Duration, Instant},
};
use time::{
    format_description::well_known::{Rfc2822, Rfc3339},
    OffsetDateTime,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    sync::watch,
    time::timeout,
};
use tokio_rustls::{
    rustls::{
        self,
        pki_types::{CertificateDer, ServerName},
        ClientConfig, RootCertStore,
    },
    TlsConnector,
};
use zeroize::Zeroizing;

const MAX_DATA: usize = 16384;
const MAX_REPLY: usize = 8192;
const MAX_LINE: usize = 512;
const MAX_REPLY_LINES: usize = 32;
const MAX_BUDGET: Duration = Duration::from_secs(10);

pub struct EmailConfig {
    host: Secret,
    username: Secret,
    password: Secret,
    from: Secret,
    to: Secret,
}
fn dns(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.parse::<std::net::IpAddr>().is_err()
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn address(value: &str) -> bool {
    if value.len() > 254 {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && dns(domain)
        && local.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&b))
        })
}
impl EmailConfig {
    /// Fixed port465/implicit TLS, one ASCII recipient, no URL/custom transport.
    pub fn new(
        host: Secret,
        username: Secret,
        password: Secret,
        from: Secret,
        to: Secret,
    ) -> Result<Self> {
        if !dns(host.expose())
            || username.expose().is_empty()
            || username.expose().len() > 64
            || !username.expose().bytes().all(|b| (33..=126).contains(&b))
            || password.expose().is_empty()
            || password.expose().len() > 256
            || !password.expose().bytes().all(|b| (32..=126).contains(&b))
            || !address(from.expose())
            || !address(to.expose())
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            host,
            username,
            password,
            from,
            to,
        })
    }
}
pub struct EmailRequest {
    config: EmailConfig,
    delivery_id: String,
    line: rustsimhub_domain::LineEpoch,
    data: Zeroizing<String>,
}
impl EmailRequest {
    /// No network, storage or background work. Header values never come from
    /// SMS sender/body; the committed timestamp is parsed before Date formatting.
    /// Share is still the caller's real Owner-approved policy, not consent here.
    pub fn prepare(
        config: EmailConfig,
        delivery_id: &str,
        message: &Message,
        share: Share,
    ) -> Result<Self> {
        message.validate()?;
        if !uuid(delivery_id) || !uuid(&message.id) || message.binary || message.duplicate_candidate
        {
            return Err(Error::Invalid);
        }
        let date =
            OffsetDateTime::parse(&message.timestamp, &Rfc3339).map_err(|_| Error::Invalid)?;
        let date = date.format(&Rfc2822).map_err(|_| Error::Invalid)?;
        let body = Zeroizing::new(match share {
            Share::Notice => "收到一条新短信，请在 RustSimHub 查看。".into(),
            Share::MaskedText => crate::masked_text(&message.body),
            Share::FullText => format!(
                "{}\n{}\n{}",
                message.sender, message.timestamp, message.body
            ),
        });
        let encoded = Zeroizing::new(STANDARD.encode(body.as_bytes()));
        let mut data=Zeroizing::new(format!("Date: {date}\r\nFrom: {}\r\nTo: {}\r\nSubject: RustSimHub notification\r\nMessage-ID: <rsh-{delivery_id}@rustsimhub.invalid>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: base64\r\n\r\n",config.from.expose(),config.to.expose()));
        for chunk in encoded.as_bytes().chunks(76) {
            data.push_str(std::str::from_utf8(chunk).map_err(|_| Error::Invalid)?);
            data.push_str("\r\n");
        }
        if data.len() > MAX_DATA
            || data
                .split("\r\n")
                .any(|line| line.len() + 2 > MAX_LINE || line.starts_with('.'))
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            config,
            delivery_id: delivery_id.into(),
            line: message.line,
            data,
        })
    }
}
pub struct EmailClaim {
    binding: Binding,
    id: String,
    attempt_id: String,
    generation: u64,
    request: EmailRequest,
}
impl EmailClaim {
    pub fn new(
        binding: Binding,
        id: String,
        attempt_id: String,
        generation: u64,
        request: EmailRequest,
    ) -> Result<Self> {
        binding.validate()?;
        if !uuid(&id)
            || !uuid(&attempt_id)
            || generation == 0
            || generation > i64::MAX as u64
            || id != request.delivery_id
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            binding,
            id,
            attempt_id,
            generation,
            request,
        })
    }
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn attempt_id(&self) -> &str {
        &self.attempt_id
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn line(&self) -> rustsimhub_domain::LineEpoch {
        self.request.line
    }
    /// Binds every immutable private field. This local candidate digest does
    /// not authorize a send or define a new production outbox/worker format.
    pub fn fingerprint(&self) -> Result<[u8; 32]> {
        #[derive(Serialize)]
        struct Fields<'a> {
            contract: u8,
            transport: &'static str,
            binding: &'a Binding,
            id: &'a str,
            attempt_id: &'a str,
            generation: u64,
            line: rustsimhub_domain::LineEpoch,
            host: &'a Secret,
            username: &'a Secret,
            password: &'a Secret,
            from: &'a Secret,
            to: &'a Secret,
            data: &'a str,
        }
        let c = &self.request.config;
        let bytes = Zeroizing::new(serde_json::to_vec(&Fields {
            contract: 1,
            transport: "smtp",
            binding: &self.binding,
            id: &self.id,
            attempt_id: &self.attempt_id,
            generation: self.generation,
            line: self.line(),
            host: &c.host,
            username: &c.username,
            password: &c.password,
            from: &c.from,
            to: &c.to,
            data: self.request.data.as_str(),
        })?);
        if bytes.len() > 65536 {
            return Err(Error::Invalid);
        }
        Ok(Sha256::digest(bytes.as_slice()).into())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailStage {
    BeforeConnect,
    BeforeTls,
    BeforeAuthenticate,
    BeforeMailFrom,
    BeforeRecipient,
    BeforeData,
    BeforeSubmit,
}
#[allow(async_fn_in_trait)]
pub trait EmailFence {
    /// Must check the actual current Owner/realm/SIM/configuration/attempt and
    /// exact fingerprint at EACH stage. No current Core implementation is
    /// supplied: worker4 supports only its three HTTPS stages, not this port.
    async fn check(&mut self, stage: EmailStage, claim: &EmailClaim) -> Result<()>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailOutcome {
    NotSubmitted {
        reason: Failure,
    },
    /// Only acceptance by this SMTP server after the final DATA terminator.
    /// No automatic conversion to an untagged HTTP worker4 receipt exists.
    Confirmed {
        smtp_code: u16,
    },
    Unknown {
        reason: Failure,
    },
}
pub struct EmailSender {
    tls: Arc<ClientConfig>,
    budget: Duration,
}
fn live(
    cancel: &watch::Receiver<bool>,
    started: Instant,
    budget: Duration,
) -> std::result::Result<(), Failure> {
    if *cancel.borrow() || cancel.has_changed().is_err() {
        return Err(Failure::Cancelled);
    }
    if started.elapsed() >= budget {
        return Err(Failure::Deadline);
    }
    Ok(())
}
impl EmailSender {
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
        let tls =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| Error::Invalid)?
                .with_root_certificates(roots)
                .with_no_client_auth();
        Ok(Self {
            tls: Arc::new(tls),
            budget: MAX_BUDGET,
        })
    }
    pub fn with_budget(mut self, budget: Duration) -> Result<Self> {
        if budget.is_zero() || budget > MAX_BUDGET {
            return Err(Error::Invalid);
        }
        self.budget = budget;
        Ok(self)
    }
    pub async fn deliver(
        &self,
        claim: &EmailClaim,
        connector: &mut impl Connector,
        fence: &mut impl EmailFence,
        mut cancel: watch::Receiver<bool>,
    ) -> EmailOutcome {
        let may_submit = Cell::new(false);
        if *cancel.borrow() || cancel.has_changed().is_err() {
            return EmailOutcome::NotSubmitted {
                reason: Failure::Cancelled,
            };
        }
        let started = Instant::now();
        let cancellation = cancel.clone();
        let alive = || live(&cancellation, started, self.budget);
        let work = async {
            alive()?;
            claim.fingerprint().map_err(|_| Failure::InvalidRequest)?;
            let c = &claim.request.config;
            let name = ServerName::try_from(c.host.expose().to_owned())
                .map_err(|_| Failure::InvalidRequest)?;
            fence
                .check(EmailStage::BeforeConnect, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            let tcp = connector
                .connect(c.host.expose(), 465)
                .await
                .map_err(|_| Failure::Connect)?;
            alive()?;
            fence
                .check(EmailStage::BeforeTls, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            let tls = TlsConnector::from(self.tls.clone())
                .connect(name, tcp)
                .await
                .map_err(|_| Failure::Tls)?;
            alive()?;
            let mut io = BufReader::new(tls);
            let mut total = 0;
            reply(&mut io, 220, &mut total).await?;
            alive()?;
            io.write_all(b"EHLO rustsimhub.invalid\r\n")
                .await
                .map_err(|_| Failure::Protocol)?;
            if !reply(&mut io, 250, &mut total).await? {
                return Err(Failure::Protocol);
            }
            alive()?;
            fence
                .check(EmailStage::BeforeAuthenticate, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            let mut sasl = Zeroizing::new(Vec::with_capacity(322));
            sasl.push(0);
            sasl.extend_from_slice(c.username.expose().as_bytes());
            sasl.push(0);
            sasl.extend_from_slice(c.password.expose().as_bytes());
            let encoded = Zeroizing::new(STANDARD.encode(sasl.as_slice()));
            let auth = Zeroizing::new(format!("AUTH PLAIN {}\r\n", encoded.as_str()));
            if auth.len() > MAX_LINE {
                return Err(Failure::InvalidRequest);
            }
            io.write_all(auth.as_bytes())
                .await
                .map_err(|_| Failure::Protocol)?;
            reply(&mut io, 235, &mut total).await?;
            alive()?;
            fence
                .check(EmailStage::BeforeMailFrom, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            let from = Zeroizing::new(format!("MAIL FROM:<{}>\r\n", c.from.expose()));
            io.write_all(from.as_bytes())
                .await
                .map_err(|_| Failure::Protocol)?;
            reply(&mut io, 250, &mut total).await?;
            alive()?;
            fence
                .check(EmailStage::BeforeRecipient, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            let to = Zeroizing::new(format!("RCPT TO:<{}>\r\n", c.to.expose()));
            io.write_all(to.as_bytes())
                .await
                .map_err(|_| Failure::Protocol)?;
            reply(&mut io, 250, &mut total).await?;
            alive()?;
            fence
                .check(EmailStage::BeforeData, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            io.write_all(b"DATA\r\n")
                .await
                .map_err(|_| Failure::Protocol)?;
            reply(&mut io, 354, &mut total).await?;
            alive()?;
            // Even a lost final permission ACK cannot prove that body write
            // permission was never delivered. No safe-to-retry assertion here.
            may_submit.set(true);
            fence
                .check(EmailStage::BeforeSubmit, claim)
                .await
                .map_err(|_| Failure::Denied)?;
            alive()?;
            for chunk in claim.request.data.as_bytes().chunks(1024) {
                alive()?;
                io.write_all(chunk).await.map_err(|_| Failure::Protocol)?;
            }
            alive()?;
            io.write_all(b".\r\n")
                .await
                .map_err(|_| Failure::Protocol)?;
            io.flush().await.map_err(|_| Failure::Protocol)?;
            reply(&mut io, 250, &mut total).await?;
            alive()?;
            Ok(())
        };
        let mut result = tokio::select! {biased;
            _=cancel.changed()=>Err(Failure::Cancelled),
            result=timeout(self.budget,work)=>result.unwrap_or(Err(Failure::Deadline)),
        };
        // A ready future that blocked its executor past the absolute budget
        // must not turn a late server reply into timely confirmation.
        if started.elapsed() >= self.budget {
            result = Err(Failure::Deadline);
        }
        if *cancel.borrow() || cancel.has_changed().is_err() {
            result = Err(Failure::Cancelled);
        }
        match result {
            Ok(()) => EmailOutcome::Confirmed { smtp_code: 250 },
            Err(reason) if may_submit.get() => EmailOutcome::Unknown { reason },
            Err(reason) => EmailOutcome::NotSubmitted { reason },
        }
    }
}

async fn reply<S: tokio::io::AsyncRead + Unpin>(
    io: &mut BufReader<S>,
    expected: u16,
    total: &mut usize,
) -> std::result::Result<bool, Failure> {
    let mut auth_plain = false;
    for _ in 0..MAX_REPLY_LINES {
        let mut line = Zeroizing::new(Vec::with_capacity(MAX_LINE));
        loop {
            if line.len() == MAX_LINE || *total == MAX_REPLY {
                return Err(Failure::TooLarge);
            }
            let b = io.read_u8().await.map_err(|_| Failure::Protocol)?;
            *total += 1;
            line.push(b);
            if line.ends_with(b"\r\n") {
                break;
            }
            if b == b'\n' {
                return Err(Failure::Protocol);
            }
        }
        if line.len() < 5 || !line[..3].iter().all(u8::is_ascii_digit) {
            return Err(Failure::Protocol);
        }
        let code = u16::from(line[0] - b'0') * 100
            + u16::from(line[1] - b'0') * 10
            + u16::from(line[2] - b'0');
        if code != expected {
            return Err(Failure::Unconfirmed);
        }
        let (continued, text) = if line.len() == 5 {
            (false, &b""[..])
        } else {
            if !b" -".contains(&line[3]) {
                return Err(Failure::Protocol);
            }
            (line[3] == b'-', &line[4..line.len() - 2])
        };
        if !text.iter().all(|b| (32..=126).contains(b)) {
            return Err(Failure::Protocol);
        }
        let mut words = text.split(|b| *b == b' ').filter(|p| !p.is_empty());
        if words
            .next()
            .is_some_and(|s| s.eq_ignore_ascii_case(b"AUTH"))
            && words.any(|s| s.eq_ignore_ascii_case(b"PLAIN"))
        {
            auth_plain = true;
        }
        if !continued {
            // No PIPELINING: already buffered replies for a command that has
            // not been sent cannot be accepted as its later acknowledgement.
            if !io.buffer().is_empty() {
                return Err(Failure::Protocol);
            }
            return Ok(auth_plain);
        }
    }
    Err(Failure::TooLarge)
}
