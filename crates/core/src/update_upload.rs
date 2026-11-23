// SPDX-License-Identifier: AGPL-3.0-only
//! Durable, single-slot receipt for an explicitly confirmed browser upload.
//! This inbox publishes bytes for maintenance and never selects an application.
use crate::linux::write_frame;
use rustix::fs::{OFlags, RenameFlags, CWD};
use rustsimhub_auth::Auth;
use rustsimhub_policy::{allows, Scope};
use rustsimhub_wire::{
    AuthCommand, AuthRequest, Response, UpdateRunPhase, UpdateRunRecord, UpdateUpload,
    UpdateUploadBinding as Binding, UpdateUploadRecord as Record, AUTH_PROTOCOL,
    UPDATE_INBOX_ACCEPTED as RECEIPT, UPDATE_INBOX_HEADER_LIMIT as HEADER_LIMIT, UPDATE_RUN_LIMIT,
    UPDATE_RUN_PATH,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{io::AsyncReadExt, net::UnixStream, sync::Semaphore, time::timeout};

type Outcome<T> = Result<T, &'static str>;
const SPARE: &str = ".receiving";

pub(crate) struct Inbox {
    directory: PathBuf,
    incoming: Arc<Semaphore>,
}
impl Inbox {
    #[cfg(test)]
    pub(crate) fn new(state_directory: &Path) -> Self {
        Self::at(state_directory.join(rustsimhub_wire::UPDATE_INBOX_NAME))
    }

    pub(crate) fn for_device() -> Option<Self> {
        let enabled = std::env::var_os("RUSTSIMHUB_WEB_UPDATE_HEALTH")?;
        if enabled != "1" {
            return None;
        }
        let inbox = Self::at(PathBuf::from("/var/lib/rustsimhub-core/update-upload"));
        // Never fall back to the small business-data partition. The optional
        // Root unit pre-creates and bind-mounts the dedicated application spool.
        inbox
            .check_directory()
            .ok()
            .filter(|present| *present)
            .map(|_| inbox)
    }

    fn at(directory: PathBuf) -> Self {
        Self {
            directory,
            incoming: Arc::new(Semaphore::new(1)),
        }
    }

    fn check_directory(&self) -> Outcome<bool> {
        match fs::symlink_metadata(&self.directory) {
            Ok(meta) if meta.is_dir() && owned(&meta, 0o700) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            _ => Err("inbox_unavailable"),
        }
    }

    fn current(&self) -> Outcome<Option<Record>> {
        if !self.check_directory()? {
            return Ok(None);
        }
        let Some(mut f) = read_file(&self.directory.join(RECEIPT))? else {
            return Ok(None);
        };
        let mut length = [0; 4];
        f.read_exact(&mut length).map_err(|_| "inbox_unavailable")?;
        let length = u32::from_be_bytes(length) as usize;
        if !(1..=HEADER_LIMIT).contains(&length) {
            return Err("inbox_unavailable");
        }
        let mut header = vec![0; length];
        f.read_exact(&mut header).map_err(|_| "inbox_unavailable")?;
        let record: Record = serde_json::from_slice(&header).map_err(|_| "inbox_unavailable")?;
        let size = f.metadata().map_err(|_| "inbox_unavailable")?.len();
        if record.format != 1
            || !record.upload.valid()
            || size != 4 + length as u64 + u64::from(record.upload.length)
        {
            return Err("inbox_unavailable");
        }
        Ok(Some(record))
    }

    fn publish(
        &self,
        record: &Record,
        bytes: &[u8],
        reauthorize: impl FnOnce() -> Outcome<()>,
    ) -> Outcome<Value> {
        if let Some(saved) = self.current()? {
            reauthorize()?;
            return if saved.binding == record.binding && saved.upload == record.upload {
                Ok(receipt(&saved))
            } else {
                Err("update_pending")
            };
        }
        if !self.check_directory()? {
            DirBuilder::new()
                .mode(0o700)
                .create(&self.directory)
                .map_err(|_| "inbox_unavailable")?;
            sync_dir(self.directory.parent().ok_or("inbox_unavailable")?)?;
        }
        let available = fs2::available_space(&self.directory).map_err(|_| "inbox_unavailable")?;
        if available < bytes.len() as u64 + 16 * 1024 * 1024 {
            return Err("inbox_unavailable");
        }
        let pending = self.directory.join(SPARE);
        if read_file(&pending)?.is_some() {
            // The single writer permit makes this an incomplete prior transfer.
            fs::remove_file(&pending).map_err(|_| "inbox_unavailable")?;
        }
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)
            .map_err(|_| "inbox_unavailable")?;
        let result = (|| {
            let header = serde_json::to_vec(record).map_err(|_| "invalid_update")?;
            if header.len() > HEADER_LIMIT {
                return Err("invalid_update");
            }
            f.write_all(&(header.len() as u32).to_be_bytes())
                .and_then(|()| f.write_all(&header))
                .and_then(|()| f.write_all(bytes))
                .and_then(|()| f.sync_all())
                .map_err(|_| "inbox_unavailable")?;
            reauthorize()?;
            rustix::fs::renameat_with(
                CWD,
                &pending,
                CWD,
                self.directory.join(RECEIPT),
                RenameFlags::NOREPLACE,
            )
            .map_err(|_| "inbox_unavailable")?;
            sync_dir(&self.directory)?;
            Ok(receipt(record))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&pending);
        }
        result
    }
}

