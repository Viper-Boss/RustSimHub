// SPDX-License-Identifier: AGPL-3.0-only
//! Read-only reconciliation. No Send/Create/Delete/Enable/PIN methods exist here.
use rustsimhub_auth::SimFingerprinter;
use rustsimhub_storage::{Handle, IncomingSms, LineObservation, LineStatus};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[cfg(test)]
pub(crate) mod checks;
mod sources;
mod watch;
use sources::Sources;
#[derive(Default, Clone, serde::Serialize)]
pub(crate) struct Stats {
    pub subscriptions: bool,
    pub reconciliations: u64,
    pub signals: u64,
    pub reconnects: u64,
    pub identity_invalidations: u64,
    pub object_read_errors: u64,
    pub storage_errors: u64,
}
pub(crate) type Metrics = Arc<Mutex<Stats>>;
fn metric(m: &Metrics, f: impl FnOnce(&mut Stats)) {
    if let Ok(mut s) = m.lock() {
        f(&mut s);
    }
}
const SERVICE: &str = "org.freedesktop.ModemManager1";
const MODEM: &str = "org.freedesktop.ModemManager1.Modem";
fn identity_readable(lock: u32, state: i32) -> bool {
    // MM treats PIN2/PUK2 as operational; never submit an unlock code for ordinary reads.
    matches!(lock, 1 | 3 | 5) && (3..=11).contains(&state)
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
    state: i32,
}
/// A volatile binding tied to this connection and continuously polled identity fence.
#[derive(Clone)]
pub(crate) struct Binding {
    pub connection: zbus::Connection,
    pub context: rustsimhub_storage::SendContext,
    iccid: String,
    sim: OwnedObjectPath,
    guard: tokio::sync::watch::Receiver<watch::Notice>,
    fence: u64,
    seen: Instant,
}
impl Binding {
    pub fn valid(&self) -> bool {
        self.seen.elapsed() < Duration::from_secs(15) && watch::valid(&self.guard, self.fence)
    }
    pub fn revision(&self) -> u64 {
        self.guard.borrow().revision
    }
    pub fn object_valid(&self, path: &str, revision: u64) -> bool {
        let n = self.guard.borrow();
        self.valid()
            && n.reset_sources <= revision
            && !n.retired.iter().any(|(r, p)| *r > revision && p == path)
    }
    pub fn call_valid(&self, path: &str, revision: u64) -> bool {
        let n = self.guard.borrow();
        self.valid()
            && n.reset_calls <= revision
            && !n
                .retired_calls
                .iter()
                .any(|(r, p)| *r > revision && p == path)
    }
    pub async fn fresh(&self) -> bool {
        if !self.valid() {
            return false;
        }
        let Ok(path) = OwnedObjectPath::try_from(self.context.modem.as_str()) else {
            return false;
        };
        matches!(tokio::time::timeout(Duration::from_secs(5), snapshot(&self.connection, Some(&path))).await,
            Ok(Ok(s)) if self.valid() && s.owner == self.context.owner && s.modem == path && s.sim == self.sim && s.iccid.as_deref() == Some(&self.iccid) && s.status == LineStatus::Observing && (8..=11).contains(&s.state))
    }
}
pub(crate) type Bindings = tokio::sync::watch::Sender<Option<Binding>>;
async fn snapshot(
    c: &zbus::Connection,
    cached_modem: Option<&OwnedObjectPath>,
) -> Result<Snapshot> {
    let bus = zbus::fdo::DBusProxy::new(c).await?;
    let owner = bus.get_name_owner(SERVICE.try_into()?).await?.to_string();
    let (path, p) = if let Some(path) = cached_modem {
        let proxy = zbus::fdo::PropertiesProxy::builder(c)
            .destination(owner.clone())?
            .path(path.clone())?
            .build()
            .await?;
        (path.clone(), proxy.get_all(MODEM.try_into()?).await?)
    } else {
        let objects =
            zbus::fdo::ObjectManagerProxy::new(c, owner.as_str(), "/org/freedesktop/ModemManager1")
                .await?
                .get_managed_objects()
                .await?;
        if objects.len() > sources::MAX_OBJECTS + 128 {
            return Err("object limit".into());
        }
        let mut modems = objects.into_iter().filter(|(_, v)| v.contains_key(MODEM));
        let (path, mut interfaces) = modems.next().ok_or("no modem")?;
        if modems.next().is_some() {
            return Err("ambiguous modem".into());
        }
        (
            path,
            interfaces
                .remove(MODEM)
                .ok_or("modem interface unavailable")?,
        )
    };
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
        let iccid = rustsimhub_domain::canonical_sim_identifier(&simproxy.sim_identifier().await?)
            .ok_or("SIM identity unavailable")?;
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
        state,
    })
}
async fn read_message(
    c: &zbus::Connection,
    s: &Snapshot,
    path: &OwnedObjectPath,
    line: rustsimhub_domain::LineEpoch,
) -> Result<Option<IncomingSms>> {
    let props = zbus::fdo::PropertiesProxy::builder(c)
        .destination(s.owner.clone())?
        .path(path.clone())?
        .build()
        .await?;
    let p: HashMap<String, OwnedValue> = props
        .get_all("org.freedesktop.ModemManager1.Sms".try_into()?)
        .await?;
    decode_message(&p, path.to_string(), line)
}
fn decode_message(
    p: &HashMap<String, OwnedValue>,
    source: String,
    line: rustsimhub_domain::LineEpoch,
) -> Result<Option<IncomingSms>> {
    let state = u32::try_from(p.get("State").ok_or("missing SMS state")?)?;
    let kind = u32::try_from(p.get("PduType").ok_or("missing SMS kind")?)?;
    if state != 3 || !matches!(kind, 1 | 32) {
        return Ok(None);
    }
    let string = |name: &str, max: usize| -> Result<String> {
        let value = <&str>::try_from(p.get(name).ok_or("missing SMS property")?)?;
        if value.len() > max {
            return Err("SMS property limit".into());
        }
        Ok(value.to_owned())
    };
    let data = Vec::<u8>::try_from(p.get("Data").ok_or("missing SMS data")?.try_clone()?)?;
    let text = string("Text", 4096)?;
    if data.len() > 4096 || (!data.is_empty() && !text.is_empty()) {
        return Err("SMS payload limit".into());
    }
    Ok(Some(IncomingSms {
        line,
        source,
        sender: string("Number", 256)?,
        timestamp: string("Timestamp", 128)?,
        text,
        data,
    }))
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
async fn invalidate(store: &Handle, backend: String, metrics: &Metrics) {
    let h = store.clone();
    if !matches!(
        tokio::task::spawn_blocking(move || h.observe_line(LineObservation {
            backend,
            fingerprint: None,
            status: LineStatus::Unavailable
        }))
        .await,
        Ok(Ok(_))
    ) {
        metric(metrics, |s| {
            s.storage_errors = s.storage_errors.saturating_add(1)
        });
    }
}
struct Context<'a> {
    owner: String,
    prefix: String,
    fingerprinter: &'a SimFingerprinter,
    store: &'a Handle,
    metrics: &'a Metrics,
    bindings: Option<&'a Bindings>,
}
async fn reconcile(
    c: &zbus::Connection,
    context: &Context<'_>,
    stable: &mut Stable,
    sources: &mut Sources,
    cached_modem: &mut Option<OwnedObjectPath>,
    guard: tokio::sync::watch::Receiver<watch::Notice>,
    fence: u64,
) -> Result<bool> {
    let owner = context.owner.as_str();
    let backend_prefix = context.prefix.as_str();
    let fingerprinter = context.fingerprinter;
    let store = context.store;
    let metrics = context.metrics;
    let before = snapshot(c, cached_modem.as_ref()).await?;
    if before.owner != owner {
        return Err("backend changed".into());
    }
    *cached_modem = Some(before.modem.clone());
    let backend = format!("{backend_prefix}:{}:{}", before.modem, before.sim);
    let key = before
        .iccid
        .as_deref()
        .map(|id| fingerprinter.fingerprint(id))
        .transpose()?;
    let previous = stable.candidate.as_ref().map(|(b, k, _)| (b.clone(), *k));
    let ready = stable.observe(&backend, key, Instant::now());
    if previous != stable.candidate.as_ref().map(|(b, k, _)| (b.clone(), *k)) {
        sources.reset();
    }
    let status = if ready {
        LineStatus::Ready
    } else {
        before.status
    };
    let h = store.clone();
    let observe_guard = guard.clone();
    let send_backend = backend.clone();
    let view = tokio::task::spawn_blocking(move || {
        h.observe_line_if(
            LineObservation {
                backend,
                fingerprint: key,
                status,
            },
            move || watch::valid(&observe_guard, fence),
        )
    })
    .await??;
    if !ready {
        if let Some(b) = context.bindings {
            b.send_replace(None);
        }
        return Ok(false);
    }
    let proxy = MessagingProxy::builder(c)
        .destination(owner.to_owned())?
        .path(before.modem.clone())?
        .build()
        .await?;
    // Initial and periodic full enumeration repair missed events. Content is read in fair bounded batches.
    if sources.list_due() {
        let paths = proxy.list().await?;
        if sources.needs_restore() {
            let h = store.clone();
            let line = view.line;
            let known = paths.iter().map(|p| p.to_string()).collect();
            let snapshot =
                tokio::task::spawn_blocking(move || h.saved_sources(line, known)).await??;
            sources.restore(snapshot)?;
        }
        sources.refresh(paths)?;
    }
    let (paths, more) = sources.next_batch();
    let mut batch = Vec::new();
    for path in paths {
        match tokio::time::timeout(
            Duration::from_secs(1),
            read_message(c, &before, &path, view.line),
        )
        .await
        {
            Ok(Ok(Some(mut sms))) => {
                sources.bind(&mut sms)?;
                batch.push(sms);
            }
            Ok(Ok(None)) => sources.incomplete(path.as_str()),
            _ => metric(metrics, |s| {
                s.object_read_errors = s.object_read_errors.saturating_add(1)
            }),
        }
    }
    let after = snapshot(c, Some(&before.modem)).await?;
    if before.owner != after.owner
        || before.modem != after.modem
        || before.sim != after.sim
        || before.iccid != after.iccid
        || after.status != LineStatus::Observing
    {
        return Err("SIM changed during read".into());
    }
    let h = store.clone();
    if let (Some(publisher), Some(fingerprint), Some(iccid)) = (context.bindings, key, after.iccid)
    {
        publisher.send_replace(Some(Binding {
            connection: c.clone(),
            context: rustsimhub_storage::SendContext {
                line: view.line,
                backend: send_backend,
                owner: after.owner,
                modem: after.modem.to_string(),
                fingerprint,
            },
            iccid,
            sim: after.sim,
            guard: guard.clone(),
            fence,
            seen: Instant::now(),
        }));
    }
    let failures = tokio::task::spawn_blocking(move || {
        let mut failures = 0u64;
        for sms in batch {
            if !watch::valid(&guard, fence) {
                break;
            }
            let receive_guard = guard.clone();
            if h.receive_sms_if(sms, move || watch::valid(&receive_guard, fence))
                .is_err()
            {
                failures += 1;
            }
        }
        failures
    })
    .await?;
    metric(metrics, |s| {
        s.storage_errors = s.storage_errors.saturating_add(failures);
        s.reconciliations = s.reconciliations.saturating_add(1);
    });
    Ok(more)
}
async fn session(
    c: zbus::Connection,
    store: &Handle,
    _instance: &str,
    _connection_generation: u64,
    fingerprinter: &SimFingerprinter,
    metrics: &Metrics,
    bindings: Option<&Bindings>,
) -> Result<()> {
    let mut watcher = watch::Watcher::connect(&c).await?;
    metric(metrics, |s| s.subscriptions = true);
    // Persistent line identity is the broker UUID and MM owner, not the Core login instance.
    // Live guard/fence, authenticated instance and lease boot remain separate runtime barriers.
    let bus = zbus::fdo::DBusProxy::new(&c).await?;
    let bus_id = bus.get_id().await?;
    if bus_id.len() != 32 || !bus_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid broker identity".into());
    }
    let prefix = format!("mm:{bus_id}:{}", watcher.owner);
    let context = Context {
        owner: watcher.owner.clone(),
        prefix: prefix.clone(),
        fingerprinter,
        store,
        metrics,
        bindings,
    };
    let mut stable = Stable::default();
    let mut sources = Sources::default();
    let mut cached_modem = None;
    let mut consumed = 0;
    let mut source_reset = 0;
    let mut fence = 0;
    loop {
        let notice = watcher.receiver.borrow_and_update().clone();
        if notice.ended {
            return Err("watcher ended".into());
        }
        metric(metrics, |s| {
            s.signals = s
                .signals
                .saturating_add(notice.revision.saturating_sub(consumed))
        });
        if notice.reset_sources != source_reset {
            sources.reset_uncertain();
            source_reset = notice.reset_sources;
        }
        for (revision, path) in &notice.retired {
            if *revision > consumed {
                sources.retire(path);
            }
        }
        consumed = notice.revision;
        if notice.fence != fence {
            if let Some(b) = bindings {
                b.send_replace(None);
            }
            invalidate(store, format!("{prefix}:invalid:{}", notice.fence), metrics).await;
            stable.candidate = None;
            sources.reset();
            fence = notice.fence;
            cached_modem = None;
            metric(metrics, |s| {
                s.identity_invalidations = s.identity_invalidations.saturating_add(1)
            });
        }
        let guard = watcher.receiver.clone();
        let cycle = reconcile(
            &c,
            &context,
            &mut stable,
            &mut sources,
            &mut cached_modem,
            guard,
            fence,
        );
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            watch::checked(cycle, &mut watcher.receiver, fence),
        )
        .await;
        let more = match result {
            Ok(Ok(more)) => more,
            _ => {
                if watcher.receiver.borrow().fence != fence && !watcher.receiver.borrow().ended {
                    continue;
                }
                return Err("reconciliation unavailable".into());
            }
        };
        // Burst coalescing limits CPU; the signal task remains continuously polled during every wait/read.
        tokio::time::sleep(Duration::from_millis(200)).await;
        if watcher.receiver.borrow().revision != consumed {
            continue;
        }
        let observing = stable
            .candidate
            .as_ref()
            .is_some_and(|(_, _, since)| since.elapsed() < Duration::from_secs(2));
        let wait = if more {
            Duration::from_millis(1)
        } else if observing {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(10)
        };
        tokio::select! {_=tokio::time::sleep(wait)=>{},changed=watcher.receiver.changed()=>{changed.map_err(|_|"watcher ended")?;}}
    }
}
pub async fn run(
    store: Handle,
    instance: String,
    fingerprinter: SimFingerprinter,
    metrics: Metrics,
    bindings: Option<Bindings>,
) {
    let mut generation = 0u64;
    let mut backoff = 1u64;
    loop {
        let Some(next) = generation.checked_add(1) else {
            return;
        };
        generation = next;
        // Builder setup is synchronous; only the actual connection future is timed.
        let conn = match zbus::connection::Builder::system() {
            Ok(b) => {
                tokio::time::timeout(
                    Duration::from_secs(5),
                    b.method_timeout(Duration::from_secs(5)).build(),
                )
                .await
            }
            Err(_) => {
                invalidate(&store, instance.clone(), &metrics).await;
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(30);
                continue;
            }
        };
        if let Ok(Ok(c)) = conn {
            let start = Instant::now();
            let _ = session(
                c,
                &store,
                &instance,
                generation,
                &fingerprinter,
                &metrics,
                bindings.as_ref(),
            )
            .await;
            if start.elapsed() > Duration::from_secs(30) {
                backoff = 1;
            }
        }
        if let Some(b) = &bindings {
            b.send_replace(None);
        }
        metric(&metrics, |s| {
            s.subscriptions = false;
            s.reconnects = s.reconnects.saturating_add(1);
        });
        invalidate(&store, instance.clone(), &metrics).await;
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}
#[cfg(test)]
pub(crate) async fn test_binding(
    connection: zbus::Connection,
    context: rustsimhub_storage::SendContext,
) -> (Binding, impl Drop) {
    let watcher = watch::Watcher::connect(&connection).await.unwrap();
    let s = snapshot(&connection, None).await.unwrap();
    (
        Binding {
            connection,
            context,
            iccid: s.iccid.unwrap(),
            sim: s.sim,
            guard: watcher.receiver.clone(),
            fence: 0,
            seen: Instant::now(),
        },
        watcher,
    )
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
            assert!(!identity_readable(lock, 99));
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
