// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
mod authorized;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
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
