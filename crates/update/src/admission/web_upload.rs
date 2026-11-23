// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed-path maintenance consumption of a Core-owned, explicitly authorized upload.
use super::*;
use crate::{
    bundle,
    store::{read_root_release_key, Store},
};
use rustsimhub_wire::{
    UpdateUpload, UpdateUploadRecord, UPDATE_INBOX_ACCEPTED, UPDATE_INBOX_HEADER_LIMIT,
};

pub(super) const APP: &str = "/var/lib/rustsimhub/rustsimhub-v1/system/app";
pub(super) const CORE_UID: u32 = 986;
pub(super) const WEB_INBOX: &str = "/opt/rustsimhub/web-update/inbox";
pub(super) const WEB_BACKUP_ROOT: &str = "/opt/rustsimhub/web-update/backup";

pub(super) fn checked_spool() -> Result<()> {
    check_spool(
        Path::new("/opt/rustsimhub/versions"),
        Path::new("/opt/rustsimhub/web-update"),
        CORE_UID,
        0,
    )
}

fn check_spool(versions: &Path, spool: &Path, core_uid: u32, root_uid: u32) -> Result<()> {
    let versions = denied_io(fs::symlink_metadata(versions))?;
    if !versions.is_dir() || versions.uid() != root_uid || versions.mode() & 0o022 != 0 {
        return Err(Error::Denied);
    }
    for (path, uid, mode) in [
        (spool.to_owned(), root_uid, 0o755),
        (spool.join("inbox"), core_uid, 0o700),
        (spool.join("backup"), root_uid, 0o700),
    ] {
        let dir = denied_io(fs::symlink_metadata(path))?;
        if !dir.is_dir()
            || dir.uid() != uid
            || dir.mode() & 0o7777 != mode
            || dir.dev() != versions.dev()
        {
            return Err(Error::Denied);
        }
    }
    Ok(())
}

struct OpenReceipt {
    record: UpdateUploadRecord,
    handle: File,
    directory: Observation,
    file: Observation,
}

fn open_receipt(path: &Path, uid: u32) -> Result<Option<OpenReceipt>> {
    let directory = Observation::capture(path)?;
    let Some(stamp) = directory.stamp.as_ref() else {
        return Ok(None);
    };
    if stamp.mode != 0o040700 || stamp.uid != uid {
        return Err(Error::Denied);
    }
    let file = Observation::capture(&path.join(UPDATE_INBOX_ACCEPTED))?;
    let Some(file_stamp) = file.stamp.as_ref() else {
        return Ok(None);
    };
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
    directory.revalidate()?;
    file.revalidate()?;
    Ok(Some(OpenReceipt {
        record,
        handle,
        directory,
        file,
    }))
}

fn load(
    path: &Path,
    uid: u32,
    request_id: &str,
    manifest_sha256: &str,
) -> Result<(UpdateUploadRecord, Vec<u8>)> {
    let OpenReceipt {
        record,
        mut handle,
        directory,
        file,
    } = open_receipt(path, uid)?.ok_or(Error::Denied)?;
    if record.upload.request_id != request_id || record.upload.manifest_sha256 != manifest_sha256 {
        return Err(Error::Denied);
    }
    let mut bytes = vec![0; record.upload.length as usize];
    denied_io(handle.read_exact(&mut bytes))?;
    directory.revalidate()?;
    file.revalidate()?;
    if file.stamp.as_ref() != Some(&Stamp::from(denied_io(handle.metadata())?))
        || manifest_hash(&bytes) != record.upload.bundle_sha256
    {
        return Err(Error::Integrity);
    }
    Ok((record, bytes))
}

pub(super) fn pending_upload() -> Result<Option<UpdateUpload>> {
    checked_spool()?;
    Ok(open_receipt(Path::new(WEB_INBOX), CORE_UID)?.map(|opened| opened.record.upload))
}

/// Verify the pending bytes before interrupting a running application group.
pub(super) fn check_web_upload(request_id: &str, manifest: &str) -> Result<()> {
    if !UpdateUpload::valid_reference(request_id, manifest) {
        return Err(Error::Invalid);
    }
    checked_spool()?;
    let (record, bytes) = load(Path::new(WEB_INBOX), CORE_UID, request_id, manifest)?;
    let state = crate::store::inspection::ReadOnlyState::observe()?;
    let (_, _, _, has_candidate) = state.runtime_version()?;
    if has_candidate {
        return Err(Error::Busy);
    }
    let bundle = bundle::verify_for_admission(&bytes, state.runtime_key()?, 6, 0)?;
    if bundle.epoch() != record.upload.epoch || manifest_hash(bundle.manifest) != manifest {
        return Err(Error::Integrity);
    }
    if bundle.epoch() <= state.floor_epoch()? {
        return Err(Error::Rollback);
    }
    Ok(())
}

