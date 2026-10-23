// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use rustsimhub_domain::{CallAction, CallState};
use rustsimhub_policy::{allows, Principal, Scope};
#[derive(Clone, Serialize)]
pub struct CallSummary {
    pub id: String,
    pub line: LineEpoch,
    pub direction: i32,
    pub state: CallState,
    pub modem_state: i32,
    pub reason: i32,
    pub availability: String,
    pub media_state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
}
pub struct CallObservation {
    pub source: String,
    pub number: String,
    pub direction: i32,
    pub state: i32,
    pub reason: i32,
}
#[derive(Serialize)]
pub struct CallPage {
    pub calls: Vec<CallSummary>,
    pub cursor: InboxCursor,
}
#[derive(Serialize)]
pub struct CallEvent {
    pub seq: String,
    pub call_id: String,
    pub line: LineEpoch,
    pub kind: String,
}
#[derive(Serialize)]
pub struct CallEvents {
    pub events: Vec<CallEvent>,
    pub cursor: InboxCursor,
}
#[derive(Clone, Serialize)]
pub struct CallOperation {
    pub id: String,
    pub call_id: Option<String>,
    pub line: LineEpoch,
    pub state: SmsState,
}
// Trusted adapter inputs; no Debug and never serialize private payloads or modem paths.
pub struct CallRequest {
    pub idempotency_key: String,
    pub line: LineEpoch,
    pub action: CallAction,
    pub call_id: Option<String>,
    pub payload: String,
}
pub struct CallIntent {
    pub operation: CallOperation,
    pub action: CallAction,
    pub payload: String,
    pub backend: String,
    pub phase: String,
    pub path: Option<String>,
    pub revision: Option<u64>,
    pub principal_generation: u64,
}
fn controller(c: &Connection, p: &Principal) -> Result<()> {
    let current = super::auth::lookup_principal(c, &p.public_key)?.ok_or(Error::Forbidden)?;
    if &current != p || !p.active || !allows(p.scopes, Scope::CallsControl) {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn read_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<CallSummary> {
    let state: i32 = r.get(3)?;
    let direction: i32 = r.get(2)?;
    let availability: String = r.get(5)?;
    Ok(CallSummary {
        id: r.get(0)?,
        line: LineEpoch(r.get::<_, i64>(1)? as u64),
        direction,
        state: if availability == "observed" {
            CallState::from_modem(state, direction).unwrap_or(CallState::Reconciling)
        } else {
            CallState::Reconciling
        },
        modem_state: state,
        reason: r.get(4)?,
        availability,
        media_state: "none",
        number: None,
    })
}
fn call_summary(c: &Connection, id: &str) -> Result<CallSummary> {
    c.query_row(
        "SELECT id,line_epoch,direction,state,reason,availability FROM calls WHERE id=?",
        [id],
        read_summary,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
fn operation(c: &Connection, id: &str, p: &Principal) -> Result<CallOperation> {
    controller(c, p)?;
    c.query_row(
        "SELECT id,call_id,line_epoch,state FROM call_operations WHERE id=? AND client_id=?",
        params![id, p.id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)?, r.get(3)?)),
    )
    .optional()?
    .map(|(id, call_id, line, state)| {
        Ok(CallOperation {
            id,
            call_id,
            line: LineEpoch(line as u64),
            state: parse_state(state)?,
        })
    })
    .unwrap_or(Err(Error::NotFound))
}
impl Handle {
    pub fn observe_call(
        &self,
        b: SendContext,
        o: CallObservation,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<CallSummary> {
        self.call(move|db|{
   if o.source.is_empty()||o.source.len()>256||o.number.len()>256||!(0..=2).contains(&o.direction)||!(0..=7).contains(&o.state)||!(0..=255).contains(&o.reason){return Err(Error::InvalidInput);}
   let tx=db.connection.transaction()?;if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;
   let old:Option<(String,String,i32,i32,i32,String)>=tx.query_row("SELECT id,number,direction,state,reason,availability FROM calls WHERE backend=? AND source=?",params![b.backend,o.source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
   let id=if let Some((id,number,direction,state,reason,availability))=old {
    if number!=o.number||direction!=o.direction {return Err(Error::Conflict);}
    if state!=o.state||reason!=o.reason||availability!="observed"{
     tx.execute("UPDATE calls SET state=?,reason=?,availability='observed' WHERE id=?",params![o.state,o.reason,id])?;
     tx.execute("INSERT INTO call_events(call_id,line_epoch,kind) VALUES(?,?,'changed')",params![id,checked_line(b.line)?])?;
    }id
   }else{
    let id=Uuid::new_v4().to_string();tx.execute("INSERT INTO calls(id,line_epoch,backend,source,fingerprint,number,direction,state,reason,availability) VALUES(?,?,?,?,?,?,?,?,?,'observed')",params![id,checked_line(b.line)?,b.backend,o.source,b.fingerprint.as_slice(),o.number,o.direction,o.state,o.reason])?;
    tx.execute("INSERT INTO call_events(call_id,line_epoch,kind) VALUES(?,?,'observed')",params![id,checked_line(b.line)?])?;id
   };let result=call_summary(&tx,&id)?;tx.commit()?;Ok(result)
  })
    }
    pub fn calls_unavailable(&self, ids: Vec<String>) -> Result<()> {
        self.call(move|db|{
  if ids.len()>64{return Err(Error::InvalidInput);}let tx=db.connection.transaction()?;
  for id in ids {let old=call_summary(&tx,&id)?;if old.availability=="observed"{tx.execute("UPDATE calls SET availability='reconciling' WHERE id=?",[&id])?;tx.execute("INSERT INTO call_events(call_id,line_epoch,kind) VALUES(?,?,'unavailable')",params![id,checked_line(old.line)?])?;}}
  tx.commit()?;Ok(())
 })
    }
    pub fn calls(&self, after: Option<InboxCursor>, limit: usize) -> Result<CallPage> {
        self.call(move|db|{
  let(epoch,mut seq)=super::inbox::position(&db.connection,after.as_ref(),"calls",limit)?;
  let mut stmt=db.connection.prepare("SELECT id,line_epoch,direction,state,reason,availability,seq FROM calls WHERE seq>? ORDER BY seq LIMIT ?")?;
  let rows=stmt.query_map(params![seq,limit as i64],|r|Ok((read_summary(r)?,r.get::<_,i64>(6)?)))?;let mut calls=vec![];for row in rows{let(s,n)=row?;calls.push(s);seq=n;}
  Ok(CallPage{calls,cursor:InboxCursor{db_epoch:epoch,seq:seq.to_string()}})
 })
    }
    pub fn call_detail(&self, id: String) -> Result<CallSummary> {
        self.call(move |db| {
            let mut detail = call_summary(&db.connection, &id)?;
            detail.number = Some(db.connection.query_row(
                "SELECT number FROM calls WHERE id=?",
                [id],
                |row| row.get(0),
            )?);
            Ok(detail)
        })
    }
    pub fn call_events(&self, after: Option<InboxCursor>, limit: usize) -> Result<CallEvents> {
        self.call(move|db|{
  let(epoch,mut seq)=super::inbox::position(&db.connection,after.as_ref(),"call_events",limit)?;let mut stmt=db.connection.prepare("SELECT seq,call_id,line_epoch,kind FROM call_events WHERE seq>? ORDER BY seq LIMIT ?")?;
  let rows=stmt.query_map(params![seq,limit as i64],|r|Ok(CallEvent{seq:r.get::<_,i64>(0)?.to_string(),call_id:r.get(1)?,line:LineEpoch(r.get::<_,i64>(2)? as u64),kind:r.get(3)?}))?;let mut events=vec![];for row in rows{let e=row?;seq=e.seq.parse().map_err(|_|Error::Corrupt)?;events.push(e);}Ok(CallEvents{events,cursor:InboxCursor{db_epoch:epoch,seq:seq.to_string()}})
 })
    }
    pub fn prepare_call(
        &self,
        r: CallRequest,
        p: Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<CallOperation> {
        self.call(move|db|{
  controller(&db.connection,&p)?;
  if r.idempotency_key.starts_with("__") || r.idempotency_key.is_empty()||r.idempotency_key.len()>128||r.payload.len()>32||r.line.0==0 {return Err(Error::InvalidInput);}
  match r.action {
   CallAction::Dial=>{if r.call_id.is_some()||!(3..=32).contains(&r.payload.len())||!r.payload.strip_prefix('+').unwrap_or(&r.payload).bytes().all(|c|c.is_ascii_digit()){return Err(Error::InvalidInput);}},
   CallAction::Dtmf=>{if r.call_id.is_none()||r.payload.len()!=1||!r.payload.bytes().all(|c|b"0123456789ABCD*#".contains(&c)){return Err(Error::InvalidInput);}},
   _=>if r.call_id.is_none()||!r.payload.is_empty(){return Err(Error::InvalidInput);},
  }
  let hash=Sha256::digest(serde_json::to_vec(&(r.line,r.action,r.call_id.as_deref(),r.payload.as_str())).map_err(|_|Error::InvalidInput)?);
  let tx=db.connection.transaction()?;
  let old:Option<(String,Vec<u8>)>=tx.query_row("SELECT id,params_hash FROM call_operations WHERE client_id=? AND idempotency_key=?",params![p.id,r.idempotency_key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
  if let Some((id,old))=old {if old.as_slice()!=hash.as_slice(){return Err(Error::Conflict);}return operation(&tx,&id,&p);}
  if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;if b.line!=r.line{return Err(Error::StaleLine);}
  if let Some(id)=&r.call_id {let row:(String,String,i64)=tx.query_row("SELECT backend,availability,line_epoch FROM calls WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;if row.0!=b.backend||row.1!="observed"||row.2!=checked_line(b.line)?{return Err(Error::StaleLine);}}
  let id=Uuid::new_v4().to_string();tx.execute("INSERT INTO call_operations VALUES(?,?,?,?,?,?,?,?,?,?,'prepared','prepared',NULL,NULL)",params![id,p.id,i64::try_from(p.generation).map_err(|_|Error::InvalidInput)?,r.idempotency_key,hash.as_slice(),checked_line(r.line)?,r.action.name(),r.call_id,r.payload,b.backend])?;let op=operation(&tx,&id,&p)?;tx.commit()?;Ok(op)
 })
    }
    pub fn call_operation(&self, id: String, p: Principal) -> Result<CallOperation> {
        self.call(move |db| operation(&db.connection, &id, &p))
    }
    pub fn cancel_prepared_call(&self, id: String, p: Principal) -> Result<CallOperation> {
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            controller(&tx, &p)?;
            let old = operation(&tx, &id, &p)?;
            if old.state == SmsState::Cancelled { return Ok(old); }
            // A late submit either wins first, or sees Cancelled and cannot
            // obtain a permit. Never relabel Submitting/Unknown as cancelled.
            let changed = tx.execute(
                "UPDATE call_operations SET state='cancelled',phase='resolved' WHERE id=? AND client_id=? AND principal_generation=? AND state='prepared' AND phase='prepared'",
                params![id,p.id,i64::try_from(p.generation).map_err(|_| Error::InvalidInput)?],
            )?;
            if changed != 1 { return Err(Error::InvalidTransition); }
            let op = operation(&tx, &id, &p)?;
            tx.commit()?;
            Ok(op)
        })
    }
    pub fn call_intent(&self, id: String, p: Principal) -> Result<CallIntent> {
        self.call(move|db|{
  let op=operation(&db.connection,&id,&p)?;
  let(action,payload,backend,phase,path,revision,generation):(String,String,String,String,Option<String>,Option<i64>,i64)=db.connection.query_row("SELECT action,payload,backend,phase,object_path,object_revision,principal_generation FROM call_operations WHERE id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
  let action=serde_json::from_value(serde_json::Value::String(action)).map_err(|_|Error::Corrupt)?;
  Ok(CallIntent{operation:op,action,payload,backend,phase,path,revision:revision.map(|r|r as u64),principal_generation:generation as u64})
 })
    }
    pub fn begin_call(
        &self,
        id: String,
        p: Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<()> {
        self.call(move|db|{
  let tx=db.connection.transaction()?;controller(&tx,&p)?;let op=operation(&tx,&id,&p)?;if op.state!=SmsState::Prepared||op.line!=b.line{return Err(Error::InvalidTransition);}
if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;
  let changed=tx.execute("UPDATE call_operations SET state='submitting',phase='creating' WHERE id=? AND backend=? AND principal_generation=?",params![id,b.backend,i64::try_from(p.generation).map_err(|_|Error::InvalidInput)?])?;if changed!=1{return Err(Error::Forbidden);}tx.commit()?;Ok(())
 })
    }
    pub fn permit_call(
        &self,
        id: String,
        p: Principal,
        b: SendContext,
        call_id: String,
        path_revision: (String, u64),
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<()> {
        self.call(move|db|{
  let tx=db.connection.transaction()?;controller(&tx,&p)?;let op=operation(&tx,&id,&p)?;if op.state!=SmsState::Submitting||op.line!=b.line{return Err(Error::InvalidTransition);}
if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;
  let (path,revision)=path_revision;if !path.starts_with("/org/freedesktop/ModemManager1/Call/")||path.len()>224 {return Err(Error::InvalidInput);}
  let target=call_summary(&tx,&call_id)?;if target.line!=b.line||target.availability!="observed"{return Err(Error::StaleLine);}
  let call_backend:String=tx.query_row("SELECT backend FROM calls WHERE id=?",[&call_id],|r|r.get(0))?;if call_backend!=b.backend{return Err(Error::StaleLine);}
  let changed=tx.execute("UPDATE call_operations SET call_id=?,object_path=?,object_revision=?,phase='permitted' WHERE id=? AND phase='creating' AND backend=? AND principal_generation=?",params![call_id,path,i64::try_from(revision).map_err(|_|Error::InvalidInput)?,id,b.backend,i64::try_from(p.generation).map_err(|_|Error::InvalidInput)?])?;if changed!=1{return Err(Error::InvalidTransition);}tx.commit()?;Ok(())
 })
    }
    pub fn finish_call(&self, id: String, client: String, state: SmsState) -> Result<()> {
        self.call(move|db|{
  if !matches!(state,SmsState::Unknown|SmsState::Cancelled){return Err(Error::InvalidInput);}
  let tx=db.connection.transaction()?;
  let (old,phase):(String,String)=tx.query_row("SELECT state,phase FROM call_operations WHERE id=? AND client_id=?",params![id,client],|r|Ok((r.get(0)?,r.get(1)?)))?;
  if !matches!(old.as_str(),"submitting"|"unknown")||state==SmsState::Submitted&&phase!="permitted"||state==SmsState::Cancelled&&phase!="creating" {return Err(Error::InvalidTransition);}
  let name=serde_json::to_value(state).map_err(|_|Error::InvalidInput)?;tx.execute("UPDATE call_operations SET state=?,phase=CASE WHEN ?='submitted' THEN 'resolved' ELSE phase END WHERE id=?",params![name.as_str().ok_or(Error::InvalidInput)?,name.as_str(),id])?;tx.commit()?;Ok(())
 })
    }
}

impl Handle {
    pub fn confirm_call(
        &self,
        id: String,
        p: rustsimhub_policy::Principal,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<()> {
        self.call(move|db|{
  let tx=db.connection.transaction()?;controller(&tx,&p)?;if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;
  let changed=tx.execute("UPDATE call_operations SET state='submitted',phase='resolved' WHERE id=? AND client_id=? AND backend=? AND line_epoch=? AND principal_generation=? AND state IN ('submitting','unknown') AND phase='permitted'",params![id,p.id,b.backend,checked_line(b.line)?,i64::try_from(p.generation).map_err(|_|Error::InvalidInput)?])?;
  if changed!=1{return Err(Error::InvalidTransition);}tx.commit()?;Ok(())
 })
    }
}

impl Handle {
    /// Trusted cleanup of a call this core created/accepted. Never exposed as an IPC command.
    pub fn prepare_call_cleanup(
        &self,
        origin: String,
        client: String,
        b: SendContext,
        target: (String, String, u64),
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<CallOperation> {
        self.call(move|db|{
  let tx=db.connection.transaction()?;if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;
  let (generation,backend,action,old_call):(i64,String,String,Option<String>)=tx.query_row("SELECT principal_generation,backend,action,call_id FROM call_operations WHERE id=? AND client_id=?",params![origin,client],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
  let (call_id,path,revision)=target;
  if backend!=b.backend||!matches!(action.as_str(),"dial"|"accept")||old_call.as_deref().is_some_and(|id|id!=call_id)||!path.starts_with("/org/freedesktop/ModemManager1/Call/")||path.len()>224{return Err(Error::Forbidden);}
  let (backend,line,availability):(String,i64,String)=tx.query_row("SELECT backend,line_epoch,availability FROM calls WHERE id=?",[&call_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
  if backend!=b.backend||line!=checked_line(b.line)?||availability!="observed"{return Err(Error::StaleLine);}
  let id=Uuid::new_v4().to_string();let key=format!("__cleanup__:{origin}");
  tx.execute("INSERT INTO call_operations VALUES(?,?,?,?,zeroblob(32),?,'hangup',?,'',?,'submitting','permitted',?,?)",params![id,client,generation,key,checked_line(b.line)?,call_id,b.backend,path,i64::try_from(revision).map_err(|_|Error::InvalidInput)?])?;
  tx.commit()?;Ok(CallOperation{id,call_id:Some(call_id),line:b.line,state:SmsState::Submitting})
 })
    }
}

impl Handle {
    /// Only the core cleanup path uses this, after observing TERMINATED on the same owned object.
    pub fn confirm_call_cleanup(
        &self,
        id: String,
        b: SendContext,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<()> {
        self.call(move|db|{
  let tx=db.connection.transaction()?;if !guard(){return Err(Error::StaleLine);}super::outgoing::ready(&tx,&b)?;
  let changed=tx.execute("UPDATE call_operations SET state='submitted',phase='resolved' WHERE id=? AND substr(idempotency_key,1,12)='__cleanup__:' AND backend=? AND line_epoch=? AND phase='permitted' AND state IN ('submitting','unknown')",params![id,b.backend,checked_line(b.line)?])?;
  if changed!=1{return Err(Error::InvalidTransition);}tx.commit()?;Ok(())
 })
    }
}
