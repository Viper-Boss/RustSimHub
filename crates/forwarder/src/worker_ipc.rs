// SPDX-License-Identifier: AGPL-3.0-only
//! Finite Root/worker protocol, separate from browser authentication. One
//! request per Unix connection; the server MUST select roles using SO_PEERCRED.
use crate::{model::uuid, Claim, DeliveryOutcome, Error, Fence, Result, Secret, Stage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::timeout,
};

pub const PROTOCOL: u16 = 4;
pub const MAX_FRAME: usize = 65_536;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub protocol: u16,
    pub id: String,
    pub command: Command,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    State {},
    Grant {},
    Revoke {},
    Acquire {
        lease: Secret,
    },
    Check {
        lease: Secret,
        attempt_id: String,
        claim_sha256: [u8; 32],
        stage: Stage,
    },
    Finish {
        lease: Secret,
        attempt_id: String,
        outcome: DeliveryOutcome,
    },
}
impl Command {
    pub fn root_only(&self) -> bool {
        matches!(self, Self::State {} | Self::Grant {} | Self::Revoke {})
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub protocol: u16,
    pub id: String,
    pub result: Value,
}
pub fn parse(bytes: &[u8]) -> Result<Request> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Invalid);
    }
    let request: Request = serde_json::from_slice(bytes)?;
    if request.protocol != PROTOCOL || !uuid(&request.id) {
        return Err(Error::Invalid);
    }
    match &request.command {
        Command::Acquire { lease }
        | Command::Check { lease, .. }
        | Command::Finish { lease, .. }
            if lease.expose().len() != 64
                || !lease
                    .expose()
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            return Err(Error::Invalid);
        }
        _ => {}
    }
    match &request.command {
        Command::Check { attempt_id, .. } | Command::Finish { attempt_id, .. }
            if !uuid(attempt_id) =>
        {
            return Err(Error::Invalid)
        }
        Command::Finish {
            outcome: DeliveryOutcome::Confirmed { status },
            ..
        } if !(200..300).contains(status) => return Err(Error::Invalid),
        _ => {}
    }
    Ok(request)
}
pub async fn read_frame(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let len = stream.read_u32().await? as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(Error::Invalid);
    }
    let mut bytes = vec![0; len];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}
pub async fn write_frame(stream: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Invalid);
    }
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(bytes).await?;
    Ok(())
}
/// No retry, persistent socket, ambient authority or task. Root passes the
/// secret only to its selected worker; server checks the actual peer UID too.
pub async fn request(socket: &std::path::Path, core_uid: u32, command: Command) -> Result<Value> {
    timeout(Duration::from_secs(5), async {
        let mut stream = UnixStream::connect(socket).await?;
        if stream.peer_cred().map(|p| p.uid()).ok() != Some(core_uid) {
            return Err(Error::Forbidden);
        }
        let id = uuid::Uuid::new_v4().to_string();
        let bytes = serde_json::to_vec(&Request {
            protocol: PROTOCOL,
            id: id.clone(),
            command,
        })?;
        parse(&bytes)?;
        write_frame(&mut stream, &bytes).await?;
        let response: Response = serde_json::from_slice(&read_frame(&mut stream).await?)?;
        if response.protocol != PROTOCOL || response.id != id {
            return Err(Error::Forbidden);
        }
        if let Some(error) = response.result.get("error") {
            return Err(if error == "retry_later" {
                Error::Busy
            } else {
                Error::Forbidden
            });
        }
        Ok(response.result)
    })
    .await
    .map_err(|_| Error::Storage)? // A lost response is ambiguous, never retry-later.
}
pub struct Client {
    socket: PathBuf,
    core_uid: u32,
    lease: Secret,
}

/// Hash the ACTUAL immutable claim about to be used by the HTTPS sender,
/// including target URL, credentials, payload, IDs, generation and SIM line.
/// Never reuse a digest copied from a pre-mutation/acquire response.
pub fn claim_sha256(claim: &Claim) -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let encoded = zeroize::Zeroizing::new(serde_json::to_vec(claim)?);
    Ok(Sha256::digest(encoded.as_slice()).into())
}
impl Client {
    pub fn new(socket: PathBuf, core_uid: u32, lease: Secret) -> Self {
        Self {
            socket,
            core_uid,
            lease,
        }
    }
    fn token(&self) -> Secret {
        Secret::new(self.lease.expose().to_owned())
    }
    pub async fn acquire(&mut self) -> Result<Option<Claim>> {
        let value = request(
            &self.socket,
            self.core_uid,
            Command::Acquire {
                lease: self.token(),
            },
        )
        .await?;
        let claim: Option<Claim> =
            serde_json::from_value(value.get("claim").ok_or(Error::Invalid)?.clone())?;
        let digest: Option<[u8; 32]> =
            serde_json::from_value(value.get("claim_sha256").ok_or(Error::Invalid)?.clone())?;
        if claim.as_ref().map(claim_sha256).transpose()? != digest {
            return Err(Error::Invalid);
        }
        Ok(claim)
    }
    pub async fn finish(&mut self, claim: &Claim, outcome: DeliveryOutcome) -> Result<()> {
        let value = request(
            &self.socket,
            self.core_uid,
            Command::Finish {
                lease: self.token(),
                attempt_id: claim.attempt_id.clone(),
                outcome,
            },
        )
        .await?;
        if value["accepted"] != true {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}
impl Fence for Client {
    async fn check(&mut self, stage: Stage, claim: &Claim) -> Result<()> {
        let value = request(
            &self.socket,
            self.core_uid,
            Command::Check {
                lease: self.token(),
                attempt_id: claim.attempt_id.clone(),
                claim_sha256: claim_sha256(claim)?,
                stage,
            },
        )
        .await?;
        if value["allowed"] != true {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn only_explicit_preadmission_busy_is_retryable() {
        for close in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("core.sock");
            let listener = tokio::net::UnixListener::bind(&path).unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = parse(&read_frame(&mut stream).await.unwrap()).unwrap();
                if !close {
                    let response = Response {
                        protocol: PROTOCOL,
                        id: request.id,
                        result: serde_json::json!({"error":"retry_later"}),
                    };
                    write_frame(&mut stream, &serde_json::to_vec(&response).unwrap())
                        .await
                        .unwrap();
                }
            });
            let result = request(
                &path,
                rustix::process::geteuid().as_raw(),
                Command::Grant {},
            )
            .await;
            server.await.unwrap();
            if close {
                assert!(result.is_err() && !matches!(result, Err(Error::Busy)));
            } else {
                assert!(matches!(result, Err(Error::Busy)));
            }
        }
    }
}
