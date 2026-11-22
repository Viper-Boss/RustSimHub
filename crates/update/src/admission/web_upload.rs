// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-path maintenance consumption of a Core-owned, explicitly authorized upload.
use super::*;
use crate::{
    bundle,
    store::{read_root_release_key, Store},
};
use rustsimhub_wire::{
    UpdateUploadRecord, UPDATE_INBOX_ACCEPTED, UPDATE_INBOX_HEADER_LIMIT, UPDATE_INBOX_NAME,
};

const APP: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/app";
const CORE_UID: u32 = 986;

fn load(path: &Path, uid: u32) -> Result<(UpdateUploadRecord, Vec<u8>)> {
    let directory = Observation::capture(path)?;
    let stamp = directory.stamp.as_ref().ok_or(Error::Denied)?;
    if stamp.mode != 0o040700 || stamp.uid != uid {
        return Err(Error::Denied);
    }
    let file = Observation::capture(&path.join(UPDATE_INBOX_ACCEPTED))?;
    let file_stamp = file.stamp.as_ref().ok_or(Error::Denied)?;
    let limit = (4 + UPDATE_INBOX_HEADER_LIMIT + bundle::MAX_BUNDLE) as u64;
    let mut handle = private_file(&file.path, uid, 0o600, limit)?;
    if Stamp::from(denied_io(handle.metadata())?) != *file_stamp {
        return Err(Error::Busy);
    }
    let mut length = [0; 4];
    denied_io(handle.read_exact(&mut length))?;
    let length = u32::from_be_bytes(length) as usize;
    if !(1..=UPDATE_INBOX_HEADER_LIMIT).contains(&length) {
        return Err(Error::Invalid);
    }
    let mut header = vec![0; length];
    denied_io(handle.read_exact(&mut header))?;
    let record: UpdateUploadRecord = serde_json::from_slice(&header).map_err(|_| Error::Invalid)?;
    if record.format != 1
        || !record.upload.valid()
        || file_stamp.size != 4 + length as u64 + u64::from(record.upload.length)
    {
        return Err(Error::Invalid);
    }
    let mut bytes = vec![0; record.upload.length as usize];
    denied_io(handle.read_exact(&mut bytes))?;
    directory.revalidate()?;
    file.revalidate()?;
    if Stamp::from(denied_io(handle.metadata())?) != *file_stamp
        || manifest_hash(&bytes) != record.upload.bundle_sha256
    {
        return Err(Error::Integrity);
    }
    Ok((record, bytes))
}

pub fn admit_web_upload() -> Result<(String, Admission)> {
    // A persistent browser receipt does not authorize stopping services, resetting
    // the rollback floor, or overriding the fixed trusted release key.
    let snapshot = MaintenanceSnapshot::observe()?;
    let (record, bytes) = load(
        &Path::new("/").join(STATE).join(UPDATE_INBOX_NAME),
        CORE_UID,
    )?;
    snapshot.business.check_web_update_owner(&record.binding)?;
    let key = read_root_release_key()?;
    let schema = u32::try_from(snapshot.business.version.format).map_err(|_| Error::Schema)?;
    let signed = bundle::verify_for_admission(&bytes, &key, schema, 0)?;
    if signed.epoch() != record.upload.epoch
        || manifest_hash(signed.manifest) != record.upload.manifest_sha256
    {
        return Err(Error::Integrity);
    }
    let mut store = Store::open(Path::new(APP), key, schema)?;
    if signed.epoch() <= store.floor() {
        return Err(Error::Rollback);
    }
    let manifest = signed.manifest.to_vec();
    let signature = signed.signature.to_vec();
    let payloads: Vec<_> = signed.payloads.into_iter().map(<[u8]>::to_vec).collect();
    drop(bytes);
    let result = admit_stage(&mut store, &snapshot, &manifest, &signature, &payloads)?;
    Ok((record.upload.request_id, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_wire::{UpdateUpload, UpdateUploadBinding};
    use std::os::unix::{fs::symlink, fs::PermissionsExt};

    #[test]
    fn web_upload_intake_checks_owner_file_bounds_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        let uid = rustix::process::geteuid().as_raw();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join(UPDATE_INBOX_ACCEPTED);
        let data = b"candidate bytes";
        let record = UpdateUploadRecord {
            format: 1,
            binding: UpdateUploadBinding {
                realm: "fixture".into(),
                owner_key: [7; 32],
                generation: 1,
            },
            upload: UpdateUpload {
                request_id: "12345678-1234-4234-8234-123456789abc".into(),
                manifest_sha256: "a".repeat(64),
                bundle_sha256: manifest_hash(data),
                epoch: 115,
                length: data.len() as u32,
            },
        };
        let header = serde_json::to_vec(&record).unwrap();
        let mut bytes = (header.len() as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(data);
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let (read, uploaded) = load(dir.path(), uid).unwrap();
        assert_eq!(read.upload, record.upload);
        assert_eq!(uploaded, data);
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, &bytes).unwrap();
        assert!(matches!(load(dir.path(), uid), Err(Error::Integrity)));
        let moved = dir.path().join("other");
        fs::rename(&path, &moved).unwrap();
        symlink(&moved, &path).unwrap();
        assert!(load(dir.path(), uid).is_err());
    }

    #[test]
    fn web_upload_admission_rechecks_current_owner_under_database_fence() {
        let dir = tempfile::tempdir().unwrap();
        let identity = tempfile::tempdir().unwrap();
        for p in [dir.path(), identity.path()] {
            fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let store = rustsimhub_storage::Store::open(dir.path()).unwrap();
        let mut auth = rustsimhub_auth::Auth::open(identity.path(), store.handle()).unwrap();
        let signer = SigningKey::from_bytes(&[101; 32]);
        let key = signer.verifying_key().to_bytes();
        let c = auth.bootstrap_challenge(key).unwrap();
        let owner = auth
            .bootstrap_finish(&c.id, &signer.sign(&c.transcript).to_bytes())
            .unwrap();
        let binding = UpdateUploadBinding {
            realm: auth.durable_realm().unwrap(),
            owner_key: key,
            generation: owner.generation,
        };
        drop(auth);
        drop(store);
        let guard = Database::observe(
            dir.path(),
            "store.sqlite3",
            rustix::process::geteuid().as_raw(),
            true,
            512 * 1024 * 1024,
        )
        .unwrap();
        assert!(guard.check_web_update_owner(&binding).is_ok());
        let generation = UpdateUploadBinding {
            generation: binding.generation + 1,
            ..binding.clone()
        };
        assert!(matches!(
            guard.check_web_update_owner(&generation),
            Err(Error::Denied)
        ));
        let realm = UpdateUploadBinding {
            realm: "other-realm".into(),
            ..binding
        };
        assert!(matches!(
            guard.check_web_update_owner(&realm),
            Err(Error::Denied)
        ));
    }
}
