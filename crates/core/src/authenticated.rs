// SPDX-License-Identifier: AGPL-3.0-only
use crate::linux::{probe, read_frame, write_frame};
use rustsimhub_auth::{Auth, Identity};
use rustsimhub_policy::Scope;
use rustsimhub_storage::Store;
use rustsimhub_wire::{parse_auth_request, AuthCommand, Response, AUTH_PROTOCOL, MAX_FRAME};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::Semaphore,
    time::timeout,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) fn dispatch(auth: &mut Auth, command: AuthCommand) -> rustsimhub_auth::Result<Value> {
    use AuthCommand::*;
    Ok(match command {
        Identity {} => {
            json!({"public_key":auth.public_key(),"signature_algorithm":"ed25519","protocol":AUTH_PROTOCOL})
        }
        Challenge { public_key } => serde_json::to_value(auth.login_challenge(public_key)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        Authenticate {
            challenge_id,
            signature,
        } => {
            let token = auth.login_finish(&challenge_id, &signature)?;
            json!({"token":token.expose(),"expires_in_seconds":900})
        }
        Health { token } => {
            auth.authorize(&token, Scope::StatusRead)?;
            json!({"ok":true,"mode":"authenticated_read_only","write_api":false})
        }
        Capabilities { token } => {
            auth.authorize(&token, Scope::StatusRead)?;
            json!({"probe":true})
        }
        OpenPairing { token } => {
            auth.open_pairing(&token)?;
            json!({"ok":true,"expires_in_seconds":300})
        }
        PairChallenge {
            public_key,
            kind,
            scopes,
        } => serde_json::to_value(auth.pairing_challenge(public_key, kind, &scopes)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        PairProof {
            challenge_id,
            signature,
        } => json!({"pairing_id":auth.pairing_finish(&challenge_id,&signature)?}),
        Approve {
            token,
            pairing_id,
            public_key,
            kind,
            scopes,
        } => serde_json::to_value(auth.approve(&token, &pairing_id, public_key, kind, &scopes)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        Revoke { token, public_key } => {
            auth.revoke(&token, public_key)?;
            json!({"ok":true})
        }
    })
}
pub(crate) async fn handle(stream: &mut UnixStream, auth: Arc<Mutex<Auth>>) -> Result<()> {
    let body = timeout(Duration::from_secs(5), read_frame(stream)).await??;
    let request = parse_auth_request(&body)?;
    let capabilities = matches!(request.command, AuthCommand::Capabilities { .. });
    let result = tokio::task::spawn_blocking(move || match auth.try_lock() {
        Ok(mut a) => dispatch(&mut a, request.command),
        Err(_) => Err(rustsimhub_auth::Error::Busy),
    })
    .await?;
    let result = match result {
        Ok(_) if capabilities => match probe().await {
            Ok(p) => serde_json::to_value(p)?,
            Err(_) => json!({"ok":false,"error":"modem_unavailable"}),
        },
        Ok(value) => value,
        Err(_) => json!({"ok":false,"error":"denied"}),
    };
    let response = serde_json::to_vec(&Response {
        protocol: AUTH_PROTOCOL,
        id: request.id,
        result,
    })?;
    timeout(Duration::from_secs(5), write_frame(stream, &response)).await??;
    Ok(())
}
pub async fn serve(socket: &str, uid: u32, state: &str, identity: &str) -> Result<()> {
    let (state, identity) = (state.to_owned(), identity.to_owned());
    let (store, auth) = tokio::task::spawn_blocking(move || -> rustsimhub_auth::Result<_> {
        let store = Store::open(state)?;
        let auth = Auth::new(Identity::open(identity)?, store.handle())?;
        Ok((store, Arc::new(Mutex::new(auth))))
    })
    .await??;
    let listener: UnixListener = crate::linux::bind_listener(socket)?;
    let limit = Arc::new(Semaphore::new(8));
    let _store = store; // Sole DB writer stays alive for the entire daemon lifetime.
    loop {
        let (mut stream, _) = listener.accept().await?;
        if stream.peer_cred()?.uid() != uid {
            continue;
        }
        let permit = match limit.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let auth = auth.clone();
        tokio::spawn(async move {
            let _ = timeout(Duration::from_secs(15), handle(&mut stream, auth)).await;
            drop(permit);
        });
    }
}
/// Explicit offline terminal provisioning. No bootstrap operation is accepted over IPC.
pub fn provision(state: &str, identity: &str, key: &str) -> Result<()> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid public key".into());
    }
    let mut public = [0u8; 32];
    for (index, b) in public.iter_mut().enumerate() {
        *b = u8::from_str_radix(&key[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid public key")?;
    }
    let store = Store::open(state)?;
    let mut auth = Auth::new(Identity::open(identity)?, store.handle())?;
    let challenge = auth.bootstrap_challenge(public)?;
    println!("{}", serde_json::to_string(&challenge)?);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().take(1025).read_to_string(&mut input)?;
    if input.len() > 1024 {
        return Err("proof too large".into());
    }
    let signature: Vec<u8> = serde_json::from_str(&input).map_err(|_| "invalid proof")?;
    auth.bootstrap_finish(&challenge.id, &signature)?;
    println!("{}", json!({"ok":true,"owner_initialized":true}));
    Ok(())
}
/// Request JSON comes from bounded stdin; tokens are never command-line arguments.
pub async fn request(socket: &str) -> Result<()> {
    let mut body = Vec::new();
    std::io::stdin()
        .take((MAX_FRAME + 1) as u64)
        .read_to_end(&mut body)?;
    let req = parse_auth_request(&body)?;
    let mut stream = timeout(Duration::from_secs(5), UnixStream::connect(socket)).await??;
    timeout(Duration::from_secs(5), write_frame(&mut stream, &body)).await??;
    let result = timeout(Duration::from_secs(15), read_frame(&mut stream)).await??;
    let response: Response = serde_json::from_slice(&result)?;
    if response.protocol != AUTH_PROTOCOL || response.id != req.id {
        return Err("unexpected response".into());
    }
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
}

#[cfg(test)]
mod tests;
