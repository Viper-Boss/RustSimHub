// SPDX-License-Identifier: AGPL-3.0-only
use super::{PortError, Result};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};

/// Local, Root-owned installation accounts. No numeric UID is reserved here,
/// and this type cannot be deserialized from a client or policy request.
pub struct InstalledRoles {
    pub core: u32,
    pub edge: u32,
    pub forwarder: u32,
    pub ble: u32,
    pub media: u32,
    pub engine: u32,
    pub engine_gid: u32,
}
impl InstalledRoles {
    pub fn read() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open("/etc/passwd")
            .map_err(|_| PortError::Unavailable)?;
        let m = file.metadata().map_err(|_| PortError::Io)?;
        if !m.is_file()
            || m.uid() != 0
            || m.mode() & 0o022 != 0
            || m.nlink() != 1
            || m.len() > 65536
        {
            return Err(PortError::Denied);
        }
        let mut raw = String::new();
        file.take(65537)
            .read_to_string(&mut raw)
            .map_err(|_| PortError::Io)?;
        if raw.len() > 65536 {
            return Err(PortError::Denied);
        }
        let current = rustix::process::geteuid().as_raw();
        let roles = Self::parse(&raw)?;
        if current == 0 || roles.core != current {
            return Err(PortError::Denied);
        }
        Ok(roles)
    }
    fn parse(raw: &str) -> Result<Self> {
        const NAMES: [&str; 6] = [
            "rustsimhub-core",
            "rustsimhub-edge",
            "rustsimhub-forwarder",
            "rustsimhub-ble",
            "rustsimhub-media",
            "rustsimhub-nkn",
        ];
        let mut selected = [None; 6];
        let mut counts = HashMap::<u32, usize>::new();
        if raw.lines().count() > 4096 {
            return Err(PortError::Denied);
        }
        for line in raw.lines().filter(|v| !v.is_empty()) {
            let columns = line.split(':').collect::<Vec<_>>();
            if columns.len() != 7 {
                return Err(PortError::Denied);
            }
            let uid = columns[2].parse::<u32>().map_err(|_| PortError::Denied)?;
            *counts.entry(uid).or_default() += 1;
            if let Some(i) = NAMES.iter().position(|name| *name == columns[0]) {
                let gid = columns[3].parse::<u32>().map_err(|_| PortError::Denied)?;
                if selected[i].is_some()
                    || uid == 0
                    || uid == u32::MAX
                    || gid == 0
                    || gid == u32::MAX
                {
                    return Err(PortError::Denied);
                }
                selected[i] = Some((uid, gid));
            }
        }
        let assigned = selected
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or(PortError::Unavailable)?;
        if assigned.iter().map(|v| v.0).collect::<HashSet<_>>().len() != 6
            || assigned.iter().any(|v| counts.get(&v.0) != Some(&1))
        {
            return Err(PortError::Denied);
        }
        Ok(Self {
            core: assigned[0].0,
            edge: assigned[1].0,
            forwarder: assigned[2].0,
            ble: assigned[3].0,
            media: assigned[4].0,
            engine: assigned[5].0,
            engine_gid: assigned[5].1,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_installed_accounts_reject_missing_aliases_and_uid_reuse() {
        let raw = [
            "rustsimhub-core",
            "rustsimhub-edge",
            "rustsimhub-forwarder",
            "rustsimhub-ble",
            "rustsimhub-media",
            "rustsimhub-nkn",
        ]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            format!(
                "{name}:x:{}:{}:fixture:/nonexistent:/usr/sbin/nologin\n",
                1001 + i,
                1001 + i
            )
        })
        .collect::<String>();
        assert_eq!(InstalledRoles::parse(&raw).unwrap().engine, 1006);
        for other in [
            raw.replace("rustsimhub-nkn:", "missing:"),
            raw.replace("1006:1006", "1001:1006"),
            format!("{raw}alias:x:1006:1006:f:/nonexistent:/bin/false\n"),
            format!("{raw}rustsimhub-core:x:2001:2001:f:/none:/bin/false\n"),
        ] {
            assert!(InstalledRoles::parse(&other).is_err())
        }
    }
}
