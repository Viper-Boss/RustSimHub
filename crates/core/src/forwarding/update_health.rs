// SPDX-License-Identifier: AGPL-3.0-only
//! Read only existing controller metadata; never materialize policy/credentials.
use super::*;
use rustsimhub_core::update_health::{
    Error as HealthError, ForwarderLease, Result as HealthResult,
};
impl Controller {
    pub(crate) fn health_locked(
        &self,
        auth: &Auth,
        work: &mut dyn FnMut(ForwarderLease) -> HealthResult<()>,
    ) -> HealthResult<()> {
        let state = self.0.try_lock().map_err(|_| HealthError::Busy)?;
        if state.quick.is_some() || state.quick_stop.is_some() {
            return Err(HealthError::Busy);
        }
        if let Some(q) = &state.queue {
            q.current_binding(auth).map_err(|_| HealthError::Denied)?;
        }
        let (generation, until) = match &state.worker {
            Some(session) => {
                let (g, t) = session
                    .health_binding(auth)
                    .map_err(|_| HealthError::Denied)?;
                (Some(g), Some(t))
            }
            None => (None, None),
        };
        work(ForwarderLease {
            queue_owned: state.queue.is_some(),
            admitted_generation: generation,
            admitted_until: until,
        })
    }
}
