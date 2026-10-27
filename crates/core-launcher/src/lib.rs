// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! A non-root, finite startup adapter. Fixed program/arguments only; no API,
//! network, policy mutation, account creation or service control.
use serde::Deserialize;
use serde_json::Value;
mod compatibility;
pub use compatibility::{check_upgrade, UpgradeReport};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::launch;
#[cfg(not(target_os = "linux"))]
pub fn launch() -> Result<(), Error> {
    Err(Error)
}

pub const CORE: &str = "/opt/rustsimhub/selected/rustsimhub-core";
pub const CONTRACT: &str = "/etc/rustsimhub/setup-ap-contract.json";
// Verified pre-capability release profiles, not device identities. Unknown old
// metadata is rejected rather than guessing which command it understands.
const FACTORY16: &str = "9bbc71d42fb5b10d0a2346579dccc56f6d732828689c2879fed48b1b796a5d39";
const FORWARDING48: &str = "d641489c23d019d2c7011f8709c51d2890f0fd3aab4d75ecbb42e0d1158f9cae";
const APP49: &str = "f0226d2b32ec69a7ab1a062516a75dfbe7229b403d8f72210c7c8e2a099896b6";

#[derive(Debug, PartialEq, Eq)]
pub struct Error;
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Device,
    Forwarding,
    SetupAp,
}
pub fn installation_contract() -> Value {
    serde_json::from_str(include_str!(
        "../../../firmware/system/setup-ap/contract.json"
    ))
    .expect("reviewed build-time installation contract")
}
#[derive(Deserialize)]
struct Metadata {
    component: Option<String>,
    target_arch: Option<String>,
    source_archive_sha256: String,
    database_schema: u32,
    development_fixtures: bool,
    service_launch: Option<Capabilities>,
    setup_ap_installation_contract: Option<Contract>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Capabilities {
    format: u16,
    modes: Vec<String>,
}
// Typed parsing rejects duplicate keys, unknown fields and coercions. A Value
// alone would silently accept the last occurrence of a duplicated key.
#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Contract {
    format: u16,
    auth_protocol: u16,
    database_schema: u16,
    application_format: u16,
    parent_channel: u16,
    station_trial_channel: u16,
    station_commit_channel: u16,
    material_format: u16,
    owner_adoption: u16,
    core_mode: String,
    root_mode: String,
    origin: String,
    maximum_window_seconds: u16,
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
/// Explicit new capabilities plus exactly three verified historical profiles.
/// A missing/unusable AP contract disables AP, never forwarding or phone/SMS.
pub fn select(metadata: &[u8], installed: Option<&[u8]>) -> Result<Mode, Error> {
    select_for_arch(metadata, installed, std::env::consts::ARCH)
}

fn select_for_arch(metadata: &[u8], installed: Option<&[u8]>, arch: &str) -> Result<Mode, Error> {
    if metadata.len() > 16384 {
        return Err(Error);
    }
    let meta: Metadata = serde_json::from_slice(metadata).map_err(|_| Error)?;
    if meta.database_schema != 6
        || meta.development_fixtures
        || !digest(&meta.source_archive_sha256)
        || meta.component.as_deref().is_some_and(|v| v != "core")
        || meta.target_arch.as_deref().is_some_and(|v| v != arch)
    {
        return Err(Error);
    }
    let (forwarding, ap) = if let Some(caps) = meta.service_launch {
        if meta.component.as_deref() != Some("core")
            || meta.target_arch.as_deref() != Some(arch)
            || caps.format != 1
            || caps.modes.is_empty()
            || caps.modes.len() > 3
            || !caps.modes.iter().any(|m| m == "device")
            || caps.modes.iter().enumerate().any(|(i, m)| {
                !["device", "forwarding_managed", "setup_ap"].contains(&m.as_str())
                    || caps.modes[..i].contains(m)
            })
        {
            return Err(Error);
        }
        let forwarding = caps.modes.iter().any(|m| m == "forwarding_managed");
        let ap = caps.modes.iter().any(|m| m == "setup_ap");
        if ap && !forwarding {
            return Err(Error);
        }
        (forwarding, ap)
    } else {
        match meta.source_archive_sha256.as_str() {
            FACTORY16 => (false, false),
            FORWARDING48 => (true, false),
            APP49 => (true, true),
            _ => return Err(Error),
        }
    };
    let expected = installation_contract();
    let expected_typed: Contract = serde_json::from_value(expected.clone()).map_err(|_| Error)?;
    let contract_ok = installed
        .filter(|v| v.len() <= 4096)
        .and_then(|v| serde_json::from_slice::<Contract>(v).ok())
        .is_some_and(|v| v == expected_typed);
    if ap && contract_ok && meta.setup_ap_installation_contract.as_ref() == Some(&expected_typed) {
        Ok(Mode::SetupAp)
    } else if forwarding {
        Ok(Mode::Forwarding)
    } else {
        Ok(Mode::Device)
    }
}
pub fn arguments(mode: Mode) -> Vec<&'static str> {
    let mut args = vec![
        match mode {
            Mode::Device => "--serve-device",
            Mode::Forwarding => "--serve-device-forwarding-managed",
            Mode::SetupAp => "--serve-device-forwarding-managed-setup-ap",
        },
        "/run/rustsimhub/core/core.sock",
        "985",
        "/var/lib/rustsimhub-core/state",
        "/var/lib/rustsimhub-core/identity",
        "/run/rustsimhub/core/media.sock",
        "984",
        "/var/lib/rustsimhub-core/archive",
        "/var/lib/rustsimhub-core/access",
        "/var/lib/rustsimhub-core/compat",
        "/var/lib/rustsimhub-core/journal",
        "/var/lib/rustsimhub-core/setup",
    ];
    if mode != Mode::Device {
        args.push("983");
    }
    if mode == Mode::SetupAp {
        args.extend([
            "/run/rustsimhub-setup-ap-core",
            "/run/rustsimhub-setup-ap-root",
            "/var/lib/rustsimhub-core/setup-ap-journal",
        ]);
    }
    args
}
#[cfg(test)]
mod tests;
