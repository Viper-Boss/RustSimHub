// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
fn main() {
    if std::env::args().skip(1).collect::<Vec<_>>() != ["--probe-readonly"] {
        eprintln!("usage: rustsimhub-audio-alsa --probe-readonly");
        std::process::exit(2);
    }
    match rustsimhub_audio_alsa::probe() {
        Ok(p) => println!(
            "profile_matches={} routes_on={}/7 pcm_opened=false mixer_written=false",
            p.profile_matches,
            p.routes_on.iter().filter(|v| **v).count()
        ),
        Err(_) => {
            eprintln!("audio profile unavailable");
            std::process::exit(1);
        }
    }
}
