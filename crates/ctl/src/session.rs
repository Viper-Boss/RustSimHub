// SPDX-License-Identifier: AGPL-3.0-only
use crate::Result;
use rustsimhub_auth::{read_private, write_private_new};
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroize;
// No Debug: never display a raw bearer token.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub protocol: u16,
    pub device_public_key: [u8; 32],
    pub client_public_key: [u8; 32],
    pub server_uid: u32,
    pub token: String,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}
impl Session {
    pub fn write(&self, path: &Path) -> Result<()> {
        let mut bytes = zeroize::Zeroizing::new(serde_json::to_vec(self)?);
        write_private_new(path, &bytes)?;
        bytes.zeroize();
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = read_private(path, 4096)?;
        let s: Self = serde_json::from_slice(&bytes).map_err(|_| "invalid session file")?;
        if s.protocol != 2
            || s.token.len() != 64
            || !s
                .token
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("invalid session".into());
        }
        Ok(s)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_session_cannot_be_overwritten_or_loaded_from_symlink() {
        let d = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = d.path().join("session.json");
        let s = Session {
            protocol: 2,
            device_public_key: [1; 32],
            client_public_key: [2; 32],
            server_uid: 1,
            token: "a".repeat(64),
        };
        s.write(&path).unwrap();
        assert!(s.write(&path).is_err());
        assert_eq!(Session::load(&path).unwrap().token, s.token);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&path, d.path().join("linked")).unwrap();
            assert!(Session::load(&d.path().join("linked")).is_err());
        }
    }
}
