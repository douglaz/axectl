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
        IpAddr::V6(_) => {
            Err(anyhow!("IPv6 networks are not yet supported for auto-detection"))
        }
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
    network_str.parse().context("Failed to parse network string")
}

/// Get all IP addresses in a network range
pub fn get_network_addresses(network: &IpNetwork) -> Vec<IpAddr> {
    match network {
        IpNetwork::V4(ipv4_net) => {
            ipv4_net.iter().map(IpAddr::V4).collect()
        }
        IpNetwork::V6(ipv6_net) => {
            // For IPv6, we'll limit to a reasonable number of addresses
            ipv6_net.iter().take(1000).map(IpAddr::V6).collect()
        }
    }
}

/// Check if an IP address is in a private range
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => {
            ipv4.is_private()
        }
        IpAddr::V6(ipv6) => {
            // Basic IPv6 private address check
            ipv6.is_loopback() || 
            ipv6.segments()[0] & 0xfe00 == 0xfc00 || // Unique local addresses
            ipv6.segments()[0] & 0xffc0 == 0xfe80    // Link-local addresses
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
        assert!(fallbacks.iter().any(|net| net.to_string().contains("192.168.1.0")));
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
                println!("Detected network: {}", info.network_str);
                assert!(info.is_private); // Local networks should be private
            }
            Err(e) => {
                // In some test environments, this might fail
                println!("Auto-detection failed (expected in some environments): {}", e);
            }
        }
    }
}