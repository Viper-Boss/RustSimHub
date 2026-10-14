// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Pure policy types. No hardware, network, clock, or persistence access.
mod history;
pub use history::{HistoryCursor, HistoryEntity};
mod nas;
pub use nas::NasEntity;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxCursor {
    pub db_epoch: String,
    pub seq: String,
}
use serde::{Deserialize, Serialize};
mod calls;
pub use calls::{CallAction, CallLease, CallState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineEpoch(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    Busy,
    Expired,
    WrongOwner,
    StaleBoot,
    StaleLine,
    StaleGrant,
    InvalidTransition,
    Overflow,
}

/// A grant is a fencing number, not a bearer credential. Core must authenticate
/// the client before invoking this module. Ticks are boot-monotonic milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallGrant {
    pub boot: String,
    pub line: LineEpoch,
    pub client: String,
    pub generation: u64,
    pub expires_ms: u64,
}

#[derive(Debug)]
pub struct CallLeases {
    boot: String,
    line: LineEpoch,
    generation: u64,
    current: Option<CallGrant>,
}

impl CallLeases {
    pub const TTL_MS: u64 = 30_000;

    pub fn new(boot: String, line: LineEpoch) -> Self {
        Self {
            boot,
            line,
            generation: 0,
            current: None,
        }
    }

    /// A modem's live call blocks a new reservation even after a lease expires.
    pub fn acquire(
        &mut self,
        client: String,
        now_ms: u64,
        modem_busy: bool,
    ) -> Result<CallGrant, PolicyError> {
        if modem_busy || self.current.as_ref().is_some_and(|g| now_ms < g.expires_ms) {
            return Err(PolicyError::Busy);
        }
        let expires_ms = now_ms
            .checked_add(Self::TTL_MS)
            .ok_or(PolicyError::Overflow)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or(PolicyError::Overflow)?;
        let grant = CallGrant {
            boot: self.boot.clone(),
            line: self.line,
            client,
            generation: self.generation,
            expires_ms,
        };
        self.current = Some(grant.clone());
        Ok(grant)
    }

    pub fn validate(
        &self,
        grant: &CallGrant,
        client: &str,
        now_ms: u64,
    ) -> Result<(), PolicyError> {
        if grant.boot != self.boot {
            return Err(PolicyError::StaleBoot);
        }
        if grant.line != self.line {
            return Err(PolicyError::StaleLine);
        }
        if grant.client != client {
            return Err(PolicyError::WrongOwner);
        }
        let active = self.current.as_ref().ok_or(PolicyError::StaleGrant)?;
        if grant.generation != active.generation || grant.client != active.client {
            return Err(PolicyError::StaleGrant);
        }
        if now_ms >= active.expires_ms {
            return Err(PolicyError::Expired);
        }
        Ok(())
    }

    pub fn renew(
        &mut self,
        grant: &CallGrant,
        client: &str,
        now_ms: u64,
    ) -> Result<CallGrant, PolicyError> {
        self.validate(grant, client, now_ms)?;
        let expires_ms = now_ms
            .checked_add(Self::TTL_MS)
            .ok_or(PolicyError::Overflow)?;
        let active = self.current.as_mut().ok_or(PolicyError::StaleGrant)?;
        active.expires_ms = expires_ms;
        Ok(active.clone())
    }

    pub fn release(
        &mut self,
        grant: &CallGrant,
        client: &str,
        now_ms: u64,
    ) -> Result<(), PolicyError> {
        self.validate(grant, client, now_ms)?;
        self.current = None;
        Ok(())
    }

