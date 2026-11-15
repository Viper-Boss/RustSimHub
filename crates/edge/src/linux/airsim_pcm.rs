// SPDX-License-Identifier: AGPL-3.0-only
//! Private-network AirSIM PCM relay. Core proves the app and owns the lease;
//! the media worker redeems the single-use ticket before opening ALSA.
use super::*;
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixStream,
};

const FRAME: usize = 640; // 16 kHz, signed little-endian mono, 20 ms.
const IO_WAIT: Duration = Duration::from_millis(40);
const CORE_WAIT: Duration = Duration::from_millis(700);
const FRAME_AGE: Duration = Duration::from_millis(60);
// The phone batches microphone frames before TCP delivery. This is a bounded
// input jitter buffer, separate from the local worker/downlink's 60ms limit.
// It also covers TCP fragments, measured from their original first byte.
// An 88 ms split batch, 60 ms headroom and a 35 ms reactor stall must fit
// without retiming the first byte. The native playout age remains 60 ms.
const UPLINK_JITTER_AGE: Duration = Duration::from_millis(200);
const UPLINK_QUEUE_FRAMES: usize = 10;
const UPLINK_BURST_FRAMES: usize = 3;
const UPLINK_INITIAL_CREDITS: usize = 3;
const UPLINK_REFILL_CREDITS: usize = 3;
const UPLINK_PREFILL_WAIT: Duration = Duration::from_millis(60);
const UPLINK_MIN_SPACING: Duration = Duration::from_millis(10);
const UPLINK_FIRST_REFILL: Duration = Duration::from_millis(30);
const UPLINK_POLL_PERIOD: Duration = Duration::from_millis(5);
// AirSIM's public wire marker, identical to rustsimhub_airsim::PCM_READY.
const READY: &[u8] = b"AIRSIMREADY";

/// Fixed labels and elapsed time only: never peer addresses, prefaces,
/// credentials, session identifiers or PCM. One bounded record per link.
struct Trace {
    phase: &'static str,
    started: Instant,
}
impl Drop for Trace {
    fn drop(&mut self) {
        eprintln!(
            "airsim_pcm_closed phase={} elapsed_ms={}",
            self.phase,
            self.started.elapsed().as_millis()
        );
    }
}

async fn core(app: &App, command: AuthCommand) -> Option<Value> {
    let operation = match &command {
        AuthCommand::AirsimPcmOpen { .. } => "open",
        AuthCommand::AirsimPcmPulse { close: true, .. } => "close",
        AuthCommand::AirsimPcmPulse { .. } => "pulse",
        _ => "invalid",
    };
    let started = Instant::now();
    match timeout(CORE_WAIT, ask(app, command)).await {
        Ok(Some(value)) => Some(value),
        result => {
            let outcome = if result.is_err() {
                "timeout"
            } else {
                "unavailable_or_refused"
            };
            eprintln!(
                "airsim_pcm_core operation={operation} outcome={outcome} elapsed_ms={}",
                started.elapsed().as_millis()
            );
            None
        }
    }
}

/// Read exactly one bounded preface, leaving any coalesced PCM bytes on TCP.
async fn preface<R: AsyncRead + Unpin>(reader: &mut R) -> Option<String> {
    let mut bytes = Vec::new();
    while bytes.len() < 1024 {
        let b = reader.read_u8().await.ok()?;
        bytes.push(b);
        if b == b'\n' {
            return String::from_utf8(bytes).ok();
        }
        if !b.is_ascii_graphic() && b != b' ' {
            return None;
        }
    }
    None
}

/// Pair averaging for 16→8 kHz; bounded linear interpolation for 8→16 kHz.
/// 16 kHz is byte-exact passthrough. No allocation depends on remote lengths.
fn convert(input: &[u8], from: u32, to: u32) -> Option<Vec<u8>> {
    if !matches!(from, 8_000 | 16_000)
        || !matches!(to, 8_000 | 16_000)
        || input.len() != (from as usize / 50) * 2
    {
        return None;
    }
    if from == to {
        return Some(input.to_vec());
    }
    let samples: Vec<i16> = input
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    let mut out = Vec::with_capacity((to as usize / 50) * 2);
    if from == 16_000 {
        for pair in samples.as_chunks::<2>().0 {
            out.extend_from_slice(&((pair[0] as i32 + pair[1] as i32) / 2).to_le_bytes()[..2]);
        }
    } else {
        for (i, sample) in samples.iter().enumerate() {
            out.extend_from_slice(&sample.to_le_bytes());
            let next = samples.get(i + 1).unwrap_or(sample);
            out.extend_from_slice(&((*sample as i32 + *next as i32) / 2).to_le_bytes()[..2]);
        }
    }
    Some(out)
}

struct Queue {
    partial: Vec<u8>,
    partial_at: Option<Instant>,
    frames: VecDeque<(Instant, Vec<u8>)>,
    size: usize,
    dropped: u64,
    partial_expired: u64,
    overflow: u64,
    queued_expired: u64,
}
impl Queue {
    fn new(rate: u32) -> Self {
        Self {
            partial: Vec::new(),
            partial_at: None,
            frames: VecDeque::with_capacity(UPLINK_QUEUE_FRAMES),
            size: rate as usize / 50 * 2,
            dropped: 0,
            partial_expired: 0,
            overflow: 0,
            queued_expired: 0,
        }
    }
    fn push(&mut self, bytes: &[u8], now: Instant) -> bool {
        let mut bytes = bytes;
        if let Some(at) = self.partial_at {
            let Some(age) = now.checked_duration_since(at) else {
                return false;
            };
            let take = bytes.len().min(self.size - self.partial.len());
            self.partial.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.partial.len() != self.size {
                return true;
            }
            // TCP may split one raw frame across microphone batches. Finish
            // its byte boundary even when stale; discard that whole frame,
            // never the remaining stream or the independent downlink.
            if age < UPLINK_JITTER_AGE {
                if self.frames.len() == UPLINK_QUEUE_FRAMES {
                    self.frames.pop_front();
                    self.dropped = self.dropped.saturating_add(1);
                    self.overflow = self.overflow.saturating_add(1);
                }
                self.frames.push_back((at, self.partial.clone()));
            } else {
                self.dropped = self.dropped.saturating_add(1);
                self.partial_expired = self.partial_expired.saturating_add(1);
            }
            self.partial.clear();
            self.partial_at = None;
        }
        for chunk in bytes.chunks_exact(self.size) {
            if self.frames.len() == UPLINK_QUEUE_FRAMES {
                self.frames.pop_front();
                self.dropped = self.dropped.saturating_add(1);
                self.overflow = self.overflow.saturating_add(1);
            }
            self.frames.push_back((now, chunk.to_vec()));
        }
        let remainder = bytes.chunks_exact(self.size).remainder();
        if !remainder.is_empty() {
            self.partial.extend_from_slice(remainder);
            self.partial_at = Some(now);
        }
        true
    }
    fn next_fresh(&mut self, now: Instant) -> Option<Vec<u8>> {
        // TCP can coalesce two legitimate consecutive frames. Pace both in
        // order instead of deleting one at each tick. The ten-frame phone
        // jitter budget is finite; first-byte timestamps are never rejuvenated.
        while let Some((at, bytes)) = self.frames.pop_front() {
            if now
                .checked_duration_since(at)
                .is_some_and(|age| age < UPLINK_JITTER_AGE)
            {
                return Some(bytes);
            }
            self.dropped = self.dropped.saturating_add(1);
            self.queued_expired = self.queued_expired.saturating_add(1);
        }
        None
    }
}

