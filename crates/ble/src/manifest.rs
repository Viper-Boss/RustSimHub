// SPDX-License-Identifier: AGPL-3.0-only
//! Fixed object tree; backend implementations receive no caller-selected paths.

pub const ROOT: &str = "/org/rustsimhub/recovery/v1";
pub const SERVICE_PATH: &str = "/org/rustsimhub/recovery/v1/service0";
pub const ADVERTISEMENT_PATH: &str = "/org/rustsimhub/recovery/v1/advertisement0";
pub const SERVICE_UUID: &str = "002c85a2-c6d6-411c-b266-9e99fb4cdd0a";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Characteristic {
    DeviceInfo,
    AuthExchange,
    Command,
    Result,
}

impl Characteristic {
    pub const ALL: [Self; 4] = [
        Self::DeviceInfo,
        Self::AuthExchange,
        Self::Command,
        Self::Result,
    ];

    pub const fn uuid(self) -> &'static str {
        match self {
            Self::DeviceInfo => "4a13c416-2d30-46e1-a8fe-8cc00fa82ce7",
            Self::AuthExchange => "f443c973-2d89-4991-830f-634bd75dc3cd",
            Self::Command => "dc9899ec-c8f0-43cf-9202-ed5cdebd4562",
            Self::Result => "4faaacb4-d4aa-44df-afdd-5e474c9e7f86",
        }
    }

    pub const fn path(self) -> &'static str {
        match self {
            Self::DeviceInfo => "/org/rustsimhub/recovery/v1/service0/char0",
            Self::AuthExchange => "/org/rustsimhub/recovery/v1/service0/char1",
            Self::Command => "/org/rustsimhub/recovery/v1/service0/char2",
            Self::Result => "/org/rustsimhub/recovery/v1/service0/char3",
        }
    }

    pub const fn flags(self) -> &'static [&'static str] {
        match self {
            Self::DeviceInfo => &["read"],
            Self::AuthExchange => &["write", "notify"],
            Self::Command => &["write"],
            Self::Result => &["read", "notify"],
        }
    }
}

/// Zero-sized manifest; a payload cannot extend the registered subtree.
#[derive(Clone, Copy, Debug)]
pub struct FixedManifest;

/// The backend must use Type=peripheral and only SERVICE_UUID; no local name,
/// adapter Power/Pair/Trusted mutation, additional services, or arbitrary data.
#[derive(Clone, Copy, Debug)]
pub struct FixedAdvertisement;

pub const DEVICE_INFO: &[u8] =
    br#"{"protocol":1,"product":"RustSimHub","preapproval_required":true,"message_bytes":2048}"#;
