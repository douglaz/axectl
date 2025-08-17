use anyhow::{anyhow, Context, Result};
use ipnetwork::{IpNetwork, Ipv4Network};
use local_ip_address::local_ip;
use std::net::{IpAddr, Ipv4Addr};

/// Auto-detect the local network range
pub fn auto_detect_network() -> Result<IpNetwork> {
    let local_ip = local_ip().context("Failed to get local IP address")?;

    match local_ip {
        IpAddr::V4(ipv4) => {
            // Common home network patterns
            let octets = ipv4.octets();

            let network = if octets[0] == 192 && octets[1] == 168 {
                // 192.168.x.0/24
                Ipv4Network::new(Ipv4Addr::new(192, 168, octets[2], 0), 24)
                    .context("Failed to create 192.168.x.0/24 network")?
            } else if octets[0] == 10 {
                // 10.x.x.0/24 (using same third octet)
                Ipv4Network::new(Ipv4Addr::new(10, octets[1], octets[2], 0), 24)
                    .context("Failed to create 10.x.x.0/24 network")?
            } else if octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31 {
                // 172.16-31.x.0/24
                Ipv4Network::new(Ipv4Addr::new(172, octets[1], octets[2], 0), 24)
                    .context("Failed to create 172.16-31.x.0/24 network")?
            } else {
                // Default to /24 subnet of current IP
                Ipv4Network::new(Ipv4Addr::new(octets[0], octets[1], octets[2], 0), 24)
                    .context("Failed to create default /24 network")?
            };

            Ok(IpNetwork::V4(network))
        }
        IpAddr::V6(_) => Err(anyhow!(
            "IPv6 networks are not yet supported for auto-detection"
        )),
    }
}

/// Get common fallback networks to scan
pub fn get_fallback_networks() -> Vec<IpNetwork> {
    let networks = vec![
        "192.168.1.0/24",
        "192.168.0.0/24",
        "10.0.0.0/24",
        "10.0.1.0/24",
        "172.16.0.0/24",
    ];

    networks
        .into_iter()
        .filter_map(|net| net.parse().ok())
        .collect()
}

/// Parse a network string into IpNetwork
pub fn parse_network(network_str: &str) -> Result<IpNetwork> {
    network_str
        .parse()
        .context("Failed to parse network string")
}

/// Get all IP addresses in a network range
pub fn get_network_addresses(network: &IpNetwork) -> Vec<IpAddr> {
    match network {
        IpNetwork::V4(ipv4_net) => ipv4_net.iter().map(IpAddr::V4).collect(),
        IpNetwork::V6(ipv6_net) => {
            // For IPv6, we'll limit to a reasonable number of addresses
            ipv6_net.iter().take(1000).map(IpAddr::V6).collect()
        }
    }
}

/// Check if an IP address is in a private range
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_private(),
        IpAddr::V6(ipv6) => {
            // Basic IPv6 private address check
            ipv6.is_loopback() ||
            ipv6.segments()[0] & 0xfe00 == 0xfc00 || // Unique local addresses
            ipv6.segments()[0] & 0xffc0 == 0xfe80 // Link-local addresses
        }
    }
}

/// Get network information for display
pub fn get_network_info(network: &IpNetwork) -> NetworkInfo {
    let addresses = get_network_addresses(network);
    let host_count = addresses.len();

    NetworkInfo {
        network: *network,
        network_str: network.to_string(),
        first_host: addresses.first().copied(),
        last_host: addresses.last().copied(),
        host_count,
        is_private: addresses.first().map(is_private_ip).unwrap_or(false),
    }
}

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub network: IpNetwork,
    pub network_str: String,
    pub first_host: Option<IpAddr>,
    pub last_host: Option<IpAddr>,
    pub host_count: usize,
    pub is_private: bool,
}