#[derive(Default)]
struct UplinkTrace {
    input_bytes: u64,
    input_reads: u64,
    max_batch_bytes: u64,
    selected: u64,
    muted: u64,
    sent: u64,
    dropped: u64,
    partial_expired: u64,
    overflow: u64,
    queued_expired: u64,
    input_peak: u32,
    output_peak: u32,
}
fn pcm_peak(bytes: &[u8]) -> u32 {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| i16::from_le_bytes(*b).unsigned_abs() as u32)
        .max()
        .unwrap_or(0)
}
impl Drop for UplinkTrace {
    fn drop(&mut self) {
        // Aggregate levels only; never samples, speech, peers or credentials.
        eprintln!("airsim_pcm_uplink input_bytes={} selected={} muted={} sent={} input_peak={} output_peak={} input_reads={} max_batch_bytes={} dropped={} partial_expired={} overflow={} queued_expired={}",
            self.input_bytes, self.selected, self.muted, self.sent, self.input_peak, self.output_peak,
            self.input_reads, self.max_batch_bytes, self.dropped, self.partial_expired, self.overflow, self.queued_expired);
    }
}
impl UplinkTrace {
    fn observe_queue(&mut self, queue: &Queue) {
        self.dropped = queue.dropped;
        self.partial_expired = queue.partial_expired;
        self.overflow = queue.overflow;
        self.queued_expired = queue.queued_expired;
    }
}
/// Elapsed-time credits survive skipped reactor wakeups. Three credits
/// accumulate, with a minimum gap between writes to protect native playout.
/// The initial, bounded prefill gives the independent native playout clock
/// a small cushion; it never extends an input frame's first-byte deadline.
struct UplinkPacer {
    credits: usize,
    next_refill: Option<Instant>,
    last_sent: Option<Instant>,
}
impl UplinkPacer {
    fn new() -> Self {
        Self {
            credits: UPLINK_INITIAL_CREDITS,
            next_refill: None,
            last_sent: None,
        }
    }
    fn ready(&self, queue: &Queue, now: Instant) -> bool {
        self.next_refill.is_some()
            || queue.frames.front().is_some_and(|(at, _)| {
                now.checked_duration_since(*at)
                    .is_some_and(|age| age >= UPLINK_PREFILL_WAIT)
            })
    }
    fn available(&mut self, now: Instant) -> bool {
        let period = Duration::from_millis(20);
        if let Some(next) = self.next_refill {
            if let Some(elapsed) = now.checked_duration_since(next) {
                let ticks = elapsed.as_nanos() / period.as_nanos() + 1;
                self.credits = (self.credits + ticks.min(UPLINK_REFILL_CREDITS as u128) as usize)
                    .min(UPLINK_REFILL_CREDITS);
                let remainder =
                    Duration::from_nanos((elapsed.as_nanos() % period.as_nanos()) as u64);
                self.next_refill = Some(now + period - remainder);
            }
        }
        self.credits > 0
            && self.last_sent.is_none_or(|at| {
                now.checked_duration_since(at)
                    .is_some_and(|age| age >= UPLINK_MIN_SPACING)
            })
    }
    fn sent(&mut self, now: Instant) {
        self.credits -= 1;
        self.last_sent = Some(now);
        self.next_refill
            .get_or_insert(now + UPLINK_FIRST_REFILL);
    }
}
async fn uplink<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut phone: R,
    mut media: W,
    rate: u32,
    muted: &AtomicBool,
) -> std::io::Result<()> {
    let mut bytes = [0; 4 * FRAME];
    let mut queue = Queue::new(rate);
    let mut trace = UplinkTrace::default();
    let mut pacer = UplinkPacer::new();
    let mut tick = tokio::time::interval(UPLINK_POLL_PERIOD);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            n=phone.read(&mut bytes) => {
                let n=n?; if n==0 {return Ok(());}
                trace.input_bytes = trace.input_bytes.saturating_add(n as u64);
                trace.input_reads = trace.input_reads.saturating_add(1);
                trace.max_batch_bytes = trace.max_batch_bytes.max(n as u64);
                if !queue.push(&bytes[..n],Instant::now()) {return Err(std::io::ErrorKind::InvalidData.into());}
                trace.observe_queue(&queue);
            }
            _=tick.tick() => {}
        }
        if !pacer.ready(&queue, Instant::now()) {
            continue;
        }
        for _ in 0..UPLINK_BURST_FRAMES {
            let now = Instant::now();
            if !pacer.available(now) {
                break;
            }
            let ready = queue.next_fresh(now);
            trace.observe_queue(&queue);
            let Some(bytes) = ready else { break };
            let mut frame = convert(&bytes, rate, 16_000).ok_or(std::io::ErrorKind::InvalidData)?;
            trace.selected = trace.selected.saturating_add(1);
            trace.input_peak = trace.input_peak.max(pcm_peak(&frame));
            if muted.load(Ordering::Acquire) {
                trace.muted = trace.muted.saturating_add(1);
                frame.fill(0);
            }
            trace.output_peak = trace.output_peak.max(pcm_peak(&frame));
            pacer.sent(now);
            timeout(IO_WAIT, media.write_all(&frame))
                .await
                .map_err(|_| std::io::ErrorKind::TimedOut)??;
            trace.sent = trace.sent.saturating_add(1);
        }
    }
}

