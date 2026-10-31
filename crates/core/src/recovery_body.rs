// SPDX-License-Identifier: AGPL-3.0-only
//! Opt-in Core application-body v1 decoding AFTER actual Auth verification.
//! No secure-session framing, listener, Wi-Fi actor or backend is installed.
//! All synchronous gates call SQLite: use the existing blocking Auth worker.
use rustsimhub_auth::{
    recovery_capability::{AuthorizedRecoveryRequest, RecoveryAction},
    Auth, Error, RecoveryExecutionAdmission, RecoveryExecutionClaim, RecoveryExecutionPending,
    RecoveryExecutionRequest, RecoveryTrialAuthority, RecoveryTrialControl, Result,
};
use rustsimhub_storage::RecoveryExecutionReceipt;
use serde::{Deserialize, Deserializer};
use zeroize::Zeroizing;

/// These strings are never Debug/Serialize; parsed SSID and password allocations
/// are zeroized on error/drop. This does not promise serde scratch is wiped.
struct PrivateText(Zeroizing<String>);
impl<'de> Deserialize<'de> for PrivateText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        String::deserialize(d).map(|s| Self(Zeroizing::new(s)))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeginBody {
    ssid: PrivateText,
    passphrase: PrivateText,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlBody {
    trial_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LookupBody {
    target_request_id: String,
}
fn parse<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|_| Error::InvalidInput)
}
fn canonical_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}

pub struct RecoveryWifiCredentials {
    ssid: PrivateText,
    passphrase: PrivateText,
}
impl RecoveryWifiCredentials {
    pub fn ssid(&self) -> &str {
        &self.ssid.0
    }
    pub fn passphrase(&self) -> &str {
        &self.passphrase.0
    }
}
enum Parameters {
    Begin(RecoveryWifiCredentials),
    Control,
}
/// Only decode_v1 constructs these opaque values. No caller-supplied target or
/// independent credentials can be attached after authentication/admission.
pub enum DecodedRecoveryRequest {
    Write(Box<CoreRecoveryWrite>),
    Lookup(Box<CoreRecoveryLookup>),
}
pub struct CoreRecoveryWrite {
    request: RecoveryExecutionRequest,
    parameters: Parameters,
}
pub struct CoreRecoveryLookup {
    request: AuthorizedRecoveryRequest,
    target: String,
}
pub enum CoreRecoveryAdmission {
    New(CoreRecoveryPending),
    Existing(Box<RecoveryExecutionReceipt>),
}
pub struct CoreRecoveryPending {
    pending: Box<RecoveryExecutionPending>,
    parameters: Parameters,
}
pub struct CoreRecoveryClaim {
    claim: RecoveryExecutionClaim,
    parameters: Parameters,
}
pub struct CoreRecoveryTrial {
    authority: RecoveryTrialAuthority,
    credentials: RecoveryWifiCredentials,
}

