// SPDX-License-Identifier: AGPL-3.0-only
use super::{boot, files, Context};
use crate::{egress::Route, Error, Result};
use serde_json::Value;
use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};
pub(super) fn route(c: &Context) -> Result<Route> {
    let v: Value = serde_json::from_slice(&files::read(&c.network_status, 60000)?)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Invalid)?
        .as_secs();
    select(&v, &c.interface, &boot()?, now)
}
fn select(v: &Value, interface: &str, boot: &str, now: u64) -> Result<Route> {
    if v["schema"] != 1
        || v["available"] != true
        || v["boot_id"] != boot
        || !v["sample_unix_seconds"]
            .as_u64()
            .is_some_and(|t| t <= now.saturating_add(2) && now.saturating_sub(t) <= 30)
    {
        return Err(Error::Forbidden);
    }
    let devices = v["devices"]
        .as_array()
        .filter(|v| v.len() <= 16)
        .ok_or(Error::Invalid)?;
    let matches: Vec<_> = devices
        .iter()
        .filter(|v| v["interface"] == interface)
        .collect();
    if matches.len() != 1 {
        return Err(Error::Forbidden);
    }
    let d = matches[0];
    let expected = if interface == "usb0" {
        "ethernet"
    } else {
        "wifi"
    };
    if d["kind"] != expected || !d["state"].as_str().is_some_and(|s| s.starts_with("100 ")) {
        return Err(Error::Forbidden);
    }
    let mut ipv4 = None;
    let mut ipv6 = None;
    for key in ["ipv4", "ipv6"] {
        let addresses = list(d, key)?;
        for value in addresses {
            let value = value.as_str().ok_or(Error::Invalid)?;
            let (address, prefix) = value.split_once('/').ok_or(Error::Invalid)?;
            let ip: IpAddr = address.parse().map_err(|_| Error::Invalid)?;
            let prefix: u8 = prefix.parse().map_err(|_| Error::Invalid)?;
            match ip {
                IpAddr::V4(v) if key == "ipv4" && prefix <= 32 => {
                    if ipv4.is_none() {
                        ipv4 = Some(v)
                    }
                }
                IpAddr::V6(v) if key == "ipv6" && prefix <= 128 => {
                    if ipv6.is_none_or(|old: std::net::Ipv6Addr| {
                        old.is_unicast_link_local() && !v.is_unicast_link_local()
                    }) {
                        ipv6 = Some(v)
                    }
                }
                _ => return Err(Error::Invalid),
            }
        }
    }
    let mut dns_servers = vec![];
    for key in ["dns_ipv4", "dns_ipv6"] {
        for value in list(d, key)? {
            let text = value.as_str().ok_or(Error::Invalid)?;
            let server = match text.parse::<IpAddr>() {
                Ok(ip) => SocketAddr::new(ip, 53),
                #[cfg(feature = "dev-fixtures")]
                Err(_) if interface == "lo" => text.parse().map_err(|_| Error::Invalid)?,
                Err(_) => return Err(Error::Invalid),
            };
            if server.is_ipv4() && ipv4.is_none() || server.is_ipv6() && ipv6.is_none() {
                continue;
            }
            if !dns_servers.contains(&server) && dns_servers.len() < 3 {
                dns_servers.push(server)
            }
        }
    }
    if dns_servers.is_empty() {
        return Err(Error::Forbidden);
    }
    // The explicit interface is validated by Context; no user path is used.
    let index = std::fs::read_to_string(format!("/sys/class/net/{interface}/ifindex"))?;
    let route = Route {
        interface: interface.into(),
        ifindex: index.trim().parse().map_err(|_| Error::Invalid)?,
        ipv4,
        ipv6,
        dns_servers,
    };
    route.validate()?;
    Ok(route)
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    match v.get(key) {
        None => Ok(&[]),
        Some(v) => v
            .as_array()
            .filter(|v| v.len() <= 16)
            .map(|v| v.as_slice())
            .ok_or(Error::Invalid),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        serde_json::json!({"schema":1,"available":true,"boot_id":"synthetic-boot","sample_unix_seconds":100,
        "devices":[{"interface":"lo","kind":"wifi","state":"100 (connected)","ipv4":["127.0.0.1/8"],"ipv6":["::1/128"],"dns_ipv4":["127.0.0.1"],"dns_ipv6":["::1"]}]})
    }
    #[test]
    fn selected_network_is_fresh_unique_typed_and_never_ambient_cellular() {
        let v = sample();
        let r = select(&v, "lo", "synthetic-boot", 110).unwrap();
        assert_eq!(r.ifindex, 1);
        assert_eq!(r.dns_servers.len(), 2);
        assert!(select(&v, "usb0", "synthetic-boot", 110).is_err());
        assert!(select(&v, "lo", "another-boot", 110).is_err());
        assert!(select(&v, "lo", "synthetic-boot", 140).is_err());
        for (key, value) in [
            ("kind", serde_json::json!("cellular")),
            ("state", serde_json::json!("30 (disconnected)")),
            ("ipv4", serde_json::json!(["0.0.0.0/0"])),
            ("dns_ipv4", serde_json::json!(["0.0.0.0"])),
        ] {
            let mut v = sample();
            v["devices"][0][key] = value;
            assert!(select(&v, "lo", "synthetic-boot", 110).is_err());
        }
        let mut v = sample();
        let duplicate = v["devices"][0].clone();
        v["devices"].as_array_mut().unwrap().push(duplicate);
        assert!(select(&v, "lo", "synthetic-boot", 110).is_err());
    }
}
