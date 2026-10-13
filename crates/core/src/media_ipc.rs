// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_media::{
    protocol::{self, Command, Credentials, Reply},
    Authority, Context, Permit, Ticket,
};
use rustsimhub_policy::Principal;
use serde_json::{json, Value};
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::oneshot,
    time::timeout,
};
pub(crate) enum Action {
    Redeem {
        ticket: [u8; 32],
        peer_key: [u8; 32],
    },
    Pulse,
    Disconnect,
}
pub(crate) struct Job {
    pub connection: u64,
    pub action: Action,
    pub reply: oneshot::Sender<Result<Value, &'static str>>,
}
pub(crate) struct State {
    authority: Authority,
    pub owner: Option<Principal>,
    pub peer_key: [u8; 32],
    permit: Option<Permit>,
    connection: Option<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            authority: Authority::new(),
            owner: None,
            peer_key: [0; 32],
            permit: None,
            connection: None,
        }
    }
}
impl State {
    pub fn expire(&mut self, now: u64) {
        if self.permit.as_ref().is_some_and(|p| p.check(now).is_err()) {
            self.revoke();
        }
    }
    pub fn issue(
        &mut self,
        p: Principal,
        context: Context,
        now: u64,
    ) -> Result<Credentials, &'static str> {
        let ticket = self
            .authority
            .issue(context.clone(), now)
            .map_err(|_| "media_denied")?;
        self.owner = Some(p);
        self.peer_key = context.peer_key;
        Ok(Credentials {
            ticket: ticket.into_bytes(),
            peer_key: context.peer_key,
            boot: context.boot,
        })
    }
    pub fn redeem(
        &mut self,
        connection: u64,
        ticket: [u8; 32],
        peer_key: [u8; 32],
        context: &Context,
        now: u64,
    ) -> Result<Value, &'static str> {
        if self.connection.is_some() {
            return Err("media_busy");
        }
        let permit = self
            .authority
            .consume(Ticket::from_bytes(ticket), context, &peer_key, now)
            .map_err(|_| "media_denied")?;
        self.connection = Some(connection);
        self.permit = Some(permit);
        self.receipt(context, now)
    }
    pub fn pulse(
        &mut self,
        connection: u64,
        context: &Context,
        now: u64,
    ) -> Result<Value, &'static str> {
        if self.connection != Some(connection) {
            return Err("media_denied");
        }
        if self.authority.refresh(Some(context), now).is_err() {
            self.revoke();
            return Err("media_denied");
        }
        self.receipt(context, now)
    }
    fn receipt(&self, context: &Context, now: u64) -> Result<Value, &'static str> {
        let until = self
            .permit
            .as_ref()
            .ok_or("media_denied")?
            .deadline_ms(now)
            .map_err(|_| "media_denied")?;
        Ok(json!({"boot":context.boot,"until_ms":until}))
    }
    pub fn disconnected(&mut self, connection: u64) {
        if self.connection == Some(connection) {
            self.revoke();
        }
    }
    pub fn revoke(&mut self) {
        self.authority.revoke();
        self.permit = None;
        self.connection = None;
        self.owner = None;
    }
}
pub(crate) fn bind(
    path: &str,
) -> Result<(UnixListener, crate::socket_runtime::SocketRuntime), Box<dyn std::error::Error>> {
    crate::socket_runtime::SocketRuntime::bind(Path::new(path))
}
pub(crate) async fn serve(
    listener: UnixListener,
    uid: u32,
    controller: crate::calls::Controller,
) -> Result<(), &'static str> {
    let mut tasks = tokio::task::JoinSet::new();
    let mut next = 0u64;
    loop {
        let accepted = tokio::select! {_ = tasks.join_next(),if !tasks.is_empty()=>continue, v=listener.accept()=>v};
        let (stream, _) = accepted.map_err(|_| "media_listener_failed")?;
        if stream.peer_cred().map(|p| p.uid()).ok() != Some(uid) || tasks.len() >= 2 {
            continue;
        }
        next = next.checked_add(1).ok_or("media_connections_exhausted")?;
        let c = controller.clone();
        let connection = next;
        tasks.spawn(async move {
            let _ = handle(stream, &c, connection).await;
            let _ = c.media(connection, Action::Disconnect).await;
        });
    }
}
async fn handle(
    mut stream: UnixStream,
    c: &crate::calls::Controller,
    connection: u64,
) -> Result<(), &'static str> {
    let mut previous = 0u64;
    loop {
        let bytes = timeout(
            Duration::from_millis(if previous == 0 { 5_000 } else { 750 }),
            async {
                let len = stream.read_u32().await? as usize;
                if len == 0 || len > protocol::MAX_FRAME {
                    return Err(std::io::Error::other("media frame too large"));
                }
                let mut bytes = vec![0; len];
                stream.read_exact(&mut bytes).await?;
                Ok(bytes)
            },
        )
        .await
        .map_err(|_| "media_timeout")?
        .map_err(|_| "media_closed")?;
        let request = protocol::parse(&bytes).map_err(|_| "media_denied")?;
        if Some(request.id) != previous.checked_add(1) {
            return Err("media_order");
        }
        let action = match request.command {
            Command::Redeem { ticket, peer_key } if previous == 0 => {
                Action::Redeem { ticket, peer_key }
            }
            Command::Pulse {} if previous > 0 => Action::Pulse,
            _ => return Err("media_order"),
        };
        let value = c.media(connection, action).await?;
        let reply = Reply {
            protocol: protocol::PROTOCOL,
            id: request.id,
            boot: value["boot"].as_str().ok_or("media_denied")?.into(),
            until_ms: value["until_ms"].as_u64().ok_or("media_denied")?,
        };
        let bytes = serde_json::to_vec(&reply).map_err(|_| "media_denied")?;
        timeout(Duration::from_millis(250), async {
            stream.write_u32(bytes.len() as u32).await?;
            stream.write_all(&bytes).await
        })
        .await
        .map_err(|_| "media_timeout")?
        .map_err(|_| "media_closed")?;
        previous = request.id;
    }
}
