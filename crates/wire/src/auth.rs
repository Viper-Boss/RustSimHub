// SPDX-License-Identifier: AGPL-3.0-only
use crate::MAX_FRAME;
use rustsimhub_policy::{Kind, Scope};
use serde::{Deserialize, Serialize};
pub const AUTH_PROTOCOL: u16 = 2;
#[derive(Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum LegacyBindingExpectation {
    Absent {},
    Present { binding_epoch: String },
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NknOwner {
    ManualEngine,
    Remote,
    Statistics,
}
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
    UsageParticipationStatus {
        token: String,
    },
    UsageParticipationSet {
        token: String,
        enabled: bool,
        disclosure_version: Option<u16>,
        expected_generation: String,
        expected_core_instance: String,
        request_id: String,
    },
    UsageParticipationRequest {
        token: String,
        request_id: String,
    },
    UsageStatsQuery {
        token: String,
        window: String,
    },
    NknStatus {
        token: String,
    },
    NknRequest {
        token: String,
        request_id: String,
        expected_core_instance: String,
    },
    SetNknEnabled {
        token: String,
        owner: NknOwner,
        enabled: bool,
        expected_revision: String,
        expected_core_instance: String,
        request_id: String,
    },
    RecoveryGrant {
        token: String,
        public_key: [u8; 32],
        max_trial_ms: u32,
        expected_generation: String,
        request_id: String,
    },
    RecoveryRevoke {
        token: String,
        public_key: [u8; 32],
        expected_generation: String,
        request_id: String,
    },
    RecoveryKey {
        token: String,
        public_key: [u8; 32],
    },
    RecoveryRequest {
        token: String,
        request_id: String,
    },
    Identity {},
    ForwardingStatus {
        token: String,
    },
    SetForwardingEnabled {
        token: String,
        expected_generation: u64,
        request_id: String,
        enabled: bool,
    },
    ForwardingConfigure {
        token: String,
        expected_generation: u64,
        request_id: String,
        // Bounded by MAX_FRAME here, then decoded and validated by Core.
        // No Debug implementation: this object can contain target credentials.
        policy: serde_json::Value,
    },
    ForwardingRules {
        token: String,
    },
    ForwardingRequest {
        token: String,
        request_id: String,
    },
    ForwardingTest {
        token: String,
        expected_generation: u64,
        request_id: String,
        target_id: String,
    },
    ForwardingQuickTest {
        token: String,
        expected_generation: u64,
        request_id: String,
        target_id: String,
    },
    ForwardingTestRequest {
        token: String,
        request_id: String,
    },
    ForwardingReceipts {
        token: String,
        after: String,
        limit: usize,
    },
    ForwardingPrune {
        token: String,
        generation: u64,
        through: String,
    },
    ForwardingHistoryPrepare {
        token: String,
        expected_generation: u64,
        request_id: String,
        range: serde_json::Value,
    },
    ForwardingHistoryCommit {
        token: String,
        expected_generation: u64,
        request_id: String,
        confirmation_id: String,
        accept_full_text: bool,
    },
    ForwardingHistoryCancel {
        token: String,
        request_id: String,
        confirmation_id: String,
    },
    ForwardingHistoryArchive {
        token: String,
        expected_generation: u64,
        request_id: String,
        confirmation_id: String,
    },
    ForwardingHistoryRequest {
        token: String,
        request_id: String,
    },
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
    BindLegacyChecked {
        token: String,
        core_id: String,
        public_key: [u8; 32],
        expected_generation: u64,
        scopes: Vec<Scope>,
        expected_binding: LegacyBindingExpectation,
    },
    RevokeLegacyChecked {
        token: String,
        core_id: String,
        expected_binding_epoch: String,
    },
    RevokeLegacy {
        token: String,
        core_id: String,
    },
    /// Read back an existing alias after an unconfirmed management request.
    /// This does not create a NAS principal, mapping or session.
    LegacyBindingStatus {
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
    /// First Owner claim without the setup code; Core allows it only while the
    /// first-setup AP is Ready (the claimant is on the device's own hotspot).
    SetupOpenChallenge {
        public_key: [u8; 32],
    },
    SetupOpenFinish {
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
    SystemStatus {
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
    SetupApOperationStatus {
        token: String,
        request_key_sha256: String,
    },
    SetupApStart {
        token: String,
        idempotency_key: String,
    },
    SetupApCancel {
        token: String,
        operation_id: String,
    },
    SetupApWifiBegin {
        token: String,
        operation_id: String,
        trial_id: String,
        ssid: String,
        passphrase: String,
    },
    SetupApScanBegin {
        token: String,
        operation_id: String,
        scan_id: String,
    },
    SetupApScanStatus {
        token: String,
        operation_id: String,
        scan_id: String,
    },
    SetupApScanCancel {
        token: String,
        operation_id: String,
        scan_id: String,
    },
    SetupApWifiStatus {
        token: String,
        operation_id: String,
        trial_id: String,
    },
    SetupApWifiCancel {
        token: String,
        operation_id: String,
        trial_id: String,
    },
    SetupApWifiConfirm {
        token: String,
        operation_id: String,
        trial_id: String,
        commit_id: String,
        ssid: String,
        passphrase: String,
    },
    SetupApWifiCommitStatus {
        token: String,
        operation_id: String,
        commit_id: String,
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
    InboxRecent {
        token: String,
        before: Option<rustsimhub_domain::InboxCursor>,
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
    CallsRecent {
        token: String,
        before: Option<rustsimhub_domain::InboxCursor>,
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
    /// AirSIM gateway management (owner).
    AirsimStatus {
        token: String,
    },
    AirsimConfigure {
        token: String,
        enabled: bool,
        pcm_rate: u32,
    },
    AirsimOpenPairing {
        token: String,
    },
    AirsimClosePairing {
        token: String,
    },
    AirsimRevoke {
        token: String,
        pairing_id: String,
    },
    /// A plain request received by Edge on the AirSIM pairing port. It carries
    /// no bearer: the app proves itself with the pairing code.
    AirsimPairing {
        method: String,
        target: String,
        body: String,
        peer: String,
    },
    /// A request received on the AirSIM control port; Core verifies the
    /// app's HMAC headers against its stored pairings.
    AirsimControl {
        method: String,
        target: String,
        version: Option<String>,
        timestamp: Option<String>,
        nonce: Option<String>,
        signature: Option<String>,
        body: String,
        peer: String,
    },
    /// Continues a held AirSIM long poll.
    AirsimPoll {
        ticket: String,
    },
    /// Edge-only: Core authenticates the app's signed PCM preface.
    AirsimPcmOpen {
        preface: String,
        peer: String,
    },
    /// Edge-only liveness/cleanup, using an opaque, non-persistent capability.
    AirsimPcmPulse {
        ticket: String,
        close: bool,
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
/// Largest AirSIM request body Edge may forward; the frame limit is 64 KiB.
pub const AIRSIM_MAX_BODY: usize = 16 * 1024;
fn airsim_request_valid(method: &str, target: &str, body: &str, peer: &str) -> bool {
    matches!(method, "GET" | "POST")
        && target.starts_with('/')
        && target.len() <= 512
        && target.bytes().all(|b| b.is_ascii_graphic())
        && body.len() <= AIRSIM_MAX_BODY
        && peer.parse::<std::net::IpAddr>().is_ok()
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
        AuthCommand::UsageParticipationStatus { token } if !source_valid(token) => {
            return Err(invalid())
        }
        AuthCommand::UsageParticipationSet {
            token,
            enabled,
            disclosure_version,
            expected_generation,
            expected_core_instance,
            request_id,
        } => {
            if !source_valid(token)
                || !canonical_generation(expected_generation)
                || !source_valid(expected_core_instance)
                || !canonical_uuid(request_id)
                || (*enabled && *disclosure_version != Some(1))
                || (!*enabled && disclosure_version.is_some())
            {
                return Err(invalid());
            }
        }
        AuthCommand::UsageParticipationRequest { token, request_id } => {
            if !source_valid(token) || !canonical_uuid(request_id) {
                return Err(invalid());
            }
        }
        AuthCommand::UsageStatsQuery { token, window } => {
            if !source_valid(token) || !matches!(window.as_str(), "24h" | "30d" | "90d") {
                return Err(invalid());
            }
        }
        AuthCommand::NknStatus { token } if !source_valid(token) => return Err(invalid()),
        AuthCommand::NknRequest {
            token,
            request_id,
            expected_core_instance,
        } => {
            if !source_valid(token)
                || !canonical_uuid(request_id)
                || !source_valid(expected_core_instance)
            {
                return Err(invalid());
            }
        }
        AuthCommand::SetNknEnabled {
            token,
            expected_revision,
            expected_core_instance,
            request_id,
            ..
        } => {
            if !source_valid(token)
                || !canonical_generation(expected_revision)
                || !source_valid(expected_core_instance)
                || !canonical_uuid(request_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::RecoveryGrant {
            token,
            max_trial_ms,
            expected_generation,
            request_id,
            ..
        } => {
            if token.len() != 64
                || !(1..=40000).contains(max_trial_ms)
                || !canonical_generation(expected_generation)
                || !canonical_uuid(request_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::RecoveryRevoke {
            token,
            expected_generation,
            request_id,
            ..
        } => {
            if token.len() != 64
                || !canonical_generation(expected_generation)
                || !canonical_uuid(request_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::RecoveryKey { token, .. } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::RecoveryRequest { token, request_id } => {
            if token.len() != 64 || !canonical_uuid(request_id) {
                return Err(invalid());
            }
        }
        AuthCommand::SetForwardingEnabled {
            token,
            expected_generation,
            request_id,
            ..
        } => {
            if token.len() != 64
                || *expected_generation >= i64::MAX as u64
                || !canonical_uuid(request_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingStatus { token } | AuthCommand::ForwardingRules { token } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingRequest { token, request_id }
        | AuthCommand::ForwardingHistoryRequest { token, request_id }
        | AuthCommand::ForwardingTestRequest { token, request_id } => {
            if token.len() != 64 || !canonical_uuid(request_id) {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingTest {
            token,
            expected_generation,
            request_id,
            target_id,
        }
        | AuthCommand::ForwardingQuickTest {
            token,
            expected_generation,
            request_id,
            target_id,
        } => {
            if token.len() != 64
                || *expected_generation == 0
                || *expected_generation > i64::MAX as u64
                || !canonical_uuid(request_id)
                || !canonical_uuid(target_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingConfigure {
            token,
            expected_generation,
            request_id,
            policy,
        } => {
            if token.len() != 64
                || *expected_generation >= i64::MAX as u64
                || !canonical_uuid(request_id)
                || !policy.is_object()
            {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingHistoryPrepare {
            token,
            expected_generation,
            request_id,
            range,
        } => {
            if token.len() != 64
                || *expected_generation == 0
                || *expected_generation > i64::MAX as u64
                || !canonical_uuid(request_id)
                || !range.is_object()
            {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingHistoryCommit {
            token,
            expected_generation,
            request_id,
            confirmation_id,
            ..
        }
        | AuthCommand::ForwardingHistoryArchive {
            token,
            expected_generation,
            request_id,
            confirmation_id,
        } => {
            if token.len() != 64
                || *expected_generation == 0
                || *expected_generation > i64::MAX as u64
                || !canonical_uuid(request_id)
                || !canonical_uuid(confirmation_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingHistoryCancel {
            token,
            request_id,
            confirmation_id,
        } => {
            if token.len() != 64 || !canonical_uuid(request_id) || !canonical_uuid(confirmation_id)
            {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingReceipts {
            token,
            after,
            limit,
        } => {
            if token.len() != 64 || !decimal_sequence(after) || !(1..=32).contains(limit) {
                return Err(invalid());
            }
        }
        AuthCommand::ForwardingPrune {
            token,
            generation,
            through,
        } => {
            if token.len() != 64
                || *generation == 0
                || *generation > i64::MAX as u64
                || !decimal_sequence(through)
            {
                return Err(invalid());
            }
        }
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
        }
        | AuthCommand::BindLegacyChecked {
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
            if let AuthCommand::BindLegacyChecked {
                expected_binding: LegacyBindingExpectation::Present { binding_epoch },
                ..
            } = &req.command
            {
                if !source_valid(binding_epoch) {
                    return Err(invalid());
                }
            }
        }
        AuthCommand::RevokeLegacyChecked {
            token,
            core_id,
            expected_binding_epoch,
        } => {
            if token.len() != 64 || !legacy_id(core_id) || !source_valid(expected_binding_epoch) {
                return Err(invalid());
            }
        }
        AuthCommand::RevokeLegacy { token, core_id }
        | AuthCommand::LegacyBindingStatus { token, core_id } => {
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
        AuthCommand::SetupOpenChallenge { .. } => {}
        AuthCommand::SetupOpenFinish {
            challenge_id,
            signature,
        } => {
            if challenge_id.len() != 64
                || !challenge_id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || signature.len() != 64
            {
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
        AuthCommand::SetupApOperationStatus {
            token,
            request_key_sha256,
        } => {
            if token.len() != 64
                || request_key_sha256.len() != 64
                || !request_key_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
        AuthCommand::SetupApWifiConfirm {
            token,
            operation_id,
            trial_id,
            commit_id,
            ssid,
            passphrase,
        } => {
            if token.len() != 64
                || !canonical_uuid(operation_id)
                || !canonical_uuid(trial_id)
                || !canonical_uuid(commit_id)
                || !crate::wifi_credentials_valid(ssid, passphrase)
            {
                return Err(invalid());
            }
        }
        AuthCommand::SetupApWifiCommitStatus {
            token,
            operation_id,
            commit_id,
        } => {
            if token.len() != 64 || !canonical_uuid(operation_id) || !canonical_uuid(commit_id) {
                return Err(invalid());
            }
        }
        AuthCommand::SetupApWifiBegin {
            token,
            operation_id,
            trial_id,
            ssid,
            passphrase,
        } => {
            if token.len() != 64
                || !canonical_uuid(operation_id)
                || !canonical_uuid(trial_id)
                || !crate::wifi_credentials_valid(ssid, passphrase)
            {
                return Err(invalid());
            }
        }
        AuthCommand::SetupApScanBegin {
            token,
            operation_id,
            scan_id,
        }
        | AuthCommand::SetupApScanStatus {
            token,
            operation_id,
            scan_id,
        }
        | AuthCommand::SetupApScanCancel {
            token,
            operation_id,
            scan_id,
        } => {
            if token.len() != 64 || !canonical_uuid(operation_id) || !canonical_uuid(scan_id) {
                return Err(invalid());
            }
        }
        AuthCommand::SetupApWifiStatus {
            token,
            operation_id,
            trial_id,
        }
        | AuthCommand::SetupApWifiCancel {
            token,
            operation_id,
            trial_id,
        } => {
            if token.len() != 64 || !canonical_uuid(operation_id) || !canonical_uuid(trial_id) {
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
        AuthCommand::Calls { token, limit, .. }
        | AuthCommand::CallsRecent { token, limit, .. }
        | AuthCommand::CallEvents { token, limit, .. } => {
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
        | AuthCommand::SystemStatus { token }
        | AuthCommand::NetworkStatus { token }
        | AuthCommand::WifiNetworks { token }
        | AuthCommand::SetupApStatus { token }
        | AuthCommand::NetworkDiagnostics { token }
        | AuthCommand::SmsManagementStatus { token }
        | AuthCommand::OpenPairing { token }
        | AuthCommand::AirsimStatus { token }
        | AuthCommand::AirsimOpenPairing { token }
        | AuthCommand::AirsimClosePairing { token }
        | AuthCommand::PendingPairings { token }
        | AuthCommand::Clients { token }
        | AuthCommand::Revoke { token, .. } => {
            if token.len() != 64 {
                return Err(invalid());
            }
        }
        AuthCommand::AirsimConfigure {
            token, pcm_rate, ..
        } => {
            if token.len() != 64 || !matches!(pcm_rate, 8_000 | 16_000) {
                return Err(invalid());
            }
        }
        AuthCommand::AirsimRevoke { token, pairing_id } => {
            if token.len() != 64
                || pairing_id.len() != 24
                || !pairing_id.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(invalid());
            }
        }
        AuthCommand::AirsimPairing {
            method,
            target,
            body,
            peer,
        } => {
            if !airsim_request_valid(method, target, body, peer) {
                return Err(invalid());
            }
        }
        AuthCommand::AirsimControl {
            method,
            target,
            version,
            timestamp,
            nonce,
            signature,
            body,
            peer,
        } => {
            if !airsim_request_valid(method, target, body, peer)
                || [version, timestamp, nonce, signature]
                    .into_iter()
                    .flatten()
                    .any(|h| h.len() > 256 || !h.bytes().all(|b| b.is_ascii_graphic()))
            {
                return Err(invalid());
            }
        }
        AuthCommand::AirsimPcmOpen { preface, peer } => {
            if preface.is_empty()
                || preface.len() > 1024
                || !preface.ends_with('\n')
                || !preface[..preface.len() - 1]
                    .bytes()
                    .all(|b| b.is_ascii_graphic() || b == b' ')
                || peer.len() > 45
                || peer.parse::<std::net::IpAddr>().is_err()
            {
                return Err(invalid());
            }
        }
        AuthCommand::AirsimPoll { ticket } | AuthCommand::AirsimPcmPulse { ticket, .. } => {
            if ticket.len() != 32 || !ticket.bytes().all(|b| b.is_ascii_hexdigit()) {
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
        | AuthCommand::InboxRecent { token, limit, .. }
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

fn canonical_generation(value: &str) -> bool {
    value.len() <= 19
        && value
            .parse::<u64>()
            .is_ok_and(|v| v < i64::MAX as u64 && v.to_string() == value)
}
fn canonical_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
fn decimal_sequence(value: &str) -> bool {
    value
        .parse::<i64>()
        .is_ok_and(|n| n >= 0 && n.to_string() == value)
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
    fn checked_legacy_management_requires_explicit_finite_binding_expectations() {
        let valid = serde_json::json!({"protocol":2,"id":"nas-checked","command":{
            "op":"bind_legacy_checked","token":"a".repeat(64),"core_id":"nas-1", "public_key":vec![2;32],
            "expected_generation":1,"scopes":["status.read"],"expected_binding":{"state":"absent"}}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        let mut present = valid.clone();
        present["command"]["expected_binding"] =
            serde_json::json!({"state":"present","binding_epoch":"b".repeat(64)});
        assert!(parse_auth_request(&serde_json::to_vec(&present).unwrap()).is_ok());
        for expected in [
            serde_json::Value::Null,
            serde_json::json!({"state":"any"}),
            serde_json::json!({"state":"absent","binding_epoch":"b".repeat(64)}),
            serde_json::json!({"state":"present","binding_epoch":"B".repeat(64)}),
            serde_json::json!({"state":"present","binding_epoch":"short"}),
        ] {
            let mut bad = valid.clone();
            bad["command"]["expected_binding"] = expected;
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut missing = valid;
        missing["command"]
            .as_object_mut()
            .unwrap()
            .remove("expected_binding");
        assert!(parse_auth_request(&serde_json::to_vec(&missing).unwrap()).is_err());
        for epoch in ["b".repeat(64), "bad".into(), "B".repeat(64)] {
            let command = serde_json::json!({"protocol":2,"id":"revoke","command":{"op":"revoke_legacy_checked","token":"a".repeat(64),"core_id":"nas-1","expected_binding_epoch":epoch}});
            assert_eq!(
                parse_auth_request(&serde_json::to_vec(&command).unwrap()).is_ok(),
                epoch == "b".repeat(64)
            );
        }
    }
    #[test]
    fn legacy_binding_status_is_a_bounded_read_without_write_parameters() {
        let valid = serde_json::json!({"protocol":2,"id":"binding-query","command":{
            "op":"legacy_binding_status","token":"a".repeat(64),"core_id":"nas-approved_1"}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (field, value) in [
            ("token", serde_json::json!("short")),
            ("core_id", serde_json::json!("")),
            ("core_id", serde_json::json!("a".repeat(129))),
            ("core_id", serde_json::json!("../aliases")),
            ("core_id", serde_json::json!("nas\nother")),
            ("public_key", serde_json::json!(vec![0; 32])),
            ("scopes", serde_json::json!(["sms.send"])),
            ("owner", serde_json::json!(true)),
            ("bind_if_missing", serde_json::json!(true)),
        ] {
            let mut bad = valid.clone();
            bad["command"][field] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
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
    fn recent_lists_are_bounded_and_refuse_extra_controls() {
        for op in ["inbox_recent", "calls_recent"] {
            let good = serde_json::json!({"protocol":2,"id":"recent","command":{"op":op,"token":"a".repeat(64),"before":null,"limit":16}});
            assert!(
                super::parse_auth_request(&serde_json::to_vec(&good).unwrap()).is_ok(),
                "{op}"
            );
            for limit in [0, 33] {
                let bad = serde_json::json!({"protocol":2,"id":"recent","command":{"op":op,"token":"a".repeat(64),"before":null,"limit":limit}});
                assert!(
                    super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                    "{op} {limit}"
                );
            }
            for bad in [
                serde_json::json!({"protocol":2,"id":"recent","command":{"op":op,"token":"short","before":null,"limit":16}}),
                serde_json::json!({"protocol":2,"id":"recent","command":{"op":op,"token":"a".repeat(64),"before":null,"limit":16,"number":true}}),
                serde_json::json!({"protocol":2,"id":"recent","command":{"op":op,"before":null,"limit":16}}),
            ] {
                assert!(
                    super::parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err(),
                    "{op} {bad}"
                );
            }
        }
    }

    #[test]
    fn airsim_commands_are_bounded_and_owner_ones_need_a_token() {
        let token = "a".repeat(64);
        let wrap = |command: serde_json::Value| {
            super::parse_auth_request(
                &serde_json::to_vec(
                    &serde_json::json!({"protocol":2,"id":"airsim","command":command}),
                )
                .unwrap(),
            )
        };
        let control = |patch: &dyn Fn(&mut serde_json::Value)| {
            let mut v = serde_json::json!({"op":"airsim_control","method":"GET","target":"/v1/health",
                "version":"1","timestamp":"1793600000","nonce":"nonce-abc","signature":"sig","body":"","peer":"192.168.1.20"});
            patch(&mut v);
            wrap(v)
        };
        assert!(control(&|_| {}).is_ok());
        assert!(control(&|v| v["version"] = serde_json::Value::Null).is_ok());
        assert!(control(&|v| v["method"] = "DELETE".into()).is_err());
        assert!(control(&|v| v["target"] = "v1/health".into()).is_err());
        assert!(control(&|v| v["target"] = "/a b".into()).is_err());
        assert!(control(&|v| v["target"] = format!("/{}", "a".repeat(512)).into()).is_err());
        assert!(control(&|v| v["peer"] = "not-an-address".into()).is_err());
        assert!(control(&|v| v["nonce"] = "a b".into()).is_err());
        assert!(control(&|v| v["signature"] = "s".repeat(257).into()).is_err());
        assert!(control(&|v| v["body"] = "x".repeat(super::AIRSIM_MAX_BODY + 1).into()).is_err());
        assert!(control(&|v| v["token"] = "a".repeat(64).into()).is_err());
        assert!(wrap(serde_json::json!({"op":"airsim_pairing","method":"POST","target":"/pair/v1/claim","body":"{}","peer":"fe80::1"})).is_ok());
        assert!(wrap(serde_json::json!({"op":"airsim_poll","ticket":"0".repeat(32)})).is_ok());
        assert!(wrap(serde_json::json!({"op":"airsim_poll","ticket":"zz"})).is_err());
        for op in [
            "airsim_status",
            "airsim_open_pairing",
            "airsim_close_pairing",
        ] {
            assert!(
                wrap(serde_json::json!({"op":op,"token":token})).is_ok(),
                "{op}"
            );
            assert!(
                wrap(serde_json::json!({"op":op,"token":"short"})).is_err(),
                "{op}"
            );
            assert!(wrap(serde_json::json!({"op":op})).is_err(), "{op}");
        }
        for rate in [8000, 16000] {
            assert!(wrap(serde_json::json!({"op":"airsim_configure","token":token,"enabled":true,"pcm_rate":rate})).is_ok());
        }
        assert!(wrap(serde_json::json!({"op":"airsim_configure","token":token,"enabled":true,"pcm_rate":44100})).is_err());
        assert!(wrap(
            serde_json::json!({"op":"airsim_revoke","token":token,"pairing_id":"0".repeat(24)})
        )
        .is_ok());
        assert!(
            wrap(serde_json::json!({"op":"airsim_revoke","token":token,"pairing_id":"../x"}))
                .is_err()
        );
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

    #[test]
    fn ap_station_commands_bind_canonical_ids_and_cannot_supply_root_authority() {
        let valid = serde_json::json!({"protocol":2,"id":"ap-station","command":{"op":"setup_ap_wifi_begin","token":"a".repeat(64),"operation_id":"11111111-1111-4111-8111-111111111111","trial_id":"22222222-2222-4222-8222-222222222222","ssid":"synthetic","passphrase":"synthetic-password"}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key, value) in [
            ("operation_id", serde_json::json!("../pending")),
            ("trial_id", serde_json::json!("not-a-uuid")),
            ("ssid", serde_json::json!("中".repeat(11))),
            ("passphrase", serde_json::json!("short")),
            ("token", serde_json::json!("")),
            ("owner_generation", serde_json::json!(1)),
            ("interface", serde_json::json!("wwan0")),
            ("settings", serde_json::json!({"apn":"injected"})),
            ("path", serde_json::json!("/etc/passwd")),
        ] {
            let mut bad = valid.clone();
            bad["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for op in ["setup_ap_wifi_status", "setup_ap_wifi_cancel"] {
            let mut c = valid.clone();
            c["command"].as_object_mut().unwrap().remove("ssid");
            c["command"].as_object_mut().unwrap().remove("passphrase");
            c["command"]["op"] = op.into();
            assert!(parse_auth_request(&serde_json::to_vec(&c).unwrap()).is_ok());
            c["command"]["trial_id"] = "../other".into();
            assert!(parse_auth_request(&serde_json::to_vec(&c).unwrap()).is_err());
        }
    }

    #[test]
    fn ap_rescan_commands_are_owner_token_and_canonical_ids_only() {
        for op in [
            "setup_ap_scan_begin",
            "setup_ap_scan_status",
            "setup_ap_scan_cancel",
        ] {
            let good = serde_json::json!({"protocol":2,"id":"scan","command":{"op":op,"token":"a".repeat(64),
                "operation_id":"11111111-1111-4111-8111-111111111111","scan_id":"22222222-2222-4222-8222-222222222222"}});
            assert!(parse_auth_request(&serde_json::to_vec(&good).unwrap()).is_ok());
            for (key, value) in [
                ("scan_id", serde_json::json!("../scan")),
                ("operation_id", serde_json::json!("wrong")),
                ("token", serde_json::json!("")),
                ("binding", serde_json::json!({})),
                ("interface", serde_json::json!("wlan0")),
                ("ssid", serde_json::json!("synthetic")),
                ("deadline_uptime_ms", serde_json::json!(1)),
            ] {
                let mut bad = good.clone();
                bad["command"][key] = value;
                assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
            }
        }
    }
    #[test]
    fn ap_confirmation_and_status_reject_credential_changes_and_authority_injection() {
        let valid = serde_json::json!({"protocol":2,"id":"confirm","command":{"op":"setup_ap_wifi_confirm","token":"a".repeat(64),"operation_id":"11111111-1111-4111-8111-111111111111","trial_id":"22222222-2222-4222-8222-222222222222","commit_id":"33333333-3333-4333-8333-333333333333","ssid":"synthetic","passphrase":"synthetic-password"}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key, value) in [
            ("commit_id", serde_json::json!("../else")),
            ("trial_id", serde_json::json!("short")),
            ("passphrase", serde_json::json!("weak")),
            ("owner_generation", serde_json::json!(1)),
            ("settings", serde_json::json!({})),
            ("path", serde_json::json!("/etc/profile")),
        ] {
            let mut bad = valid.clone();
            bad["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut status = valid;
        status["command"]["op"] = "setup_ap_wifi_commit_status".into();
        for key in ["trial_id", "ssid", "passphrase"] {
            status["command"].as_object_mut().unwrap().remove(key);
        }
        assert!(parse_auth_request(&serde_json::to_vec(&status).unwrap()).is_ok());
        status["command"]["commit_id"] = "../no".into();
        assert!(parse_auth_request(&serde_json::to_vec(&status).unwrap()).is_err());
    }
    #[test]
    fn open_setup_claim_has_no_code_or_token_and_rejects_malformed_finish() {
        let challenge = serde_json::json!({"protocol":2,"id":"open","command":{"op":"setup_open_challenge","public_key":vec![1;32]}});
        assert!(parse_auth_request(&serde_json::to_vec(&challenge).unwrap()).is_ok());
        let mut bad = challenge.clone();
        bad["command"]["setup_code"] = "0".repeat(64).into();
        assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        let good = serde_json::json!({"protocol":2,"id":"open","command":{"op":"setup_open_finish","challenge_id":"a".repeat(64),"signature":vec![0;64]}});
        assert!(parse_auth_request(&serde_json::to_vec(&good).unwrap()).is_ok());
        for (key, value) in [
            ("challenge_id", serde_json::json!("A".repeat(64))),
            ("challenge_id", serde_json::json!("../x")),
            ("signature", serde_json::json!(vec![0; 63])),
        ] {
            let mut bad = good.clone();
            bad["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
    }
    #[test]
    fn ap_read_only_lookup_rejects_paths_uppercase_and_extra_fields() {
        let good = serde_json::json!({"protocol":2,"id":"lookup","command":{"op":"setup_ap_operation_status","token":"a".repeat(64),"request_key_sha256":"b".repeat(64)}});
        assert!(parse_auth_request(&serde_json::to_vec(&good).unwrap()).is_ok());
        for v in ["../pending".to_owned(), "B".repeat(64), "a".repeat(63)] {
            let mut bad = good.clone();
            bad["command"]["request_key_sha256"] = v.into();
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut bad = good;
        bad["command"]["path"] = "/etc/profile".into();
        assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
    #[test]
    fn historical_forwarding_commands_reject_unsafe_identifiers_generations_and_authority_overrides(
    ) {
        let valid = serde_json::json!({"protocol":2,"id":"history","command":{"op":"forwarding_history_prepare","token":"a".repeat(64),"expected_generation":1,"request_id":"11111111-1111-4111-8111-111111111111","range":{"after":null,"limit":16,"targets":["22222222-2222-4222-8222-222222222222"]}}});
        assert!(parse_auth_request(&serde_json::to_vec(&valid).unwrap()).is_ok());
        for (key, value) in [
            ("request_id", serde_json::json!("../outbox")),
            ("expected_generation", serde_json::json!(0)),
            ("owner_generation", serde_json::json!(1)),
            ("path", serde_json::json!("/run/worker")),
        ] {
            let mut bad = valid.clone();
            bad["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        let mut commit = valid.clone();
        commit["command"]["op"] = "forwarding_history_commit".into();
        commit["command"].as_object_mut().unwrap().remove("range");
        assert!(parse_auth_request(&serde_json::to_vec(&commit).unwrap()).is_err());
        commit["command"]["accept_full_text"] = false.into();
        assert!(parse_auth_request(&serde_json::to_vec(&commit).unwrap()).is_err());
        commit["command"]["confirmation_id"] = "33333333-3333-4333-8333-333333333333".into();
        assert!(parse_auth_request(&serde_json::to_vec(&commit).unwrap()).is_ok());
        for op in ["forwarding_history_cancel", "forwarding_history_request"] {
            let mut command = commit.clone();
            command["command"]["op"] = op.into();
            command["command"]
                .as_object_mut()
                .unwrap()
                .remove("expected_generation");
            command["command"]
                .as_object_mut()
                .unwrap()
                .remove("accept_full_text");
            if op == "forwarding_history_request" {
                command["command"]
                    .as_object_mut()
                    .unwrap()
                    .remove("confirmation_id");
            }
            assert!(parse_auth_request(&serde_json::to_vec(&command).unwrap()).is_ok());
        }
        let mut archive = commit;
        archive["command"]["op"] = "forwarding_history_archive".into();
        archive["command"]
            .as_object_mut()
            .unwrap()
            .remove("accept_full_text");
        assert!(parse_auth_request(&serde_json::to_vec(&archive).unwrap()).is_ok());
    }
}

#[cfg(test)]
mod nkn_candidate_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn participation_requires_disclosure_and_exact_context_without_paths_or_keys() {
        let original = json!({"protocol":2,"id":"consent","command":{"op":"usage_participation_set","token":"a".repeat(64),"enabled":true,"disclosure_version":1,"expected_generation":"0","expected_core_instance":"b".repeat(64),"request_id":"11111111-1111-4111-8111-111111111111"}});
        assert!(parse_auth_request(&serde_json::to_vec(&original).unwrap()).is_ok());
        for (key, value) in [
            ("disclosure_version", json!(null)),
            ("disclosure_version", json!(2)),
            ("enabled", json!(false)),
            ("expected_generation", json!(0)),
            ("expected_generation", json!("00")),
            ("expected_core_instance", json!("other")),
            ("seed", json!("private")),
            ("directory", json!("/tmp/other")),
            ("request_id", json!("not-an-id")),
        ] {
            let mut changed = original.clone();
            changed["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
        let mut off = original;
        off["command"]["enabled"] = false.into();
        off["command"]["disclosure_version"] = serde_json::Value::Null;
        assert!(parse_auth_request(&serde_json::to_vec(&off).unwrap()).is_ok());
    }
    #[test]
    fn usage_query_accepts_only_fixed_windows_without_endpoint_or_key_fields() {
        let original = json!({"protocol":2,"id":"usage","command":{"op":"usage_stats_query","token":"a".repeat(64),"window":"24h"}});
        for window in ["24h", "30d", "90d"] {
            let mut value = original.clone();
            value["command"]["window"] = json!(window);
            assert!(parse_auth_request(&serde_json::to_vec(&value).unwrap()).is_ok());
        }
        for (key, value) in [
            ("window", json!("1h")),
            ("window", json!(24)),
            ("token", json!("missing")),
            ("url", json!("https://example.com")),
            ("topic", json!("other-app")),
            ("seed", json!([1, 2, 3])),
            ("enabled", json!(true)),
        ] {
            let mut changed = original.clone();
            changed["command"][key] = value;
            assert!(parse_auth_request(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
    }
    #[test]
    fn nkn_control_has_finite_reference_and_exact_context_cas_without_root_claims() {
        let value = json!({"protocol":2,"id":"nkn","command":{"op":"set_nkn_enabled","token":"a".repeat(64),"owner":"remote","enabled":true,"expected_revision":"0","expected_core_instance":"b".repeat(64),"request_id":"11111111-1111-4111-8111-111111111111"}});
        assert!(parse_auth_request(&serde_json::to_vec(&value).unwrap()).is_ok());
        for (key, v) in [
            ("owner", json!("root")),
            ("owner", json!("arbitrary_service")),
            ("expected_revision", json!(0)),
            ("expected_revision", json!("00")),
            ("expected_revision", json!("18446744073709551616")),
            ("expected_core_instance", json!("claimed-current-core")),
            ("enabled", json!(1)),
            ("request_id", json!("../chosen")),
            ("root", json!(true)),
            ("pid", json!(123)),
        ] {
            let mut changed = value.clone();
            changed["command"][key] = v;
            assert!(parse_auth_request(&serde_json::to_vec(&changed).unwrap()).is_err());
        }
    }
}
