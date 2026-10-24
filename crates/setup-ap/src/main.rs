// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
fn main() {
    #[cfg(target_os = "linux")]
    let result = rustsimhub_setup_ap::executor::cli(std::env::args().skip(1).collect());
    #[cfg(not(target_os = "linux"))]
    let result: Result<(), ()> = Err(());
    if result.is_err() {
        eprintln!("setup AP: protected configuration or bounded operation failed");
        std::process::exit(1);
    }
}
