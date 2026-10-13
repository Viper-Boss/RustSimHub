// SPDX-License-Identifier: AGPL-3.0-only
use crate::MAX_FRAME;
use rustsimhub_policy::{Kind, Scope};
use serde::{Deserialize, Serialize};
pub const AUTH_PROTOCOL: u16 = 2;
// Deliberately no Debug: requests may contain session tokens.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthRequest {
    pub protocol: u16,
    pub id: String,
    pub command: AuthCommand,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthCommand {
    Identity {},
    Challenge {
        public_key: [u8; 32],
    },
    Authenticate {
        challenge_id: String,
        signature: Vec<u8>,
    },
    Health {
        token: String,
    },
    Capabilities {
        token: String,
    },
    LineStatus {
        token: String,
    },
    PrepareSms {
        token: String,
        line_epoch: u64,
        idempotency_key: String,
        recipient: String,
        text: String,
    },
    SubmitSms {
        token: String,
        operation_id: String,
    },
    SmsOperation {
        token: String,
        operation_id: String,
    },
    Inbox {
        token: String,
        after: Option<rustsimhub_domain::InboxCursor>,
        limit: usize,
    },
    SmsDetail {
        token: String,
        sms_id: String,
    },
    InboxEvents {
        token: String,
        after: Option<rustsimhub_domain::InboxCursor>,
        limit: usize,
    },
    OpenPairing {
        token: String,
    },
    PendingPairings {
        token: String,
    },
    Clients {
        token: String,
    },
    PairChallenge {
        public_key: [u8; 32],
        kind: Kind,
        scopes: Vec<Scope>,
    },
    PairProof {
        challenge_id: String,
        signature: Vec<u8>,
    },
    Approve {
        token: String,
        pairing_id: String,
        public_key: [u8; 32],
        kind: Kind,
        scopes: Vec<Scope>,
    },
    Revoke {
        token: String,
        public_key: [u8; 32],
    },
}
pub fn parse_auth_request(body: &[u8]) -> std::io::Result<AuthRequest> {
    let invalid = || {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid authenticated request",
        )
    };
    if body.is_empty() || body.len() > MAX_FRAME {
        return Err(invalid());
    }
    let req: AuthRequest = serde_json::from_slice(body).map_err(|_| invalid())?;
    if req.protocol != AUTH_PROTOCOL || req.id.is_empty() || req.id.len() > 128 {
        return Err(invalid());
    }
    match &req.command {
        AuthCommand::PrepareSms {
            token,
            line_epoch,
            idempotency_key,
            recipient,
            text,
        } => {
            if token.len() != 64
                || *line_epoch == 0
                || *line_epoch > i64::MAX as u64
                || idempotency_key.is_empty()
                || idempotency_key.len() > 128
                || !(3..=32).contains(&recipient.len())
                || text.is_empty()
                || text.len() > 4096
                || text.contains('\0')
            {
                return Err(invalid());
            }
        }
        AuthCommand::SubmitSms {
            token,
            operation_id,
        }
        | AuthCommand::SmsOperation {
            token,
            operation_id,
        } => {
            if token.len() != 64 || operation_id.len() != 36 {
                return Err(invalid());
            }
        }
        AuthCommand::Authenticate {
            challenge_id,
            signature,
        }
        | AuthCommand::PairProof {
            challenge_id,
            signature,
        } => {
            if challenge_id.len() != 64 || signature.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::Health { token }
        | AuthCommand::Capabilities { token }
        | AuthCommand::LineStatus { token }
        | AuthCommand::OpenPairing { token }
        | AuthCommand::PendingPairings { token }
        | AuthCommand::Clients { token }
        | AuthCommand::Revoke { token, .. } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::Approve {
            token,
            pairing_id,
            scopes,
            ..
        } => {
            if token.len() != 64 || pairing_id.len() != 64 || scopes.len() > 10 {
                return Err(invalid());
            }
        }
        AuthCommand::PairChallenge { scopes, .. } if scopes.len() > 10 => return Err(invalid()),
        AuthCommand::Inbox { token, limit, .. } | AuthCommand::InboxEvents { token, limit, .. } => {
            if token.len() != 64 || !(1..=32).contains(limit) {
                return Err(invalid());
            }
        }
        AuthCommand::SmsDetail { token, sms_id } if token.len() != 64 || sms_id.len() != 36 => {
            return Err(invalid())
        }
        _ => {}
    }
    Ok(req)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outgoing_requests_reject_identity_injection_and_invalid_private_payloads() {
        let command = serde_json::json!({"op":"prepare_sms","token":"a".repeat(64),"line_epoch":2,"idempotency_key":"a","recipient":"+12025550123","text":"synthetic"});
        let mut request = serde_json::json!({"protocol":2,"id":"a","command":command});
        assert!(parse_auth_request(&serde_json::to_vec(&request).unwrap()).is_ok());
        request["command"]["client_id"] = "injected".into();
        assert!(parse_auth_request(&serde_json::to_vec(&request).unwrap()).is_err());
        request["command"]
            .as_object_mut()
            .unwrap()
            .remove("client_id");
        for text in [String::new(), "x".repeat(4097), "nul\0text".to_owned()] {
            request["command"]["text"] = text.into();
            assert!(parse_auth_request(&serde_json::to_vec(&request).unwrap()).is_err());
        }
    }
    #[test]
    fn roles_unknown_scopes_bootstrap_and_missing_tokens_fail_closed() {
        for value in [
            r#"{"protocol":2,"id":"a","command":{"op":"identity","role":"owner"}}"#,
            r#"{"protocol":2,"id":"a","command":{"op":"health"}}"#,
            r#"{"protocol":2,"id":"a","command":{"op":"bootstrap_owner"}}"#,
            r#"{"protocol":1,"id":"a","command":{"op":"identity"}}"#,
        ] {
            assert!(parse_auth_request(value.as_bytes()).is_err());
        }
        let key = vec![0; 32];
        let invalid = serde_json::json!({"protocol":2,"id":"a","command":{"op":"pair_challenge","public_key":key,"kind":"nas","scopes":["everything"]}});
        assert!(parse_auth_request(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}
