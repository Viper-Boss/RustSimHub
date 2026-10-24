// SPDX-License-Identifier: AGPL-3.0-only
//! Root-only fixed nft guard. No shell, flush-ruleset, data/APN activation or parameter API.
use crate::{transaction::Guard, Error, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};

// DHCP and HTTPS to this AP only. DNS, SSH, other local services, IPv6 and forwarding
// remain unavailable over the setup AP. Other interfaces and quota tables are untouched.
pub const RULES: &str = r#"create table inet rustsimhub_setup
add chain inet rustsimhub_setup input { type filter hook input priority -290; policy accept; }
add chain inet rustsimhub_setup forward { type filter hook forward priority -290; policy accept; }
add rule inet rustsimhub_setup input iifname "wlan0" meta nfproto ipv4 udp sport 68 udp dport 67 accept
add rule inet rustsimhub_setup input iifname "wlan0" ip saddr 172.31.85.0/24 ip daddr 172.31.85.1 tcp dport 8443 accept
add rule inet rustsimhub_setup input iifname "wlan0" drop
add rule inet rustsimhub_setup forward iifname "wlan0" drop
add rule inet rustsimhub_setup forward oifname "wlan0" drop
"#;
// The trial returns to the AP before reporting. Only DHCP replies are additionally allowed;
// no station HTTPS listener, general established traffic, DNS proxy or forwarding is opened.
pub const TRIAL_RULES: &str = r#"create table inet rustsimhub_setup
add chain inet rustsimhub_setup input { type filter hook input priority -290; policy accept; }
add chain inet rustsimhub_setup forward { type filter hook forward priority -290; policy accept; }
add rule inet rustsimhub_setup input iifname "wlan0" meta nfproto ipv4 udp sport 68 udp dport 67 accept
add rule inet rustsimhub_setup input iifname "wlan0" ip saddr 172.31.85.0/24 ip daddr 172.31.85.1 tcp dport 8443 accept
add rule inet rustsimhub_setup input iifname "wlan0" meta nfproto ipv4 udp sport 67 udp dport 68 accept
add rule inet rustsimhub_setup input iifname "wlan0" drop
add rule inet rustsimhub_setup forward iifname "wlan0" drop
add rule inet rustsimhub_setup forward oifname "wlan0" drop
"#;
fn canonical(bytes: &[u8]) -> Result<[u8; 32]> {
    let mut v: Value = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
    let rows = v
        .get_mut("nftables")
        .and_then(Value::as_array_mut)
        .ok_or(Error::Invalid)?;
    if rows.len() > 32 {
        return Err(Error::Invalid);
    }
    rows.retain(|r| r.get("metainfo").is_none());
    if ![8, 9].contains(&rows.len()) {
        return Err(Error::Changed);
    }
    fn clean(v: &mut Value) {
        match v {
            Value::Object(map) => {
                map.remove("handle");
                for value in map.values_mut() {
                    clean(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    clean(value);
                }
            }
            _ => {}
        }
    }
    clean(&mut v);
    Ok(Sha256::digest(serde_json::to_vec(&v).map_err(|_| Error::Invalid)?).into())
}
async fn command(args: &[&str], input: Option<&str>) -> Result<Vec<u8>> {
    if rustix::process::geteuid().as_raw() != 0 {
        return Err(Error::Denied);
    }
    let mut child = Command::new("/usr/sbin/nft")
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| Error::Io)?;
    let mut stdout = child.stdout.take().ok_or(Error::Io)?;
    let work = async {
        if let Some(s) = input {
            let mut stdin = child.stdin.take().ok_or(Error::Io)?;
            stdin.write_all(s.as_bytes()).await.map_err(|_| Error::Io)?;
            drop(stdin);
        }
        let mut bytes = Vec::new();
        (&mut stdout)
            .take(65537)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| Error::Io)?;
        if bytes.len() > 65536 {
            return Err(Error::Invalid);
        }
        let status = child.wait().await.map_err(|_| Error::Io)?;
        if !status.success() {
            return Err(Error::Io);
        }
        Ok(bytes)
    };
    match timeout(Duration::from_secs(4), work).await {
        Ok(result) => result,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err(Error::Busy)
        }
    }
}
#[derive(Default)]
pub struct NftGuard {
    baseline: Option<[u8; 32]>,
    station_trial: bool,
}
impl NftGuard {
    pub fn new() -> Self {
        Self::default()
    }
    /// Explicit candidate path only; the ordinary AP worker keeps its original guard.
    pub fn for_station_trial() -> Self {
        Self {
            baseline: None,
            station_trial: true,
        }
    }
}
impl Guard for NftGuard {
    async fn install(&mut self) -> Result<()> {
        if self.baseline.is_some() {
            return Err(Error::Denied);
        }
        // Existing tables are not replaced or automatically removed after an unknown outcome.
        command(
            &["-f", "-"],
            Some(if self.station_trial {
                TRIAL_RULES
            } else {
                RULES
            }),
        )
        .await?;
        let expected = canonical(if self.station_trial {
            include_bytes!("setup-guard-trial-v1.json").as_slice()
        } else {
            include_bytes!("setup-guard-v1.json").as_slice()
        })?;
        let actual =
            canonical(&command(&["-j", "list", "table", "inet", "rustsimhub_setup"], None).await?)?;
        if actual != expected {
            return Err(Error::Changed);
        }
        self.baseline = Some(expected);
        self.verify().await
    }
    async fn verify(&self) -> Result<()> {
        let expected = self.baseline.ok_or(Error::Denied)?;
        if canonical(&command(&["-j", "list", "table", "inet", "rustsimhub_setup"], None).await?)?
            != expected
        {
            return Err(Error::Changed);
        }
        Ok(())
    }
    async fn release(&mut self) -> Result<()> {
        self.verify().await?;
        command(&["delete", "table", "inet", "rustsimhub_setup"], None).await?;
        self.baseline = None;
        Ok(())
    }
}
impl crate::station_trial::TrialGuard for NftGuard {
    async fn verify_trial(&self) -> Result<()> {
        if !self.station_trial {
            return Err(Error::Denied);
        }
        self.verify().await
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn isolation_is_fixed_and_verification_ignores_only_kernel_handles_and_metainfo() {
        assert!(RULES.contains(crate::ADDRESS) && RULES.contains(crate::INTERFACE));
        assert!(RULES.contains(&format!("tcp dport {}", crate::HTTPS_PORT)));
        assert!(!RULES.contains("flush") && !RULES.contains("masquerade"));
        assert!(
            RULES.contains("forward iifname \"wlan0\" drop")
                && RULES.contains("forward oifname \"wlan0\" drop")
        );
        assert!(RULES.contains("udp dport 67 accept") && RULES.contains("tcp dport 8443 accept"));
        assert!(!RULES.contains("dport 53") && !RULES.contains("wwan") && !RULES.contains("usb0"));
        let mut rows = vec![serde_json::json!({"metainfo":{"version":"synthetic"}})];
        rows.extend(
            (0..8).map(|n| serde_json::json!({"rule":{"handle":n,"expr":[{"drop":null}]}})),
        );
        let mut v = serde_json::json!({"nftables":rows});
        let before = canonical(&serde_json::to_vec(&v).unwrap()).unwrap();
        v["nftables"][0]["metainfo"]["version"] = "other".into();
        v["nftables"][1]["rule"]["handle"] = 999.into();
        assert_eq!(before, canonical(&serde_json::to_vec(&v).unwrap()).unwrap());
        v["nftables"][1]["rule"]["expr"][0] = serde_json::json!({"accept":null});
        assert_ne!(before, canonical(&serde_json::to_vec(&v).unwrap()).unwrap());
        assert!(canonical(br#"{"nftables":[]}"#).is_err());
    }
    #[test]
    fn guard_reference_rejects_same_size_rule_chain_or_destination_changes() {
        let bytes = include_bytes!("setup-guard-v1.json");
        let expected = canonical(bytes).unwrap();
        for (row, key, value) in [
            (1, "prio", serde_json::json!(0)),
            (2, "policy", serde_json::json!("drop")),
        ] {
            let mut v: Value = serde_json::from_slice(bytes).unwrap();
            v["nftables"][row]["chain"][key] = value;
            assert_ne!(
                expected,
                canonical(&serde_json::to_vec(&v).unwrap()).unwrap()
            );
        }
        let mut v: Value = serde_json::from_slice(bytes).unwrap();
        v["nftables"][4]["rule"]["expr"][2]["match"]["right"] = "192.0.2.1".into();
        assert_ne!(
            expected,
            canonical(&serde_json::to_vec(&v).unwrap()).unwrap()
        );
    }
    #[test]
    fn station_trial_adds_only_dhcp_client_replies_and_keeps_all_ap_and_forwarding_rules() {
        let old: Value = serde_json::from_slice(include_bytes!("setup-guard-v1.json")).unwrap();
        let mut trial: Value =
            serde_json::from_slice(include_bytes!("setup-guard-trial-v1.json")).unwrap();
        let added = trial["nftables"].as_array_mut().unwrap().remove(5);
        assert_eq!(trial, old);
        assert_eq!(added["rule"]["expr"][2]["match"]["right"], 67);
        assert_eq!(added["rule"]["expr"][3]["match"]["right"], 68);
        assert!(TRIAL_RULES.contains("udp sport 67 udp dport 68 accept"));
        assert!(!TRIAL_RULES.contains("ct state") && !TRIAL_RULES.contains("dport 53"));
        assert_ne!(
            canonical(include_bytes!("setup-guard-trial-v1.json")).unwrap(),
            canonical(include_bytes!("setup-guard-v1.json")).unwrap()
        );
    }
}
