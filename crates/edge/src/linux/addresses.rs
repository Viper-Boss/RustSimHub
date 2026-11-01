// SPDX-License-Identifier: AGPL-3.0-only
//! Current private IPv4 addresses of wlan0, read from /proc without ioctl or unsafe code.
//! Used only when the Edge is served under the fixed `.local` name, so a Wi-Fi change
//! (reconfiguration or a new DHCP lease) does not leave the daily page unreachable.
//! The same daily Edge also follows the fixed usb0 address, so a USB tool on the owner's
//! computer reaches it with the `.local` Host/Origin over the cable.
use std::net::{Ipv4Addr, SocketAddr};

const INTERFACE: &str = "wlan0";
const MAX_ADDRESSES: usize = 2;
/// Fixed device side of the USB link (shipped `usb.nmconnection`, 10.55.0.2/29). Never DHCP.
pub(super) const USB0: Ipv4Addr = Ipv4Addr::new(10, 55, 0, 2);

/// Listener set: the configured addresses first, then, only under the `.local` name, the
/// fixed usb0 address and the current wlan0 addresses. No duplicates; an address that is not
/// assigned yet is skipped at bind time and retried.
pub(super) fn followed(
    configured: &[SocketAddr],
    port: u16,
    follow: bool,
    wlan: &[Ipv4Addr],
) -> Vec<SocketAddr> {
    let mut v = configured.to_vec();
    if follow {
        for ip in std::iter::once(&USB0).chain(wlan) {
            let a = SocketAddr::new((*ip).into(), port);
            if !v.contains(&a) {
                v.push(a);
            }
        }
    }
    v
}
/// The setup hotspot's own address belongs to the setup edge only. Field test 2026-10-29: the
/// daily edge bound it first while the AP was up, the setup edge then exited and Root closed
/// every first-setup round.
const SETUP_AP: Ipv4Addr = Ipv4Addr::new(192, 168, 100, 1);

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
                    && a != SETUP_AP
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
    fn usb0_is_followed_only_under_the_local_name_and_never_duplicated() {
        let configured: Vec<SocketAddr> = vec!["192.168.120.167:8443".parse().unwrap()];
        let wlan = [Ipv4Addr::new(192, 168, 0, 42)];
        assert_eq!(
            followed(&configured, 8443, true, &wlan),
            [
                configured[0],
                "10.55.0.2:8443".parse().unwrap(),
                "192.168.0.42:8443".parse().unwrap()
            ]
        );
        // An IP-origin Edge (the setup Edge) keeps exactly its configured listeners.
        assert_eq!(followed(&configured, 8443, false, &wlan), configured);
        // Already configured (e.g. an explicit additional listen IP): listed once.
        let with_usb: Vec<SocketAddr> = vec![configured[0], "10.55.0.2:8443".parse().unwrap()];
        assert_eq!(followed(&with_usb, 8443, true, &[]), with_usb);
    }

    #[test]
    fn the_setup_hotspot_address_is_never_followed() {
        let routes = "Iface	Destination	Gateway	Flags	RefCnt	Use	Metric	Mask
wlan0	0064A8C0	00000000	0001	0	0	0	00FFFFFF
";
        let trie = "Main:
  |-- 192.168.100.1
     /32 host LOCAL
";
        assert!(current(trie, routes).is_empty());
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
