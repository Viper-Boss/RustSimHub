// SPDX-License-Identifier: AGPL-3.0-only
#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["mode-controller"] {
        return match rustsimhub_ble::bluetooth_mode::serve().await {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("Bluetooth mode controller: {err}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let [peer] = args.as_slice() else {
        eprintln!("usage: rustsimhub-nas-bluetooth PAIRED_NAS_MAC");
        return std::process::ExitCode::FAILURE;
    };
    if let Err(error) = rustsimhub_ble::nas_bluetooth::run(peer).await {
        eprintln!("NasSimHub Bluetooth: {error}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("NasSimHub Bluetooth requires Linux and BlueZ");
    std::process::exit(1);
}
