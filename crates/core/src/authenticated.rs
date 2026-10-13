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
use tokio::{net::UnixStream, sync::Semaphore, time::timeout};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) fn dispatch(auth: &mut Auth, command: AuthCommand) -> rustsimhub_auth::Result<Value> {
    use AuthCommand::*;
    Ok(match command {
        PrepareSms { .. }
        | SubmitSms { .. }
        | SmsOperation { .. }
        | CallStatus { .. }
        | PrepareCall { .. }
        | SubmitCall { .. }
        | CallOperation { .. }
        | RenewCall { .. }
        | IssueMedia { .. } => return Err(rustsimhub_auth::Error::Forbidden),
        WifiTrial { .. } | NetworkStatus { .. } => return Err(rustsimhub_auth::Error::Forbidden),
        Calls {
            token,
            after,
            limit,
        } => serde_json::to_value(auth.calls(&token, after, limit)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        CallDetail { token, call_id } => serde_json::to_value(auth.call_detail(&token, call_id)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        CallEvents {
            token,
            after,
            limit,
        } => serde_json::to_value(auth.call_events(&token, after, limit)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
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
        LineStatus { token } => serde_json::to_value(auth.line_status(&token)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        Inbox {
            token,
            after,
            limit,
        } => serde_json::to_value(auth.inbox(&token, after, limit)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        SmsDetail { token, sms_id } => serde_json::to_value(auth.sms_detail(&token, sms_id)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        InboxEvents {
            token,
            after,
            limit,
        } => serde_json::to_value(auth.inbox_events(&token, after, limit)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        OpenPairing { token } => {
            auth.open_pairing(&token)?;
            json!({"ok":true,"expires_in_seconds":300})
        }
        PendingPairings { token } => serde_json::to_value(auth.pending_pairings(&token)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
        Clients { token } => serde_json::to_value(auth.clients(&token)?)
            .map_err(|_| rustsimhub_auth::Error::InvalidInput)?,
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
pub(crate) async fn handle(
    stream: &mut UnixStream,
    auth: Arc<Mutex<Auth>>,
    metrics: Option<crate::observer::Metrics>,
    sender: Option<crate::sender::Controller>,
    calls: Option<crate::calls::Controller>,
) -> Result<()> {
    let body = timeout(Duration::from_secs(5), read_frame(stream)).await??;
    let request = parse_auth_request(&body)?;
    let calling_enabled = calls.as_ref().is_some_and(|c| c.controls());
    let calls_observed = calls.is_some();
    let messaging_enabled = sender.is_some();
    let health = matches!(request.command, AuthCommand::Health { .. });
    let capabilities = matches!(request.command, AuthCommand::Capabilities { .. });
    let line_status = matches!(request.command, AuthCommand::LineStatus { .. });
    let sending = matches!(
        request.command,
        AuthCommand::PrepareSms { .. }
            | AuthCommand::SubmitSms { .. }
            | AuthCommand::SmsOperation { .. }
    );
    let calling = matches!(
        request.command,
        AuthCommand::CallStatus { .. }
            | AuthCommand::PrepareCall { .. }
            | AuthCommand::SubmitCall { .. }
            | AuthCommand::CallOperation { .. }
            | AuthCommand::RenewCall { .. }
            | AuthCommand::IssueMedia { .. }
    );
    let result = if let AuthCommand::NetworkStatus { token } = request.command {
        crate::network::authorized(auth.clone(), token).await
    } else if let AuthCommand::WifiTrial {
        token,
        idempotency_key,
    } = request.command
    {
        crate::network_trial::authorized(auth.clone(), token, idempotency_key).await
    } else if calling {
        match calls {
            Some(c) => c
                .request(request.command)
                .await
                .map_err(|_| rustsimhub_auth::Error::Forbidden),
            None => Err(rustsimhub_auth::Error::Forbidden),
        }
    } else if sending {
        match sender {
            Some(s) => s
                .request(request.command)
                .await
                .map_err(|_| rustsimhub_auth::Error::Forbidden),
            None => Err(rustsimhub_auth::Error::Forbidden),
        }
    } else {
        tokio::task::spawn_blocking(move || match auth.try_lock() {
            Ok(mut a) => dispatch(&mut a, request.command),
            Err(_) => Err(rustsimhub_auth::Error::Busy),
        })
        .await?
    };
    let result = match result {
        Ok(_) if capabilities => match probe().await {
            Ok(p) => {
                let mut value = serde_json::to_value(p)?;
                value["messaging_api"] = messaging_enabled.into();
                value["call_control_api"] = calling_enabled.into();
                value["call_observer"] = calls_observed.into();
                value
            }
            Err(_) => json!({"ok":false,"error":"modem_unavailable"}),
        },
        Ok(mut value) => {
            if health && calling_enabled {
                value = json!({"ok":true,"mode":"authenticated_telephony","write_api":true,"media_available":false});
            } else if health && messaging_enabled {
                value = json!({"ok":true,"mode":"authenticated_messaging","write_api":true});
            }
            if let (true, Some(m)) = (line_status, metrics) {
                if let Ok(s) = m.lock() {
                    value["observer"] = serde_json::to_value(s.clone())?;
                }
            }
            value
        }
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
    serve_mode(socket, uid, state, identity, (false, false, 0)).await
}
pub async fn serve_media(
    socket: &str,
    uid: u32,
    state: &str,
    identity: &str,
    media_socket: &str,
    media_uid: u32,
) -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    serve_inner_media(
        socket,
        uid,
        state,
        identity,
        (true, true, 2),
        Some((media_socket, media_uid)),
        async move {
            tokio::select! {_=term.recv()=>{},_=interrupt.recv()=>{}}
        },
    )
    .await
}
pub async fn serve_observing(socket: &str, uid: u32, state: &str, identity: &str) -> Result<()> {
    serve_mode(socket, uid, state, identity, (true, false, 0)).await
}
pub async fn serve_messaging(socket: &str, uid: u32, state: &str, identity: &str) -> Result<()> {
    serve_mode(socket, uid, state, identity, (true, true, 0)).await
}
pub async fn serve_calls(
    socket: &str,
    uid: u32,
    state: &str,
    identity: &str,
    writes: bool,
) -> Result<()> {
    serve_mode(
        socket,
        uid,
        state,
        identity,
        (true, writes, if writes { 2 } else { 1 }),
    )
    .await
}
async fn serve_mode(
    socket: &str,
    uid: u32,
    state: &str,
    identity: &str,
    (observing, messaging, call_mode): (bool, bool, u8),
) -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    serve_inner(
        socket,
        uid,
        state,
        identity,
        (observing, messaging, call_mode),
        async move {
            tokio::select! { _=term.recv()=>{},_=interrupt.recv()=>{} }
        },
    )
    .await
}
#[cfg(test)]
pub(crate) async fn serve_until(
    socket: &str,
    uid: u32,
    state: &str,
    identity: &str,
    stop: impl std::future::Future<Output = ()>,
) -> Result<()> {
    serve_inner(socket, uid, state, identity, (false, false, 0), stop).await
}
async fn serve_inner(
    socket: &str,
    uid: u32,
    state: &str,
    identity: &str,
    (observing, messaging, call_mode): (bool, bool, u8),
    stop: impl std::future::Future<Output = ()>,
) -> Result<()> {
    serve_inner_media(
        socket,
        uid,
        state,
        identity,
        (observing, messaging, call_mode),
        None,
        stop,
    )
    .await
}
async fn serve_inner_media(
    socket: &str,
    uid: u32,
    state: &str,
    identity: &str,
    (observing, messaging, call_mode): (bool, bool, u8),
    media_config: Option<(&str, u32)>,
    stop: impl std::future::Future<Output = ()>,
) -> Result<()> {
    let (state, identity) = (state.to_owned(), identity.to_owned());
    let (store, auth) = tokio::task::spawn_blocking(move || -> rustsimhub_auth::Result<_> {
        let store = Store::open(state)?;
        let auth = Auth::open(identity, store.handle())?;
        Ok((store, Arc::new(Mutex::new(auth))))
    })
    .await??;
    let (listener, runtime) =
        crate::socket_runtime::SocketRuntime::bind(std::path::Path::new(socket))?;
    let limit = Arc::new(Semaphore::new(8));
    let observer_store = store.handle();
    let metrics = Arc::new(Mutex::new(crate::observer::Stats::default()));
    let observer_metrics = metrics.clone();
    let (publisher, bindings) = tokio::sync::watch::channel(None);
    let boot = auth
        .try_lock()
        .map_err(|_| "startup auth busy")?
        .observer_instance();
    let (call_controller, mut call_worker) = crate::calls::channel(
        auth.clone(),
        store.handle(),
        bindings.clone(),
        boot,
        call_mode == 2,
    );
    call_worker.media_enabled = media_config.is_some();
    let (media_listener, media_runtime, media_uid) = match media_config {
        Some((path, uid)) => {
            let (listener, runtime) = crate::media_ipc::bind(path)?;
            (Some(listener), Some(runtime), uid)
        }
        None => (None, None, 0),
    };
    let media_controller = call_controller.clone();
    let mut media_serving = Box::pin(async move {
        match media_listener {
            Some(l) => crate::media_ipc::serve(l, media_uid, media_controller).await,
            None => std::future::pending().await,
        }
    });
    let calls = (call_mode > 0).then_some(call_controller);
    let mut controlling = Box::pin(async move {
        if call_mode > 0 {
            call_worker.run().await
        } else {
            std::future::pending::<()>().await
        }
    });
    let (controller, worker) = crate::sender::channel(auth.clone(), store.handle(), bindings);
    let sender = messaging.then_some(controller);
    let mut sending = Box::pin(async move {
        if messaging {
            worker.run().await
        } else {
            std::future::pending::<()>().await
        }
    });
    let observer_publisher = (messaging || call_mode > 0).then_some(publisher);
    let observer_context = if observing {
        let a = auth.try_lock().map_err(|_| "auth busy during startup")?;
        Some((a.observer_instance(), a.sim_fingerprinter()?))
    } else {
        None
    };
    let mut observer = Box::pin(async move {
        match observer_context {
            Some((instance, key)) => {
                crate::observer::run(
                    observer_store,
                    instance,
                    key,
                    observer_metrics,
                    observer_publisher,
                )
                .await
            }
            None => std::future::pending::<()>().await,
        }
    });
    let mut tasks = tokio::task::JoinSet::new();
    tokio::pin!(stop);
    let result: Result<()> = loop {
        let accepted = tokio::select! {
            biased;
            _=&mut stop=>break Ok(()),
            _=&mut observer=>break Err("observer stopped".into()),
            _=&mut sending=>break Err("sender stopped".into()),
            _=&mut controlling=>break Err("call controller stopped".into()),
            _=&mut media_serving=>break Err("media authorizer stopped".into()),
            _=tasks.join_next(),if !tasks.is_empty()=>continue,
            accepted=listener.accept()=>accepted,
        };
        let (mut stream, _) = match accepted {
            Ok(s) => s,
            Err(e) => break Err(e.into()),
        };
        if stream.peer_cred().map(|c| c.uid()).ok() != Some(uid) {
            continue;
        }
        if tasks.len() >= 8 {
            continue;
        }
        let permit = match limit.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let auth = auth.clone();
        let metrics = metrics.clone();
        let sender = sender.clone();
        let calls = calls.clone();
        tasks.spawn(async move {
            let _ = timeout(
                Duration::from_secs(15),
                handle(&mut stream, auth, Some(metrics), sender, calls),
            )
            .await;
            drop(permit);
        });
    };
    drop(listener); // Stop admission before draining outstanding work.
    drop(media_serving);
    if let Some(c) = &calls {
        c.shutdown();
        let _ = timeout(Duration::from_secs(15), &mut controlling).await;
    }
    drop(observer);
    drop(sending);
    drop(controlling);
    drop(calls);
    drop(sender);
    if timeout(Duration::from_secs(16), async {
        while tasks.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
    // Outstanding blocking handlers may finish; Store processes queued jobs before closing.
    drop(auth);
    tokio::task::spawn_blocking(move || drop(store)).await?;
    drop(runtime); // Unlink only our inode, after the single writer has closed.
    drop(media_runtime);
    result
}
/// Explicit offline terminal provisioning. No bootstrap operation is accepted over IPC.
pub fn provision(state: &str, identity: &str, key: &str) -> Result<()> {
    offline_owner(state, identity, key, false)
}
pub fn recover_owner(state: &str, identity: &str, key: &str) -> Result<()> {
    offline_owner(state, identity, key, true)
}
fn offline_owner(state: &str, identity: &str, key: &str, recover: bool) -> Result<()> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid public key".into());
    }
    let mut public = [0u8; 32];
    for (index, b) in public.iter_mut().enumerate() {
        *b = u8::from_str_radix(&key[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid public key")?;
    }
    let store = Store::open(state)?;
    let mut auth = Auth::open(identity, store.handle())?;
    let challenge = if recover {
        auth.recovery_challenge(public)?
    } else {
        auth.bootstrap_challenge(public)?
    };
    println!("{}", serde_json::to_string(&challenge)?);
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().take(1025).read_to_string(&mut input)?;
    if input.len() > 1024 {
        return Err("proof too large".into());
    }
    let signature: Vec<u8> = serde_json::from_str(&input).map_err(|_| "invalid proof")?;
    if recover {
        auth.recovery_finish(&challenge.id, &signature)?;
    } else {
        auth.bootstrap_finish(&challenge.id, &signature)?;
    }
    println!(
        "{}",
        json!({"ok":true,"owner_initialized":!recover,"owner_recovered":recover})
    );
    Ok(())
}
pub fn backup_identity(identity: &str, backup: &str) -> Result<()> {
    Identity::load(identity)?.backup(backup)?;
    println!("{}", json!({"ok":true,"private_identity_backup":true}));
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
