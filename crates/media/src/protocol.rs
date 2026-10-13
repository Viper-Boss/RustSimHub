// SPDX-License-Identifier: AGPL-3.0-only
use serde::{Deserialize, Serialize};
pub const PROTOCOL: u16 = 1;
pub const MAX_FRAME: usize = 2048;
// Deliberately no Debug: credential and request carry bearer tickets.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub ticket: [u8; 32],
    pub peer_key: [u8; 32],
    pub boot: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub id: u64,
    pub command: Command,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Redeem {
        ticket: [u8; 32],
        peer_key: [u8; 32],
    },
    Pulse {},
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub protocol: u16,
    pub id: u64,
    pub boot: String,
    pub until_ms: u64,
}
pub fn parse(bytes: &[u8]) -> Result<Request, crate::Error> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(crate::Error::Denied);
    }
    let r: Request = serde_json::from_slice(bytes).map_err(|_| crate::Error::Denied)?;
    if r.protocol != PROTOCOL || r.id == 0 {
        return Err(crate::Error::Denied);
    }
    Ok(r)
}
#[cfg(test)]
mod tests {
    #[test]
    fn media_protocol_rejects_control_actions_roles_and_unknown_fields() {
        for bytes in [
            r#"{"protocol":1,"id":1,"command":{"op":"pulse","role":"owner"}}"#,
            r#"{"protocol":2,"id":1,"command":{"op":"pulse"}}"#,
            r#"{"protocol":1,"id":1,"command":{"op":"dial"}}"#,
            r#"{"protocol":1,"id":0,"command":{"op":"pulse"}}"#,
        ] {
            assert!(super::parse(bytes.as_bytes()).is_err());
        }
        assert!(super::parse(&[b' '; super::MAX_FRAME + 1]).is_err());
    }
}
