// SPDX-License-Identifier: AGPL-3.0-only
//! Durable records, not cryptographic proofs or ordinary login principals.
use super::*;
use rustsimhub_policy::{allows, Kind, Principal, Scope};

const MAX_GRANTS: usize = 32;
const MAX_AUDIT: usize = 256;

#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryKey {
    pub profile: String,
    pub public_key: Vec<u8>,
}
impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey(..)")
    }
}
impl RecoveryKey {
    /// Shape only. The Auth security adapter must validate the approved profile/key.
    pub fn validate(&self) -> Result<()> {
        if self.profile.is_empty()
            || self.profile.len() > 64
            || !self
                .profile
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || self.public_key.is_empty()
            || self.public_key.len() > 1024
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryLimits {
    pub session_ms: u32,
    pub request_budget: u16,
}
impl RecoveryLimits {
    pub fn validate(self) -> Result<()> {
        if !(15_000..=300_000).contains(&self.session_ms)
            || !(1..=128).contains(&self.request_budget)
        {
            return Err(Error::InvalidInput);
        }
        Ok(())
    }
}

/// A checked durable snapshot, not a bearer or sealed Core capability.
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryGrant {
    pub id: String,
    pub realm: String,
    pub owner_id: String,
    pub owner_key: [u8; 32],
    pub owner_generation: u64,
    pub key: RecoveryKey,
    pub generation: u64,
    pub active: bool,
    pub limits: RecoveryLimits,
}
impl std::fmt::Debug for RecoveryGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryGrant(..)")
    }
}
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryAudit {
    pub seq: u64,
    pub realm: String,
    pub grant_id: String,
    pub generation: u64,
    pub kind: String,
}