    pub fn change_line(&mut self, line: LineEpoch) -> Result<(), PolicyError> {
        if line.0 <= self.line.0 {
            return Err(PolicyError::StaleLine);
        }
        self.line = line;
        self.current = None;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmsState {
    Prepared,
    Submitting,
    Submitted,
    Unknown,
    Cancelled,
}

/// Only domain state; persistence must commit Submitting BEFORE modem Send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SmsOperation {
    pub idempotency_key: String,
    pub line: LineEpoch,
    state: SmsState,
}

impl SmsOperation {
    pub fn prepare(idempotency_key: String, line: LineEpoch) -> Self {
        Self {
            idempotency_key,
            line,
            state: SmsState::Prepared,
        }
    }
    pub fn state(&self) -> SmsState {
        self.state
    }
    pub fn begin_submit(&mut self, current_line: LineEpoch) -> Result<(), PolicyError> {
        if current_line != self.line {
            if self.state == SmsState::Prepared {
                self.state = SmsState::Cancelled;
            }
            return Err(PolicyError::StaleLine);
        }
        if self.state != SmsState::Prepared {
            return Err(PolicyError::InvalidTransition);
        }
        self.state = SmsState::Submitting;
        Ok(())
    }
    pub fn confirm_submitted(&mut self) -> Result<(), PolicyError> {
        if self.state != SmsState::Submitting {
            return Err(PolicyError::InvalidTransition);
        }
        self.state = SmsState::Submitted;
        Ok(())
    }
    /// Timeout/restart cannot prove whether the baseband accepted the SMS.
    pub fn recover_uncertain(&mut self) {
        if self.state == SmsState::Submitting {
            self.state = SmsState::Unknown;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expired_owner_cannot_renew_or_control_new_grant() {
        let mut book = CallLeases::new("boot-a".into(), LineEpoch(1));
        let old = book.acquire("a".into(), 0, false).unwrap();
        assert_eq!(book.renew(&old, "a", 30_000), Err(PolicyError::Expired));
        let new = book.acquire("b".into(), 30_000, false).unwrap();
        assert_eq!(
            book.validate(&old, "a", 30_001),
            Err(PolicyError::StaleGrant)
        );
        assert_eq!(
            book.validate(&new, "a", 30_001),
            Err(PolicyError::WrongOwner)
        );
    }
    #[test]
    fn lease_expiry_does_not_imply_modem_idle() {
        let mut book = CallLeases::new("boot".into(), LineEpoch(1));
        book.acquire("a".into(), 0, false).unwrap();
        assert_eq!(
            book.acquire("b".into(), 40_000, true),
            Err(PolicyError::Busy)
        );
    }
    #[test]
    fn reboot_and_sim_change_fence_old_grants() {
        let mut first = CallLeases::new("boot-a".into(), LineEpoch(1));
        let grant = first.acquire("a".into(), 0, false).unwrap();
        first.change_line(LineEpoch(2)).unwrap();
        assert_eq!(first.validate(&grant, "a", 1), Err(PolicyError::StaleLine));
        let reboot = CallLeases::new("boot-b".into(), LineEpoch(1));
        assert_eq!(reboot.validate(&grant, "a", 1), Err(PolicyError::StaleBoot));
    }
    #[test]
    fn forged_expiry_cannot_extend_server_lease() {
        let mut book = CallLeases::new("boot".into(), LineEpoch(1));
        let mut grant = book.acquire("a".into(), 0, false).unwrap();
        grant.expires_ms = u64::MAX;
        assert_eq!(
            book.validate(&grant, "a", 30_000),
            Err(PolicyError::Expired)
        );
    }
    #[test]
    fn interrupted_sms_is_never_automatically_resent() {
        let mut sms = SmsOperation::prepare("request-a".into(), LineEpoch(1));
        sms.begin_submit(LineEpoch(1)).unwrap();
        sms.recover_uncertain();
        assert_eq!(sms.state(), SmsState::Unknown);
        assert_eq!(
            sms.begin_submit(LineEpoch(1)),
            Err(PolicyError::InvalidTransition)
        );
        assert_eq!(sms.confirm_submitted(), Err(PolicyError::InvalidTransition));
    }
    #[test]
    fn sim_change_cancels_prepared_sms_and_duplicate_submit_is_rejected() {
        let mut sms = SmsOperation::prepare("request-a".into(), LineEpoch(1));
        assert_eq!(sms.begin_submit(LineEpoch(2)), Err(PolicyError::StaleLine));
        assert_eq!(sms.state(), SmsState::Cancelled);
        assert_eq!(
            sms.begin_submit(LineEpoch(1)),
            Err(PolicyError::InvalidTransition)
        );
        let mut sent = SmsOperation::prepare("request-b".into(), LineEpoch(2));
        sent.begin_submit(LineEpoch(2)).unwrap();
        sent.confirm_submitted().unwrap();
        assert_eq!(
            sent.begin_submit(LineEpoch(2)),
            Err(PolicyError::InvalidTransition)
        );
    }
    #[test]
    fn tick_overflow_never_creates_a_grant() {
        let mut book = CallLeases::new("boot".into(), LineEpoch(1));
        assert_eq!(
            book.acquire("a".into(), u64::MAX, false),
            Err(PolicyError::Overflow)
        );
    }
}
