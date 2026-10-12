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
    OpenPairing {
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
        | AuthCommand::OpenPairing { token }
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
        AuthCommand::PairChallenge { scopes, .. } => {
            if scopes.len() > 10 {
                return Err(invalid());
            }
        }
        _ => {}
    }
    Ok(req)
}
#[cfg(test)]
mod tests {
    use super::*;
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