/// The app opens PCM while an owned call may still be dialling. Send its
/// authenticated ready marker and silence, but never open ALSA before ACTIVE.
async fn activate(
    app: &App,
    phone: &mut TcpStream,
    opened: Value,
    trace: &mut Trace,
) -> Option<Value> {
    let rate = opened["pcm_rate"]
        .as_u64()
        .filter(|r| matches!(r, 8_000 | 16_000))?;
    let size = rate as usize / 50 * 2;
    let ticket = opened["ticket"].as_str()?;
    trace.phase = "phone_ready";
    timeout(IO_WAIT, phone.write_all(READY)).await.ok()?.ok()?;
    trace.phase = "waiting_active";
    let mut poll = tokio::time::interval(Duration::from_millis(250));
    let mut silence = tokio::time::interval(Duration::from_millis(20));
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    silence.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut consumed = 0usize;
    let mut bytes = [0; 4 * FRAME];
    let zeros = [0; FRAME];
    let end = Instant::now() + Duration::from_secs(90);
    let active = loop {
        if Instant::now() >= end {
            return None;
        }
        tokio::select! {
            n=phone.read(&mut bytes)=>{let n=n.ok()?;if n==0{return None;} consumed=(consumed+n)%size;},
            _=silence.tick()=>{timeout(IO_WAIT,phone.write_all(&zeros[..size])).await.ok()?.ok()?;},
            _=poll.tick()=>{
                let value=core(app,AuthCommand::AirsimPcmPulse{ticket:ticket.into(),close:false}).await?;
                if value["active"]==true {if value["credentials"].is_null(){return None;}break value;}
                if value["waiting"]!=true {return None;}
            }
        }
    };
    // Discard pre-call microphone bytes, preserving raw-frame alignment. This
    // drain is bounded; a flooding sender is closed, not allowed to starve I/O.
    trace.phase = "drain_pre_call";
    let mut drained = false;
    for _ in 0..16 {
        match phone.try_read(&mut bytes) {
            Ok(0) => return None,
            Ok(n) => consumed = (consumed + n) % size,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                drained = true;
                break;
            }
            Err(_) => return None,
        }
    }
    if !drained {
        return None;
    }
    let mut result = opened;
    result["credentials"] = active["credentials"].clone();
    // Do not wait for a microphone frame before redeeming the one-use grant
    // and starting downlink. The uplink alone discards the partial remainder.
    result["uplink_skip_bytes"] = serde_json::json!((size - consumed) % size);
    Some(result)
}
async fn downlink<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut media: R,
    phone: W,
    rate: u32,
) -> std::io::Result<()> {
    let (latest, receiver) = tokio::sync::watch::channel(None);
    let drops = DownlinkDrops::default();
    // Drain the local worker while TCP is congested. Only the newest complete
    // frame is retained; no detached task or accumulating audio queue.
    let reader = async move {
        let result: std::io::Result<()> = async {
            loop {
                let mut pcm = [0; FRAME];
                timeout(Duration::from_secs(2), media.read_exact(&mut pcm))
                    .await
                    .map_err(|_| std::io::ErrorKind::TimedOut)??;
                latest.send_replace(Some(DownlinkFrame {
                    at: Instant::now(),
                    pcm,
                }));
            }
        }
        .await;
        drop(latest);
        result
    };
    let writer = write_downlink(phone, receiver, rate, &drops);
    tokio::pin!(reader, writer);
    tokio::select! {
        result = &mut writer => result,
        result = &mut reader => {
            // A complete last frame may precede EOF. Drain that one fresh
            // value before reporting closure; writes remain time-bounded.
            match writer.await {
                Err(error) if error.kind() != std::io::ErrorKind::UnexpectedEof => Err(error),
                _ => result,
            }
        }
    }
}

#[derive(Clone)]
struct DownlinkFrame {
    at: Instant,
    pcm: [u8; FRAME],
}

#[derive(Default)]
struct DownlinkDrops {
    no_progress: std::sync::atomic::AtomicU64,
    stale: std::sync::atomic::AtomicU64,
}
impl Drop for DownlinkDrops {
    fn drop(&mut self) {
        eprintln!(
            "airsim_pcm_downlink_drops no_progress={} stale={}",
            self.no_progress.load(Ordering::Relaxed),
            self.stale.load(Ordering::Relaxed)
        );
    }
}

