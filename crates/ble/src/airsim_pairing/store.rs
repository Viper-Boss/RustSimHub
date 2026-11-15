// SPDX-License-Identifier: AGPL-3.0-only

use rustsimhub_airsim::{
    base64url, decode_base64url,
    voble::{Credential, Error, Store},
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    identity: String,
    current: Option<String>,
    previous: Option<String>,
}

pub(super) struct Disk {
    path: PathBuf,
    directory: File,
    lock: File,
    identity: String,
    credential: Option<Credential>,
}

impl Disk {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        match DirBuilder::new().mode(0o700).create(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
            .open(path)?;
        let uid = rustix::process::geteuid().as_raw();
        let private = |meta: &fs::Metadata| meta.uid() == uid && meta.mode() & 0o077 == 0;
        if !private(&directory.metadata()?) {
            return Err(invalid());
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path.join("lock"))?;
        let metadata = lock.metadata()?;
        if !metadata.is_file() || metadata.nlink() != 1 || !private(&metadata) {
            return Err(invalid());
        }
        lock.try_lock().map_err(io::Error::from)?;
        let pending = path.join("state.new");
        match fs::remove_file(pending) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path.join("state.json"));
        let record = match file {
            Ok(file) => {
                let metadata = file.metadata()?;
                if !metadata.is_file() || metadata.nlink() != 1 || !private(&metadata) {
                    return Err(invalid());
                }
                let mut bytes = Vec::new();
                file.take(512).read_to_end(&mut bytes)?;
                if bytes.len() == 512 {
                    return Err(invalid());
                }
                Some(serde_json::from_slice::<Record>(&bytes).map_err(|_| invalid())?)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let new = record.is_none();
        let record = match record {
            Some(value) => value,
            None => {
                let mut id = [0; 18];
                getrandom::fill(&mut id)
                    .map_err(|_| io::Error::other("random source unavailable"))?;
                Record {
                    identity: format!("rustsimhub-{}", base64url(&id)),
                    current: None,
                    previous: None,
                }
            }
        };
        let valid_id = record
            .identity
            .strip_prefix("rustsimhub-")
            .is_some_and(|id| {
                id.len() == 24
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            });
        if !valid_id {
            return Err(invalid());
        }
        let current = record.current.as_deref().map(decode).transpose()?;
        let previous = record.previous.as_deref().map(decode).transpose()?;
        if current.is_none() && previous.is_some() {
            return Err(invalid());
        }
        let state = Self {
            path: path.to_path_buf(),
            directory,
            lock,
            identity: record.identity,
            credential: current.map(|key| Credential::from_bytes(key, previous)),
        };
        if new {
            state.persist(None)?;
        }
        Ok(state)
    }

    pub(super) fn identity(&self) -> &str {
        &self.identity
    }

    pub(super) fn revoke(&mut self) -> io::Result<()> {
        self.persist(None)?;
        self.credential = None;
        Ok(())
    }

    fn persist(&self, credential: Option<&Credential>) -> io::Result<()> {
        let keys = credential.map(Credential::keys);
        let record = Record {
            identity: self.identity.clone(),
            current: keys.map(|(current, _)| base64url(current)),
            previous: keys.and_then(|(_, previous)| previous.map(|key| base64url(key))),
        };
        let pending = self.path.join("state.new");
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&pending)?;
            serde_json::to_writer(&mut file, &record).map_err(io::Error::other)?;
            file.flush()?;
            file.sync_all()?;
            fs::rename(&pending, self.path.join("state.json"))?;
            self.directory.sync_all()
        })();
        if result.is_err() {
            let _ = fs::remove_file(pending);
        }
        result
    }
}

impl Drop for Disk {
    fn drop(&mut self) {
        let _ = self.lock.unlock();
    }
}

impl Store for Disk {
    fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }

    fn save(&mut self, credential: Credential) -> Result<(), Error> {
        self.persist(Some(&credential))
            .map_err(|_| Error::Unavailable)?;
        self.credential = Some(credential);
        Ok(())
    }
}

fn decode(value: &str) -> io::Result<[u8; 32]> {
    decode_base64url(value)
        .map_err(|_| invalid())?
        .try_into()
        .map_err(|_| invalid())
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid or non-private VoBLE state",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_key_lock_and_revocation() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        let mut store = Disk::open(&path).unwrap();
        let identity = store.identity().to_owned();
        assert!(Disk::open(&path).is_err());
        store
            .save(Credential::from_bytes([1; 32], Some([2; 32])))
            .unwrap();
        drop(store);
        let mut store = Disk::open(&path).unwrap();
        assert_eq!(store.identity(), identity);
        assert_eq!(
            store.credential().unwrap().keys(),
            (&[1; 32], Some(&[2; 32]))
        );
        store.revoke().unwrap();
        drop(store);
        let store = Disk::open(&path).unwrap();
        assert!(store.credential().is_none());
        assert_eq!(store.identity(), identity);
    }

    #[test]
    fn corrupt_state_is_preserved() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state");
        drop(Disk::open(&path).unwrap());
        fs::write(path.join("state.json"), "{").unwrap();
        assert!(Disk::open(&path).is_err());
        assert_eq!(fs::read(path.join("state.json")).unwrap(), b"{");
    }
}
