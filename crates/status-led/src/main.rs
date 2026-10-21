// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
#[cfg(target_os = "linux")]
mod linux;
fn main() {
    #[cfg(target_os = "linux")]
    if linux::run().is_err() {
        eprintln!("status indicator stopped: fixed board/input/output validation failed");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("Linux UFI003 only");
        std::process::exit(1);
    }
}
