// SPDX-License-Identifier: AGPL-3.0-only
//! Native workers only trust Linux root or the root-managed RustSimHub core identity.
use std::{
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
};
pub fn core_uid(uid: u32) -> bool {
    if uid == 0 {
        return true;
    }
    service_uid("rustsimhub-core", uid)
}
pub fn service_uid(name: &str, uid: u32) -> bool {
    if !matches!(
        name,
        "rustsimhub-core" | "rustsimhub-edge" | "rustsimhub-media"
    ) {
        return false;
    }
    let Ok(f) = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open("/etc/passwd")
    else {
        return false;
    };
    let Ok(m) = f.metadata() else {
        return false;
    };
    if !m.is_file() || m.uid() != 0 || m.mode() & 0o022 != 0 || m.nlink() != 1 || m.len() > 131072 {
        return false;
    }
    let mut bytes = Vec::new();
    if f.take(131073).read_to_end(&mut bytes).is_err() || bytes.len() > 131072 {
        return false;
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let rows = text
        .lines()
        .filter(|row| row.starts_with(&format!("{name}:")))
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return false;
    }
    let fields = rows[0].split(':').collect::<Vec<_>>();
    fields.len() == 7
        && fields[2].parse::<u32>().ok() == Some(uid)
        && fields[6] == "/usr/sbin/nologin"
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_local_account_cannot_be_selected_as_core() {
        assert!(core_uid(0));
        assert!(!core_uid(u32::MAX));
        assert!(!core_uid(65534));
    }
}
