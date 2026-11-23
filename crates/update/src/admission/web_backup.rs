// SPDX-License-Identifier: AGPL-3.0-only
//! A rolling SQLite backup for the web-requested, same-schema application trial.
use super::*;
use rustix::fs::{renameat_with, RenameFlags, CWD};
use rustsimhub_wire::UpdateUpload;
use sha2::{Digest, Sha256};
use std::{
    fs::DirBuilder,
    io::{self, Seek, SeekFrom},
    os::unix::fs::DirBuilderExt,
};

const CURRENT: &str = "web-update-backup";
const NEXT: &str = "web-update-backup.next";
const BUSINESS: &str = "business.sqlite3";
const OUTBOX: &str = "outbox.sqlite3";
const RECEIPT: &str = "receipt.json";

fn backup_directory(path: &Path, uid: u32) -> Result<bool> {
    let directory = Observation::capture(path)?;
    let Some(stamp) = directory.stamp else {
        return Ok(false);
    };
    if stamp.mode != 0o040700 || stamp.uid != uid {
        return Err(Error::Denied);
    }
    for entry in denied_io(fs::read_dir(path))? {
        let entry = denied_io(entry)?;
        let name = entry.file_name();
        let limit = match name.to_str() {
            Some(BUSINESS) => 512 * 1024 * 1024,
            Some(OUTBOX) => 32 * 1024 * 1024,
            Some(RECEIPT) => 4096,
            _ => return Err(Error::Denied),
        };
        private_file(&entry.path(), uid, 0o600, limit)?;
    }
    Ok(true)
}

fn remove_pending(path: &Path, uid: u32) -> Result<()> {
    if backup_directory(path, uid)? {
        for name in [BUSINESS, OUTBOX, RECEIPT] {
            match fs::remove_file(path.join(name)) {
                Ok(()) => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(_) => return Err(Error::Io),
            }
        }
        denied_io(fs::remove_dir(path))?;
    }
    Ok(())
}

fn copy_database(database: &Database, path: &Path, uid: u32, limit: u64) -> Result<String> {
    let mut source = database.backup_source()?;
    let len = denied_io(source.metadata())?.len();
    if len > limit {
        return Err(Error::Invalid);
    }
    let mut copy = denied_io(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path),
    )?;
    if denied_io(io::copy(&mut source.by_ref().take(len + 1), &mut copy))? != len {
        return Err(Error::Integrity);
    }
    denied_io(copy.sync_all())?;
    drop(copy);

    let mut expected = Sha256::new();
    denied_io(source.seek(SeekFrom::Start(0)))?;
    let mut actual = Sha256::new();
    let mut readback = private_file(path, uid, 0o600, len)?;
    if denied_io(io::copy(&mut source.take(len + 1), &mut expected))? != len
        || denied_io(io::copy(&mut readback, &mut actual))? != len
    {
        return Err(Error::Integrity);
    }
    let readback_digest = actual.finalize();
    if expected.finalize() != readback_digest {
        return Err(Error::Integrity);
    }
    database.revalidate()?;
    Ok(format!("{readback_digest:x}"))
}

/// Called only with the Store lock and the original stopped-writer snapshot.
/// Publish before changing the slot. A crash can leave NEXT, which is private
/// and only contains an incomplete copy or the previous verified generation.
pub(super) fn save(
    root: &Path,
    snapshot: &MaintenanceSnapshot,
    upload: &UpdateUpload,
) -> Result<()> {
    snapshot.revalidate()?;
    let uid = rustix::process::geteuid().as_raw();
    let current = root.join(CURRENT);
    let next = root.join(NEXT);
    let retained = backup_directory(&current, uid)?;
    remove_pending(&next, uid)?;
    let business_bytes = denied_io(snapshot.business.backup_source()?.metadata())?.len();
    let outbox_bytes = snapshot
        .outbox
        .as_ref()
        .map(|db| denied_io(db.backup_source()?.metadata()).map(|metadata| metadata.len()))
        .transpose()?
        .unwrap_or(0);
    // Leave room for the expanded app and subsequent database/ledger writes.
    let required = business_bytes + outbox_bytes + u64::from(upload.length) + 16 * 1024 * 1024;
    if denied_io(fs2::available_space(root))? < required {
        return Err(Error::Io);
    }
    denied_io(DirBuilder::new().mode(0o700).create(&next))?;

    let business = copy_database(
        &snapshot.business,
        &next.join(BUSINESS),
        uid,
        512 * 1024 * 1024,
    )?;
    let outbox = snapshot
        .outbox
        .as_ref()
        .map(|db| copy_database(db, &next.join(OUTBOX), uid, 32 * 1024 * 1024))
        .transpose()?;
    let report = serde_json::to_vec(&serde_json::json!({
        "format": 1,
        "request_id": upload.request_id,
        "manifest_sha256": upload.manifest_sha256,
        "business_sha256": business,
        "outbox_sha256": outbox,
    }))
    .map_err(|_| Error::Invalid)?;
    crate::store::write_new(&next.join(RECEIPT), &report, false)?;
    denied_io(File::open(&next).and_then(|f| f.sync_all()))?;
    snapshot.revalidate()?;

    let flags = if retained {
        RenameFlags::EXCHANGE
    } else {
        RenameFlags::NOREPLACE
    };
    renameat_with(CWD, &next, CWD, &current, flags).map_err(|_| Error::Io)?;
    denied_io(File::open(root).and_then(|f| f.sync_all()))?;
    if retained {
        remove_pending(&next, uid)?;
        denied_io(File::open(root).and_then(|f| f.sync_all()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn web_backup_rotates_verified_sqlite_copies_and_refuses_unknown_pending_data() {
        let fixture = super::super::tests::fixture(Mode::SetupAp, true);
        let snapshot = super::super::tests::snapshot(fixture.path());
        let root = tempfile::tempdir().unwrap();
        let mut upload = UpdateUpload {
            request_id: "12345678-1234-4234-8234-123456789abc".into(),
            manifest_sha256: "a".repeat(64),
            bundle_sha256: "b".repeat(64),
            epoch: 115,
            length: 1,
        };
        save(root.path(), &snapshot, &upload).unwrap();
        let current = root.path().join(CURRENT);
        let pending = root.path().join(NEXT);
        let read_receipt = || {
            serde_json::from_slice::<serde_json::Value>(&fs::read(current.join(RECEIPT)).unwrap())
                .unwrap()
        };
        let read_digest = |name| crate::manifest_hash(&fs::read(current.join(name)).unwrap());
        let first = read_receipt();
        assert_eq!(first["business_sha256"], read_digest(BUSINESS));
        assert_eq!(first["outbox_sha256"], read_digest(OUTBOX));
        assert_eq!(first["request_id"], upload.request_id);
        assert!(!pending.exists());

        upload.manifest_sha256 = "c".repeat(64);
        denied_io(DirBuilder::new().mode(0o700).create(&pending)).unwrap();
        symlink(current.join(OUTBOX), pending.join(OUTBOX)).unwrap();
        assert_eq!(save(root.path(), &snapshot, &upload), Err(Error::Io));
        assert!(pending.join(OUTBOX).is_symlink());
        assert_eq!(read_receipt(), first);

        fs::remove_file(pending.join(OUTBOX)).unwrap();
        save(root.path(), &snapshot, &upload).unwrap();
        assert_eq!(read_receipt()["manifest_sha256"], upload.manifest_sha256);
        assert_eq!(read_receipt()["business_sha256"], first["business_sha256"]);
        assert!(!pending.exists());
    }
}
