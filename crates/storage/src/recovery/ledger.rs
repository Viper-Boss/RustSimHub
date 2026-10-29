// SPDX-License-Identifier: AGPL-3.0-only
//! Durable request tombstones. Only trusted Core may reserve/claim; a receipt is
//! not a cryptographic proof, nor permission to bypass the live Auth/Wi-Fi gates.
use super::*;
const MAX_RECEIPTS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryMutation {
    BeginWifi,
    ConfirmWifi,
    CancelWifi,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryReceiptState {
    Prepared,
    Accepted,
    Unknown,
    Ready,
    Saved,
    RolledBack,
    Rejected,
}
#[derive(Clone, PartialEq, Eq)]
pub struct RecoveryReceipt {
    pub realm: String,
    pub grant_id: String,
    pub generation: u64,
    pub request_id: String,
    pub action: RecoveryMutation,
    pub body_digest: [u8; 32],
    pub state: RecoveryReceiptState,
    pub trial_id: Option<String>,
}
impl std::fmt::Debug for RecoveryReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryReceipt(..)")
    }
}
/// No Clone/Deserialize/public constructor. The single writer changes prepared
/// to accepted before returning it. Dropping it never makes a request retryable.
pub struct RecoveryExecutionPermit {
    receipt: RecoveryReceipt,
}
pub enum RecoveryReservation {
    New(RecoveryReceipt),
    Existing(RecoveryReceipt),
}
fn action(a: RecoveryMutation) -> &'static str {
    match a {
        RecoveryMutation::BeginWifi => "begin_wifi",
        RecoveryMutation::ConfirmWifi => "confirm_wifi",
        RecoveryMutation::CancelWifi => "cancel_wifi",
    }
}
fn state(s: RecoveryReceiptState) -> &'static str {
    match s {
        RecoveryReceiptState::Prepared => "prepared",
        RecoveryReceiptState::Accepted => "accepted",
        RecoveryReceiptState::Unknown => "unknown",
        RecoveryReceiptState::Ready => "ready",
        RecoveryReceiptState::Saved => "saved",
        RecoveryReceiptState::RolledBack => "rolled_back",
        RecoveryReceiptState::Rejected => "rejected",
    }
}
fn trial(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn request(value: &str) -> bool {
    uuid(value) && value != "00000000-0000-0000-0000-000000000000"
}
fn valid(r: &RecoveryReceipt) -> bool {
    use RecoveryMutation::*;
    use RecoveryReceiptState::*;
    if !uuid(&r.realm)
        || !uuid(&r.grant_id)
        || !positive(r.generation)
        || !request(&r.request_id)
        || r.trial_id.as_ref().is_some_and(|v| !trial(v))
    {
        return false;
    }
    match r.state {
        Prepared | Accepted => (r.action == BeginWifi) == r.trial_id.is_none(),
        Ready => r.action == BeginWifi && r.trial_id.is_some(),
        Saved => r.action == ConfirmWifi && r.trial_id.is_some(),
        RolledBack => r.trial_id.is_some(),
        Unknown | Rejected => r.action == BeginWifi || r.trial_id.is_some(),
    }
}
fn rows(conn: &Connection) -> Result<Vec<RecoveryReceipt>> {
    let mut q=conn.prepare("SELECT format,realm,grant_id,generation,request_id,action,body_digest,state,trial_id FROM recovery_receipts LIMIT 257")?;
    let records = q.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, Vec<u8>>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
        ))
    })?;
    let mut result = Vec::new();
    for record in records {
        let (format, realm, grant_id, generation, request_id, a, digest, s, trial_id) = record?;
        if format != 1 {
            return Err(Error::Corrupt);
        }
        let action = match a.as_str() {
            "begin_wifi" => RecoveryMutation::BeginWifi,
            "confirm_wifi" => RecoveryMutation::ConfirmWifi,
            "cancel_wifi" => RecoveryMutation::CancelWifi,
            _ => return Err(Error::Corrupt),
        };
        let state = match s.as_str() {
            "prepared" => RecoveryReceiptState::Prepared,
            "accepted" => RecoveryReceiptState::Accepted,
            "unknown" => RecoveryReceiptState::Unknown,
            "ready" => RecoveryReceiptState::Ready,
            "saved" => RecoveryReceiptState::Saved,
            "rolled_back" => RecoveryReceiptState::RolledBack,
            "rejected" => RecoveryReceiptState::Rejected,
            _ => return Err(Error::Corrupt),
        };
        let receipt = RecoveryReceipt {
            realm,
            grant_id,
            generation: u64::try_from(generation).map_err(|_| Error::Corrupt)?,
            request_id,
            action,
            body_digest: digest.try_into().map_err(|_| Error::Corrupt)?,
            state,
            trial_id,
        };
        if !valid(&receipt) {
            return Err(Error::Corrupt);
        }
        result.push(receipt);
    }
    if result.len() > MAX_RECEIPTS {
        return Err(Error::Corrupt);
    }
    Ok(result)
}
fn same_key(a: &RecoveryReceipt, b: &RecoveryReceipt) -> bool {
    a.realm == b.realm
        && a.grant_id == b.grant_id
        && a.generation == b.generation
        && a.request_id == b.request_id
}
fn read(conn: &Connection, key: &RecoveryReceipt) -> Result<RecoveryReceipt> {
    rows(conn)?
        .into_iter()
        .find(|r| same_key(r, key))
        .ok_or(Error::NotFound)
}
pub(super) fn validate(conn: &Connection) -> Result<()> {
    let grants = grants(conn)?;
    for row in rows(conn)? {
        if !grants
            .iter()
            .any(|g| g.id == row.grant_id && row.generation <= g.generation)
        {
            return Err(Error::Corrupt);
        }
    }
    Ok(())
}
pub(crate) fn recover(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        "UPDATE recovery_receipts SET state='unknown' WHERE state IN ('prepared','accepted')",
        [],
    )?;
    Ok(())
}
impl Handle {
    /// Trusted Core has already verified the action and exact body. The digest
    /// is all that persists; no SSID, passphrase, private key or session secret.
    pub fn reserve_recovery_request(
        &self,
        grant: RecoveryGrant,
        request_id: String,
        action: RecoveryMutation,
        body_digest: [u8; 32],
        trial_id: Option<String>,
    ) -> Result<RecoveryReservation> {
        if !request(&request_id) {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            let tx = db.connection.transaction()?;
            let g = checked(&tx, &grant)?;
            let candidate = RecoveryReceipt {
                realm: g.realm,
                grant_id: g.id,
                generation: g.generation,
                request_id,
                action,
                body_digest,
                state: RecoveryReceiptState::Prepared,
                trial_id,
            };
            if !valid(&candidate) {
                return Err(Error::InvalidInput);
            }
            let records = rows(&tx)?;
            if let Some(existing) = records.iter().find(|r| same_key(r, &candidate)) {
                // A begin request learns its trial after execution; compare its
                // immutable inputs, not that later result field.
                if existing.action != candidate.action
                    || existing.body_digest != candidate.body_digest
                    || (candidate.action != RecoveryMutation::BeginWifi
                        && existing.trial_id != candidate.trial_id)
                {
                    return Err(Error::Conflict);
                }
                return Ok(RecoveryReservation::Existing(existing.clone()));
            }
            if records.len() >= MAX_RECEIPTS {
                return Err(Error::Full);
            }
            tx.execute(
                "INSERT INTO recovery_receipts VALUES(1,?,?,?,?,?,?,'prepared',?)",
                params![
                    candidate.realm,
                    candidate.grant_id,
                    sql_number(candidate.generation)?,
                    candidate.request_id,
                    action_text(candidate.action),
                    candidate.body_digest.as_slice(),
                    candidate.trial_id
                ],
            )?;
            tx.commit()?;
            Ok(RecoveryReservation::New(candidate))
        })
    }
    pub fn claim_recovery_request(
        &self,
        grant: RecoveryGrant,
        expected: RecoveryReceipt,
    ) -> Result<RecoveryExecutionPermit> {
        self.call(move|db| {
            let tx=db.connection.transaction()?; let g=checked(&tx,&grant)?;
            if expected.realm!=g.realm || expected.grant_id!=g.id || expected.generation!=g.generation { return Err(Error::Forbidden); }
            let mut r=read(&tx,&expected)?;
            if r!=expected || r.state!=RecoveryReceiptState::Prepared { return Err(Error::Conflict); }
            tx.execute("UPDATE recovery_receipts SET state='accepted' WHERE realm=? AND grant_id=? AND generation=? AND request_id=?",params![r.realm,r.grant_id,sql_number(r.generation)?,r.request_id])?;
            r.state=RecoveryReceiptState::Accepted; tx.commit()?;
            Ok(RecoveryExecutionPermit{receipt:r})
        })
    }
    /// Persist factual evidence even after revocation. This consumes a previously
    /// issued permit; it cannot grant a new operation or skip live Wi-Fi gates.
    pub fn finish_recovery_request(
        &self,
        permit: RecoveryExecutionPermit,
        outcome: RecoveryReceiptState,
        trial_id: Option<String>,
    ) -> Result<RecoveryReceipt> {
        if matches!(
            outcome,
            RecoveryReceiptState::Prepared | RecoveryReceiptState::Accepted
        ) {
            return Err(Error::InvalidInput);
        }
        self.call(move|db| {
            let tx=db.connection.transaction()?; enabled(&tx)?;
            let mut r=read(&tx,&permit.receipt)?;
            if r!=permit.receipt || r.state!=RecoveryReceiptState::Accepted || r.realm!=realm(&tx)? { return Err(Error::Conflict); }
            if r.trial_id.is_some() && r.trial_id!=trial_id { return Err(Error::Conflict); }
            r.state=outcome; r.trial_id=trial_id;
            if !valid(&r) { return Err(Error::InvalidInput); }
            tx.execute("UPDATE recovery_receipts SET state=?,trial_id=? WHERE realm=? AND grant_id=? AND generation=? AND request_id=?",params![state(r.state),r.trial_id,r.realm,r.grant_id,sql_number(r.generation)?,r.request_id])?;
            tx.commit()?; Ok(r)
        })
    }
    pub fn recovery_request(
        &self,
        grant: RecoveryGrant,
        request_id: String,
    ) -> Result<RecoveryReceipt> {
        if !request(&request_id) {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            let g = checked(&db.connection, &grant)?;
            rows(&db.connection)?
                .into_iter()
                .find(|r| {
                    r.realm == g.realm
                        && r.grant_id == g.id
                        && r.generation == g.generation
                        && r.request_id == request_id
                })
                .ok_or(Error::NotFound)
        })
    }
    /// Recent Owner authentication is the Auth caller's duty. A revoked recovery
    /// key alone cannot inspect receipts; the current owner may reconcile them.
    pub fn owner_recovery_request(
        &self,
        principal: Principal,
        expected: RecoveryReceipt,
    ) -> Result<RecoveryReceipt> {
        self.call(move |db| {
            owner(&db.connection, &principal)?;
            enabled(&db.connection)?;
            let g = grants(&db.connection)?
                .into_iter()
                .find(|g| g.id == expected.grant_id && g.owner_id == principal.id)
                .ok_or(Error::Forbidden)?;
            if expected.realm != realm(&db.connection)? || g.realm != expected.realm {
                return Err(Error::Forbidden);
            }
            read(&db.connection, &expected)
        })
    }
}
fn action_text(value: RecoveryMutation) -> &'static str {
    action(value)
}

#[cfg(test)]
mod tests;
