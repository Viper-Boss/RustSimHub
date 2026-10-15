// SPDX-License-Identifier: AGPL-3.0-only
//! One Core's network changes exclude new voice mutations, including in-flight calls.
use rustsimhub_domain::CallAction;
use std::sync::{Arc, OnceLock};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};
#[derive(Default)]
pub(crate) struct Coordination {
    gate: Arc<RwLock<()>>,
}
static GLOBAL: OnceLock<Arc<Coordination>> = OnceLock::new();
pub(crate) fn global() -> Arc<Coordination> {
    GLOBAL
        .get_or_init(|| Arc::new(Coordination::default()))
        .clone()
}
impl Coordination {
    pub(crate) fn network(&self) -> Result<OwnedRwLockWriteGuard<()>, &'static str> {
        self.gate
            .clone()
            .try_write_owned()
            .map_err(|_| "voice_mutation_inflight")
    }
    pub(crate) fn telephony(
        &self,
        action: CallAction,
    ) -> Result<Option<OwnedRwLockReadGuard<()>>, &'static str> {
        if action == CallAction::Hangup {
            return Ok(None);
        }
        self.gate
            .clone()
            .try_read_owned()
            .map(Some)
            .map_err(|_| "network_trial_active")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_and_voice_are_exclusive_but_hangup_always_remains_available() {
        let c = Coordination::default();
        let network = c.network().unwrap();
        for action in [CallAction::Dial, CallAction::Accept, CallAction::Dtmf] {
            assert!(c.telephony(action).is_err());
        }
        assert!(c.telephony(CallAction::Hangup).unwrap().is_none());
        drop(network);
        let voice = c.telephony(CallAction::Dial).unwrap();
        assert!(c.network().is_err());
        drop(voice);
        assert!(c.network().is_ok());
    }
}
