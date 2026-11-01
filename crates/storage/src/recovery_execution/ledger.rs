// SPDX-License-Identifier: AGPL-3.0-only
//! Trusted in-process Auth adapter only. Never a wire or authentication API.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
mod observations;
pub use observations::{RecoveryExecutionCompletion, RecoveryExecutionProgress};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryExecutionAction {
    BeginWifi,
    ConfirmWifi,
    CancelWifi,
}
impl RecoveryExecutionAction {
    fn name(self) -> &'static str {
        match self {
            Self::BeginWifi => "begin_wifi",
            Self::ConfirmWifi => "confirm_wifi",
            Self::CancelWifi => "cancel_wifi",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryExecutionState {
    Reserved,
    Claimed,
    Starting,
    Connecting,
    Ready,
    Saving,
    Saved,
    RolledBack,
    Rejected,
    Unknown,
}
impl RecoveryExecutionState {
    fn parse(value: &str) -> Result<Self> {
        Ok(match value {
            "reserved" => Self::Reserved,
            "claimed" => Self::Claimed,
            "starting" => Self::Starting,
            "connecting" => Self::Connecting,
            "ready" => Self::Ready,
            "saving" => Self::Saving,
            "saved" => Self::Saved,
            "rolled_back" => Self::RolledBack,
            "rejected" => Self::Rejected,
            "unknown" => Self::Unknown,
            _ => return Err(Error::Corrupt),
        })
    }
}
/// Snapshot only: no permission, secret body, public constructor or wire enum.
#[derive(Clone)]
pub struct RecoveryExecutionReceipt(Receipt);
impl RecoveryExecutionReceipt {
    pub fn request_id(&self) -> &str {
        &self.0.request
    }
    pub fn trial_id(&self) -> Option<&str> {
        self.0.trial.as_deref()
    }
    pub fn state(&self) -> RecoveryExecutionState {
        RecoveryExecutionState::parse(&self.0.state).expect("validated receipt")
    }
    pub fn action(&self) -> RecoveryExecutionAction {
        match self.0.action.as_str() {
            "begin_wifi" => RecoveryExecutionAction::BeginWifi,
            "confirm_wifi" => RecoveryExecutionAction::ConfirmWifi,
            _ => RecoveryExecutionAction::CancelWifi,
        }
    }
    pub fn settings_saved(&self) -> Option<bool> {
        self.0.saved.map(|n| n == 1)
    }
    pub fn rollback_verified(&self) -> Option<bool> {
        self.0.rolled_back.map(|n| n == 1)
    }
}
pub enum RecoveryExecutionReservation {
    Reserved(StorageRecoveryExecutionReservation),
    Existing(RecoveryExecutionReceipt),
}
impl RecoveryExecutionReservation {
    pub fn receipt(&self) -> &RecoveryExecutionReceipt {
        match self {
            Self::Reserved(r) => r.receipt(),
            Self::Existing(r) => r,
        }
    }
}
/// Private ticket returned ONLY for a newly inserted reservation. Existing
/// receipts never reconstruct it. Retains the original live/deadline fence;
/// losing it conservatively strands Reserved until explicit reconciliation.
/// No Clone/Debug/Serde/public constructor.
pub struct StorageRecoveryExecutionReservation {
    input: Box<TrustedRecoveryExecution>,
    receipt: RecoveryExecutionReceipt,
}
impl StorageRecoveryExecutionReservation {
    pub fn receipt(&self) -> &RecoveryExecutionReceipt {
        &self.receipt
    }
}
/// Auth clones its actual connection cancellation flag and bounded deadline.
/// This trusted source constructor does not authenticate a caller. No payload
/// bool/closure, wall clock, Auth lock, Store callback or await runs on the writer.
pub struct RecoveryExecutionFence {
    live: Arc<AtomicBool>,
    deadline: Instant,
}
impl RecoveryExecutionFence {
    pub fn from_trusted_auth(live: Arc<AtomicBool>, deadline: Instant) -> Self {
        Self { live, deadline }
    }
    fn check(&self) -> Result<()> {
        if !self.live.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
}
/// Source-only adapter metadata. Storage verifies current database authority,
/// not crypto/body authenticity; the private Auth API supplies those checks.
/// No Deserialize/Debug/Clone; never construct this from untrusted JSON.
pub struct TrustedRecoveryExecution {
    grant: RecoveryGrant,
    request: String,
    action: RecoveryExecutionAction,
    fingerprint: [u8; 32],
    trial: Option<String>,
    fence: RecoveryExecutionFence,
}
impl TrustedRecoveryExecution {
    /// Trusted actor may only shorten a writer fence to its ORIGINAL trial
    /// deadline. This cannot extend the originating capability/session.
    pub fn with_deadline_limit(mut self, deadline: Instant) -> Self {
        self.fence.deadline = self.fence.deadline.min(deadline);
        self
    }
    pub fn from_trusted_auth(
        grant: RecoveryGrant,
        request: String,
        action: RecoveryExecutionAction,
        fingerprint: [u8; 32],
        trial: Option<String>,
        fence: RecoveryExecutionFence,
    ) -> Result<Self> {
        if !uuid(&request)
            || match action {
                RecoveryExecutionAction::BeginWifi => trial.is_some(),
                _ => !trial.as_deref().is_some_and(valid_trial),
            }
        {
            return Err(Error::InvalidInput);
        }
        Ok(Self {
            grant,
            request,
            action,
            fingerprint,
            trial,
            fence,
        })
    }
}
/// Returned only after the one reserved->claimed commit. No Clone/Serde/public
/// constructor; dropping it never requeues the claim or marks an effect undone.
pub struct StorageRecoveryExecutionClaim {
    receipt: RecoveryExecutionReceipt,
}
impl StorageRecoveryExecutionClaim {
    pub fn receipt(&self) -> &RecoveryExecutionReceipt {
        &self.receipt
    }
}
fn valid_trial(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn require(db: &Database) -> Result<()> {
    if db.mode != OpenMode::Execution {
        return Err(Error::InvalidTransition);
    }
    validate(&db.connection)
}
fn lookup(conn: &Connection, g: &RecoveryGrant, request: &str) -> Result<Option<Receipt>> {
    Ok(conn.query_row("SELECT * FROM recovery_execution_receipts WHERE realm=? AND grant_id=? AND grant_generation=? AND request_id=?",params![g.realm,g.id,g.generation as i64,request],Receipt::row).optional()?)
}
fn matches(receipt: &Receipt, input: &TrustedRecoveryExecution) -> bool {
    receipt.realm == input.grant.realm
        && receipt.pin == input.grant.device_public_key
        && receipt.owner == input.grant.owner_id
        && receipt.owner_generation == input.grant.owner_generation as i64
        && receipt.grant == input.grant.id
        && receipt.grant_generation == input.grant.generation as i64
        && receipt.request == input.request
        && receipt.action == input.action.name()
        && receipt.fingerprint == input.fingerprint
        && (input.action == RecoveryExecutionAction::BeginWifi || receipt.trial == input.trial)
}
fn no_unknown(conn: &Connection) -> Result<()> {
    if conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM recovery_execution_receipts WHERE state='unknown')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(Error::Conflict);
    }
    Ok(())
}
fn parent_active(conn: &Connection, input: &TrustedRecoveryExecution) -> Result<()> {
    let g = &input.grant;
    if !conn.query_row("SELECT EXISTS(SELECT 1 FROM recovery_execution_receipts WHERE realm=? AND device_public_key=? AND owner_id=? AND owner_generation=? AND grant_id=? AND grant_generation=? AND action='begin_wifi' AND trial_id=? AND state IN ('claimed','starting','connecting','ready','saving'))",params![g.realm,g.device_public_key.as_slice(),g.owner_id,g.owner_generation as i64,g.id,g.generation as i64,input.trial],|r|r.get::<_,bool>(0))? { return Err(Error::Conflict); }
    Ok(())
}
fn begin_exclusive(conn: &Connection, except: Option<&TrustedRecoveryExecution>) -> Result<()> {
    let occupied = if let Some(input) = except {
        let g = &input.grant;
        conn.query_row("SELECT EXISTS(SELECT 1 FROM recovery_execution_receipts WHERE action='begin_wifi' AND state IN ('reserved','claimed','starting','connecting','ready','saving','unknown') AND NOT (realm=? AND grant_id=? AND grant_generation=? AND request_id=?))",params![g.realm,g.id,g.generation as i64,input.request],|r|r.get::<_,bool>(0))?
    } else {
        conn.query_row("SELECT EXISTS(SELECT 1 FROM recovery_execution_receipts WHERE action='begin_wifi' AND state IN ('reserved','claimed','starting','connecting','ready','saving','unknown'))",[],|r|r.get::<_,bool>(0))?
    };
    if occupied {
        return Err(Error::Conflict);
    }
    Ok(())
}

fn reserve_locked(
    db: &mut Database,
    input: TrustedRecoveryExecution,
) -> Result<RecoveryExecutionReservation> {
    require(db)?;
    input.fence.check()?;
    let tx = db
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    recovery_grants::check_current(&tx, &input.grant)?;
    if let Some(receipt) = lookup(&tx, &input.grant, &input.request)? {
        if !matches(&receipt, &input) {
            return Err(Error::Conflict);
        }
        input.fence.check()?;
        tx.commit()?;
        input.fence.check()?;
        return Ok(RecoveryExecutionReservation::Existing(
            RecoveryExecutionReceipt(receipt),
        ));
    }
    no_unknown(&tx)?;
    if input.action == RecoveryExecutionAction::BeginWifi {
        begin_exclusive(&tx, None)?;
    } else {
        parent_active(&tx, &input)?;
    }
    let (total, noncancel): (i64, i64) = tx.query_row(
        "SELECT COUNT(*),COALESCE(SUM(action!='cancel_wifi'),0) FROM recovery_execution_receipts",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if total >= 256 || (input.action != RecoveryExecutionAction::CancelWifi && noncancel >= 255) {
        return Err(Error::Full);
    }
    let g = &input.grant;
    tx.execute("INSERT INTO recovery_execution_receipts VALUES(2,?,?,?,?,?,?,?,?,?,'reserved',?,NULL,NULL)",params![g.realm,g.device_public_key.as_slice(),g.owner_id,g.owner_generation as i64,g.id,g.generation as i64,input.request,input.action.name(),input.fingerprint.as_slice(),input.trial])?;
    let receipt = lookup(&tx, g, &input.request)?.ok_or(Error::Corrupt)?;
    validate(&tx)?;
    input.fence.check()?;
    tx.commit()?;
    input.fence.check()?;
    Ok(RecoveryExecutionReservation::Reserved(
        StorageRecoveryExecutionReservation {
            input: Box::new(input),
            receipt: RecoveryExecutionReceipt(receipt),
        },
    ))
}
fn claim_locked(
    db: &mut Database,
    reservation: StorageRecoveryExecutionReservation,
    new_trial: Option<String>,
) -> Result<StorageRecoveryExecutionClaim> {
    let input = *reservation.input;
    require(db)?;
    input.fence.check()?;
    let trial = match input.action {
        RecoveryExecutionAction::BeginWifi => new_trial
            .filter(|t| valid_trial(t))
            .ok_or(Error::InvalidInput)?,
        _ => {
            if new_trial.is_some() {
                return Err(Error::InvalidInput);
            }
            input.trial.clone().ok_or(Error::InvalidInput)?
        }
    };
    let tx = db
        .connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    recovery_grants::check_current(&tx, &input.grant)?;
    let receipt = lookup(&tx, &input.grant, &input.request)?.ok_or(Error::NotFound)?;
    if !matches(&receipt, &input) || receipt.state != "reserved" {
        return Err(Error::Conflict);
    }
    no_unknown(&tx)?;
    if input.action == RecoveryExecutionAction::BeginWifi {
        begin_exclusive(&tx, Some(&input))?;
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM recovery_execution_receipts WHERE action='begin_wifi' AND trial_id=?)",[&trial],|r|r.get::<_,bool>(0))? { return Err(Error::Conflict); }
    } else {
        parent_active(&tx, &input)?;
    }
    let g = &input.grant;
    if tx.execute("UPDATE recovery_execution_receipts SET state='claimed',trial_id=? WHERE realm=? AND grant_id=? AND grant_generation=? AND request_id=? AND state='reserved'",params![trial,g.realm,g.id,g.generation as i64,input.request])?!=1 { return Err(Error::Conflict); }
    validate(&tx)?;
    let receipt = lookup(&tx, g, &input.request)?.ok_or(Error::Corrupt)?;
    // Final in-transaction cancellation/deadline decision; DB grant cannot
    // change concurrently on this actor. A later cancellation can leave a
    // committed claimed tombstone, but never turns it back into reserved.
    input.fence.check()?;
    tx.commit()?;
    input.fence.check()?;
    Ok(StorageRecoveryExecutionClaim {
        receipt: RecoveryExecutionReceipt(receipt),
    })
}
impl Handle {
    pub fn reserve_recovery_execution_from_trusted_auth(
        &self,
        input: TrustedRecoveryExecution,
    ) -> Result<RecoveryExecutionReservation> {
        self.call(move |db| reserve_locked(db, input))
    }
    pub fn claim_recovery_execution_from_trusted_auth(
        &self,
        reservation: StorageRecoveryExecutionReservation,
        new_trial: Option<String>,
    ) -> Result<StorageRecoveryExecutionClaim> {
        self.call(move |db| claim_locked(db, reservation, new_trial))
    }
    pub fn lookup_recovery_execution_from_trusted_auth(
        &self,
        grant: RecoveryGrant,
        request: String,
        fence: RecoveryExecutionFence,
    ) -> Result<Option<RecoveryExecutionReceipt>> {
        if !uuid(&request) {
            return Err(Error::InvalidInput);
        }
        self.call(move |db| {
            require(db)?;
            fence.check()?;
            let tx = db.connection.transaction()?;
            recovery_grants::check_current(&tx, &grant)?;
            let receipt = lookup(&tx, &grant, &request)?.map(RecoveryExecutionReceipt);
            fence.check()?;
            tx.commit()?;
            fence.check()?;
            Ok(receipt)
        })
    }
    pub fn check_recovery_execution_claim_from_trusted_auth(
        &self,
        input: TrustedRecoveryExecution,
        claim: &StorageRecoveryExecutionClaim,
    ) -> Result<()> {
        self.check_execution_claim(input, claim, false)
    }
    /// Initial derivation only. A progressed claim cannot mint a fresh trial window.
    pub fn check_recovery_execution_start_from_trusted_auth(
        &self,
        input: TrustedRecoveryExecution,
        claim: &StorageRecoveryExecutionClaim,
    ) -> Result<()> {
        self.check_execution_claim(input, claim, true)
    }
    fn check_execution_claim(
        &self,
        input: TrustedRecoveryExecution,
        claim: &StorageRecoveryExecutionClaim,
        start: bool,
    ) -> Result<()> {
        if !matches(&claim.receipt.0, &input) {
            return Err(Error::Forbidden);
        }
        let trial = claim.receipt.0.trial.clone();
        self.call(move |db| {
            require(db)?;
            input.fence.check()?;
            let tx = db.connection.transaction()?;
            recovery_grants::check_current(&tx, &input.grant)?;
            let receipt = lookup(&tx, &input.grant, &input.request)?.ok_or(Error::NotFound)?;
            if !matches(&receipt, &input)
                || receipt.trial != trial
                || if start {
                    receipt.state != "claimed"
                } else {
                    !observations::active_claim(&receipt)
                }
            {
                return Err(Error::Forbidden);
            }
            input.fence.check()?;
            tx.commit()?;
            input.fence.check()
        })
    }
}

#[cfg(test)]
mod tests;
