// SPDX-License-Identifier: AGPL-3.0-only
//! Trusted Core observations, never deserialize a frontend success flag here.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryExecutionProgress {
    Starting,
    Connecting,
    Ready,
}
impl RecoveryExecutionProgress {
    fn name(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Connecting => "connecting",
            Self::Ready => "ready",
        }
    }
}
/// Facts supplied by the trusted original backend after its actual save or
/// exact compensation. No Deserialize or network bool/permission conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryExecutionCompletion {
    Saved,
    RolledBack,
    Unknown,
}
impl RecoveryExecutionCompletion {
    fn name(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::RolledBack => "rolled_back",
            Self::Unknown => "unknown",
        }
    }
}
pub(super) fn active(state: &str) -> bool {
    matches!(
        state,
        "claimed" | "starting" | "connecting" | "ready" | "saving"
    )
}
pub(super) fn active_claim(row: &Receipt) -> bool {
    match row.action.as_str() {
        "begin_wifi" => active(&row.state),
        "confirm_wifi" => matches!(row.state.as_str(), "claimed" | "saving"),
        "cancel_wifi" => row.state == "claimed",
        _ => false,
    }
}
fn same_claim(row: &Receipt, snapshot: &Receipt) -> bool {
    row.format == snapshot.format
        && row.realm == snapshot.realm
        && row.pin == snapshot.pin
        && row.owner == snapshot.owner
        && row.owner_generation == snapshot.owner_generation
        && row.grant == snapshot.grant
        && row.grant_generation == snapshot.grant_generation
        && row.request == snapshot.request
        && row.action == snapshot.action
        && row.fingerprint == snapshot.fingerprint
        && row.trial == snapshot.trial
}
fn current(conn: &Connection, snapshot: &Receipt) -> Result<Receipt> {
    let row=conn.query_row("SELECT * FROM recovery_execution_receipts WHERE realm=? AND grant_id=? AND grant_generation=? AND request_id=?",params![snapshot.realm,snapshot.grant,snapshot.grant_generation,snapshot.request],Receipt::row).optional()?.ok_or(Error::NotFound)?;
    if !same_claim(&row, snapshot) {
        return Err(Error::Forbidden);
    }
    Ok(row)
}
fn checked(
    conn: &Connection,
    input: &TrustedRecoveryExecution,
    snapshot: &Receipt,
) -> Result<Receipt> {
    input.fence.check()?;
    recovery_grants::check_current(conn, &input.grant)?;
    let row = current(conn, snapshot)?;
    if !matches(&row, input) {
        return Err(Error::Forbidden);
    }
    Ok(row)
}
fn update(conn: &Connection, row: &Receipt, state: &str) -> Result<()> {
    let (saved, rollback) = match state {
        "saved" => (Some(1), None),
        "rolled_back" => (Some(0), Some(1)),
        _ => (None, None),
    };
    if conn.execute("UPDATE recovery_execution_receipts SET state=?,settings_saved=?,rollback_verified=? WHERE realm=? AND grant_id=? AND grant_generation=? AND request_id=? AND state=?",params![state,saved,rollback,row.realm,row.grant,row.grant_generation,row.request,row.state])?!=1 {return Err(Error::Conflict);}
    Ok(())
}
fn pair(begin: &Receipt, control: &Receipt) -> Result<()> {
    if begin.action != "begin_wifi"
        || !matches!(control.action.as_str(), "confirm_wifi" | "cancel_wifi")
        || !begin.same_trial(control)
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
impl Handle {
    pub fn record_recovery_progress_from_trusted_core(
        &self,
        input: TrustedRecoveryExecution,
        claim: &StorageRecoveryExecutionClaim,
        progress: RecoveryExecutionProgress,
    ) -> Result<RecoveryExecutionReceipt> {
        let snapshot = claim.receipt.0.clone();
        self.call(move |db| {
            require(db)?;
            let tx = db
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let row = checked(&tx, &input, &snapshot)?;
            let next = progress.name();
            if row.action != "begin_wifi"
                || !(row.state == next
                    || matches!(
                        (row.state.as_str(), next),
                        ("claimed", "starting")
                            | ("starting", "connecting")
                            | ("connecting", "ready")
                            | ("ready", "connecting")
                    ))
            {
                return Err(Error::InvalidTransition);
            }
            update(&tx, &row, next)?;
            validate(&tx)?;
            let result = current(&tx, &snapshot)?;
            input.fence.check()?;
            tx.commit()?;
            input.fence.check()?;
            Ok(RecoveryExecutionReceipt(result))
        })
    }
    /// Durable user confirmation BEFORE attempting save. Both rows change in
    /// the same transaction; cancellation and Begin are not confirmation.
    pub fn record_recovery_saving_from_trusted_core(
        &self,
        begin: TrustedRecoveryExecution,
        begin_claim: &StorageRecoveryExecutionClaim,
        confirm: TrustedRecoveryExecution,
        confirm_claim: &StorageRecoveryExecutionClaim,
    ) -> Result<()> {
        let (bs, cs) = (
            begin_claim.receipt.0.clone(),
            confirm_claim.receipt.0.clone(),
        );
        self.call(move |db| {
            require(db)?;
            let tx = db
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let b = checked(&tx, &begin, &bs)?;
            let c = checked(&tx, &confirm, &cs)?;
            pair(&b, &c)?;
            if c.action != "confirm_wifi"
                || !matches!(
                    (b.state.as_str(), c.state.as_str()),
                    ("ready", "claimed") | ("saving", "saving")
                )
            {
                return Err(Error::InvalidTransition);
            }
            update(&tx, &b, "saving")?;
            update(&tx, &c, "saving")?;
            validate(&tx)?;
            begin.fence.check()?;
            confirm.fence.check()?;
            tx.commit()?;
            begin.fence.check()?;
            confirm.fence.check()
        })
    }
    /// Accounting only, including after revoke/disconnect/deadline. Private
    /// original claims must match their exact durable records. Never returns
    /// authority or clears Unknown. Backend evidence is a trusted-source fact.
    pub fn finish_recovery_trial_from_trusted_core(
        &self,
        begin: &StorageRecoveryExecutionClaim,
        control: Option<&StorageRecoveryExecutionClaim>,
        completion: RecoveryExecutionCompletion,
    ) -> Result<RecoveryExecutionReceipt> {
        let (bs, cs) = (
            begin.receipt.0.clone(),
            control.map(|c| c.receipt.0.clone()),
        );
        self.call(move|db|{
            require(db)?;
            let tx=db.connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let b=current(&tx,&bs)?;
            if b.action!="begin_wifi" {return Err(Error::Forbidden);}
            let c=cs.as_ref().map(|s|current(&tx,s)).transpose()?;
            if let Some(c)=&c {pair(&b,c)?;}
            let next=completion.name();
            if b.state==next && c.as_ref().is_none_or(|c|c.state==next) {
                if completion==RecoveryExecutionCompletion::Saved && c.as_ref().is_none_or(|c|c.action!="confirm_wifi") {return Err(Error::Forbidden);}
                tx.commit()?;return Ok(RecoveryExecutionReceipt(b));
            }
            if !active_claim(&b) || c.as_ref().is_some_and(|c|!active_claim(c)) {return Err(Error::InvalidTransition);}
            if completion==RecoveryExecutionCompletion::Saved && (b.state!="saving" || c.as_ref().is_none_or(|c|c.action!="confirm_wifi" || c.state!="saving")) {return Err(Error::InvalidTransition);}
            update(&tx,&b,next)?;
            if let Some(c)=&c {update(&tx,c,next)?;}
            // Competing/admitted requests were not executed by this actor.
            // Preserve their uncertainty rather than manufacture their outcome.
            tx.execute("UPDATE recovery_execution_receipts SET state='unknown',settings_saved=NULL,rollback_verified=NULL WHERE realm=? AND device_public_key=? AND owner_id=? AND owner_generation=? AND grant_id=? AND grant_generation=? AND trial_id=? AND state IN ('reserved','claimed','starting','connecting','ready','saving')",params![b.realm,b.pin,b.owner,b.owner_generation,b.grant,b.grant_generation,b.trial])?;
            validate(&tx)?;
            let result=current(&tx,&bs)?;tx.commit()?;Ok(RecoveryExecutionReceipt(result))
        })
    }
}

#[cfg(test)]
mod tests;
