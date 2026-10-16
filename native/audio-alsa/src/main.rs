// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
mod authorized;
mod control_socket;
mod rtc_lan;
mod trusted;
mod worker;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let [command, config] = args.as_slice() {
        if command == "--serve-worker" {
            if let Err(reason) = worker::run(config) {
                eprintln!("media worker stopped: {reason}");
                std::process::exit(1);
            }
            return;
        }
        if command == "--rtc-lan-peer" {
            if let Err(reason) = rtc_lan::run(config) {
                eprintln!("LAN media stopped: {reason}");
                std::process::exit(1);
            }
            return;
        }
    }
    if args == ["--rtc-self-check"] {
        match rustsimhub_audio_alsa::rtc::self_check() {
            Ok(value) => println!("{value}"),
            Err(_) => {
                eprintln!("synthetic encrypted transport check failed");
                std::process::exit(1);
            }
        }
        return;
    }
    if args == ["--codec-self-check"] {
        match rustsimhub_audio_alsa::codec::self_check() {
            Ok(value) => println!("{value}"),
            Err(_) => {
                eprintln!("synthetic codec check failed");
                std::process::exit(1);
            }
        }
        return;
    }
    if args == ["--build-info"] {
        println!(
            "{}",
            serde_json::json!({"source_archive_sha256":rustsimhub_media::build_source_sha256(),"local_media_worker_supported":true})
        );
        return;
    }
    if let [command, socket, uid, credentials, seconds] = args.as_slice() {
        if command == "--watch-authorized" || command == "--duplex-authorized" {
            let result = uid
                .parse()
                .ok()
                .zip(seconds.parse().ok())
                .ok_or("invalid arguments")
                .and_then(|(uid, seconds)| {
                    authorized::run(
                        socket,
                        uid,
                        credentials,
                        seconds,
                        command == "--duplex-authorized",
                    )
                });
            if let Err(reason) = result {
                eprintln!("audio stopped: {reason}");
                std::process::exit(1);
            }
            return;
        }
    }
    if args != ["--probe-readonly"] {
        eprintln!("usage: rustsimhub-audio-alsa --probe-readonly");
        std::process::exit(2);
    }
    match rustsimhub_audio_alsa::probe() {
        Ok(p) => println!(
            "profile_matches={} routes_on={}/7 pcm_opened=false mixer_written=false",
            p.profile_matches,
            p.routes_on.iter().filter(|v| **v).count()
        ),
        Err(stage) => {
            eprintln!("audio profile unavailable: {stage:?}");
            std::process::exit(1);
        }
    }
}
