// SPDX-License-Identifier: AGPL-3.0-only
//! Executable failure samples for the A-29 proposal, NOT an SMTP sender.
//! Exercises the current public production interfaces without network access.
#![cfg(target_os = "linux")]

use rustsimhub_domain::LineEpoch;
use rustsimhub_forwarder::{
    worker_ipc, Acknowledgement, Claim, Connector, DeliveryOutcome, Destination, Error, Failure,
    Fence, HttpsSender, PolicyEdit, PreparedRequest, Secret, Stage, FORWARDING_WORKER_PROTOCOL,
    OUTBOX_CURRENT_FORMAT,
};
use serde_json::json;
use std::{cell::Cell, rc::Rc};
use tokio::{net::TcpStream, sync::watch};

const ID: &str = "00000000-0000-4000-8000-000000000001";
const ATTEMPT: &str = "00000000-0000-4000-8000-000000000002";

fn proposed_destination() -> serde_json::Value {
    json!({"kind":"email","smtp_host":"mail.example.invalid","port":465,
        "tls":"implicit","username":"SYNTHETIC_USER","password":"SYNTHETIC_PASSWORD",
        "mail_from":"sender@example.invalid","rcpt_to":"recipient@example.invalid"})
}

fn claim(url: &str) -> Claim {
    Claim {
        id: ID.into(),
        attempt_id: ATTEMPT.into(),
        generation: 1,
        line: LineEpoch(1),
        request: PreparedRequest {
            url: Secret::new(url.into()),
            bearer: None,
            body: Secret::new("{\"notice\":\"synthetic\"}".into()),
            acknowledgement: Acknowledgement::Http2xx,
        },
    }
}

#[test]
fn smtp_configuration_is_rejected_by_both_existing_private_and_owner_edit_types() {
    let destination = proposed_destination();
    assert!(serde_json::from_value::<Destination>(destination.clone()).is_err());
    let edit = json!({"rules_revision":null,"enabled":false,"rules":[],
        "targets":[{"id":ID,"destination":destination}]});
    assert!(serde_json::from_value::<PolicyEdit>(edit).is_err());

    // Adding SMTP fields to a valid existing target is rejected as well;
    // kind=webhook cannot be used as a way to carry a new private schema.
    let disguised = json!({"kind":"webhook","url":"https://example.invalid/",
        "bearer":null,"smtp_host":"mail.example.invalid"});
    assert!(serde_json::from_value::<Destination>(disguised).is_err());
}

#[test]
fn proposed_transport_payload_cannot_be_smuggled_into_a_current_claim() {
    let original = claim("https://example.invalid/synthetic");
    let digest = worker_ipc::claim_sha256(&original).unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value["request"]["transport"] = "smtp".into();
    assert!(serde_json::from_value::<Claim>(value.clone()).is_err());
    value["request"] = json!({"transport":"smtp","email":proposed_destination()});
    assert!(serde_json::from_value::<Claim>(value).is_err());

    // The currently supported immutable claim is credential/payload bound.
    // A future tagged SMTP variant must preserve this property, not copy a
    // digest from an earlier HTTPS claim or omit private mail recipients.
    let mut changed = claim("https://example.invalid/synthetic");
    changed.request.bearer = Some(Secret::new("SYNTHETIC_CHANGED_CREDENTIAL".into()));
    assert_ne!(digest, worker_ipc::claim_sha256(&changed).unwrap());
    changed = claim("https://example.invalid/synthetic");
    changed.request.body = Secret::new("{\"notice\":\"different synthetic payload\"}".into());
    assert_ne!(digest, worker_ipc::claim_sha256(&changed).unwrap());
}

struct CountingConnector(Rc<Cell<usize>>);
impl Connector for CountingConnector {
    async fn connect(&mut self, _: &str, _: u16) -> rustsimhub_forwarder::Result<TcpStream> {
        self.0.set(self.0.get() + 1);
        Err(Error::Forbidden)
    }
}
struct CountingFence(Rc<Cell<usize>>);
impl Fence for CountingFence {
    async fn check(&mut self, _: Stage, _: &Claim) -> rustsimhub_forwarder::Result<()> {
        self.0.set(self.0.get() + 1);
        Err(Error::Forbidden)
    }
}

#[tokio::test]
async fn actual_https_sender_rejects_smtp_before_any_authority_or_connect_callback() {
    let certificate = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let sender = HttpsSender::new(vec![certificate.cert.der().clone()]).unwrap();
    let connects = Rc::new(Cell::new(0));
    let checks = Rc::new(Cell::new(0));
    let mut connector = CountingConnector(connects.clone());
    let mut fence = CountingFence(checks.clone());
    let (_cancel, receiver) = watch::channel(false);
    for url in [
        "smtp://mail.example.invalid:25",
        "smtps://mail.example.invalid:465",
        "https://mail.example.invalid:465\r\nMAIL FROM:<sender@example.invalid>",
    ] {
        assert_eq!(
            sender
                .deliver(&claim(url), &mut connector, &mut fence, receiver.clone())
                .await,
            DeliveryOutcome::NotSubmitted {
                reason: Failure::InvalidRequest
            }
        );
    }
    assert_eq!(connects.get(), 0);
    assert_eq!(checks.get(), 0);
}

#[test]
fn worker_v4_has_no_smtp_receipt_domain_and_refuses_proposed_v5() {
    assert_eq!(FORWARDING_WORKER_PROTOCOL, 4);
    assert_eq!(OUTBOX_CURRENT_FORMAT, 3);
    let mut request = json!({"protocol":4,"id":ID,"command":{"op":"finish",
        "lease":"a".repeat(64),"attempt_id":ATTEMPT,
        "outcome":{"kind":"confirmed","status":250}}});
    // 250 is in the current HTTP 2xx range. This is NOT proof of an SMTP
    // exchange, a DATA acknowledgement, or delivery to a recipient device.
    assert!(worker_ipc::parse(&serde_json::to_vec(&request).unwrap()).is_ok());
    request["command"]["outcome"]["transport"] = "smtp".into();
    assert!(worker_ipc::parse(&serde_json::to_vec(&request).unwrap()).is_err());
    request["command"]["outcome"] = json!({"kind":"smtp_accepted","code":250});
    assert!(worker_ipc::parse(&serde_json::to_vec(&request).unwrap()).is_err());
    request["command"]["outcome"] = json!({"kind":"confirmed","status":250});
    request["protocol"] = 5.into();
    assert!(worker_ipc::parse(&serde_json::to_vec(&request).unwrap()).is_err());
}
