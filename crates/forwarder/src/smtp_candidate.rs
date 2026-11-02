// SPDX-License-Identifier: AGPL-3.0-only
//! Opt-in outbox4/worker5 transport types and original SMTP/HTTPS dispatch.
//! No account, queue, worker, socket, migration or network starts on creation.
use crate::{
    email::{
        EmailClaim, EmailConfig, EmailFence, EmailOutcome, EmailRequest, EmailSender, EmailStage,
    },
    Claim, Connector, DeliveryOutcome, Destination, Error, Failure, Fence, HttpsSender, Message,
    PreparedRequest, Result, Secret, Share, Stage, State,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio_rustls::rustls::pki_types::CertificateDer;
use zeroize::Zeroizing;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailTls {
    Implicit,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreparedDeliveryV1 {
    Https { request: PreparedRequest },
    Smtp { request: EmailRequest },
}
impl From<PreparedRequest> for PreparedDeliveryV1 {
    fn from(request: PreparedRequest) -> Self {
        Self::Https { request }
    }
}
impl PreparedDeliveryV1 {
    pub fn is_smtp(&self) -> bool {
        matches!(self, Self::Smtp { .. })
    }
    pub fn validate(&self) -> Result<()> {
        let serialized = Zeroizing::new(serde_json::to_vec(self)?);
        if serialized.len() > 65536 {
            return Err(Error::Invalid);
        }
        match self {
            Self::Https { request } => {
                crate::model::endpoint(request.url.expose())?;
                if request.body.expose().len() > 32768
                    || request.bearer.as_ref().is_some_and(|v| {
                        v.expose().is_empty()
                            || v.expose().len() > 2048
                            || !v.expose().bytes().all(|b| (33..=126).contains(&b))
                    })
                    || !serde_json::from_str::<serde_json::Value>(request.body.expose())
                        .is_ok_and(|v| v.is_object())
                {
                    return Err(Error::Invalid);
                };
                Ok(())
            }
            Self::Smtp { request } => request.validate_candidate(),
        }
    }
    pub fn stages(&self) -> &'static [Stage] {
        if self.is_smtp() {
            &[
                Stage::BeforeConnect,
                Stage::BeforeTls,
                Stage::BeforeSmtpAuth,
                Stage::BeforeSmtpMailFrom,
                Stage::BeforeSmtpRcptTo,
                Stage::BeforeSmtpData,
                Stage::BeforeSubmit,
            ]
        } else {
            &[Stage::BeforeConnect, Stage::BeforeTls, Stage::BeforeSubmit]
        }
    }
}
pub(crate) fn prepare_email(
    destination: &Destination,
    message: &Message,
    share: Share,
    id: &str,
    header_date: time::OffsetDateTime,
) -> Result<PreparedDeliveryV1> {
    destination.validate()?;
    let Destination::Email {
        smtp_host,
        port: 465,
        tls: EmailTls::Implicit,
        username,
        password,
        mail_from,
        rcpt_to,
    } = destination
    else {
        return Err(Error::Invalid);
    };
    let copy = |secret: &Secret| Secret::new(secret.expose().to_owned());
    let config = EmailConfig::new(
        copy(smtp_host),
        copy(username),
        copy(password),
        copy(mail_from),
        copy(rcpt_to),
    )?;
    let request = EmailRequest::prepare_at(config, id, message, share, header_date)?;
    request.validate_candidate()?;
    Ok(PreparedDeliveryV1::Smtp { request })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfirmationV1 {
    Http { status: u16 },
    Smtp { code: u16 },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DeliveryOutcomeV1 {
    NotSubmitted {
        reason: Failure,
    },
    Confirmed {
        confirmation: ConfirmationV1,
    },
    Unknown {
        observed: Option<ConfirmationV1>,
        reason: Failure,
    },
}
impl From<DeliveryOutcome> for DeliveryOutcomeV1 {
    fn from(value: DeliveryOutcome) -> Self {
        match value {
            DeliveryOutcome::NotSubmitted { reason } => Self::NotSubmitted { reason },
            DeliveryOutcome::Confirmed { status } => Self::Confirmed {
                confirmation: ConfirmationV1::Http { status },
            },
            DeliveryOutcome::Unknown { status, reason } => Self::Unknown {
                observed: status.map(|status| ConfirmationV1::Http { status }),
                reason,
            },
        }
    }
}
impl DeliveryOutcomeV1 {
    pub fn is_confirmed(&self) -> bool {
        matches!(self, Self::Confirmed { .. })
    }
    pub fn is_not_submitted(&self) -> bool {
        matches!(self, Self::NotSubmitted { .. })
    }
    pub fn queue_state(self) -> State {
        match self {
            Self::Confirmed { .. } => State::Confirmed,
            Self::Unknown { .. } => State::Unknown,
            Self::NotSubmitted { .. } => State::Failed,
        }
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Confirmed {
                confirmation: ConfirmationV1::Http { status },
            } if !(200..300).contains(status) => Err(Error::Invalid),
            Self::Confirmed {
                confirmation: ConfirmationV1::Smtp { code },
            } if *code != 250 => Err(Error::Invalid),
            Self::Unknown {
                observed: Some(ConfirmationV1::Http { status }),
                ..
            } if !(100..600).contains(status) => Err(Error::Invalid),
            Self::Unknown {
                observed: Some(ConfirmationV1::Smtp { code }),
                ..
            } if !(100..600).contains(code) => Err(Error::Invalid),
            _ => Ok(()),
        }
    }
    pub fn validate_claim(&self, claim: &Claim) -> Result<()> {
        self.validate()?;
        let confirmation = match self {
            Self::Confirmed { confirmation } => Some(*confirmation),
            Self::Unknown { observed, .. } => *observed,
            _ => None,
        };
        if confirmation
            .is_some_and(|c| matches!(c, ConfirmationV1::Smtp { .. }) != claim.request.is_smtp())
        {
            return Err(Error::Forbidden);
        }
        claim.request.validate()
    }
}

pub struct Sender {
    https: HttpsSender,
    smtp: EmailSender,
}
impl Sender {
    pub fn new(roots: Vec<CertificateDer<'static>>) -> Result<Self> {
        Ok(Self {
            https: HttpsSender::new(roots.clone())?,
            smtp: EmailSender::new(roots)?,
        })
    }
    pub async fn deliver(
        &self,
        claim: &Claim,
        connector: &mut impl Connector,
        fence: &mut impl Fence,
        cancel: watch::Receiver<bool>,
    ) -> DeliveryOutcomeV1 {
        match &claim.request {
            PreparedDeliveryV1::Https { .. } => self
                .https
                .deliver(claim, connector, fence, cancel)
                .await
                .into(),
            PreparedDeliveryV1::Smtp { request } => {
                let email = match email_claim(claim, request) {
                    Ok(value) => value,
                    Err(_) => {
                        return DeliveryOutcomeV1::NotSubmitted {
                            reason: Failure::InvalidRequest,
                        }
                    }
                };
                let expected = match email.fingerprint() {
                    Ok(hash) => hash,
                    Err(_) => {
                        return DeliveryOutcomeV1::NotSubmitted {
                            reason: Failure::InvalidRequest,
                        }
                    }
                };
                let mut adapter = StageFence {
                    original: claim,
                    inner: fence,
                    expected,
                };
                match self
                    .smtp
                    .deliver(&email, connector, &mut adapter, cancel)
                    .await
                {
                    EmailOutcome::NotSubmitted { reason } => {
                        DeliveryOutcomeV1::NotSubmitted { reason }
                    }
                    EmailOutcome::Confirmed { smtp_code: 250 } => DeliveryOutcomeV1::Confirmed {
                        confirmation: ConfirmationV1::Smtp { code: 250 },
                    },
                    EmailOutcome::Confirmed { .. } => DeliveryOutcomeV1::Unknown {
                        observed: None,
                        reason: Failure::Protocol,
                    },
                    EmailOutcome::Unknown { reason } => DeliveryOutcomeV1::Unknown {
                        observed: None,
                        reason,
                    },
                }
            }
        }
    }
}
fn email_claim(claim: &Claim, request: &EmailRequest) -> Result<EmailClaim> {
    claim.binding.validate()?;
    request.validate_candidate()?;
    let (id, line) = request.identity();
    if id != claim.id || line != claim.line {
        return Err(Error::Invalid);
    }
    // Explicit bounded snapshot of the immutable queue claim. Both the encoded
    // private copy and every decoded credential/MIME field zeroize on drop.
    let raw = Zeroizing::new(serde_json::to_vec(request)?);
    if raw.len() > 65536 {
        return Err(Error::Invalid);
    }
    let owned = serde_json::from_slice(&raw)?;
    EmailClaim::new(
        claim.binding.clone(),
        claim.id.clone(),
        claim.attempt_id.clone(),
        claim.generation,
        owned,
    )
}
struct StageFence<'a, F> {
    original: &'a Claim,
    inner: &'a mut F,
    expected: [u8; 32],
}
impl<F: Fence> EmailFence for StageFence<'_, F> {
    async fn check(&mut self, stage: EmailStage, claim: &EmailClaim) -> Result<()> {
        if claim.fingerprint()? != self.expected {
            return Err(Error::Forbidden);
        }
        let stage = match stage {
            EmailStage::BeforeConnect => Stage::BeforeConnect,
            EmailStage::BeforeTls => Stage::BeforeTls,
            EmailStage::BeforeAuthenticate => Stage::BeforeSmtpAuth,
            EmailStage::BeforeMailFrom => Stage::BeforeSmtpMailFrom,
            EmailStage::BeforeRecipient => Stage::BeforeSmtpRcptTo,
            EmailStage::BeforeData => Stage::BeforeSmtpData,
            EmailStage::BeforeSubmit => Stage::BeforeSubmit,
        };
        self.inner.check(stage, self.original).await
    }
}
