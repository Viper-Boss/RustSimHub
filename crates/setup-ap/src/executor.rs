// SPDX-License-Identifier: AGPL-3.0-only
//! Root consumer for the protected Core lease. Explicit CLI only; no service auto-install.
use crate::{
    channel::{
        self,
        files::{Directory, Journal, Slot},
        Action, Availability, Binding, Hint, RootLease, State, Status,
    },
    transaction::{self, Gate, Guard, Network, Outcome, Phase},
    Credentials, Error, Result,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{watch, Mutex},
    time::{timeout, Instant},
};
mod material;
mod provision;
use material::Material;

struct LeaseGate<'a, R> {
    input: &'a Directory,
    readiness: &'a R,
    lease: Mutex<RootLease>,
    sequence: AtomicU64,
    boot: String,
}
/// No production always-allow implementation: actual TLS/Origin, identity and private material.
trait Ready {
    fn check(&self) -> impl std::future::Future<Output = Result<()>> + Send;
    fn ap_check(&self) -> impl std::future::Future<Output = Result<()>> + Send;
}
impl<R: Ready + Sync> Gate for LeaseGate<'_, R> {
    async fn check(&self) -> Result<()> {
        let bytes = self.input.read(Slot::Hint)?.ok_or(Error::Denied)?;
        let hint = Hint::parse(&bytes, channel::uptime_ms()?, &self.boot)?;
        let seq = hint.sequence;
        if !self
            .lease
            .lock()
            .await
            .check(hint, channel::uptime_ms()?, &self.boot)?
        {
            return Err(Error::Denied);
        }
        self.sequence.store(seq, Ordering::SeqCst);
        timeout(Duration::from_secs(3), self.readiness.check())
            .await
            .map_err(|_| Error::Busy)??;
        // A slow TLS check must not turn an expired or changed hint into a valid gate.
        let bytes = self.input.read(Slot::Hint)?.ok_or(Error::Denied)?;
        let hint = Hint::parse(&bytes, channel::uptime_ms()?, &self.boot)?;
        let seq = hint.sequence;
        if !self
            .lease
            .lock()
            .await
            .check(hint, channel::uptime_ms()?, &self.boot)?
        {
            return Err(Error::Denied);
        }
        self.sequence.store(seq, Ordering::SeqCst);
        Ok(())
    }
}
fn receipt(binding: &Binding, state: State, sequence: u64) -> Result<Status> {
    let v = Status {
        format: 1,
        binding: binding.clone(),
        state,
        sequence,
        sample_uptime_ms: channel::uptime_ms()?,
        rollback_verified: state == State::Closed,
        guard_released: state == State::Closed,
    };
    v.validate(binding, v.sample_uptime_ms)?;
    Ok(v)
}
fn publish(output: &Directory, v: &Status) -> Result<()> {
    v.validate(&v.binding, channel::uptime_ms()?)?;
    output.publish(Slot::Status, v)
}
/// Remains pinned until the transaction finishes cleanup, including on cancellation/output failure.
async fn execute<N: Network, G: Guard, R: Ready + Sync>(
    worker: &Worker,
    network: &mut N,
    guard: &mut G,
    readiness: &R,
    credentials: &Credentials,
    hint: Hint,
    mut stopped: watch::Receiver<bool>,
) -> Result<Status> {
    let time = channel::uptime_ms()?;
    let lease = RootLease::begin(
        hint.clone(),
        time,
        &worker.boot,
        &worker.realm,
        &worker.key,
        &worker.material,
    )?;
    let gate = LeaseGate {
        input: &worker.input,
        readiness,
        lease: Mutex::new(lease),
        sequence: AtomicU64::new(1),
        boot: worker.boot.clone(),
    };
    if *stopped.borrow() || stopped.has_changed().is_err() {
        return Err(Error::Denied);
    }
    worker.journal.consume(&hint.binding)?;
    // Durable intent precedes this acknowledgement and every mutation.
    publish(&worker.output, &receipt(&hint.binding, State::Starting, 1)?)?;
    let (cancel, cancelled) = watch::channel(false);
    let (phases, mut phase) = watch::channel(Phase::Preflight);
    let remaining = hint
        .binding
        .deadline_uptime_ms
        .checked_sub(channel::uptime_ms()?)
        .ok_or(Error::Denied)?;
    let work = transaction::run(
        network,
        guard,
        &gate,
        credentials,
        Duration::from_millis(remaining),
        cancelled,
        phases,
    );
    tokio::pin!(work);
    let mut poll = tokio::time::interval(Duration::from_millis(500));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut shutting_down = false;
    let mut heard_ready = false;
    let mut waiting_ap: Option<Instant> = None;
    let outcome = loop {
        tokio::select! {
            biased;
            result = &mut work => break result,
            _ = stopped.changed(), if !shutting_down => {
                shutting_down = true;
                cancel.send_replace(true);
            }
            _ = poll.tick() => {
                if !shutting_down && (*stopped.borrow() || gate.check().await.is_err()) {
                    shutting_down = true;
                    cancel.send_replace(true);
                }
                let mut state = if shutting_down { State::Closing } else { match *phase.borrow_and_update() {
                    Phase::Preflight | Phase::Starting => State::Starting,
                    Phase::Ready => State::Ready,
                    Phase::Closing => State::Closing,
                    Phase::Closed => State::Closing, // Only the returned verified outcome closes.
                    Phase::Unknown => State::Unknown,
                }};
                if state == State::Ready {
                    let usable = timeout(Duration::from_secs(3), readiness.ap_check()).await.is_ok_and(|v| v.is_ok());
                    if usable {
                        heard_ready = true;
                    } else if heard_ready || waiting_ap.get_or_insert_with(Instant::now).elapsed() >= Duration::from_secs(10) {
                        shutting_down = true; cancel.send_replace(true); state = State::Closing;
                    } else {
                        state = State::Starting; // NM up is insufficient: Edge must accept the AP address.
                    }
                }
                if state == State::Ready && gate.check().await.is_err() {
                    shutting_down = true; cancel.send_replace(true); state = State::Closing;
                }
                let result = receipt(&hint.binding, state, gate.sequence.load(Ordering::SeqCst)).and_then(|v| publish(&worker.output, &v));
                if result.is_err() { shutting_down = true; cancel.send_replace(true); }
            }
        }
    };
    // Err is possible only BEFORE guard.install in transaction::run; no side effect was attempted.
    // Once guard installation is attempted all uncertain failures return Outcome::Unknown.
    let state = if matches!(outcome, Ok(Outcome::Unknown)) {
        State::Unknown
    } else {
        State::Closed
    };
    let done = receipt(&hint.binding, state, gate.sequence.load(Ordering::SeqCst))?;
    if state == State::Closed {
        worker.journal.finish_closed(&done, done.sample_uptime_ms)?;
    }
    publish(&worker.output, &done)?;
    Ok(done)
}

