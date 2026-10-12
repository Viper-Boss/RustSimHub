// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only reconciliation. No Send/Create/Delete/Enable/PIN methods exist here.
use rustsimhub_auth::SimFingerprinter;
use rustsimhub_storage::{Handle, IncomingSms, LineObservation, LineStatus};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SERVICE: &str = "org.freedesktop.ModemManager1";
const MODEM: &str = "org.freedesktop.ModemManager1.Modem";
fn identity_readable(lock: u32, state: i32) -> bool {
    // MM treats PIN2/PUK2 as operational; never submit an unlock code for ordinary reads.
    matches!(lock, 1 | 3 | 5) && state >= 3
}
#[zbus::proxy(interface = "org.freedesktop.ModemManager1.Sim")]
trait Sim {
    #[zbus(property)]
    fn sim_identifier(&self) -> zbus::Result<String>;
}
#[zbus::proxy(interface = "org.freedesktop.ModemManager1.Modem.Messaging")]
trait Messaging {
    fn list(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}
struct Snapshot {
    owner: String,
    modem: OwnedObjectPath,
    sim: OwnedObjectPath,
    iccid: Option<String>,
    status: LineStatus,
}
async fn snapshot(c: &zbus::Connection) -> Result<Snapshot> {
    let bus = zbus::fdo::DBusProxy::new(c).await?;
    let owner = bus.get_name_owner(SERVICE.try_into()?).await?.to_string();
    let objects =
        zbus::fdo::ObjectManagerProxy::new(c, owner.as_str(), "/org/freedesktop/ModemManager1")
            .await?
            .get_managed_objects()
            .await?;
    if objects.len() > 1024 {
        return Err("object limit".into());
    }
    let mut modems = objects.iter().filter(|(_, v)| v.contains_key(MODEM));
    let (path, interfaces) = modems.next().ok_or("no modem")?;
    if modems.next().is_some() {
        return Err("ambiguous modem".into());
    }
    let p = &interfaces[MODEM];
    let sim = OwnedObjectPath::try_from(
        p.get("Sim")
            .ok_or("SIM property unavailable")?
            .try_clone()?,
    )?;
    let lock = u32::try_from(p.get("UnlockRequired").ok_or("lock unavailable")?)?;
    let state = i32::try_from(p.get("State").ok_or("state unavailable")?)?;
    let (iccid, status) = if sim.as_str() == "/" {
        (None, LineStatus::Absent)
    } else if !identity_readable(lock, state) {
        (
            None,
            if matches!(lock, 2 | 4 | 6..=16) {
                LineStatus::Locked
            } else {
                LineStatus::Unavailable
            },
        )
    } else {
        let simproxy = SimProxy::builder(c)
            .destination(owner.clone())?
            .path(sim.clone())?
            .build()
            .await?;
        let iccid = simproxy.sim_identifier().await?;
        if !(8..=32).contains(&iccid.len()) || !iccid.bytes().all(|b| b.is_ascii_digit()) {
            return Err("SIM identity unavailable".into());
        }
        (Some(iccid), LineStatus::Observing)
    };
    if bus.get_name_owner(SERVICE.try_into()?).await?.as_str() != owner {
        return Err("backend changed".into());
    }
    Ok(Snapshot {
        owner,
        modem: path.clone(),
        sim,
        iccid,
        status,
    })
}
async fn messages(c: &zbus::Connection, s: &Snapshot) -> Result<Vec<IncomingSms>> {
    let proxy = MessagingProxy::builder(c)
        .destination(s.owner.clone())?
        .path(s.modem.clone())?
        .build()
        .await?;
    let paths = proxy.list().await?;
    // Explicit limit: refuse oversized enumeration, preserve hardware messages for later recovery.
    if paths.len() > 256 {
        return Err("SMS enumeration limit".into());
    }
    let mut messages = Vec::new();
    for path in paths {
        let props = zbus::fdo::PropertiesProxy::builder(c)
            .destination(s.owner.clone())?
            .path(path.clone())?
            .build()
            .await?;
        let p: HashMap<String, OwnedValue> = props
            .get_all("org.freedesktop.ModemManager1.Sms".try_into()?)
            .await?;
        let state = p.get("State").and_then(|v| u32::try_from(v).ok());
        let kind = p.get("PduType").and_then(|v| u32::try_from(v).ok());
        if state != Some(3) || !matches!(kind, Some(1 | 32)) {
            continue;
        } // Complete MT deliver only.
        let string = |name: &str| -> Result<String> {
            Ok(<&str>::try_from(p.get(name).ok_or("missing SMS property")?)?.to_owned())
        };
        let data = Vec::<u8>::try_from(p.get("Data").ok_or("missing SMS data")?.try_clone()?)?;
        messages.push(IncomingSms {
            line: rustsimhub_domain::LineEpoch(0),
            source: path.to_string(),
            sender: string("Number")?,
            timestamp: string("Timestamp")?,
            text: string("Text")?,
            data,
        });
    }
    Ok(messages)
}
#[derive(Default)]
struct Stable {
    candidate: Option<(String, [u8; 32], Instant)>,
}
impl Stable {
    fn observe(&mut self, backend: &str, key: Option<[u8; 32]>, now: Instant) -> bool {
        let Some(key) = key else {
            self.candidate = None;
            return false;
        };
        match &self.candidate {
            Some((b, k, since)) if b == backend && k == &key => {
                return now.duration_since(*since) >= Duration::from_secs(2)
            }
            _ => {}
        }
        self.candidate = Some((backend.to_owned(), key, now));
        false
    }
}
pub async fn run(store: Handle, instance: String, fingerprinter: SimFingerprinter) {
    let mut stable = Stable::default();
    loop {
        // A fresh connection and unique bus owner fence every cycle; no cached modem path survives restart.
        let cycle = async {
            let c = zbus::Connection::system().await?;
            let before = snapshot(&c).await?;
            let backend = format!(
                "{instance}:{}:{}:{}",
                before.owner, before.modem, before.sim
            );
            let key = match before.iccid.as_deref() {
                Some(id) => Some(fingerprinter.fingerprint(id)?),
                None => None,
            };
            let ready = stable.observe(&backend, key, Instant::now());
            let status = if ready {
                LineStatus::Ready
            } else {
                before.status
            };
            let h = store.clone();
            let b = backend.clone();
            let view = tokio::task::spawn_blocking(move || {
                h.observe_line(LineObservation {
                    backend: b,
                    fingerprint: key,
                    status,
                })
            })
            .await??;
            if ready {
                let mut sms = messages(&c, &before).await?;
                let after = snapshot(&c).await?;
                if before.owner != after.owner
                    || before.modem != after.modem
                    || before.sim != after.sim
                    || before.iccid != after.iccid
                    || after.status != LineStatus::Observing
                {
                    return Err("SIM changed during read".into());
                }
                for message in &mut sms {
                    message.line = view.line;
                }
                let h = store.clone();
                tokio::task::spawn_blocking(
                    move || -> std::result::Result<(), rustsimhub_storage::Error> {
                        for message in sms {
                            h.receive_sms(message)?;
                        }
                        Ok(())
                    },
                )
                .await??;
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        };
        if !matches!(
            tokio::time::timeout(Duration::from_secs(12), cycle).await,
            Ok(Ok(()))
        ) {
            stable.candidate = None;
            let h = store.clone();
            let b = instance.clone();
            let _ = tokio::task::spawn_blocking(move || {
                h.observe_line(LineObservation {
                    backend: b,
                    fingerprint: None,
                    status: LineStatus::Unavailable,
                })
            })
            .await;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pin2_and_puk2_allow_reads_only_in_operational_modem_state() {
        for lock in [1, 3, 5] {
            assert!(identity_readable(lock, 8));
            assert!(identity_readable(lock, 3));
            assert!(!identity_readable(lock, 2));
            assert!(!identity_readable(lock, -1));
        }
        for lock in [0, 2, 4, 6, 16, 99] {
            assert!(!identity_readable(lock, 8));
        }
    }
    #[test]
    fn readiness_needs_monotonic_stability_and_resets_on_identity_or_backend_loss() {
        let mut s = Stable::default();
        let t = Instant::now();
        assert!(!s.observe("a", Some([1; 32]), t));
        assert!(!s.observe("a", Some([1; 32]), t + Duration::from_secs(1)));
        assert!(s.observe("a", Some([1; 32]), t + Duration::from_secs(2)));
        assert!(!s.observe("b", Some([1; 32]), t + Duration::from_secs(3)));
        assert!(!s.observe("b", Some([2; 32]), t + Duration::from_secs(5)));
        assert!(!s.observe("b", None, t + Duration::from_secs(8)));
        assert!(!s.observe("b", Some([2; 32]), t + Duration::from_secs(9)));
    }
}