fn uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
fn positive(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}
fn sql_number(value: u64) -> Result<i64> {
    if !positive(value) {
        return Err(Error::Corrupt);
    }
    i64::try_from(value).map_err(|_| Error::Corrupt)
}
fn next(value: u64) -> Result<u64> {
    value
        .checked_add(1)
        .filter(|v| positive(*v))
        .ok_or(Error::Conflict)
}
fn realm(conn: &Connection) -> Result<String> {
    let value: String = conn.query_row("SELECT db_epoch FROM meta WHERE singleton=1", [], |r| {
        r.get(0)
    })?;
    if !uuid(&value) {
        return Err(Error::Corrupt);
    }
    Ok(value)
}
fn enabled(conn: &Connection) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != RECOVERY_SCHEMA {
        return Err(Error::InvalidTransition);
    }
    Ok(())
}
fn owner(conn: &Connection, expected: &Principal) -> Result<()> {
    auth::checked_owner(conn, expected)?;
    if !allows(expected.scopes, Scope::NetworkManage)
        || !allows(expected.scopes, Scope::ClientsManage)
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn grants(conn: &Connection) -> Result<Vec<RecoveryGrant>> {
    enabled(conn)?;
    let mut q=conn.prepare("SELECT id,format,realm,owner_id,owner_key,owner_generation,key_profile,public_key,generation,active,session_ms,request_budget FROM recovery_grants ORDER BY id LIMIT 33")?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Vec<u8>>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, Vec<u8>>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, i64>(9)?,
            r.get::<_, u32>(10)?,
            r.get::<_, u16>(11)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (
            id,
            format,
            realm,
            owner_id,
            owner_key,
            owner_generation,
            profile,
            public_key,
            generation,
            active,
            session_ms,
            request_budget,
        ) = row?;
        let owner_generation = u64::try_from(owner_generation).map_err(|_| Error::Corrupt)?;
        let generation = u64::try_from(generation).map_err(|_| Error::Corrupt)?;
        let key = RecoveryKey {
            profile,
            public_key,
        };
        let limits = RecoveryLimits {
            session_ms,
            request_budget,
        };
        if format != 1
            || !uuid(&id)
            || !uuid(&realm)
            || !uuid(&owner_id)
            || !positive(owner_generation)
            || !positive(generation)
            || !matches!(active, 0 | 1)
            || key.validate().is_err()
            || limits.validate().is_err()
        {
            return Err(Error::Corrupt);
        }
        let owner_key = owner_key.try_into().map_err(|_| Error::Corrupt)?;
        result.push(RecoveryGrant {
            id,
            realm,
            owner_id,
            owner_key,
            owner_generation,
            key,
            generation,
            active: active == 1,
            limits,
        });
    }
    if result.len() > MAX_GRANTS {
        return Err(Error::Corrupt);
    }
    Ok(result)
}
fn audit_rows(conn: &Connection) -> Result<Vec<RecoveryAudit>> {
    let mut q = conn.prepare(
        "SELECT seq,realm,grant_id,generation,kind FROM recovery_audit ORDER BY seq LIMIT 257",
    )?;
    let rows = q.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (seq, realm, grant_id, generation, kind) = row?;
        let r = RecoveryAudit {
            seq: u64::try_from(seq).map_err(|_| Error::Corrupt)?,
            realm,
            grant_id,
            generation: u64::try_from(generation).map_err(|_| Error::Corrupt)?,
            kind,
        };
        if !positive(r.seq)
            || !positive(r.generation)
            || !uuid(&r.realm)
            || !uuid(&r.grant_id)
            || !matches!(
                r.kind.as_str(),
                "grant" | "reapprove" | "revoke" | "restore"
            )
        {
            return Err(Error::Corrupt);
        }
        result.push(r);
    }
    if result.len() > MAX_AUDIT {
        return Err(Error::Corrupt);
    }
    Ok(result)
}
fn audit(conn: &Connection, grant: &RecoveryGrant, kind: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO recovery_audit(realm,grant_id,generation,kind) VALUES(?,?,?,?)",
        params![realm(conn)?, grant.id, sql_number(grant.generation)?, kind],
    )?;
    // Only audit history is a ring. Grant tombstones are never evicted/replayed.
    conn.execute("DELETE FROM recovery_audit WHERE seq NOT IN (SELECT seq FROM recovery_audit ORDER BY seq DESC LIMIT 256)",[])?;
    Ok(())
}
fn checked(conn: &Connection, expected: &RecoveryGrant) -> Result<RecoveryGrant> {
    let current = grants(conn)?
        .into_iter()
        .find(|g| g.id == expected.id)
        .ok_or(Error::Forbidden)?;
    if current != *expected || !current.active || current.realm != realm(conn)? {
        return Err(Error::Forbidden);
    }
    let principal = auth::lookup_principal(conn, &current.owner_key)?.ok_or(Error::Forbidden)?;
    if principal.id != current.owner_id
        || principal.generation != current.owner_generation
        || principal.kind != Kind::Owner
    {
        return Err(Error::Forbidden);
    }
    owner(conn, &principal)?;
    Ok(current)
}
pub(super) fn validate(conn: &Connection) -> Result<()> {
    (|| {
        for grant in grants(conn)? {
            let principal =
                auth::lookup_principal(conn, &grant.owner_key)?.ok_or(Error::Corrupt)?;
            if principal.id != grant.owner_id || principal.kind != Kind::Owner {
                return Err(Error::Corrupt);
            }
        }
        audit_rows(conn)?;
        realm(conn)?;
        let foreign = conn.prepare("PRAGMA foreign_key_check")?.exists([])?;
        if foreign {
            return Err(Error::Corrupt);
        }
        Ok(())
    })()
    .map_err(|e| {
        if matches!(e, Error::Database | Error::InvalidTransition) {
            Error::Corrupt
        } else {
            e
        }
    })
}
pub(super) fn quarantine(tx: &Transaction<'_>) -> Result<()> {
    for mut grant in grants(tx)? {
        grant.generation = next(grant.generation)?;
        tx.execute(
            "UPDATE recovery_grants SET active=0,generation=? WHERE id=?",
            params![sql_number(grant.generation)?, grant.id],
        )?;
        audit(tx, &grant, "restore")?;
    }
    Ok(())
}
impl Handle {
    pub fn recovery_enabled(&self) -> Result<bool> {
        self.call(|db| {
            let v: i64 = db
                .connection
                .pragma_query_value(None, "user_version", |r| r.get(0))?;
            Ok(v == RECOVERY_SCHEMA)
        })
    }
    /// Auth must first validate the recovery key with the approved security adapter.
    /// No public endpoint calls this trusted writer primitive directly.
    pub fn grant_recovery(
        &self,
        owner_principal: Principal,
        key: RecoveryKey,
        limits: RecoveryLimits,
        expected_generation: u64,
    ) -> Result<RecoveryGrant> {
        key.validate()?;
        limits.validate()?;
        if expected_generation > i64::MAX as u64 {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            let tx=db.connection.transaction()?; enabled(&tx)?; owner(&tx,&owner_principal)?;
            let ordinary: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM principals WHERE public_key=? UNION ALL SELECT 1 FROM device_identity WHERE public_key=?)",params![key.public_key,key.public_key],|r|r.get(0))?;
            if ordinary { return Err(Error::Forbidden); }
            let rows=grants(&tx)?;
            let existing=rows.iter().find(|g|g.key==key);
            let (id,generation,kind)=if let Some(g)=existing {
                if expected_generation==0 || g.generation!=expected_generation || g.active || g.owner_id!=owner_principal.id { return Err(Error::Conflict); }
                (g.id.clone(),next(g.generation)?,"reapprove")
            } else {
                if expected_generation!=0 { return Err(Error::Conflict); }
                if rows.len()>=MAX_GRANTS { return Err(Error::Full); }
                (Uuid::new_v4().to_string(),1,"grant")
            };
            let grant=RecoveryGrant{id,realm:realm(&tx)?,owner_id:owner_principal.id,owner_key:owner_principal.public_key,owner_generation:owner_principal.generation,key,generation,active:true,limits};
            if existing.is_some() {
                tx.execute("UPDATE recovery_grants SET realm=?,owner_generation=?,generation=?,active=1,session_ms=?,request_budget=? WHERE id=?",params![grant.realm,sql_number(grant.owner_generation)?,sql_number(grant.generation)?,limits.session_ms,limits.request_budget,grant.id])?;
            } else {
                tx.execute("INSERT INTO recovery_grants VALUES(?,1,?,?,?,?,?,?,?,1,?,?)",params![grant.id,grant.realm,grant.owner_id,grant.owner_key.as_slice(),sql_number(grant.owner_generation)?,grant.key.profile,grant.key.public_key,sql_number(grant.generation)?,limits.session_ms,limits.request_budget])?;
            }
            audit(&tx,&grant,kind)?; tx.commit()?; Ok(grant)
        })
    }
    pub fn recovery_grants(&self, owner_principal: Principal) -> Result<Vec<RecoveryGrant>> {
        self.call(move |db| {
            owner(&db.connection, &owner_principal)?;
            grants(&db.connection)
        })
    }
    pub fn recovery_audit(&self, owner_principal: Principal) -> Result<Vec<RecoveryAudit>> {
        self.call(move |db| {
            enabled(&db.connection)?;
            owner(&db.connection, &owner_principal)?;
            audit_rows(&db.connection)
        })
    }
    pub fn revoke_recovery(
        &self,
        owner_principal: Principal,
        id: String,
        expected_generation: u64,
    ) -> Result<RecoveryGrant> {
        if !uuid(&id) || !positive(expected_generation) {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            owner(&tx, &owner_principal)?;
            let mut g = grants(&tx)?
                .into_iter()
                .find(|g| g.id == id)
                .ok_or(Error::NotFound)?;
            if g.owner_id != owner_principal.id {
                return Err(Error::Forbidden);
            }
            if g.generation != expected_generation {
                return Err(Error::Conflict);
            }
            if g.active {
                g.generation = next(g.generation)?;
                g.active = false;
                tx.execute(
                    "UPDATE recovery_grants SET active=0,generation=? WHERE id=?",
                    params![sql_number(g.generation)?, id],
                )?;
                audit(&tx, &g, "revoke")?;
            }
            tx.commit()?;
            Ok(g)
        })
    }
    /// Recheck every durable binding on the single writer. This is not session proof.
    pub fn check_recovery_grant(&self, expected: RecoveryGrant) -> Result<RecoveryGrant> {
        self.call(move |db| checked(&db.connection, &expected))
    }
}

#[cfg(test)]
mod tests;
