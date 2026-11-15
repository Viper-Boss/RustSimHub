// SPDX-License-Identifier: AGPL-3.0-only

use super::{
    backend::{Backend, Status},
    send, Channel, Context, Link, Reply,
};
use rustsimhub_airsim::voble::{
    call::{self, Command, Media, Target, PCM_BYTES},
    fragment_control,
};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::Weak,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::mpsc,
    task::JoinHandle,
    time::timeout,
};
use zeroize::Zeroizing;

const POLL: Duration = Duration::from_millis(900);
const AUDIO_AGE: Duration = Duration::from_millis(60);

pub(super) struct Microphone {
    pub bytes: Vec<u8>,
    pub received: Instant,
}

struct Call {
    target: Target,
    provisional_id: Option<String>,
    direction: String,
    state: String,
    number: String,
    owned: bool,
    media_used: bool,
}

impl Call {
    fn matches_command(&self, command: &Command) -> bool {
        self.target == command.target
            || (self.owned
                && matches!(command.name.as_str(), "end" | "hangup")
                && self.target.generation == command.target.generation
                && self.provisional_id.as_deref() == Some(command.target.call.as_str()))
    }
}

struct MediaTask(JoinHandle<()>);
impl Drop for MediaTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Controller {
    service: Weak<Context>,
    pairing: Link,
    backend: Backend,
    auth: call::Channel,
    identity: String,
    active: Option<Call>,
    latest_generation: u64,
    media: Option<MediaTask>,
    reply_id: u16,
    commands: VecDeque<(Instant, String)>,
}

pub(super) async fn run(
    service: Weak<Context>,
    pairing: Link,
    backend: Backend,
    key: Zeroizing<[u8; 32]>,
    identity: String,
    mut requests: mpsc::Receiver<Vec<u8>>,
) {
    let mut c = Controller {
        service,
        pairing,
        backend,
        auth: call::Channel::new(&key),
        identity,
        active: None,
        latest_generation: 0,
        media: None,
        reply_id: 0,
        commands: VecDeque::new(),
    };
    let mut tick = tokio::time::interval(POLL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            next = requests.recv() => {
                let Some(bytes) = next else { break; };
                let Ok(now) = call::clock_ms() else { break; };
                let Ok(command) = c.auth.request(&bytes, now) else { continue; };
                if c.request(command).await.is_err() {
                    break;
                }
            }
            _ = tick.tick(), if c.auth.established() => {
                if c.poll().await.is_err() { break; }
            }
        }
    }
    c.stop_media();
    if let Some(service) = c.service.upgrade() {
        service.remove(&c.pairing.stream);
    }
}

type CommandResult = Result<String, &'static str>;

impl Controller {
    async fn out(&mut self, name: &str, target: &Target, body: Value) -> io::Result<()> {
        let ctrl = self
            .service
            .upgrade()
            .and_then(|ctx| ctx.output(&self.pairing, Channel::Control))
            .ok_or(io::ErrorKind::NotConnected)?;
        let bytes = self
            .auth
            .sign(name, target, body)
            .map_err(|_| io::ErrorKind::InvalidData)?;
        self.reply_id = self.reply_id.wrapping_add(1);
        let frames = fragment_control(&bytes, self.reply_id, ctrl.packet)
            .map_err(|_| io::ErrorKind::InvalidData)?;
        send(&Reply {
            stream: ctrl.stream,
            frames,
            ready: false,
        })
        .await
    }

