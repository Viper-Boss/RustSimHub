// SPDX-License-Identifier: AGPL-3.0-only
use super::{Result, ROLES};
use crate::{admission::MaintenanceSnapshot, Error};
use std::collections::BTreeMap;

pub(crate) const PROPERTIES: [&str; 9] = [
    "Id",
    "LoadState",
    "ActiveState",
    "SubState",
    "MainPID",
    "User",
    "ControlGroup",
    "NeedDaemonReload",
    "InvocationID",
];
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Unit {
    pub(super) index: usize,
    pub(super) pid: u32,
    pub(super) active: bool,
    pub(super) stopped: bool,
    pub(super) cgroup: String,
    invocation: String,
    state: String,
    sub_state: String,
}
pub(super) fn observe() -> Result<Vec<Unit>> {
    (0..ROLES.len())
        .map(|i| parse(i, &MaintenanceSnapshot::runtime_unit(i)?))
        .collect()
}
pub(super) fn parse(index: usize, bytes: &[u8]) -> Result<Unit> {
    if bytes.len() > 4096 {
        return Err(Error::Invalid);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('=').ok_or(Error::Invalid)?;
        if !PROPERTIES.contains(&key) || values.insert(key, value).is_some() {
            return Err(Error::Invalid);
        }
    }
    let (unit, _, user, uid) = *ROLES.get(index).ok_or(Error::Invalid)?;
    if values.len() != PROPERTIES.len()
        || values["Id"] != unit
        || values["LoadState"] != "loaded"
        || values["NeedDaemonReload"] != "no"
        || (values["User"] != user && values["User"] != uid.to_string())
    {
        return Err(Error::Denied);
    }
    let pid = values["MainPID"]
        .parse::<u32>()
        .map_err(|_| Error::Invalid)?;
    if values["MainPID"] != pid.to_string() {
        return Err(Error::Invalid);
    }
    let active = values["ActiveState"] == "active" && values["SubState"] == "running" && pid != 0;
    let stopped = values["ActiveState"] == "inactive" && values["SubState"] == "dead" && pid == 0;
    if (!active && index != 3) || (pid != 0 && !active) {
        return Err(Error::Integrity);
    }
    if !active
        && !stopped
        && !matches!(
            (values["ActiveState"], values["SubState"]),
            ("failed", "failed")
                | ("activating", "start")
                | ("activating", "start-pre")
                | ("deactivating", "stop")
                | ("deactivating", "stop-post")
        )
    {
        return Err(Error::Integrity);
    }
    let cgroup = values["ControlGroup"];
    let expected = format!("/system.slice/{unit}");
    if (active && cgroup != expected) || (!active && !cgroup.is_empty() && cgroup != expected) {
        return Err(Error::Integrity);
    }
    let invocation = values["InvocationID"];
    if (active && invocation.len() != 32)
        || (!invocation.is_empty()
            && (invocation.len() != 32 || !invocation.bytes().all(|c| c.is_ascii_hexdigit())))
    {
        return Err(Error::Invalid);
    }
    Ok(Unit {
        index,
        pid,
        active,
        stopped,
        cgroup: expected,
        invocation: invocation.into(),
        state: values["ActiveState"].into(),
        sub_state: values["SubState"].into(),
    })
}
