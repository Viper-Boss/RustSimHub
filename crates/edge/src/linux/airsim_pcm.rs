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
// AirSIM's public wire marker, identical to rustsimhub_airsim::PCM_READY.
const READY: &[u8] = b"AIRSIMREADY";

async fn core(app: &App, command: AuthCommand) -> Option<Value> {
    timeout(CORE_WAIT, ask(app, command)).await.ok().flatten()
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
}
impl Queue {
    fn new(rate: u32) -> Self {
        Self {
            partial: Vec::new(),
            partial_at: None,
            frames: VecDeque::new(),
            size: rate as usize / 50 * 2,
        }
    }
    fn push(&mut self, bytes: &[u8], now: Instant) -> bool {
        if self.partial_at.is_some_and(|at| {
            now.checked_duration_since(at)
                .is_none_or(|age| age >= FRAME_AGE)
        }) {
            return false; // Cannot discard part of a raw frame and stay aligned.
        }
        if self.partial.is_empty() {
            self.partial_at = Some(now);
        }
        // The only caller reads into a fixed four-frame array.
        self.partial.extend_from_slice(bytes);
        let whole = self.partial.len() / self.size;
        for chunk in self.partial[..whole * self.size].chunks_exact(self.size) {
            if self.frames.len() == 2 {
                self.frames.pop_front();
            }
            self.frames.push_back((now, chunk.to_vec()));
        }
        self.partial.drain(..whole * self.size);
        if self.partial.is_empty() {
            self.partial_at = None;
        } else if whole > 0 {
            self.partial_at = Some(now);
        }
        true
    }
    fn latest(&mut self, now: Instant) -> Option<Vec<u8>> {
        let latest = self.frames.pop_back();
        self.frames.clear();
        latest
            .filter(|(at, _)| {
                now.checked_duration_since(*at)
                    .is_some_and(|age| age < FRAME_AGE)
            })
            .map(|(_, b)| b)
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
    let mut tick = tokio::time::interval(Duration::from_millis(20));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            n=phone.read(&mut bytes) => {
                let n=n?; if n==0 {return Ok(());}
                if !queue.push(&bytes[..n],Instant::now()) {return Err(std::io::ErrorKind::InvalidData.into());}
            }
            _=tick.tick() => {
                if let Some(bytes)=queue.latest(Instant::now()) {
                    let mut frame=convert(&bytes,rate,16_000).ok_or(std::io::ErrorKind::InvalidData)?;
                    if muted.load(Ordering::Acquire) { frame.fill(0); }
                    timeout(IO_WAIT,media.write_all(&frame)).await.map_err(|_|std::io::ErrorKind::TimedOut)??;
                }
            }
        }
    }
}

/// The app opens PCM while an owned call may still be dialling. Send its
/// authenticated ready marker and silence, but never open ALSA before ACTIVE.
async fn activate(app: &App, phone: &mut TcpStream, opened: Value) -> Option<Value> {
    let rate = opened["pcm_rate"]
        .as_u64()
        .filter(|r| matches!(r, 8_000 | 16_000))?;
    let size = rate as usize / 50 * 2;
    let ticket = opened["ticket"].as_str()?;
    timeout(IO_WAIT, phone.write_all(READY)).await.ok()?.ok()?;
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
    if consumed != 0 {
        timeout(FRAME_AGE, phone.read_exact(&mut bytes[..size - consumed]))
            .await
            .ok()?
            .ok()?;
    }
    let mut result = opened;
    result["credentials"] = active["credentials"].clone();
    Some(result)
}
async fn downlink<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut media: R,
    mut phone: W,
    rate: u32,
) -> std::io::Result<()> {
    let mut frame = [0; FRAME];
    loop {
        timeout(Duration::from_secs(2), media.read_exact(&mut frame))
            .await
            .map_err(|_| std::io::ErrorKind::TimedOut)??;
        let output = convert(&frame, 16_000, rate).ok_or(std::io::ErrorKind::InvalidData)?;
        timeout(IO_WAIT, phone.write_all(&output))
            .await
            .map_err(|_| std::io::ErrorKind::TimedOut)??;
    }
}

