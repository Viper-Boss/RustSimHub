// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
mod linux;
fn main() {
    #[cfg(target_os = "linux")]
    if let Err(reason) = linux::run(std::env::args().skip(1).collect()) {
        // Errors are fixed labels. Request bodies, key material and addresses are never logged.
        eprintln!("edge stopped: {reason}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("edge requires Linux");
        std::process::exit(1);
    }
}
