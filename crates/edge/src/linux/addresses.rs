// SPDX-License-Identifier: AGPL-3.0-only
//! Current private IPv4 addresses of wlan0, read from /proc without ioctl or unsafe code.
//! Used only when the Edge is served under the fixed `.local` name, so a Wi-Fi change
//! (reconfiguration or a new DHCP lease) does not leave the daily page unreachable.
use std::net::Ipv4Addr;

const INTERFACE: &str = "wlan0";
const MAX_ADDRESSES: usize = 2;

pub(super) fn wlan0() -> Vec<Ipv4Addr> {
    match (
        std::fs::read_to_string("/proc/net/fib_trie"),
        std::fs::read_to_string("/proc/net/route"),
    ) {
        (Ok(trie), Ok(routes)) => current(&trie, &routes),
        _ => Vec::new(),
    }
}

/// Local (`/32 host LOCAL`) addresses inside a subnet routed directly on wlan0.
pub(super) fn current(trie: &str, routes: &str) -> Vec<Ipv4Addr> {
    let subnets: Vec<(u32, u32)> = routes
        .lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let (device, destination, gateway, mask) =
                (f.first()?, f.get(1)?, f.get(2)?, f.get(7)?);
            let (d, g, m) = (route(destination)?, route(gateway)?, route(mask)?);
            (*device == INTERFACE && g == 0 && m != 0).then_some((d, m))
        })
        .collect();
    let mut found = Vec::new();
    let mut last: Option<Ipv4Addr> = None;
    for line in trie.lines() {
        let line = line.trim();
        if let Some(address) = line.strip_prefix("|-- ") {
            last = address.parse().ok();
        } else if line == "/32 host LOCAL" {
            if let Some(a) = last.take() {
                let v = u32::from(a);
                if a.is_private()
                    && subnets.iter().any(|(d, m)| v & m == d & m)
                    && !found.contains(&a)
                    && found.len() < MAX_ADDRESSES
                {
                    found.push(a);
                }
            }
        }
    }
    found
}

/// `/proc/net/route` prints the network-order bytes as a host little-endian integer.
fn route(hex: &str) -> Option<u32> {
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(u32::from_be_bytes(v.to_le_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const ROUTES: &str =
        "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
wlan0\t00000000\t0178A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
wlan0\t0078A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n\
usb0\t0000370A\t00000000\t0001\t0\t0\t100\tF8FFFFFF\t0\t0\t0\n";
    const TRIE: &str = "Main:\n  +-- 0.0.0.0/0 3 0 5\n     |-- 0.0.0.0\n        /0 universe UNICAST\n     +-- 10.55.0.0/29 2 0 2\n        |-- 10.55.0.0\n           /29 link UNICAST\n        |-- 10.55.0.2\n           /32 host LOCAL\n     +-- 192.168.120.0/24 2 0 2\n        |-- 192.168.120.0\n           /24 link UNICAST\n        |-- 192.168.120.167\n           /32 host LOCAL\n        |-- 192.168.120.255\n           /32 link BROADCAST\nLocal:\n  +-- 127.0.0.0/8 2 0 2\n     |-- 127.0.0.1\n        /32 host LOCAL\n     |-- 192.168.120.167\n        /32 host LOCAL\n";

    #[test]
    fn only_local_private_addresses_on_wlan0_subnets_are_listed_once() {
        assert_eq!(current(TRIE, ROUTES), [Ipv4Addr::new(192, 168, 120, 167)]);
        // usb0, loopback and broadcast entries never appear; no wlan0 route means nothing.
        let usb_only = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n\
usb0\t0000370A\t00000000\t0001\t0\t0\t100\tF8FFFFFF\n";
        assert!(current(TRIE, usb_only).is_empty());
        assert!(current("", ROUTES).is_empty());
        assert!(current(TRIE, "").is_empty());
    }

    #[test]
    fn a_new_wifi_subnet_is_picked_up_and_public_addresses_are_ignored() {
        let routes = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n\
wlan0\t0000A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\n\
wlan0\t00000008\t00000000\t0001\t0\t0\t600\t000000FF\n";
        let trie =
            "Main:\n  |-- 192.168.0.42\n     /32 host LOCAL\n  |-- 8.0.0.9\n     /32 host LOCAL\n";
        assert_eq!(current(trie, routes), [Ipv4Addr::new(192, 168, 0, 42)]);
    }
}
