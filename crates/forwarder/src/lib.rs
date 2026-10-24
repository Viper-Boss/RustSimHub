// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Native forwarding rules, durable outbox and explicitly invoked HTTPS.
//! Construction starts no thread, service, socket or modem action. Core must authenticate a
//! current recent Owner before configuring it, and the live Owner/SIM fence
//! must also be checked by the eventual delivery worker before writing bytes.
//! A Binding identifies that authority; it is deliberately NOT an auth proof.
mod model;
pub use model::*;
#[cfg(target_os = "linux")]
mod authority;
#[cfg(target_os = "linux")]
mod queue;
#[cfg(target_os = "linux")]
pub use queue::*;
#[cfg(target_os = "linux")]
mod delivery;
#[cfg(target_os = "linux")]
pub use delivery::*;
#[cfg(target_os = "linux")]
pub mod egress;
#[cfg(target_os = "linux")]
pub mod worker;
#[cfg(target_os = "linux")]
pub mod worker_ipc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Forbidden,
    Disabled,
    Stale,
    Conflict,
    Full,
    Busy,
    Storage,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "forwarder: {self:?}")
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Self::Storage
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self::Invalid
    }
}
#[cfg(target_os = "linux")]
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
}
