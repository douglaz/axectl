use anyhow::{Context, Result};
use ipnetwork::IpNetwork;
use std::net::IpAddr;
use std::time::Duration;
use tokio::time::timeout;

use crate::api::{AxeOsClient, Device, DeviceStatus, DeviceType};
use crate::discovery::network::{NetworkInfo, get_network_addresses};

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub devices_found: Vec<Device>,
    pub scan_info: ScanInfo,
}

#[derive(Debug, Clone)]
pub struct ScanInfo {
    pub network_scanned: NetworkInfo,
    pub addresses_scanned: usize,
    pub responsive_addresses: usize,
    pub axeos_devices: usize,
    pub scan_duration_seconds: f64,
    pub errors_encountered: usize,
}

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub timeout_per_host: Duration,
    pub parallel_scans: usize,
    pub axeos_only: bool,
    pub include_unreachable: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            timeout_per_host: Duration::from_millis(500),
            parallel_scans: 50,
            axeos_only: true,
            include_unreachable: false,
        }
    }
}

pub async fn scan_network(network: IpNetwork, config: ScanConfig) -> Result<ScanResult> {
    let start_time = std::time::Instant::now();
    let network_info = crate::discovery::network::get_network_info(&network);

    let addresses = get_network_addresses(&network);

    // Skip network and broadcast addresses for IPv4
    let scan_addresses: Vec<IpAddr> = match network {
        IpNetwork::V4(_) => {
            // Skip first (network) and last (broadcast) addresses
            if addresses.len() > 2 {
                addresses[1..addresses.len() - 1].to_vec()
            } else {
                addresses
            }
        }
        IpNetwork::V6(_) => addresses,
    };

    let devices = scan_addresses_parallel(scan_addresses.clone(), config.clone()).await?;

    let scan_duration = start_time.elapsed();
    let errors_encountered = 0; // We'll track this in a more sophisticated implementation

    let scan_info = ScanInfo {
        network_scanned: network_info,
        addresses_scanned: scan_addresses.len(),
        responsive_addresses: devices
            .iter()
            .filter(|d| d.status != DeviceStatus::Offline)
            .count(),
        axeos_devices: devices.len(),
        scan_duration_seconds: scan_duration.as_secs_f64(),
        errors_encountered,
    };

    Ok(ScanResult {
        devices_found: devices,
        scan_info,
    })
}

async fn scan_addresses_parallel(
    addresses: Vec<IpAddr>,
    config: ScanConfig,
) -> Result<Vec<Device>> {
    use futures::stream::{self, StreamExt};

    let devices = stream::iter(addresses)
        .map(|addr| scan_single_address(addr, config.clone()))
        .buffer_unordered(config.parallel_scans)
        .collect::<Vec<_>>()
        .await;

    let mut found_devices = Vec::new();
    for device_result in devices {
        match device_result {
            Ok(Some(device)) => found_devices.push(device),
            Ok(None) => {} // No device found
            Err(_) => {}   // Error scanning - could track this
        }
    }

    Ok(found_devices)
}

async fn scan_single_address(ip: IpAddr, config: ScanConfig) -> Result<Option<Device>> {
    let ip_str = ip.to_string();

    // Create AxeOS client with appropriate timeout
    let client = AxeOsClient::with_timeout(&ip_str, config.timeout_per_host)?;

    // Try to connect and identify the device
    let health_check = timeout(config.timeout_per_host, client.health_check()).await;

    match health_check {
        Ok(Ok(true)) => {
            // Device is responsive, try to get system info with proper device type detection
            if let Ok(Ok((system_info, device_type))) =
                timeout(config.timeout_per_host, client.get_complete_device_info()).await
            {
                let device = Device {
                    name: system_info.hostname.clone(),
                    ip_address: ip_str,
                    device_type,
                    serial_number: Some(system_info.mac_address.clone()),
                    status: DeviceStatus::Online,
                    discovered_at: chrono::Utc::now(),
                    last_seen: chrono::Utc::now(),
                    stats: None,
                };

                return Ok(Some(device));
            }

            // If we can't get system info but health check passed,
            // it might be an AxeOS device that's not fully responsive
            if !config.axeos_only {
                let device = Device {
                    name: format!("Unknown-{ip_str}"),
                    ip_address: ip_str,
                    device_type: DeviceType::Unknown,
                    serial_number: None,
                    status: DeviceStatus::Online,
                    discovered_at: chrono::Utc::now(),
                    last_seen: chrono::Utc::now(),
                    stats: None,
                };

                return Ok(Some(device));
            }
        }
        Ok(Ok(false)) | Ok(Err(_)) | Err(_) => {
            // Device not responsive or error
            if config.include_unreachable {
                let device = Device {
                    name: format!("Offline-{ip_str}"),
                    ip_address: ip_str,
                    device_type: DeviceType::Unknown,
                    serial_number: None,
                    status: DeviceStatus::Offline,
                    discovered_at: chrono::Utc::now(),
                    last_seen: chrono::Utc::now(),
                    stats: None,
                };

                return Ok(Some(device));
            }
        }
    }

    Ok(None)
}

