// SPDX-License-Identifier: AGPL-3.0-only
//! Bounded call controller; all hardware mutations require a committed single-use intent.
use crate::observer::Binding;
use rustsimhub_auth::Auth;
use rustsimhub_domain::{CallAction, CallGrant, CallLease, CallLeases, LineEpoch, SmsState};
use rustsimhub_policy::{Principal, Scope};
use rustsimhub_storage::{CallObservation, CallRequest, CallSummary, Handle};
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
type Result<T> = std::result::Result<T, &'static str>;
#[zbus::proxy(interface = "org.freedesktop.ModemManager1.Modem.Voice")]
trait Voice {
    #[zbus(property)]
    fn emergency_only(&self) -> zbus::Result<bool>;
    fn list_calls(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn create_call(&self, properties: HashMap<String, OwnedValue>)
        -> zbus::Result<OwnedObjectPath>;
}
#[zbus::proxy(interface = "org.freedesktop.ModemManager1.Call")]
trait Call {
    fn start(&self) -> zbus::Result<()>;
    fn accept(&self) -> zbus::Result<()>;
    fn hangup(&self) -> zbus::Result<()>;
    fn send_dtmf(&self, dtmf: &str) -> zbus::Result<()>;
}
struct Job {
    command: AuthCommand,
    reply: oneshot::Sender<Result<Value>>,
}
#[derive(Clone)]
pub(crate) struct Controller {
    normal: mpsc::Sender<Job>,
    urgent: mpsc::Sender<Job>,
    stop: watch::Sender<bool>,
    controls: bool,
    media_jobs: mpsc::Sender<crate::media_ipc::Job>,
}
struct Live {
    summary: CallSummary,
    path: OwnedObjectPath,
    revision: u64,
    number: String,
}
struct Owned {
    principal: Principal,
    grant: CallGrant,
    call_id: String,
    origin: String,
    cleanup_attempted: bool,
}
pub(crate) struct Worker {
    normal: mpsc::Receiver<Job>,
    urgent: mpsc::Receiver<Job>,
    stop: watch::Receiver<bool>,
    auth: Arc<Mutex<Auth>>,
    store: Handle,
    bindings: watch::Receiver<Option<Binding>>,
    writes: bool,
    boot: String,
    epoch: LineEpoch,
    book: CallLeases,
    owned: Option<Owned>,
    live: HashMap<String, Live>,
    backend: Option<String>,
    catalogue: bool,
    media_jobs: mpsc::Receiver<crate::media_ipc::Job>,
    media: crate::media_ipc::State,
    pub(crate) media_enabled: bool,
    coordination: Arc<crate::coordination::Coordination>,
    #[cfg(test)]
    advance_ms: u64,
}
pub(crate) fn channel(
    auth: Arc<Mutex<Auth>>,
    store: Handle,
    bindings: watch::Receiver<Option<Binding>>,
    boot: String,
    writes: bool,
) -> (Controller, Worker) {
    let (n, nr) = mpsc::channel(8);
    let (u, ur) = mpsc::channel(2);
    let (stop_tx, stop_rx) = watch::channel(false);
    let (media_tx, media_rx) = mpsc::channel(4);
    let book = CallLeases::new(boot.clone(), LineEpoch(1));
    (
        Controller {
            normal: n,
            urgent: u,
            stop: stop_tx,
            controls: writes,
            media_jobs: media_tx,
        },
        Worker {
            normal: nr,
            urgent: ur,
            stop: stop_rx,
            auth,
            store,
            bindings,
            writes,
            boot,
            epoch: LineEpoch(1),
            book,
            owned: None,
            live: HashMap::new(),
            backend: None,
            catalogue: false,
            media_jobs: media_rx,
            media: crate::media_ipc::State::default(),
            media_enabled: false,
            coordination: crate::coordination::global(),
            #[cfg(test)]
            advance_ms: 0,
        },
    )
}
impl Controller {
    pub(crate) async fn media(
        &self,
        connection: u64,
        action: crate::media_ipc::Action,
    ) -> Result<Value> {
        let (reply, rx) = oneshot::channel();
        self.media_jobs
            .try_send(crate::media_ipc::Job {
                connection,
                action,
                reply,
            })
            .map_err(|_| "media_busy")?;
        tokio::time::timeout(Duration::from_millis(450), rx)
            .await
            .map_err(|_| "media_timeout")?
            .map_err(|_| "media_closed")?
    }
    pub fn controls(&self) -> bool {
        self.controls
    }
    pub fn shutdown(&self) {
        self.stop.send_replace(true);
    }

