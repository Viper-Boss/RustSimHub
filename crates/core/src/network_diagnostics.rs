// SPDX-License-Identifier: AGPL-3.0-only
//! Authenticated finite projection of a Root monitor; no shells or hardware descriptors.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_policy::Scope;
use rustsimhub_wire::AuthCommand;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const INPUT: &str = "/run/rustsimhub-network-input";
const STATUS: &str = "/run/rustsimhub-network/status.json";
static WRITER: Mutex<()> = Mutex::new(());
type Result<T> = rustsimhub_auth::Result<T>;
fn read_status(path: &Path, uid: u32, mode: u32) -> Result<Value> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(path)
        .map_err(|_| Error::Io)?;
    let m = f.metadata().map_err(|_| Error::Io)?;
    if !m.is_file()
        || m.uid() != uid
        || m.mode() & 0o777 != mode
        || m.nlink() != 1
        || m.len() > 60000
    {
        return Err(Error::Forbidden);
    }
    let mut b = vec![];
    f.take(60001).read_to_end(&mut b).map_err(|_| Error::Io)?;
    if b.len() > 60000 {
        return Err(Error::InvalidInput);
    }
    serde_json::from_slice(&b).map_err(|_| Error::InvalidInput)
}
fn snapshot() -> Result<Value> {
    let v = read_status(Path::new(STATUS), 0, 0o640)?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").map_err(|_| Error::Io)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Io)?
        .as_secs();
    if v["schema"] != 1
        || v["boot_id"].as_str() != Some(boot.trim())
        || !v["sample_unix_seconds"]
            .as_u64()
            .is_some_and(|t| t <= now + 2 && now.saturating_sub(t) <= 30)
    {
        return Err(Error::Io);
    }
    Ok(v)
}
fn publish(
    request_id: &str,
    enabled: bool,
    limit_bytes: u64,
    reset_day: u8,
    acknowledge_meter_gap: bool,
) -> Result<()> {
    if request_id.len() != 36
        || !request_id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() || b == b'-')
        || !(1..=31).contains(&reset_day)
        || !(1..=100 * 1024 * 1024 * 1024 * 1024u64).contains(&limit_bytes)
    {
        return Err(Error::InvalidInput);
    }
    let _guard = WRITER.try_lock().map_err(|_| Error::Busy)?;
    let status = snapshot()?;
    if status["last_request_id"] == request_id {
        return Ok(());
    }
    let uptime = uptime_ms()?;
    let value = json!({"schema":1,"request_id":request_id,"boot_id":status["boot_id"],"expires_uptime_ms":uptime+15000,"policy":{"enabled":enabled,"limit_bytes":limit_bytes,"reset_day":reset_day},"acknowledge_meter_gap":acknowledge_meter_gap});
    write_input("policy.json", request_id, &value)
}
fn uptime_ms() -> Result<u64> {
    fs::read_to_string("/proc/uptime")
        .map_err(|_| Error::Io)?
        .split('.')
        .next()
        .ok_or(Error::Io)?
        .parse::<u64>()
        .ok()
        .and_then(|v| v.checked_mul(1000))
        .ok_or(Error::Io)
}
fn write_input(file: &str, request_id: &str, value: &Value) -> Result<()> {
    let dir = Path::new(INPUT);
    let m = fs::symlink_metadata(dir).map_err(|_| Error::Io)?;
    if !m.is_dir()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o777 != 0o750
        || m.gid() != 0
    {
        return Err(Error::Forbidden);
    }
    let tmp = dir.join(format!(".{file}-{request_id}.pending"));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o640)
        .open(&tmp)
        .map_err(|_| Error::Io)?;
    f.set_permissions(fs::Permissions::from_mode(0o640))
        .map_err(|_| Error::Io)?;
    f.write_all(&serde_json::to_vec(&value).map_err(|_| Error::InvalidInput)?)
        .map_err(|_| Error::Io)?;
    f.sync_all().map_err(|_| Error::Io)?;
    drop(f);
    fs::rename(tmp, dir.join(file)).map_err(|_| Error::Io)?;
    Ok(())
}
struct RoamingConfirmation {
    session: [u8; 32],
    code: String,
    version: String,
    deadline: Instant,
}
impl RoamingConfirmation {
    fn matches(
        &self,
        token: &str,
        version: &str,
        confirmation: Option<&str>,
        now: Instant,
    ) -> bool {
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        Some(self.code.as_str()) == confirmation
            && self.session == digest
            && self.version == version
            && now < self.deadline
    }
}
static ROAMING_CONFIRMATION: Mutex<Option<RoamingConfirmation>> = Mutex::new(None);