    async fn request(&mut self, command: Command) -> io::Result<()> {
        let Some(id) = command.body["request_id"].as_str().filter(|id| {
            (8..=64).contains(&id.len())
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }) else {
            return Ok(());
        };
        let now = Instant::now();
        self.commands
            .retain(|(at, _)| now.duration_since(*at) < Duration::from_secs(35));
        let result =
            if self.commands.len() >= 128 || self.commands.iter().any(|(_, record)| record == id) {
                Err("operation_already_received")
            } else {
                self.commands.push_back((now, id.to_owned()));
                self.execute(&command, id).await
            };
        let (ok, code, detail) = match result {
            Ok(detail) => (true, "ok", detail),
            Err(code) => (
                false,
                code,
                if code == "outcome_unknown" {
                    "结果未确认，请勿重复操作".into()
                } else {
                    code.into()
                },
            ),
        };
        let sent = self
            .out(
                "response",
                &command.target,
                json!({"request_id":id,"ok":ok,"code":code,"detail":detail}),
            )
            .await;
        if ok && command.name == "unpair" {
            // Give the phone a chance to acknowledge the revocation. Even
            // when it disconnects first, stop advertising the withdrawn peer.
            if let Some(service) = self.service.upgrade() {
                service.revoked.notify_one();
            }
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        sent
    }

    async fn execute(&mut self, command: &Command, id: &str) -> CommandResult {
        match command.name.as_str() {
            "ping" => return Ok("ready".into()),
            "unpair" => {
                self.stop_media();
                self.service
                    .upgrade()
                    .ok_or("bluetooth_disconnected")?
                    .revoke(&self.pairing)
                    .map_err(|_| "outcome_unknown")?;
                return Ok("unpaired".into());
            }
            "device_info" => {
                let mut metadata = self
                    .backend
                    .call("GET", "/v1/health", Value::Null)
                    .await
                    .map_err(status)?;
                metadata["agent_id"] = self.identity.clone().into();
                metadata["agent_kind"] = "standalone".into();
                metadata["call_generation"] = self.latest_generation.into();
                // Keep responses small enough for the iPhone's message fragment limit.
                let info = json!({
                    "agent_id": metadata["agent_id"], "agent_kind": metadata["agent_kind"],
                    "device_name": metadata["device_name"], "call_generation": metadata["call_generation"],
                    "phone_number": metadata["phone_number"], "sim_slots": metadata["sim_slots"],
                });
                return Ok(info.to_string());
            }
            "dial" => return self.dial(command, id).await,
            _ => {}
        }

        let call = self
            .active
            .as_ref()
            .filter(|call| call.matches_command(command))
            .ok_or("call_mismatch")?;
        let action = match command.name.as_str() {
            "answer" if call.state == "incoming" && !call.owned => "answer",
            "reject" if call.state == "incoming" && !call.owned => "reject",
            "end" | "hangup" if call.owned => "hangup",
            "dtmf" if call.owned => "dtmf",
            "start_media" if call.owned && call.state == "active" && !call.media_used => {
                return self.start_media(&command.target).await;
            }
            "stop_media" if call.owned => {
                self.stop_media();
                return Ok("stopped".into());
            }
            _ => return Err("call_mismatch"),
        };
        let mut body = json!({"call_id":call.target.call});
        if action == "dtmf" {
            let digit = command.body["number"]
                .as_str()
                .filter(|d| d.len() == 1 && "0123456789#*".contains(d))
                .ok_or("invalid_digit")?;
            body["digit"] = digit.into();
        }
        self.backend
            .call("POST", &format!("/api/calls/{action}"), body)
            .await
            .map_err(status)?;
        if action == "answer" {
            self.active.as_mut().ok_or("call_mismatch")?.owned = true;
        } else if matches!(action, "hangup" | "reject") {
            self.stop_media();
        }
        Ok("accepted".into())
    }

    async fn dial(&mut self, command: &Command, id: &str) -> CommandResult {
        if self.active.is_some()
            || command.target.call.is_empty()
            || command.target.generation <= self.latest_generation
        {
            return Err("call_busy");
        }
        let number = command.body["number"]
            .as_str()
            .filter(|number| (3..=64).contains(&number.len()))
            .ok_or("invalid_number")?;
        self.latest_generation = command.target.generation;
        let created = self
            .backend
            .call(
                "POST",
                "/api/calls/dial",
                json!({"number": number, "operation_id": id}),
            )
            .await
            .map_err(status)?;
        let id = created["call_id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 160)
            .ok_or("outcome_unknown")?;
        self.active = Some(Call {
            target: Target {
                call: id.to_owned(),
                generation: command.target.generation,
            },
            provisional_id: Some(command.target.call.clone()),
            direction: "outgoing".into(),
            state: String::new(),
            number: number.into(),
            owned: true,
            media_used: false,
        });
        Ok("accepted".into())
    }

    async fn poll(&mut self) -> io::Result<()> {
        let mut route = "/api/calls/status?observe=1".to_owned();
        if let Some(active) = self.active.as_ref().filter(|call| call.owned) {
            let id = active.target.call.as_str();
            if id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
            {
                route.push_str("&own=");
                route.push_str(id);
            }
        }
        let update = match timeout(
            Duration::from_millis(750),
            self.backend.call("GET", &route, Value::Null),
        )
        .await
        {
            Ok(Ok(update)) => update,
            Ok(Err(Status::Revoked)) => {
                self.finish_call().await?;
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            _ => return Ok(()), // A failed query does not prove the modem hung up.
        };
        let observed = &update["active"];
        let Some(id) = observed["id"]
            .as_str()
            .filter(|id| !id.is_empty() && id.len() <= 160)
        else {
            if observed.is_null() {
                self.finish_call().await?;
            }
            return Ok(());
        };
        let phase = match observed["state"].as_str() {
            Some("incoming") => "incoming",
            Some("dialing") => "dialing",
            Some("alerting") => "connecting",
            Some("active") => "active",
            _ => return Ok(()),
        };
        if let Some(current) = &self.active {
            if current.target.call != id || (!current.owned && phase != "incoming") {
                self.finish_call().await?;
            }
        }
        if self.active.is_none() && phase == "incoming" {
            self.latest_generation = self
                .latest_generation
                .checked_add(1)
                .filter(|v| *v < i64::MAX as u64)
                .ok_or(io::ErrorKind::InvalidData)?;
            self.active = Some(Call {
                target: Target {
                    call: id.to_owned(),
                    generation: self.latest_generation,
                },
                provisional_id: None,
                direction: "incoming".into(),
                number: observed["number"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(64)
                    .collect(),
                state: String::new(),
                owned: false,
                media_used: false,
            });
        }
        if let Some(call) = self.active.as_mut().filter(|call| call.state != phase) {
            call.state = phase.into();
            let (target, body) = event(call, phase);
            self.out("call_state", &target, body).await?;
        }
        Ok(())
    }

    async fn finish_call(&mut self) -> io::Result<()> {
        self.stop_media();
        if let Some(call) = self.active.take() {
            let (target, body) = event(&call, "ended");
            self.out("call_state", &target, body).await?;
        }
        Ok(())
    }

    async fn start_media(&mut self, target: &Target) -> CommandResult {
        let audio = self
            .service
            .upgrade()
            .and_then(|ctx| ctx.output(&self.pairing, Channel::Audio))
            .filter(|link| link.packet >= 97)
            .ok_or("audio_mtu_or_subscription_unavailable")?;
        let encoder = self
            .auth
            .media(target)
            .map_err(|_| "media_authentication_failed")?;
        let decoder = self
            .auth
            .media(target)
            .map_err(|_| "media_authentication_failed")?;
        let (stream, size) = self.backend.media().await.map_err(|_| "pcm_unavailable")?;
        // Phone streaming restarts its sequence at zero. A single key/context
        // may start once only to avoid reusing the AES-GCM nonce on the air.
        self.active.as_mut().ok_or("call_mismatch")?.media_used = true;
        let (tx, rx) = mpsc::channel(12);
        self.service
            .upgrade()
            .and_then(|ctx| ctx.microphone(&self.pairing, Some(tx)))
            .ok_or("bluetooth_disconnected")?;
        self.media = Some(MediaTask(tokio::spawn(async move {
            let _ = audio_bridge(stream, size, audio, encoder, decoder, rx).await;
        })));
        Ok("media_ready".into())
    }

    fn stop_media(&mut self) {
        self.media = None;
        if let Some(service) = self.service.upgrade() {
            service.microphone(&self.pairing, None);
        }
    }
}

fn event(call: &Call, state: &str) -> (Target, Value) {
    (
        call.target.clone(),
        json!({"direction":call.direction, "state":state, "number":call.number}),
    )
}

fn status(from: Status) -> &'static str {
    match from {
        Status::Refused => "core_refused",
        Status::Unknown => "outcome_unknown",
        Status::Revoked => "bridge_revoked",
    }
}

async fn audio_bridge(
    stream: UnixStream,
    size: usize,
    output: Link,
    mut encoder: Media,
    mut decoder: Media,
    mut microphone: mpsc::Receiver<Microphone>,
) -> io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    let down = async {
        let mut pcm = [0; PCM_BYTES * 2];
        loop {
            timeout(Duration::from_secs(2), reader.read_exact(&mut pcm[..size])).await??;
            let frames = encoder
                .downlink(&narrow(&pcm[..size]), output.packet)
                .map_err(|_| io::ErrorKind::InvalidData)?;
            // Bound notification latency as a whole. The radio can always drop
            // a delayed frame and recover on the next sequence number.
            let sent = timeout(Duration::from_millis(20), async {
                for frame in frames {
                    let n = output
                        .stream
                        .async_io(tokio::io::Interest::WRITABLE, |mut fd| fd.write(&frame))
                        .await?;
                    if n != frame.len() {
                        return Err(io::ErrorKind::WriteZero.into());
                    }
                }
                Ok::<_, io::Error>(())
            })
            .await;
            if let Ok(result) = sent {
                result?;
            }
        }
    };
    let up = async {
        let mut wide = [0; PCM_BYTES * 2];
        while let Some(frame) = microphone.recv().await {
            if frame.received.elapsed() >= AUDIO_AGE {
                continue;
            }
            let Ok(Some(pcm)) = decoder.uplink(&frame.bytes, frame.received) else {
                continue;
            };
            let data = if size == PCM_BYTES {
                &pcm[..]
            } else {
                widen(&pcm, &mut wide);
                &wide[..]
            };
            timeout(Duration::from_millis(40), writer.write_all(data)).await??;
        }
        Ok(())
    };
    tokio::select! { result = down => result, result = up => result }
}

fn narrow(pcm: &[u8]) -> [u8; PCM_BYTES] {
    let mut output = [0; PCM_BYTES];
    if pcm.len() == PCM_BYTES {
        output.copy_from_slice(pcm);
        return output;
    }
    for (pair, out) in pcm
        .as_chunks::<4>()
        .0
        .iter()
        .zip(output.as_chunks_mut::<2>().0.iter_mut())
    {
        let a = i32::from(i16::from_le_bytes([pair[0], pair[1]]));
        let b = i32::from(i16::from_le_bytes([pair[2], pair[3]]));
        out.copy_from_slice(&(((a + b) / 2) as i16).to_le_bytes());
    }
    output
}

fn widen(pcm: &[u8; PCM_BYTES], output: &mut [u8; PCM_BYTES * 2]) {
    for (index, (sample, out)) in pcm
        .as_chunks::<2>()
        .0
        .iter()
        .zip(output.as_chunks_mut::<4>().0.iter_mut())
        .enumerate()
    {
        let a = i32::from(i16::from_le_bytes([sample[0], sample[1]]));
        let next = pcm.get(index * 2 + 2..index * 2 + 4).unwrap_or(sample);
        let b = i32::from(i16::from_le_bytes([next[0], next[1]]));
        out[..2].copy_from_slice(sample);
        out[2..].copy_from_slice(&(((a + b) / 2) as i16).to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use socket2::{Domain, Socket, Type};
    use std::{io::Read, sync::Arc};
    use tokio::io::{unix::AsyncFd, Interest};

    #[test]
    fn early_hangup_matches_only_this_outgoing_attempt() {
        let mut call = Call {
            target: Target {
                call: "modem-call".into(),
                generation: 17,
            },
            provisional_id: Some("phone-attempt".into()),
            direction: "outgoing".into(),
            state: "dialing".into(),
            number: "10010".into(),
            owned: true,
            media_used: false,
        };
        let command = |name: &str, id: &str, generation| Command {
            name: name.into(),
            target: Target {
                call: id.into(),
                generation,
            },
            body: Value::Null,
        };

        assert!(call.matches_command(&command("end", "phone-attempt", 17)));
        assert!(call.matches_command(&command("hangup", "modem-call", 17)));
        assert!(!call.matches_command(&command("end", "phone-attempt", 16)));
        assert!(!call.matches_command(&command("end", "other-attempt", 17)));
        assert!(!call.matches_command(&command("dtmf", "phone-attempt", 17)));
        assert!(!call.matches_command(&command("start_media", "phone-attempt", 17)));
        call.owned = false;
        assert!(!call.matches_command(&command("end", "phone-attempt", 17)));
    }

    fn fixture(data: &str) -> Vec<u8> {
        let data = data.trim();
        (0..data.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&data[at..at + 2], 16).unwrap())
            .collect()
    }

    #[tokio::test]
    async fn encrypted_ble_and_pcm_pass_speech_both_ways_and_close_with_the_worker() {
        let mut auth = call::Channel::new(&std::array::from_fn(|index| index as u8));
        let control = fixture(include_str!("../../../airsim/tests/data/voble-control.hex"));
        let target = auth.request(&control, 1_700_000_000_500).unwrap().target;
        let encoder = auth.media(&target).unwrap();
        let decoder = auth.media(&target).unwrap();
        let (local, remote) = Socket::pair(Domain::UNIX, Type::SEQPACKET, None).unwrap();
        local.set_nonblocking(true).unwrap();
        remote.set_nonblocking(true).unwrap();
        let output = Link {
            packet: 180,
            stream: Arc::new(AsyncFd::new(local).unwrap()),
        };
        let phone = AsyncFd::new(remote).unwrap();
        let (mut worker, relay) = UnixStream::pair().unwrap();
        let (mic, rx) = mpsc::channel(2);
        let bridge = tokio::spawn(audio_bridge(relay, 640, output, encoder, decoder, rx));
        timeout(Duration::from_secs(1), async {
            let voice: Vec<u8> = std::iter::repeat_n(988i16, 320)
                .flat_map(i16::to_le_bytes)
                .collect();
            worker.write_all(&voice).await.unwrap();
            let mut frame = [0; 180];
            let read = phone
                .async_io(Interest::READABLE, |mut fd| fd.read(&mut frame))
                .await
                .unwrap();
            assert_eq!(read, 180);
            assert_eq!(&frame[..7], &[0xa5, 0x5a, 1, 1, 0, 1, 2]);
            assert_eq!(&frame[15..18], &[0, 2, 162]);
            let read = phone
                .async_io(Interest::READABLE, |mut fd| fd.read(&mut frame))
                .await
                .unwrap();
            assert_eq!(read, 32);
            assert_eq!(&frame[15..18], &[1, 2, 14]);
            mic.send(Microphone {
                bytes: fixture(include_str!(
                    "../../../airsim/tests/data/voble-microphone.hex"
                )),
                received: Instant::now(),
            })
            .await
            .unwrap();
            let mut pcm = [0u8; 640];
            worker.read_exact(&mut pcm).await.unwrap();
            let first: Vec<_> = pcm
                .as_chunks::<2>()
                .0
                .iter()
                .take(8)
                .map(|p| i16::from_le_bytes([p[0], p[1]]))
                .collect();
            assert_eq!(first, [988, 0, -988, 0, 988, 0, -988, 0]);
            drop(worker);
            assert_eq!(
                bridge.await.unwrap().unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
        })
        .await
        .unwrap();
    }
}
