// SPDX-License-Identifier: AGPL-3.0-only
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    OutgoingPending,
    Dialing,
    RingingOut,
    RingingIn,
    Active,
    Held,
    Waiting,
    Ended,
    Reconciling,
}
impl CallState {
    pub fn from_modem(state: i32, direction: i32) -> Option<Self> {
        Some(match state {
            0 if direction == 2 => Self::OutgoingPending,
            0 => Self::Reconciling,
            1 => Self::Dialing,
            2 => Self::RingingOut,
            3 => Self::RingingIn,
            4 => Self::Active,
            5 => Self::Held,
            6 => Self::Waiting,
            7 => Self::Ended,
            _ => return None,
        })
    }
}
/// A fencing receipt, never a bearer credential. Server checks its own expiry and authenticated principal.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallLease {
    pub call_id: String,
    pub boot: String,
    pub line: super::LineEpoch,
    pub generation: u64,
    pub expires_in_seconds: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallAction {
    Dial,
    Accept,
    Hangup,
    Dtmf,
}
impl CallAction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dial => "dial",
            Self::Accept => "accept",
            Self::Hangup => "hangup",
            Self::Dtmf => "dtmf",
        }
    }
    pub fn allowed(self, state: i32, direction: i32) -> bool {
        match self {
            Self::Dial => state == 0 && direction == 2,
            Self::Accept => state == 3 && direction == 1,
            Self::Hangup => (0..=6).contains(&state),
            Self::Dtmf => state == 4,
        }
    }
    pub fn confirmed(self, state: i32) -> bool {
        match self {
            Self::Dial => (1..=6).contains(&state),
            Self::Accept => state == 4,
            Self::Hangup => state == 7,
            Self::Dtmf => false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modem_state_unknown_never_means_ended_or_idle() {
        assert_eq!(CallState::from_modem(0, 1), Some(CallState::Reconciling));
        assert_eq!(CallState::from_modem(7, 1), Some(CallState::Ended));
        assert!(CallState::from_modem(99, 2).is_none());
    }
    #[test]
    fn answer_and_tone_require_correct_live_state() {
        assert!(CallAction::Accept.allowed(3, 1));
        assert!(!CallAction::Accept.allowed(3, 2));
        assert!(!CallAction::Accept.allowed(4, 1));
        assert!(CallAction::Dtmf.allowed(4, 1));
        assert!(!CallAction::Dtmf.allowed(5, 1));
        assert!(!CallAction::Dial.confirmed(0));
        assert!(!CallAction::Hangup.confirmed(4));
    }
}
