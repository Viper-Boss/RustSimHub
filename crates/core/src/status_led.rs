// SPDX-License-Identifier: AGPL-3.0-only
//! Core publishes expiring, content-free hints. It never opens GPIO/sysfs.
use rustsimhub_auth::{Auth, Error};
use rustsimhub_policy::Scope;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
const INPUT: &str = "/run/rustsimhub-led-input";
const LEDGER: &str = "/var/lib/rustsimhub-core/state/indicator-read.json";
type Result<T> = std::result::Result<T, Error>;
#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    sms: BTreeSet<String>,
    calls: BTreeSet<String>,
    connected: BTreeSet<String>,
}
#[derive(Default)]
struct State {
    views: HashMap<(String, String), Instant>,
    ledger: Option<Ledger>,
    dirty: bool,
    snapshot: Option<rustsimhub_storage::IndicatorSnapshot>,
    version: Option<(i64, i64, i64)>,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}
fn directory(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    if !m.is_dir() || m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o777 != 0o700 {
        return Err(Error::Forbidden);
    }
    Ok(())
}
/// The LED runs as root without capabilities, so it reads through group root:
/// setgid 2750 owned by Core. A legacy 0700 directory still publishes, unread.
fn input_directory(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path).map_err(|_| Error::Io)?;
    let shared = m.mode() & 0o7777 == 0o2750 && m.gid() == 0;
    if !m.is_dir()
        || m.uid() != rustix::process::geteuid().as_raw()
        || !(shared || m.mode() & 0o7777 == 0o700)
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}
fn enabled() -> bool {
    input_directory(Path::new(INPUT)).is_ok()
}
fn checked_read(path: &Path) -> Result<Vec<u8>> {
    checked_read_mode(path, &[0o600])
}
fn checked_read_mode(path: &Path, modes: &[u32]) -> Result<Vec<u8>> {
    let f = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(|_| Error::Io)?;
    let m = f.metadata().map_err(|_| Error::Io)?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || !modes.contains(&(m.mode() & 0o7777))
        || m.nlink() != 1
        || m.len() > 4_194_304
    {
        return Err(Error::Forbidden);
    }
    let mut bytes = Vec::new();
    f.take(4_194_305)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > 4_194_304 {
        return Err(Error::InvalidInput);
    }
    Ok(bytes)
}
fn atomic(path: &Path, bytes: &[u8], persistent: bool) -> Result<()> {
    let parent = path.parent().ok_or(Error::Io)?;
    // Persistent ledger stays private; the runtime hint is group-readable.
    let mode = if persistent {
        directory(parent)?;
        0o600
    } else {
        input_directory(parent)?;
        0o640
    };
    match fs::symlink_metadata(path) {
        Ok(_) => {
            // A hint left by an older Core (0600) is replaced, not trusted.
            checked_read_mode(path, &[mode, 0o600])?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Error::Io),
    }
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| Error::Io)?;
    let name: String = nonce.iter().map(|v| format!("{v:02x}")).collect();
    let temp = parent.join(format!(".indicator-{name}.pending"));
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(mode)
        .open(&temp)
        .map_err(|_| Error::Io)?;
    f.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(|_| Error::Io)?;
    f.write_all(bytes).map_err(|_| Error::Io)?;
    if persistent {
        f.sync_all().map_err(|_| Error::Io)?;
    }
    fs::rename(&temp, path).map_err(|_| Error::Io)?;
    if persistent {
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| Error::Io)?;
    }
    Ok(())
}
fn valid_id(value: &str) -> bool {
    value.len() == 36 && value.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}