struct Worker {
    input: Directory,
    output: Directory,
    journal: Journal,
    boot: String,
    realm: String,
    key: String,
    material: String,
}
impl Worker {
    /// Recovery can re-emit a durable terminal proof, never Start or guessed rollback.
    fn recover(&self) -> Result<bool> {
        let Some(binding) = self.journal.pending()? else {
            return Ok(false);
        };
        if let Some(mut closed) = self.journal.closed(&binding)? {
            closed.sample_uptime_ms = channel::uptime_ms()?;
            self.journal
                .finish_closed(&closed, closed.sample_uptime_ms)?;
            publish(&self.output, &closed)?;
            return Ok(false);
        }
        // Preserve sequence monotonicity so Core cannot ignore a restart's Unknown as stale.
        let mut sequence = 1;
        if let Some(raw) = self.output.read(Slot::Status)? {
            if let Ok(old) = serde_json::from_slice::<Status>(&raw) {
                if old.validate(&binding, old.sample_uptime_ms).is_ok() {
                    sequence = sequence.max(old.sequence);
                }
            }
        }
        if let Some(raw) = self.input.read(Slot::Hint)? {
            if let Ok(hint) = serde_json::from_slice::<Hint>(&raw) {
                if hint.binding == binding && (1..=1024).contains(&hint.sequence) {
                    sequence = sequence.max(hint.sequence);
                }
            }
        }
        publish(&self.output, &receipt(&binding, State::Unknown, sequence)?)?;
        Ok(true)
    }
    fn completed(&self, raw: &[u8]) -> Result<bool> {
        if raw.len() > channel::MAX_BYTES {
            return Err(Error::Invalid);
        }
        let hint: Hint = serde_json::from_slice(raw).map_err(|_| Error::Invalid)?;
        if let Some(mut done) = self.journal.closed(&hint.binding)? {
            done.sample_uptime_ms = channel::uptime_ms()?;
            publish(&self.output, &done)?;
            return Ok(true);
        }
        Ok(false)
    }
    fn availability(&self, ready: bool) -> Result<()> {
        self.output.publish(
            Slot::Availability,
            &Availability {
                format: 1,
                boot_id: self.boot.clone(),
                realm: self.realm.clone(),
                device_key_sha256: self.key.clone(),
                material_sha256: self.material.clone(),
                sample_uptime_ms: channel::uptime_ms()?,
                tls_origin_ready: ready,
                gate_ready: ready,
            },
        )
    }
}
pub fn cli(args: Vec<String>) -> Result<()> {
    if let [mode, parent, core, edge, edge_uid, core_uid] = args.as_slice() {
        if mode == "--prepare-from-core" {
            if rustix::process::geteuid().as_raw() != 0 {
                return Err(Error::Denied);
            }
            let value = provision::prepare_from_core(
                Path::new(parent),
                Path::new(core),
                Path::new(edge),
                edge_uid.parse().map_err(|_| Error::Invalid)?,
                core_uid.parse().map_err(|_| Error::Invalid)?,
            )?;
            println!("{value}");
            return Ok(());
        }
    }
    if let [mode, material, core, uid, output] = args.as_slice() {
        if mode == "--export-first-setup" {
            if rustix::process::geteuid().as_raw() != 0 {
                return Err(Error::Denied);
            }
            let value = provision::export_first_setup(
                Path::new(material),
                Path::new(core),
                uid.parse().map_err(|_| Error::Invalid)?,
                Path::new(output),
            )?;
            println!("{value}");
            return Ok(());
        }
    }
    if let [mode, parent, binding, edge, uid] = args.as_slice() {
        if mode == "--prepare-material" {
            if rustix::process::geteuid().as_raw() != 0 {
                return Err(Error::Denied);
            }
            let uid = uid.parse().map_err(|_| Error::Invalid)?;
            let value =
                provision::prepare(Path::new(parent), Path::new(binding), Path::new(edge), uid)?;
            println!("{value}");
            return Ok(());
        }
    }
    if let [mode, material, output] = args.as_slice() {
        if mode == "--export-material-trusted" {
            if rustix::process::geteuid().as_raw() != 0 {
                return Err(Error::Denied);
            }
            let value = provision::export(Path::new(material), Path::new(output))?;
            println!("{value}");
            return Ok(());
        }
    }
    let [mode, config, input, output, journal] = args.as_slice() else {
        return Err(Error::Invalid);
    };
    if mode != "--serve" || rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    let material = Material::root(Path::new(config))?;
    let worker = Worker {
        input: Directory::open(Path::new(input), material.core_uid, 0, 0o2750)?,
        output: Directory::open(Path::new(output), 0, material.core_uid, 0o2750)?,
        journal: Journal::root(Path::new(journal))?,
        boot: channel::boot_id()?,
        realm: material.realm.clone(),
        key: material.device_hash(),
        material: material.digest.clone(),
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| Error::Io)?
        .block_on(async {
            let (stop, stopped) = watch::channel(false);
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .map_err(|_| Error::Io)?;
            let mut interrupt =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                    .map_err(|_| Error::Io)?;
            let signals = tokio::spawn(async move {
                tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {} }
                stop.send_replace(true);
            });
            let result = serve(worker, material, stopped).await;
            signals.abort();
            let _ = signals.await;
            result
        })
}
async fn serve(worker: Worker, material: Material, stopped: watch::Receiver<bool>) -> Result<()> {
    let mut next_probe = Instant::now();
    loop {
        if *stopped.borrow() || stopped.has_changed().is_err() {
            worker.availability(false)?;
            return Ok(());
        }
        if worker.recover()? {
            worker.availability(false)?;
        } else {
            if Instant::now() >= next_probe {
                worker.availability(
                    timeout(Duration::from_secs(3), material.check())
                        .await
                        .is_ok_and(|v| v.is_ok()),
                )?;
                next_probe = Instant::now() + Duration::from_secs(3);
            }
            if let Some(bytes) = worker.input.read(Slot::Hint)? {
                if !worker.completed(&bytes)? {
                    if let Ok(hint) = Hint::parse(&bytes, channel::uptime_ms()?, &worker.boot) {
                        if hint.action == Action::Start
                            && RootLease::begin(
                                hint.clone(),
                                channel::uptime_ms()?,
                                &worker.boot,
                                &worker.realm,
                                &worker.key,
                                &worker.material,
                            )
                            .is_ok()
                        {
                            // Consumed requests are denied by the journal before any side effect.
                            worker.availability(false)?;
                            let mut network = timeout(
                                Duration::from_secs(5),
                                crate::nm::NetworkManager::system(),
                            )
                            .await
                            .map_err(|_| Error::Busy)??;
                            let mut guard = crate::guard::NftGuard::new();
                            let _result = execute(
                                &worker,
                                &mut network,
                                &mut guard,
                                &material,
                                &material.credentials,
                                hint,
                                stopped.clone(),
                            )
                            .await;
                            next_probe = Instant::now();
                        }
                    }
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
#[cfg(test)]
mod tests;
