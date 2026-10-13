// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_policy::{allows, Principal, Scope};
// Trusted live binding; deliberately no Debug or Serialize.
#[derive(Clone)]
pub struct SendContext {
    pub line: LineEpoch,
    pub backend: String,
    pub owner: String,
    pub modem: String,
    pub fingerprint: [u8; 32],
}
pub struct SendAttempt {
    pub operation: Operation,
    pub context: SendContext,
    pub principal_generation: u64,
    pub sms_path: Option<String>,
    pub phase: String,
    pub object_revision: Option<u64>,
    pub recipient: String,
    pub text: String,
}
fn sender(c: &Connection, p: &Principal) -> Result<()> {
    let current = super::auth::lookup_principal(c, &p.public_key)?.ok_or(Error::Forbidden)?;
    if &current != p || !p.active || !allows(p.scopes, Scope::SmsSend) {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub(super) fn ready(c: &Connection, b: &SendContext) -> Result<()> {
    if b.backend.is_empty()
        || b.backend.len() > 256
        || !b.owner.starts_with(':')
        || b.owner.len() > 255
        || !b.modem.starts_with("/org/freedesktop/ModemManager1/Modem/")
        || b.modem.len() > 224
    {
        return Err(Error::InvalidInput);
    }
    let (line,backend,key,status):(i64,String,Option<Vec<u8>>,String)=c.query_row("SELECT m.line_epoch,o.backend,o.sim_fingerprint,o.status FROM meta m,line_observation o WHERE m.singleton=1 AND o.singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    if checked_line(b.line)? != line
        || backend != b.backend
        || key.as_deref() != Some(b.fingerprint.as_slice())
        || status != "ready"
    {
        return Err(Error::StaleLine);
    }
    Ok(())
}
fn attempt(c: &Connection, id: &str, p: &Principal) -> Result<SendAttempt> {
    sender(c, p)?;
    let op = operation(c, id, &p.id)?;
    let row=c.query_row("SELECT a.principal_generation,a.backend,a.owner,a.modem,a.sim_fingerprint,a.sms_path,a.phase,a.object_revision,o.recipient,o.body FROM send_attempts a JOIN sms_operations o ON o.id=a.operation_id WHERE a.operation_id=?",[id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,Vec<u8>>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,String>(6)?,r.get::<_,Option<i64>>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?))).optional()?.ok_or(Error::NotFound)?;
    let fingerprint = row.4.try_into().map_err(|_| Error::Corrupt)?;
    Ok(SendAttempt {
        context: SendContext {
            line: op.line,
            backend: row.1,
            owner: row.2,
            modem: row.3,
            fingerprint,
        },
        operation: op,
        principal_generation: row.0 as u64,
        sms_path: row.5,
        phase: row.6,
        object_revision: row.7.map(|v| v as u64),
        recipient: row.8,
        text: row.9,
    })
}
impl Handle {
    pub fn prepare_authorized(
        &self,
        request: SmsRequest,
        p: Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<Operation> {
        self.call(move |db| {
            sender(&db.connection,&p)?;
            if request.client_id!=p.id {return Err(Error::Forbidden);}
            let exists:bool=db.connection.query_row("SELECT EXISTS(SELECT 1 FROM sms_operations WHERE client_id=? AND idempotency_key=?)",params![p.id,request.idempotency_key],|r|r.get(0))?;
            if !exists {if !guard(){return Err(Error::StaleLine);}ready(&db.connection,&b)?;if request.line!=b.line {return Err(Error::StaleLine);}}
            db.prepare(request)
        })
    }
    pub fn sender_operation(&self, id: String, p: Principal) -> Result<Operation> {
        self.call(move |db| {
            sender(&db.connection, &p)?;
            operation(&db.connection, &id, &p.id)
        })
    }
    pub fn begin_authorized(
        &self,
        id: String,
        p: Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<Submission> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            sender(&tx, &p)?;
            if !guard() {
                return Err(Error::StaleLine);
            }
            ready(&tx, &b)?;
            let mut op = operation(&tx, &id, &p.id)?;
            if op.line != b.line {
                return Err(Error::StaleLine);
            }
            if op.state != SmsState::Prepared {
                return Err(Error::InvalidTransition);
            }
            let (recipient, text) = tx.query_row(
                "SELECT recipient,body FROM sms_operations WHERE id=?",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            tx.execute(
                "INSERT INTO send_attempts VALUES(?,?,?,?,?,?,NULL,NULL,'creating')",
                params![
                    id,
                    i64::try_from(p.generation).map_err(|_| Error::InvalidInput)?,
                    b.backend,
                    b.owner,
                    b.modem,
                    b.fingerprint.as_slice()
                ],
            )?;
            tx.execute(
                "UPDATE sms_operations SET state='submitting' WHERE id=?",
                [&id],
            )?;
            event(&tx, Some(&id), checked_line(op.line)?, "submitting")?;
            tx.commit()?;
            op.state = SmsState::Submitting;
            Ok(Submission {
                operation: op,
                recipient,
                text,
            })
        })
    }
    pub fn send_attempt(&self, id: String, p: Principal) -> Result<SendAttempt> {
        self.call(move |db| attempt(&db.connection, &id, &p))
    }
    pub fn bind_created(
        &self,
        id: String,
        p: Principal,
        b: SendContext,
        path: String,
        revision: u64,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            sender(&tx, &p)?;
            let a = attempt(&tx, &id, &p)?;
            if a.operation.state != SmsState::Submitting
                || a.phase != "creating"
                || a.principal_generation != p.generation
            {
                return Err(Error::InvalidTransition);
            }
            if !guard() {
                return Err(Error::StaleLine);
            }
            ready(&tx, &b)?;
            same_binding(&a.context, &b)?;
            if path.len() > 224 || !path.starts_with("/org/freedesktop/ModemManager1/SMS/") {
                return Err(Error::InvalidInput);
            }
            tx.execute(
                "UPDATE send_attempts SET sms_path=?,object_revision=?,phase='created' WHERE operation_id=?",
                params![path, i64::try_from(revision).map_err(|_|Error::InvalidInput)?, id],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
    pub fn permit_send(
        &self,
        id: String,
        p: Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            sender(&tx, &p)?;
            let a = attempt(&tx, &id, &p)?;
            if a.operation.state != SmsState::Submitting
                || a.phase != "created"
                || a.sms_path.is_none()
                || a.principal_generation != p.generation
            {
                return Err(Error::InvalidTransition);
            }
            if !guard() {
                return Err(Error::StaleLine);
            }
            ready(&tx, &b)?;
            same_binding(&a.context, &b)?;
            tx.execute(
                "UPDATE send_attempts SET phase='sending' WHERE operation_id=?",
                [&id],
            )?;
            tx.commit()?;
            Ok(())
        })
    }
    /// Internal known-before-Send cancellation only. A sending permit can never be reset for retry.
    pub fn cancel_unsent(&self, id: String, client: String) -> Result<()> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let op = operation(&tx, &id, &client)?;
            let phase: String = tx.query_row(
                "SELECT phase FROM send_attempts WHERE operation_id=?",
                [&id],
                |r| r.get(0),
            )?;
            if op.state != SmsState::Submitting || !matches!(phase.as_str(), "creating" | "created")
            {
                return Err(Error::InvalidTransition);
            }
            tx.execute(
                "UPDATE sms_operations SET state='cancelled' WHERE id=?",
                [&id],
            )?;
            tx.execute(
                "UPDATE send_attempts SET phase='abandoned' WHERE operation_id=?",
                [&id],
            )?;
            event(&tx, Some(&id), checked_line(op.line)?, "cancelled")?;
            tx.commit()?;
            Ok(())
        })
    }
    pub fn resolve_sent(
        &self,
        id: String,
        p: Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<Operation> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            sender(&tx, &p)?;
            let a = attempt(&tx, &id, &p)?;
            if !matches!(a.operation.state, SmsState::Submitting | SmsState::Unknown)
                || a.phase != "sending"
                || a.principal_generation != p.generation
            {
                return Err(Error::InvalidTransition);
            }
            if !guard() {
                return Err(Error::StaleLine);
            }
            ready(&tx, &b)?;
            same_binding(&a.context, &b)?;
            tx.execute(
                "UPDATE sms_operations SET state='submitted' WHERE id=?",
                [&id],
            )?;
            tx.execute(
                "UPDATE send_attempts SET phase='resolved' WHERE operation_id=?",
                [&id],
            )?;
            event(&tx, Some(&id), checked_line(a.operation.line)?, "submitted")?;
            tx.commit()?;
            Ok(Operation {
                id,
                line: a.operation.line,
                state: SmsState::Submitted,
            })
        })
    }
}
fn same_binding(a: &SendContext, b: &SendContext) -> Result<()> {
    if a.line != b.line
        || a.backend != b.backend
        || a.owner != b.owner
        || a.modem != b.modem
        || a.fingerprint != b.fingerprint
    {
        return Err(Error::StaleLine);
    }
    Ok(())
}