/// Local application-body v1 candidate, not A's encrypted wire protocol. The
/// private capability/action/exact body/current grant are checked BEFORE parse;
/// no payload role, request action, token or "verified" flag is accepted.
pub fn decode_v1(
    auth: &Auth,
    request: AuthorizedRecoveryRequest,
) -> Result<DecodedRecoveryRequest> {
    let action = request.capability.action();
    auth.check_recovery_capability(&request.capability, action, &request.body)?;
    let (parameters, target) = match action {
        RecoveryAction::BeginWifi => {
            let b: BeginBody = parse(&request.body)?;
            if !rustsimhub_wire::wifi_credentials_valid(&b.ssid.0, &b.passphrase.0) {
                return Err(Error::InvalidInput);
            }
            (
                Parameters::Begin(RecoveryWifiCredentials {
                    ssid: b.ssid,
                    passphrase: b.passphrase,
                }),
                None,
            )
        }
        RecoveryAction::ConfirmWifi | RecoveryAction::CancelWifi => {
            let b: ControlBody = parse(&request.body)?;
            // Auth performs the strict 64 lowercase-hex target validation.
            (Parameters::Control, Some(b.trial_id))
        }
        RecoveryAction::QueryOperation => {
            let b: LookupBody = parse(&request.body)?;
            if !canonical_uuid(&b.target_request_id) {
                return Err(Error::InvalidInput);
            }
            auth.check_recovery_capability(&request.capability, action, &request.body)?;
            return Ok(DecodedRecoveryRequest::Lookup(Box::new(
                CoreRecoveryLookup {
                    request,
                    target: b.target_request_id,
                },
            )));
        }
        _ => return Err(Error::InvalidInput),
    };
    let request = auth.prepare_recovery_execution(request, target)?;
    Ok(DecodedRecoveryRequest::Write(Box::new(CoreRecoveryWrite {
        request,
        parameters,
    })))
}
impl CoreRecoveryWrite {
    pub fn action(&self) -> RecoveryAction {
        self.request.action()
    }
    pub fn request_id(&self) -> &str {
        self.request.request_id()
    }
    pub fn target_trial(&self) -> Option<&str> {
        self.request.target_trial()
    }
    pub fn credentials(&self) -> Option<&RecoveryWifiCredentials> {
        match &self.parameters {
            Parameters::Begin(c) => Some(c),
            Parameters::Control => None,
        }
    }
    pub fn reserve(self: Box<Self>, auth: &Auth) -> Result<CoreRecoveryAdmission> {
        let Self {
            request,
            parameters,
        } = *self;
        Ok(match auth.reserve_recovery_execution(request)? {
            RecoveryExecutionAdmission::Reserved(pending) => {
                CoreRecoveryAdmission::New(CoreRecoveryPending {
                    pending,
                    parameters,
                })
            }
            RecoveryExecutionAdmission::Existing(receipt) => {
                CoreRecoveryAdmission::Existing(receipt)
            }
        })
    }
}
impl CoreRecoveryLookup {
    pub fn target_request_id(&self) -> &str {
        &self.target
    }
    /// Always uses the UUID parsed from THIS exact authenticated body.
    pub fn lookup(&self, auth: &Auth) -> Result<Option<RecoveryExecutionReceipt>> {
        auth.lookup_recovery_execution(&self.request, self.target.clone())
    }
}
impl CoreRecoveryPending {
    /// Original engine allocates a Begin trial id before effects. A control
    /// cannot substitute a new id; Auth/Storage enforce that independently.
    pub fn claim(self, auth: &Auth, new_trial: Option<String>) -> Result<CoreRecoveryClaim> {
        let Self {
            pending,
            parameters,
        } = self;
        let claim = auth.claim_recovery_execution(pending, new_trial)?;
        Ok(CoreRecoveryClaim { claim, parameters })
    }
}
impl CoreRecoveryClaim {
    pub fn request_id(&self) -> &str {
        self.claim.request().request_id()
    }
    pub fn action(&self) -> RecoveryAction {
        self.claim.request().action()
    }
    pub fn begin(self, auth: &Auth) -> Result<CoreRecoveryTrial> {
        let Parameters::Begin(credentials) = self.parameters else {
            return Err(Error::InvalidInput);
        };
        let authority = auth.begin_recovery_trial(self.claim)?;
        Ok(CoreRecoveryTrial {
            authority,
            credentials,
        })
    }
    pub fn bind_control(
        self,
        auth: &Auth,
        trial: &CoreRecoveryTrial,
    ) -> Result<RecoveryTrialControl> {
        if !matches!(self.parameters, Parameters::Control) {
            return Err(Error::InvalidInput);
        }
        auth.bind_recovery_trial_control(&trial.authority, self.claim)
    }
}
impl CoreRecoveryTrial {
    pub fn authority(&self) -> &RecoveryTrialAuthority {
        &self.authority
    }
    pub fn credentials(&self) -> &RecoveryWifiCredentials {
        &self.credentials
    }
}

#[cfg(test)]
mod tests;
