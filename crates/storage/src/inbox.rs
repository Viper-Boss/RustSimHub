// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineStatus {
    Unavailable,
    Absent,
    Locked,
    Observing,
    Ready,
}
impl LineStatus {
    fn name(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Absent => "absent",
            Self::Locked => "locked",
            Self::Observing => "observing",
            Self::Ready => "ready",
        }
    }
}
// Trusted adapter input; never expose the fingerprint or backend in public responses.
pub struct LineObservation {
    pub backend: String,
    pub fingerprint: Option<[u8; 32]>,
    pub status: LineStatus,
}
#[derive(Debug, Serialize)]
pub struct LineView {
    pub line: LineEpoch,
    pub status: LineStatus,
}
pub struct IncomingSms {
    pub line: LineEpoch,
    pub source: String,
    pub sender: String,
    pub timestamp: String,
    pub text: String,
    pub data: Vec<u8>,
}
#[derive(Debug, Clone, Serialize)]
pub struct SmsSummary {
    pub id: String,
    pub line: LineEpoch,
    pub binary: bool,
    pub duplicate_candidate: Option<String>,
}
// No Debug: reading a message must not accidentally log its contents.
#[derive(Serialize)]
pub struct SmsDetail {
    pub summary: SmsSummary,
    pub sender: String,
    pub timestamp: String,
    pub text: String,
    pub data: Vec<u8>,
}
pub use rustsimhub_domain::InboxCursor;
#[derive(Debug, Serialize)]
pub struct InboxPage {
    pub messages: Vec<SmsSummary>,
    pub cursor: InboxCursor,
}
#[derive(Debug, Serialize)]
pub struct InboxEvent {
    pub seq: String,
    pub sms_id: Option<String>,
    pub line: LineEpoch,
    pub kind: String,
}
#[derive(Serialize)]
pub struct InboxEvents {
    pub events: Vec<InboxEvent>,
    pub cursor: InboxCursor,
}

