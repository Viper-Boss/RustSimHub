// SPDX-License-Identifier: AGPL-3.0-only
//! Opt-in, one-connection remote media worker. Core remains the sole authority;
//! no call control, identity store, URL, LAN fallback or production activation.
mod audio;
mod config;
mod server;
use crate::remote::Session;
use rustsimhub_media::{
    local::{self, Client},
    remote_worker::{self as wire, Command, Release, Snapshot},
    worker::{Counters, Failure, Phase},
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Failure>;
const CORE_UID: u32 = 986;
const EDGE_UID: u32 = 985;
const MEDIA_UID: u32 = 984;
struct Host {
    core: PathBuf,
    core_uid: u32,
    registry: config::Registry,
    factory: audio::Factory,
}
struct Owner {
    id: String,
    rtc: Option<Session>,
    core: Option<Client>,
    audio: Option<audio::Audio>,
    phase: Phase,
    failure: Option<Failure>,
    release: Release,
    since: Instant,
    next_pulse: Instant,
}
impl Owner {
    fn new() -> Self {
        Self {
            id: String::new(),
            rtc: None,
            core: None,
            audio: None,
            phase: Phase::Handshaking,
            failure: None,
            release: Release::NotRequested,
            since: Instant::now(),
            next_pulse: Instant::now(),
        }
    }
    fn revoke(&mut self, failure: Option<Failure>) {
        // Synchronous source loss barrier, before waiting for PCM or any network work.
        self.core = None;
        if let Some(audio) = &self.audio {
            audio.stop();
        }
        self.rtc = None; // Session::drop fences actual TURN socket and clears codec.
        self.release = Release::Unknown;
        self.failure = failure;
        self.phase = Phase::Closing;
    }
    fn snapshot(&self, answer: Option<String>) -> Result<Snapshot> {
        let mut counters = Counters::default();
        let mut opened = false;
        if let Some(audio) = &self.audio {
            let s = audio.status.lock().map_err(|_| Failure::Unavailable)?;
            opened = s.opened;
            counters.captured = s.stats.captured_frames;
            counters.played = s.stats.played_frames;
            counters.dropped = s.stats.dropped_frames;
            counters.xruns = s.stats.xruns;
        }
        let peer_key = self
            .rtc
            .as_ref()
            .and_then(|r| r.verified_peer())
            .map(|p| p.fingerprint());
        if let Some(rtc) = &self.rtc {
            let (sent, received, dropped) = rtc.counters();
            counters.sent = sent;
            counters.received = received;
            counters.dropped += dropped;
        }
        Ok(Snapshot {
            session: self.id.clone(),
            phase: if self.phase == Phase::Authorizing && opened {
                Phase::Active
            } else {
                self.phase
            },
            peer_key,
            answer,
            pcm_opened: opened,
            counters,
            failure: self.failure,
            release: self.release,
        })
    }
    async fn command(&mut self, command: Command, host: &Host) -> Result<Snapshot> {
        match command {
            Command::Prepare {
                session,
                relay_handle,
                expected_peer,
                offer,
            } => {
                if !self.id.is_empty() {
                    return Err(Failure::Denied);
                }
                self.id = session;
                let registration = (host.registry)(&relay_handle)?;
                let (rtc, answer) = tokio::time::timeout(
                    Duration::from_millis(wire::PREPARE_MS),
                    Session::answer(registration, &offer, expected_peer),
                )
                .await
                .map_err(|_| Failure::Timeout)?
                .map_err(|_| Failure::Denied)?;
                self.rtc = Some(rtc);
                self.since = Instant::now();
                self.snapshot(Some(answer))
            }
            Command::Inspect { .. } => self.snapshot(None),
            Command::Authorize { credentials, .. } => {
                if self.phase != Phase::Verified || self.core.is_some() || self.audio.is_some() {
                    return Err(Failure::Denied);
                }
                let peer = self
                    .rtc
                    .as_ref()
                    .and_then(|r| r.verified_peer())
                    .ok_or(Failure::Denied)?
                    .fingerprint();
                if peer != credentials.peer_key {
                    return Err(Failure::Denied);
                }
                self.phase = Phase::Authorizing;
                // Original actual Core IPC verifies boot/ticket/call/line/lease/principal.
                let core = Client::redeem(&host.core, host.core_uid, credentials)
                    .await
                    .map_err(|_| Failure::Denied)?;
                let permit = core.permit();
                self.core = Some(core);
                self.rtc
                    .as_mut()
                    .ok_or(Failure::Closed)?
                    .authorize(permit.clone(), peer)
                    .map_err(|_| Failure::Denied)?;
                self.audio = Some(audio::Audio::start(permit, host.factory.clone())?);
                self.next_pulse = Instant::now();
                // Opening runs on its owned PCM thread, never blocks Core observation/EOF.
                self.snapshot(None)
            }
            Command::Stop { .. } => {
                self.revoke(None);
                self.tick().await?;
                self.snapshot(None)
            }
        }
    }
    async fn tick(&mut self) -> Result<()> {
        if self.phase == Phase::Closing {
            if self.audio.as_ref().is_none_or(|a| a.finished()) {
                self.audio = None;
                self.phase = Phase::Closed;
            }
            return Ok(());
        }
        if self.phase == Phase::Closed || self.rtc.is_none() {
            return Ok(());
        }
        if self.phase == Phase::Handshaking && self.since.elapsed() >= Duration::from_secs(15) {
            return Err(Failure::Timeout);
        }
        if self.audio.as_ref().is_some_and(|a| a.finished()) {
            return Err(Failure::Denied);
        }
        // Retain the exact RTC operation across Core pulses. Never cancel a partial
        // TLS write/read and then reuse its transport merely because a timer fired.
        {
            let rtc = self.rtc.as_mut().ok_or(Failure::Closed)?;
            let audio = self.audio.as_mut();
            let core = &mut self.core;
            let next_pulse = &mut self.next_pulse;
            let network = async {
                rtc.service().await.map_err(|_| Failure::Denied)?;
                if let Some(audio) = audio {
                    if audio
                        .status
                        .lock()
                        .map_err(|_| Failure::Unavailable)?
                        .opened
                    {
                        for _ in 0..rustsimhub_media::QUEUE_FRAMES {
                            match audio.capture.try_recv() {
                                Ok(mut frame) => {
                                    let result = rtc.send(&frame).await;
                                    frame.samples.fill(0);
                                    result.map_err(|_| Failure::Denied)?;
                                }
                                Err(_) => break,
                            }
                        }
                        for _ in 0..rustsimhub_media::QUEUE_FRAMES {
                            let Some(mut frame) = rtc.receive().map_err(|_| Failure::Denied)?
                            else {
                                break;
                            };
                            match audio.playback.try_send(frame) {
                                Ok(()) => {}
                                Err(std::sync::mpsc::TrySendError::Full(late)) => {
                                    frame = late;
                                    frame.samples.fill(0);
                                }
                                Err(std::sync::mpsc::TrySendError::Disconnected(mut late)) => {
                                    late.samples.fill(0);
                                    return Err(Failure::Closed);
                                }
                            }
                        }
                    }
                }
                Ok(())
            };
            tokio::pin!(network);
            loop {
                tokio::select! { biased;
                    _ = tokio::time::sleep_until((*next_pulse).into()), if core.is_some() => {
                        core.as_mut().ok_or(Failure::Closed)?.pulse().await.map_err(|_| Failure::Denied)?;
                        *next_pulse = Instant::now() + Duration::from_millis(250);
                    }
                    result = &mut network => { result?; break; }
                }
            }
            // Release borrows only after the complete current TLS operation.
        }
        if self.phase == Phase::Handshaking
            && self.rtc.as_ref().and_then(|r| r.verified_peer()).is_some()
        {
            self.phase = Phase::Verified;
        }
        if self.phase == Phase::Authorizing
            && self
                .audio
                .as_ref()
                .is_some_and(|a| a.status.lock().is_ok_and(|s| s.opened))
        {
            self.phase = Phase::Active;
        }
        Ok(())
    }
    async fn drain(&mut self) {
        self.revoke(self.failure);
        while self.audio.as_ref().is_some_and(|a| !a.finished()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.audio = None;
        self.phase = Phase::Closed;
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.revoke(self.failure); /* Audio::drop retains and joins any late owner. */
    }
}
/// `bytes` must come from the existing private native config reader. The CLI is
/// explicit and feature-gated; installed service/Root material is not activated.
pub fn run(bytes: &[u8]) -> std::result::Result<(), &'static str> {
    if rustix::process::getuid().as_raw() != MEDIA_UID
        || rustix::process::geteuid().as_raw() != MEDIA_UID
        || !crate::service_identity::service_uid("rustsimhub-media", MEDIA_UID)
        || !crate::service_identity::service_uid("rustsimhub-edge", EDGE_UID)
        || !crate::service_identity::core_uid(CORE_UID)
    {
        return Err("fixed remote media service identities required");
    }
    let registry = config::installed(bytes).map_err(|_| "invalid installed remote registration")?;
    let host = Host {
        core: "/run/rustsimhub/core/media.sock".into(),
        core_uid: CORE_UID,
        registry,
        factory: Arc::new(|permit| {
            crate::UfiDuplex::open(permit, local::now_ms)
                .map(|p| Box::new(p) as Box<dyn rustsimhub_media::Pcm + Send>)
                .map_err(|_| Failure::Unavailable)
        }),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "remote runtime unavailable")?;
    runtime.block_on(async {
        let (listener, _guard) = crate::remote_control_socket::bind(std::path::Path::new(
            "/run/rustsimhub/media/remote.sock",
        ))?;
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .map_err(|_| "signal unavailable")?;
        let mut interrupt =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|_| "signal unavailable")?;
        server::serve(listener, host, EDGE_UID, async {
            tokio::select! { _ = term.recv() => {}, _ = interrupt.recv() => {} }
        })
        .await
        .map_err(|_| "remote worker unavailable")
    })
}

// Test host injection exists only in test builds. Production has one installed
// registry, fixed service identities/paths and the concrete UFI PCM factory.
#[cfg(test)]
pub async fn serve_fixture(
    listener: tokio::net::UnixListener,
    core: PathBuf,
    core_uid: u32,
    edge_uid: u32,
    registry: impl Fn(&str) -> Result<crate::remote::Registration> + Send + Sync + 'static,
    factory: impl Fn(&rustsimhub_media::Permit) -> Result<Box<dyn rustsimhub_media::Pcm + Send>>
        + Send
        + Sync
        + 'static,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<()> {
    server::serve(
        listener,
        Host {
            core,
            core_uid,
            registry: Arc::new(registry),
            factory: Arc::new(factory),
        },
        edge_uid,
        shutdown,
    )
    .await
}
