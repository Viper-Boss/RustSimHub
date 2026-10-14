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
    SetupChallenge {
        setup_code: String,
        public_key: [u8; 32],
    },
    SetupFinish {
        setup_code: String,
        challenge_id: String,
        signature: Vec<u8>,
    },
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
    NetworkStatus {
        token: String,
    },
    WifiTrial {
        token: String,
        idempotency_key: String,
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
    HistorySources {
        token: String,
    },
    HistoryPage {
        token: String,
        source: String,
        entity: rustsimhub_domain::HistoryEntity,
        after: Option<rustsimhub_domain::HistoryCursor>,
        limit: usize,
    },
    HistoryDetail {
        token: String,
        source: String,
        entity: rustsimhub_domain::HistoryEntity,
        record_id: String,
    },
    GrantHistory {
        token: String,
        public_key: [u8; 32],
        expected_generation: u64,
        source: String,
        legacy_user: Option<String>,
        entities: Vec<rustsimhub_domain::HistoryEntity>,
    },
    RevokeHistory {
        token: String,
        public_key: [u8; 32],
        source: String,
    },
    Calls {
        token: String,
        after: Option<rustsimhub_domain::InboxCursor>,
        limit: usize,
    },
    CallDetail {
        token: String,
        call_id: String,
    },
    CallEvents {
        token: String,
        after: Option<rustsimhub_domain::InboxCursor>,
        limit: usize,
    },
    CallStatus {
        token: String,
    },
    PrepareCall {
        token: String,
        line_epoch: u64,
        idempotency_key: String,
        action: rustsimhub_domain::CallAction,
        call_id: Option<String>,
        payload: String,
    },
    SubmitCall {
        token: String,
        operation_id: String,
        lease: Option<rustsimhub_domain::CallLease>,
        urgent: bool,
    },
    CallOperation {
        token: String,
        operation_id: String,
    },
    RenewCall {
        token: String,
        lease: rustsimhub_domain::CallLease,
    },
    IssueMedia {
        token: String,
        lease: rustsimhub_domain::CallLease,
        peer_key: [u8; 32],
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
        AuthCommand::HistorySources { token } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::HistoryPage {
            token,
            source,
            limit,
            after,
            ..
        } => {
            if token.len() != 64
                || !source_valid(source)
                || !(1..=16).contains(limit)
                || after.as_ref().is_some_and(|c| {
                    c.archive_epoch.len() != 64
                        || c.audience.len() != 64
                        || !source_valid(&c.source)
                        || c.after.is_empty()
                        || c.after.len() > 19
                        || !c.after.bytes().all(|b| b.is_ascii_digit())
                })
            {
                return Err(invalid());
            }
        }
        AuthCommand::HistoryDetail {
            token,
            source,
            record_id,
            ..
        } => {
            if token.len() != 64
                || !source_valid(source)
                || record_id.is_empty()
                || record_id.len() > 19
                || !record_id.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(invalid());
            }
        }
        AuthCommand::GrantHistory {
            token,
            source,
            expected_generation,
            legacy_user,
            entities,
            ..
        } => {
            if token.len() != 64
                || !source_valid(source)
                || *expected_generation == 0
                || *expected_generation > i64::MAX as u64
                || entities.is_empty()
                || entities.len() > 3
                || legacy_user.as_ref().is_some_and(|u| {
                    u.is_empty() || u.len() > 128 || u.chars().any(char::is_control)
                })
            {
                return Err(invalid());
            }
        }
        AuthCommand::RevokeHistory { token, source, .. } => {
            if token.len() != 64 || !source_valid(source) {
                return Err(invalid());
            }
        }
        AuthCommand::SetupChallenge { setup_code, .. }
        | AuthCommand::SetupFinish { setup_code, .. } => {
            if setup_code.len() != 64
                || !setup_code
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid());
            }
            let invalid_finish = matches!(&req.command,AuthCommand::SetupFinish {challenge_id,signature,..} if challenge_id.len()!=64 || !challenge_id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) || signature.len()!=64);
            if invalid_finish {
                return Err(invalid());
            }
        }
        AuthCommand::WifiTrial {
            token,
            idempotency_key,
        } => {
            if token.len() != 64
                || idempotency_key.is_empty()
                || idempotency_key.len() > 128
                || idempotency_key.chars().any(char::is_control)
            {
                return Err(invalid());
            }
        }
        AuthCommand::Calls { token, limit, .. } | AuthCommand::CallEvents { token, limit, .. } => {
            if token.len() != 64 || !(1..=32).contains(limit) {
                return Err(invalid());
            }
        }
        AuthCommand::CallDetail { token, call_id } => {
            if token.len() != 64 || call_id.len() != 36 {
                return Err(invalid());
            }
        }
        AuthCommand::CallStatus { token } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::CallOperation {
            token,
            operation_id,
        }
        | AuthCommand::SubmitCall {
            token,
            operation_id,
            ..
        } => {
            if token.len() != 64 || operation_id.len() != 36 {
                return Err(invalid());
            }
        }
        AuthCommand::RenewCall { token, lease } | AuthCommand::IssueMedia { token, lease, .. } => {
            if token.len() != 64
                || lease.call_id.len() != 36
                || lease.boot.len() != 64
                || lease.line.0 == 0
                || lease.generation == 0
                || lease.expires_in_seconds > 30
            {
                return Err(invalid());
            }
        }
        AuthCommand::PrepareCall {
            token,
            line_epoch,
            idempotency_key,
            call_id,
            payload,
            ..
        } => {
            if token.len() != 64
                || *line_epoch == 0
                || *line_epoch > i64::MAX as u64
                || idempotency_key.is_empty()
                || idempotency_key.len() > 128
                || call_id.as_ref().is_some_and(|c| c.len() != 36)
                || payload.len() > 32
            {
                return Err(invalid());
            }
        }

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
        | AuthCommand::NetworkStatus { token }
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
fn source_valid(source: &str) -> bool {
    source.len() == 64
        && source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_commands_reject_remote_identity_overrides_and_unbounded_queries() {
        let valid = serde_json::json!({"protocol":2,"id":"history","command":{"op":"history_page","token":"a".repeat(64),"source":"b".repeat(64),"entity":"sms","after":null,"limit":16}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("limit", serde_json::json!(17)),
            ("source", serde_json::json!("../private")),
            ("entity", serde_json::json!("principals")),
            ("legacy_user", serde_json::json!("injected")),
            ("public_key", serde_json::json!(vec![0; 32])),
        ] {
            let mut v = valid.clone();
            v["command"][field] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let grant = serde_json::json!({"protocol":2,"id":"grant","command":{"op":"grant_history","token":"a".repeat(64),"public_key":vec![7;32],"expected_generation":1,"source":"b".repeat(64),"legacy_user":"legacy_user_a","entities":["sms"]}});
        assert!(parse_auth_request(&serde_json::to_vec(&grant).unwrap()).is_ok());
        for (field, value) in [
            ("expected_generation", serde_json::json!(0)),
            ("legacy_user", serde_json::json!("nul\u{0}user")),
            ("kind", serde_json::json!("owner")),
            ("entities", serde_json::json!([])),
        ] {
            let mut v = grant.clone();
            v["command"][field] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&v).unwrap()).is_err());
        }
    }
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
