// SPDX-License-Identifier: AGPL-3.0-only
//! Private-file LAN diagnostic. Hardware opens only after DTLS AND core authorization.
use rustsimhub_audio_alsa::rtc::Session;
use rustsimhub_media::{
    local::{self, Client},
    Frame, Pump, RemoteAuthority, SAMPLES,
};
use serde::Deserialize;
use std::{
    io::{Read, Write},
    net::{IpAddr, SocketAddr, UdpSocket},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    OfferSynthetic,
    OfferSyntheticGated,
    AnswerSynthetic,
    AnswerAuthorized,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    mode: Mode,
    bind: SocketAddr,
    peer_ip: IpAddr,
    seconds: u64,
    core_socket: Option<PathBuf>,
    core_uid: Option<u32>,
}
fn directory(path: &Path) -> Result<(), &'static str> {
    let m = std::fs::symlink_metadata(path).map_err(|_| "private directory unavailable")?;
    if !m.is_dir() || m.uid() != rustix::process::geteuid().as_raw() || m.mode() & 0o077 != 0 {
        return Err("private directory denied");
    }
    Ok(())
}
pub(crate) fn read(path: &Path, limit: usize) -> Result<Vec<u8>, &'static str> {
    directory(path.parent().ok_or("private path denied")?)?;
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|_| "private file unavailable")?;
    let m = f.metadata().map_err(|_| "private file denied")?;
    if !m.is_file()
        || m.uid() != rustix::process::geteuid().as_raw()
        || m.mode() & 0o077 != 0
        || m.nlink() != 1
        || m.len() > limit as u64
    {
        return Err("private file denied");
    }
    let mut bytes = Vec::new();
    f.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "private read failed")?;
    if bytes.len() > limit {
        return Err("private size denied");
    }
    Ok(bytes)
}
fn write(path: &Path, bytes: &[u8]) -> Result<(), &'static str> {
    directory(path.parent().ok_or("private path denied")?)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("private output name")?;
    let temporary = path.with_file_name(format!("{name}.next"));
    let mut f = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temporary)
        .map_err(|_| "private output already exists or unavailable")?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "private write failed")?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        &temporary,
        rustix::fs::CWD,
        path,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|_| "private output already exists or unavailable")?;
    std::fs::File::open(path.parent().ok_or("private output parent")?)
        .and_then(|d| d.sync_all())
        .map_err(|_| "private output sync failed")
}
fn wait(path: &Path, limit: usize, end: Instant) -> Result<Vec<u8>, &'static str> {
    loop {
        if Instant::now() >= end {
            return Err("signaling deadline");
        }
        match std::fs::symlink_metadata(path) {
            Ok(_) => return read(path, limit),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => return Err("private signaling denied"),
        }
    }
}
pub fn run(path: &str) -> Result<(), &'static str> {
    let path = Path::new(path);
    let base = path.parent().ok_or("private config path")?;
    let config: Config =
        serde_json::from_slice(&read(path, 4096)?).map_err(|_| "invalid private config")?;
    if !(1..=10).contains(&config.seconds) {
        return Err("invalid diagnostic duration");
    }
    let socket = UdpSocket::bind(config.bind).map_err(|_| "UDP bind failed")?;
    let end = Instant::now() + Duration::from_secs(30);
    let mut session = match config.mode {
        Mode::OfferSynthetic | Mode::OfferSyntheticGated => {
            let (mut session, offer, pending) =
                Session::offer(&socket, config.peer_ip).map_err(|_| "offer failed")?;
            write(&base.join("offer.sdp"), offer.as_bytes())?;
            let answer = wait(&base.join("answer.sdp"), 16384, end)?;
            session
                .accept_answer(
                    &socket,
                    pending,
                    std::str::from_utf8(&answer).map_err(|_| "answer encoding")?,
                )
                .map_err(|_| "answer denied")?;
            session
        }
        _ => {
            let offer = wait(&base.join("offer.sdp"), 16384, end)?;
            let (session, answer) = Session::answer(
                &socket,
                config.peer_ip,
                std::str::from_utf8(&offer).map_err(|_| "offer encoding")?,
            )
            .map_err(|_| "offer denied")?;
            write(&base.join("answer.sdp"), answer.as_bytes())?;
            session
        }
    };
    while session.verified_peer().is_none() {
        if Instant::now() >= end {
            return Err("DTLS deadline");
        }
        session
            .service(&socket, local::now_ms())
            .map_err(|_| "DTLS handshake failed")?;
        std::thread::sleep(Duration::from_millis(2));
    }
    let peer = session.verified_peer().ok_or("DTLS peer unavailable")?;
    write(
        &base.join("verified.json"),
        serde_json::to_vec(&serde_json::json!({"dtls_verified":true,"peer_key":peer}))
            .map_err(|_| "verified metadata")?
            .as_slice(),
    )?;
    if matches!(config.mode, Mode::OfferSyntheticGated) {
        // Diagnostic peer only: finish DTLS before dialing, then await an explicit
        // private start marker. No PCM and no media packets while waiting.
        let start = base.join("synthetic-start");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match std::fs::symlink_metadata(&start) {
                Ok(_) => {
                    if read(&start, 16)? != b"start\n" {
                        return Err("invalid synthetic start marker");
                    }
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err("private synthetic start denied"),
            }
            if Instant::now() >= deadline {
                return Err("synthetic start deadline");
            }
            session
                .service(&socket, local::now_ms())
                .map_err(|_| "gated transport stopped")?;
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    match config.mode {
        Mode::AnswerAuthorized => authorized(base, &config, session, socket, peer),
        _ => synthetic(base, config.seconds, session, socket, peer),
    }
}
fn synthetic(
    base: &Path,
    seconds: u64,
    mut session: Session,
    socket: UdpSocket,
    peer: [u8; 32],
) -> Result<(), &'static str> {
    let boot = "s".repeat(64);
    let mut authority = RemoteAuthority::new(boot.clone()).map_err(|_| "synthetic gate")?;
    let now = local::now_ms();
    authority
        .update(&boot, now + 1000, now)
        .map_err(|_| "synthetic gate")?;
    session
        .authorize(authority.permit(), peer, now)
        .map_err(|_| "synthetic identity")?;
    let end = Instant::now() + Duration::from_secs(seconds);
    let mut next = Instant::now();
    let mut sequence = 0;
    let mut peak = 0;
    let mut peer_closed = false;
    while Instant::now() < end {
        let now = local::now_ms();
        authority
            .update(&boot, now + 1000, now)
            .map_err(|_| "synthetic expired")?;
        match session.service(&socket, now) {
            Ok(()) => {}
            Err(rustsimhub_audio_alsa::rtc::Error::Closed) => {
                peer_closed = true;
                break;
            }
            Err(_) => return Err("synthetic transport failed"),
        }
        if Instant::now() >= next {
            session
                .send(
                    &socket,
                    &Frame {
                        samples: [0; SAMPLES],
                        sequence,
                        at_ms: now,
                    },
                    now,
                )
                .map_err(|_| "synthetic send failed")?;
            sequence += 1;
            next += Duration::from_millis(20);
            if next < Instant::now() {
                next = Instant::now() + Duration::from_millis(20);
            }
        }
        while let Some(f) = session
            .receive(local::now_ms())
            .map_err(|_| "synthetic receive failed")?
        {
            peak = peak.max(
                f.samples
                    .iter()
                    .map(|s| i32::from(*s).unsigned_abs())
                    .max()
                    .unwrap_or(0),
            );
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    authority.revoke();
    let stopped = session.service(&socket, local::now_ms());
    let revoked = stopped == Err(rustsimhub_audio_alsa::rtc::Error::Authorization);
    let closed = stopped.is_err();
    let value = serde_json::json!({"synthetic_endpoint":true,"pcm_opened":false,"uplink":"encoded_silence","dtls_verified":true,"sent_frames":session.sent_frames,"received_frames":session.received_frames,"dropped_frames":session.dropped_frames,"decoded_peak":peak,"revocation_closed":revoked,"closed":closed,"peer_closed":peer_closed});
    let bytes = serde_json::to_vec(&value).map_err(|_| "summary failed")?;
    write(&base.join("result.json"), &bytes)?;
    println!("{value}");
    if !closed {
        return Err("synthetic did not close");
    }
    Ok(())
}
fn authorized(
    base: &Path,
    config: &Config,
    mut session: Session,
    socket: UdpSocket,
    peer: [u8; 32],
) -> Result<(), &'static str> {
    let ticket_path = base.join("ticket.json");
    let wait_end = Instant::now() + Duration::from_secs(30);
    while !ticket_path.exists() {
        if Instant::now() >= wait_end {
            return Err("core ticket deadline");
        }
        session
            .service(&socket, local::now_ms())
            .map_err(|_| "pre-authorization transport lost")?;
        std::thread::sleep(Duration::from_millis(2));
    }
    let credentials = local::read_credentials(&ticket_path).map_err(|_| "core ticket denied")?;
    if credentials.peer_key != peer {
        return Err("core ticket and verified DTLS identity differ");
    }
    let core_socket = config.core_socket.as_ref().ok_or("core socket missing")?;
    let core_uid = config.core_uid.ok_or("core uid missing")?;
    if !crate::trusted::core_uid(core_uid) {
        return Err("untrusted core identity");
    }
    let seconds = config.seconds;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime failed")?;
    let value=runtime.block_on(async{
  let mut client=Client::redeem(core_socket,core_uid,credentials).await.map_err(|_|"core authorization denied")?;let permit=client.permit();session.authorize(permit.clone(),peer,local::now_ms()).map_err(|_|"DTLS media gate denied")?;
  let worker=std::thread::spawn(move||{
   let device=rustsimhub_audio_alsa::UfiDuplex::open(&permit,local::now_ms).map_err(|_|"PCM open denied")?;let mut pump=Pump::new(device,permit,local::now_ms()).map_err(|_|"PCM gate denied")?;
   let end=Instant::now()+Duration::from_secs(seconds);let mut transport_failed=false;
   while Instant::now()<end{let now=local::now_ms();if session.service(&socket,now).is_err()||pump.step(local::now_ms()).is_err(){transport_failed=true;break;}
    while let Some(frame)=pump.capture(local::now_ms()).map_err(|_|"PCM capture stopped")?{if session.send(&socket,&frame,local::now_ms()).is_err(){transport_failed=true;break;}}
    while let Some(frame)=session.receive(local::now_ms()).map_err(|_|"RTC receive stopped")?{pump.enqueue(frame,local::now_ms()).map_err(|_|"PCM queue denied")?;}
    if transport_failed{break;}std::thread::sleep(Duration::from_millis(2));
   }
   pump.close();session.close();let s=pump.stats();Ok::<_,&'static str>(serde_json::json!({"pcm_opened":true,"dtls_verified":true,"core_authorized":true,"sent_frames":session.sent_frames,"received_frames":session.received_frames,"captured_frames":s.captured_frames,"played_frames":s.played_frames,"capture_peak":s.capture_peak,"playback_peak":s.playback_peak,"xruns":s.xruns,"dropped_frames":s.dropped_frames+session.dropped_frames,"transport_failed":transport_failed,"closed":true,"recording":false}))
  });
  let mut ticks=tokio::time::interval(Duration::from_millis(250));ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  let mut term=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|_|"signal unavailable")?;
  let mut interrupt=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).map_err(|_|"signal unavailable")?;
  let limit=tokio::time::Instant::now()+Duration::from_secs(seconds+3);
  while !worker.is_finished(){tokio::select!{_=term.recv()=>break,_=interrupt.recv()=>break,_=tokio::time::sleep_until(limit)=>break,_=ticks.tick()=>{if client.pulse().await.is_err(){break;}}}}
  drop(client);worker.join().map_err(|_|"media worker failed")?
 })?;
    write(
        &base.join("result.json"),
        &serde_json::to_vec(&value).map_err(|_| "summary failed")?,
    )?;
    println!("{value}");
    Ok(())
}
