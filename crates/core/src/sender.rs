// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit, bounded sender. No startup pump and no automatic resend of uncertain operations.
use crate::observer::Binding;
use rustsimhub_auth::Auth;
use rustsimhub_domain::{LineEpoch, SmsState};
use rustsimhub_policy::{Principal, Scope};
use rustsimhub_storage::{Handle, Operation, SmsRequest};
use rustsimhub_wire::AuthCommand;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
type Result<T> = std::result::Result<T, &'static str>;
#[zbus::proxy(interface = "org.freedesktop.ModemManager1.Modem.Messaging")]
trait Messaging {
    fn create(&self, properties: HashMap<String, OwnedValue>) -> zbus::Result<OwnedObjectPath>;
}
#[zbus::proxy(interface = "org.freedesktop.ModemManager1.Sms")]
trait Sms {
    fn send(&self) -> zbus::Result<()>;
}
struct Job {
    command: AuthCommand,
    reply: oneshot::Sender<Result<Value>>,
}
#[derive(Clone)]
pub(crate) struct Controller {
    queue: mpsc::Sender<Job>,
}
pub(crate) struct Worker {
    queue: mpsc::Receiver<Job>,
    auth: Arc<Mutex<Auth>>,
    store: Handle,
    bindings: watch::Receiver<Option<Binding>>,
}
pub(crate) fn channel(
    auth: Arc<Mutex<Auth>>,
    store: Handle,
    bindings: watch::Receiver<Option<Binding>>,
) -> (Controller, Worker) {
    let (tx, rx) = mpsc::channel(8);
    (
        Controller { queue: tx },
        Worker {
            queue: rx,
            auth,
            store,
            bindings,
        },
    )
}
impl Controller {
    pub async fn request(&self, command: AuthCommand) -> Result<Value> {
        let (reply, rx) = oneshot::channel();
        self.queue
            .try_send(Job { command, reply })
            .map_err(|_| "sender_busy")?;
        tokio::time::timeout(Duration::from_secs(7), rx)
            .await
            .map_err(|_| "pending_query_operation")?
            .map_err(|_| "sender_stopped")?
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
impl Worker {
    pub async fn run(mut self) {
        while let Some(job) = self.queue.recv().await {
            let result = self.process(job.command).await;
            let _ = job.reply.send(result);
        }
    }
    async fn principal(&self, token: &str) -> Result<Principal> {
        let (a, t) = (self.auth.clone(), token.to_owned());
        tokio::task::spawn_blocking(move || {
            a.try_lock()
                .map_err(|_| "auth_busy")?
                .authorize(&t, Scope::SmsSend)
                .map_err(|_| "denied")
        })
        .await
        .map_err(|_| "auth_unavailable")?
    }
    fn binding(&self) -> Result<Binding> {
        self.bindings
            .borrow()
            .clone()
            .filter(|b| b.valid())
            .ok_or("line_unavailable")
    }
    async fn operation(&self, id: String, p: Principal) -> Result<Operation> {
        let h = self.store.clone();
        db(move || h.sender_operation(id, p)).await
    }
    async fn unknown(&self, id: &str, p: &Principal) {
        let (h, id, client) = (self.store.clone(), id.to_owned(), p.id.clone());
        let _ = db(move || h.mark_unknown(&id, &client)).await;
    }
    async fn cancel(&self, id: &str, p: &Principal) {
        let (h, id, client) = (self.store.clone(), id.to_owned(), p.id.clone());
        let _ = db(move || h.cancel_unsent(id, client)).await;
    }
    async fn process(&self, command: AuthCommand) -> Result<Value> {
        let operation = match command {
            AuthCommand::PrepareSms {
                token,
                line_epoch,
                idempotency_key,
                recipient,
                text,
            } => {
                let p = self.principal(&token).await?;
                let b = self.binding()?;
                let h = self.store.clone();
                let context = b.context.clone();
                db(move || {
                    h.prepare_authorized(
                        SmsRequest {
                            client_id: p.id.clone(),
                            idempotency_key,
                            line: LineEpoch(line_epoch),
                            recipient,
                            text,
                        },
                        p,
                        context,
                        move || b.valid(),
                    )
                })
                .await?
            }
            AuthCommand::SmsOperation {
                token,
                operation_id,
            } => {
                let p = self.principal(&token).await?;
                let op = self.operation(operation_id.clone(), p.clone()).await?;
                if op.state == SmsState::Unknown {
                    self.reconcile(&operation_id, &token, &p).await.ok();
                }
                self.operation(operation_id, p).await?
            }
            AuthCommand::SubmitSms {
                token,
                operation_id,
            } => {
                let p = self.principal(&token).await?;
                let op = self.operation(operation_id.clone(), p.clone()).await?;
                if op.state == SmsState::Prepared {
                    self.submit(&operation_id, &token, &p).await?;
                }
                self.operation(operation_id, p).await?
            }
            _ => return Err("invalid_sender_command"),
        };
        serde_json::to_value(operation).map_err(|_| "invalid_operation")
    }
    async fn submit(&self, id: &str, token: &str, p: &Principal) -> Result<()> {
        let b = self.binding()?;
        if !b.fresh().await {
            return Err("line_unavailable");
        }
        let (h, key, principal, context, guard) = (
            self.store.clone(),
            id.to_owned(),
            p.clone(),
            b.context.clone(),
            b.clone(),
        );
        let draft =
            db(move || h.begin_authorized(key, principal, context, move || guard.valid())).await?;
        // Any failure from Create is ambiguous: persist Unknown; never try Create again.
        let created = async {
            if !b.fresh().await {
                return Err("line_changed");
            }
            self.principal(token).await?;
            let proxy = MessagingProxy::builder(&b.connection)
                .destination(b.context.owner.clone())
                .map_err(|_| "backend_invalid")?
                .path(b.context.modem.clone())
                .map_err(|_| "backend_invalid")?
                .build()
                .await
                .map_err(|_| "backend_unavailable")?;
            let properties = HashMap::from([
                (
                    "number".to_owned(),
                    OwnedValue::from(zbus::zvariant::Str::from(draft.recipient.as_str())),
                ),
                (
                    "text".to_owned(),
                    OwnedValue::from(zbus::zvariant::Str::from(draft.text.as_str())),
                ),
            ]);
            tokio::time::timeout(Duration::from_secs(8), proxy.create(properties))
                .await
                .map_err(|_| "create_uncertain")?
                .map_err(|_| "create_uncertain")
        }
        .await;
        let path = match created {
            Ok(path) => path,
            Err(_) => {
                self.unknown(id, p).await;
                return Ok(());
            }
        };
        // Nothing below this branch has called Send. Failed proof/permission/fence safely abandons the draft.
        let object_revision = b.revision();
        let permit = async {
            let state = verify(&b, &path, &draft.recipient, &draft.text).await?;
            if !matches!(state, 0 | 1)
                || !b.object_valid(path.as_str(), object_revision)
                || !b.fresh().await
            {
                return Err("draft_changed");
            }
            let principal = self.principal(token).await?;
            let (h, key, context, guard, created_path) = (
                self.store.clone(),
                id.to_owned(),
                b.context.clone(),
                b.clone(),
                path.to_string(),
            );
            db(move || {
                h.bind_created(
                    key,
                    principal,
                    context,
                    created_path.clone(),
                    object_revision,
                    move || guard.object_valid(&created_path, object_revision),
                )
            })
            .await?;
            let principal = self.principal(token).await?;
            if !b.fresh().await {
                return Err("line_changed");
            }
            let (h, key, context, guard) = (
                self.store.clone(),
                id.to_owned(),
                b.context.clone(),
                b.clone(),
            );
            let object_path = path.to_string();
            db(move || {
                h.permit_send(key, principal, context, move || {
                    guard.object_valid(&object_path, object_revision)
                })
            })
            .await
        }
        .await;
        if permit.is_err() {
            self.cancel(id, p).await;
            return Ok(());
        }
        // Authorization linearizes at the durable permit. A committed permit is never reused.
        let sent = async {
            if !b.object_valid(path.as_str(), object_revision) || !b.fresh().await {
                return Err("line_changed");
            }
            self.principal(token).await?;
            let proxy = SmsProxy::builder(&b.connection)
                .destination(b.context.owner.clone())
                .map_err(|_| "backend_invalid")?
                .path(path)
                .map_err(|_| "backend_invalid")?
                .build()
                .await
                .map_err(|_| "backend_unavailable")?;
            tokio::time::timeout(Duration::from_secs(8), proxy.send())
                .await
                .map_err(|_| "send_uncertain")?
                .map_err(|_| "send_uncertain")
        }
        .await;
        if sent.is_ok() {
            let _ = self.reconcile(id, token, p).await;
        }
        // Send's void reply only means queued. Only the bound object's SENT state is Submitted.
        if self
            .operation(id.to_owned(), p.clone())
            .await
            .is_ok_and(|o| o.state == SmsState::Submitting)
        {
            self.unknown(id, p).await;
        }
        Ok(())
    }
    async fn reconcile(&self, id: &str, token: &str, p: &Principal) -> Result<()> {
        let b = self.binding()?;
        let (h, key, principal) = (self.store.clone(), id.to_owned(), p.clone());
        let a = db(move || h.send_attempt(key, principal)).await?;
        if a.phase != "sending"
            || a.context.backend != b.context.backend
            || a.context.line != b.context.line
            || a.context.fingerprint != b.context.fingerprint
            || a.context.owner != b.context.owner
            || a.context.modem != b.context.modem
        {
            return Err("binding_changed");
        }
        let path = OwnedObjectPath::try_from(a.sms_path.ok_or("object_unbound")?)
            .map_err(|_| "object_invalid")?;
        let revision = a.object_revision.ok_or("object_unbound")?;
        if !b.object_valid(path.as_str(), revision)
            || verify(&b, &path, &a.recipient, &a.text).await? != 5
            || !b.fresh().await
        {
            return Err("unconfirmed");
        }
        let principal = self.principal(token).await?;
        let (h, key, context, guard) = (
            self.store.clone(),
            id.to_owned(),
            b.context.clone(),
            b.clone(),
        );
        db(move || {
            h.resolve_sent(key, principal, context, move || {
                guard.object_valid(path.as_str(), revision)
            })
        })
        .await?;
        Ok(())
    }
}
async fn verify(b: &Binding, path: &OwnedObjectPath, recipient: &str, text: &str) -> Result<u32> {
    if !b.valid() {
        return Err("binding_changed");
    }
    let proxy = zbus::fdo::PropertiesProxy::builder(&b.connection)
        .destination(b.context.owner.clone())
        .map_err(|_| "object_invalid")?
        .path(path.clone())
        .map_err(|_| "object_invalid")?
        .build()
        .await
        .map_err(|_| "object_unavailable")?;
    let p = tokio::time::timeout(
        Duration::from_secs(5),
        proxy.get_all(
            "org.freedesktop.ModemManager1.Sms"
                .try_into()
                .map_err(|_| "object_invalid")?,
        ),
    )
    .await
    .map_err(|_| "object_unavailable")?
    .map_err(|_| "object_unavailable")?;
    let string = |name| p.get(name).and_then(|v| <&str>::try_from(v).ok());
    let kind = p
        .get("PduType")
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("object_invalid")?;
    let data = Vec::<u8>::try_from(
        p.get("Data")
            .ok_or("object_invalid")?
            .try_clone()
            .map_err(|_| "object_invalid")?,
    )
    .map_err(|_| "object_invalid")?;
    if !b.valid()
        || !matches!(kind, 2 | 33)
        || string("Number") != Some(recipient)
        || string("Text") != Some(text)
        || !data.is_empty()
    {
        return Err("object_changed");
    }
    p.get("State")
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("object_invalid")
}
