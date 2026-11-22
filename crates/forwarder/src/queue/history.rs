// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit historical delivery. Preparing never enqueues or starts a worker.
//! Core retains the recent Owner lock while sampling and committing a plan.
use super::*;
use rustsimhub_auth::Auth;
use rustsimhub_storage::{Handle, LineStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_HISTORY_DELIVERIES: usize = 64;
const MAX_BATCHES: i64 = 64;
const PREVIEW_TTL: Duration = Duration::from_secs(300);
pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS history_batches(id TEXT PRIMARY KEY,generation INTEGER NOT NULL CHECK(generation>0),line INTEGER NOT NULL CHECK(line>0),state TEXT NOT NULL CHECK(state IN('prepared','submitted','cancelled','expired')),plan TEXT NOT NULL CHECK(length(plan)<=65536)) STRICT;";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryRange {
    pub after: Option<InboxCursor>,
    pub limit: usize,
    pub targets: Vec<String>,
}
impl HistoryRange {
    fn validate(&self, realm: &str) -> Result<()> {
        if !(1..=32).contains(&self.limit)
            || self.targets.is_empty()
            || self.targets.len() > MAX_TARGETS
            || self.targets.iter().any(|t| !crate::model::uuid(t))
            || self.targets.iter().collect::<HashSet<_>>().len() != self.targets.len()
        {
            return Err(Error::Invalid);
        }
        if let Some(after) = &self.after {
            cursor(after, realm)?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryDelivery {
    pub id: String,
    pub message_id: String,
    pub target_id: String,
    pub rule_id: String,
    pub share: Share,
}
/// No original SMS, endpoint, credentials or payload fingerprint is returned.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryPreview {
    pub id: String,
    pub confirmation_id: String,
    pub generation: u64,
    pub line: LineEpoch,
    pub range: HistoryRange,
    pub cursor: InboxCursor,
    pub scanned: usize,
    pub skipped_messages: usize,
    pub already_queued_or_recorded: usize,
    pub deliveries: Vec<HistoryDelivery>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedPlan {
    preview: HistoryPreview,
    digests: Vec<[u8; 32]>,
    #[cfg(feature = "smtp-production-candidate")]
    prepared_unix_seconds: i64,
}
/// Early format-3 prototypes had no confirmation challenge. Preserve their
/// receipts, expire their previews on open, and reject every old wire commit.
#[cfg(not(feature = "smtp-production-candidate"))]
pub(super) fn ensure_confirmation_ids(conn: &mut Connection) -> Result<()> {
    let mut rows = Vec::new();
    {
        let mut query = conn.prepare("SELECT id,plan FROM history_batches LIMIT 65")?;
        for row in query.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            rows.push(row?);
        }
    }
    if rows.len() > MAX_BATCHES as usize {
        return Err(Error::Storage);
    }
    let tx = conn.transaction()?;
    for (id, raw) in rows {
        if raw.len() > 65536 {
            return Err(Error::Storage);
        }
        let mut value: serde_json::Value = serde_json::from_str(&raw)?;
        let preview = value
            .get_mut("preview")
            .and_then(|v| v.as_object_mut())
            .ok_or(Error::Storage)?;
        if !preview.contains_key("confirmation_id") {
            preview.insert("confirmation_id".into(), Uuid::new_v4().to_string().into());
            let saved: SavedPlan = serde_json::from_value(value)?;
            if saved.preview.id != id
                || saved.preview.deliveries.len() != saved.digests.len()
                || saved.digests.len() > MAX_HISTORY_DELIVERIES
            {
                return Err(Error::Storage);
            }
            let raw = serde_json::to_string(&saved)?;
            if raw.len() > 65536 {
                return Err(Error::Storage);
            }
            tx.execute(
                "UPDATE history_batches SET plan=? WHERE id=?",
                params![raw, id],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
#[cfg(feature = "smtp-production-candidate")]
pub(super) fn ensure_confirmation_ids(conn: &mut Connection) -> Result<()> {
    let mut query = conn.prepare("SELECT id,plan FROM history_batches LIMIT 65")?;
    let mut rows = query.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        let id: String = row.get(0)?;
        let raw = zeroize::Zeroizing::new(row.get::<_, String>(1)?);
        if count > MAX_BATCHES || raw.len() > 65536 {
            return Err(Error::Storage);
        }
        let plan: SavedPlan = serde_json::from_str(&raw)?;
        if plan.preview.id != id
            || !crate::model::uuid(&plan.preview.confirmation_id)
            || plan.preview.deliveries.len() != plan.digests.len()
            || plan.digests.len() > MAX_HISTORY_DELIVERIES
            || time::OffsetDateTime::from_unix_timestamp(plan.prepared_unix_seconds).is_err()
        {
            return Err(Error::Storage);
        }
    }
    Ok(())
}
#[derive(Serialize)]
pub struct HistoryReceipt {
    pub preview: HistoryPreview,
    pub state: String,
    pub expires_in_seconds: u64,
    pub receipts: Vec<Receipt>,
    pub missing_receipts: bool,
    pub not_found_is_not_proof: bool,
}

fn live(reader: &Handle, binding: &Binding, expected: Option<LineEpoch>) -> Result<LineEpoch> {
    if reader
        .inbox_event_head()
        .map_err(|_| Error::Storage)?
        .db_epoch
        != binding.realm
    {
        return Err(Error::Stale);
    }
    let view = reader.line_status().map_err(|_| Error::Storage)?;
    if view.status != LineStatus::Ready || expected.is_some_and(|line| line != view.line) {
        return Err(Error::Stale);
    }
    Ok(view.line)
}
fn message(reader: &Handle, id: &str, line: LineEpoch) -> Result<Message> {
    let detail = reader.sms_detail(id.to_owned()).map_err(|_| Error::Stale)?;
    if detail.summary.id != id || detail.summary.line != line {
        return Err(Error::Stale);
    }
    let value = Message {
        id: detail.summary.id,
        line,
        sender: detail.sender,
        timestamp: detail.timestamp,
        body: detail.text,
        duplicate_candidate: detail.summary.duplicate_candidate.is_some(),
        binary: detail.summary.binary,
    };
    value.validate()?;
    Ok(value)
}
fn fingerprint(sms: &Message, request: &PreparedDelivery) -> Result<[u8; 32]> {
    let bytes = zeroize::Zeroizing::new(serde_json::to_vec(&(
        &sms.id,
        sms.line,
        &sms.sender,
        &sms.timestamp,
        &sms.body,
        sms.binary,
        sms.duplicate_candidate,
        request,
    ))?);
    Ok(Sha256::digest(&*bytes).into())
}
impl Queue {
    fn expire_history(&mut self) -> Result<()> {
        let expired: Vec<_> = self
            .history_deadlines
            .iter()
            .filter(|(_, deadline)| Instant::now() >= **deadline)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            self.conn.execute(
                "UPDATE history_batches SET state='expired' WHERE id=? AND state='prepared'",
                [&id],
            )?;
            self.history_deadlines.remove(&id);
        }
        Ok(())
    }
    fn saved_history(&self, binding: &Binding, id: &str) -> Result<Option<(String, SavedPlan)>> {
        self.require_binding(binding)?;
        if !crate::model::uuid(id) {
            return Err(Error::Invalid);
        }
        let saved: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT state,plan FROM history_batches WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        saved
            .map(|(state, plan)| {
                if plan.len() > 65536 {
                    return Err(Error::Storage);
                }
                let plan: SavedPlan = serde_json::from_str(&plan)?;
                if plan.preview.id != id
                    || !crate::model::uuid(&plan.preview.confirmation_id)
                    || plan.digests.len() != plan.preview.deliveries.len()
                    || plan.digests.len() > MAX_HISTORY_DELIVERIES
                {
                    return Err(Error::Storage);
                }
                Ok((state, plan))
            })
            .transpose()
    }
    pub fn history_receipt_owner(
        &mut self,
        auth: &mut Auth,
        token: &str,
        id: &str,
    ) -> Result<Option<HistoryReceipt>> {
        let binding = Binding::from_owner(auth, token)?;
        self.require_binding(&binding)?;
        self.expire_history()?;
        let Some((state, plan)) = self.saved_history(&binding, id)? else {
            return Ok(None);
        };
        let mut receipts = Vec::new();
        let mut missing_receipts = false;
        if state == "submitted" {
            for delivery in &plan.preview.deliveries {
                let seq: Option<i64> = self
                    .conn
                    .query_row("SELECT seq FROM jobs WHERE id=?", [&delivery.id], |r| {
                        r.get(0)
                    })
                    .optional()?;
                if let Some(seq) = seq {
                    let receipt = self
                        .receipts(seq - 1, 1)?
                        .into_iter()
                        .next()
                        .ok_or(Error::Storage)?;
                    if receipt.id != delivery.id {
                        return Err(Error::Storage);
                    }
                    receipts.push(receipt);
                } else {
                    missing_receipts = true;
                }
            }
        }
        binding.check_current(auth)?;
        Ok(Some(HistoryReceipt {
            preview: plan.preview,
            state,
            expires_in_seconds: self
                .history_deadlines
                .get(id)
                .map_or(0, |d| d.saturating_duration_since(Instant::now()).as_secs()),
            receipts,
            missing_receipts,
            not_found_is_not_proof: true,
        }))
    }
    pub fn prepare_history_owner(
        &mut self,
        auth: &mut Auth,
        reader: &Handle,
        token: &str,
        generation: u64,
        id: &str,
        range: HistoryRange,
    ) -> Result<HistoryReceipt> {
        let binding = Binding::from_owner(auth, token)?;
        let meta = self.fence(&binding, generation)?;
        range.validate(&binding.realm)?;
        self.expire_history()?;
        if let Some((_, previous)) = self.saved_history(&binding, id)? {
            if previous.preview.generation != generation
                || serde_json::to_vec(&previous.preview.range)? != serde_json::to_vec(&range)?
            {
                return Err(Error::Conflict);
            }
            return self
                .history_receipt_owner(auth, token, id)?
                .ok_or(Error::Storage);
        }
        if self.probe.is_some() || self.history_deadlines.len() >= 8 {
            return Err(Error::Busy);
        }
        let count: i64 = self
            .conn
            .query_row("SELECT count(*) FROM history_batches", [], |r| r.get(0))?;
        if count >= MAX_BATCHES {
            return Err(Error::Full);
        }
        if range
            .targets
            .iter()
            .any(|id| !meta.policy.targets.iter().any(|t| &t.id == id))
        {
            return Err(Error::Invalid);
        }
        let line = live(reader, &binding, None)?;
        let page = reader
            .inbox(range.after.clone(), range.limit)
            .map_err(|_| Error::Stale)?;
        if page.cursor.db_epoch != binding.realm {
            return Err(Error::Stale);
        }
        let mut preview = HistoryPreview {
            id: id.into(),
            confirmation_id: Uuid::new_v4().to_string(),
            generation,
            line,
            range,
            cursor: page.cursor,
            scanned: page.messages.len(),
            skipped_messages: 0,
            already_queued_or_recorded: 0,
            deliveries: Vec::new(),
        };
        let mut digests = Vec::new();
        #[cfg(feature = "smtp-production-candidate")]
        let prepared_unix_seconds = time::OffsetDateTime::now_utc().unix_timestamp();
        for summary in page.messages {
            if summary.line != line || summary.binary || summary.duplicate_candidate.is_some() {
                preview.skipped_messages += 1;
                continue;
            }
            let sms = message(reader, &summary.id, line)?;
            let before = preview.deliveries.len();
            let mut targets = HashSet::new();
            for rule in &meta.policy.rules {
                if !rule.matches(&sms) {
                    continue;
                }
                for target_id in &rule.targets {
                    if !preview.range.targets.contains(target_id) || !targets.insert(target_id) {
                        continue;
                    }
                    let exists: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE message_id=? AND target_id=? AND (generation=? OR state IN('queued','in_flight','unknown','confirmed')))", params![sms.id,target_id,generation as i64], |r| r.get(0))?;
                    if exists {
                        preview.already_queued_or_recorded += 1;
                        continue;
                    }
                    if preview.deliveries.len() == MAX_HISTORY_DELIVERIES {
                        return Err(Error::Full);
                    }
                    let target = meta
                        .policy
                        .targets
                        .iter()
                        .find(|t| &t.id == target_id)
                        .ok_or(Error::Storage)?;
                    let job_id = Uuid::new_v4().to_string();
                    #[cfg(feature = "smtp-production-candidate")]
                    let request = crate::model::prepare_with_date(
                        target,
                        rule,
                        &sms,
                        &job_id,
                        time::OffsetDateTime::from_unix_timestamp(prepared_unix_seconds)
                            .map_err(|_| Error::Invalid)?,
                    )?;
                    #[cfg(not(feature = "smtp-production-candidate"))]
                    let request = prepare(target, rule, &sms, &job_id)?;
                    digests.push(fingerprint(&sms, &request)?);
                    preview.deliveries.push(HistoryDelivery {
                        id: job_id,
                        message_id: sms.id.clone(),
                        target_id: target_id.clone(),
                        rule_id: rule.id.clone(),
                        share: rule.share,
                    });
                }
            }
            if preview.deliveries.len() == before {
                preview.skipped_messages += 1;
            }
        }
        live(reader, &binding, Some(line))?;
        if Binding::from_owner(auth, token)? != binding {
            return Err(Error::Forbidden);
        }
        let plan = serde_json::to_string(&SavedPlan {
            preview,
            digests,
            #[cfg(feature = "smtp-production-candidate")]
            prepared_unix_seconds,
        })?;
        if plan.len() > 65536 {
            return Err(Error::Full);
        }
        self.conn.execute(
            "INSERT INTO history_batches(id,generation,line,state,plan) VALUES(?,?,?,'prepared',?)",
            params![id, generation as i64, line.0 as i64, plan],
        )?;
        self.history_deadlines
            .insert(id.into(), Instant::now() + PREVIEW_TTL);
        self.history_receipt_owner(auth, token, id)?
            .ok_or(Error::Storage)
    }
    pub fn cancel_history_owner(
        &mut self,
        auth: &mut Auth,
        token: &str,
        id: &str,
        confirmation_id: &str,
    ) -> Result<HistoryReceipt> {
        let binding = Binding::from_owner(auth, token)?;
        self.require_binding(&binding)?;
        self.expire_history()?;
        let (state, saved) = self.saved_history(&binding, id)?.ok_or(Error::Stale)?;
        if saved.preview.confirmation_id != confirmation_id {
            return Err(Error::Stale);
        }
        if state == "submitted" {
            return Err(Error::Conflict);
        }
        self.conn.execute(
            "UPDATE history_batches SET state='cancelled' WHERE id=? AND state='prepared'",
            [id],
        )?;
        self.history_deadlines.remove(id);
        self.history_receipt_owner(auth, token, id)?
            .ok_or(Error::Storage)
    }
    pub fn commit_history_owner(
        &mut self,
        auth: &mut Auth,
        reader: &Handle,
        token: &str,
        generation: u64,
        confirmation: (&str, &str),
        accept_full_text: bool,
    ) -> Result<HistoryReceipt> {
        let (id, confirmation_id) = confirmation;
        let binding = Binding::from_owner(auth, token)?;
        self.require_binding(&binding)?;
        self.expire_history()?;
        let (state, saved) = self.saved_history(&binding, id)?.ok_or(Error::Stale)?;
        if saved.preview.confirmation_id != confirmation_id {
            return Err(Error::Stale);
        }
        // Lost-reply recovery is read-only even after disable or a new SIM.
        if saved.preview.generation != generation {
            return Err(Error::Conflict);
        }
        if state == "submitted" {
            return self
                .history_receipt_owner(auth, token, id)?
                .ok_or(Error::Storage);
        }
        if state != "prepared" || !self.history_deadlines.contains_key(id) {
            return Err(Error::Stale);
        }
        let meta = self.fence(&binding, generation)?;
        if !meta.enabled {
            return Err(Error::Disabled);
        }
        if self.probe.is_some() {
            return Err(Error::Busy);
        }
        if saved.preview.deliveries.is_empty() {
            return Err(Error::Invalid);
        }
        if !accept_full_text
            && saved
                .preview
                .deliveries
                .iter()
                .any(|d| d.share == Share::FullText)
        {
            return Err(Error::Forbidden);
        }
        let line = live(reader, &binding, Some(saved.preview.line))?;
        let mut requests = Vec::new();
        for (delivery, digest) in saved.preview.deliveries.iter().zip(&saved.digests) {
            let sms = message(reader, &delivery.message_id, line)?;
            let rule = meta
                .policy
                .rules
                .iter()
                .find(|r| r.id == delivery.rule_id)
                .ok_or(Error::Stale)?;
            if !rule.matches(&sms)
                || !rule.targets.contains(&delivery.target_id)
                || rule.share != delivery.share
            {
                return Err(Error::Stale);
            }
            let target = meta
                .policy
                .targets
                .iter()
                .find(|t| t.id == delivery.target_id)
                .ok_or(Error::Stale)?;
            #[cfg(feature = "smtp-production-candidate")]
            let prepared = crate::model::prepare_with_date(
                target,
                rule,
                &sms,
                &delivery.id,
                time::OffsetDateTime::from_unix_timestamp(saved.prepared_unix_seconds)
                    .map_err(|_| Error::Invalid)?,
            )?;
            #[cfg(not(feature = "smtp-production-candidate"))]
            let prepared = prepare(target, rule, &sms, &delivery.id)?;
            if fingerprint(&sms, &prepared)? != *digest {
                return Err(Error::Stale);
            }
            let request = zeroize::Zeroizing::new(serde_json::to_string(&prepared)?);
            requests.push(request);
        }
        live(reader, &binding, Some(line))?;
        if Binding::from_owner(auth, token)? != binding {
            return Err(Error::Forbidden);
        }
        let tx = self.conn.transaction()?;
        let (pending, total): (i64, i64) = tx.query_row(
            "SELECT coalesce(sum(state IN('queued','in_flight','unknown')),0),count(*) FROM jobs",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if pending < 0
            || total < 0
            || pending as usize + requests.len() > MAX_PENDING
            || total as usize + requests.len() > MAX_RECEIPTS
        {
            return Err(Error::Full);
        }
        for (delivery, request) in saved.preview.deliveries.iter().zip(&requests) {
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE message_id=? AND target_id=? AND (generation=? OR state IN('queued','in_flight','unknown','confirmed')))", params![delivery.message_id,delivery.target_id,generation as i64], |r| r.get(0))?;
            if exists {
                return Err(Error::Conflict);
            }
            tx.execute("INSERT INTO jobs(id,message_id,target_id,rule_id,generation,line,state,request) VALUES(?,?,?,?,?,?,'queued',?)", params![delivery.id,delivery.message_id,delivery.target_id,delivery.rule_id,generation as i64,line.0 as i64,request.as_str()])?;
        }
        tx.execute(
            "UPDATE history_batches SET state='submitted' WHERE id=? AND state='prepared'",
            [id],
        )?;
        tx.commit()?;
        self.history_deadlines.remove(id);
        self.history_receipt_owner(auth, token, id)?
            .ok_or(Error::Storage)
    }
    /// Explicit removal of a terminal batch index only. Job receipts and
    /// idempotency remain separate; a new preview always has a new challenge,
    /// so a delayed old confirmation/cancellation cannot act on it.
    pub fn archive_history_owner(
        &mut self,
        auth: &mut Auth,
        token: &str,
        generation: u64,
        id: &str,
        confirmation_id: &str,
    ) -> Result<()> {
        let binding = Binding::from_owner(auth, token)?;
        self.fence(&binding, generation)?;
        self.expire_history()?;
        let (state, saved) = self.saved_history(&binding, id)?.ok_or(Error::Stale)?;
        if saved.preview.confirmation_id != confirmation_id || state == "prepared" {
            return Err(Error::Stale);
        }
        if state == "submitted" {
            for delivery in &saved.preview.deliveries {
                let state: Option<String> = self
                    .conn
                    .query_row("SELECT state FROM jobs WHERE id=?", [&delivery.id], |r| {
                        r.get(0)
                    })
                    .optional()?;
                if !state
                    .as_deref()
                    .is_some_and(|s| matches!(s, "confirmed" | "failed" | "cancelled"))
                {
                    return Err(Error::Busy);
                }
            }
        }
        self.conn
            .execute("DELETE FROM history_batches WHERE id=?", [id])?;
        self.history_deadlines.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn expired_monotonic_preview_only_changes_its_state_without_queueing_or_clock_fallback() {
        let d = tempfile::tempdir().unwrap();
        fs::set_permissions(d.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let b = Binding {
            device_key: [1; 32],
            realm: "00000000-0000-4000-8000-000000000001".into(),
            owner_key: [2; 32],
            owner_generation: 1,
        };
        let mut q = Queue::open(d.path(), b).unwrap();
        let id = Uuid::new_v4().to_string();
        q.conn
            .execute(
                "INSERT INTO history_batches VALUES(?,1,1,'prepared','{}')",
                [&id],
            )
            .unwrap();
        q.history_deadlines
            .insert(id.clone(), Instant::now() - Duration::from_secs(1));
        q.expire_history().unwrap();
        assert_eq!(
            q.conn
                .query_row::<String, _, _>(
                    "SELECT state FROM history_batches WHERE id=?",
                    [id],
                    |r| r.get(0)
                )
                .unwrap(),
            "expired"
        );
        assert!(q.history_deadlines.is_empty());
        assert!(q.receipts(0, 32).unwrap().is_empty());
    }
}
