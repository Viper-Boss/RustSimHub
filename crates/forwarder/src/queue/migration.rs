// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit 3 -> 4 conversion. Only a direct Root parent may admit the child.
//! SQLite writes run as the original data UID; Root never owns its journal.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::fd::AsFd,
    os::unix::net::UnixStream,
};
use zeroize::Zeroizing;

pub(crate) const DIRECTORY: &str = "/var/lib/rustsimhub/rustsimhub-v1/core/state/forwarding";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Permit {
    pub format: u16,
    pub source_sha256: [u8; 32],
    pub backup_sha256: [u8; 32],
    pub deadline_ms: u64,
    pub data_uid: u32,
}
pub(crate) fn boot_ms() -> Result<u64> {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
    Ok(u64::try_from(t.tv_sec).map_err(|_| Error::Invalid)? * 1000
        + u64::try_from(t.tv_nsec).map_err(|_| Error::Invalid)? / 1_000_000)
}
impl Permit {
    fn live(&self) -> Result<()> {
        let now = boot_ms()?;
        if self.format != 1
            || self.data_uid != 986
            || self.data_uid != rustix::process::geteuid().as_raw()
            || rustix::process::getuid().as_raw() != self.data_uid
            || self.source_sha256 != self.backup_sha256
            || self.source_sha256 == [0; 32]
            || self.deadline_ms <= now
            || self.deadline_ms - now > 60_000
        {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
/// Private executable entry, before Tokio/settings/worker initialization. A
/// JSON request alone cannot authorize it: verify kernel credentials FIRST.
pub fn child() -> Result<()> {
    let input = std::io::stdin();
    let peer =
        rustix::net::sockopt::socket_peercred(input.as_fd()).map_err(|_| Error::Forbidden)?;
    let parent = rustix::process::getppid().ok_or(Error::Forbidden)?;
    if peer.uid.as_raw() != 0 || peer.pid != parent || parent.as_raw_nonzero().get() <= 1 {
        return Err(Error::Forbidden);
    }
    // A duplicate of stdin keeps the SAME authenticated socket, not a path.
    let fd = rustix::io::dup(input.as_fd()).map_err(|_| Error::Storage)?;
    let mut channel = UnixStream::from(fd);
    channel.set_read_timeout(Some(Duration::from_secs(5)))?;
    channel.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut size = [0; 4];
    channel.read_exact(&mut size)?;
    let n = u32::from_be_bytes(size) as usize;
    if n == 0 || n > 4096 {
        return Err(Error::Invalid);
    }
    let mut raw = Zeroizing::new(vec![0; n]);
    channel.read_exact(&mut raw)?;
    let permit: Permit = serde_json::from_slice(&raw)?;
    permit.live()?;
    migrate(Path::new(DIRECTORY), &permit, || {
        channel.write_all(b"R")?;
        let mut ack = [0];
        channel.read_exact(&mut ack)?;
        if ack != *b"G" {
            return Err(Error::Forbidden);
        }
        permit.live()
    })?;
    channel.write_all(b"D")?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPlan {
    preview: LegacyPreview,
    digests: Vec<[u8; 32]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPreview {
    id: String,
    #[serde(default)]
    confirmation_id: Option<String>,
    generation: u64,
    line: LineEpoch,
    range: HistoryRange,
    cursor: InboxCursor,
    scanned: usize,
    skipped_messages: usize,
    already_queued_or_recorded: usize,
    deliveries: Vec<HistoryDelivery>,
}
impl LegacyPreview {
    fn current(self) -> HistoryPreview {
        HistoryPreview {
            id: self.id,
            confirmation_id: self
                .confirmation_id
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            generation: self.generation,
            line: self.line,
            range: self.range,
            cursor: self.cursor,
            scanned: self.scanned,
            skipped_messages: self.skipped_messages,
            already_queued_or_recorded: self.already_queued_or_recorded,
            deliveries: self.deliveries,
        }
    }
}
/// Validate ALL old business values before the first mutation. Requests are
/// wrapped as-is; no encoder, clock, new recipient or delivery ID is invoked.
fn convert(conn: &mut Connection, before_commit: impl FnOnce() -> Result<()>) -> Result<()> {
    let app: i64 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if app != APP_ID || version != 3 {
        return Err(Error::Forbidden);
    }
    // Refuse triggers/views/extra tables rather than executing unknown code.
    let mut schema = conn.prepare(
        "SELECT type,name FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let objects = schema
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if objects
        != [
            ("table", "history_batches"),
            ("table", "jobs"),
            ("table", "meta"),
            ("index", "pending"),
            ("table", "requests"),
        ]
        .map(|(t, n)| (t.to_owned(), n.to_owned()))
    {
        return Err(Error::Forbidden);
    }
    drop(schema);
    let integrity: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if integrity != "ok" {
        return Err(Error::Storage);
    }
    let (binding, generation, enabled, cursor, policy): (String, i64, bool, i64, String) = conn
        .query_row(
            "SELECT binding,generation,enabled,cursor,policy FROM meta WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
    if binding.len() > 4096 || policy.len() > 65536 || generation < 0 || cursor < 0 {
        return Err(Error::Invalid);
    }
    let binding: Binding = serde_json::from_str(&binding)?;
    binding.validate()?;
    let policy_raw = Zeroizing::new(policy);
    let policy: Policy = serde_json::from_str(&policy_raw)?;
    policy.validate()?;
    if policy.enabled != enabled
        || policy
            .targets
            .iter()
            .any(|t| matches!(t.destination, Destination::Email { .. }))
    {
        return Err(Error::Invalid);
    }
    let requests:i64=conn.query_row("SELECT count(*) FROM requests WHERE length(id)!=36 OR length(digest)!=32 OR generation<1 OR generation>?",[generation],|r|r.get(0))?;
    if requests != 0
        || conn.query_row::<i64, _, _>("SELECT count(*) FROM requests", [], |r| r.get(0))? > 64
    {
        return Err(Error::Invalid);
    }
    let mut changes = Vec::new();
    {
        let mut query=conn.prepare("SELECT id,attempt_id,generation,line,state,request,message_id,target_id,rule_id,purpose FROM jobs LIMIT 4097")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            if changes.len() >= MAX_RECEIPTS {
                return Err(Error::Full);
            }
            let id: String = row.get(0)?;
            let attempt: Option<String> = row.get(1)?;
            let g: i64 = row.get(2)?;
            let line: i64 = row.get(3)?;
            let state: String = row.get(4)?;
            let request: Option<String> = row.get(5)?;
            let ids = [row.get::<_, String>(6)?, row.get(7)?, row.get(8)?];
            let purpose: String = row.get(9)?;
            if !model::uuid(&id)
                || ids.iter().any(|s| !model::uuid(s))
                || attempt.as_deref().is_some_and(|s| !model::uuid(s))
                || g < 1
                || g > generation
                || line < 1
                || !["sms", "test"].contains(&purpose.as_str())
            {
                return Err(Error::Invalid);
            }
            let state = State::parse(&state)?;
            if matches!(state, State::Queued | State::InFlight | State::Unknown)
                && request.is_none()
            {
                return Err(Error::Invalid);
            }
            let wrapped = request
                .map(|raw| -> Result<Zeroizing<String>> {
                    let raw = Zeroizing::new(raw);
                    if raw.len() > 65536 {
                        return Err(Error::Invalid);
                    }
                    let request: PreparedRequest = serde_json::from_str(&raw)?;
                    let tagged = crate::smtp_candidate::PreparedDeliveryV1::Https { request };
                    tagged.validate()?;
                    // Keep the ORIGINAL object bytes including payload/credentials.
                    let wrapped =
                        Zeroizing::new(format!("{{\"transport\":\"https\",\"request\":{}}}", *raw));
                    if wrapped.len() > 65536 {
                        return Err(Error::Invalid);
                    }
                    Ok(wrapped)
                })
                .transpose()?;
            changes.push((id, wrapped));
        }
    }
    let mut plans = Vec::new();
    {
        let mut query =
            conn.prepare("SELECT id,plan,generation,line FROM history_batches LIMIT 65")?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            if plans.len() >= 64 {
                return Err(Error::Full);
            }
            let id: String = row.get(0)?;
            let raw = Zeroizing::new(row.get::<_, String>(1)?);
            if raw.len() > 65536 {
                return Err(Error::Invalid);
            }
            // Typed parsing rejects duplicate fields too, including early
            // plans whose optional confirmation challenge was absent.
            let plan: LegacyPlan = serde_json::from_str(&raw)?;
            let preview = plan.preview.current();
            if !model::uuid(&id)
                || preview.id != id
                || !model::uuid(&preview.confirmation_id)
                || preview.generation
                    != u64::try_from(row.get::<_, i64>(2)?).map_err(|_| Error::Invalid)?
                || preview.line.0
                    != u64::try_from(row.get::<_, i64>(3)?).map_err(|_| Error::Invalid)?
                || preview.deliveries.len() != plan.digests.len()
                || plan.digests.len() > MAX_HISTORY_DELIVERIES
                || preview.generation > generation as u64
                || preview.cursor.db_epoch != binding.realm
                || model::seq(&preview.cursor.seq).is_err()
                || !(1..=32).contains(&preview.range.limit)
                || preview.range.targets.is_empty()
                || preview.range.targets.len() > MAX_TARGETS
                || preview.range.targets.iter().any(|s| !model::uuid(s))
                || preview.deliveries.iter().any(|d| {
                    [&d.id, &d.message_id, &d.target_id, &d.rule_id]
                        .iter()
                        .any(|s| !model::uuid(s))
                })
            {
                return Err(Error::Invalid);
            }
            let mut value = serde_json::to_value(preview)?;
            // No new confirmation can revive a prepared legacy preview.
            let raw = serde_json::json!({"preview":value.take(),"digests":plan.digests,"prepared_unix_seconds":0});
            let raw = Zeroizing::new(serde_json::to_string(&raw)?);
            if raw.len() > 65536 {
                return Err(Error::Invalid);
            }
            plans.push((id, raw));
        }
    }
    let tx = conn.transaction()?;
    for (id, request) in changes {
        tx.execute(
            "UPDATE jobs SET request=? WHERE id=?",
            params![request.as_deref(), id],
        )?;
    }
    for (id, plan) in plans {
        tx.execute(
            "UPDATE history_batches SET plan=? WHERE id=?",
            params![&*plan, id],
        )?;
    }
    tx.execute(
        "UPDATE jobs SET state='unknown' WHERE state='in_flight'",
        [],
    )?;
    tx.execute(
        "UPDATE history_batches SET state='expired' WHERE state='prepared'",
        [],
    )?;
    tx.pragma_update(None, "user_version", 4)?;
    before_commit()?;
    tx.commit()?;
    Ok(())
}
fn migrate(
    directory: &Path,
    permit: &Permit,
    before_commit: impl FnOnce() -> Result<()>,
) -> Result<()> {
    permit.live()?;
    let inspection = crate::inspect_outbox(directory)?;
    let mut raw = Zeroizing::new(Vec::new());
    File::open(directory.join("outbox.sqlite3"))?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)?;
    if raw.len() > 32 * 1024 * 1024 {
        return Err(Error::Invalid);
    }
    if <[u8; 32]>::from(Sha256::digest(&*raw)) != permit.source_sha256 {
        return Err(Error::Stale);
    }
    if inspection.version().format != 3 || inspection.version().application_id != APP_ID {
        return Err(Error::Forbidden);
    }
    // Keep the inspection's writer fence while opening the same original DB.
    let mut conn = Connection::open_with_flags(
        directory.join("outbox.sqlite3"),
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    conn.busy_timeout(Duration::ZERO)?;
    conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON;")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut ops = 0;
    conn.progress_handler(
        1000,
        Some(move || {
            ops += 1000;
            ops >= 10_000_000 || Instant::now() >= deadline
        }),
    )?;
    // The read transaction from inspection cannot coexist with a DELETE commit.
    // It is explicitly closed while its INDEPENDENT flock remains owned.
    inspection.finish_read()?;
    convert(&mut conn, || {
        permit.live()?;
        before_commit()
    })?;
    File::open(directory)?.sync_all()?;
    drop(conn);
    drop(inspection);
    Ok(())
}

#[cfg(test)]
mod tests;