fn owned(meta: &fs::Metadata, mode: u32) -> bool {
    meta.uid() == rustix::process::geteuid().as_raw() && meta.mode() & 0o7777 == mode
}

fn read_file(path: &Path) -> Outcome<Option<File>> {
    match OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
        .open(path)
    {
        Ok(file) => {
            let meta = file.metadata().map_err(|_| "inbox_unavailable")?;
            if !meta.is_file() || !owned(&meta, 0o600) || meta.nlink() != 1 {
                return Err("inbox_unavailable");
            }
            Ok(Some(file))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("inbox_unavailable"),
    }
}

fn sync_dir(path: &Path) -> Outcome<()> {
    File::open(path)
        .and_then(|dir| dir.sync_all())
        .map_err(|_| "inbox_unavailable")
}

fn receipt(record: &Record) -> Value {
    json!({"state":"received","request_id":record.upload.request_id,
        "epoch":record.upload.epoch,"manifest_sha256":record.upload.manifest_sha256})
}

fn run_phase(path: &Path, uid: u32, upload: &UpdateUpload) -> Outcome<Option<UpdateRunPhase>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("inbox_unavailable"),
    };
    let metadata = file.metadata().map_err(|_| "inbox_unavailable")?;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.mode() & 0o7777 != 0o644
        || metadata.nlink() != 1
        || metadata.len() > UPDATE_RUN_LIMIT as u64
    {
        return Err("inbox_unavailable");
    }
    let mut bytes = Vec::new();
    file.take(UPDATE_RUN_LIMIT as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "inbox_unavailable")?;
    let record: UpdateRunRecord =
        serde_json::from_slice(&bytes).map_err(|_| "inbox_unavailable")?;
    if bytes.len() > UPDATE_RUN_LIMIT || !record.valid() {
        return Err("inbox_unavailable");
    }
    Ok(record.matches(upload).then_some(record.phase))
}

fn owner(auth: &Mutex<Auth>, token: &str, fresh: bool) -> Outcome<Binding> {
    let mut auth = auth.try_lock().map_err(|_| "busy")?;
    let principal = if fresh {
        auth.recent_owner(token)
    } else {
        auth.owner_session(token)
    }
    .map_err(|error| match error {
        rustsimhub_auth::Error::Expired if fresh => "reauth_required",
        _ => "denied",
    })?;
    if !allows(principal.scopes, Scope::UpdateManage) {
        return Err("denied");
    }
    Ok(Binding {
        realm: auth.durable_realm().map_err(|_| "inbox_unavailable")?,
        owner_key: principal.public_key,
        generation: principal.generation,
    })
}

async fn submit(
    stream: &mut UnixStream,
    auth: Arc<Mutex<Auth>>,
    inbox: Arc<Inbox>,
    token: String,
    upload: UpdateUpload,
) -> Outcome<Value> {
    let expires = Instant::now() + Duration::from_secs(12);
    let binding = owner(&auth, &token, true)?;
    let permit = Arc::clone(&inbox.incoming)
        .try_acquire_owned()
        .map_err(|_| "busy")?;
    let mut bytes = vec![0; upload.length as usize];
    timeout(Duration::from_secs(8), stream.read_exact(&mut bytes))
        .await
        .map_err(|_| "incomplete_upload")?
        .map_err(|_| "incomplete_upload")?;
    let mut extra = [0; 1];
    if timeout(Duration::from_secs(1), stream.read(&mut extra))
        .await
        .map_err(|_| "incomplete_upload")?
        .map_err(|_| "incomplete_upload")?
        != 0
    {
        return Err("invalid_update");
    }
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        if format!("{:x}", Sha256::digest(&bytes)) != upload.bundle_sha256 {
            return Err("invalid_update");
        }
        let record = Record {
            format: 1,
            binding,
            upload,
        };
        inbox.publish(&record, &bytes, || {
            if Instant::now() >= expires || owner(&auth, &token, true)? != record.binding {
                return Err("denied");
            }
            Ok(())
        })
    })
    .await
    .map_err(|_| "inbox_unavailable")?
}

