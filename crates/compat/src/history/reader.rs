// SPDX-License-Identifier: AGPL-3.0-only
//! Archive reader and explicit delegation ledger, independent of the live business schema.
use super::*;
use rustsimhub_domain::{HistoryCursor, HistoryEntity};
use rustsimhub_policy::{allows, Kind, Principal, Scope};
const ACCESS_APP: i64 = 0x48524731;

#[derive(Serialize)]
pub struct HistorySource {
    pub source: String,
    pub entities: Vec<HistoryEntity>,
}
pub struct HistoryService {
    _archive_lock: File,
    _archive_file: File,
    archive: Connection,
    epoch: String,
    archive_stamp: (u64, i64, i64, i64, i64),
    _access_lock: File,
    _access_file: File,
    access: Connection,
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn entity_scope(e: HistoryEntity) -> Scope {
    match e {
        HistoryEntity::Calls => Scope::CallsRead,
        _ => Scope::SmsRead,
    }
}
fn entities(bits: u8) -> Vec<HistoryEntity> {
    [
        HistoryEntity::Sms,
        HistoryEntity::Calls,
        HistoryEntity::Contacts,
    ]
    .into_iter()
    .filter(|e| bits & e.bit() != 0)
    .collect()
}
fn user_valid(user: &str) -> bool {
    !user.is_empty() && user.len() <= 128 && !user.chars().any(char::is_control)
}
impl HistoryService {
    pub fn open(
        archive_path: &Path,
        access_path: &Path,
        device: [u8; 32],
        realm: &str,
    ) -> Result<Self> {
        if realm.len() != 36 {
            return Err(Error::Forbidden);
        }
        let archive_lock = archive_lock(archive_path, false)?;
        let mut archive_file = private_file(&archive_path.join("history.sqlite"), false, false)?;
        let archive_stamp = stamp(&archive_file)?;
        if archive_file.metadata()?.len() > MAX_ARCHIVE_BYTES {
            return Err(Error::Bound);
        }
        // The archive remains locked for this reader's lifetime: imports are explicitly offline.
        let epoch = hex(&hash_archive(&mut archive_file)?);
        let archive = db_read(&archive_file)?;
        archive_check(&archive)?;
        let source_count: i64 = archive.query_row(
            "SELECT COUNT(DISTINCT source) FROM history_records",
            [],
            |r| r.get(0),
        )?;
        if source_count > 32 {
            return Err(Error::Bound);
        }
        for source in archive
            .prepare("SELECT DISTINCT source FROM history_records")?
            .query_map([], |r| r.get::<_, String>(0))?
        {
            namespace(&source?)?;
        }
        directory(access_path)?;
        let lock_path = access_path.join(".access.lock");
        let access_lock = private_file(&lock_path, true, !lock_path.try_exists()?)?;
        FileExt::try_lock_exclusive(&access_lock).map_err(|_| Error::Busy)?;
        let path = access_path.join("access.sqlite");
        let new = !path.try_exists()?;
        let access_file = private_file(&path, true, new)?;
        if access_file.metadata()?.len() > 1024 * 1024 {
            return Err(Error::Bound);
        }
        let access = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        access.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;PRAGMA max_page_count=256;")?;
        if new {
            access.execute_batch("BEGIN IMMEDIATE;CREATE TABLE binding(singleton INTEGER PRIMARY KEY CHECK(singleton=1),device BLOB NOT NULL,realm TEXT NOT NULL,archive_epoch TEXT NOT NULL);CREATE TABLE grants(public_key BLOB NOT NULL,generation INTEGER NOT NULL,kind TEXT NOT NULL,source TEXT NOT NULL,legacy_user TEXT,entities INTEGER NOT NULL CHECK(entities BETWEEN 1 AND 7),PRIMARY KEY(public_key,source));PRAGMA application_id=1213351729;PRAGMA user_version=1;")?;
            access.execute(
                "INSERT INTO binding VALUES(1,?1,?2,?3)",
                params![device, realm, epoch],
            )?;
            access.execute_batch("COMMIT;")?;
        }
        if access.query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))? != ACCESS_APP
            || access.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? != 1
        {
            return Err(Error::Schema);
        }
        let bound = access.query_row(
            "SELECT device,realm,archive_epoch FROM binding WHERE singleton=1",
            [],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?;
        if bound != (device.to_vec(), realm.to_owned(), epoch.clone()) {
            return Err(Error::Forbidden);
        }
        access.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA journal_mode=DELETE;PRAGMA synchronous=FULL;")?;
        // Auxiliary rows participate in access control and per-user presentation.
        let mut statement=archive.prepare("SELECT payload,digest FROM history_records WHERE entity IN ('modemdeck_user_lines','modemdeck_user_message_thread_state','modemdeck_user_call_state')")?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            if count > MAX_ROWS {
                return Err(Error::Bound);
            }
            parse_payload(row.get(0)?, row.get(1)?)?;
        }
        drop(rows);
        drop(statement);
        if stamp(&archive_file)? != archive_stamp {
            return Err(Error::Corrupt);
        }
        Ok(Self {
            _archive_lock: archive_lock,
            _archive_file: archive_file,
            archive,
            epoch,
            archive_stamp,
            _access_lock: access_lock,
            _access_file: access_file,
            access,
        })
    }
    fn source_exists(&self, source: &str) -> Result<()> {
        self.unchanged()?;
        namespace(source)?;
        if self
            .archive
            .query_row(
                "SELECT 1 FROM history_records WHERE source=?1 LIMIT 1",
                [source],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .is_none()
        {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    fn unchanged(&self) -> Result<()> {
        if stamp(&self._archive_file)? != self.archive_stamp {
            return Err(Error::Corrupt);
        }
        Ok(())
    }
    pub fn grant(
        &mut self,
        owner: &Principal,
        current: &Principal,
        source: &str,
        user: Option<&str>,
        requested: &[HistoryEntity],
    ) -> Result<()> {
        checked_owner(owner)?;
        self.source_exists(source)?;
        if !current.active
            || current.kind == Kind::Owner
            || current.generation == 0
            || current.generation > i64::MAX as u64
            || requested.is_empty()
            || requested.len() > 3
        {
            return Err(Error::Forbidden);
        }
        if user.is_none() && current.kind != Kind::Nas {
            return Err(Error::Forbidden);
        }
        if user.is_some_and(|u| !user_valid(u)) {
            return Err(Error::Forbidden);
        }
        let mut bits = 0;
        for e in requested {
            if bits & e.bit() != 0 || !allows(current.scopes, entity_scope(*e)) {
                return Err(Error::Forbidden);
            }
            bits |= e.bit();
        }
        if let Some(user) = user {
            let associated:i64=self.archive.query_row("SELECT COUNT(*) FROM history_records WHERE source=?1 AND ((entity='modemdeck_user_lines' AND json_extract(payload,'$.user_id')=?2) OR (entity='contacts' AND json_extract(payload,'$.owner_user_id')=?2))",params![source,user],|r|r.get(0))?;
            if associated == 0 {
                return Err(Error::Forbidden);
            }
        }
        let count: i64 = self
            .access
            .query_row("SELECT COUNT(*) FROM grants", [], |r| r.get(0))?;
        let exists = self
            .access
            .query_row(
                "SELECT 1 FROM grants WHERE public_key=?1 AND source=?2",
                params![current.public_key, source],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .is_some();
        if count >= 128 && !exists {
            return Err(Error::Bound);
        }
        let kind = if current.kind == Kind::Nas {
            "nas"
        } else {
            "client"
        };
        self.access.execute("INSERT INTO grants VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(public_key,source) DO UPDATE SET generation=excluded.generation,kind=excluded.kind,legacy_user=excluded.legacy_user,entities=excluded.entities",params![current.public_key,current.generation as i64,kind,source,user,bits])?;
        self._access_file.sync_all()?;
        Ok(())
    }
    pub fn revoke(&mut self, owner: &Principal, key: [u8; 32], source: &str) -> Result<()> {
        checked_owner(owner)?;
        namespace(source)?;
        self.access.execute(
            "DELETE FROM grants WHERE public_key=?1 AND source=?2",
            params![key, source],
        )?;
        self._access_file.sync_all()?;
        Ok(())
    }
    fn audience(
        &self,
        current: &Principal,
        source: &str,
        entity: HistoryEntity,
    ) -> Result<(Option<String>, String)> {
        self.source_exists(source)?;
        if !current.active || !allows(current.scopes, entity_scope(entity)) {
            return Err(Error::Forbidden);
        }
        let user = if current.kind == Kind::Owner {
            None
        } else {
            let (generation,kind,user,bits)=self.access.query_row("SELECT generation,kind,legacy_user,entities FROM grants WHERE public_key=?1 AND source=?2",params![current.public_key,source],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,u8>(3)?))).optional()?.ok_or(Error::Forbidden)?;
            if generation < 1
                || generation as u64 != current.generation
                || bits & entity.bit() == 0
                || kind
                    != if current.kind == Kind::Nas {
                        "nas"
                    } else {
                        "client"
                    }
                || user.is_none() && current.kind != Kind::Nas
            {
                return Err(Error::Forbidden);
            }
            if user.as_deref().is_some_and(|u| !user_valid(u)) {
                return Err(Error::Corrupt);
            }
            user
        };
        // Cursor is never authority; it must match the independently derived current audience.
        let mut hash = Sha256::new();
        hash.update(b"RustSimHub.HistoryAudience.v1\0");
        hash.update(current.public_key);
        hash.update(current.generation.to_be_bytes());
        hash.update([entity.bit()]);
        hash.update(source.as_bytes());
        hash.update(user.as_deref().unwrap_or("\0owner-or-explicit-nas"));
        Ok((user, hex(&hash.finalize())))
    }
    pub fn sources(&self, current: &Principal) -> Result<Vec<HistorySource>> {
        self.unchanged()?;
        if !current.active {
            return Err(Error::Forbidden);
        }
        let mut result = Vec::new();
        for source in self
            .archive
            .prepare("SELECT DISTINCT source FROM history_records ORDER BY source")?
            .query_map([], |r| r.get::<_, String>(0))?
        {
            let source = source?;
            let mut permitted = Vec::new();
            for e in entities(7) {
                match self.audience(current, &source, e) {
                    Ok(_) => permitted.push(e),
                    Err(Error::Forbidden) => {}
                    Err(e) => return Err(e),
                }
            }
            if !permitted.is_empty() {
                result.push(HistorySource {
                    source,
                    entities: permitted,
                });
            }
        }
        self.unchanged()?;
        Ok(result)
    }
    pub fn page(
        &self,
        current: &Principal,
        source: &str,
        entity: HistoryEntity,
        after: Option<&HistoryCursor>,
        limit: usize,
    ) -> Result<Value> {
        if !(1..=16).contains(&limit) {
            return Err(Error::Bound);
        }
        let (user, audience) = self.audience(current, source, entity)?;
        let after = match after {
            Some(c)
                if c.archive_epoch == self.epoch
                    && c.source == source
                    && c.entity == entity
                    && c.audience == audience
                    && !c.after.is_empty()
                    && c.after.len() <= 19
                    && c.after.bytes().all(|b| b.is_ascii_digit()) =>
            {
                c.after.parse::<i64>().map_err(|_| Error::Forbidden)?
            }
            Some(_) => return Err(Error::Forbidden),
            None => 0,
        };
        let sql=format!("SELECT seq,payload,digest FROM history_records r WHERE r.source=?1 AND r.entity=?2 AND r.seq>?3 AND {} ORDER BY r.seq LIMIT ?5",user_filter(entity));
        let mut stmt = self.archive.prepare(&sql)?;
        let mut rows = stmt.query(params![source, entity.table(), after, user, limit as i64])?;
        let mut messages = Vec::new();
        let mut last = after;
        while let Some(row) = rows.next()? {
            let seq = row.get::<_, i64>(0)?;
            let record = parse_payload(row.get(1)?, row.get(2)?)?;
            let mut entry = summary(seq, entity, &record)?;
            entry["user_state"] = self.user_state(source, user.as_deref(), entity, &record)?;
            messages.push(entry);
            last = seq;
        }
        self.unchanged()?;
        Ok(
            serde_json::json!({"records":messages,"cursor":HistoryCursor{archive_epoch:self.epoch.clone(),audience,source:source.into(),entity,after:last.to_string()},"archived":true}),
        )
    }
    pub fn detail(
        &self,
        current: &Principal,
        source: &str,
        entity: HistoryEntity,
        id: &str,
    ) -> Result<Value> {
        let (user, _) = self.audience(current, source, entity)?;
        if id.is_empty() || id.len() > 19 || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::Forbidden);
        }
        let seq = id.parse::<i64>().map_err(|_| Error::Forbidden)?;
        if seq < 1 {
            return Err(Error::Forbidden);
        }
        let sql=format!("SELECT payload,digest FROM history_records r WHERE r.source=?1 AND r.entity=?2 AND r.seq=?3 AND {}",user_filter(entity));
        let (payload, digest) = self
            .archive
            .query_row(&sql, params![source, entity.table(), seq, user], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .optional()?
            .ok_or(Error::Forbidden)?;
        let record = parse_payload(payload, digest)?;
        let mut result = summary(seq, entity, &record)?;
        if result["deleted"] == true {
            result["deleted_at"] = record.get("deleted_at").cloned().unwrap_or(Value::Null);
            self.unchanged()?;
            return Ok(result);
        }
        let fields: &[&str] = match entity {
            HistoryEntity::Sms => &[
                "sender",
                "recipient",
                "peer",
                "content",
                "type",
                "state",
                "delivery_status",
                "timestamp",
                "created_at",
                "deleted_at",
            ],
            HistoryEntity::Calls => &[
                "direction",
                "remote_number",
                "phase",
                "created_at",
                "updated_at",
                "active_at",
                "ended_at",
                "read_at",
                "end_reason",
                "failure_code",
            ],
            HistoryEntity::Contacts => &["display_name", "avatar", "notes", "is_favorite"],
        };
        for field in fields {
            result[*field] = record.get(*field).cloned().unwrap_or(Value::Null);
        }
        result["user_state"] = self.user_state(source, user.as_deref(), entity, &record)?;
        if user.is_some() && entity == HistoryEntity::Calls {
            result
                .as_object_mut()
                .ok_or(Error::Corrupt)?
                .remove("read_at");
        }
        // Do not return raw SIM identifiers, legacy user IDs, or private endpoint/line identity.
        if entity == HistoryEntity::Contacts {
            let contact = record
                .get("id")
                .and_then(Value::as_str)
                .ok_or(Error::Corrupt)?;
            let mut stmt=self.archive.prepare("SELECT payload,digest FROM history_records WHERE source=?1 AND entity='contact_phones' AND json_extract(payload,'$.contact_id')=?2 LIMIT 17")?;
            let mut rows = stmt.query(params![source, contact])?;
            let mut phones = Vec::new();
            while let Some(row) = rows.next()? {
                let p = parse_payload(row.get(0)?, row.get(1)?)?;
                let mut phone = Map::new();
                for field in [
                    "label",
                    "original_number",
                    "canonical_e164",
                    "region",
                    "is_primary",
                ] {
                    phone.insert(field.into(), p.get(field).cloned().unwrap_or(Value::Null));
                }
                phones.push(Value::Object(phone));
            }
            if phones.len() > 16 {
                return Err(Error::Bound);
            }
            result["phones"] = phones.into();
        }
        if serde_json::to_vec(&result)
            .map_err(|_| Error::Corrupt)?
            .len()
            > 48 * 1024
        {
            return Err(Error::Bound);
        }
        self.unchanged()?;
        Ok(result)
    }
    fn user_state(
        &self,
        source: &str,
        user: Option<&str>,
        entity: HistoryEntity,
        record: &Value,
    ) -> Result<Value> {
        let Some(user) = user else {
            return Ok(Value::Null);
        };
        let state=match entity {
   HistoryEntity::Contacts=>return Ok(Value::Null),
   HistoryEntity::Sms=>self.archive.query_row("SELECT payload,digest FROM history_records WHERE entity='modemdeck_user_message_thread_state' AND source=?1 AND json_extract(payload,'$.user_id')=?2 AND json_extract(payload,'$.line_id')=?3 AND json_extract(payload,'$.peer')=?4 LIMIT 1",params![source,user,record["line_id"].as_str(),record["peer"].as_str()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?))).optional()?,
   HistoryEntity::Calls=>self.archive.query_row("SELECT payload,digest FROM history_records WHERE entity='modemdeck_user_call_state' AND source=?1 AND json_extract(payload,'$.user_id')=?2 AND json_extract(payload,'$.call_id')=?3 LIMIT 1",params![source,user,record["id"].as_str()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Vec<u8>>(1)?))).optional()?,
  };
        let Some((payload, digest)) = state else {
            return Ok(Value::Null);
        };
        let state = parse_payload(payload, digest)?;
        let read = if entity == HistoryEntity::Sms {
            record["id"].as_i64().ok_or(Error::Corrupt)?
                <= state["last_read_sms_id"].as_i64().ok_or(Error::Corrupt)?
        } else {
            state["is_read"].as_i64().ok_or(Error::Corrupt)? != 0
        };
        Ok(
            serde_json::json!({"read":read,"favorite":state["is_favorite"].as_i64().ok_or(Error::Corrupt)?!=0,"marked_unread":if entity==HistoryEntity::Sms {state["marked_unread"].as_i64().ok_or(Error::Corrupt)?!=0}else{false}}),
        )
    }
}
fn stamp(file: &File) -> Result<(u64, i64, i64, i64, i64)> {
    let m = file.metadata()?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.nlink() != 1
        || m.mode() & 0o077 != 0
    {
        return Err(Error::Forbidden);
    }
    Ok((
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    ))
}
fn user_filter(entity: HistoryEntity) -> &'static str {
    match entity {
  HistoryEntity::Contacts=>"(?4 IS NULL OR json_extract(r.payload,'$.owner_user_id')=?4)",
  _=>"(?4 IS NULL OR EXISTS(SELECT 1 FROM history_records u WHERE u.source=r.source AND u.entity='modemdeck_user_lines' AND json_extract(u.payload,'$.user_id')=?4 AND json_extract(u.payload,'$.line_id')=json_extract(r.payload,'$.line_id')))",
 }
}
fn checked_owner(owner: &Principal) -> Result<()> {
    if owner.kind != Kind::Owner || !owner.active || !allows(owner.scopes, Scope::ClientsManage) {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn summary(seq: i64, entity: HistoryEntity, record: &Value) -> Result<Value> {
    if seq < 1 || !record.is_object() {
        return Err(Error::Corrupt);
    }
    let deleted = record.get("deleted_at").is_some_and(|v| !v.is_null());
    Ok(serde_json::json!({"id":seq.to_string(),"entity":entity,"archived":true,"deleted":deleted}))
}
fn parse_payload(payload: String, digest: Vec<u8>) -> Result<Value> {
    if payload.len() > MAX_ROW || Sha256::digest(payload.as_bytes()).as_slice() != digest {
        return Err(Error::Corrupt);
    }
    serde_json::from_str(&payload).map_err(|_| Error::Corrupt)
}
fn hash_archive(file: &mut File) -> Result<[u8; 32]> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut bytes = [0; 32768];
    let mut total = 0;
    loop {
        let n = file.read(&mut bytes)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > MAX_ARCHIVE_BYTES {
            return Err(Error::Bound);
        }
        hash.update(&bytes[..n]);
    }
    Ok(hash.finalize().into())
}
