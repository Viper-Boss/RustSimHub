// SPDX-License-Identifier: AGPL-3.0-only
use crate::{parse_key, public_hex, session::Session, Result};
use rustsimhub_auth::{Challenge, ClientKey, ProofExpectation};
use rustsimhub_policy::{mask, Kind, Scope};
use rustsimhub_wire::{check_len, AuthCommand, AuthRequest, Response, MAX_FRAME};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::timeout,
};
fn socket_owner(socket: &str) -> Result<u32> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let path = Path::new(socket);
    let meta = std::fs::symlink_metadata(path)?;
    let parent = std::fs::symlink_metadata(path.parent().ok_or("socket parent missing")?)?;
    if !meta.file_type().is_socket()
        || !parent.is_dir()
        || parent.uid() != meta.uid()
        || parent.mode() & 0o022 != 0
    {
        return Err("unsafe socket".into());
    }
    Ok(meta.uid())
}
fn check_session(socket: &str, s: &Session) -> Result<()> {
    if socket_owner(socket)? != s.server_uid {
        return Err("server UID changed".into());
    }
    Ok(())
}
fn kind(value: &str) -> Result<Kind> {
    match value {
        "client" => Ok(Kind::Client),
        "nas" => Ok(Kind::Nas),
        _ => Err("invalid kind".into()),
    }
}
fn scopes(value: &str) -> Result<Vec<Scope>> {
    if value.len() > 200 {
        return Err("invalid scopes".into());
    }
    let scopes: Vec<Scope> = value
        .split(',')
        .map(|s| serde_json::from_value(Value::String(s.into())))
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| "invalid scopes")?;
    mask(&scopes).ok_or("invalid scopes")?;
    Ok(scopes)
}
async fn call(socket: &str, command: AuthCommand) -> Result<Value> {
    let uid = socket_owner(socket)?;
    let request = AuthRequest {
        protocol: 2,
        id: "ctl".into(),
        command,
    };
    let bytes = serde_json::to_vec(&request)?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("invalid request size".into());
    }
    let reply = timeout(Duration::from_secs(15), async {
        let mut s = UnixStream::connect(socket).await?;
        if s.peer_cred()?.uid() != uid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "unexpected server UID",
            ));
        }
        s.write_u32(bytes.len() as u32).await?;
        s.write_all(&bytes).await?;
        let len = check_len(s.read_u32().await?)?;
        let mut reply = vec![0; len];
        s.read_exact(&mut reply).await?;
        Ok::<_, std::io::Error>(reply)
    })
    .await??;
    let reply: Response = serde_json::from_slice(&reply).map_err(|_| "invalid response")?;
    if reply.protocol != 2 || reply.id != "ctl" || reply.result.get("error").is_some() {
        return Err("request denied".into());
    }
    Ok(reply.result)
}
pub async fn run(args: &[String]) -> Result<()> {
    if let [command, socket, session, lease_path, peer, output] = args {
        if command == "issue-media" {
            let s = Session::load(Path::new(session))?;
            check_session(socket, &s)?;
            let lease = serde_json::from_slice(&rustsimhub_auth::read_private(
                Path::new(lease_path),
                MAX_FRAME,
            )?)
            .map_err(|_| "invalid lease")?;
            let value = call(
                socket,
                AuthCommand::IssueMedia {
                    token: s.token.clone(),
                    lease,
                    peer_key: parse_key(peer)?,
                },
            )
            .await?;
            rustsimhub_auth::write_private_new(Path::new(output), &serde_json::to_vec(&value)?)?;
            println!("{}", json!({"ok":true,"media_ticket_saved":true}));
            return Ok(());
        }
    }
    if let [command, input, output] = args {
        if command == "save-call-lease" {
            let bytes = rustsimhub_auth::read_private(Path::new(input), MAX_FRAME)?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid operation")?;
            let lease: rustsimhub_domain::CallLease =
                serde_json::from_value(value.get("lease").cloned().ok_or("no lease")?)
                    .map_err(|_| "invalid lease")?;
            rustsimhub_auth::write_private_new(Path::new(output), &serde_json::to_vec(&lease)?)?;
            println!("{}", json!({"ok":true,"lease_saved":true}));
            return Ok(());
        }
    }
    if let [command, socket, session] = args {
        if matches!(
            command.as_str(),
            "calls" | "call-events" | "call-status" | "network"
        ) {
            let s = Session::load(Path::new(session))?;
            check_session(socket, &s)?;
            let cmd = match command.as_str() {
                "calls" => AuthCommand::Calls {
                    token: s.token.clone(),
                    after: None,
                    limit: 32,
                },
                "call-events" => AuthCommand::CallEvents {
                    token: s.token.clone(),
                    after: None,
                    limit: 32,
                },
                "network" => AuthCommand::NetworkStatus {
                    token: s.token.clone(),
                },
                _ => AuthCommand::CallStatus {
                    token: s.token.clone(),
                },
            };
            println!("{}", serde_json::to_string(&call(socket, cmd).await?)?);
            return Ok(());
        }
    }
    if let [command, socket, session, input] = args {
        if command == "wifi-trial" {
            let s = Session::load(Path::new(session))?;
            check_session(socket, &s)?;
            println!(
                "{}",
                serde_json::to_string(
                    &call(
                        socket,
                        AuthCommand::WifiTrial {
                            token: s.token.clone(),
                            idempotency_key: input.clone()
                        }
                    )
                    .await?
                )?
            );
            return Ok(());
        }
        if matches!(
            command.as_str(),
            "call" | "prepare-call" | "call-operation" | "renew-call"
        ) {
            let s = Session::load(Path::new(session))?;
            check_session(socket, &s)?;
            let cmd = match command.as_str() {
                "call" => AuthCommand::CallDetail {
                    token: s.token.clone(),
                    call_id: input.clone(),
                },
                "call-operation" => AuthCommand::CallOperation {
                    token: s.token.clone(),
                    operation_id: input.clone(),
                },
                "renew-call" => AuthCommand::RenewCall {
                    token: s.token.clone(),
                    lease: serde_json::from_slice(&rustsimhub_auth::read_private(
                        Path::new(input),
                        MAX_FRAME,
                    )?)
                    .map_err(|_| "invalid lease")?,
                },
                _ => {
                    #[derive(serde::Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Draft {
                        line_epoch: u64,
                        idempotency_key: String,
                        action: rustsimhub_domain::CallAction,
                        call_id: Option<String>,
                        payload: String,
                    }
                    let d: Draft = serde_json::from_slice(&rustsimhub_auth::read_private(
                        Path::new(input),
                        MAX_FRAME,
                    )?)
                    .map_err(|_| "invalid private call draft")?;
                    AuthCommand::PrepareCall {
                        token: s.token.clone(),
                        line_epoch: d.line_epoch,
                        idempotency_key: d.idempotency_key,
                        action: d.action,
                        call_id: d.call_id,
                        payload: d.payload,
                    }
                }
            };
            println!("{}", serde_json::to_string(&call(socket, cmd).await?)?);
            return Ok(());
        }
    }
    if let [command, socket, session, id, lease_path, priority] = args {
        if command == "submit-call" {
            let s = Session::load(Path::new(session))?;
            check_session(socket, &s)?;
            let lease = if lease_path == "none" {
                None
            } else {
                Some(
                    serde_json::from_slice(&rustsimhub_auth::read_private(
                        Path::new(lease_path),
                        MAX_FRAME,
                    )?)
                    .map_err(|_| "invalid lease")?,
                )
            };
            let urgent = match priority.as_str() {
                "normal" => false,
                "hangup" => true,
                _ => return Err("invalid priority".into()),
            };
            println!(
                "{}",
                serde_json::to_string(
                    &call(
                        socket,
                        AuthCommand::SubmitCall {
                            token: s.token.clone(),
                            operation_id: id.clone(),
                            lease,
                            urgent
                        }
                    )
                    .await?
                )?
            );
            return Ok(());
        }
    }

    if let [command, socket, session, input] = args {
        if matches!(
            command.as_str(),
            "prepare-sms" | "submit-sms" | "sms-operation"
        ) {
            let s = Session::load(Path::new(session))?;
            check_session(socket, &s)?;
            let request = if command == "prepare-sms" {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Draft {
                    line_epoch: u64,
                    idempotency_key: String,
                    recipient: String,
                    text: String,
                }
                let bytes = rustsimhub_auth::read_private(Path::new(input), MAX_FRAME)?;
                let d: Draft =
                    serde_json::from_slice(&bytes).map_err(|_| "invalid private draft")?;
                AuthCommand::PrepareSms {
                    token: s.token.clone(),
                    line_epoch: d.line_epoch,
                    idempotency_key: d.idempotency_key,
                    recipient: d.recipient,
                    text: d.text,
                }
            } else if command == "submit-sms" {
                AuthCommand::SubmitSms {
                    token: s.token.clone(),
                    operation_id: input.clone(),
                }
            } else {
                AuthCommand::SmsOperation {
                    token: s.token.clone(),
                    operation_id: input.clone(),
                }
            };
            println!("{}", serde_json::to_string(&call(socket, request).await?)?);
            return Ok(());
        }
    }
    if let [command, socket, path, id] = args {
        if command == "sms" {
            let s = Session::load(Path::new(path))?;
            check_session(socket, &s)?;
            println!(
                "{}",
                serde_json::to_string(
                    &call(
                        socket,
                        AuthCommand::SmsDetail {
                            token: s.token.clone(),
                            sms_id: id.clone()
                        }
                    )
                    .await?
                )?
            );
            return Ok(());
        }
    }
    if let [command, socket, directory, pin, path] = args {
        if command == "login" {
            let key = ClientKey::load(directory)?;
            let pin = parse_key(pin)?;
            let server_uid = socket_owner(socket)?;
            let c: Challenge = serde_json::from_value(
                call(
                    socket,
                    AuthCommand::Challenge {
                        public_key: key.public_key(),
                    },
                )
                .await?,
            )
            .map_err(|_| "invalid challenge")?;
            let proof = key.prove(&c, pin, ProofExpectation::Login)?;
            let issued = call(
                socket,
                AuthCommand::Authenticate {
                    challenge_id: c.id,
                    signature: proof,
                },
            )
            .await?;
            let token = issued["token"]
                .as_str()
                .filter(|v| v.len() == 64)
                .ok_or("invalid session")?
                .to_owned();
            if socket_owner(socket)? != server_uid {
                return Err("server UID changed".into());
            }
            let session = Session {
                protocol: 2,
                device_public_key: pin,
                client_public_key: key.public_key(),
                server_uid,
                token,
            };
            session.write(Path::new(path))?;
            println!(
                "{}",
                json!({"ok":true,"session_saved":true,"expires_in_seconds":900})
            );
            return Ok(());
        }
    }
    if let [command, socket, path] = args {
        let s = Session::load(Path::new(path))?;
        check_session(socket, &s)?;
        let request = match command.as_str() {
            "status" => AuthCommand::Health {
                token: s.token.clone(),
            },
            "capabilities" => AuthCommand::Capabilities {
                token: s.token.clone(),
            },
            "line" => AuthCommand::LineStatus {
                token: s.token.clone(),
            },
            "inbox" => AuthCommand::Inbox {
                token: s.token.clone(),
                after: None,
                limit: 32,
            },
            "inbox-events" => AuthCommand::InboxEvents {
                token: s.token.clone(),
                after: None,
                limit: 32,
            },
            "open-pairing" => AuthCommand::OpenPairing {
                token: s.token.clone(),
            },
            "pending" => AuthCommand::PendingPairings {
                token: s.token.clone(),
            },
            "clients" => AuthCommand::Clients {
                token: s.token.clone(),
            },
            _ => return Err("unknown command".into()),
        };
        println!("{}", serde_json::to_string(&call(socket, request).await?)?);
        return Ok(());
    }
    if let [command, socket, directory, pin, requested_kind, requested_scopes] = args {
        if command == "pair" {
            let key = ClientKey::load(directory)?;
            let pin = parse_key(pin)?;
            let kind = kind(requested_kind)?;
            let scopes = scopes(requested_scopes)?;
            let c: Challenge = serde_json::from_value(
                call(
                    socket,
                    AuthCommand::PairChallenge {
                        public_key: key.public_key(),
                        kind,
                        scopes: scopes.clone(),
                    },
                )
                .await?,
            )
            .map_err(|_| "invalid challenge")?;
            let proof = key.prove(
                &c,
                pin,
                ProofExpectation::Pair {
                    kind,
                    scopes: mask(&scopes).ok_or("invalid scopes")?,
                },
            )?;
            let pending = call(
                socket,
                AuthCommand::PairProof {
                    challenge_id: c.id,
                    signature: proof,
                },
            )
            .await?;
            let id = pending["pairing_id"]
                .as_str()
                .filter(|v| v.len() == 64)
                .ok_or("invalid pairing")?;
            println!(
                "{}",
                json!({"pairing_id":id,"public_key_hex":public_hex(key.public_key()),"kind":kind,"scopes":scopes,"requires_owner_approval":true})
            );
            return Ok(());
        }
    }
    if let [command, socket, path, id, key, requested_kind, requested_scopes] = args {
        if command == "approve" {
            let s = Session::load(Path::new(path))?;
            check_session(socket, &s)?;
            println!(
                "{}",
                serde_json::to_string(
                    &call(
                        socket,
                        AuthCommand::Approve {
                            token: s.token.clone(),
                            pairing_id: id.clone(),
                            public_key: parse_key(key)?,
                            kind: kind(requested_kind)?,
                            scopes: scopes(requested_scopes)?
                        }
                    )
                    .await?
                )?
            );
            return Ok(());
        }
    }
    if let [command, socket, path, key] = args {
        if command == "revoke" {
            let s = Session::load(Path::new(path))?;
            check_session(socket, &s)?;
            println!(
                "{}",
                serde_json::to_string(
                    &call(
                        socket,
                        AuthCommand::Revoke {
                            token: s.token.clone(),
                            public_key: parse_key(key)?
                        }
                    )
                    .await?
                )?
            );
            return Ok(());
        }
    }
    Err("use --help".into())
}