pub(crate) async fn handle(
    stream: &mut UnixStream,
    auth: Arc<Mutex<Auth>>,
    inbox: Option<Arc<Inbox>>,
    request: AuthRequest,
) -> Result<(), Box<dyn std::error::Error>> {
    let result = match (inbox, request.command) {
        (Some(inbox), AuthCommand::SubmitUpdateUpload { token, upload }) => {
            submit(stream, auth, inbox, token, upload).await
        }
        (Some(inbox), AuthCommand::UpdateUploadStatus { token }) => {
            tokio::task::spawn_blocking(move || {
                let binding = owner(&auth, &token, false)?;
                let record = inbox.current()?;
                if record
                    .as_ref()
                    .is_some_and(|record| record.binding != binding)
                {
                    return Err("update_pending");
                }
                let can_submit = owner(&auth, &token, true).is_ok();
                let receipt = record
                    .as_ref()
                    .map(|record| {
                        let mut value = receipt(record);
                        if let Some(phase) =
                            run_phase(Path::new(UPDATE_RUN_PATH), 0, &record.upload)?
                        {
                            value["phase"] = json!(phase);
                        }
                        Ok::<_, &'static str>(value)
                    })
                    .transpose()?;
                Ok(json!({"available":true,"can_submit":can_submit,"receipt":receipt}))
            })
            .await
            .map_err(|_| "inbox_unavailable")
            .and_then(|value| value)
        }
        (Some(inbox), AuthCommand::DiscardUpdateUpload { token, request_id }) => {
            tokio::task::spawn_blocking(move || {
                let binding = owner(&auth, &token, true)?;
                let _permit = inbox.incoming.try_acquire().map_err(|_| "busy")?;
                let record = inbox.current()?.ok_or("receipt_missing")?;
                if record.binding != binding
                    || record.upload.request_id != request_id
                    || run_phase(Path::new(UPDATE_RUN_PATH), 0, &record.upload)?
                        == Some(UpdateRunPhase::Running)
                {
                    return Err("update_pending");
                }
                fs::remove_file(inbox.directory.join(RECEIPT)).map_err(|_| "inbox_unavailable")?;
                sync_dir(&inbox.directory)?;
                Ok(json!({"discarded":true,"request_id":request_id}))
            })
            .await
            .map_err(|_| "inbox_unavailable")
            .and_then(|value| value)
        }
        _ => Err("unsupported"),
    }
    .unwrap_or_else(|error| json!({"error":error}));
    let bytes = serde_json::to_vec(&Response {
        protocol: AUTH_PROTOCOL,
        id: request.id,
        result,
    })?;
    timeout(Duration::from_secs(5), write_frame(stream, &bytes)).await??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn update_upload_status_only_reads_the_matching_root_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("current.json");
        let uid = rustix::process::geteuid().as_raw();
        let mut upload = UpdateUpload {
            request_id: "12345678-1234-4234-8234-123456789abc".into(),
            manifest_sha256: "a".repeat(64),
            bundle_sha256: "c".repeat(64),
            epoch: 115,
            length: 1,
        };
        let run = UpdateRunRecord {
            format: 1,
            request_id: upload.request_id.clone(),
            manifest_sha256: upload.manifest_sha256.clone(),
            epoch: upload.epoch,
            baseline_sha256: "b".repeat(64),
            active_timers: [true, false],
            phase: UpdateRunPhase::Running,
        };
        assert_eq!(run_phase(&path, uid, &upload), Ok(None));
        fs::write(&path, serde_json::to_vec(&run).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            run_phase(&path, uid, &upload),
            Ok(Some(UpdateRunPhase::Running))
        );
        upload.epoch += 1;
        assert_eq!(run_phase(&path, uid, &upload), Ok(None));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(run_phase(&path, uid, &upload).is_err());
        let link = dir.path().join("symlink");
        symlink(&path, &link).unwrap();
        assert!(run_phase(&link, uid, &upload).is_err());
    }
}