impl NetworkInfo {
    pub fn estimated_scan_time_seconds(&self, timeout_per_host_ms: u64) -> u64 {
        (self.host_count as u64 * timeout_per_host_ms) / 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_network() -> Result<()> {
        let network = parse_network("192.168.1.0/24")?;
        assert!(matches!(network, IpNetwork::V4(_)));

        let network = parse_network("10.0.0.0/8")?;
        assert!(matches!(network, IpNetwork::V4(_)));

        Ok(())
    }

    #[test]
    fn test_get_network_addresses() {
        let network: IpNetwork = "192.168.1.0/30".parse().unwrap(); // Only 4 addresses
        let addresses = get_network_addresses(&network);
        assert_eq!(addresses.len(), 4);
        assert_eq!(addresses[0], IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)));
        assert_eq!(addresses[3], IpAddr::V4(Ipv4Addr::new(192, 168, 1, 3)));
    }

    #[test]
    fn test_is_private_ip() {
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn test_get_fallback_networks() {
        let fallbacks = get_fallback_networks();
        assert!(!fallbacks.is_empty());
        assert!(fallbacks
            .iter()
            .any(|net| net.to_string().contains("192.168.1.0")));
    }

    #[test]
    fn test_network_info() {
        let network: IpNetwork = "192.168.1.0/28".parse().unwrap(); // 16 addresses
        let info = get_network_info(&network);

        assert_eq!(info.host_count, 16);
        assert!(info.is_private);
        assert_eq!(info.network_str, "192.168.1.0/28");

        // Estimate should be reasonable (16 hosts * 100ms = 1.6 seconds)
        assert_eq!(info.estimated_scan_time_seconds(100), 1);
    }

    #[test]
    fn test_auto_detect_network() {
        // This test may fail in environments without network connectivity
        // but should work in most development environments
        match auto_detect_network() {
            Ok(network) => {
                let info = get_network_info(&network);
                eprintln!("Detected network: {}", info.network_str);
                assert!(info.is_private); // Local networks should be private
            }
            Err(e) => {
                // In some test environments, this might fail
                eprintln!(
                    "Auto-detection failed (expected in some environments): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_parse_network_invalid() {
        let result = parse_network("invalid.network");
        assert!(result.is_err());

        let result = parse_network("300.300.300.300/24");
        assert!(result.is_err());

        let result = parse_network("192.168.1.0/33");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_network_addresses_ipv6() {
        let network: IpNetwork = "2001:db8::/126".parse().unwrap(); // 4 addresses
        let addresses = get_network_addresses(&network);
        assert_eq!(addresses.len(), 4);

        // Should be IPv6 addresses
        for addr in addresses {
            assert!(matches!(addr, IpAddr::V6(_)));
        }
    }

    #[test]
    fn test_get_network_addresses_large_ipv6() {
        let network: IpNetwork = "2001:db8::/64".parse().unwrap(); // Many addresses
        let addresses = get_network_addresses(&network);

        // Should be limited to 1000 addresses for IPv6
        assert_eq!(addresses.len(), 1000);
    }

    #[test]
    fn test_is_private_ip_ipv6() {
        use std::net::Ipv6Addr;

        // Loopback
        assert!(is_private_ip(&IpAddr::V6(Ipv6Addr::LOCALHOST)));

        // Unique local addresses (fc00::/7)
        assert!(is_private_ip(&IpAddr::V6("fc00::1".parse().unwrap())));
        assert!(is_private_ip(&IpAddr::V6("fd00::1".parse().unwrap())));

        // Link-local addresses (fe80::/10)
        assert!(is_private_ip(&IpAddr::V6("fe80::1".parse().unwrap())));

        // Global unicast (should not be private)
        assert!(!is_private_ip(&IpAddr::V6("2001:db8::1".parse().unwrap())));
    }

    #[test]
    fn test_is_private_ip_edge_cases() {
        use std::net::Ipv4Addr;

        // Loopback (127.0.0.1) - not considered "private" by Rust's is_private()
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::LOCALHOST)));

        // Link-local (169.254.0.0/16) - also not considered "private" by Rust's is_private()
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(169, 254, 1, 1))));

        // Multicast (224.0.0.0/4) - not private but special
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(224, 0, 0, 1))));

        // Documentation ranges (should not be private in our implementation)
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))));
        assert!(!is_private_ip(&IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1))));

        // Actual private ranges
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(is_private_ip(&IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1))));
    }

    #[test]
    fn test_network_info_estimated_scan_time() {
        let network: IpNetwork = "192.168.1.0/29".parse().unwrap(); // 8 addresses
        let info = get_network_info(&network);

        assert_eq!(info.estimated_scan_time_seconds(100), 0); // 8 * 100ms = 800ms = 0s
        assert_eq!(info.estimated_scan_time_seconds(500), 4); // 8 * 500ms = 4000ms = 4s
        assert_eq!(info.estimated_scan_time_seconds(1000), 8); // 8 * 1000ms = 8000ms = 8s
    }

    #[test]
    fn test_network_info_fields() {
        let network: IpNetwork = "10.0.0.0/30".parse().unwrap(); // 4 addresses
        let info = get_network_info(&network);

        assert_eq!(info.network, network);
        assert_eq!(info.network_str, "10.0.0.0/30");
        assert_eq!(info.host_count, 4);
        assert!(info.is_private);
        assert_eq!(
            info.first_host,
            Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)))
        );
        assert_eq!(info.last_host, Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 3))));
    }

    #[test]
    fn test_get_fallback_networks_details() {
        let fallbacks = get_fallback_networks();

        // Should have exactly 5 fallback networks
        assert_eq!(fallbacks.len(), 5);

        // All should be IPv4
        for network in &fallbacks {
            assert!(matches!(network, IpNetwork::V4(_)));
        }

        // Should include common home networks
        let network_strings: Vec<String> = fallbacks.iter().map(|n| n.to_string()).collect();
        assert!(network_strings.contains(&"192.168.1.0/24".to_string()));
        assert!(network_strings.contains(&"192.168.0.0/24".to_string()));
        assert!(network_strings.contains(&"10.0.0.0/24".to_string()));
        assert!(network_strings.contains(&"10.0.1.0/24".to_string()));
        assert!(network_strings.contains(&"172.16.0.0/24".to_string()));
    }

    #[test]
    fn test_parse_network_various_formats() -> Result<()> {
        // IPv4 networks
        let network = parse_network("172.16.0.0/12")?;
        assert!(matches!(network, IpNetwork::V4(_)));

        let network = parse_network("10.0.0.0/8")?;
        assert!(matches!(network, IpNetwork::V4(_)));

        // IPv6 networks
        let network = parse_network("2001:db8::/32")?;
        assert!(matches!(network, IpNetwork::V6(_)));

        let network = parse_network("fe80::/64")?;
        assert!(matches!(network, IpNetwork::V6(_)));

        Ok(())
    }

    #[test]
    fn test_get_network_addresses_single_host() {
        // /32 network should have only one address
        let network: IpNetwork = "192.168.1.100/32".parse().unwrap();
        let addresses = get_network_addresses(&network);
        assert_eq!(addresses.len(), 1);
        assert_eq!(addresses[0], IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)));
    }

    #[test]
    fn test_network_info_empty_network() {
        // This is an edge case - a /32 network
        let network: IpNetwork = "192.168.1.1/32".parse().unwrap();
        let info = get_network_info(&network);

        assert_eq!(info.host_count, 1);
        assert_eq!(info.first_host, info.last_host);
        assert!(info.is_private);
        assert_eq!(info.estimated_scan_time_seconds(1000), 1);
    }
}
