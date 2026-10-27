// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only diagnostics for the existing legacy-column/history-archive contract.
//! Reports contain static schema names and counts, never row keys or payloads.
use super::*;
use std::collections::BTreeSet;

const MAX_OBJECTS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    InvalidNamespace,
    PrivateSourceRequired,
    OfflineSourceRequired,
    SourceUnreadable,
    SourceCorrupt,
    SourceChanged,
    Busy,
    Bound,
    MissingTable,
    MissingColumns,
    UnsupportedRecords,
    InvalidRecord,
    InvalidText,
    ConflictingRecords,
    ArchiveUnreadable,
    OfflineArchiveRequired,
    PrivateArchiveRequired,
    UnsupportedArchiveFormat,
    ArchiveChanged,
    SourceDestinationOverlap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    NotRequested,
    NotChecked,
    NewArchive,
    Compared,
    Rejected,
}

#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub rows: usize,
    pub valid: usize,
    pub new: usize,
    pub unchanged: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub rejected: usize,
    pub uncompared: usize,
}

#[derive(Debug, Default, Serialize)]
pub struct TableReport {
    pub present: bool,
    pub rows: Option<usize>,
    pub row_count_exact: bool,
    pub missing_columns: Vec<&'static str>,
    pub missing_values: BTreeMap<&'static str, usize>,
    pub counts: Counts,
}

