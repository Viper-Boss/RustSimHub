// SPDX-License-Identifier: AGPL-3.0-only
use super::*;
use serde::Deserialize;

// Keep display metadata separate from the delivery ledger and its replay fences.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS receipt_display (
 seq INTEGER PRIMARY KEY REFERENCES jobs(seq) ON DELETE CASCADE,
 recorded_at INTEGER,
 hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN (0,1))
) STRICT;
CREATE TRIGGER IF NOT EXISTS record_display_time AFTER INSERT ON jobs BEGIN
 INSERT INTO receipt_display(seq,recorded_at) VALUES(NEW.seq,unixepoch());
END;";

pub(super) fn ensure(connection: &Connection) -> Result<()> {
    connection.execute_batch(SCHEMA)?;
    Ok(())
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogFilter {
    pub state: Option<String>,
    pub kind: Option<String>,
    pub target: Option<String>,
}

impl LogFilter {
    fn validate(&self) -> Result<()> {
        if self
            .state
            .as_deref()
            .is_some_and(|s| !matches!(s, "confirmed" | "failed" | "cancelled" | "pending"))
            || self
                .kind
                .as_deref()
                .is_some_and(|s| !matches!(s, "sms" | "test"))
            || self.target.as_deref().is_some_and(|s| !model::uuid(s))
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }
}

#[derive(Serialize)]
pub struct LogRow {
    #[serde(flatten)]
    pub receipt: Receipt,
    pub line: u64,
    pub recorded_at: Option<i64>,
}

#[derive(Serialize)]
pub struct LogPage {
    pub records: Vec<LogRow>,
    pub through: String,
    pub removable: u32,
}

const FILTER: &str = "
j.seq<=?1 AND COALESCE(d.hidden,0)=0
AND (?2 IS NULL OR j.state=?2 OR (?2='pending' AND j.state IN ('queued','in_flight','unknown')))
AND (?3 IS NULL OR j.purpose=?3)
AND (?4 IS NULL OR j.target_id=?4)";
const TERMINAL: &str = "j.state IN ('confirmed','failed','cancelled')";

impl Queue {
    pub fn log_page(
        &self,
        filter: LogFilter,
        before: Option<i64>,
        limit: usize,
    ) -> Result<LogPage> {
        filter.validate()?;
        if before.is_some_and(|seq| seq <= 0) || !(1..=32).contains(&limit) {
            return Err(Error::Invalid);
        }
        let through: i64 =
            self.conn
                .query_row("SELECT COALESCE(MAX(seq),0) FROM jobs", [], |r| r.get(0))?;
        let removable: u32 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM jobs j LEFT JOIN receipt_display d ON d.seq=j.seq WHERE {FILTER} AND {TERMINAL}"),
            params![through,filter.state,filter.kind,filter.target],
            |row| row.get(0),
        )?;
        let mut query = self.conn.prepare(&format!(
            "SELECT j.seq,j.id,j.message_id,j.target_id,j.rule_id,j.generation,j.state,j.purpose='test',j.line,d.recorded_at
             FROM jobs j LEFT JOIN receipt_display d ON d.seq=j.seq WHERE {FILTER} ORDER BY j.seq DESC LIMIT ?5"
        ))?;
        let bound = before.map_or(through, |seq| seq - 1);
        let rows = query.query_map(
            params![
                bound,
                filter.state,
                filter.kind,
                filter.target,
                limit as i64
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?.to_string(),
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get(7)?,
                    row.get::<_, i64>(8)?,
                    row.get(9)?,
                ))
            },
        )?;
        let records: Result<Vec<_>> = rows
            .map(|row| {
                let (
                    seq,
                    id,
                    message_id,
                    target_id,
                    rule_id,
                    generation,
                    state,
                    synthetic,
                    line,
                    recorded_at,
                ) = row?;
                Ok(LogRow {
                    receipt: Receipt {
                        seq,
                        id,
                        message_id,
                        target_id,
                        rule_id,
                        generation: u64::try_from(generation).map_err(|_| Error::Storage)?,
                        state: State::parse(&state)?,
                        synthetic,
                    },
                    line: u64::try_from(line).map_err(|_| Error::Storage)?,
                    recorded_at,
                })
            })
            .collect();
        Ok(LogPage {
            records: records?,
            through: through.to_string(),
            removable,
        })
    }

    pub fn clear_log(
        &mut self,
        binding: &Binding,
        filter: LogFilter,
        through: i64,
        only: Option<i64>,
    ) -> Result<usize> {
        self.require_binding(binding)?;
        filter.validate()?;
        if through <= 0 || only.is_some_and(|seq| seq <= 0 || seq > through) {
            return Err(Error::Invalid);
        }
        Ok(self.conn.execute(
            &format!(
                "INSERT INTO receipt_display(seq,hidden)
             SELECT j.seq,1 FROM jobs j LEFT JOIN receipt_display d ON d.seq=j.seq
             WHERE {FILTER} AND {TERMINAL} AND (?5 IS NULL OR j.seq=?5)
             ON CONFLICT(seq) DO UPDATE SET hidden=1"
            ),
            params![through, filter.state, filter.kind, filter.target, only],
        )?)
    }
}