async fn write_downlink<W: AsyncWrite + Unpin>(
    mut phone: W,
    mut frames: tokio::sync::watch::Receiver<Option<DownlinkFrame>>,
    rate: u32,
    drops: &DownlinkDrops,
) -> std::io::Result<()> {
    loop {
        frames
            .changed()
            .await
            .map_err(|_| std::io::ErrorKind::UnexpectedEof)?;
        let Some(frame) = frames.borrow_and_update().clone() else {
            continue;
        };
        let age = Instant::now()
            .checked_duration_since(frame.at)
            .ok_or(std::io::ErrorKind::InvalidData)?;
        let Some(remaining) = FRAME_AGE.checked_sub(age).filter(|d| !d.is_zero()) else {
            drops.stale.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        let output = convert(&frame.pcm, 16_000, rate).ok_or(std::io::ErrorKind::InvalidData)?;
        let mut written = 0;
        let result = timeout(IO_WAIT.min(remaining), async {
            while written < output.len() {
                let n = phone.write(&output[written..]).await?;
                if n == 0 {
                    return Err(std::io::ErrorKind::WriteZero.into());
                }
                written += n;
            }
            Ok::<_, std::io::Error>(())
        })
        .await;
        match result {
            Ok(result) => result?,
            Err(_) if written == 0 => {
                drops.no_progress.fetch_add(1, Ordering::Relaxed);
            }
            // A partially transmitted raw frame cannot be discarded without
            // corrupting every following frame's sample boundary.
            Err(_) => return Err(std::io::ErrorKind::TimedOut.into()),
        }
    }
}

async fn relay(app: &App, phone: TcpStream, opened: &Value, trace: &mut Trace) -> Option<()> {
    trace.phase = "worker_endpoint";
    let rate = opened["pcm_rate"]
        .as_u64()
        .filter(|r| matches!(r, 8_000 | 16_000))? as u32;
    let skip = opened["uplink_skip_bytes"]
        .as_u64()
        .filter(|n| *n < rate as u64 / 50 * 2)? as usize;
    let session = opened["session"].as_str()?;
    if !rustsimhub_media::worker::session_id(session) {
        return None;
    }
    let credentials: rustsimhub_media::protocol::Credentials =
        serde_json::from_value(opened["credentials"].clone()).ok()?;
    let (path, uid) = app.media.as_ref()?.pcm_endpoint()?;
    super::super::socket_safe(&path, uid).ok()?;
    trace.phase = "worker_connect";
    let mut media = timeout(Duration::from_secs(1), UnixStream::connect(path))
        .await
        .ok()?
        .ok()?;
    let local_socket = socket2::SockRef::from(&media);
    local_socket.set_send_buffer_size(4 * FRAME).ok()?;
    local_socket.set_recv_buffer_size(4 * FRAME).ok()?;
    if media.peer_cred().ok()?.uid() != uid {
        return None;
    }
    let hello = serde_json::to_vec(
        &serde_json::json!({"protocol":1,"session":session,"credentials":credentials}),
    )
    .ok()?;
    if hello.len() > 2048 {
        return None;
    }
    trace.phase = "worker_hello";
    timeout(Duration::from_secs(1), async {
        media.write_u32(hello.len() as u32).await?;
        media.write_all(&hello).await?;
        if media.read_u8().await? != 1 {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        Ok::<_, std::io::Error>(())
    })
    .await
    .ok()?
    .ok()?;
    trace.phase = "streaming";
    let muted = AtomicBool::new(true);
    let ticket = opened["ticket"].as_str()?;
    let heartbeat = async {
        let mut tick = tokio::time::interval(Duration::from_millis(250));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let Some(v) = core(
                app,
                AuthCommand::AirsimPcmPulse {
                    ticket: ticket.into(),
                    close: false,
                },
            )
            .await
            else {
                break;
            };
            if v["active"] != true {
                break;
            }
            muted.store(v["muted"].as_bool().unwrap_or(true), Ordering::Release);
        }
    };
    let (phone_read, phone_write) = phone.into_split();
    let (media_read, media_write) = media.into_split();
    let up = async {
        let mut phone_read = phone_read;
        if skip != 0 {
            // This bounded discard is not played and does not delay downlink.
            // EOF or authority loss still closes both directions as before.
            let mut discarded = [0; FRAME];
            phone_read.read_exact(&mut discarded[..skip]).await?;
        }
        uplink(phone_read, media_write, rate, &muted).await
    };
    // No detached pumps. Completion/cancellation drops both socket halves;
    // media EOF invalidates the worker's permit, without redial or reconnect.
    tokio::select! {
        result=up=>{eprintln!("airsim_pcm_stream_closed leg=uplink error={:?}",result.err().map(|e|e.kind()));},
        result=downlink(media_read,phone_write,rate)=>{eprintln!("airsim_pcm_stream_closed leg=downlink error={:?}",result.err().map(|e|e.kind()));},
        _=heartbeat=>{eprintln!("airsim_pcm_stream_closed leg=authority");},
    }
    Some(())
}

pub(super) async fn handle(app: &App, mut phone: TcpStream, peer: IpAddr) {
    let mut trace = Trace {
        phase: "peer_or_media",
        started: Instant::now(),
    };
    if !rustsimhub_media::worker::local_ip(peer) || app.media.is_none() {
        return;
    }
    trace.phase = "phone_preface";
    let Ok(Some(preface)) = timeout(Duration::from_secs(3), preface(&mut phone)).await else {
        return;
    };
    trace.phase = "core_open";
    let Some(opened) = core(
        app,
        AuthCommand::AirsimPcmOpen {
            preface,
            peer: peer.to_string(),
        },
    )
    .await
    else {
        return;
    };
    let Some(ticket) = opened["ticket"].as_str().map(str::to_owned) else {
        return;
    };
    trace.phase = "phone_buffers";
    let socket = socket2::SockRef::from(&phone);
    let configured = phone.set_nodelay(true).is_ok()
        && socket.set_recv_buffer_size(4096).is_ok()
        && socket.set_send_buffer_size(4096).is_ok();
    if configured {
        if let Some(active) = activate(app, &mut phone, opened, &mut trace).await {
            let _ = relay(app, phone, &active, &mut trace).await;
        }
    }
    let _ = core(
        app,
        AuthCommand::AirsimPcmPulse {
            ticket,
            close: true,
        },
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use tokio::{io::duplex, net::UnixListener};

    #[tokio::test]
    async fn cancelling_a_listener_closes_its_inflight_pcm_clients() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = super::super::tests::app(dir.path().join("absent-core.sock"));
        app.media = Some(Arc::new(super::super::super::media::Bridge::isolated_pcm(
            dir.path().join("worker.sock"),
            std::fs::metadata(dir.path()).unwrap().uid(),
        )));
        let limit = app.limit.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(super::super::accept_loop(app, listener, false, true));
        let mut phone = TcpStream::connect(address).await.unwrap();
        phone.write_all(b"A").await.unwrap();
        timeout(Duration::from_secs(1), async {
            while limit.available_permits() == 8 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        server.abort();
        assert!(server.await.unwrap_err().is_cancelled());
        match timeout(Duration::from_secs(1), phone.read(&mut [0; 1]))
            .await
            .unwrap()
        {
            Ok(0) => {}
            // Closing a socket with the unread preface may produce RST
            // rather than FIN. Both prove closure, never a live client.
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
            other => panic!("retired listener left a client open: {other:?}"),
        }
        assert_eq!(
            limit.available_permits(),
            8,
            "the retired listener leaves no detached clients"
        );
    }

    #[test]
    fn sample_conversion_preserves_signed_extremes_lengths_and_passthrough() {
        let narrow: Vec<u8> = [i16::MIN, i16::MAX]
            .into_iter()
            .cycle()
            .take(160)
            .flat_map(i16::to_le_bytes)
            .collect();
        let wide = convert(&narrow, 8_000, 16_000).unwrap();
        assert_eq!(wide.len(), 640);
        assert_eq!(&wide[..2], &i16::MIN.to_le_bytes());
        assert_eq!(&wide[2..4], &0i16.to_le_bytes());
        assert_eq!(convert(&wide, 16_000, 16_000).unwrap(), wide);
        let constant: Vec<u8> = std::iter::repeat_n(-1234i16, 320)
            .flat_map(i16::to_le_bytes)
            .collect();
        let down = convert(&constant, 16_000, 8_000).unwrap();
        assert_eq!(down.len(), 320);
        assert!(down
            .as_chunks::<2>()
            .0
            .iter()
            .all(|b| *b == (-1234i16).to_le_bytes()));
        assert!(convert(&[0; 319], 8_000, 16_000).is_none());
        assert!(convert(&[0; 640], 48_000, 8_000).is_none());
    }
    #[test]
    fn bursts_partials_and_expiry_have_bounded_storage_and_no_old_replay() {
        let mut q = Queue::new(8_000);
        let now = Instant::now();
        assert!(q.push(&[1], now));
        assert!(q.next_fresh(now).is_none());
        assert!(q.push(&[2; 319], now + Duration::from_millis(1)));
        assert_eq!(q.next_fresh(now + Duration::from_millis(59)).unwrap()[0], 1);
        for n in 0..12 {
            assert!(q.push(&[n; 320], now));
            assert!(q.frames.len() <= UPLINK_QUEUE_FRAMES);
        }
        for n in (12 - UPLINK_QUEUE_FRAMES as u8)..12 {
            assert_eq!(q.next_fresh(now).unwrap(), vec![n; 320]);
        }
        assert!(q.frames.is_empty());
        assert!(q.push(&[3; 320], now));
        assert!(q.next_fresh(now + UPLINK_JITTER_AGE).is_none());
        assert!(q.push(&[1], now));
        assert!(q.push(&[2; 319], now + UPLINK_JITTER_AGE));
        assert!(q.next_fresh(now + UPLINK_JITTER_AGE).is_none());
    }
    #[test]
    fn expired_partial_is_discarded_at_its_boundary_without_replaying_or_retiming_it() {
        let mut q = Queue::new(8_000);
        let at = Instant::now();
        assert!(q.push(&[1], at));
        assert!(q.push(&[2; 318], at + FRAME_AGE));
        assert_eq!(q.partial.len(), 319);
        assert!(q.next_fresh(at + FRAME_AGE).is_none());
        let fresh = at + UPLINK_JITTER_AGE + Duration::from_millis(20);
        let mut bytes = vec![3]; // Complete and discard the stale frame.
        bytes.extend_from_slice(&[4; 320]);
        bytes.push(5); // Next partial retains this first-byte timestamp.
        assert!(q.push(&bytes, fresh));
        assert_eq!(q.next_fresh(fresh).unwrap(), vec![4; 320]);
        assert_eq!(q.partial, vec![5]);
        assert_eq!(q.partial_at, Some(fresh));
        assert!(q.push(&[6; 319], fresh + Duration::from_millis(59)));
        assert!(
            q.next_fresh(fresh + UPLINK_JITTER_AGE).is_none(),
            "completion cannot rejuvenate old bytes"
        );
        assert!(q.push(&[7], fresh + Duration::from_secs(1)));
        assert!(
            !q.push(&[8], fresh),
            "future partial timestamps remain invalid"
        );
    }

    #[test]
    fn phone_fragments_share_one_finite_first_byte_jitter_deadline() {
        let mut q = Queue::new(8_000);
        let at = Instant::now();
        assert!(q.push(&[1], at));
        assert!(q.push(&[2; 319], at + UPLINK_JITTER_AGE));
        assert!(q.next_fresh(at + UPLINK_JITTER_AGE).is_none());
        assert_eq!(
            q.dropped, 1,
            "incomplete phone frames expire at the same first-byte deadline"
        );
        assert_eq!(q.partial_expired, 1);
        let fresh = at + UPLINK_JITTER_AGE + Duration::from_millis(20);
        assert!(q.push(&[3; 320], fresh));
        assert_eq!(
            q.next_fresh(fresh + UPLINK_JITTER_AGE - Duration::from_millis(1))
                .unwrap(),
            vec![3; 320]
        );
        assert!(q.push(&[4; 320], fresh));
        assert!(q.next_fresh(fresh + UPLINK_JITTER_AGE).is_none());
        assert_eq!(
            q.dropped, 2,
            "whole microphone frames expire at the new jitter boundary"
        );
        assert_eq!(q.queued_expired, 1);
    }

    #[test]
    fn split_microphone_batches_preserve_speech_and_original_age_at_both_rates() {
        for rate in [8_000, 16_000] {
            let size = rate as usize / 50 * 2;
            for gap_ms in [60u64, 80, 100, 140, 180] {
                for split in [1, size / 2, size - 1] {
                    let at = Instant::now();
                    let input: Vec<_> = (1..=4).flat_map(|n| vec![n; size]).collect();
                    let mut q = Queue::new(rate);
                    assert!(q.push(&input[..split], at));
                    let completed = at + Duration::from_millis(gap_ms);
                    assert!(q.push(&input[split..], completed));
                    for n in 1..=4 {
                        let tick = completed + Duration::from_millis(20 * (n as u64 - 1));
                        assert_eq!(
                            q.next_fresh(tick),
                            Some(vec![n; size]),
                            "a split microphone batch must not lose its first 20ms of speech"
                        );
                    }
                    assert_eq!(q.dropped, 0);
                    assert!(q.frames.is_empty());
                    // Completion cannot restart the first-byte expiry clock.
                    let mut q = Queue::new(rate);
                    assert!(q.push(&input[..split], at));
                    assert!(q.push(&input[split..size], completed));
                    assert!(q.next_fresh(at + UPLINK_JITTER_AGE).is_none());
                    assert_eq!(q.dropped, 1);
                }
            }
        }
    }

    #[test]
    fn uplink_drop_reasons_account_for_expiry_and_bounded_overflow_separately() {
        let at = Instant::now();
        let mut q = Queue::new(8_000);
        assert!(q.push(&[0; (UPLINK_QUEUE_FRAMES + 1) * 320], at));
        assert_eq!(q.frames.len(), UPLINK_QUEUE_FRAMES);
        assert_eq!(q.overflow, 1);
        assert!(q.next_fresh(at + UPLINK_JITTER_AGE).is_none());
        assert_eq!(q.queued_expired, UPLINK_QUEUE_FRAMES as u64);
        assert!(q.push(&[1], at));
        assert!(q.push(&[2; 319], at + UPLINK_JITTER_AGE));
        assert_eq!(q.partial_expired, 1);
        assert_eq!(q.dropped, q.partial_expired + q.overflow + q.queued_expired);
        let mut trace = UplinkTrace::default();
        trace.observe_queue(&q);
        assert_eq!(trace.dropped, UPLINK_QUEUE_FRAMES as u64 + 2);
        assert_eq!(trace.partial_expired, 1);
        assert_eq!(trace.overflow, 1);
        assert_eq!(trace.queued_expired, UPLINK_QUEUE_FRAMES as u64);
    }

    #[test]
    fn pacing_credits_follow_elapsed_time_without_unbounded_catchup() {
        let at = Instant::now();
        let mut p = UplinkPacer::new();
        assert!(p.available(at));
        p.sent(at);
        assert!(!p.available(at));
        assert!(!p.available(at + Duration::from_millis(9)));
        assert!(p.available(at + Duration::from_millis(10)));
        p.sent(at + Duration::from_millis(10));
        assert!(p.available(at + Duration::from_millis(20)));
        p.sent(at + Duration::from_millis(20));
        let delayed = at + Duration::from_millis(75);
        assert!(p.available(delayed));
        assert_eq!(p.credits, 3, "credits at 30 ms, 50 ms and 70 ms elapsed");
        p.sent(delayed);
        assert!(!p.available(delayed));
        assert_eq!(p.next_refill, Some(at + Duration::from_millis(90)));
        let much_later = at + Duration::from_secs(600);
        assert!(p.available(much_later));
        assert_eq!(p.credits, UPLINK_REFILL_CREDITS);
        p.sent(much_later);
        assert!(
            !p.available(much_later),
            "a long stall never permits an unspaced replay burst"
        );
        assert!(!p.available(much_later + Duration::from_millis(9)));
        assert!(p.available(much_later + UPLINK_MIN_SPACING));
    }

    #[test]
    fn prefill_is_bounded_by_frames_or_time_without_retiming_speech() {
        let at = Instant::now();
        let p = UplinkPacer::new();
        let mut q = Queue::new(8_000);
        assert!(!p.ready(&q, at));
        assert!(q.push(&[1; 320], at));
        assert!(!p.ready(&q, at + Duration::from_millis(39)));
        assert!(p.ready(&q, at + UPLINK_PREFILL_WAIT));
        assert!(q.push(&[2; 640], at));
        assert!(
            !p.ready(&q, at),
            "a batch still needs input jitter headroom"
        );
        assert!(p.ready(&q, at + UPLINK_PREFILL_WAIT));
        assert!(q.next_fresh(at + UPLINK_JITTER_AGE).is_none());
        assert_eq!(
            q.queued_expired, 3,
            "prefill never refreshes first-byte ages"
        );
    }
    #[tokio::test]
    async fn preface_is_bounded_and_does_not_consume_coalesced_audio() {
        let (mut w, mut r) = duplex(2048);
        w.write_all(b"AIRSIMVWL1 1 nonce signature\n\x01\x02")
            .await
            .unwrap();
        assert_eq!(
            preface(&mut r).await.unwrap(),
            "AIRSIMVWL1 1 nonce signature\n"
        );
        assert_eq!(r.read_u16_le().await.unwrap(), 0x0201);
        let (mut w, mut r) = duplex(2048);
        w.write_all(&[b'x'; 1024]).await.unwrap();
        assert!(preface(&mut r).await.is_none());
        let (mut w, mut r) = duplex(32);
        w.write_all(b"bad\r\n").await.unwrap();
        assert!(preface(&mut r).await.is_none());
    }
    #[tokio::test]
    async fn uplink_mute_resampling_and_eof_are_applied_by_the_actual_pump() {
        for muted in [false, true] {
            let (mut phone, reader) = duplex(4096);
            let (writer, mut media) = duplex(4096);
            let task = tokio::spawn(async move {
                uplink(reader, writer, 8_000, &AtomicBool::new(muted)).await
            });
            let input: Vec<u8> = std::iter::repeat_n(1234i16, 160)
                .flat_map(i16::to_le_bytes)
                .collect();
            phone.write_all(&input).await.unwrap();
            let mut output = [0; 640];
            timeout(Duration::from_secs(1), media.read_exact(&mut output))
                .await
                .unwrap()
                .unwrap();
            let expected = if muted { 0i16 } else { 1234i16 };
            assert!(output
                .as_chunks::<2>()
                .0
                .iter()
                .all(|b| *b == expected.to_le_bytes()));
            drop(phone);
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn uplink_paces_both_coalesced_fresh_frames_without_deleting_the_first() {
        for rate in [8_000, 16_000] {
            let (mut phone, reader) = duplex(4096);
            let (writer, mut media) = duplex(4096);
            let task =
                tokio::spawn(
                    async move { uplink(reader, writer, rate, &AtomicBool::new(false)).await },
                );
            let input: Vec<_> = [1234i16, -2345i16]
                .into_iter()
                .flat_map(|sample| {
                    std::iter::repeat_n(sample, (rate / 50) as usize).flat_map(i16::to_le_bytes)
                })
                .collect();
            phone.write_all(&input).await.unwrap();
            for sample in [1234i16, -2345i16] {
                let mut output = [0; FRAME];
                timeout(Duration::from_secs(1), media.read_exact(&mut output))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(
                    output
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .all(|bytes| *bytes == sample.to_le_bytes()),
                    "coalesced microphone frames must be played in order"
                );
            }
            drop(phone);
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn uplink_preserves_four_frame_phone_batches_without_deleting_half_the_speech() {
        for rate in [8_000, 16_000] {
            let (mut phone, reader) = duplex(4096);
            let (writer, mut media) = duplex(4096);
            let task =
                tokio::spawn(
                    async move { uplink(reader, writer, rate, &AtomicBool::new(false)).await },
                );
            let samples = [700i16, 1400, 2100, 2800];
            let input: Vec<_> = samples
                .into_iter()
                .flat_map(|sample| {
                    std::iter::repeat_n(sample, (rate / 50) as usize).flat_map(i16::to_le_bytes)
                })
                .collect();
            phone.write_all(&input).await.unwrap();
            for sample in samples {
                let mut frame = [0; FRAME];
                timeout(Duration::from_secs(1), media.read_exact(&mut frame))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(
                    frame
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .all(|b| *b == sample.to_le_bytes()),
                    "an 80ms microphone batch must reach playout in order"
                );
            }
            drop(phone);
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }
    #[tokio::test]
    async fn partially_written_downlink_closes_instead_of_corrupting_the_next_frame() {
        let (mut source, reader) = duplex(1024);
        let (writer, _slow_phone) = duplex(1);
        source.write_all(&[7; 640]).await.unwrap();
        let result = timeout(Duration::from_secs(1), downlink(reader, writer, 8_000))
            .await
            .unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn unsent_downlink_timeout_drops_a_frame_without_ending_the_actual_pump() {
        for rate in [8_000, 16_000] {
            let frame_len = (rate / 50 * 2) as usize;
            let (mut source, reader) = duplex(1);
            let (mut writer, mut phone) = duplex(frame_len);
            // No part of the first audio frame can fit until this unrelated
            // prefix is consumed. A partial frame must still fail closed.
            writer.write_all(&vec![9; frame_len]).await.unwrap();
            let task = tokio::spawn(downlink(reader, writer, rate));
            source.write_all(&[1; FRAME]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(80)).await;
            assert!(!task.is_finished(), "an unsent frame must not close PCM");
            let mut output = vec![0; frame_len];
            phone.read_exact(&mut output).await.unwrap();
            assert_eq!(output, vec![9; frame_len]);
            source.write_all(&[2; FRAME]).await.unwrap();
            timeout(Duration::from_secs(1), phone.read_exact(&mut output))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(output, vec![2; frame_len], "old audio must not replay");
            drop(source);
            assert_eq!(
                timeout(Duration::from_secs(1), task)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::UnexpectedEof
            );
        }
    }

    #[tokio::test]
    async fn downlink_drains_a_complete_final_frame_before_reporting_source_eof() {
        for rate in [8_000, 16_000] {
            let (mut source, reader) = duplex(FRAME);
            let (writer, mut phone) = duplex(FRAME);
            source.write_all(&[7; FRAME]).await.unwrap();
            drop(source);
            let task = tokio::spawn(downlink(reader, writer, rate));
            let mut output = vec![0; (rate / 50 * 2) as usize];
            timeout(Duration::from_secs(1), phone.read_exact(&mut output))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(output, vec![7; output.len()]);
            assert_eq!(
                timeout(Duration::from_secs(1), task)
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::UnexpectedEof
            );
        }
    }

    #[tokio::test]
    async fn congested_phone_does_not_block_the_worker_or_accumulate_old_audio() {
        let (mut source, reader) = duplex(FRAME * 2);
        let (mut writer, mut phone) = duplex(320);
        writer.write_all(&[9; 320]).await.unwrap();
        let task = tokio::spawn(downlink(reader, writer, 8_000));
        // A sequential write-then-read pump needs 100 * 40 ms here. The
        // bounded latest-frame pump must keep draining the worker instead.
        timeout(Duration::from_millis(500), async {
            for _ in 0..100 {
                source.write_all(&[1; FRAME]).await.unwrap();
            }
        })
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        let mut frame = [0; 320];
        phone.read_exact(&mut frame).await.unwrap();
        assert_eq!(frame, [9; 320]);
        source.write_all(&[2; FRAME]).await.unwrap();
        timeout(Duration::from_secs(1), phone.read_exact(&mut frame))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame, [2; 320], "expired burst must not replay");
        drop(source);
        assert_eq!(
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }

    #[tokio::test]
    async fn downlink_discards_stale_frames_without_retiming_them() {
        let (sender, receiver) = tokio::sync::watch::channel(None);
        let (writer, mut phone) = duplex(FRAME);
        let drops = Arc::new(DownlinkDrops::default());
        let observed = drops.clone();
        sender.send_replace(Some(DownlinkFrame {
            at: Instant::now() - FRAME_AGE,
            pcm: [1; FRAME],
        }));
        let task =
            tokio::spawn(async move { write_downlink(writer, receiver, 8_000, &drops).await });
        timeout(Duration::from_secs(1), async {
            while observed.stale.load(Ordering::Relaxed) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        sender.send_replace(Some(DownlinkFrame {
            at: Instant::now(),
            pcm: [2; FRAME],
        }));
        drop(sender);
        let mut output = [0; 320];
        timeout(Duration::from_secs(1), phone.read_exact(&mut output))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(output, [2; 320]);
        assert_eq!(observed.stale.load(Ordering::Relaxed), 1);
        assert_eq!(observed.no_progress.load(Ordering::Relaxed), 0);
        assert_eq!(
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }

    #[tokio::test]
    async fn downlink_rejects_future_frame_timestamps_before_writing() {
        let (sender, receiver) = tokio::sync::watch::channel(None);
        let (writer, mut phone) = duplex(FRAME);
        sender.send_replace(Some(DownlinkFrame {
            at: Instant::now() + Duration::from_secs(5),
            pcm: [1; FRAME],
        }));
        let result = write_downlink(writer, receiver, 8_000, &DownlinkDrops::default()).await;
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(phone.read(&mut [0; 1]).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn a_delayed_microphone_fragment_does_not_end_the_actual_uplink_pump() {
        let (mut phone, reader) = duplex(4096);
        let (writer, mut media) = duplex(4096);
        let task =
            tokio::spawn(
                async move { uplink(reader, writer, 8_000, &AtomicBool::new(false)).await },
            );
        phone.write_all(&[1]).await.unwrap();
        // Expiry is measured from the first byte using the phone input
        // input budget, independently of when the next pacing wake runs.
        tokio::time::sleep(UPLINK_JITTER_AGE + Duration::from_millis(20)).await;
        let mut batch = vec![2; 319];
        batch.extend(std::iter::repeat_n(1234i16, 160).flat_map(i16::to_le_bytes));
        phone.write_all(&batch).await.unwrap();
        let mut frame = [0; 640];
        timeout(Duration::from_secs(1), media.read_exact(&mut frame))
            .await
            .unwrap()
            .unwrap();
        assert!(frame
            .as_chunks::<2>()
            .0
            .iter()
            .all(|b| *b == 1234i16.to_le_bytes()));
        assert!(!task.is_finished());
        drop(phone);
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn socket_relay_keeps_credentials_local_and_closes_worker_on_revoke_or_disconnect() {
        for (revoked, partial) in [(false, false), (true, false), (false, true), (true, true)] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            let core_path = dir.path().join("core.sock");
            let core_listener = UnixListener::bind(&core_path).unwrap();
            let media_path = dir.path().join("pcm.sock");
            let media_listener = UnixListener::bind(&media_path).unwrap();
            let mut app = super::super::tests::app(core_path);
            app.media = Some(Arc::new(super::super::super::media::Bridge::isolated_pcm(
                dir.path().join("worker.sock"),
                std::fs::metadata(dir.path()).unwrap().uid(),
            )));
            let stop = Arc::new(AtomicBool::new(false));
            let core_stop = stop.clone();
            let core_task = tokio::spawn(async move {
                let mut seen = Vec::new();
                let mut issued = false;
                loop {
                    let (mut s, _) = core_listener.accept().await.unwrap();
                    let b = match async {
                        let size = s.read_u32().await? as usize;
                        assert!(size < 4096);
                        let mut b = vec![0; size];
                        s.read_exact(&mut b).await?;
                        Ok::<_, std::io::Error>(b)
                    }
                    .await
                    {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            assert!(matches!(
                                error.kind(),
                                std::io::ErrorKind::UnexpectedEof
                                    | std::io::ErrorKind::ConnectionReset
                            ));
                            continue;
                        }
                    };
                    let r: Value = serde_json::from_slice(&b).unwrap();
                    let command = &r["command"];
                    let close = command["close"] == true;
                    let result = if command["op"] == "airsim_pcm_open" {
                        assert_eq!(command["peer"], "127.0.0.1");
                        serde_json::json!({"ticket":"a".repeat(32),"session":"b".repeat(32),"pcm_rate":8_000,"waiting":true})
                    } else {
                        assert_eq!(command["ticket"], "a".repeat(32));
                        let credentials = if !issued {
                            issued = true;
                            serde_json::json!({"ticket":vec![9;32],"peer_key":vec![8;32],"boot":"synthetic"})
                        } else {
                            Value::Null
                        };
                        serde_json::json!({"active":!close&&!core_stop.load(Ordering::Acquire),"muted":false,"credentials":credentials})
                    };
                    seen.push(command.clone());
                    let reply = serde_json::to_vec(
                        &serde_json::json!({"protocol":2,"id":r["id"],"result":result}),
                    )
                    .unwrap();
                    let written = async {
                        s.write_u32(reply.len() as u32).await?;
                        s.write_all(&reply).await
                    }
                    .await;
                    if let Err(error) = written {
                        // Closing the phone or revoking its authority cancels
                        // an in-flight heartbeat response. Still require the
                        // subsequent close command below, not a live client.
                        assert!(matches!(
                            error.kind(),
                            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                        ));
                    }
                    if close {
                        break seen;
                    }
                }
            });
            let (microphone_ready, mut microphone_checked) = tokio::sync::oneshot::channel::<()>();
            let media_task = tokio::spawn(async move {
                let (mut s, _) = media_listener.accept().await.unwrap();
                let size = s.read_u32().await.unwrap() as usize;
                assert!(size < 2048);
                let mut b = vec![0; size];
                s.read_exact(&mut b).await.unwrap();
                let hello: Value = serde_json::from_slice(&b).unwrap();
                assert_eq!(
                    hello["credentials"]["ticket"],
                    serde_json::json!(vec![9; 32])
                );
                s.write_u8(1).await.unwrap();
                s.write_all(&[7; 640]).await.unwrap();
                if partial {
                    timeout(Duration::from_secs(1), async {
                        loop {
                            let mut frame = [0; 640];
                            s.read_exact(&mut frame).await.unwrap();
                            if frame == [0; 640] {
                                continue;
                            } // Muted until first authority pulse.
                            assert!(frame
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .all(|b| *b == 1234i16.to_le_bytes()));
                            break;
                        }
                    })
                    .await
                    .unwrap();
                }
                let _ = microphone_ready.send(());
                let mut tail = Vec::new();
                timeout(Duration::from_secs(2), s.take(4097).read_to_end(&mut tail))
                    .await
                    .unwrap()
                    .unwrap();
                assert!(tail.len() <= 4096);
                if !partial {
                    assert!(tail.is_empty());
                }
            });
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut phone = TcpStream::connect(listener.local_addr().unwrap())
                .await
                .unwrap();
            let (s, p) = listener.accept().await.unwrap();
            let edge = tokio::spawn(async move { handle(&app, s, p.ip()).await });
            let mut preface = b"AIRSIMVWL1 1 synthetic-nonce synthetic-signature\n".to_vec();
            if partial {
                preface.push(42);
            }
            phone.write_all(&preface).await.unwrap();
            let mut ready = [0; 11];
            phone.read_exact(&mut ready).await.unwrap();
            assert_eq!(&ready, b"AIRSIMREADY");
            let mut audio = [0; 320];
            timeout(Duration::from_secs(1), async {
                loop {
                    phone.read_exact(&mut audio).await.unwrap();
                    if audio != [0; 320] {
                        break;
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(
                audio, [7; 320],
                "phone receives only PCM, no local accept byte/credential JSON"
            );
            if partial {
                let mut microphone = vec![99; 319];
                microphone.extend(std::iter::repeat_n(1234i16, 160).flat_map(i16::to_le_bytes));
                phone.write_all(&microphone).await.unwrap();
                // Wait for actual non-muted, correctly aligned data rather
                // than racing the first authority response with a sleep.
                let fresh: Vec<_> = std::iter::repeat_n(1234i16, 160)
                    .flat_map(i16::to_le_bytes)
                    .collect();
                timeout(Duration::from_secs(1), async {
                    let mut tick = tokio::time::interval(Duration::from_millis(20));
                    loop {
                        tokio::select! {
                            result=&mut microphone_checked=>{result.unwrap();break;},
                            _=tick.tick()=>{phone.write_all(&fresh).await.unwrap();},
                        }
                    }
                })
                .await
                .unwrap();
            }
            if revoked {
                stop.store(true, Ordering::Release);
                assert_eq!(
                    timeout(Duration::from_secs(1), phone.read_u8())
                        .await
                        .unwrap()
                        .unwrap_err()
                        .kind(),
                    std::io::ErrorKind::UnexpectedEof
                );
            }
            drop(phone);
            timeout(Duration::from_secs(2), edge)
                .await
                .unwrap()
                .unwrap();
            timeout(Duration::from_secs(2), media_task)
                .await
                .unwrap()
                .unwrap();
            let seen = timeout(Duration::from_secs(2), core_task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(seen.last().unwrap()["close"], true);
        }
    }
}
