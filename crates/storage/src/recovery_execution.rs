// SPDX-License-Identifier: AGPL-3.0-only
//! Explicit schema9 admission only. No request dispatch, claim, fingerprint key
//! derivation or authorization port is supplied by this module.
use super::*;
const DDL: &str = include_str!("recovery-execution.sql");
const GRANTS: &str = include_str!("recovery-grants.sql");
const MAX_GENERATION: i64 = i64::MAX - 1;
fn uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
type CatalogEntry = (String, String, String, Option<String>);
fn catalog(conn: &Connection) -> Result<Vec<CatalogEntry>> {
    conn.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE tbl_name GLOB 'recovery_*' OR name GLOB 'recovery_*' ORDER BY type,name")?
        .query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?
        .collect::<std::result::Result<_,_>>().map_err(|_|Error::Corrupt)
}
fn check_catalog(conn: &Connection, execution: bool) -> Result<()> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(GRANTS)?;
    if execution {
        expected.execute_batch(DDL)?;
    }
    if catalog(conn)? != catalog(&expected)? {
        return Err(Error::Corrupt);
    }
    Ok(())
}
pub(super) fn validate_source(conn: &Connection) -> Result<()> {
    recovery_grants::validate(conn)?;
    check_catalog(conn, false)
}
#[derive(Clone)]
struct Receipt {
    format: i64,
    realm: String,
    pin: Vec<u8>,
    owner: String,
    owner_generation: i64,
    grant: String,
    grant_generation: i64,
    request: String,
    action: String,
    fingerprint: Vec<u8>,
    state: String,
    trial: Option<String>,
    saved: Option<i64>,
    rolled_back: Option<i64>,
}
impl Receipt {
    fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            format: r.get(0)?,
            realm: r.get(1)?,
            pin: r.get(2)?,
            owner: r.get(3)?,
            owner_generation: r.get(4)?,
            grant: r.get(5)?,
            grant_generation: r.get(6)?,
            request: r.get(7)?,
            action: r.get(8)?,
            fingerprint: r.get(9)?,
            state: r.get(10)?,
            trial: r.get(11)?,
            saved: r.get(12)?,
            rolled_back: r.get(13)?,
        })
    }
    fn valid(&self) -> bool {
        let evidence = match self.state.as_str() {
            "saved" => self.saved == Some(1) && self.rolled_back.is_none(),
            "rolled_back" => self.saved == Some(0) && self.rolled_back == Some(1),
            "reserved" | "claimed" | "starting" | "connecting" | "ready" | "saving"
            | "rejected" | "unknown" => self.saved.is_none() && self.rolled_back.is_none(),
            _ => false,
        };
        self.format == 2
            && uuid(&self.realm)
            && uuid(&self.owner)
            && uuid(&self.grant)
            && uuid(&self.request)
            && (1..=MAX_GENERATION).contains(&self.owner_generation)
            && (1..=MAX_GENERATION).contains(&self.grant_generation)
            && self.pin.len() == 32
            && self.fingerprint.len() == 32
            && matches!(
                self.action.as_str(),
                "begin_wifi" | "confirm_wifi" | "cancel_wifi"
            )
            && self.trial.as_ref().is_none_or(|trial| {
                trial.len() == 64
                    && trial
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            && (self.action == "begin_wifi" || self.trial.is_some())
            && (matches!(self.state.as_str(), "reserved" | "rejected" | "unknown")
                || self.trial.is_some())
            && (!(self.action == "begin_wifi" && self.state == "reserved") || self.trial.is_none())
            && evidence
    }
    fn same_trial(&self, other: &Self) -> bool {
        self.trial.is_some()
            && self.trial == other.trial
            && self.realm == other.realm
            && self.pin == other.pin
            && self.owner == other.owner
            && self.owner_generation == other.owner_generation
            && self.grant == other.grant
            && self.grant_generation == other.grant_generation
    }
}
pub(super) fn validate(conn: &Connection) -> Result<()> {
    if conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))? != 9 {
        return Err(Error::InvalidTransition);
    }
    recovery_grants::validate_catalog(conn)?;
    check_catalog(conn, true)?;
    let meta: Vec<(i64, String, i64, String)> = conn
        .prepare(
            "SELECT singleton,model,grant_format,fingerprint_scheme FROM recovery_execution_meta",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::Corrupt)?;
    if meta
        != vec![(
            1,
            "fixed-ed25519-execution-v2".into(),
            1,
            "identity-hmac-sha256-v2".into(),
        )]
    {
        return Err(Error::Corrupt);
    }
    let rows: Vec<Receipt> = conn
        .prepare("SELECT * FROM recovery_execution_receipts LIMIT 257")?
        .query_map([], Receipt::row)?
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::Corrupt)?;
    if rows.len() > 256
        || rows.iter().filter(|r| r.action != "cancel_wifi").count() > 255
        || rows.iter().any(|r| !r.valid())
    {
        return Err(Error::Corrupt);
    }
    if rows.is_empty() {
        return Ok(());
    }
    let pin: Vec<u8> = conn
        .query_row(
            "SELECT public_key FROM device_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| Error::Corrupt)?;
    for receipt in &rows {
        let (kind, generation): (String, i64) = conn
            .query_row(
                "SELECT kind,generation FROM principals WHERE id=?",
                [&receipt.owner],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| Error::Corrupt)?;
        let (grant_realm,grant_pin,owner,owner_generation,grant_generation):(String,Vec<u8>,String,i64,i64)=conn.query_row(
            "SELECT realm,device_public_key,owner_id,owner_generation,generation FROM recovery_grants WHERE id=?",[&receipt.grant],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).map_err(|_|Error::Corrupt)?;
        if kind != "owner"
            || receipt.owner_generation > generation
            || receipt.grant_generation > grant_generation
            || receipt.pin != pin
            || receipt.pin != grant_pin
            || (receipt.grant_generation == grant_generation
                && (receipt.realm != grant_realm
                    || receipt.owner != owner
                    || receipt.owner_generation != owner_generation))
        {
            return Err(Error::Corrupt);
        }
        if receipt.action != "begin_wifi"
            && !rows
                .iter()
                .any(|r| r.action == "begin_wifi" && receipt.same_trial(r))
        {
            return Err(Error::Corrupt);
        }
        if receipt.state == "saved"
            && receipt.action == "begin_wifi"
            && !rows
                .iter()
                .any(|r| r.action == "confirm_wifi" && r.state == "saved" && receipt.same_trial(r))
        {
            return Err(Error::Corrupt);
        }
        if matches!(receipt.state.as_str(), "saved" | "rolled_back")
            && rows.iter().any(|r| {
                receipt.same_trial(r)
                    && matches!(r.state.as_str(), "saved" | "rolled_back")
                    && r.state != receipt.state
            })
        {
            return Err(Error::Corrupt);
        }
    }
    Ok(())
}
fn migrate_locked(db: &mut Database, backup: &Path) -> Result<()> {
    if db.mode != OpenMode::ExecutionSource {
        return Err(Error::InvalidTransition);
    }
    validate_source(&db.connection)?;
    db.backup(backup)?;
    let tx = db.connection.transaction()?;
    tx.execute_batch(DDL)?;
    validate(&tx)?;
    tx.commit()?;
    db.mode = OpenMode::Execution;
    Ok(())
}
pub(super) fn recover(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute("UPDATE recovery_execution_receipts SET state='unknown' WHERE state IN ('reserved','claimed','starting','connecting','ready','saving')",[])?;
    validate(&tx)?;
    tx.commit()?;
    Ok(())
}
impl Store {
    /// Offline exact7->9 under one writer, completed backup before any format
    /// change. This returns no live store; explicitly open9 after success.
    pub fn migrate_recovery_execution(
        directory: impl AsRef<Path>,
        backup: impl AsRef<Path>,
    ) -> Result<()> {
        let store = Self::start(
            directory.as_ref().to_owned(),
            None,
            OpenMode::ExecutionSource,
        )?;
        let backup = backup.as_ref().to_owned();
        store.handle.call(move |db| migrate_locked(db, &backup))
    }
    pub fn open_recovery_execution(directory: impl AsRef<Path>) -> Result<Self> {
        Self::start(directory.as_ref().to_owned(), None, OpenMode::Execution)
    }
    pub fn restore_recovery_execution(
        backup: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::start(
            destination.as_ref().to_owned(),
            Some(backup.as_ref().to_owned()),
            OpenMode::Execution,
        )
    }
}
#[cfg(test)]
mod tests;
