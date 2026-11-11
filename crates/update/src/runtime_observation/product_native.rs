// SPDX-License-Identifier: AGPL-3.0-only
//! Synthetic PID1/cgroup membership only. Every process and executable fact,
//! UID/capability/NNP/start generation and worker scan is actual kernel state.
use super::*;
pub(super) struct NativePid1;
impl Sources for NativePid1 {
    fn boot(&self) -> Result<String> {
        linux::boot()
    }
    fn units(&self) -> Result<Vec<Unit>> {
        units::observe()
    }
    fn processes(&self, units: &[Unit], slot: &Slot) -> Result<(Vec<Process>, WorkerState)> {
        if units.len() != 4 || !linux::uid_processes(983)?.is_empty() || !units[3].stopped {
            return Err(Error::Denied);
        }
        let text = std::fs::read_to_string("/proc/self/cgroup").map_err(|_| Error::Io)?;
        let cgroup = text
            .lines()
            .find_map(|l| l.strip_prefix("0::"))
            .ok_or(Error::Denied)?;
        let mut result = vec![];
        for (index, unit) in units.iter().take(3).enumerate() {
            if !unit.active {
                return Err(Error::Denied);
            }
            result.push(linux::probe_verified(
                index,
                unit.pid,
                ROLES[index].3,
                slot,
                cgroup,
            )?);
        }
        Ok((result, WorkerState::Off))
    }
}