    pub async fn request(&self, command: AuthCommand) -> Result<Value> {
        let (reply, rx) = oneshot::channel();
        let urgent = matches!(&command, AuthCommand::SubmitCall { urgent: true, .. });
        (if urgent { &self.urgent } else { &self.normal })
            .try_send(Job { command, reply })
            .map_err(|_| "call_controller_busy")?;
        tokio::time::timeout(Duration::from_secs(8), rx)
            .await
            .map_err(|_| "pending_query_operation")?
            .map_err(|_| "controller_stopped")?
    }
}
async fn db<T: Send + 'static>(
    f: impl FnOnce() -> std::result::Result<T, rustsimhub_storage::Error> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|_| "storage_unavailable")?
        .map_err(|_| "storage_denied")
}
async fn bounded<T, E>(
    f: impl std::future::Future<Output = std::result::Result<T, E>>,
) -> Result<T> {
    tokio::time::timeout(Duration::from_secs(5), f)
        .await
        .map_err(|_| "modem_timeout")?
        .map_err(|_| "modem_unavailable")
}
impl Worker {
    async fn mutation_guard(
        &self,
        action: CallAction,
    ) -> Result<Option<tokio::sync::OwnedRwLockReadGuard<()>>> {
        let permit = self.coordination.telephony(action)?;
        if permit.is_some() && crate::network_trial::enabled() {
            // A previous Core may have disappeared while NM still owns a finite rollback.
            tokio::time::timeout(
                Duration::from_secs(3),
                crate::network_trial::no_pending_checkpoint(),
            )
            .await
            .map_err(|_| "network_state_unknown")?
            .map_err(|_| "network_checkpoint_pending")?;
        }
        Ok(permit)
    }
    async fn media_context(
        &self,
        p: &Principal,
        peer_key: [u8; 32],
    ) -> Result<rustsimhub_media::Context> {
        if !self.writes || !self.media_enabled {
            return Err("media_disabled");
        }
        let o = self.owned.as_ref().ok_or("call_unowned")?;
        if &o.principal != p
            || o.cleanup_attempted
            || !rustsimhub_policy::allows(p.scopes, Scope::MediaUse)
            || !rustsimhub_policy::allows(p.scopes, Scope::CallsControl)
        {
            return Err("media_denied");
        }
        self.book
            .validate(&o.grant, &p.id, self.now())
            .map_err(|_| "lease_expired")?;
        let b = self.binding()?;
        let l = self
            .live
            .values()
            .find(|l| l.summary.id == o.call_id)
            .ok_or("call_unavailable")?;
        if b.context.line != o.grant.line
            || self.backend.as_deref() != Some(&b.context.backend)
            || !b.call_valid(l.path.as_str(), l.revision)
            || !b.fresh().await
        {
            return Err("line_unavailable");
        }
        let data = read_call(&b, &l.path).await?;
        if data.state != 4
            || data.number != l.number
            || data.direction != l.summary.direction
            || !b.call_valid(l.path.as_str(), l.revision)
        {
            return Err("call_inactive");
        }
        let (h, key, expected) = (self.store.clone(), p.public_key, p.clone());
        if !db(move || h.principal(key))
            .await?
            .is_some_and(|v| v == expected)
        {
            return Err("media_denied");
        }
        if !b.call_valid(l.path.as_str(), l.revision) {
            return Err("line_unavailable");
        }
        Ok(rustsimhub_media::Context {
            boot: self.boot.clone(),
            backend: b.context.backend.clone(),
            line: o.grant.line.0,
            call: o.call_id.clone(),
            call_revision: l.revision.max(1),
            lease_generation: o.grant.generation,
            principal: p.id.clone(),
            principal_generation: p.generation,
            peer_key,
            lease_until_ms: o.grant.expires_ms,
            active: true,
            media_scope: true,
            control_scope: true,
        })
    }
    async fn media_request(
        &self,
        token: &str,
        lease: &CallLease,
        peer_key: [u8; 32],
    ) -> Result<(Principal, rustsimhub_media::Context)> {
        if !self.writes || !self.media_enabled {
            return Err("media_disabled");
        }
        let principal = self.principal(token, Scope::MediaUse).await?;
        if principal != self.principal(token, Scope::CallsControl).await? {
            return Err("media_denied");
        }
        self.validate_lease(&principal, lease, &lease.call_id)?;
        let context = self.media_context(&principal, peer_key).await?;
        Ok((principal, context))
    }
    async fn handle_media(&mut self, j: crate::media_ipc::Job) {
        use crate::media_ipc::Action;
        self.media.expire(self.now());
        let result = match j.action {
            Action::Disconnect => {
                self.media.disconnected(j.connection);
                Ok(json!({"ok":true}))
            }
            action => {
                let context = match self.media.owner.clone() {
                    Some(p) => self.media_context(&p, self.media.peer_key).await,
                    None => Err("media_denied"),
                };
                match context {
                    Ok(context) => {
                        let now = self.now();
                        match action {
                            Action::Redeem { ticket, peer_key } => {
                                self.media
                                    .redeem(j.connection, ticket, peer_key, &context, now)
                            }
                            Action::Pulse => self.media.pulse(j.connection, &context, now),
                            Action::Disconnect => unreachable!(),
                        }
                    }
                    Err(e) => {
                        self.media.revoke();
                        Err(e)
                    }
                }
            }
        };
        let _ = j.reply.send(result);
    }
    pub async fn run(mut self) {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let job = tokio::select! {biased;
                _=self.stop.changed()=>{self.media.revoke();self.cleanup(true).await;return;},
             j=self.urgent.recv()=>match j{Some(j)=>Some(j),None=>{self.cleanup(true).await;return;}},
             j=self.normal.recv()=>match j{Some(j)=>Some(j),None=>{self.cleanup(true).await;return;}},
             j=self.media_jobs.recv()=>{if let Some(j)=j {self.handle_media(j).await;}continue;},
             _=tick.tick()=>None,
             changed=self.bindings.changed()=>{if changed.is_err(){return;}None},
            };
            if let Some(j) = job {
                self.expire().await;
                let result = self.process(j.command).await;
                let _ = j.reply.send(result);
            } else {
                let _ = self.refresh().await;
                self.expire().await;
            }
        }
    }
    fn now(&self) -> u64 {
        let now = rustsimhub_media::local::now_ms();
        #[cfg(test)]
        let now = now.saturating_add(self.advance_ms);
        now
    }
    fn binding(&self) -> Result<Binding> {
        self.bindings
            .borrow()
            .clone()
            .filter(|b| b.valid())
            .ok_or("line_unavailable")
    }
    async fn principal(&self, token: &str, scope: Scope) -> Result<Principal> {
        let (a, t) = (self.auth.clone(), token.to_owned());
        tokio::task::spawn_blocking(move || {
            a.try_lock()
                .map_err(|_| "auth_busy")?
                .authorize(&t, scope)
                .map_err(|_| "denied")
        })
        .await
        .map_err(|_| "auth_unavailable")?
    }
    async fn unavailable(&mut self) {
        self.media.revoke();
        self.catalogue = false;
        let ids = self.live.values().map(|l| l.summary.id.clone()).collect();
        let h = self.store.clone();
        let _ = db(move || h.calls_unavailable(ids)).await;
    }
    async fn refresh(&mut self) -> Result<()> {
        match tokio::time::timeout(Duration::from_secs(6), self.refresh_inner()).await {
            Ok(Ok(())) => Ok(()),
            _ => {
                self.unavailable().await;
                Err("calls_reconciling")
            }
        }
    }
    async fn refresh_inner(&mut self) -> Result<()> {
        self.catalogue = false;
        let b = self.binding()?;
        if !b.fresh().await {
            return Err("line_unavailable");
        }
        let changed = self.backend.as_deref() != Some(&b.context.backend);
        if changed {
            self.unavailable().await;
            self.live.clear();
            self.owned = None;
            self.book = CallLeases::new(self.boot.clone(), b.context.line);
            self.epoch = b.context.line;
        }
        let paths = if changed {
            let voice = VoiceProxy::builder(&b.connection)
                .destination(b.context.owner.clone())
                .map_err(|_| "backend_invalid")?
                .path(b.context.modem.clone())
                .map_err(|_| "backend_invalid")?
                .build()
                .await
                .map_err(|_| "backend_invalid")?;
            bounded(voice.list_calls()).await?
        } else {
            let proxy = zbus::fdo::PropertiesProxy::builder(&b.connection)
                .destination(b.context.owner.clone())
                .map_err(|_| "backend_invalid")?
                .path(b.context.modem.clone())
                .map_err(|_| "backend_invalid")?
                .build()
                .await
                .map_err(|_| "backend_invalid")?;
            let value = tokio::time::timeout(
                Duration::from_secs(5),
                proxy.get(
                    "org.freedesktop.ModemManager1.Modem.Voice"
                        .try_into()
                        .map_err(|_| "backend_invalid")?,
                    "Calls",
                ),
            )
            .await
            .map_err(|_| "modem_timeout")?
            .map_err(|_| "modem_unavailable")?;
            Vec::<OwnedObjectPath>::try_from(value).map_err(|_| "invalid_catalogue")?
        };
        if paths.len() > 64 {
            return Err("call_limit");
        }
        let mut seen = HashSet::new();
        for path in paths {
            if !valid_path(&path) || !seen.insert(path.to_string()) {
                return Err("invalid_catalogue");
            }
            self.read_live(&b, path).await?;
        }
        if !b.fresh().await {
            return Err("line_changed");
        }
        let missing: Vec<String> = self
            .live
            .keys()
            .filter(|p| !seen.contains(*p))
            .cloned()
            .collect();
        let mut ids = vec![];
        for path in missing {
            if let Some(old) = self.live.remove(&path) {
                ids.push(old.summary.id);
            }
        }
        if !ids.is_empty() {
            let h = self.store.clone();
            db(move || h.calls_unavailable(ids)).await?;
        }
        self.backend = Some(b.context.backend.clone());
        self.catalogue = true;
        if let Some(o) = &self.owned {
            if self
                .live
                .values()
                .all(|l| l.summary.id != o.call_id || l.summary.modem_state == 7)
            {
                let now = self.now();
                let _ = self.book.release(&o.grant, &o.principal.id, now);
                self.owned = None;
            }
        }
        Ok(())
    }
    async fn read_live(&mut self, b: &Binding, path: OwnedObjectPath) -> Result<String> {
        let data = read_call(b, &path).await?;
        let old = self.live.get(path.as_str());
        let reused = old.is_some_and(|l| {
            !b.call_valid(path.as_str(), l.revision)
                || l.number != data.number
                || l.summary.direction != data.direction
        });
        let revision = if reused || old.is_none() {
            b.revision()
        } else {
            old.ok_or("call_unavailable")?.revision
        };
        if !b.call_valid(path.as_str(), revision) {
            return Err("call_changed");
        }
        if reused {
            let id = old.ok_or("call_unavailable")?.summary.id.clone();
            let h = self.store.clone();
            db(move || h.calls_unavailable(vec![id])).await?;
        }
        let source = format!("{path}#{revision}");
        let (h, context, guard, object) = (
            self.store.clone(),
            b.context.clone(),
            b.clone(),
            path.to_string(),
        );
        let number = data.number.clone();
        let summary = db(move || {
            h.observe_call(
                context,
                CallObservation {
                    source,
                    number: data.number,
                    direction: data.direction,
                    state: data.state,
                    reason: data.reason,
                },
                move || guard.call_valid(&object, revision),
            )
        })
        .await?;
        let id = summary.id.clone();
        self.live.insert(
            path.to_string(),
            Live {
                summary,
                path,
                revision,
                number,
            },
        );
        Ok(id)
    }
    fn lease(&self, p: &Principal) -> Option<CallLease> {
        let o = self.owned.as_ref()?;
        if self
            .live
            .values()
            .any(|l| l.summary.id == o.call_id && l.summary.modem_state == 7)
        {
            return None;
        }
        if &o.principal != p || self.book.validate(&o.grant, &p.id, self.now()).is_err() {
            return None;
        }
        Some(CallLease {
            call_id: o.call_id.clone(),
            boot: o.grant.boot.clone(),
            line: o.grant.line,
            generation: o.grant.generation,
            expires_in_seconds: ((o.grant.expires_ms - self.now()) / 1000) as u32,
        })
    }
    fn validate_lease(&self, p: &Principal, receipt: &CallLease, id: &str) -> Result<()> {
        let o = self.owned.as_ref().ok_or("call_unowned")?;
        if &o.principal != p
            || o.call_id != id
            || receipt.call_id != id
            || receipt.boot != o.grant.boot
            || receipt.line != o.grant.line
            || receipt.generation != o.grant.generation
        {
            return Err("call_owned");
        }
        self.book
            .validate(&o.grant, &p.id, self.now())
            .map_err(|_| "lease_expired")
    }
    async fn result(&self, id: String, p: Principal) -> Result<Value> {
        let h = self.store.clone();
        let principal = p.clone();
        let op = db(move || h.call_operation(id, principal)).await?;
        let lease = self
            .lease(&p)
            .filter(|l| op.call_id.as_deref() == Some(&l.call_id));
        Ok(json!({"operation":op,"lease":lease}))
    }
    async fn process(&mut self, command: AuthCommand) -> Result<Value> {
        match command {
            AuthCommand::IssueMedia {
                token,
                lease,
                peer_key,
            } => {
                let (p, context) = self.media_request(&token, &lease, peer_key).await?;
                let now = self.now();
                self.media.expire(now);
                serde_json::to_value(self.media.issue(p, context, now)?).map_err(|_| "media_denied")
            }
            AuthCommand::CheckMediaLease { token, lease } => {
                let (_, c) = self.media_request(&token, &lease, [0; 32]).await?;
                // Stable over renewal, but changes with every owner/line/call/lease incarnation.
                let binding = serde_json::to_vec(&(
                    "rustsimhub-media-owner-binding-v1",
                    c.boot,
                    c.backend,
                    c.line,
                    c.call,
                    c.call_revision,
                    c.lease_generation,
                    c.principal,
                    c.principal_generation,
                ))
                .map_err(|_| "media_denied")?;
                let binding = Sha256::digest(binding)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                Ok(json!({"media_lease_valid":true,"binding":binding}))
            }
            AuthCommand::CallStatus { token } => {
                self.principal(&token, Scope::CallsRead).await?;
                let _ = self.refresh().await;
                Ok(
                    json!({"available":self.catalogue,"observed_calls":self.live.len(),"active_calls":self.live.values().filter(|l|l.summary.modem_state!=7).count(),"control_api":self.writes,"media_authorization_api":self.media_enabled,"media_available":false}),
                )
            }
            AuthCommand::PrepareCall {
                token,
                line_epoch,
                idempotency_key,
                action,
                call_id,
                payload,
            } => {
                if !self.writes {
                    return Err("calls_read_only");
                }
                let p = self.principal(&token, Scope::CallsControl).await?;
                let _network = self.mutation_guard(action).await?;
                let b = self.binding()?;
                let (h, context, guard) = (self.store.clone(), b.context.clone(), b.clone());
                let op = db(move || {
                    h.prepare_call(
                        CallRequest {
                            line: LineEpoch(line_epoch),
                            idempotency_key,
                            action,
                            call_id,
                            payload,
                        },
                        p,
                        context,
                        move || guard.valid(),
                    )
                })
                .await?;
                Ok(json!({"operation":op,"lease":null}))
            }
            AuthCommand::CallOperation {
                token,
                operation_id,
            } => {
                let p = self.principal(&token, Scope::CallsControl).await?;
                let _ = self.reconcile(&operation_id, &token, &p).await;
                self.result(operation_id, p).await
            }
            AuthCommand::RenewCall { token, lease } => {
                if !self.writes {
                    return Err("calls_read_only");
                }
                let p = self.principal(&token, Scope::CallsControl).await?;
                self.refresh().await?;
                self.validate_lease(&p, &lease, &lease.call_id)?;
                let b = self.binding()?;
                let l = self
                    .live
                    .values()
                    .find(|l| l.summary.id == lease.call_id)
                    .ok_or("call_unavailable")?;
                if !b.call_valid(l.path.as_str(), l.revision) || l.summary.modem_state == 7 {
                    return Err("call_unavailable");
                }
                let now = self.now();
                let o = self.owned.as_mut().ok_or("call_unowned")?;
                o.grant = self
                    .book
                    .renew(&o.grant, &p.id, now)
                    .map_err(|_| "lease_expired")?;
                Ok(json!({"lease":self.lease(&p)}))
            }
            AuthCommand::SubmitCall {
                token,
                operation_id,
                lease,
                urgent,
            } => {
                if !self.writes {
                    return Err("calls_read_only");
                }
                let p = self.principal(&token, Scope::CallsControl).await?;
                let (h, id, principal) = (self.store.clone(), operation_id.clone(), p.clone());
                let intent = db(move || h.call_intent(id, principal)).await?;
                if intent.action == CallAction::Hangup {
                    self.media.revoke();
                }
                if urgent && intent.action != CallAction::Hangup {
                    return Err("invalid_priority");
                }
                if intent.operation.state == SmsState::Prepared {
                    self.submit(&operation_id, &token, &p, lease.as_ref())
                        .await?;
                }
                self.result(operation_id, p).await
            }
            _ => Err("invalid_call_command"),
        }
    }
    async fn finish(&self, id: &str, p: &Principal, state: SmsState) {
        let (h, id, client) = (self.store.clone(), id.to_owned(), p.id.clone());
        let _ = db(move || h.finish_call(id, client, state)).await;
    }
    async fn submit(
        &mut self,
        id: &str,
        token: &str,
        p: &Principal,
        receipt: Option<&CallLease>,
    ) -> Result<()> {
        self.refresh().await?;
        let b = self.binding()?;
        let (h, key, principal) = (self.store.clone(), id.to_owned(), p.clone());
        let intent = db(move || h.call_intent(key, principal)).await?;
        let _network = self.mutation_guard(intent.action).await?;
        if intent.backend != b.context.backend
            || intent.operation.line != b.context.line
            || intent.principal_generation != p.generation
        {
            return Err("stale_operation");
        }
        if matches!(intent.action, CallAction::Dial | CallAction::Accept) {
            if self.owned.is_some() {
                return Err("call_owned");
            }
            if self.live.values().any(|l| {
                l.summary.modem_state != 7
                    && intent.operation.call_id.as_deref() != Some(&l.summary.id)
            }) {
                return Err("modem_busy");
            }
        } else {
            let target = intent
                .operation
                .call_id
                .as_deref()
                .ok_or("target_missing")?;
            self.validate_lease(p, receipt.ok_or("lease_required")?, target)?;
        }
        let grant = if matches!(intent.action, CallAction::Dial | CallAction::Accept) {
            Some(
                self.book
                    .acquire(p.id.clone(), self.now(), false)
                    .map_err(|_| "call_owned")?,
            )
        } else {
            None
        };
        let (h, key, principal, context, guard) = (
            self.store.clone(),
            id.to_owned(),
            p.clone(),
            b.context.clone(),
            b.clone(),
        );
        if let Err(e) =
            db(move || h.begin_call(key, principal, context, move || guard.valid())).await
        {
            if let Some(g) = grant {
                let _ = self.book.release(&g, &p.id, self.now());
            }
            return Err(e);
        }
        let target = if intent.action == CallAction::Dial {
            let created = async {
                if !self.urgent.is_empty() {
                    return Err("urgent_pending");
                }
                if !b.fresh().await {
                    return Err("line_changed");
                }
                self.principal(token, Scope::CallsControl).await?;
                let voice = VoiceProxy::builder(&b.connection)
                    .destination(b.context.owner.clone())
                    .map_err(|_| "backend_invalid")?
                    .path(b.context.modem.clone())
                    .map_err(|_| "backend_invalid")?
                    .build()
                    .await
                    .map_err(|_| "backend_invalid")?;
                if bounded(voice.emergency_only()).await? {
                    return Err("emergency_only");
                }
                bounded(voice.create_call(HashMap::from([(
                    "number".to_owned(),
                    OwnedValue::from(zbus::zvariant::Str::from(intent.payload.as_str())),
                )])))
                .await
            }
            .await;
            match created {
                Ok(path) if valid_path(&path) => {
                    let target = self.read_live(&b, path).await;
                    match target {
                        Ok(id) => id,
                        Err(_) => {
                            self.finish(id, p, SmsState::Unknown).await;
                            return Ok(());
                        }
                    }
                }
                _ => {
                    self.finish(id, p, SmsState::Unknown).await;
                    return Ok(());
                }
            }
        } else {
            intent.operation.call_id.clone().ok_or("target_missing")?
        };
        if let Some(grant) = grant {
            self.owned = Some(Owned {
                principal: p.clone(),
                grant,
                call_id: target.clone(),
                origin: id.to_owned(),
                cleanup_attempted: false,
            });
        }
        let permit = async {
            let l = self
                .live
                .values()
                .find(|l| l.summary.id == target)
                .ok_or("call_unavailable")?;
            let data = read_call(&b, &l.path).await?;
            if !intent.action.allowed(data.state, data.direction)
                || intent.action == CallAction::Dial && data.number != intent.payload
                || !b.call_valid(l.path.as_str(), l.revision)
                || !b.fresh().await
            {
                return Err("call_changed");
            }
            let principal = self.principal(token, Scope::CallsControl).await?;
            let owned = self.owned.as_ref().ok_or("call_unowned")?;
            self.book
                .validate(&owned.grant, &p.id, self.now())
                .map_err(|_| "lease_expired")?;
            let deadline = owned.grant.expires_ms;
            let (h, key, context, guard, path, revision, target) = (
                self.store.clone(),
                id.to_owned(),
                b.context.clone(),
                b.clone(),
                l.path.to_string(),
                l.revision,
                target.clone(),
            );
            db(move || {
                h.permit_call(
                    key,
                    principal,
                    context,
                    target,
                    (path.clone(), revision),
                    move || {
                        rustsimhub_media::local::now_ms() < deadline
                            && guard.call_valid(&path, revision)
                    },
                )
            })
            .await
        }
        .await;
        if permit.is_err() {
            self.finish(id, p, SmsState::Cancelled).await;
            return Ok(());
        }
        let l = self
            .live
            .values()
            .find(|l| l.summary.id == target)
            .ok_or("call_unavailable")?;
        let path = l.path.clone();
        let revision = l.revision;
        let sent = async {
            if intent.action != CallAction::Hangup && !self.urgent.is_empty() {
                return Err("urgent_pending");
            }
            let owned = self.owned.as_ref().ok_or("call_unowned")?;
            self.book
                .validate(&owned.grant, &p.id, self.now())
                .map_err(|_| "lease_expired")?;
            if !b.call_valid(path.as_str(), revision) || !b.fresh().await {
                return Err("call_changed");
            }
            self.principal(token, Scope::CallsControl).await?;
            let proxy = CallProxy::builder(&b.connection)
                .destination(b.context.owner.clone())
                .map_err(|_| "backend_invalid")?
                .path(path)
                .map_err(|_| "backend_invalid")?
                .build()
                .await
                .map_err(|_| "backend_invalid")?;
            match intent.action {
                CallAction::Dial => bounded(proxy.start()).await,
                CallAction::Accept => bounded(proxy.accept()).await,
                CallAction::Hangup => bounded(proxy.hangup()).await,
                CallAction::Dtmf => bounded(proxy.send_dtmf(&intent.payload)).await,
            }
        }
        .await;
        if sent.is_ok() {
            if intent.action == CallAction::Dtmf {
                let _ = self.confirm(id, token, p, &b, &target).await;
            } else {
                let _ = self.reconcile(id, token, p).await;
            }
        }
        let (h, key, principal) = (self.store.clone(), id.to_owned(), p.clone());
        if db(move || h.call_operation(key, principal))
            .await
            .is_ok_and(|op| op.state == SmsState::Submitting)
        {
            self.finish(id, p, SmsState::Unknown).await;
        }
        Ok(())
    }
    async fn confirm(
        &self,
        id: &str,
        token: &str,
        p: &Principal,
        b: &Binding,
        target: &str,
    ) -> Result<()> {
        let l = self
            .live
            .values()
            .find(|l| l.summary.id == target)
            .ok_or("call_unavailable")?;
        let principal = self.principal(token, Scope::CallsControl).await?;
        if principal != *p {
            return Err("denied");
        }
        let (h, key, context, guard, path, revision) = (
            self.store.clone(),
            id.to_owned(),
            b.context.clone(),
            b.clone(),
            l.path.to_string(),
            l.revision,
        );
        db(move || {
            h.confirm_call(key, principal, context, move || {
                guard.call_valid(&path, revision)
            })
        })
        .await
    }
    async fn reconcile(&mut self, id: &str, token: &str, p: &Principal) -> Result<()> {
        let (h, key, principal) = (self.store.clone(), id.to_owned(), p.clone());
        let a = db(move || h.call_intent(key, principal)).await?;
        if !matches!(a.operation.state, SmsState::Submitting | SmsState::Unknown)
            || a.phase != "permitted"
            || a.principal_generation != p.generation
        {
            return Err("not_reconcilable");
        }
        let b = self.binding()?;
        if b.context.backend != a.backend || b.context.line != a.operation.line {
            return Err("binding_changed");
        }
        let path = OwnedObjectPath::try_from(a.path.ok_or("object_missing")?)
            .map_err(|_| "object_invalid")?;
        let revision = a.revision.ok_or("object_missing")?;
        if !b.call_valid(path.as_str(), revision) {
            return Err("call_changed");
        }
        let data = read_call(&b, &path).await?;
        if !a.action.confirmed(data.state) || !b.fresh().await {
            return Err("unconfirmed");
        }
        let target = a.operation.call_id.ok_or("object_missing")?;
        self.read_live(&b, path).await?;
        self.confirm(id, token, p, &b, &target).await
    }
    async fn expire(&mut self) {
        self.cleanup(false).await;
    }
    async fn cleanup(&mut self, force: bool) {
        let Some(o) = &self.owned else {
            return;
        };
        if o.cleanup_attempted {
            return;
        }
        let (h, key, p) = (
            self.store.clone(),
            o.principal.public_key,
            o.principal.clone(),
        );
        let current = db(move || h.principal(key)).await;
        if !force
            && self.book.validate(&o.grant, &p.id, self.now()).is_ok()
            && current.is_ok_and(|c| c.as_ref() == Some(&p))
        {
            return;
        }
        self.media.revoke();
        if !self.writes {
            return;
        }
        let Ok(b) = self.binding() else {
            return;
        };
        let Some(l) = self.live.values().find(|l| l.summary.id == o.call_id) else {
            return;
        };
        if l.summary.modem_state == 7
            || !b.call_valid(l.path.as_str(), l.revision)
            || !b.fresh().await
        {
            return;
        }
        // A reused path must not turn expiry cleanup into a hangup of someone else's call.
        let Ok(data) = read_call(&b, &l.path).await else {
            return;
        };
        if data.number != l.number
            || data.direction != l.summary.direction
            || data.state == 7
            || !b.call_valid(l.path.as_str(), l.revision)
        {
            return;
        }
        let (path, revision, target, origin, client) = (
            l.path.clone(),
            l.revision,
            l.summary.id.clone(),
            o.origin.clone(),
            p.id.clone(),
        );
        let (h, context, guard, object) = (
            self.store.clone(),
            b.context.clone(),
            b.clone(),
            path.to_string(),
        );
        let op = db(move || {
            h.prepare_call_cleanup(
                origin,
                client,
                context,
                (target, object.clone(), revision),
                move || guard.call_valid(&object, revision),
            )
        })
        .await;
        let Ok(op) = op else {
            return;
        };
        if let Some(o) = &mut self.owned {
            o.cleanup_attempted = true;
        }
        let sent = async {
            if !b.call_valid(path.as_str(), revision) || !b.fresh().await {
                return Err("call_changed");
            }
            let proxy = CallProxy::builder(&b.connection)
                .destination(b.context.owner.clone())
                .map_err(|_| "backend_invalid")?
                .path(path.clone())
                .map_err(|_| "backend_invalid")?
                .build()
                .await
                .map_err(|_| "backend_invalid")?;
            bounded(proxy.hangup()).await
        }
        .await;
        let confirmed = if sent.is_ok() {
            matches!(read_call(&b,&path).await,Ok(data) if data.state==7) && b.fresh().await
        } else {
            false
        };
        if confirmed {
            let _ = self.read_live(&b, path.clone()).await;
            let (h, id, context, guard, object) = (
                self.store.clone(),
                op.id.clone(),
                b.context.clone(),
                b.clone(),
                path.to_string(),
            );
            if db(move || {
                h.confirm_call_cleanup(id, context, move || guard.call_valid(&object, revision))
            })
            .await
            .is_ok()
            {
                return;
            }
        }
        self.finish(&op.id, &p, SmsState::Unknown).await;
    }
}
fn valid_path(p: &OwnedObjectPath) -> bool {
    p.as_str()
        .starts_with("/org/freedesktop/ModemManager1/Call/")
        && p.as_str().len() <= 224
}
struct Data {
    number: String,
    state: i32,
    direction: i32,
    reason: i32,
}
async fn read_call(b: &Binding, path: &OwnedObjectPath) -> Result<Data> {
    if !valid_path(path) || !b.valid() {
        return Err("call_invalid");
    }
    let proxy = zbus::fdo::PropertiesProxy::builder(&b.connection)
        .destination(b.context.owner.clone())
        .map_err(|_| "call_invalid")?
        .path(path.clone())
        .map_err(|_| "call_invalid")?
        .build()
        .await
        .map_err(|_| "call_invalid")?;
    let props = bounded(
        proxy.get_all(
            "org.freedesktop.ModemManager1.Call"
                .try_into()
                .map_err(|_| "call_invalid")?,
        ),
    )
    .await?;
    let int = |name| {
        props
            .get(name)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or("call_invalid")
    };
    let number = props
        .get("Number")
        .and_then(|v| <&str>::try_from(v).ok())
        .filter(|s| s.len() <= 256)
        .ok_or("call_invalid")?
        .to_owned();
    let (state, direction, reason) = (int("State")?, int("Direction")?, int("StateReason")?);
    if !(0..=7).contains(&state)
        || !(0..=2).contains(&direction)
        || !(0..=255).contains(&reason)
        || !b.valid()
    {
        return Err("call_invalid");
    }
    Ok(Data {
        number,
        state,
        direction,
        reason,
    })
}

#[cfg(test)]
#[path = "calls_checks.rs"]
mod checks;
