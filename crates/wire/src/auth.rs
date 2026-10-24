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
    ConfigurationIndicator {
        token: String,
        view_id: String,
        active: bool,
    },
    NotificationRead {
        token: String,
        entity: String,
        record_id: String,
    },
    BindLegacy {
        token: String,
        core_id: String,
        public_key: [u8; 32],
        expected_generation: u64,
        scopes: Vec<Scope>,
    },
    RevokeLegacy {
        token: String,
        core_id: String,
    },
    LegacySession {
        core_id: String,
        device_id: String,
        binding_epoch: String,
        timestamp: String,
        nonce: String,
        signature: String,
        device_proof: String,
        body: Vec<u8>,
    },
    LegacyRead {
        token: String,
        entity: rustsimhub_domain::NasEntity,
        after: Option<rustsimhub_domain::InboxCursor>,
        limit: usize,
        record_id: Option<String>,
    },
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
    SessionChallenge {
        token: String,
    },
    RenewSession {
        token: String,
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
    WifiNetworks {
        token: String,
    },
    SetupApStatus {
        token: String,
    },
    SetupApStart {
        token: String,
        idempotency_key: String,
    },
    SetupApCancel {
        token: String,
        operation_id: String,
    },
    NetworkDiagnostics {
        token: String,
    },
    SetNetworkPolicy {
        token: String,
        request_id: String,
        enabled: bool,
        limit_bytes: u64,
        reset_day: u8,
        #[serde(default)]
        acknowledge_meter_gap: bool,
    },
    WifiTrial {
        token: String,
        idempotency_key: String,
    },
    WifiBegin {
        token: String,
        idempotency_key: String,
        ssid: String,
        passphrase: String,
    },
    WifiStatus {
        token: String,
        trial_id: String,
    },
    WifiConfirm {
        token: String,
        trial_id: String,
    },
    WifiCancel {
        token: String,
        trial_id: String,
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
    SmsManagementStatus {
        token: String,
    },
    DeleteSms {
        token: String,
        sms_id: String,
    },
    RestoreSms {
        token: String,
        sms_id: String,
    },
    DeletedInbox {
        token: String,
        after: Option<rustsimhub_domain::InboxCursor>,
        limit: usize,
    },
    DeletedSmsDetail {
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
    CancelPreparedCall {
        token: String,
        operation_id: String,
    },
    CheckMediaPreflight {
        token: String,
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
    CheckMediaLease {
        token: String,
        lease: rustsimhub_domain::CallLease,
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
        AuthCommand::NotificationRead {
            token,
            entity,
            record_id,
        } => {
            if token.len() != 64
                || !matches!(entity.as_str(), "sms" | "calls")
                || record_id.len() != 36
                || !record_id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b'-')
            {
                return Err(invalid());
            }
        }
        AuthCommand::ConfigurationIndicator { token, view_id, .. } => {
            if token.len() != 64
                || view_id.len() != 36
                || !view_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
            {
                return Err(invalid());
            }
        }
        AuthCommand::BindLegacy {
            token,
            core_id,
            expected_generation,
            scopes,
            ..
        } => {
            if token.len() != 64
                || !legacy_id(core_id)
                || *expected_generation == 0
                || *expected_generation > i64::MAX as u64
                || scopes.is_empty()
                || scopes.len() > 3
            {
                return Err(invalid());
            }
        }
        AuthCommand::RevokeLegacy { token, core_id } => {
            if token.len() != 64 || !legacy_id(core_id) {
                return Err(invalid());
            }
        }
        AuthCommand::LegacySession {
            core_id,
            device_id,
            binding_epoch,
            timestamp,
            nonce,
            signature,
            device_proof,
            body,
        } => {
            if !legacy_id(core_id)
                || device_id.len() > 64
                || device_id.is_empty()
                || !source_valid(binding_epoch)
                || timestamp.is_empty()
                || timestamp.len() > 64
                || nonce.is_empty()
                || nonce.len() > 128
                || signature.len() != 88
                || device_proof.len() != 88
                || body.len() > 8192
            {
                return Err(invalid());
            }
        }
        AuthCommand::LegacyRead {
            token,
            entity,
            after,
            limit,
            record_id,
        } => {
            if token.len() != 64
                || !(1..=16).contains(limit)
                || after.as_ref().is_some_and(|c| {
                    c.db_epoch.len() != 36
                        || c.seq.is_empty()
                        || c.seq.len() > 20
                        || !c.seq.bytes().all(|b| b.is_ascii_digit())
                })
                || record_id
                    .as_ref()
                    .is_some_and(|v| v.is_empty() || v.len() > 128)
                || (*entity == rustsimhub_domain::NasEntity::SmsDetail) != record_id.is_some()
            {
                return Err(invalid());
            }
        }
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
        AuthCommand::SetNetworkPolicy {
            token,
            request_id,
            limit_bytes,
            reset_day,
            ..
        } => {
            if token.len() != 64
                || request_id.len() != 36
                || !request_id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b'-')
                || !(1..=100 * 1024 * 1024 * 1024 * 1024u64).contains(limit_bytes)
                || !(1..=31).contains(reset_day)
            {
                return Err(invalid());
            }
        }
        AuthCommand::SetupApStart {
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
        AuthCommand::SetupApCancel {
            token,
            operation_id,
        } => {
            if token.len() != 64
                || operation_id.len() != 36
                || !operation_id
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() || b == b'-')
            {
                return Err(invalid());
            }
        }
        AuthCommand::WifiBegin {
            token,
            idempotency_key,
            ssid,
            passphrase,
        } => {
            if token.len() != 64
                || !wifi_credentials_valid(ssid, passphrase)
                || idempotency_key.is_empty()
                || idempotency_key.len() > 128
                || idempotency_key.chars().any(char::is_control)
            {
                return Err(invalid());
            }
        }
        AuthCommand::WifiStatus { token, trial_id }
        | AuthCommand::WifiConfirm { token, trial_id }
        | AuthCommand::WifiCancel { token, trial_id } => {
            if token.len() != 64 || !source_valid(trial_id) {
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
        AuthCommand::CallStatus { token } | AuthCommand::CheckMediaPreflight { token } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::CallOperation {
            token,
            operation_id,
        }
        | AuthCommand::CancelPreparedCall {
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
        AuthCommand::RenewCall { token, lease }
        | AuthCommand::IssueMedia { token, lease, .. }
        | AuthCommand::CheckMediaLease { token, lease } => {
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
        AuthCommand::RenewSession {
            token,
            challenge_id,
            signature,
        } => {
            let hex = |value: &str| {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            };
            if !hex(token) || !hex(challenge_id) || signature.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::SessionChallenge { token } => {
            if token.len() != 64
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
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
        | AuthCommand::WifiNetworks { token }
        | AuthCommand::SetupApStatus { token }
        | AuthCommand::NetworkDiagnostics { token }
        | AuthCommand::SmsManagementStatus { token }
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
        AuthCommand::Inbox { token, limit, .. }
        | AuthCommand::InboxEvents { token, limit, .. }
        | AuthCommand::DeletedInbox { token, limit, .. } => {
            if token.len() != 64 || !(1..=32).contains(limit) {
                return Err(invalid());
            }
        }
        AuthCommand::SmsDetail { token, sms_id }
        | AuthCommand::DeleteSms { token, sms_id }
        | AuthCommand::RestoreSms { token, sms_id }
        | AuthCommand::DeletedSmsDetail { token, sms_id }
            if token.len() != 64 || sms_id.len() != 36 =>
        {
            return Err(invalid())
        }
        _ => {}
    }
    Ok(req)
}
fn legacy_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn source_valid(source: &str) -> bool {
    source.len() == 64
        && source
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn wifi_credentials_valid(ssid: &str, passphrase: &str) -> bool {
    !ssid.is_empty()
        && ssid.len() <= 32
        && !ssid.chars().any(char::is_control)
        && ((8..=63).contains(&passphrase.len())
            && passphrase.bytes().all(|b| (0x20..=0x7e).contains(&b))
            || passphrase.len() == 64 && passphrase.bytes().all(|b| b.is_ascii_hexdigit()))
}
#[cfg(test)]
mod tests {
    #[test]
    fn setup_ap_commands_are_finite_and_cannot_supply_root_authority_or_settings() {
        for op in ["setup_ap_status", "setup_ap_start", "setup_ap_cancel"] {
            let mut value = serde_json::json!({"protocol":2,"id":"ap","command":{"op":op,"token":"a".repeat(64)}});
            if op == "setup_ap_start" {
                value["command"]["idempotency_key"] = "once".into();
            }
            if op == "setup_ap_cancel" {
                value["command"]["operation_id"] = "00000000-0000-4000-8000-000000000001".into();
            }
            assert!(super::parse_auth_request(&serde_json::to_vec(&value).unwrap()).is_ok());
            for (key, extra) in [
                ("uid", serde_json::json!(0)),
                ("interface", serde_json::json!("other")),
                ("nonce", serde_json::json!("b".repeat(64))),
                ("window_seconds", serde_json::json!(999999)),
                ("passphrase", serde_json::json!("unsafe")),
            ] {
                let mut bad = value.clone();
                bad["command"][key] = extra;
                assert!(super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
            }
        }
    }
    #[test]
    fn wifi_catalogue_cannot_request_scan_select_devices_or_read_secrets() {
        let valid = serde_json::json!({"protocol":2,"id":"wifi","command":{"op":"wifi_networks","token":"a".repeat(64)}});
        assert!(super::parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key, value) in [
            ("token", serde_json::json!("short")),
            ("scan", serde_json::json!(true)),
            ("device", serde_json::json!("/foreign/1")),
            ("secrets", serde_json::json!(true)),
        ] {
            let mut bad = valid.clone();
            bad["command"][key] = value;
            assert!(super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
    #[test]
    fn sms_management_is_bounded_and_cannot_override_owner_or_modem_actions() {
        for op in ["delete_sms", "restore_sms", "deleted_sms_detail"] {
            let valid = serde_json::json!({"protocol":2,"id":"trash","command":{"op":op,"token":"a".repeat(64),"sms_id":"11111111-1111-4111-8111-111111111111"}});
            assert!(super::parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
            for (key, value) in [
                ("token", serde_json::json!("short")),
                ("sms_id", serde_json::json!("../path")),
                ("owner", serde_json::json!(true)),
                ("delete_from_sim", serde_json::json!(true)),
            ] {
                let mut bad = valid.clone();
                bad["command"][key] = value;
                assert!(super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
            }
        }
        for limit in [0, 33] {
            let bad = serde_json::json!({"protocol":2,"id":"trash","command":{"op":"deleted_inbox","token":"a".repeat(64),"after":null,"limit":limit}});
            assert!(super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
    #[test]
    fn indicator_commands_refuse_paths_extra_controls_and_missing_authority() {
        for command in [
            serde_json::json!({"op":"configuration_indicator","token":"a".repeat(64),"view_id":"11111111-1111-4111-8111-111111111111","active":true}),
            serde_json::json!({"op":"notification_read","token":"a".repeat(64),"entity":"sms","record_id":"11111111-1111-4111-8111-111111111111"}),
        ] {
            let valid = serde_json::json!({"protocol":2,"id":"indicator","command":command});
            assert!(super::parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
            for (field, value) in [
                ("path", serde_json::json!("/sys/class/leds/red:")),
                ("token", serde_json::json!("short")),
                ("color", serde_json::json!("red")),
            ] {
                let mut bad = valid.clone();
                bad["command"][field] = value;
                assert!(super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
            }
        }
        let bad = serde_json::json!({"protocol":2,"id":"indicator","command":{"op":"notification_read","token":"a".repeat(64),"entity":"principals","record_id":"11111111-1111-4111-8111-111111111111"}});
        assert!(super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    use super::*;
    #[test]
    fn wifi_provisioning_bounds_credentials_and_rejects_network_overrides() {
        let valid = serde_json::json!({"protocol":2,"id":"wifi","command":{"op":"wifi_begin","token":"a".repeat(64),"idempotency_key":"once","ssid":"synthetic","passphrase":"synthetic-pass"}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("ssid", serde_json::json!("中".repeat(11))),
            ("ssid", serde_json::json!("nul\u{0}ssid")),
            ("passphrase", serde_json::json!("short")),
            ("passphrase", serde_json::json!("x".repeat(64))),
            ("type", serde_json::json!("gsm")),
            ("mode", serde_json::json!("ap")),
            ("interface", serde_json::json!("wwan0")),
            ("timeout", serde_json::json!(0)),
        ] {
            let mut request = valid.clone();
            request["command"][field] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&request).unwrap()).is_err());
        }
        assert!(wifi_credentials_valid(
            "中".repeat(10).as_str(),
            &"A".repeat(64)
        ));
    }
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
    #[test]
    fn continuation_requires_bound_bearer_nonce_and_exact_signature_shape() {
        let valid = serde_json::json!({"protocol":2,"id":"renew","command":{"op":"renew_session","token":"a".repeat(64),"challenge_id":"b".repeat(64),"signature":vec![0;64]}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key, value) in [
            ("token", serde_json::json!("x".repeat(64))),
            ("challenge_id", serde_json::json!("x".repeat(64))),
            ("signature", serde_json::json!(vec![0; 63])),
            ("authenticated", serde_json::json!(0)),
        ] {
            let mut v = valid.clone();
            v["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&v).unwrap()).is_err());
        }
    }
    #[test]
    fn network_settings_reject_paths_invalid_limits_and_missing_authority() {
        let valid = serde_json::json!({"protocol":2,"id":"network","command":{"op":"set_network_policy","token":"a".repeat(64),"request_id":"11111111-1111-4111-8111-111111111111","enabled":true,"limit_bytes":1000000,"reset_day":31}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key, value) in [
            ("token", serde_json::json!("")),
            ("request_id", serde_json::json!("../policy")),
            ("limit_bytes", serde_json::json!(0)),
            ("reset_day", serde_json::json!(32)),
            ("path", serde_json::json!("/etc/passwd")),
            ("operator", serde_json::json!("injected")),
        ] {
            let mut v = valid.clone();
            v["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        assert!(parse_auth_request(
            br#"{"protocol":2,"id":"n","command":{"op":"network_diagnostics"}}"#
        )
        .is_err());
    }
}
