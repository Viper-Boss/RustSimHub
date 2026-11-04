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
    #[serde(rename = "product-recovery-health-v1")]
    RecoveryHealth,
}
impl Profile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy-v1",
            Self::Product => "product-v1",
            Self::Email => "product-email-v1",
            Self::Recovery => "product-recovery-v1",
            Self::EmailRecovery => "product-email-recovery-v1",
            Self::RecoveryHealth => "product-recovery-health-v1",
        }
    }
    pub const fn product(self) -> bool {
        !matches!(self, Self::Legacy)
    }
    pub const fn recovery(self) -> bool {
        matches!(
            self,
            Self::Recovery | Self::EmailRecovery | Self::RecoveryHealth
        )
    }
    pub fn targets(self) -> &'static [TargetSpec] {
        match self {
            Self::Legacy => &LEGACY,
            Self::Product | Self::Email => &PRODUCT,
            Self::Recovery | Self::EmailRecovery => &RECOVERY,
            Self::RecoveryHealth => &RECOVERY_HEALTH,
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
    HealthDropin,
    HealthTmpfiles,
    HealthService,
    HealthTimer,
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
            Self::HealthDropin => "health_dropin",
            Self::HealthTmpfiles => "health_tmpfiles",
            Self::HealthService => "health_service",
            Self::HealthTimer => "health_timer",
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
            Self::HealthDropin => "health_dropin.payload",
            Self::HealthTmpfiles => "health_tmpfiles.payload",
            Self::HealthService => "health_service.payload",
            Self::HealthTimer => "health_timer.payload",
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
const HEALTH_DROP: TargetSpec = spec(
    TargetId::HealthDropin,
    "/etc/systemd/system/rustsimhub-core.service.d/55-product-health.conf",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const HEALTH_TMP: TargetSpec = spec(
    TargetId::HealthTmpfiles,
    "/etc/tmpfiles.d/rustsimhub-update-health.conf",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const HEALTH_SERVICE: TargetSpec = spec(
    TargetId::HealthService,
    "/etc/systemd/system/rustsimhub-update-health.service",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const HEALTH_TIMER: TargetSpec = spec(
    TargetId::HealthTimer,
    "/etc/systemd/system/rustsimhub-update-health.timer",
    0o644,
    false,
    MaterialPhase::StageFiles,
);
const RECOVERY_HEALTH: [TargetSpec; 14] = [
    PROFILE,
    DROPIN,
    LAUNCHER,
    ROOT,
    SDK,
    NKN_UNIT,
    TMP,
    BLE,
    BLE_UNIT,
    BUS,
    HEALTH_DROP,
    HEALTH_TMP,
    HEALTH_SERVICE,
    HEALTH_TIMER,
];
const ALL: [TargetSpec; 14] = RECOVERY_HEALTH;