/// Scan a single IP address to check if it's running AxeOS
pub async fn probe_single_device(ip: &str, timeout: Duration) -> Result<Option<Device>> {
    let config = ScanConfig {
        timeout_per_host: timeout,
        axeos_only: true,
        include_unreachable: false,
        ..Default::default()
    };

    let ip_addr: IpAddr = ip.parse().context("Invalid IP address")?;
    scan_single_address(ip_addr, config).await
}

/// Quick health check for a known device
pub async fn quick_health_check(ip: &str) -> Result<bool> {
    let client = AxeOsClient::with_timeout(ip, Duration::from_millis(1000))?;

    match timeout(Duration::from_millis(2000), client.health_check()).await {
        Ok(Ok(is_healthy)) => Ok(is_healthy),
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_scan_config_defaults() {
        let config = ScanConfig::default();
        assert_eq!(config.timeout_per_host, Duration::from_millis(500));
        assert_eq!(config.parallel_scans, 50);
        assert!(config.axeos_only);
        assert!(!config.include_unreachable);
    }

    #[tokio::test]
    async fn test_probe_single_device_invalid_ip() {
        let result = probe_single_device("not.an.ip", Duration::from_millis(100)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_probe_single_device_unreachable() {
        // Use a reserved IP that should not respond
        let result = probe_single_device("192.0.2.1", Duration::from_millis(100)).await;

        match result {
            Ok(None) => {} // Expected - no device found
            Ok(Some(_)) => panic!("Should not find a device on reserved IP"),
            Err(_) => {} // Also acceptable - network error
        }
    }

    #[tokio::test]
    async fn test_quick_health_check() {
        // Test with localhost (should fail since we don't run AxeOS there)
        let result = quick_health_check("127.0.0.1").await;

        match result {
            Ok(false) => {} // Expected
            Ok(true) => panic!("Localhost should not be running AxeOS"),
            Err(_) => {} // Connection error is acceptable
        }
    }

    #[tokio::test]
    async fn test_scan_small_network() {
        // Test scanning a very small network range
        let network: IpNetwork = "192.0.2.0/30".parse().unwrap(); // 4 addresses
        let config = ScanConfig {
            timeout_per_host: Duration::from_millis(50),
            parallel_scans: 2,
            axeos_only: true,
            include_unreachable: false,
        };

        let result = scan_network(network, config).await;
        assert!(result.is_ok());

        let scan_result = result.unwrap();
        assert_eq!(scan_result.scan_info.addresses_scanned, 2); // Should skip network/broadcast
        assert!(scan_result.scan_info.scan_duration_seconds > 0.0);
        assert_eq!(scan_result.devices_found.len(), 0); // Should find no devices in test range
    }

    #[tokio::test]
    async fn test_scan_config_custom() {
        let config = ScanConfig {
            timeout_per_host: Duration::from_millis(1000),
            parallel_scans: 10,
            axeos_only: false,
            include_unreachable: true,
        };

        assert_eq!(config.timeout_per_host, Duration::from_millis(1000));
        assert_eq!(config.parallel_scans, 10);
        assert!(!config.axeos_only);
        assert!(config.include_unreachable);
    }

    #[tokio::test]
    async fn test_scan_result_structure() {
        let network: IpNetwork = "192.0.2.0/30".parse().unwrap();
        let config = ScanConfig {
            timeout_per_host: Duration::from_millis(10),
            parallel_scans: 1,
            axeos_only: true,
            include_unreachable: false,
        };

        let result = scan_network(network, config).await.unwrap();

        // Verify ScanResult structure
        assert!(result.devices_found.is_empty());
        assert_eq!(result.scan_info.addresses_scanned, 2);
        assert_eq!(result.scan_info.responsive_addresses, 0);
        assert_eq!(result.scan_info.axeos_devices, 0);
        assert!(result.scan_info.scan_duration_seconds >= 0.0);
        assert_eq!(result.scan_info.errors_encountered, 0);

        // Verify NetworkInfo is populated
        assert!(!result.scan_info.network_scanned.network_str.is_empty());
        assert!(result.scan_info.network_scanned.host_count > 0);
    }

    #[tokio::test]
    async fn test_scan_ipv6_network() {
        // Test with a small IPv6 network
        let network: IpNetwork = "2001:db8::/126".parse().unwrap(); // 4 addresses
        let config = ScanConfig {
            timeout_per_host: Duration::from_millis(10),
            parallel_scans: 2,
            axeos_only: true,
            include_unreachable: false,
        };

        let result = scan_network(network, config).await;
        assert!(result.is_ok());

        let scan_result = result.unwrap();
        // IPv6 doesn't skip network/broadcast addresses like IPv4
        assert_eq!(scan_result.scan_info.addresses_scanned, 4);
        assert_eq!(scan_result.devices_found.len(), 0);
    }

    #[tokio::test]
    async fn test_probe_single_device_valid_ip() {
        // Test with a valid but unreachable IP
        let result = probe_single_device("198.51.100.1", Duration::from_millis(10)).await;

        // Should either return Ok(None) or an error, but not a device
        match result {
            Ok(None) => {} // Expected
            Ok(Some(_)) => panic!("Should not find device on test IP"),
            Err(_) => {} // Also acceptable
        }
    }

    #[tokio::test]
    async fn test_probe_single_device_localhost() {
        // Test with localhost - should be reachable but not AxeOS
        let result = probe_single_device("127.0.0.1", Duration::from_millis(100)).await;

        match result {
            Ok(None) => {} // Expected - no AxeOS device
            Ok(Some(_)) => panic!("Localhost should not be detected as AxeOS device"),
            Err(_) => {} // Connection error is acceptable
        }
    }

    #[tokio::test]
    async fn test_quick_health_check_unreachable() {
        // Test with reserved IP that should not respond
        let result = quick_health_check("192.0.2.254").await;

        match result {
            Ok(false) => {} // Expected
            Ok(true) => panic!("Reserved IP should not be healthy"),
            Err(_) => {} // Network error is acceptable
        }
    }

    #[tokio::test]
    async fn test_scan_with_include_unreachable() {
        let network: IpNetwork = "192.0.2.0/30".parse().unwrap();
        let config = ScanConfig {
            timeout_per_host: Duration::from_millis(10),
            parallel_scans: 1,
            axeos_only: false,
            include_unreachable: true,
        };

        let result = scan_network(network, config).await.unwrap();

        // With include_unreachable=true, we might get offline devices
        // The length is always >= 0, so we just verify it completes successfully

        // All devices should be marked as offline if found
        for device in result.devices_found {
            if device.status == DeviceStatus::Offline {
                assert!(device.name.starts_with("Offline-"));
                assert_eq!(device.device_type, DeviceType::Unknown);
                assert!(device.serial_number.is_none());
            }
        }
    }

    #[tokio::test]
    async fn test_scan_with_axeos_only_false() {
        let network: IpNetwork = "192.0.2.0/31".parse().unwrap(); // 2 addresses
        let config = ScanConfig {
            timeout_per_host: Duration::from_millis(10),
            parallel_scans: 1,
            axeos_only: false,
            include_unreachable: false,
        };

        let result = scan_network(network, config).await.unwrap();

        // Should complete without error even with axeos_only=false
        assert_eq!(result.scan_info.addresses_scanned, 2);
    }

    #[tokio::test]
    async fn test_parallel_scan_performance() {
        let network: IpNetwork = "192.0.2.0/29".parse().unwrap(); // 8 addresses

        // Test with different parallel scan settings
        let config_serial = ScanConfig {
            timeout_per_host: Duration::from_millis(10),
            parallel_scans: 1,
            axeos_only: true,
            include_unreachable: false,
        };

        let config_parallel = ScanConfig {
            timeout_per_host: Duration::from_millis(10),
            parallel_scans: 6,
            axeos_only: true,
            include_unreachable: false,
        };

        let start = std::time::Instant::now();
        let result1 = scan_network(network, config_serial).await.unwrap();
        let serial_duration = start.elapsed();

        let start = std::time::Instant::now();
        let result2 = scan_network(network, config_parallel).await.unwrap();
        let parallel_duration = start.elapsed();

        // Both should find the same number of devices (0 in test network)
        assert_eq!(result1.devices_found.len(), result2.devices_found.len());
        assert_eq!(
            result1.scan_info.addresses_scanned,
            result2.scan_info.addresses_scanned
        );

        // Parallel should generally be faster, but allow for some variance in test environment
        // This is more of a sanity check than a strict performance requirement
        assert!(parallel_duration <= serial_duration + Duration::from_millis(50));
    }
}