fn position(
    c: &Connection,
    after: Option<&InboxCursor>,
    table: &str,
    limit: usize,
) -> Result<(String, i64)> {
    if !(1..=32).contains(&limit) {
        return Err(Error::InvalidInput);
    }
    let epoch: String = c.query_row("SELECT db_epoch FROM meta WHERE singleton=1", [], |r| {
        r.get(0)
    })?;
    let seq = if let Some(a) = after {
        if a.db_epoch != epoch
            || a.seq.is_empty()
            || a.seq.len() > 19
            || !a.seq.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Error::ResyncRequired);
        }
        let seq = a.seq.parse::<i64>().map_err(|_| Error::ResyncRequired)?;
        let max: i64 = c.query_row(
            &format!("SELECT COALESCE(MAX(seq),0) FROM {table}"),
            [],
            |r| r.get(0),
        )?;
        if seq > max {
            return Err(Error::ResyncRequired);
        }
        seq
    } else {
        0
    };
    Ok((epoch, seq))
}
fn summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<SmsSummary> {
    Ok(SmsSummary {
        id: r.get(0)?,
        line: LineEpoch(r.get::<_, i64>(1)? as u64),
        binary: r.get(2)?,
        duplicate_candidate: r.get(3)?,
    })
}
impl Handle {
    /// Trusted nonblocking local fence, checked on the single writer immediately before mutation.
    pub fn observe_line_if(
        &self,
        input: LineObservation,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<LineView> {
        self.call(move |db| {
            if guard() {
                db.observe(input)
            } else {
                Err(Error::StaleLine)
            }
        })
    }
    pub fn receive_sms_if(
        &self,
        input: IncomingSms,
        guard: impl FnOnce() -> bool + Send + 'static,
    ) -> Result<SmsSummary> {
        self.call(move |db| {
            if guard() {
                db.receive(input)
            } else {
                Err(Error::StaleLine)
            }
        })
    }
    pub fn observe_line(&self, input: LineObservation) -> Result<LineView> {
        self.call(move |db| db.observe(input))
    }
    pub fn line_status(&self) -> Result<LineView> {
        self.call(|db| db.line_view())
    }
    pub fn receive_sms(&self, input: IncomingSms) -> Result<SmsSummary> {
        self.call(move |db| db.receive(input))
    }
    pub fn inbox(&self, after: Option<InboxCursor>, limit: usize) -> Result<InboxPage> {
        self.call(move |db| {
        let (epoch, mut seq)=position(&db.connection,after.as_ref(),"inbox",limit)?;
        let mut stmt=db.connection.prepare("SELECT id,line_epoch,length(data)>0,duplicate_candidate,seq FROM inbox WHERE seq>? ORDER BY seq LIMIT ?")?;
        let rows=stmt.query_map(params![seq,limit as i64],|r|Ok((summary(r)?,r.get::<_,i64>(4)?)))?;
        let mut messages=Vec::new(); for row in rows {let (s,n)=row?; messages.push(s);seq=n;}
        Ok(InboxPage{messages,cursor:InboxCursor{db_epoch:epoch,seq:seq.to_string()}})
    })
    }
    pub fn sms_detail(&self, id: String) -> Result<SmsDetail> {
        self.call(move |db| {
        if Uuid::parse_str(&id).is_err() { return Err(Error::InvalidInput); }
        db.connection.query_row("SELECT id,line_epoch,length(data)>0,duplicate_candidate,sender,timestamp,body,data FROM inbox WHERE id=?",[id],|r|Ok(SmsDetail{summary:summary(r)?,sender:r.get(4)?,timestamp:r.get(5)?,text:r.get(6)?,data:r.get(7)?})).optional()?.ok_or(Error::NotFound)
    })
    }
    pub fn inbox_events(&self, after: Option<InboxCursor>, limit: usize) -> Result<InboxEvents> {
        self.call(move |db| {
        let (epoch,mut seq)=position(&db.connection,after.as_ref(),"inbox_events",limit)?;
        let mut stmt=db.connection.prepare("SELECT seq,sms_id,line_epoch,kind FROM inbox_events WHERE seq>? ORDER BY seq LIMIT ?")?;
        let rows=stmt.query_map(params![seq,limit as i64],|r|Ok(InboxEvent{seq:r.get::<_,i64>(0)?.to_string(),sms_id:r.get(1)?,line:LineEpoch(r.get::<_,i64>(2)? as u64),kind:r.get(3)?}))?;
        let mut events=Vec::new(); for row in rows {let e=row?; seq=e.seq.parse().map_err(|_|Error::Corrupt)?;events.push(e);}
        Ok(InboxEvents{events,cursor:InboxCursor{db_epoch:epoch,seq:seq.to_string()}})
    })
    }
}
impl Database {
    fn line_view(&self) -> Result<LineView> {
        let (line,status):(i64,String)=self.connection.query_row("SELECT m.line_epoch,o.status FROM meta m,line_observation o WHERE m.singleton=1 AND o.singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let status = serde_json::from_value(serde_json::Value::String(status))
            .map_err(|_| Error::Corrupt)?;
        Ok(LineView {
            line: LineEpoch(line as u64),
            status,
        })
    }
    fn observe(&mut self, input: LineObservation) -> Result<LineView> {
        if input.backend.len() > 256
            || input.backend.is_empty()
            || (matches!(input.status, LineStatus::Ready | LineStatus::Observing)
                && input.fingerprint.is_none())
        {
            return Err(Error::InvalidInput);
        }
        let (backend, key, status): (String, Option<Vec<u8>>, String) = self.connection.query_row(
            "SELECT backend,sim_fingerprint,status FROM line_observation WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let changed = backend != input.backend
            || key.as_deref() != input.fingerprint.as_ref().map(|v| v.as_slice());
        if !changed && status == input.status.name() {
            return self.line_view();
        }
        let mut line = self.line_view()?.line;
        let tx = self.connection.transaction()?;
        if changed {
            line = LineEpoch(line.0.checked_add(1).ok_or(Error::Full)?);
            Self::change_line_in(&tx, checked_line(line)?)?;
        }
        tx.execute(
            "UPDATE line_observation SET backend=?,sim_fingerprint=?,status=? WHERE singleton=1",
            params![
                input.backend,
                input.fingerprint.as_ref().map(|v| v.as_slice()),
                input.status.name()
            ],
        )?;
        tx.execute(
            "INSERT INTO inbox_events(sms_id,line_epoch,kind) VALUES(NULL,?,?)",
            params![
                checked_line(line)?,
                if changed {
                    "line_changed"
                } else {
                    "line_status"
                }
            ],
        )?;
        tx.commit()?;
        self.line_view()
    }
    fn receive(&mut self, input: IncomingSms) -> Result<SmsSummary> {
        if input.source.is_empty()
            || input.source.len() > 256
            || input.sender.len() > 256
            || input.timestamp.len() > 128
            || input.text.len() > 4096
            || input.data.len() > 4096
            || (!input.text.is_empty() && !input.data.is_empty())
        {
            return Err(Error::InvalidInput);
        }
        let tx = self.connection.transaction()?;
        let (line,status,key):(i64,String,Option<Vec<u8>>)=tx.query_row("SELECT m.line_epoch,o.status,o.sim_fingerprint FROM meta m,line_observation o WHERE m.singleton=1 AND o.singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if checked_line(input.line)? != line || status != "ready" {
            return Err(Error::StaleLine);
        }
        let key = key.ok_or(Error::StaleLine)?;
        let hash = Sha256::digest(
            serde_json::to_vec(&(
                input.sender.as_str(),
                input.timestamp.as_str(),
                input.text.as_str(),
                &input.data,
            ))
            .map_err(|_| Error::InvalidInput)?,
        );
        let prior:Option<(SmsSummary,Vec<u8>)>=tx.query_row("SELECT id,line_epoch,length(data)>0,duplicate_candidate,content_hash FROM inbox WHERE line_epoch=? AND source=?",params![line,input.source],|r|Ok((summary(r)?,r.get(4)?))).optional()?;
        if let Some((s, h)) = prior {
            if h.as_slice() != hash.as_slice() {
                return Err(Error::Conflict);
            }
            return Ok(s);
        }
        // Preserve every new source, including equal content. Candidate is an annotation, not a dedup decision.
        let candidate:Option<String>=tx.query_row("SELECT id FROM inbox WHERE sim_fingerprint=? AND content_hash=? ORDER BY seq DESC LIMIT 1",params![key,hash.as_slice()],|r|r.get(0)).optional()?;
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO inbox(id,line_epoch,sim_fingerprint,source,content_hash,sender,timestamp,body,data,duplicate_candidate) VALUES(?,?,?,?,?,?,?,?,?,?)",params![id,line,key,input.source,hash.as_slice(),input.sender,input.timestamp,input.text,input.data,candidate])?;
        tx.execute(
            "INSERT INTO inbox_events(sms_id,line_epoch,kind) VALUES(?,?,'sms_received')",
            params![id, line],
        )?;
        tx.commit()?;
        Ok(SmsSummary {
            id,
            line: input.line,
            binary: !input.data.is_empty(),
            duplicate_candidate: candidate,
        })
    }
}
