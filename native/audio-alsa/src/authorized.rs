// SPDX-License-Identifier: AGPL-3.0-only
use rustsimhub_media::{
    local::{self, Client},
    Pump,
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
pub fn run(
    socket: &str,
    uid: u32,
    credentials: &str,
    seconds: u64,
    pcm: bool,
) -> Result<(), &'static str> {
    if !(1..=60).contains(&seconds) {
        return Err("invalid duration");
    }
    let credentials =
        local::read_credentials(Path::new(credentials)).map_err(|_| "private ticket denied")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "runtime unavailable")?;
    runtime.block_on(async {
        use tokio::signal::unix::{signal,SignalKind};
        let mut term=signal(SignalKind::terminate()).map_err(|_|"signal unavailable")?;
        let mut interrupt=signal(SignalKind::interrupt()).map_err(|_|"signal unavailable")?;
        let mut client=Client::redeem(Path::new(socket),uid,credentials).await.map_err(|_|"media authorization denied")?;
        let permit=client.permit();let thread_permit=permit.clone();
        let worker=std::thread::spawn(move || {
            if !pcm {return Ok(None);}
            let device=rustsimhub_audio_alsa::UfiDuplex::open(&thread_permit,local::now_ms)?;
            let mut pump=Pump::new(device,thread_permit,local::now_ms())?;
            let start=Instant::now();let mut previous=0;
            let stopped = loop {
                if let Err(reason) = pump.step(local::now_ms()) {break reason;}
                let stats=pump.stats();
                if stats.played_frames!=previous && stats.played_frames<=16 {eprintln!("pcm_schedule elapsed_ms={} played={} captured={}",start.elapsed().as_millis(),stats.played_frames,stats.captured_frames);previous=stats.played_frames;}
                // Diagnostic only: no recording or remote audio sink. Uplink remains silence.
                while pump.capture(local::now_ms()).ok().flatten().is_some() {}
                std::thread::sleep(Duration::from_millis(5));
            };
            pump.close();Ok::<_,rustsimhub_media::Error>(Some((pump.stats(),stopped)))
        });
        let end=Instant::now()+Duration::from_secs(seconds);
        let mut ticks=tokio::time::interval(Duration::from_millis(250));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut denied=false;
        let mut worker_stopped=false;
        loop {
            tokio::select! {
                _=term.recv()=>break,
                _=interrupt.recv()=>break,
                _=tokio::time::sleep_until(end.into())=>break,
                _=ticks.tick()=>{
                    if worker.is_finished() && pcm {worker_stopped=true;break;}
                    if client.pulse().await.is_err() {denied=true;break;}
                }
            }
        }
        drop(client); // Revokes the mirrored Permit before waiting for the PCM thread.
        let stats=worker.join().map_err(|_|"audio worker failed")?.map_err(|_|"audio device unavailable")?;
        if permit.check(local::now_ms()).is_ok() {return Err("media did not close");}
        let summary=match stats {
            Some((s,reason))=>serde_json::json!({"pcm_opened":true,"captured_frames":s.captured_frames,"played_frames":s.played_frames,
                "capture_peak":s.capture_peak,"playback_peak":s.playback_peak,"xruns":s.xruns,"dropped_frames":s.dropped_frames,
                "uplink":"silence_only","closed":true,"authorization_lost":denied,"worker_stopped":worker_stopped,"worker_reason":format!("{reason:?}")}),
            None=>serde_json::json!({"pcm_opened":false,"closed":true,"authorization_lost":denied}),
        };
        println!("{summary}");
        if denied || worker_stopped {return Err("media authorization or audio stopped");}
        Ok(())
    })
}
