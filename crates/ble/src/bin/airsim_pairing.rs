// SPDX-License-Identifier: AGPL-3.0-only

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    use std::{env, path::Path, process::ExitCode};
    let args: Vec<_> = env::args_os().skip(1).collect();
    if let [command, state] = &args[..] {
        if let Some(command) = command.to_str() {
            return match rustsimhub_ble::airsim_pairing::run(command, Path::new(state)).await {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("AirSIM VoBLE: {error}");
                    ExitCode::FAILURE
                }
            };
        }
    }
    eprintln!("usage: rustsimhub-airsim-pairing <pair|serve|status|revoke> STATE_DIR");
    ExitCode::FAILURE
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("AirSIM VoBLE requires Linux and BlueZ.");
    std::process::exit(1);
}
