// SPDX-License-Identifier: AGPL-3.0-only
use super::{Context, Proof};
use crate::{Error, Result};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    os::unix::fs::MetadataExt,
    path::Path,
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

pub(super) fn program(path: &Path) -> Result<()> {
    let m = fs::metadata(path)?; // Production selected is an installer-owned bind mount.
    if !path.is_absolute()
        || !m.is_file()
        || m.uid() != 0
        || m.mode() & 0o022 != 0
        || m.mode() & 0o111 == 0
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
async fn command(args: &[&str], budget: Duration) -> Result<(bool, String)> {
    let mut child = Command::new("/usr/bin/systemctl")
        .args(["--no-ask-password", "--no-pager", "--system"])
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("SYSTEMD_COLORS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut out = child.stdout.take().ok_or(Error::Storage)?.take(16385);
    let mut bytes = vec![];
    let result = timeout(budget, async {
        out.read_to_end(&mut bytes).await?;
        let status = child.wait().await?;
        if bytes.len() > 16384 {
            return Err(Error::Invalid);
        }
        Ok((
            status.success(),
            String::from_utf8(bytes).map_err(|_| Error::Invalid)?,
        ))
    })
    .await;
    match result {
        Ok(v) => v,
        Err(_) => {
            let _ = child.kill().await;
            Err(Error::Busy)
        }
    }
}
struct Unit {
    load: String,
    active: String,
    main: u32,
    control: u32,
    group: String,
    file: String,
    triggered: String,
    triggers: String,
    kill: String,
    delegate: String,
    restart: String,
    job: String,
    service_fields: bool,
}
impl Unit {
    fn parse(text: &str) -> Result<Self> {
        let mut fields = BTreeMap::new();
        for line in text.lines() {
            let (key, value) = line.split_once('=').ok_or(Error::Invalid)?;
            if fields.insert(key, value).is_some() {
                return Err(Error::Invalid);
            }
        }
        let get = |key| fields.get(key).copied().ok_or(Error::Invalid);
        if get("LoadState")? == "not-found" {
            return Ok(Self {
                load: "not-found".into(),
                active: "inactive".into(),
                main: 0,
                control: 0,
                group: String::new(),
                file: String::new(),
                triggered: String::new(),
                triggers: String::new(),
                kill: String::new(),
                delegate: String::new(),
                restart: String::new(),
                job: "0".into(),
                service_fields: false,
            });
        }
        let service = |key| fields.get(key).copied().unwrap_or("");
        let pid = |key| match fields.get(key) {
            None => Ok(0),
            Some(value) => value.parse::<u32>().map_err(|_| Error::Invalid),
        };
        Ok(Self {
            load: get("LoadState")?.into(),
            active: get("ActiveState")?.into(),
            main: pid("MainPID")?,
            control: pid("ControlPID")?,
            group: service("ControlGroup").into(),
            file: get("UnitFileState")?.into(),
            triggered: get("TriggeredBy")?.into(),
            triggers: get("Triggers")?.into(),
            kill: service("KillMode").into(),
            delegate: service("Delegate").into(),
            restart: service("Restart").into(),
            job: get("Job")?.into(),
            service_fields: fields.contains_key("MainPID") && fields.contains_key("ControlPID"),
        })
    }
    fn inert(&self) -> bool {
        self.load == "not-found"
            || self.load == "loaded"
                && ["inactive", "failed"].contains(&self.active.as_str())
                && self.main == 0
                && self.control == 0
                && (self.job.is_empty() || self.job == "0")
                && ["static", "disabled", "masked"].contains(&self.file.as_str())
                && self.triggered.is_empty()
                && self.triggers.is_empty()
    }
}
pub(super) struct System<'a> {
    c: &'a Context,
}
impl<'a> System<'a> {
    pub(super) fn new(c: &'a Context) -> Self {
        Self { c }
    }
    async fn unit(&self, name: &str) -> Result<Unit> {
        let (ok,text)=command(&["show","--property=LoadState,ActiveState,MainPID,ControlPID,ControlGroup,UnitFileState,TriggeredBy,Triggers,KillMode,Delegate,Restart,Job","--",name],Duration::from_secs(3)).await?;
        let state = Unit::parse(&text)?;
        if !ok && state.load != "not-found" {
            return Err(Error::Forbidden);
        }
        Ok(state)
    }
    fn triggers(&self) -> [String; 3] {
        let stem = self.c.unit.strip_suffix(".service").unwrap_or("");
        [
            format!("{stem}.socket"),
            format!("{stem}.timer"),
            format!("{stem}.path"),
        ]
    }
    async fn triggers_inert(&self) -> Result<()> {
        for name in self.triggers() {
            if !self.unit(&name).await?.inert() {
                return Err(Error::Forbidden);
            }
        }
        Ok(())
    }
    pub(super) async fn start(&self) -> Result<()> {
        self.triggers_inert().await?;
        let (ok, _) = command(&["start", "--", &self.c.unit], Duration::from_secs(18)).await?;
        if !ok {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    pub(super) async fn stop(&self) -> Result<()> {
        for name in self.triggers().into_iter().chain([self.c.unit.clone()]) {
            if self.unit(&name).await?.load != "not-found" {
                let (ok, _) = command(&["stop", "--", &name], Duration::from_secs(18)).await?;
                if !ok {
                    return Err(Error::Forbidden);
                }
            }
        }
        self.empty().await
    }
    pub(super) async fn empty(&self) -> Result<()> {
        self.triggers_inert().await?;
        let unit = self.unit(&self.c.unit).await?;
        if !unit.inert()
            || unit.load == "loaded" && !unit.service_fields
            || !self.pids()?.is_empty()
            || !workers(self.c.worker_uid)?.is_empty()
        {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    fn pids(&self) -> Result<HashSet<u32>> {
        let base = if Path::new("/sys/fs/cgroup/cgroup.controllers").is_file() {
            Path::new("/sys/fs/cgroup")
        } else if Path::new("/sys/fs/cgroup/systemd/cgroup.procs").is_file() {
            Path::new("/sys/fs/cgroup/systemd")
        } else {
            return Err(Error::Forbidden);
        };
        let path = base.join("system.slice").join(&self.c.unit);
        let mut todo = vec![path];
        let mut seen = 0;
        let mut all = HashSet::new();
        while let Some(path) = todo.pop() {
            seen += 1;
            if seen > 64 {
                return Err(Error::Invalid);
            }
            let iter = match fs::read_dir(&path) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            for item in iter {
                let item = item?;
                let kind = item.file_type()?;
                if kind.is_symlink() {
                    return Err(Error::Forbidden);
                }
                if kind.is_dir() {
                    todo.push(item.path());
                }
            }
            let text = match fs::read_to_string(path.join("cgroup.procs")) {
                Ok(text) => text,
                // PID1 may remove an already-empty cgroup during this read.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if text.len() > 32768 {
                return Err(Error::Invalid);
            }
            for pid in text.lines() {
                all.insert(pid.parse().map_err(|_| Error::Invalid)?);
            }
            if all.len() > 4096 {
                return Err(Error::Invalid);
            }
        }
        Ok(all)
    }
    pub(super) async fn running(&self, p: &Proof) -> Result<()> {
        if self.live_worker().await? != p.worker_pid || ticks(p.worker_pid)? != p.worker_ticks {
            return Err(Error::Forbidden);
        }
        Ok(())
    }
    pub(super) async fn live_worker(&self) -> Result<u32> {
        self.triggers_inert().await?;
        let u = self.unit(&self.c.unit).await?;
        if u.load != "loaded"
            || u.active != "active"
            || u.main == 0
            || u.control != 0
            || !u.job.is_empty() && u.job != "0"
            || u.group != format!("/system.slice/{}", self.c.unit)
            || u.kill != "control-group"
            || u.delegate != "no"
            || u.restart != "no"
            || !u.triggered.is_empty()
            || !u.triggers.is_empty()
            || !["static", "disabled"].contains(&u.file.as_str())
            || self.pids()? != HashSet::from([u.main])
            || workers(self.c.worker_uid)? != HashSet::from([u.main])
        {
            return Err(Error::Forbidden);
        }
        worker(u.main, self.c)?;
        Ok(u.main)
    }
}
pub(super) fn ticks(pid: u32) -> Result<u64> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    text.rsplit_once(')')
        .and_then(|(_, s)| s.split_whitespace().nth(19))
        .ok_or(Error::Invalid)?
        .parse()
        .map_err(|_| Error::Invalid)
}
pub(super) fn worker(pid: u32, context: &Context) -> Result<()> {
    let text = fs::read_to_string(format!("/proc/{pid}/status"))?;
    let get = |field| {
        text.lines()
            .find_map(|s| s.strip_prefix(field))
            .ok_or(Error::Invalid)
    };
    let ids: Vec<u32> = get("Uid:")?
        .split_whitespace()
        .map(|s| s.parse().map_err(|_| Error::Invalid))
        .collect::<Result<_>>()?;
    if ids != vec![context.worker_uid; 4] {
        return Err(Error::Forbidden);
    }
    let gids: Vec<u32> = get("Gid:")?
        .split_whitespace()
        .map(|s| s.parse().map_err(|_| Error::Invalid))
        .collect::<Result<_>>()?;
    let groups: HashSet<u32> = get("Groups:")?
        .split_whitespace()
        .map(|s| s.parse().map_err(|_| Error::Invalid))
        .collect::<Result<_>>()?;
    if gids != vec![context.worker_gid; 4]
        || !groups.contains(&context.worker_group)
        || groups
            .iter()
            .any(|gid| *gid != context.worker_gid && *gid != context.worker_group)
    {
        return Err(Error::Forbidden);
    }
    for field in ["CapEff:", "CapPrm:", "CapAmb:", "CapInh:", "CapBnd:"] {
        if u64::from_str_radix(get(field)?.trim(), 16).ok() != Some(0) {
            return Err(Error::Forbidden);
        }
    }
    if get("NoNewPrivs:")?.trim() != "1" {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn workers(uid: u32) -> Result<HashSet<u32>> {
    let mut all = HashSet::new();
    let mut count = 0;
    for entry in fs::read_dir("/proc")? {
        count += 1;
        if count > 65536 {
            return Err(Error::Invalid);
        }
        let entry = entry?;
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        match fs::read_to_string(entry.path().join("status")) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
            Ok(text) => {
                let ids = text
                    .lines()
                    .find_map(|s| s.strip_prefix("Uid:"))
                    .ok_or(Error::Invalid)?;
                if ids
                    .split_whitespace()
                    .any(|s| s.parse::<u32>().ok() == Some(uid))
                {
                    all.insert(pid);
                }
            }
        }
    }
    Ok(all)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_needs_inactive_no_pid_no_job_and_no_start_trigger() {
        let base="LoadState=loaded\nActiveState=inactive\nMainPID=0\nControlPID=0\nControlGroup=\nUnitFileState=static\nTriggeredBy=\nTriggers=\nKillMode=control-group\nDelegate=no\nRestart=no\nJob=0\n";
        assert!(Unit::parse(base).unwrap().inert());
        assert!(Unit::parse(&base.replace("Job=0", "Job=")).unwrap().inert());
        for (from, to) in [
            ("inactive", "active"),
            ("MainPID=0", "MainPID=2"),
            ("ControlPID=0", "ControlPID=2"),
            ("Job=0", "Job=4"),
            ("UnitFileState=static", "UnitFileState=enabled"),
            ("TriggeredBy=\n", "TriggeredBy=unknown.timer\n"),
        ] {
            assert!(!Unit::parse(&base.replace(from, to)).unwrap().inert());
        }
        assert!(Unit::parse(&(base.to_owned() + "MainPID=0\n")).is_err());
        assert!(Unit::parse(&base.replace("MainPID=0", "MainPID=invalid")).is_err());
        assert!(Unit::parse(&base.replace("ControlPID=0", "ControlPID=invalid")).is_err());
    }
}