#[derive(Debug, Serialize)]
pub struct SourceInfo {
    pub namespace: Option<String>,
    pub format: &'static str,
    pub column_contract_revision: u8,
    pub sqlite_application_id: Option<i64>,
    pub sqlite_user_version: Option<i64>,
    // The column contract has no authoritative NAS product-version field.
    pub product_version: Option<&'static str>,
    pub snapshot_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ArchiveInfo {
    pub comparison: Comparison,
    pub sqlite_application_id: Option<i64>,
    pub sqlite_user_version: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PreflightReport {
    pub report_format: u8,
    pub read_only: bool,
    pub history_only: bool,
    pub credentials_imported: bool,
    pub hardware_operations: u8,
    pub accepted: bool,
    // None means no destination was supplied: conflict checking is unknown.
    pub eligible_for_history_import: Option<bool>,
    pub contract_scan_complete: bool,
    pub source: SourceInfo,
    pub archive: ArchiveInfo,
    pub tables: BTreeMap<&'static str, TableReport>,
    pub unsupported_tables: usize,
    pub unsupported_known_rows: usize,
    pub unsupported_counts_complete: bool,
    pub excluded_credential_tables: usize,
    pub excluded_credential_rows: usize,
    pub reasons: BTreeMap<Reason, usize>,
}

impl PreflightReport {
    fn new() -> Self {
        Self {
            report_format: 1,
            read_only: true,
            history_only: true,
            credentials_imported: false,
            hardware_operations: 0,
            accepted: false,
            eligible_for_history_import: None,
            contract_scan_complete: false,
            source: SourceInfo {
                namespace: None,
                format: "unknown",
                column_contract_revision: 1,
                sqlite_application_id: None,
                sqlite_user_version: None,
                product_version: None,
                snapshot_sha256: None,
            },
            archive: ArchiveInfo {
                comparison: Comparison::NotRequested,
                sqlite_application_id: None,
                sqlite_user_version: None,
            },
            tables: BTreeMap::new(),
            unsupported_tables: 0,
            unsupported_known_rows: 0,
            unsupported_counts_complete: true,
            excluded_credential_tables: 0,
            excluded_credential_rows: 0,
            reasons: BTreeMap::new(),
        }
    }
    fn reject(&mut self, reason: Reason) {
        *self.reasons.entry(reason).or_default() += 1;
    }
    fn finish(&mut self, compared: bool) {
        self.accepted = self.reasons.is_empty() && self.contract_scan_complete;
        self.eligible_for_history_import = compared.then_some(self.accepted);
    }
}

struct ReadArchive {
    _lock: File,
    file: File,
    db: Connection,
    digest: [u8; 32],
}

impl ReadArchive {
    fn open(path: &Path, info: &mut ArchiveInfo) -> Result<Option<Self>> {
        directory(path)?;
        let target = path.join("history.sqlite");
        offline_file(&target)?;
        match fs::symlink_metadata(&target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Interrupted initialization is not an empty destination.
                if fs::symlink_metadata(path.join("history.sqlite.initializing")).is_ok() {
                    return Err(Error::Conflict);
                }
                info.comparison = Comparison::NewArchive;
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
            Ok(_) => {}
        }
        let lock = archive_lock(path, false)?;
        let mut file = private_file(&target, false, false)?;
        let digest = hash_bounded(&mut file, MAX_ARCHIVE_BYTES)?;
        let db = db_read(&file)?;
        info.sqlite_application_id = Some(db.query_row("PRAGMA application_id", [], |r| r.get(0))?);
        info.sqlite_user_version = Some(db.query_row("PRAGMA user_version", [], |r| r.get(0))?);
        archive_check(&db)?;
        info.comparison = Comparison::Compared;
        Ok(Some(Self {
            _lock: lock,
            file,
            db,
            digest,
        }))
    }
    fn classify(&self, source: &str, c: &Contract, record: &Record) -> Result<bool> {
        let existing = self.db.query_row(
            "SELECT digest,payload FROM history_records WHERE source=?1 AND entity=?2 AND legacy_key=?3",
            params![source, c.table, record.key],
            |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?)),
        ).optional()?;
        match existing {
            None => Ok(false),
            Some((digest, payload)) if digest == record.digest && payload == record.payload => {
                Ok(true)
            }
            Some(_) => Err(Error::Conflict),
        }
    }
    fn unchanged(&mut self) -> Result<()> {
        if hash_bounded(&mut self.file, MAX_ARCHIVE_BYTES)? != self.digest {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}

fn source_reason(error: Error) -> Reason {
    match error {
        Error::Forbidden => Reason::PrivateSourceRequired,
        Error::SnapshotRequired => Reason::OfflineSourceRequired,
        Error::Corrupt => Reason::SourceCorrupt,
        Error::Busy => Reason::Busy,
        Error::Bound => Reason::Bound,
        _ => Reason::SourceUnreadable,
    }
}
fn archive_reason(error: Error) -> Reason {
    match error {
        Error::Forbidden => Reason::PrivateArchiveRequired,
        Error::SnapshotRequired => Reason::OfflineArchiveRequired,
        Error::Schema => Reason::UnsupportedArchiveFormat,
        Error::Busy => Reason::Busy,
        Error::Bound => Reason::Bound,
        _ => Reason::ArchiveUnreadable,
    }
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn row_count(db: &Connection, table: &str) -> Result<usize> {
    let count: i64 = db.query_row(
        &format!(
            "SELECT COUNT(*) FROM (SELECT 1 FROM {} LIMIT {})",
            quoted(table),
            MAX_ROWS + 1
        ),
        [],
        |r| r.get(0),
    )?;
    usize::try_from(count).map_err(|_| Error::Corrupt)
}

fn scan_table(
    snapshot: &Snapshot,
    c: &Contract,
    source: &str,
    archive: Option<&ReadArchive>,
    comparison: Comparison,
    report: &mut PreflightReport,
    table: &mut TableReport,
) -> Result<()> {
    let sql = format!(
        "SELECT {} FROM {} ORDER BY {}",
        c.columns.join(","),
        c.table,
        c.keys.join(",")
    );
    let mut statement = snapshot.db.prepare(&sql)?;
    let mut rows = statement.query([])?;
    // Only keys and canonical payload digests are retained; values never enter
    // the report and duplicate detection is bounded by MAX_ROWS.
    let mut seen = BTreeMap::new();
    while let Some(row) = rows.next()? {
        for (i, name) in c.columns.iter().enumerate() {
            let missing = match row.get_ref(i)? {
                ValueRef::Null => true,
                ValueRef::Text(value) => value.is_empty(),
                _ => false,
            };
            if missing {
                *table.missing_values.entry(*name).or_default() += 1;
            }
        }
        let record = match decode_record(c, row) {
            Ok(v) => v,
            Err(error) => {
                table.counts.rejected += 1;
                report.reject(match error {
                    Error::Bound => Reason::Bound,
                    Error::Corrupt => Reason::InvalidText,
                    _ => Reason::InvalidRecord,
                });
                continue;
            }
        };
        table.counts.valid += 1;
        if let Some(prior) = seen.get(&record.key) {
            if prior == &record.digest {
                table.counts.duplicates += 1;
            } else {
                table.counts.conflicts += 1;
                report.reject(Reason::ConflictingRecords);
            }
            continue;
        }
        seen.insert(record.key.clone(), record.digest);
        if report.archive.comparison == Comparison::Rejected {
            table.counts.uncompared += 1;
        } else if let Some(archive) = archive {
            match archive.classify(source, c, &record) {
                Ok(true) => table.counts.unchanged += 1,
                Ok(false) => table.counts.new += 1,
                Err(Error::Conflict) => {
                    table.counts.conflicts += 1;
                    report.reject(Reason::ConflictingRecords);
                }
                Err(error) => {
                    report.archive.comparison = Comparison::Rejected;
                    report.reject(archive_reason(error));
                    table.counts.uncompared += 1;
                }
            }
        } else if comparison == Comparison::NewArchive {
            table.counts.new += 1;
        } else {
            table.counts.uncompared += 1;
        }
    }
    Ok(())
}

fn analyze(
    snapshot: &Snapshot,
    source: &str,
    archive: Option<&ReadArchive>,
    report: &mut PreflightReport,
) -> Result<()> {
    report.source.sqlite_application_id = Some(snapshot.db.query_row(
        "PRAGMA application_id",
        [],
        |r| r.get(0),
    )?);
    report.source.sqlite_user_version = Some(snapshot.db.query_row(
        "PRAGMA user_version",
        [],
        |r| r.get(0),
    )?);
    let mut catalog = BTreeMap::new();
    let mut statement = snapshot.db.prepare("SELECT name,type,rootpage FROM sqlite_schema WHERE type IN ('table','view') AND name NOT GLOB 'sqlite_*' ORDER BY name LIMIT 129")?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (name, kind, root) = row?;
        if name.len() > 512 || catalog.len() == MAX_OBJECTS {
            return Err(Error::Bound);
        }
        catalog.insert(name, (kind, root));
    }
    let mut total = 0usize;
    let mut schema_matches = true;
    for c in CONTRACTS {
        let mut table = TableReport::default();
        match catalog.remove(c.table) {
            Some((kind, root)) if kind == "table" && root > 0 => {}
            object => {
                if object.is_some() {
                    report.unsupported_tables += 1;
                    report.unsupported_counts_complete = false;
                    report.reject(Reason::UnsupportedRecords);
                } else {
                    report.reject(Reason::MissingTable);
                }
                schema_matches = false;
                report.tables.insert(c.table, table);
                continue;
            }
        }
        table.present = true;
        let n = row_count(&snapshot.db, c.table)?;
        table.rows = Some(n);
        table.row_count_exact = n <= MAX_ROWS;
        table.counts.rows = n;
        total = total.checked_add(n).ok_or(Error::Bound)?;
        if total > MAX_ROWS {
            report.tables.insert(c.table, table);
            return Err(Error::Bound);
        }
        let mut columns = snapshot
            .db
            .prepare(&format!("PRAGMA table_info({})", quoted(c.table)))?;
        let names = columns
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<std::result::Result<BTreeSet<_>, _>>()?;
        table.missing_columns = c
            .columns
            .iter()
            .filter(|name| !names.contains(**name))
            .copied()
            .collect();
        if !table.missing_columns.is_empty() {
            schema_matches = false;
            table.counts.rejected = n;
            report.reject(Reason::MissingColumns);
        }
        report.tables.insert(c.table, table);
    }
    for (name, (kind, root)) in catalog {
        let ordinary = kind == "table" && root > 0;
        let n = if ordinary {
            row_count(&snapshot.db, &name)?
        } else {
            0
        };
        total = total.checked_add(n).ok_or(Error::Bound)?;
        if total > MAX_ROWS {
            return Err(Error::Bound);
        }
        if name == "modemdeck_admin_credentials" && ordinary {
            report.excluded_credential_tables += 1;
            report.excluded_credential_rows += n;
        } else {
            report.unsupported_tables += 1;
            report.unsupported_known_rows += n;
            report.unsupported_counts_complete &= ordinary;
            report.reject(Reason::UnsupportedRecords);
        }
    }
    if schema_matches {
        report.source.format = "legacy_nas_sqlite_column_contract_v1";
    }
    for c in CONTRACTS {
        let mut table = report.tables.remove(c.table).unwrap_or_default();
        if table.present && table.missing_columns.is_empty() {
            if let Err(error) = scan_table(
                snapshot,
                c,
                source,
                archive,
                report.archive.comparison,
                report,
                &mut table,
            ) {
                report.tables.insert(c.table, table);
                return Err(error);
            }
        }
        report.tables.insert(c.table, table);
    }
    report.contract_scan_complete = schema_matches;
    Ok(())
}

/// Summarize a private offline snapshot without creating or changing any file.
/// Source-only diagnostics leave destination eligibility unknown. A report is a
/// point-in-time observation, never an authorization token or an import action.
pub fn preflight(input: &Path, source: &str, destination: Option<&Path>) -> PreflightReport {
    let mut report = PreflightReport::new();
    if destination.is_some() {
        report.archive.comparison = Comparison::NotChecked;
    }
    if namespace(source).is_err() {
        report.reject(Reason::InvalidNamespace);
        report.finish(destination.is_some());
        return report;
    }
    report.source.namespace = Some(source.to_owned());
    let mut snapshot = match Snapshot::open_raw(input) {
        Ok(v) => v,
        Err(error) => {
            report.reject(source_reason(error));
            report.finish(destination.is_some());
            return report;
        }
    };
    report.source.snapshot_sha256 =
        Some(snapshot.digest.iter().map(|b| format!("{b:02x}")).collect());
    let mut archive = destination.and_then(|path| {
        if path.join("history.sqlite") == input {
            report.archive.comparison = Comparison::Rejected;
            report.reject(Reason::SourceDestinationOverlap);
            return None;
        }
        match ReadArchive::open(path, &mut report.archive) {
            Ok(v) => v,
            Err(error) => {
                report.archive.comparison = Comparison::Rejected;
                report.reject(archive_reason(error));
                None
            }
        }
    });
    if let Err(error) = analyze(&snapshot, source, archive.as_ref(), &mut report) {
        report.reject(source_reason(error));
    }
    if snapshot.unchanged().is_err() || offline_file(input).is_err() {
        report.reject(Reason::SourceChanged);
    }
    if let Some(archive) = archive.as_mut() {
        if archive.unchanged().is_err()
            || destination.is_some_and(|p| offline_file(&p.join("history.sqlite")).is_err())
        {
            report.reject(Reason::ArchiveChanged);
        }
    } else if report.archive.comparison == Comparison::NewArchive
        && destination.is_some_and(|p| {
            fs::symlink_metadata(p.join("history.sqlite")).is_ok()
                || fs::symlink_metadata(p.join("history.sqlite.initializing")).is_ok()
                || offline_file(&p.join("history.sqlite")).is_err()
        })
    {
        report.reject(Reason::ArchiveChanged);
    }
    report.finish(destination.is_some());
    report
}

#[cfg(test)]
mod tests;