fn random_hex() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::Io)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn roaming_state() -> Value {
    snapshot()
        .ok()
        .and_then(|v| v.get("data_roaming").cloned())
        .filter(Value::is_object)
        .unwrap_or_else(
            || json!({"available":false,"reason":"设备网络服务暂未提供国际数据漫游控制"}),
        )
}
fn prepare_roaming(token: String) -> Result<Value> {
    let state = roaming_state();
    let version = state["version"]
        .as_str()
        .filter(|v| v.len() == 64)
        .ok_or(Error::InvalidInput)?;
    if state["available"] != true || state["enabled"] != false {
        return Err(Error::InvalidInput);
    }
    let code = random_hex()?;
    *ROAMING_CONFIRMATION.try_lock().map_err(|_| Error::Busy)? = Some(RoamingConfirmation {
        session: Sha256::digest(token.as_bytes()).into(),
        code: code.clone(),
        version: version.into(),
        deadline: Instant::now() + Duration::from_secs(60),
    });
    Ok(json!({"confirmation":code,"version":version,"expires_in_seconds":60}))
}
fn set_roaming(token: &str, enabled: bool, confirmation: Option<&str>) -> Result<Value> {
    let _writer = WRITER.try_lock().map_err(|_| Error::Busy)?;
    let state = snapshot()?;
    let roaming = &state["data_roaming"];
    let version = roaming["version"]
        .as_str()
        .filter(|v| v.len() == 64)
        .ok_or(Error::InvalidInput)?;
    if roaming["available"] != true {
        return Err(Error::InvalidInput);
    }
    let mut confirmed = ROAMING_CONFIRMATION.try_lock().map_err(|_| Error::Busy)?;
    if enabled {
        let valid = confirmed
            .as_ref()
            .is_some_and(|pending| pending.matches(token, version, confirmation, Instant::now()));
        if !valid {
            return Err(Error::Forbidden);
        }
    }
    *confirmed = None;
    let raw = random_hex()?;
    let id = format!(
        "{}-{}-{}-{}-{}",
        &raw[..8],
        &raw[8..12],
        &raw[12..16],
        &raw[16..20],
        &raw[20..]
    );
    let value = json!({"request_id":id,"boot_id":state["boot_id"],"expires_uptime_ms":uptime_ms()?+15000,"version":version,"enabled":enabled});
    write_input("roaming.json", &id, &value)?;
    Ok(json!({"request_id":id,"state":"pending"}))
}

pub(crate) async fn authorized(auth: Arc<Mutex<Auth>>, command: AuthCommand) -> Result<Value> {
    tokio::task::spawn_blocking(move || {
        let mut a=auth.try_lock().map_err(|_|Error::Busy)?;
        match command {
            AuthCommand::NetworkDiagnostics{token}=>{
                a.authorize(&token,Scope::NetworkManage)?;
                match snapshot(){Ok(v)=>Ok(v),Err(_)=>Ok(json!({"schema":1,"available":false,"reason":"网络监测服务未启动或数据已过期"}))}
            },
            AuthCommand::SetNetworkPolicy{token,request_id,enabled,limit_bytes,reset_day,acknowledge_meter_gap}=>{
                let owner=a.recent_owner(&token)?;
                if a.authorize(&token,Scope::NetworkManage)?!=owner {return Err(Error::Forbidden);}
                publish(&request_id,enabled,limit_bytes,reset_day,acknowledge_meter_gap)?;
                Ok(json!({"request_id":request_id,"state":"pending","apn_activated":false}))
            },
            AuthCommand::DataRoamingStatus{token}=>{
                let owner = a.owner_session(&token)?;
                drop(a);
                let state = roaming_state();
                if auth.try_lock().map_err(|_| Error::Busy)?.owner_session(&token)? != owner {
                    return Err(Error::Forbidden);
                }
                Ok(state)
            },
            AuthCommand::PrepareDataRoaming{token}=>{
                a.recent_owner(&token)?;
                prepare_roaming(token)
            },
            AuthCommand::SetDataRoaming{token,enabled,confirmation}=>{
                a.recent_owner(&token)?;
                set_roaming(&token,enabled,confirmation.as_deref())
            },
            _=>Err(Error::InvalidInput)
        }
    }).await.map_err(|_|Error::Busy)?
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn roaming_confirmation_is_required_current_and_session_bound() {
        let now = Instant::now();
        let pending = RoamingConfirmation {
            session: Sha256::digest(b"owner-session").into(),
            code: "proof".into(),
            version: "sim-profile".into(),
            deadline: now + Duration::from_secs(60),
        };
        assert!(pending.matches("owner-session", "sim-profile", Some("proof"), now));
        assert!(!pending.matches("owner-session", "sim-profile", None, now));
        assert!(!pending.matches("other-session", "sim-profile", Some("proof"), now));
        assert!(!pending.matches("owner-session", "new-sim-profile", Some("proof"), now));
        assert!(!pending.matches(
            "owner-session",
            "sim-profile",
            Some("proof"),
            pending.deadline
        ));
    }
    #[test]
    fn status_rejects_links_permissions_oversize_and_invalid_json() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("status");
        fs::write(&p, b"{}").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let uid = rustix::process::geteuid().as_raw();
        assert!(read_status(&p, uid, 0o600).is_ok());
        assert!(read_status(&p, uid, 0o640).is_err());
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        assert!(read_status(&link, uid, 0o600).is_err());
        fs::hard_link(&p, d.path().join("hard")).unwrap();
        assert!(read_status(&p, uid, 0o600).is_err());
        fs::remove_file(d.path().join("hard")).unwrap();
        fs::write(&p, vec![b' '; 60001]).unwrap();
        assert!(read_status(&p, uid, 0o600).is_err());
        fs::write(&p, b"not-json").unwrap();
        assert!(read_status(&p, uid, 0o600).is_err());
    }
}
