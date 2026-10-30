// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Bounded, opaque GATT transport for the frozen T5 offline contract.
//!
//! This crate does not authenticate a browser, decrypt commands, authorize Core
//! operations, or start a service. Backend integration must supply
//! those authorities. Framing identifiers are never security nonces or proofs.
//! Default code is runtime-free. Explicit bluez-control uses an injected bus
//! connection for fixed RPCs; it does not export GATT objects or connect a bus.

#[cfg(all(target_os = "linux", feature = "bluez-advertisement"))]
pub mod bluez_advertisement;
#[cfg(all(target_os = "linux", feature = "bluez-backend"))]
pub mod bluez_backend;
#[cfg(all(target_os = "linux", feature = "bluez-control"))]
pub mod bluez_control;
#[cfg(all(target_os = "linux", feature = "bluez-gatt"))]
pub mod bluez_gatt;
#[cfg(all(target_os = "linux", feature = "bluez-peer"))]
pub mod bluez_peer;
pub mod framing;
pub mod lifecycle;
pub mod manifest;
pub mod ports;
pub mod transport;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    Protocol,
    Deadline,
    Exhausted,
    Busy,
    StaleContext,
    Unavailable,
    Backend,
    Unknown,
}

#[cfg(test)]
mod tests;