fn validate(ledger: &Ledger) -> Result<()> {
    if [&ledger.sms, &ledger.calls, &ledger.connected]
        .iter()
        .any(|set| set.len() > 32768 || set.iter().any(|id| !valid_id(id)))
    {
        return Err(Error::InvalidInput);
    }
    Ok(())
}
fn load(s: &mut State, snapshot: &rustsimhub_storage::IndicatorSnapshot) -> Result<()> {
    if s.ledger.is_some() {
        return Ok(());
    }
    let ledger = match fs::symlink_metadata(LEDGER) {
        Ok(_) => {
            let value: Ledger = serde_json::from_slice(&checked_read(Path::new(LEDGER))?)
                .map_err(|_| Error::InvalidInput)?;
            validate(&value)?;
            value
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Adopt historical records as the baseline, once only.
            s.dirty = true;
            Ledger {
                sms: snapshot.sms.iter().cloned().collect(),
                calls: snapshot.calls.iter().map(|r| r.0.clone()).collect(),
                connected: BTreeSet::new(),
            }
        }
        Err(_) => return Err(Error::Io),
    };
    s.ledger = Some(ledger);
    Ok(())
}
fn flush(s: &mut State) -> Result<()> {
    if s.dirty {
        let ledger = s.ledger.as_ref().ok_or(Error::Io)?;
        validate(ledger)?;
        let bytes = serde_json::to_vec(ledger).map_err(|_| Error::Io)?;
        atomic(Path::new(LEDGER), &bytes, true)?;
        s.dirty = false;
    }
    Ok(())
}
pub fn configuration(
    auth: &mut Auth,
    token: &str,
    view: String,
    active: bool,
) -> Result<serde_json::Value> {
    let p = auth.authorize(token, Scope::NetworkManage)?;
    if !valid_id(&view) || !enabled() {
        return Err(Error::Forbidden);
    }
    let mut s = state().lock().map_err(|_| Error::Busy)?;
    let now = Instant::now();
    s.views.retain(|_, until| *until > now);
    let key = (p.id, view);
    if active {
        if s.views.len() >= 16 && !s.views.contains_key(&key) {
            return Err(Error::Busy);
        }
        s.views.insert(key, now + Duration::from_secs(25));
    } else {
        s.views.remove(&key);
    }
    Ok(serde_json::json!({"ok":true,"expires_in_seconds":25}))
}
pub fn read(id: &str, sms: bool) -> Result<()> {
    if !enabled() || !valid_id(id) {
        return Err(Error::Forbidden);
    }
    let mut s = state().lock().map_err(|_| Error::Busy)?;
    let added = {
        let ledger = s.ledger.as_mut().ok_or(Error::Busy)?;
        let set = if sms {
            &mut ledger.sms
        } else {
            &mut ledger.calls
        };
        if set.len() >= 32768 && !set.contains(id) {
            return Err(Error::Busy);
        }
        set.insert(id.to_owned())
    };
    s.dirty |= added;
    if let Err(error) = flush(&mut s) {
        if added {
            let ledger = s.ledger.as_mut().ok_or(Error::Busy)?;
            (if sms {
                &mut ledger.sms
            } else {
                &mut ledger.calls
            })
            .remove(id);
            s.dirty = true;
        }
        return Err(error);
    }
    Ok(())
}
pub fn connected(id: &str) {
    if !enabled() || !valid_id(id) {
        return;
    }
    let Ok(mut s) = state().lock() else {
        return;
    };
    if let Some(ledger) = s.ledger.as_mut() {
        if ledger.connected.len() < 32768 && ledger.connected.insert(id.to_owned()) {
            s.dirty = true;
        }
        let _ = flush(&mut s);
    }
}
fn indicators(
    ledger: &mut Ledger,
    snapshot: &rustsimhub_storage::IndicatorSnapshot,
) -> (bool, bool, bool) {
    let mut changed = false;
    for (id, _, phase) in &snapshot.calls {
        if *phase == 4 && ledger.connected.len() < 32768 {
            changed |= ledger.connected.insert(id.clone());
        }
    }
    let sms = snapshot.sms.iter().any(|id| !ledger.sms.contains(id));
    let calls = snapshot.calls.iter().any(|(id, direction, phase)| {
        *direction == 1
            && *phase == 7
            && !ledger.connected.contains(id)
            && !ledger.calls.contains(id)
    });
    (sms, calls, changed)
}
/// An incoming call that is ringing now (modem phase 3), for the LED flash.
fn ringing(snapshot: &rustsimhub_storage::IndicatorSnapshot) -> bool {
    snapshot
        .calls
        .iter()
        .any(|(_, direction, phase)| *direction == 1 && *phase == 3)
}
fn publish(store: &rustsimhub_storage::Handle) -> Result<()> {
    let version = store.indicator_version().map_err(|_| Error::Io)?;
    let mut s = state().lock().map_err(|_| Error::Busy)?;
    let snapshot = if s.version == Some(version) && s.snapshot.is_some() {
        s.snapshot.take().ok_or(Error::Io)?
    } else {
        store.indicator_snapshot().map_err(|_| Error::Io)?
    };
    load(&mut s, &snapshot)?;
    let now = Instant::now();
    s.views.retain(|_, end| *end > now);
    let configuring = !s.views.is_empty();
    let (sms, calls, changed) = indicators(s.ledger.as_mut().ok_or(Error::Io)?, &snapshot);
    let ringing = ringing(&snapshot);
    s.dirty |= changed;
    flush(&mut s)?;
    s.snapshot = Some(snapshot);
    s.version = Some(version);
    let uptime = fs::read_to_string("/proc/uptime").map_err(|_| Error::Io)?;
    let ms = uptime
        .split('.')
        .next()
        .ok_or(Error::Io)?
        .parse::<u64>()
        .map_err(|_| Error::Io)?
        .checked_mul(1000)
        .ok_or(Error::Io)?;
    let bytes = serde_json::to_vec(&serde_json::json!({"expires_ms":ms+10_000,
        "configuring":configuring,"unread_sms":sms,"missed_calls":calls,"ringing":ringing}))
    .map_err(|_| Error::Io)?;
    atomic(&Path::new(INPUT).join("state.json"), &bytes, false)
}
pub async fn run(store: rustsimhub_storage::Handle) {
    if !enabled() {
        std::future::pending::<()>().await;
        return;
    }
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let h = store.clone();
        // Optional hints expire on failure; they cannot stop calls or messaging.
        let _ = tokio::task::spawn_blocking(move || publish(&h)).await;
    }
}
pub async fn initialize(store: rustsimhub_storage::Handle) {
    if enabled() {
        let _ = tokio::task::spawn_blocking(move || publish(&store)).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const SMS: &str = "11111111-1111-4111-8111-111111111111";
    const CALL: &str = "22222222-2222-4222-8222-222222222222";
    #[test]
    fn read_ack_and_answered_calls_do_not_create_missed_notifications() {
        let mut l = Ledger::default();
        let mut s = rustsimhub_storage::IndicatorSnapshot {
            sms: vec![SMS.into()],
            calls: vec![(CALL.into(), 1, 7)],
        };
        assert_eq!(indicators(&mut l, &s), (true, true, false));
        l.sms.insert(SMS.into());
        assert_eq!(indicators(&mut l, &s), (false, true, false));
        s.calls[0].2 = 4;
        assert_eq!(indicators(&mut l, &s), (false, false, true));
        s.calls[0].2 = 7;
        assert_eq!(indicators(&mut l, &s), (false, false, false));
        l.connected.clear();
        l.calls.insert(CALL.into());
        assert_eq!(indicators(&mut l, &s), (false, false, false));
        s.calls[0].1 = 2;
        l.calls.clear();
        assert_eq!(indicators(&mut l, &s), (false, false, false));
    }
    #[test]
    fn only_a_ringing_incoming_call_flashes() {
        let mut s = rustsimhub_storage::IndicatorSnapshot {
            sms: vec![],
            calls: vec![(CALL.into(), 1, 3)],
        };
        assert!(ringing(&s));
        for (direction, phase) in [(2, 3), (1, 4), (1, 7), (2, 2)] {
            s.calls[0] = (CALL.into(), direction, phase);
            assert!(!ringing(&s));
        }
    }
    #[test]
    fn runtime_hint_is_group_readable_and_replaces_a_legacy_private_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input");
        fs::create_dir(&input).unwrap();
        fs::set_permissions(&input, fs::Permissions::from_mode(0o700)).unwrap();
        let path = input.join("state.json");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        atomic(&path, b"{}", false).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o7777, 0o640);
        assert_eq!(fs::read(&path).unwrap(), b"{}");
        // Wrong mode on an existing hint or a world-accessible directory is refused.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(atomic(&path, b"{}", false).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        fs::set_permissions(&input, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(atomic(&path, b"{}", false).is_err());
        assert!(input_directory(&input).is_err());
        // The persistent ledger keeps its private 0600 mode.
        let ledger = dir.path().join("ledger");
        fs::create_dir(&ledger).unwrap();
        fs::set_permissions(&ledger, fs::Permissions::from_mode(0o700)).unwrap();
        atomic(&ledger.join("l.json"), b"{}", true).unwrap();
        assert_eq!(
            fs::metadata(ledger.join("l.json")).unwrap().mode() & 0o7777,
            0o600
        );
    }
    #[test]
    fn ledger_round_trip_and_path_payload_rejection() {
        let mut l = Ledger::default();
        l.sms.insert(SMS.into());
        let copy: Ledger = serde_json::from_slice(&serde_json::to_vec(&l).unwrap()).unwrap();
        assert!(copy.sms.contains(SMS));
        assert!(validate(&copy).is_ok());
        l.connected.insert("/etc/passwd".into());
        assert!(validate(&l).is_err());
    }
}
