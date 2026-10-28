// SPDX-License-Identifier: AGPL-3.0-only
#![forbid(unsafe_code)]
//! `rustsimhub-mdns [DEVICE_NAME_FILE]` answers `<device-name>.local` on wlan0/usb0 only.

#[cfg(target_os = "linux")]
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--build-info"] {
        println!(
            "{{\"component\":\"mdns\",\"target_arch\":\"{}\",\"source_archive_sha256\":{}}}",
            std::env::consts::ARCH,
            option_env!("RUSTSIMHUB_BUILD_SOURCE_SHA256")
                .map(|v| format!("\"{v}\""))
                .unwrap_or_else(|| "null".into())
        );
        return;
    }
    let path = match args.as_slice() {
        [] => "/etc/rustsimhub/device-name".to_owned(),
        [path] => path.clone(),
        _ => std::process::exit(2),
    };
    if let Err(e) = linux::run(&path) {
        eprintln!("mdns stopped: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    std::process::exit(2);
}

#[cfg(target_os = "linux")]
mod linux {
    use rustsimhub_mdns::{announceable, answer, valid_device_name, GROUP, MAX_PACKET, PORT};
    use socket2::{Domain, InterfaceIndexOrAddress, Protocol, SockAddr, SockRef, Socket, Type};
    use std::{
        net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
        time::{Duration, Instant},
    };

    /// Only these interfaces ever carry answers; cellular (wwan) never does.
    const INTERFACES: [&str; 2] = ["wlan0", "usb0"];
    const MAX_REPLIES_PER_SECOND: u32 = 20;

    pub fn run(path: &str) -> Result<(), &'static str> {
        let raw = std::fs::read_to_string(path).map_err(|_| "device name unavailable")?;
        let name = raw.trim();
        if raw.len() > 64 || !valid_device_name(name) {
            return Err("invalid device name");
        }
        let socket =
            Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(|_| "socket")?;
        socket.set_reuse_address(true).map_err(|_| "reuse")?;
        socket
            .bind(&SockAddr::from(SocketAddrV4::new(
                Ipv4Addr::UNSPECIFIED,
                PORT,
            )))
            .map_err(|_| "bind 5353")?;
        socket.set_multicast_ttl_v4(255).map_err(|_| "ttl")?;
        socket.set_multicast_loop_v4(false).map_err(|_| "loop")?;
        let socket: UdpSocket = socket.into();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|_| "timeout")?;
        eprintln!("mdns ready: answering {name}.local on wlan0/usb0 only");
        let options = SockRef::from(&socket);
        let mut joined: Option<Instant> = None;
        let mut window = Instant::now();
        let mut sent = 0u32;
        let mut buffer = [0u8; MAX_PACKET + 1];
        loop {
            // Interfaces are recreated when wlan0 switches between station and AP; rejoin.
            if joined.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
                for interface in INTERFACES {
                    if let Some(index) = index(interface) {
                        let _ = options
                            .join_multicast_v4_n(&GROUP, &InterfaceIndexOrAddress::Index(index));
                    }
                }
                joined = Some(Instant::now());
            }
            let Ok((len, SocketAddr::V4(from))) = socket.recv_from(&mut buffer) else {
                continue; // timeout, transient error or IPv6
            };
            if len > MAX_PACKET {
                continue;
            }
            let Some(local) = facing(*from.ip()) else {
                continue;
            };
            let Some(reply) = answer(&buffer[..len], name, local) else {
                continue;
            };
            if window.elapsed() >= Duration::from_secs(1) {
                window = Instant::now();
                sent = 0;
            }
            if sent >= MAX_REPLIES_PER_SECOND {
                continue;
            }
            sent += 1;
            let destination = if reply.unicast || from.port() != PORT {
                from
            } else {
                if options.set_multicast_if_v4(&local).is_err() {
                    continue;
                }
                SocketAddrV4::new(GROUP, PORT)
            };
            let _ = socket.send_to(&reply.packet, destination);
        }
    }

    /// The address the kernel would use toward `peer`, announced only if it is private and
    /// lies in a subnet routed directly on an allowed interface.
    fn facing(peer: Ipv4Addr) -> Option<Ipv4Addr> {
        let probe = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
        probe.connect((peer, PORT)).ok()?; // UDP connect sends nothing
        let SocketAddr::V4(local) = probe.local_addr().ok()? else {
            return None;
        };
        let address = *local.ip();
        let routes = std::fs::read_to_string("/proc/net/route").ok()?;
        (announceable(address) && on_allowed_interface(&routes, address, peer)).then_some(address)
    }

    /// True when both our address and the peer are inside one directly connected
    /// (gateway 0) subnet of wlan0/usb0 in the kernel's `/proc/net/route` table.
    pub(crate) fn on_allowed_interface(routes: &str, address: Ipv4Addr, peer: Ipv4Addr) -> bool {
        routes.lines().skip(1).any(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            let (Some(device), Some(destination), Some(gateway), Some(mask)) =
                (f.first(), f.get(1), f.get(2), f.get(7))
            else {
                return false;
            };
            let (Some(d), Some(g), Some(m)) = (route(destination), route(gateway), route(mask))
            else {
                return false;
            };
            INTERFACES.contains(device)
                && g == 0
                && m != 0
                && u32::from(address) & m == d & m
                && u32::from(peer) & m == d & m
        })
    }

    /// `/proc/net/route` prints the network-order bytes as a host little-endian integer.
    fn route(hex: &str) -> Option<u32> {
        let v = u32::from_str_radix(hex, 16).ok()?;
        Some(u32::from_be_bytes(v.to_le_bytes()))
    }

    fn index(interface: &str) -> Option<u32> {
        std::fs::read_to_string(format!("/sys/class/net/{interface}/ifindex"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        const ROUTES: &str =
            "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
wlan0\t00000000\t0178A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
wlan0\t0078A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\t0\t0\t0\n\
usb0\t0000370A\t00000000\t0001\t0\t0\t100\tF8FFFFFF\t0\t0\t0\n\
wwan0\t0000000A\t00000000\t0001\t0\t0\t700\t000000FF\t0\t0\t0\n";

        #[test]
        fn only_directly_connected_subnets_of_allowed_interfaces_are_announced() {
            let home = Ipv4Addr::new(192, 168, 120, 167);
            assert!(on_allowed_interface(
                ROUTES,
                home,
                Ipv4Addr::new(192, 168, 120, 5)
            ));
            assert!(on_allowed_interface(
                ROUTES,
                Ipv4Addr::new(10, 55, 0, 2),
                Ipv4Addr::new(10, 55, 0, 1)
            ));
            // A peer only reachable through the default gateway is not on our link.
            assert!(!on_allowed_interface(
                ROUTES,
                home,
                Ipv4Addr::new(8, 8, 8, 8)
            ));
            // Cellular subnets are never answered even if the address matches.
            assert!(!on_allowed_interface(
                ROUTES,
                Ipv4Addr::new(10, 1, 2, 3),
                Ipv4Addr::new(10, 1, 2, 4)
            ));
            assert!(!on_allowed_interface("", home, home));
            assert_eq!(
                route("0078A8C0"),
                Some(u32::from(Ipv4Addr::new(192, 168, 120, 0)))
            );
            assert_eq!(route("zz"), None);
        }
    }
}
