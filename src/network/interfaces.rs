// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkError {
    NoUsableInterface,
    EnumerationFailed(String),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoUsableInterface => {
                write!(f, "No usable local network interface was found")
            }
            Self::EnumerationFailed(msg) => {
                write!(f, "Failed to enumerate network interfaces: {}", msg)
            }
        }
    }
}

impl std::error::Error for NetworkError {}

/// A simplified representation of an interface address for evaluation.
#[derive(Debug, Clone)]
pub struct InterfaceCandidate {
    pub name: String,
    pub ip: IpAddr,
    pub is_loopback: bool,
}

/// Determines if an IPv4 address is an RFC 1918 private LAN address.
pub fn is_private_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    if octets[0] == 10 {
        return true;
    }
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return true;
    }
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }
    false
}

/// Determines if an IPv4 address is link-local (169.254.0.0/16).
pub fn is_link_local_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 169 && octets[1] == 254
}

/// Checks if an interface name is a known virtual/container bridge.
pub fn is_known_virtual_interface(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("docker")
        || lower.starts_with("podman")
        || lower.starts_with("veth")
        || lower.starts_with("virbr")
        || lower.starts_with("cni")
        || lower.starts_with("br-")
}

/// Ranks a candidate interface. Higher score indicates a better candidate for LAN sharing.
/// Returns None if the candidate is completely unsuitable.
fn rank_candidate(candidate: &InterfaceCandidate) -> Option<u32> {
    if candidate.is_loopback {
        return None;
    }

    let ipv4 = match candidate.ip {
        IpAddr::V4(v4) => v4,
        IpAddr::V6(_) => return None,
    };

    if ipv4.is_loopback() || ipv4.is_unspecified() || ipv4.is_broadcast() {
        return None;
    }

    if is_link_local_ipv4(&ipv4) {
        return None;
    }

    let is_virtual = is_known_virtual_interface(&candidate.name);
    let is_private = is_private_ipv4(&ipv4);

    let mut score: u32 = 100;

    if is_private {
        score += 200;
    }

    if is_virtual {
        score = score.saturating_sub(150);
    }

    Some(score)
}

/// Selects the best LAN IP address from a list of interface candidates.
pub fn select_best_lan_address(
    candidates: &[InterfaceCandidate],
) -> Result<Ipv4Addr, NetworkError> {
    let mut ranked: Vec<(u32, Ipv4Addr)> = candidates
        .iter()
        .filter_map(|c| {
            let score = rank_candidate(c)?;
            match c.ip {
                IpAddr::V4(v4) => Some((score, v4)),
                _ => None,
            }
        })
        .collect();

    ranked.sort_by_key(|a| std::cmp::Reverse(a.0));

    ranked
        .first()
        .map(|(_, ip)| *ip)
        .ok_or(NetworkError::NoUsableInterface)
}

/// Queries the local system network interfaces and returns the best LAN IPv4 address.
pub fn find_local_lan_ip() -> Result<Ipv4Addr, NetworkError> {
    let ifaddrs =
        if_addrs::get_if_addrs().map_err(|e| NetworkError::EnumerationFailed(e.to_string()))?;

    let candidates: Vec<InterfaceCandidate> = ifaddrs
        .into_iter()
        .map(|iface| {
            let is_loopback = iface.is_loopback();
            let ip = iface.addr.ip();
            InterfaceCandidate {
                name: iface.name,
                ip,
                is_loopback,
            }
        })
        .collect();

    select_best_lan_address(&candidates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn test_private_ipv4() {
        assert!(is_private_ipv4(&Ipv4Addr::new(192, 168, 1, 42)));
        assert!(is_private_ipv4(&Ipv4Addr::new(10, 0, 0, 5)));
        assert!(is_private_ipv4(&Ipv4Addr::new(172, 16, 0, 1)));
        assert!(is_private_ipv4(&Ipv4Addr::new(172, 31, 255, 254)));

        assert!(!is_private_ipv4(&Ipv4Addr::new(172, 32, 0, 1)));
        assert!(!is_private_ipv4(&Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_private_ipv4(&Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn test_select_best_lan_address_prefers_real_lan_over_docker() {
        let candidates = vec![
            InterfaceCandidate {
                name: "lo".to_string(),
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                is_loopback: true,
            },
            InterfaceCandidate {
                name: "docker0".to_string(),
                ip: IpAddr::V4(Ipv4Addr::new(172, 17, 0, 1)),
                is_loopback: false,
            },
            InterfaceCandidate {
                name: "wlan0".to_string(),
                ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
                is_loopback: false,
            },
        ];

        let selected = select_best_lan_address(&candidates).expect("should find LAN address");
        assert_eq!(selected, Ipv4Addr::new(192, 168, 1, 100));
    }

    #[test]
    fn test_select_best_lan_address_rejects_link_local_and_loopback() {
        let candidates = vec![
            InterfaceCandidate {
                name: "lo".to_string(),
                ip: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                is_loopback: true,
            },
            InterfaceCandidate {
                name: "eth0".to_string(),
                ip: IpAddr::V4(Ipv4Addr::new(169, 254, 10, 20)),
                is_loopback: false,
            },
        ];

        let result = select_best_lan_address(&candidates);
        assert_eq!(result, Err(NetworkError::NoUsableInterface));
    }

    #[test]
    fn test_find_local_lan_ip_on_host() {
        let ip = find_local_lan_ip();
        assert!(
            ip.is_ok(),
            "Expected to find local LAN IP on host: {:?}",
            ip
        );
        let ip = ip.unwrap();
        assert!(
            is_private_ipv4(&ip),
            "Expected private IPv4 address, got {}",
            ip
        );
    }
}
