// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed account policy and current local/NSS observations, never package IDs.
//! No production method mutates NSS or chooses an unused numeric identity.
use super::{
    rootio::{FileSnapshot, Root},
    Error, Result,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const POLICY: &str = "/etc/rustsimhub/product-roles.json";
const PASSWD: &str = "/etc/passwd";
const GROUP: &str = "/etc/group";
const SWITCH: &str = "/etc/nsswitch.conf";
const LIMIT: usize = 65536;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Number {
    uid: u32,
    gid: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Roles {
    ble: Number,
    nkn: Number,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    format: u16,
    roles: Roles,
}
#[derive(Debug)]
struct User {
    name: String,
    uid: u32,
    gid: u32,
}
#[derive(Debug)]
struct Group {
    name: String,
    gid: u32,
    members: Vec<String>,
}

pub struct NssObservation {
    policy: FileSnapshot,
    passwd: FileSnapshot,
    group: FileSnapshot,
    switch: FileSnapshot,
    nss_passwd: Vec<u8>,
    nss_group: Vec<u8>,
    policy_hash: String,
    ble: u32,
    nkn: u32,
    require_optional: bool,
    isolated: bool,
}
impl NssObservation {
    #[cfg(test)]
    pub fn observe(root: &Root) -> Result<Self> {
        Self::observe_for(root, true)
    }
    pub fn observe_for(root: &Root, require_optional: bool) -> Result<Self> {
        if root.is_isolated() {
            return Err(Error::Denied);
        }
        Self::capture(root, require_optional, false)
    }
    #[cfg(test)]
    pub fn observe_isolated(root: &Root) -> Result<Self> {
        Self::observe_isolated_for(root, true)
    }
    #[cfg(test)]
    pub fn observe_isolated_for(root: &Root, require_optional: bool) -> Result<Self> {
        if !root.is_isolated() {
            return Err(Error::Denied);
        }
        Self::capture(root, require_optional, true)
    }
    fn capture(root: &Root, require_optional: bool, isolated: bool) -> Result<Self> {
        let policy = required(root, POLICY)?;
        if policy.stamp.mode != 0o600 {
            return Err(Error::Denied);
        }
        let p: Policy = serde_json::from_slice(&policy.bytes).map_err(|_| Error::Invalid)?;
        validate_policy(&p)?;
        let passwd = required(root, PASSWD)?;
        let group = required(root, GROUP)?;
        let switch = required(root, SWITCH)?;
        validate_switch(&switch.bytes)?;
        let (nss_passwd, nss_group) = nss_bytes(root, isolated)?;
        for (users, groups) in [(&passwd.bytes, &group.bytes), (&nss_passwd, &nss_group)] {
            validate_identities(users, groups, &p, require_optional)?;
        }
        let local_users = parse_users(&passwd.bytes)?;
        let local_groups = parse_groups(&group.bytes)?;
        let nss_users = parse_users(&nss_passwd)?;
        let nss_groups = parse_groups(&nss_group)?;
        // Exact identities/supplementary memberships for relevant roles must
        // agree. Unrelated NSS accounts may exist, but cannot alias these IDs.
        for (name, id) in base().into_iter().chain([
            ("rustsimhub-ble", p.roles.ble.uid),
            ("rustsimhub-nkn", p.roles.nkn.uid),
        ]) {
            let a = local_users.iter().find(|u| u.name == name);
            let b = nss_users.iter().find(|u| u.name == name);
            if a.map(|u| (u.uid, u.gid)) != b.map(|u| (u.uid, u.gid)) {
                return Err(Error::Conflict);
            }
            let membership = |groups: &[Group]| -> BTreeSet<u32> {
                groups
                    .iter()
                    .filter(|g| g.members.iter().any(|m| m == name))
                    .map(|g| g.gid)
                    .collect()
            };
            if membership(&local_groups) != membership(&nss_groups) {
                return Err(Error::Conflict);
            }
            if local_groups.iter().find(|g| g.name == name).map(|g| g.gid)
                != nss_groups.iter().find(|g| g.name == name).map(|g| g.gid)
            {
                return Err(Error::Conflict);
            }
            let _ = id;
        }
        let policy_hash = digest(&policy.bytes);
        let observation = Self {
            policy,
            passwd,
            group,
            switch,
            nss_passwd,
            nss_group,
            policy_hash,
            ble: p.roles.ble.uid,
            nkn: p.roles.nkn.uid,
            require_optional,
            isolated,
        };
        observation.revalidate(root)?;
        Ok(observation)
    }
    pub fn policy_sha256(&self) -> &str {
        &self.policy_hash
    }
    pub fn ble_uid(&self) -> u32 {
        self.ble
    }
    pub fn nkn_uid(&self) -> u32 {
        self.nkn
    }
    pub fn baseline_sha256(&self) -> String {
        let mut hash = Sha256::new();
        for bytes in [
            &self.passwd.bytes,
            &self.group.bytes,
            &self.switch.bytes,
            &self.nss_passwd,
            &self.nss_group,
        ] {
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        format!("{:x}", hash.finalize())
    }
    pub fn revalidate(&self, root: &Root) -> Result<()> {
        for (path, snapshot) in [
            (POLICY, &self.policy),
            (PASSWD, &self.passwd),
            (GROUP, &self.group),
            (SWITCH, &self.switch),
        ] {
            snapshot.recheck(root, path)?;
        }
        let (passwd, group) = nss_bytes(root, self.isolated)?;
        if passwd != self.nss_passwd || group != self.nss_group {
            return Err(Error::Conflict);
        }
        let policy: Policy =
            serde_json::from_slice(&self.policy.bytes).map_err(|_| Error::Invalid)?;
        validate_identities(&passwd, &group, &policy, self.require_optional)
    }
}
fn required(root: &Root, path: &str) -> Result<FileSnapshot> {
    root.read(path, LIMIT)?.ok_or(Error::Denied)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn base() -> [(&'static str, u32); 4] {
    [
        ("rustsimhub-core", 986),
        ("rustsimhub-edge", 985),
        ("rustsimhub-media", 984),
        ("rustsimhub-forwarder", 983),
    ]
}
fn validate_policy(policy: &Policy) -> Result<()> {
    if policy.format != 1 {
        return Err(Error::Unsupported);
    }
    let mut ids = BTreeSet::new();
    for value in [&policy.roles.ble, &policy.roles.nkn] {
        if value.uid != value.gid
            || value.uid == u32::MAX
            || [0, 29, 983, 984, 985, 986, 987, 65534].contains(&value.uid)
            || !ids.insert(value.uid)
        {
            return Err(Error::Denied);
        }
    }
    Ok(())
}
fn validate_switch(bytes: &[u8]) -> Result<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let mut seen = BTreeSet::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((database, sources)) = line.split_once(':') else {
            return Err(Error::Invalid);
        };
        let database = database.trim();
        if database != "passwd" && database != "group" {
            continue;
        }
        if !seen.insert(database) {
            return Err(Error::Invalid);
        }
        let sources = sources.split_whitespace().collect::<Vec<_>>();
        if sources.is_empty()
            || !sources.contains(&"files")
            || sources.iter().any(|s| *s != "files" && *s != "systemd")
            || sources.iter().copied().collect::<BTreeSet<_>>().len() != sources.len()
        {
            return Err(Error::Unsupported);
        }
    }
    if seen.len() != 2 {
        return Err(Error::Invalid);
    }
    Ok(())
}
fn number(raw: &str) -> Result<u32> {
    let value = raw.parse::<u32>().map_err(|_| Error::Invalid)?;
    if value == u32::MAX || value.to_string() != raw {
        return Err(Error::Invalid);
    }
    Ok(value)
}
fn rows(bytes: &[u8], width: usize) -> Result<Vec<Vec<&str>>> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err(Error::Invalid);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid)?;
    let mut names = BTreeSet::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let values = line.split(':').collect::<Vec<_>>();
        if values.len() != width
            || values[0].is_empty()
            || values[0].len() > 64
            || !values[0]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.$".contains(&b))
            || !names.insert(values[0])
        {
            return Err(Error::Invalid);
        }
        rows.push(values);
    }
    if rows.is_empty() {
        return Err(Error::Invalid);
    }
    Ok(rows)
}
fn parse_users(bytes: &[u8]) -> Result<Vec<User>> {
    rows(bytes, 7)?
        .into_iter()
        .map(|r| {
            Ok(User {
                name: r[0].into(),
                uid: number(r[2])?,
                gid: number(r[3])?,
            })
        })
        .collect()
}
fn parse_groups(bytes: &[u8]) -> Result<Vec<Group>> {
    rows(bytes, 4)?
        .into_iter()
        .map(|r| {
            let members = if r[3].is_empty() {
                Vec::new()
            } else {
                r[3].split(',').map(str::to_owned).collect::<Vec<_>>()
            };
            if members.iter().any(String::is_empty)
                || members.iter().collect::<BTreeSet<_>>().len() != members.len()
            {
                return Err(Error::Invalid);
            }
            Ok(Group {
                name: r[0].into(),
                gid: number(r[2])?,
                members,
            })
        })
        .collect()
}
fn validate_identities(
    passwd: &[u8],
    group: &[u8],
    policy: &Policy,
    required_optional: bool,
) -> Result<()> {
    let users = parse_users(passwd)?;
    let groups = parse_groups(group)?;
    for (name, uid) in base() {
        validate_role(&users, &groups, name, uid, true, uid == 984)?;
    }
    let ipc = groups
        .iter()
        .filter(|g| g.name == "rustsimhub-ipc" || g.gid == 987)
        .collect::<Vec<_>>();
    if ipc.len() != 1
        || ipc[0].name != "rustsimhub-ipc"
        || ipc[0].gid != 987
        || ipc[0]
            .members
            .iter()
            .any(|m| !base().iter().any(|(name, _)| *name == m))
        || users.iter().any(|u| u.gid == 987)
    {
        return Err(Error::Denied);
    }
    for (name, value) in [
        ("rustsimhub-ble", &policy.roles.ble),
        ("rustsimhub-nkn", &policy.roles.nkn),
    ] {
        validate_role(&users, &groups, name, value.uid, required_optional, false)?;
    }
    Ok(())
}
fn validate_role(
    users: &[User],
    groups: &[Group],
    name: &str,
    uid: u32,
    required: bool,
    audio: bool,
) -> Result<()> {
    let selected = users
        .iter()
        .filter(|u| u.name == name || u.uid == uid || u.gid == uid)
        .collect::<Vec<_>>();
    let matching = groups
        .iter()
        .filter(|g| g.name == name || g.gid == uid)
        .collect::<Vec<_>>();
    if !required && selected.is_empty() && matching.is_empty() {
        return Ok(());
    }
    if selected.len() != 1
        || selected[0].name != name
        || selected[0].uid != uid
        || selected[0].gid != uid
        || matching.len() != 1
        || matching[0].name != name
        || matching[0].gid != uid
        || matching[0].members.iter().any(|m| m != name)
    {
        return Err(Error::Denied);
    }
    let audio_exists = audio
        && groups
            .iter()
            .filter(|g| g.name == "audio" || g.gid == 29)
            .map(|g| (g.name.as_str(), g.gid))
            .collect::<Vec<_>>()
            == vec![("audio", 29)];
    for group in groups
        .iter()
        .filter(|g| g.members.iter().any(|m| m == name))
    {
        let base_role = base().iter().any(|(candidate, _)| *candidate == name);
        if group.gid != uid
            && !(base_role && group.name == "rustsimhub-ipc" && group.gid == 987)
            && !(audio_exists && group.name == "audio" && group.gid == 29)
        {
            return Err(Error::Denied);
        }
    }
    Ok(())
}
fn nss_bytes(root: &Root, isolated: bool) -> Result<(Vec<u8>, Vec<u8>)> {
    if root.is_isolated() != isolated {
        return Err(Error::Denied);
    }
    if isolated {
        #[cfg(test)]
        {
            if !root.is_isolated() {
                return Err(Error::Denied);
            }
            let passwd = root
                .read("/run/product-install-test/nss-passwd", LIMIT)?
                .map(|s| s.bytes)
                .unwrap_or(required(root, PASSWD)?.bytes);
            let group = root
                .read("/run/product-install-test/nss-group", LIMIT)?
                .map(|s| s.bytes)
                .unwrap_or(required(root, GROUP)?.bytes);
            return Ok((passwd, group));
        }
        #[cfg(not(test))]
        return Err(Error::Denied);
    }
    Ok((getent("passwd")?, getent("group")?))
}
struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn getent(database: &str) -> Result<Vec<u8>> {
    if !matches!(database, "passwd" | "group") {
        return Err(Error::Denied);
    }
    let mut command = Command::new("/usr/bin/getent");
    command.arg(database);
    capture(command)
}
#[cfg(test)]
pub fn test_getent_capture(command: Command) -> Result<Vec<u8>> {
    capture(command)
}
fn capture(mut command: Command) -> Result<Vec<u8>> {
    command
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = Reap(command.spawn().map_err(|_| Error::Unsupported)?);
    let mut pipe = child.0.stdout.take().ok_or(Error::Io)?;
    let flags = rustix::fs::fcntl_getfl(&pipe).map_err(|_| Error::Io)?;
    rustix::fs::fcntl_setfl(&pipe, flags | rustix::fs::OFlags::NONBLOCK).map_err(|_| Error::Io)?;
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut bytes = Vec::new();
    let mut eof = false;
    loop {
        if Instant::now() >= deadline {
            return Err(Error::Busy);
        }
        let mut buffer = [0; 1024];
        match pipe.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(n) => {
                if bytes.len() + n > LIMIT {
                    return Err(Error::Invalid);
                }
                bytes.extend_from_slice(&buffer[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(_) => return Err(Error::Io),
        }
        if let Some(status) = child.0.try_wait().map_err(|_| Error::Io)? {
            if !status.success() {
                return Err(Error::Io);
            }
            if eof {
                return Ok(bytes);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Explicit private-root test operation only. There is no production account
/// creation API or default UID allocator; the approved policy must pre-exist.
#[cfg(test)]
pub fn prepare_accounts(root: &Root) -> Result<()> {
    if !root.is_isolated() {
        return Err(Error::Denied);
    }
    let policy = required(root, POLICY)?;
    if policy.stamp.mode != 0o600 {
        return Err(Error::Denied);
    }
    let p: Policy = serde_json::from_slice(&policy.bytes).map_err(|_| Error::Invalid)?;
    validate_policy(&p)?;
    let passwd = required(root, PASSWD)?;
    let group = required(root, GROUP)?;
    let mut users = parse_users(&passwd.bytes)?;
    let mut groups = parse_groups(&group.bytes)?;
    for (name, uid) in base() {
        validate_role(&users, &groups, name, uid, true, uid == 984)?;
    }
    let mut new_passwd = String::from_utf8(passwd.bytes.clone()).map_err(|_| Error::Invalid)?;
    let mut new_group = String::from_utf8(group.bytes.clone()).map_err(|_| Error::Invalid)?;
    for (name, value) in [
        ("rustsimhub-ble", &p.roles.ble),
        ("rustsimhub-nkn", &p.roles.nkn),
    ] {
        let present = users
            .iter()
            .any(|u| u.name == name || u.uid == value.uid || u.gid == value.uid)
            || groups.iter().any(|g| g.name == name || g.gid == value.uid);
        if present {
            validate_role(&users, &groups, name, value.uid, true, false)?;
            continue;
        }
        if !new_passwd.ends_with('\n') {
            new_passwd.push('\n');
        }
        if !new_group.ends_with('\n') {
            new_group.push('\n');
        }
        new_passwd.push_str(&format!(
            "{name}:x:{}:{}:Product optional role:/nonexistent:/usr/sbin/nologin\n",
            value.uid, value.gid
        ));
        new_group.push_str(&format!("{name}:x:{}:\n", value.gid));
        users.push(User {
            name: name.into(),
            uid: value.uid,
            gid: value.gid,
        });
        groups.push(Group {
            name: name.into(),
            gid: value.gid,
            members: Vec::new(),
        });
    }
    validate_identities(new_passwd.as_bytes(), new_group.as_bytes(), &p, true)?;
    policy.recheck(root, POLICY)?;
    passwd.recheck(root, PASSWD)?;
    group.recheck(root, GROUP)?;
    if new_passwd.as_bytes() == passwd.bytes && new_group.as_bytes() == group.bytes {
        return Ok(());
    }
    let installed_group =
        root.replace(GROUP, Some(&group), new_group.as_bytes(), group.stamp.mode)?;
    if let Err(error) = root.replace(
        PASSWD,
        Some(&passwd),
        new_passwd.as_bytes(),
        passwd.stamp.mode,
    ) {
        // Restore only our known intermediate group inode. A later writer is
        // preserved by Root::replace CAS; cleanup failure is not called undo.
        root.replace(
            GROUP,
            Some(&installed_group),
            &group.bytes,
            group.stamp.mode,
        )
        .map_err(|_| Error::Unknown)?;
        return Err(error);
    }
    Ok(())
}
