// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Core-owned recovery transport and typed admission. No daemon startup.
#[cfg(all(target_os = "linux", feature = "root-provider-candidate"))]
pub mod nkn_root;
#[cfg(all(target_os = "linux", feature = "usage-transaction-candidate"))]
pub mod nkn_statistics;
#[cfg(all(target_os = "linux", feature = "recovery-execution"))]
pub mod recovery_body;
#[cfg(all(
    target_os = "linux",
    feature = "recovery-transport",
    feature = "recovery-grants"
))]
pub mod recovery_channel;
#[cfg(all(target_os = "linux", feature = "recovery-transport"))]
pub mod recovery_transport;

// One compiled authority shared by the binary and recovery library entry.
#[cfg(target_os = "linux")]
pub mod coordination;
#[cfg(target_os = "linux")]
pub mod network_trial;

#[cfg(target_os = "linux")]
pub mod update_health;

// Pure mode metadata is available before any activation or database open.
#[cfg(all(target_os = "linux", feature = "recovery-execution"))]
#[path = "../../../tools/product-upgrade/recovery_child.rs"]
pub mod product_recovery_child;
pub mod runtime_compatibility;