pub fn admit_web_upload(request_id: &str, manifest_sha256: &str) -> Result<Admission> {
    if !UpdateUpload::valid_reference(request_id, manifest_sha256) {
        return Err(Error::Invalid);
    }
    // A persistent browser receipt does not authorize stopping services, resetting
    // the rollback floor, or overriding the fixed trusted release key.
    let snapshot = MaintenanceSnapshot::observe()?;
    checked_spool()?;
    let (record, bytes) = load(Path::new(WEB_INBOX), CORE_UID, request_id, manifest_sha256)?;
    let authorization_binding = snapshot.business.web_update_owner_proof(&record.binding)?;
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
    let baseline = store.confirmed_hash()?.to_owned();
    let manifest = signed.manifest.to_vec();
    let signature = signed.signature.to_vec();
    let payloads: Vec<_> = signed.payloads.into_iter().map(<[u8]>::to_vec).collect();
    drop(bytes);
    super::programs::admit_stage_after(
        &mut store,
        &snapshot,
        &manifest,
        &signature,
        &payloads,
        || {
            super::web_backup::save(
                Path::new(WEB_BACKUP_ROOT),
                &snapshot,
                &record.upload,
                &baseline,
                &authorization_binding,
            )
        },
    )
}

/// Called by Root maintenance after stopping the application group. A stale
/// caller cannot roll back a different trial or an already-confirmed version.
pub fn reject_web_trial(request_id: &str, manifest: &str) -> Result<bool> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    if !UpdateUpload::valid_reference(request_id, manifest) {
        return Err(Error::Invalid);
    }
    let root = Path::new(APP);
    let record = super::web_health_receipt(request_id, manifest)?;
    let mut store = Store::open_stable(root, read_root_release_key()?, record.business_schema)?;
    if record != super::web_health_receipt(request_id, manifest)? {
        return Err(Error::Busy);
    }
    store.reject_web_trial(
        manifest,
        &record.baseline_sha256,
        record.epoch,
        super::systemd::web_trial_stopped,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rustsimhub_wire::UpdateUploadBinding;
    use std::os::unix::{fs::symlink, fs::PermissionsExt};

    #[test]
    fn web_upload_spool_requires_the_slot_filesystem_and_private_directories() {
        let root = tempfile::tempdir().unwrap();
        let uid = rustix::process::geteuid().as_raw();
        let slots = root.path().join("versions");
        let spool = root.path().join("spool");
        for (path, mode) in [
            (&slots, 0o700),
            (&spool, 0o755),
            (&spool.join("inbox"), 0o700),
            (&spool.join("backup"), 0o700),
        ] {
            fs::create_dir(path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
        }
        assert_eq!(check_spool(&slots, &spool, uid, uid), Ok(()));
        fs::set_permissions(spool.join("backup"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(check_spool(&slots, &spool, uid, uid), Err(Error::Denied));
        fs::remove_dir(spool.join("backup")).unwrap();
        symlink(&slots, spool.join("backup")).unwrap();
        assert_eq!(check_spool(&slots, &spool, uid, uid), Err(Error::Denied));
    }

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
        let request = &record.upload.request_id;
        let manifest = &record.upload.manifest_sha256;
        let read_expected = || load(dir.path(), uid, request, manifest);
        let (read, uploaded) = read_expected().unwrap();
        assert!(matches!(
            load(
                dir.path(),
                uid,
                "33333333-3333-4333-8333-333333333333",
                manifest
            ),
            Err(Error::Denied)
        ));
        assert!(matches!(
            load(dir.path(), uid, request, &"b".repeat(64)),
            Err(Error::Denied)
        ));
        assert_eq!(read.upload, record.upload);
        assert_eq!(uploaded, data);
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, &bytes).unwrap();
        assert!(matches!(read_expected(), Err(Error::Integrity)));
        let moved = dir.path().join("other");
        fs::rename(&path, &moved).unwrap();
        symlink(&moved, &path).unwrap();
        assert!(read_expected().is_err());
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
        let expected_proof = serde_json::to_vec(&(
            auth.public_key(),
            binding.owner_key,
            binding.generation,
            &binding.realm,
        ))
        .unwrap();
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
        assert_eq!(
            guard.web_update_owner_proof(&binding),
            Ok(manifest_hash(&expected_proof))
        );
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
