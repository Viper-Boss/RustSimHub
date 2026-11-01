// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! Finite protected setup AP transaction. Product authorization and TLS/UI integration
//! must be supplied by the caller; no web endpoint or boot activation is installed here.
use zeroize::Zeroizing;

pub const INTERFACE: &str = "wlan0";
pub const ADDRESS: &str = "192.168.100.1";
pub const PREFIX: u32 = 24;
pub const HTTPS_PORT: u16 = 8443;
/// First-setup rounds may last an hour (user decision 2026-10-29); owner windows stay 5 minutes.
pub const MAX_WINDOW_SECONDS: u32 = 3600;

/// Read-only installation metadata; no material, bus or network access.
pub fn installation_build_info() -> serde_json::Value {
    let source = option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256").filter(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../firmware/system/setup-ap/contract.json"
    ))
    .expect("compiled setup AP contract");
    serde_json::json!({"component":"setup-ap","target_arch":std::env::consts::ARCH,"source_archive_sha256":source,"development_fixtures":false,"setup_ap_installation_contract":contract})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Denied,
    Busy,
    Io,
    Changed,
}
pub type Result<T> = std::result::Result<T, Error>;

// Intentionally no Debug/Serialize: AP passwords never belong in status or public logs.
pub struct Credentials {
    ssid: String,
    passphrase: Zeroizing<String>,
    uuid: String,
    open: bool,
}
impl Credentials {
    /// First-setup variant (user decision 2026-10-28): open AP named `RustSimHub-XXXX` after the
    /// device name, same profile UUID so rollback still finds exactly this profile.
    pub fn open_variant(&self, device_name: &str) -> Result<Self> {
        let tail = device_name
            .strip_prefix("rustsimhub-")
            .filter(|t| {
                t.len() == 4
                    && t.bytes()
                        .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
            })
            .ok_or(Error::Invalid)?;
        Ok(Self {
            ssid: format!("RustSimHub-{}", tail.to_ascii_uppercase()),
            passphrase: Zeroizing::new(String::new()),
            uuid: self.uuid.clone(),
            open: true,
        })
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn generate() -> Result<Self> {
        let mut random = Zeroizing::new([0u8; 46]);
        getrandom::fill(random.as_mut()).map_err(|_| Error::Io)?;
        let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let ssid = format!("RustSimHub-Setup-{}", hex(&random[..6]));
        let passphrase = Zeroizing::new(hex(&random[6..30]));
        let mut uuid_bytes: [u8; 16] = random[30..].try_into().map_err(|_| Error::Invalid)?;
        uuid_bytes[6] = (uuid_bytes[6] & 15) | 64;
        uuid_bytes[8] = (uuid_bytes[8] & 63) | 128;
        let u = hex(&uuid_bytes);
        let uuid = format!(
            "{}-{}-{}-{}-{}",
            &u[..8],
            &u[8..12],
            &u[12..16],
            &u[16..20],
            &u[20..]
        );
        Self::from_private_parts(ssid, passphrase, uuid)
    }
    /// Trusted installer input only, never an unauthenticated HTTP settings dictionary.
    pub fn from_private_parts(
        ssid: String,
        passphrase: Zeroizing<String>,
        uuid: String,
    ) -> Result<Self> {
        let valid_uuid = uuid.len() == 36
            && uuid.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
                }
            });
        if !ssid.starts_with("RustSimHub-Setup-")
            || ssid.len() > 32
            || ssid.len() <= "RustSimHub-Setup-".len()
            || ssid.chars().any(char::is_control)
            || passphrase.len() != 48
            || !passphrase
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || !valid_uuid
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            ssid,
            passphrase,
            uuid,
            open: false,
        })
    }
    pub fn ssid(&self) -> &str {
        &self.ssid
    }
    pub fn uuid(&self) -> &str {
        &self.uuid
    }
    /// Deliver through trusted USB/console or a per-device label, never the public AP status.
    pub fn expose_passphrase_for_trusted_delivery(&self) -> &str {
        &self.passphrase
    }
}

#[cfg(target_os = "linux")]
pub mod ap_rescan;
#[cfg(target_os = "linux")]
pub mod channel;
#[cfg(target_os = "linux")]
pub mod executor;
#[cfg(target_os = "linux")]
pub mod guard;
#[cfg(target_os = "linux")]
pub mod nm;
#[cfg(target_os = "linux")]
pub mod station_commit;
#[cfg(target_os = "linux")]
pub mod station_trial;
#[cfg(target_os = "linux")]
pub mod transaction;

#[cfg(test)]
mod tests {
    #[test]
    fn open_variant_is_named_after_the_device_and_keeps_the_profile_uuid() {
        let c = super::Credentials::generate().unwrap();
        assert!(!c.is_open());
        let o = c.open_variant("rustsimhub-0087").unwrap();
        assert!(o.is_open());
        assert_eq!(o.ssid(), "RustSimHub-0087");
        assert_eq!(o.uuid(), c.uuid());
        assert!(o.expose_passphrase_for_trusted_delivery().is_empty());
        for bad in [
            "",
            "rustsimhub-87",
            "rustsimhub-00870",
            "RustSimHub-0087",
            "rustsimhub-00 7",
            "other-0087",
        ] {
            assert!(c.open_variant(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn installation_metadata_matches_the_finite_compiled_contract() {
        let info = super::installation_build_info();
        let contract = &info["setup_ap_installation_contract"];
        assert_eq!(info["component"], "setup-ap");
        assert_eq!(info["development_fixtures"], false);
        assert_eq!(
            contract["maximum_window_seconds"],
            super::MAX_WINDOW_SECONDS
        );
        assert_eq!(
            contract["origin"],
            format!("https://{}:{}", super::ADDRESS, super::HTTPS_PORT)
        );
        assert_eq!(contract["root_mode"], "--serve-configurable");
    }
    use super::*;
    #[test]
    fn per_device_material_is_random_and_rejects_weak_or_injectable_settings() {
        let a = Credentials::generate().unwrap();
        let b = Credentials::generate().unwrap();
        assert_ne!(a.ssid(), b.ssid());
        assert_ne!(a.uuid(), b.uuid());
        assert_ne!(
            a.expose_passphrase_for_trusted_delivery(),
            b.expose_passphrase_for_trusted_delivery()
        );
        assert_eq!(a.expose_passphrase_for_trusted_delivery().len(), 48);
        for name in ["open".into(), "RustSimHub-Setup-\n".into(), "x".repeat(33)] {
            assert!(Credentials::from_private_parts(
                name,
                Zeroizing::new("a".repeat(48)),
                a.uuid().into()
            )
            .is_err());
        }
        assert!(Credentials::from_private_parts(
            a.ssid().into(),
            Zeroizing::new("password".into()),
            a.uuid().into()
        )
        .is_err());
    }
}
