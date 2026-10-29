// SPDX-License-Identifier: AGPL-3.0-only
//! Owner-only administration in the same Auth. No BLE capability/session/source acceptance.
use rustsimhub_auth::{Auth, Error, Result};
use rustsimhub_wire::AuthCommand;
use serde_json::Value;

pub(crate) fn dispatch(auth: &mut Auth, command: AuthCommand) -> Result<Value> {
    #[cfg(not(feature = "recovery-grants"))]
    {
        let _ = (auth, command);
        Err(Error::Forbidden)
    }
    #[cfg(feature = "recovery-grants")]
    {
        use AuthCommand::*;
        let generation = |v: &str| v.parse::<u64>().map_err(|_| Error::InvalidInput);
        match command {
            RecoveryGrant {
                token,
                public_key,
                max_trial_ms,
                expected_generation,
                request_id,
            } => serde_json::to_value(auth.grant_recovery_key(
                &token,
                public_key,
                max_trial_ms,
                generation(&expected_generation)?,
                request_id,
            )?),
            RecoveryRevoke {
                token,
                public_key,
                expected_generation,
                request_id,
            } => serde_json::to_value(auth.revoke_recovery_key(
                &token,
                public_key,
                generation(&expected_generation)?,
                request_id,
            )?),
            RecoveryKey { token, public_key } => {
                serde_json::to_value(auth.recovery_key(&token, public_key)?)
            }
            RecoveryRequest { token, request_id } => {
                serde_json::to_value(auth.recovery_key_request(&token, request_id)?)
            }
            _ => return Err(Error::Forbidden),
        }
        .map_err(|_| Error::InvalidInput)
    }
}
