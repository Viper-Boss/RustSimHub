// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
mod auth;
pub use auth::{
    parse_auth_request, wifi_credentials_valid, AuthCommand, AuthRequest, AUTH_PROTOCOL,
};
pub use rustsimhub_domain::{InboxCursor, NasEntity};

pub const MAX_FRAME: usize = 65_536;
pub const PROTOCOL: u16 = 1;

/// Compile-time provenance; Cargo tracks this environment input in its fingerprint.
pub fn build_source_sha256() -> Option<&'static str> {
    option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256").filter(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub id: String,
    pub command: Command,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    Health,
    Capabilities,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub protocol: u16,
    pub id: String,
    pub result: serde_json::Value,
}

pub fn check_len(len: u32) -> io::Result<usize> {
    if len == 0 || len as usize > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame size",
        ));
    }
    Ok(len as usize)
}

pub fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let len = check_len(u32::from_be_bytes(header))?;
    let mut body = vec![0; len];
    reader.read_exact(&mut body)?;
    Ok(body)
}

pub fn write_frame(writer: &mut impl Write, body: &[u8]) -> io::Result<()> {
    if body.is_empty() || body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid frame size",
        ));
    }
    writer.write_all(&(body.len() as u32).to_be_bytes())?;
    writer.write_all(body)
}

pub fn parse_request(body: &[u8]) -> io::Result<Request> {
    let request: Request = serde_json::from_slice(body)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid request"))?;
    if request.protocol != PROTOCOL || request.id.is_empty() || request.id.len() > 128 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid protocol or request id",
        ));
    }
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_size_is_rejected_before_body_read() {
        for size in [0, u32::MAX, MAX_FRAME as u32 + 1] {
            assert_eq!(
                read_frame(&mut &size.to_be_bytes()[..]).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }
    #[test]
    fn truncated_frame_is_not_a_valid_request() {
        let mut bytes = vec![];
        write_frame(&mut bytes, b"abc").unwrap();
        bytes.pop();
        assert_eq!(
            read_frame(&mut &bytes[..]).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    #[test]
    fn injected_roles_unknown_commands_and_versions_fail_closed() {
        for json in [
            r#"{"protocol":1,"id":"a","command":"health","role":"owner"}"#,
            r#"{"protocol":1,"id":"a","command":"shell"}"#,
            r#"{"protocol":99,"id":"a","command":"health"}"#,
            r#"{"protocol":1,"id":"","command":"health"}"#,
        ] {
            assert!(parse_request(json.as_bytes()).is_err());
        }
    }
}
