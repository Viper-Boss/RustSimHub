// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Core-owned recovery transport primitives. No daemon startup or business authority.
#[cfg(all(target_os = "linux", feature = "recovery-transport"))]
pub mod recovery_transport;
