// SPDX-License-Identifier: AGPL-3.0-only
//! Opt-in schema 7 administration. Never a network/cryptographic authority.
use super::*;
use rustsimhub_policy::Principal;
use serde::Deserialize;

const DDL: &str = include_str!("recovery-grants.sql");
const MAX_GENERATION: u64 = i64::MAX as u64 - 1;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryGrant {
    pub id: String,
    pub independent_public_key: [u8; 32],
    pub device_public_key: [u8; 32],
    pub realm: String,
    pub owner_id: String,
    #[serde(with = "decimal")]
    pub owner_generation: u64,
    #[serde(with = "decimal")]
    pub generation: u64,
    /// Stored flag, not proof of authority: realm/owner/session must also be current.
    pub active: bool,
    pub max_trial_ms: u32,
}
mod decimal {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
        let value = String::deserialize(d)?;
        let parsed = value.parse::<u64>().map_err(serde::de::Error::custom)?;
        if parsed.to_string() != value {
            return Err(serde::de::Error::custom("noncanonical generation"));
        }
        Ok(parsed)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryChange {
    Grant { max_trial_ms: u32 },
    Revoke,
}
fn uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
fn require_schema(conn: &Connection) -> Result<()> {
    if conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))? != 7 {
        return Err(Error::InvalidTransition);
    }
    Ok(())
}
pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    if conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))? != 6 {
        return Err(Error::InvalidTransition);
    }
    let tx = conn.transaction()?;
    tx.execute_batch(DDL)?;
    validate(&tx)?;
    tx.commit()?;
    Ok(())
}
fn valid_grant(g: &RecoveryGrant) -> bool {
    uuid(&g.id)
        && uuid(&g.realm)
        && uuid(&g.owner_id)
        && (1..=MAX_GENERATION).contains(&g.owner_generation)
        && (1..=MAX_GENERATION).contains(&g.generation)
        && (1..=40000).contains(&g.max_trial_ms)
}
fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RecoveryGrant> {
    fn key(r: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<[u8; 32]> {
        let bytes: Vec<u8> = r.get(index)?;
        bytes.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
    }
    Ok(RecoveryGrant {
        id: r.get(0)?,
        independent_public_key: key(r, 1)?,
        device_public_key: key(r, 2)?,
        realm: r.get(3)?,
        owner_id: r.get(4)?,
        owner_generation: r.get::<_, i64>(5)? as u64,
        generation: r.get::<_, i64>(6)? as u64,
        active: r.get(7)?,
        max_trial_ms: r.get(9)?,
    })
}
fn lookup(conn: &Connection, key: [u8; 32]) -> Result<Option<RecoveryGrant>> {
    let value = conn
        .query_row(
            "SELECT * FROM recovery_grants WHERE independent_public_key=?",
            [key.as_slice()],
            row,
        )
        .optional()?;
    if value.as_ref().is_some_and(|g| !valid_grant(g)) {
        return Err(Error::Corrupt);
    }
    Ok(value)
}
pub(super) fn require_database(db: &Database) -> Result<()> {
    let version = db
        .connection
        .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))?;
    match (db.mode, version) {
        (OpenMode::Recovery, 7) => Ok(()),
        #[cfg(feature = "recovery-execution")]
        (OpenMode::Execution, 9) => Ok(()),
        _ => Err(Error::InvalidTransition),
    }
}
pub(super) fn validate(conn: &Connection) -> Result<()> {
    require_schema(conn)?;
    validate_catalog(conn)
}
pub(super) fn validate_catalog(conn: &Connection) -> Result<()> {
    // Compare actual constraints, STRICT flags, indices and foreign keys to the exact DDL.
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(DDL)?;
    for name in ["recovery_grants", "recovery_grant_receipts"] {
        let sql = |c: &Connection| -> rusqlite::Result<String> {
            c.query_row(
                "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?",
                [name],
                |r| r.get(0),
            )
        };
        if sql(conn).map_err(|_| Error::Corrupt)? != sql(&expected)? {
            return Err(Error::Corrupt);
        }
    }
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE tbl_name IN ('recovery_grants','recovery_grant_receipts') AND type IN ('trigger','index') AND sql IS NOT NULL)", [], |r| r.get::<_, bool>(0))? {
        return Err(Error::Corrupt);
    }
    if conn.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(Error::Corrupt);
    }
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM recovery_grants g JOIN principals p ON g.independent_public_key=p.public_key)", [], |r| r.get::<_, bool>(0))? {
        return Err(Error::Corrupt);
    }
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM recovery_grants g JOIN principals p ON g.owner_id=p.id WHERE p.kind!='owner')", [], |r| r.get::<_, bool>(0))? {
        return Err(Error::Corrupt);
    }
    let grants: Vec<_> = conn
        .prepare("SELECT * FROM recovery_grants")?
        .query_map([], row)?
        .collect::<std::result::Result<_, _>>()?;
    let receipt_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM recovery_grant_receipts", [], |r| {
            r.get(0)
        })?;
    if grants.len() > 16
        || receipt_count + grants.iter().filter(|g| g.active).count() as i64 > 256
        || grants.iter().any(|g| !valid_grant(g))
    {
        return Err(Error::Corrupt);
    }
    let mut receipts = conn.prepare(
        "SELECT realm,owner_id,request_id,grant_id,result,action FROM recovery_grant_receipts",
    )?;
    let mut rows = receipts.query([])?;
    while let Some(r) = rows.next()? {
        let realm: String = r.get(0)?;
        let owner: String = r.get(1)?;
        let request: String = r.get(2)?;
        let id: String = r.get(3)?;
        let result: String = r.get(4)?;
        let action: String = r.get(5)?;
        let g: RecoveryGrant = serde_json::from_str(&result).map_err(|_| Error::Corrupt)?;
        let current = lookup(conn, g.independent_public_key)?.ok_or(Error::Corrupt)?;
        if !uuid(&realm)
            || !uuid(&owner)
            || !uuid(&request)
            || g.id != id
            || !valid_grant(&g)
            || current.id != g.id
            || g.generation > current.generation
            || g.active != (action == "grant")
        {
            return Err(Error::Corrupt);
        }
    }
    Ok(())
}
fn context(conn: &Connection) -> Result<(String, [u8; 32])> {
    let realm = conn.query_row("SELECT db_epoch FROM meta WHERE singleton=1", [], |r| {
        r.get(0)
    })?;
    let key: Vec<u8> = conn.query_row(
        "SELECT public_key FROM device_identity WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    Ok((realm, key.try_into().map_err(|_| Error::Corrupt)?))
}
/// Fixed-width LE fields and length-prefixed UTF-8; this is a dedupe digest, not a proof.
fn digest(
    owner: &Principal,
    realm: &str,
    pin: &[u8; 32],
    key: &[u8; 32],
    expected: u64,
    change: RecoveryChange,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"RustSimHub.RecoveryGrant.Request.v1\0");
    h.update([if matches!(change, RecoveryChange::Grant { .. }) {
        1
    } else {
        2
    }]);
    h.update(key);
    h.update(
        match change {
            RecoveryChange::Grant { max_trial_ms } => max_trial_ms,
            _ => 0,
        }
        .to_le_bytes(),
    );
    h.update(expected.to_le_bytes());
    for value in [owner.id.as_str(), realm] {
        h.update((value.len() as u32).to_le_bytes());
        h.update(value.as_bytes());
    }
    h.update(owner.generation.to_le_bytes());
    h.update(pin);
    h.finalize().into()
}
impl Store {
    /// Explicit offline upgrade only, with a completed Backup API snapshot under the same
    /// single-writer lock. Default open/restore still reject schema 7, even with the feature.
    pub fn migrate_recovery_grants(
        directory: impl AsRef<Path>,
        backup: impl AsRef<Path>,
    ) -> Result<Self> {
        let store = Self::open(directory)?;
        let backup = backup.as_ref().to_owned();
        store.handle.call(move |db| {
            db.backup(&backup)?;
            migrate(&mut db.connection)?;
            db.mode = OpenMode::Recovery;
            Ok(())
        })?;
        Ok(store)
    }
    /// Does not migrate schema 6 or silently initialize an absent database.
    pub fn open_recovery_grants(directory: impl AsRef<Path>) -> Result<Self> {
        Self::start(directory.as_ref().to_owned(), None, OpenMode::Recovery)
    }
    pub fn restore_recovery_grants(
        backup: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::start(
            destination.as_ref().to_owned(),
            Some(backup.as_ref().to_owned()),
            OpenMode::Recovery,
        )
    }
}
pub(super) fn check_current(conn: &Connection, expected: &RecoveryGrant) -> Result<RecoveryGrant> {
    let current = lookup(conn, expected.independent_public_key)?.ok_or(Error::Forbidden)?;
    let (realm, pin) = context(conn)?;
    let owner_matches: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM principals WHERE id=? AND kind='owner' AND active=1 AND generation=?)",
                params![current.owner_id, current.owner_generation as i64], |r| r.get(0),
            )?;
    let owners: i64 = conn.query_row(
        "SELECT COUNT(*) FROM principals WHERE kind='owner' AND active=1",
        [],
        |r| r.get(0),
    )?;
    if !current.active
        || &current != expected
        || current.realm != realm
        || current.device_public_key != pin
        || !owner_matches
        || owners != 1
    {
        return Err(Error::Forbidden);
    }

    Ok(current)
}
impl Handle {
    /// Internal trusted-verifier lookup. A stored row is not a session proof;
    /// the caller must first admit the actual transport in the same Auth. This
    /// does not expose an unauthenticated management command or alter grants.
    pub fn recovery_grant_for_trusted_verifier(
        &self,
        key: [u8; 32],
        generation: u64,
    ) -> Result<RecoveryGrant> {
        self.call(move |db| {
            require_database(db)?;
            let tx = db.connection.transaction()?;
            let grant = lookup(&tx, key)?.ok_or(Error::Forbidden)?;
            if grant.generation != generation {
                return Err(Error::Forbidden);
            }
            let current = check_current(&tx, &grant)?;
            tx.commit()?;
            Ok(current)
        })
    }
    /// Read-only authorization gate on the single writer, not a network proof.
    /// A historical grant-management receipt is never sufficient authority.
    pub fn check_recovery_grant(&self, expected: RecoveryGrant) -> Result<RecoveryGrant> {
        self.call(move |db| {
            require_database(db)?;
            let tx = db.connection.transaction()?;
            let current = check_current(&tx, &expected)?;
            tx.commit()?;
            Ok(current)
        })
    }
    /// Auth supplies its existing Ed25519 verifier. Storage has no second crypto implementation.
    pub fn check_recovery_key_material(&self, valid: fn(&[u8; 32]) -> bool) -> Result<()> {
        self.call(move |db| {
            if db
                .connection
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))?
                == 6
            {
                return Ok(());
            }
            require_database(db)?;
            let (.., pin) = context(&db.connection)?;
            let keys = db
                .connection
                .prepare("SELECT * FROM recovery_grants")?
                .query_map([], row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if keys.iter().any(|g| {
                !valid(&g.independent_public_key)
                    || g.independent_public_key == pin
                    || g.device_public_key != pin
            }) {
                return Err(Error::Corrupt);
            }
            Ok(())
        })
    }
    pub fn recovery_grant(&self, owner: Principal, key: [u8; 32]) -> Result<Option<RecoveryGrant>> {
        self.call(move |db| {
            require_database(db)?;
            auth::checked_owner(&db.connection, &owner)?;
            lookup(&db.connection, key)
        })
    }
    pub fn recovery_grant_request(
        &self,
        owner: Principal,
        request: String,
    ) -> Result<Option<RecoveryGrant>> {
        if !uuid(&request) {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            require_database(db)?;
            auth::checked_owner(&db.connection, &owner)?;
            let (realm, _) = context(&db.connection)?;
            let json: Option<String> = db.connection.query_row("SELECT result FROM recovery_grant_receipts WHERE realm=? AND owner_id=? AND request_id=?",
                params![realm, owner.id, request], |r| r.get(0)).optional()?;
            json.map(|json| serde_json::from_str(&json).map_err(|_| Error::Corrupt)).transpose()
        })
    }
    pub fn change_recovery_grant(
        &self,
        owner: Principal,
        key: [u8; 32],
        expected: u64,
        request: String,
        change: RecoveryChange,
    ) -> Result<RecoveryGrant> {
        if !uuid(&request)
            || expected > MAX_GENERATION
            || matches!(change, RecoveryChange::Grant { max_trial_ms } if !(1..=40000).contains(&max_trial_ms))
        {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            require_database(db)?;
            let tx = db.connection.transaction()?;
            auth::checked_owner(&tx, &owner)?;
            let (realm, pin) = context(&tx)?;
            let fingerprint = digest(&owner, &realm, &pin, &key, expected, change);
            let receipt: Option<(Vec<u8>, String)> = tx.query_row("SELECT payload_digest,result FROM recovery_grant_receipts WHERE realm=? AND owner_id=? AND request_id=?",
                params![realm, owner.id, request], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            if let Some((prior, json)) = receipt {
                if prior.as_slice() != fingerprint { return Err(Error::Conflict); }
                return serde_json::from_str(&json).map_err(|_| Error::Corrupt);
            }
            if auth::lookup_principal(&tx, &key)?.is_some() || pin == key { return Err(Error::Forbidden); }
            let prior = lookup(&tx, key)?;
            if prior.as_ref().map_or(0, |g| g.generation) != expected { return Err(Error::Conflict); }
            let mut result = match change {
                RecoveryChange::Grant { max_trial_ms } => {
                    if let Some(g) = &prior {
                        if g.active && g.realm == realm && g.owner_id == owner.id && g.owner_generation == owner.generation {
                            return Err(Error::Conflict);
                        }
                    }
                    if expected >= MAX_GENERATION - 1 { return Err(Error::Full); }
                    let count: i64 = tx.query_row("SELECT COUNT(*) FROM recovery_grants", [], |r| r.get(0))?;
                    if prior.is_none() && count >= 16 { return Err(Error::Full); }
                    RecoveryGrant { id: prior.as_ref().map(|g| g.id.clone()).unwrap_or_else(|| Uuid::new_v4().to_string()),
                        independent_public_key: key, device_public_key: pin, realm: realm.clone(), owner_id: owner.id.clone(),
                        owner_generation: owner.generation, generation: expected + 1, active: true, max_trial_ms }
                },
                RecoveryChange::Revoke => {
                    let mut g = prior.clone().ok_or(Error::NotFound)?;
                    if !g.active { return Ok(g); }
                    // Saturation can never leave an active grant usable.
                    g.generation = g.generation.saturating_add(1).min(MAX_GENERATION);
                    g.active = false;
                    g
                }
            };
            let receipts: i64 = tx.query_row("SELECT COUNT(*) FROM recovery_grant_receipts", [], |r| r.get(0))?;
            let active: i64 = tx.query_row("SELECT COUNT(*) FROM recovery_grants WHERE active=1", [], |r| r.get(0))?;
            let after_active = active - i64::from(prior.as_ref().is_some_and(|g| g.active)) + i64::from(result.active);
            if receipts + 1 + after_active > 256 { return Err(Error::Full); }
            tx.execute("INSERT INTO recovery_grants VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'wifi.recovery',?9,1) ON CONFLICT(independent_public_key) DO UPDATE SET device_public_key=excluded.device_public_key,realm=excluded.realm,owner_id=excluded.owner_id,owner_generation=excluded.owner_generation,generation=excluded.generation,active=excluded.active,max_trial_ms=excluded.max_trial_ms",
                params![result.id,key.as_slice(),result.device_public_key.as_slice(),result.realm,result.owner_id,result.owner_generation as i64,result.generation as i64,result.active,result.max_trial_ms])?;
            // Read back the real stored record; no successful receipt before durable commit.
            result = lookup(&tx, key)?.ok_or(Error::Database)?;
            tx.execute("INSERT INTO recovery_grant_receipts VALUES (?,?,?,?,?,?,?)",
                params![realm,owner.id,request,match change { RecoveryChange::Grant { .. } => "grant", _ => "revoke" },fingerprint.as_slice(),result.id,serde_json::to_string(&result).map_err(|_| Error::InvalidInput)?])?;
            tx.commit()?;
            Ok(result)
        })
    }
}

#[cfg(test)]
mod tests;