async fn relay(app: &App, phone: TcpStream, opened: &Value) -> Option<()> {
    let rate = opened["pcm_rate"]
        .as_u64()
        .filter(|r| matches!(r, 8_000 | 16_000))? as u32;
    let session = opened["session"].as_str()?;
    if !rustsimhub_media::worker::session_id(session) {
        return None;
    }
    let credentials: rustsimhub_media::protocol::Credentials =
        serde_json::from_value(opened["credentials"].clone()).ok()?;
    let (path, uid) = app.media.as_ref()?.pcm_endpoint()?;
    super::super::socket_safe(&path, uid).ok()?;
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
    // No detached pumps. Completion/cancellation drops both socket halves;
    // media EOF invalidates the worker's permit, without redial or reconnect.
    tokio::select! {
        _=uplink(phone_read,media_write,rate,&muted)=>{},
        _=downlink(media_read,phone_write,rate)=>{},
        _=heartbeat=>{},
    }
    Some(())
}

pub(super) async fn handle(app: &App, mut phone: TcpStream, peer: IpAddr) {
    if !rustsimhub_media::worker::local_ip(peer) || app.media.is_none() {
        return;
    }
    let Ok(Some(preface)) = timeout(Duration::from_secs(3), preface(&mut phone)).await else {
        return;
    };
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
    let socket = socket2::SockRef::from(&phone);
    let configured = phone.set_nodelay(true).is_ok()
        && socket.set_recv_buffer_size(4096).is_ok()
        && socket.set_send_buffer_size(4096).is_ok();
    if configured {
        if let Some(active) = activate(app, &mut phone, opened).await {
            let _ = relay(app, phone, &active).await;
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
        assert_eq!(
            timeout(Duration::from_secs(1), phone.read(&mut [0; 1]))
                .await
                .unwrap()
                .unwrap(),
            0
        );
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
        assert!(q.latest(now).is_none());
        assert!(q.push(&[2; 319], now + Duration::from_millis(1)));
        assert_eq!(q.latest(now + Duration::from_millis(59)).unwrap()[0], 1);
        for n in 0..12 {
            assert!(q.push(&[n; 320], now));
            assert!(q.frames.len() <= 2);
        }
        assert_eq!(q.latest(now).unwrap(), vec![11; 320]);
        assert!(q.frames.is_empty());
        assert!(q.push(&[3; 320], now));
        assert!(q.latest(now + FRAME_AGE).is_none());
        assert!(q.push(&[1], now));
        assert!(!q.push(&[2; 319], now + FRAME_AGE));
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
    async fn slow_downlink_closes_instead_of_accumulating_speech() {
        let (mut source, reader) = duplex(1024);
        let (writer, _slow_phone) = duplex(1);
        source.write_all(&[7; 640]).await.unwrap();
        let result = timeout(Duration::from_secs(1), downlink(reader, writer, 8_000))
            .await
            .unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    }
    #[tokio::test]
    async fn socket_relay_keeps_credentials_local_and_closes_worker_on_revoke_or_disconnect() {
        for revoked in [false, true] {
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
                    let size = s.read_u32().await.unwrap() as usize;
                    assert!(size < 4096);
                    let mut b = vec![0; size];
                    s.read_exact(&mut b).await.unwrap();
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
                    s.write_u32(reply.len() as u32).await.unwrap();
                    s.write_all(&reply).await.unwrap();
                    if close {
                        break seen;
                    }
                }
            });
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
                let n = timeout(Duration::from_secs(2), s.read(&mut [0; 1]))
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(n, 0);
            });
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut phone = TcpStream::connect(listener.local_addr().unwrap())
                .await
                .unwrap();
            let (s, p) = listener.accept().await.unwrap();
            let edge = tokio::spawn(async move { handle(&app, s, p.ip()).await });
            phone
                .write_all(b"AIRSIMVWL1 1 synthetic-nonce synthetic-signature\n")
                .await
                .unwrap();
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
