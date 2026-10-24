// SPDX-License-Identifier: AGPL-3.0-only
//! Foreground, single-thread worker. Only a Root-granted capability can obtain
//! a task. Root still owns unit/cgroup stop and must verify actual process exit.
use crate::{
    egress::{BoundConnector, Route},
    worker_ipc::Client,
    DeliveryOutcome, Error, HttpsSender, Result, Secret,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{sync::watch, time::sleep};
use tokio_rustls::rustls::pki_types::{pem::PemObject, CertificateDer};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub core_socket: PathBuf,
    pub core_uid: u32,
    pub lease: Secret,
    pub route: Route,
    pub ca_bundle: PathBuf,
}
pub fn read_settings(input: &mut impl Read) -> Result<Settings> {
    let mut bytes = zeroize::Zeroizing::new(vec![]);
    input.take(65_537).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > 65_536 {
        return Err(Error::Invalid);
    }
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn read_service_settings() -> Result<Settings> {
    let base = std::env::var_os("CREDENTIALS_DIRECTORY")
        .map(PathBuf::from)
        .ok_or(Error::Forbidden)?;
    let metadata = std::fs::symlink_metadata(&base)?;
    let uid = rustix::process::geteuid().as_raw();
    if uid == 0
        || !base.is_absolute()
        || !metadata.is_dir()
        || metadata.mode() & 0o022 != 0
        || ![0, uid].contains(&metadata.uid())
    {
        return Err(Error::Forbidden);
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(base.join("forwarding.json"))?;
    let m = file.metadata()?;
    if !m.is_file()
        || ![0, uid].contains(&m.uid())
        || m.nlink() != 1
        || m.mode() & 0o222 != 0
        || m.len() > 65536
    {
        return Err(Error::Forbidden);
    }
    read_settings(&mut file)
}
fn certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    if !path.is_absolute() {
        return Err(Error::Invalid);
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file() || m.uid() != 0 || m.mode() & 0o022 != 0 || m.len() > 1024 * 1024 {
        return Err(Error::Forbidden);
    }
    let mut bytes = vec![];
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid);
    }
    let mut roots = vec![];
    for item in CertificateDer::pem_slice_iter(&bytes) {
        roots.push(item.map_err(|_| Error::Invalid)?.into_owned());
        if roots.len() > 512 {
            return Err(Error::Invalid);
        }
    }
    if roots.is_empty() {
        return Err(Error::Invalid);
    }
    Ok(roots)
}
fn unprivileged() -> Result<()> {
    if rustix::process::geteuid().as_raw() == 0
        || rustix::process::getuid() != rustix::process::geteuid()
    {
        return Err(Error::Forbidden);
    }
    let status = std::fs::read_to_string("/proc/self/status")?;
    for field in ["CapEff:", "CapPrm:", "CapAmb:"] {
        let value = status
            .lines()
            .find_map(|line| line.strip_prefix(field))
            .ok_or(Error::Forbidden)?;
        if u64::from_str_radix(value.trim(), 16).ok() != Some(0) {
            return Err(Error::Forbidden);
        }
    }
    Ok(())
}
pub async fn run(settings: Settings, stop: impl std::future::Future<Output = ()>) -> Result<()> {
    unprivileged()?;
    if settings.core_uid == 0
        || settings.core_uid == rustix::process::geteuid().as_raw()
        || !settings.core_socket.is_absolute()
        || settings.lease.expose().len() != 64
    {
        return Err(Error::Invalid);
    }
    let sender = HttpsSender::new(certificates(&settings.ca_bundle)?)?;
    let mut connector = BoundConnector::new(settings.route)?;
    connector.probe()?;
    let mut client = Client::new(settings.core_socket, settings.core_uid, settings.lease);
    tokio::pin!(stop);
    let mut busy = 0;
    loop {
        let acquired =
            tokio::select! {biased; _=&mut stop=>return Ok(()),result=client.acquire()=>result};
        let claim = match acquired {
            Ok(Some(claim)) => claim,
            Ok(None) => {
                busy = 0;
                tokio::select! {biased; _=&mut stop=>return Ok(()),_=sleep(Duration::from_secs(1))=>{}}
                continue;
            }
            Err(Error::Forbidden) => return Ok(()), // Revocation is a normal lifecycle end.
            Err(Error::Busy) if busy < 3 => {
                busy += 1;
                tokio::select! {biased; _=&mut stop=>return Ok(()),_=sleep(Duration::from_millis(250<<busy))=>{}}
                continue;
            }
            Err(e) => return Err(e),
        };
        busy = 0;
        let (outcome, stopped) = {
            let (cancel, receiver) = watch::channel(false);
            let delivery = sender.deliver(&claim, &mut connector, &mut client, receiver);
            tokio::pin!(delivery);
            tokio::select! {
                biased;
                _=&mut stop=>{let _=cancel.send(true);(delivery.await,true)},
                result=&mut delivery=>(result,false),
            }
        };
        let mut receipt = client.finish(&claim, outcome).await;
        for attempt in 0..3 {
            if !matches!(receipt, Err(Error::Busy)) {
                break;
            }
            sleep(Duration::from_millis(100 << attempt)).await;
            receipt = client.finish(&claim, outcome).await;
        }
        if matches!(receipt, Err(Error::Forbidden)) {
            return Ok(());
        }
        receipt?;
        if stopped {
            return Ok(());
        }
        if !matches!(outcome, DeliveryOutcome::Confirmed { .. }) {
            // No loop over failed destinations or automatic repetition of an
            // ambiguous task. The durable Core receipt remains authoritative.
            return Err(Error::Forbidden);
        }
        tokio::select! {biased; _=&mut stop=>return Ok(()),_=sleep(Duration::from_secs(1))=>{}}
    }
}
