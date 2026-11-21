// SPDX-License-Identifier: AGPL-3.0-only
//! Allow roaming on an existing, inactive ordinary-data NetworkManager profile.
use super::linux::{checked, command, publish, uptime_ms, Result, BASE, INPUT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

const PERMIT: &str = "roaming-permit.json";
const NM: &str = "/usr/bin/nmcli";

#[derive(Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Permit {
    profile: String,
    sim: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    request_id: String,
    boot_id: String,
    expires_uptime_ms: u64,
    version: String,
    enabled: bool,
}

struct Profile {
    uuid: String,
    home_only: bool,
}

fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn profile() -> Result<Option<Profile>> {
    let listing = command(
        NM,
        &["-t", "-f", "UUID,TYPE,ACTIVE", "connection", "show"],
        None,
    )?;
    let mut candidates = listing.lines().filter_map(|line| {
        let (id, fields) = line.split_once(':')?;
        let (kind, active) = fields.split_once(':')?;
        (kind == "gsm").then_some((id, active))
    });
    let Some((id, active)) = candidates.next() else {
        return Ok(None);
    };
    // An active bearer is never interrupted or silently changed by this switch.
    if candidates.next().is_some() || !uuid(id) || active != "no" {
        return Ok(None);
    }
    let show = command(
        NM,
        &[
            "-g",
            "gsm.apn,gsm.auto-config,connection.autoconnect,gsm.home-only",
            "connection",
            "show",
            "uuid",
            id,
        ],
        None,
    )?;
    let mut values = show.lines();
    let Some(apn) = values.next() else {
        return Ok(None);
    };
    if apn.is_empty()
        || apn.len() > 100
        || !apn
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
        || matches!(
            apn.to_ascii_lowercase().as_str(),
            "ims" | "sos" | "emergency" | "xcap"
        )
        || values.next() != Some("no")
        || values.next() != Some("no")
    {
        return Ok(None);
    }
    let home_only = match values.next() {
        Some("yes") => true,
        Some("no") => false,
        _ => return Ok(None),
    };
    Ok(Some(Profile {
        uuid: id.into(),
        home_only,
    }))
}

fn apply(profile: &Profile, enabled: bool) -> Result<()> {
    let current = crate::roaming::profile()?.ok_or("data profile no longer eligible")?;
    if current.uuid != profile.uuid {
        return Err("data profile changed".into());
    }
    if current.home_only == enabled {
        command(
            NM,
            &[
                "-w",
                "3",
                "connection",
                "modify",
                "uuid",
                &profile.uuid,
                "gsm.home-only",
                if enabled { "no" } else { "yes" },
            ],
            None,
        )?;
    }
    match crate::roaming::profile()? {
        Some(after) if after.uuid == profile.uuid && after.home_only != enabled => Ok(()),
        _ => Err("roaming setting could not be verified".into()),
    }
}

pub(crate) struct Monitor {
    updated: Option<Instant>,
    last_request: String,
    result: Option<bool>,
    status: Value,
}
impl Monitor {
    pub(crate) fn new() -> Self {
        Self {
            updated: None,
            last_request: String::new(),
            result: None,
            status: json!({"available":false}),
        }
    }
    pub(crate) fn pending(&self, boot: &str) -> Option<Request> {
        let path = Path::new(INPUT).join("roaming.json");
        checked(&path, 986, 0o640)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .filter(|r: &Request| {
                uuid(&r.request_id)
                    && r.request_id != self.last_request
                    && r.boot_id == boot
                    && uptime_ms()
                        .is_ok_and(|n| n < r.expires_uptime_ms && r.expires_uptime_ms - n <= 15000)
            })
    }
    pub(crate) fn tick(&mut self, sim: &str, request: Option<Request>) -> Value {
        if request.is_none()
            && self
                .updated
                .is_some_and(|v| v.elapsed() < Duration::from_secs(10))
        {
            return self.status.clone();
        }
        self.updated = Some(Instant::now());
        self.status = self
            .update(sim, request.as_ref())
            .unwrap_or_else(|_| json!({"available":false,"reason":"当前数据连接无法安全修改"}));
        if let Some(r) = request {
            self.last_request = r.request_id;
            self.result = Some(
                self.status["available"] == true
                    && self.status["enabled"] == r.enabled
                    && self.status["version"] == r.version,
            );
        }
        self.status["last_request_id"] = json!(self.last_request);
        self.status["applied"] = json!(self.result);
        self.status.clone()
    }
    fn update(&self, sim: &str, request: Option<&Request>) -> Result<Value> {
        // No connection profile is created by this feature.
        let Some(mut p) = profile()? else {
            return Ok(
                json!({"available":false,"reason":"需要一份关闭自动连接且当前未连接的普通蜂窝数据配置"}),
            );
        };
        let path = Path::new(BASE).join(PERMIT);
        let permit = Permit {
            profile: p.uuid.clone(),
            sim: sim.to_owned(),
        };
        let approved = checked(&path, 0, 0o600)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Permit>(&raw).ok())
            .is_some_and(|saved| !sim.is_empty() && saved == permit);
        if !approved && !p.home_only {
            apply(&p, false)?;
            p.home_only = true;
        }
        if sim.is_empty() {
            return Ok(json!({"available":false,"reason":"SIM 状态暂不可用"}));
        }
        let version = format!("{:x}", Sha256::digest(format!("{}:{}", p.uuid, sim)));
        if let Some(r) = request.filter(|r| r.version == version) {
            apply(&p, r.enabled)?;
            if r.enabled {
                if let Err(error) = publish(BASE, PERMIT, &permit, 0o600) {
                    apply(&p, false)?;
                    return Err(error);
                }
            } else if path.try_exists()? {
                fs::remove_file(path)?;
            }
            p.home_only = !r.enabled;
        }
        Ok(json!({"available":true,"enabled":!p.home_only,"version":version}))
    }
}
