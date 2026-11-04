// SPDX-License-Identifier: AGPL-3.0-only
//! Versioned finite product-maintenance write set, separate from application/AP.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Profile {
    #[serde(rename = "legacy-v1")]
    Legacy,
    #[serde(rename = "product-v1")]
    Product,
    #[serde(rename = "product-email-v1")]
    Email,
    #[serde(rename = "product-recovery-v1")]
    Recovery,
    #[serde(rename = "product-email-recovery-v1")]
    EmailRecovery,
}
impl Profile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy-v1",
            Self::Product => "product-v1",
            Self::Email => "product-email-v1",
            Self::Recovery => "product-recovery-v1",
            Self::EmailRecovery => "product-email-recovery-v1",
        }
    }
    pub const fn product(self) -> bool {
        !matches!(self, Self::Legacy)
    }
    pub const fn recovery(self) -> bool {
        matches!(self, Self::Recovery | Self::EmailRecovery)
    }
    pub fn targets(self) -> &'static [TargetSpec] {
        match self {
            Self::Legacy => &LEGACY,
            Self::Product | Self::Email => &PRODUCT,
            Self::Recovery | Self::EmailRecovery => &RECOVERY,
        }
    }
    pub fn required_roles(self) -> &'static [RoleId] {
        if self.product() {
            &PRODUCT_ROLES
        } else {
            &BASE_ROLES
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoleId {
    Core,
    Edge,
    Media,
    Forwarder,
    Nkn,
    Ble,
}
impl RoleId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::Edge => "edge",
            Self::Media => "media",
            Self::Forwarder => "forwarder",
            Self::Nkn => "nkn",
            Self::Ble => "ble",
        }
    }
}
const BASE_ROLES: [RoleId; 4] = [RoleId::Core, RoleId::Edge, RoleId::Media, RoleId::Forwarder];
const PRODUCT_ROLES: [RoleId; 6] = [
    RoleId::Core,
    RoleId::Edge,
    RoleId::Media,
    RoleId::Forwarder,
    RoleId::Nkn,
    RoleId::Ble,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialPhase {
    StageFiles,
    ProfileSwitch,
    LauncherLast,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetId {
    ProfileConfig,
    CoreDropin,
    Launcher,
    NknRoot,
    NknSdk,
    NknUnit,
    ProductTmpfiles,
    BleHost,
    BleUnit,
    BlePolicy,
}
impl TargetId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProfileConfig => "profile_config",
            Self::CoreDropin => "core_dropin",
            Self::Launcher => "launcher",
            Self::NknRoot => "nkn_root",
            Self::NknSdk => "nkn_sdk",
            Self::NknUnit => "nkn_unit",
            Self::ProductTmpfiles => "product_tmpfiles",
            Self::BleHost => "ble_host",
            Self::BleUnit => "ble_unit",
            Self::BlePolicy => "ble_policy",
        }
    }
    pub const fn payload_name(self) -> &'static str {
        match self {
            Self::ProfileConfig => "profile_config.payload",
            Self::CoreDropin => "core_dropin.payload",
            Self::Launcher => "launcher.payload",
            Self::NknRoot => "nkn_root.payload",
            Self::NknSdk => "nkn_sdk.payload",
            Self::NknUnit => "nkn_unit.payload",
            Self::ProductTmpfiles => "product_tmpfiles.payload",
            Self::BleHost => "ble_host.payload",
            Self::BleUnit => "ble_unit.payload",
            Self::BlePolicy => "ble_policy.payload",
        }
    }
    pub fn spec(self) -> &'static TargetSpec {
        ALL.iter()
            .find(|s| s.id == self)
            .expect("exhaustive fixed target table")
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TargetSpec {
    pub id: TargetId,
    /// Fixed absolute production target. No package-selected paths.
    pub path: &'static str,
    pub mode: u32,
    pub executable: bool,
    pub phase: MaterialPhase,
}
const fn spec(
    id: TargetId,
    path: &'static str,
    mode: u32,
    executable: bool,
    phase: MaterialPhase,
) -> TargetSpec {
    TargetSpec {
        id,
        path,
        mode,
        executable,
        phase,
    }
}
const PROFILE: TargetSpec = spec(
    TargetId::ProfileConfig,
    "/etc/rustsimhub/product-profile.json",
    0o644,
    false,
    MaterialPhase::ProfileSwitch,
);
const DROPIN: TargetSpec = spec(
    TargetId::CoreDropin,
    "/etc/systemd/system/rustsimhub-core.service.d/50-product-profile.conf",
    0o644,
    false,
    MaterialPhase::ProfileSwitch,
);
const LAUNCHER: TargetSpec = spec(
    TargetId::Launcher,
    "/usr/libexec/rustsimhub/rustsimhub-core-launcher",
    0o555,
    true,
    MaterialPhase::LauncherLast,
);
const ROOT: TargetSpec = spec(
    TargetId::NknRoot,
    "/usr/libexec/rustsimhub/product/nkn-root",
    0o555,
    true,
    MaterialPhase::StageFiles,
);
const SDK: TargetSpec = spec(
    TargetId::NknSdk,
    "/usr/libexec/rustsimhub/product/nkn-sdk-engine",
    0o555,
    true,
    MaterialPhase::StageFiles,
);
const NKN_UNIT: TargetSpec = spec(
    TargetId::NknUnit,
    "/etc/systemd/system/rustsimhub-nkn-root.service",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const TMP: TargetSpec = spec(
    TargetId::ProductTmpfiles,
    "/etc/tmpfiles.d/rustsimhub-product.conf",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const BLE: TargetSpec = spec(
    TargetId::BleHost,
    "/usr/libexec/rustsimhub/product/rustsimhub-ble-recovery",
    0o555,
    true,
    MaterialPhase::StageFiles,
);
const BLE_UNIT: TargetSpec = spec(
    TargetId::BleUnit,
    "/etc/systemd/system/rustsimhub-ble-recovery.service",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const BUS: TargetSpec = spec(
    TargetId::BlePolicy,
    "/etc/dbus-1/system.d/rustsimhub-ble-product.conf",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const LEGACY: [TargetSpec; 3] = [PROFILE, DROPIN, LAUNCHER];
const PRODUCT: [TargetSpec; 7] = [PROFILE, DROPIN, LAUNCHER, ROOT, SDK, NKN_UNIT, TMP];
const RECOVERY: [TargetSpec; 10] = [
    PROFILE, DROPIN, LAUNCHER, ROOT, SDK, NKN_UNIT, TMP, BLE, BLE_UNIT, BUS,
];
const ALL: [TargetSpec; 10] = RECOVERY;
