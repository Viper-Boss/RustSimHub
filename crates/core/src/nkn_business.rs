// SPDX-License-Identifier: AGPL-3.0-only
//! Finite candidate bridge to ORIGINAL normal Core and media dispatchers. The
//! host runs the synchronous router on an owned dedicated worker, retaining its
//! accepted source descriptor until all work drains. No listener or startup.
use rustsimhub_auth::{
    nkn_session::{NknBinding, NknScope},
    Auth,
};
use rustsimhub_nkn_agent::routing::{BusinessOutcome, BusinessPort, CallBudget, Service};
use rustsimhub_wire::AuthCommand;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    runtime::Handle,
};

#[derive(Clone)]
pub struct CorePorts {
    pub(crate) auth: Arc<Mutex<Auth>>,
    pub(crate) metrics: Option<crate::observer::Metrics>,
    pub(crate) sender: Option<crate::sender::Controller>,
    pub(crate) calls: Option<crate::calls::Controller>,
    pub(crate) providers: crate::authenticated::Providers,
}
impl CorePorts {
    pub fn new(auth: Arc<Mutex<Auth>>) -> Self {
        Self {
            auth,
            metrics: None,
            sender: None,
            calls: None,
            providers: Default::default(),
        }
    }
}
static NEXT_MEDIA: AtomicU64 = AtomicU64::new(1u64 << 63);
pub struct NknBusinessPort {
    ports: CorePorts,
    scope: NknScope,
    runtime: Handle,
    media_id: u64,
    media_previous: u64,
}
impl Drop for NknBusinessPort {
    fn drop(&mut self) {
        self.scope.invalidate();
        if let Some(calls) = &self.ports.calls {
            calls.nkn_media_disconnected(self.media_id);
        }
    }
}
impl NknBusinessPort {
    /// The supplied scope is an opaque output of real Noise + membership +
    /// registered identity checks; no wire constructor or boolean admission.
    pub fn new(ports: CorePorts, scope: NknScope, runtime: Handle) -> Result<Self, &'static str> {
        ports
            .auth
            .lock()
            .map_err(|_| "auth_busy")?
            .check_nkn_scope(&scope)
            .map_err(|_| "denied")?;
        let media_id = NEXT_MEDIA
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
            .map_err(|_| "media_connections_exhausted")?;
        Ok(Self {
            ports,
            scope,
            runtime,
            media_id,
            media_previous: 0,
        })
    }
    fn check(&self) -> Result<(), BusinessOutcome> {
        self.ports
            .auth
            .lock()
            .map_err(|_| BusinessOutcome::Unavailable)?
            .check_nkn_scope(&self.scope)
            .map_err(|_| BusinessOutcome::Denied)
    }
    async fn core_request(&mut self, body: &[u8]) -> Result<Vec<u8>, BusinessOutcome> {
        let request =
            rustsimhub_wire::parse_auth_request(body).map_err(|_| BusinessOutcome::Denied)?;
        match &request.command {
            AuthCommand::Challenge { .. }
            | AuthCommand::Authenticate { .. }
            | AuthCommand::SessionChallenge { .. }
            | AuthCommand::RenewSession { .. } => return Err(BusinessOutcome::Denied),
            AuthCommand::Identity {} => {}
            _ => {
                let command =
                    serde_json::to_value(&request.command).map_err(|_| BusinessOutcome::Denied)?;
                let token = command["token"].as_str().ok_or(BusinessOutcome::Denied)?;
                self.ports
                    .auth
                    .lock()
                    .map_err(|_| BusinessOutcome::Unavailable)?
                    .check_nkn_token(&self.scope, token)
                    .map_err(|_| BusinessOutcome::Denied)?;
            }
        }
        // Same Core process, original parser, original dispatch/actors and
        // original request bytes/UUIDs. This pair is not an external socket or
        // a route to another host. Original business authorization remains live.
        let (mut local, mut application) =
            UnixStream::pair().map_err(|_| BusinessOutcome::Unavailable)?;
        let exchange = async {
            local
                .write_u32(body.len() as u32)
                .await
                .map_err(|_| BusinessOutcome::Unknown)?;
            local
                .write_all(body)
                .await
                .map_err(|_| BusinessOutcome::Unknown)?;
            let len = local
                .read_u32()
                .await
                .map_err(|_| BusinessOutcome::Unknown)?;
            if len == 0 || len > 1024 {
                return Err(BusinessOutcome::Unknown);
            }
            let mut reply = vec![0; len as usize];
            local
                .read_exact(&mut reply)
                .await
                .map_err(|_| BusinessOutcome::Unknown)?;
            Ok(reply)
        };
        let handler = crate::authenticated::handle(
            &mut application,
            self.ports.auth.clone(),
            self.ports.metrics.clone(),
            self.ports.sender.clone(),
            self.ports.calls.clone(),
            self.ports.providers.clone(),
        );
        let (reply, handled) = tokio::join!(exchange, handler);
        handled.map_err(|_| BusinessOutcome::Unknown)?;
        reply
    }
    async fn media_request(
        &mut self,
        body: &[u8],
        deadline: Instant,
    ) -> Result<Vec<u8>, BusinessOutcome> {
        use rustsimhub_media::protocol::{self, Command};
        let request = protocol::parse(body).map_err(|_| BusinessOutcome::Denied)?;
        if self.media_previous.checked_add(1) != Some(request.id) {
            return Err(BusinessOutcome::Denied);
        }
        let action = match request.command {
            Command::Redeem { ticket, peer_key } if self.media_previous == 0 => {
                crate::media_ipc::Action::Redeem { ticket, peer_key }
            }
            Command::Pulse {} if self.media_previous > 0 => crate::media_ipc::Action::Pulse,
            _ => return Err(BusinessOutcome::Denied),
        };
        let calls = self
            .ports
            .calls
            .as_ref()
            .ok_or(BusinessOutcome::Unavailable)?;
        let result = calls
            .nkn_media(self.media_id, action, self.scope.clone(), deadline)
            .await
            .map_err(|_| BusinessOutcome::Unknown)?;
        self.media_previous = request.id;
        let reply = protocol::Reply {
            protocol: protocol::PROTOCOL,
            id: request.id,
            boot: result["boot"]
                .as_str()
                .ok_or(BusinessOutcome::Unknown)?
                .into(),
            until_ms: result["until_ms"]
                .as_u64()
                .ok_or(BusinessOutcome::Unknown)?,
        };
        serde_json::to_vec(&reply).map_err(|_| BusinessOutcome::Unknown)
    }
    fn execute(
        &mut self,
        binding: &NknBinding,
        body: &[u8],
        budget: &CallBudget,
        service: Service,
    ) -> BusinessOutcome {
        if binding != self.scope.binding() || body.len() > 1024 || budget.cancelled() {
            return BusinessOutcome::Denied;
        }
        if let Err(e) = self.check() {
            return e;
        }
        let runtime = self.runtime.clone();
        let deadline = budget.deadline().min(self.scope.deadline());
        if Instant::now() >= deadline {
            return BusinessOutcome::Denied;
        }
        // Must be called on the owning dedicated router worker, never inside a
        // Tokio reactor task. No additional business queue, retry or detached job.
        let result = runtime.block_on(async {
            let cancelled = async {
                while !budget.cancelled() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            };
            let work = async {
                match service {
                    Service::CoreAuthV2 => self.core_request(body).await,
                    Service::CallMediaV1 => self.media_request(body, deadline).await,
                }
            };
            tokio::select! {biased;
                _=cancelled=>Err(BusinessOutcome::Unknown),
                _=tokio::time::sleep_until(deadline.into())=>Err(BusinessOutcome::Unknown),
                result=work=>result,
            }
        });
        match result {
            Ok(bytes)
                if !budget.cancelled() && Instant::now() < deadline && self.check().is_ok() =>
            {
                BusinessOutcome::Reply(bytes)
            }
            Ok(_) | Err(BusinessOutcome::Unknown) => {
                self.scope.invalidate();
                BusinessOutcome::Unknown
            }
            Err(e) => e,
        }
    }
}
impl BusinessPort<NknBinding> for NknBusinessPort {
    fn core_auth(
        &mut self,
        binding: &NknBinding,
        body: &[u8],
        budget: &CallBudget,
    ) -> BusinessOutcome {
        self.execute(binding, body, budget, Service::CoreAuthV2)
    }
    fn call_media(
        &mut self,
        binding: &NknBinding,
        body: &[u8],
        budget: &CallBudget,
    ) -> BusinessOutcome {
        self.execute(binding, body, budget, Service::CallMediaV1)
    }
}
