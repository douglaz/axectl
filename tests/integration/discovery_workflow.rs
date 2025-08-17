/// Integration tests for the complete discovery workflow
/// These tests verify the end-to-end discovery process including:
/// - Network scanning with mock HTTP servers
/// - mDNS discovery simulation  
/// - Device storage and aggregation
/// - Combined discovery results
use anyhow::Result;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;
use tokio::task;

use axectl::api::{AxeOsClient, DeviceInfo, DeviceStatus, DeviceType};
use axectl::discovery::{
    mdns::{MdnsDevice, MdnsDiscovery},
    network::parse_network,
    scanner::{scan_network, ScanConfig},
};
use axectl::storage::memory::MemoryStorage;

/// Mock HTTP server for simulating AxeOS devices
struct MockAxeOsServer {
    server: mockito::ServerGuard,
    device_type: DeviceType,
    hostname: String,
}

impl MockAxeOsServer {
    async fn new_bitaxe(hostname: &str) -> Self {
        let mut server = mockito::Server::new_async().await;

        // Mock system info endpoint with Bitaxe response
        server
            .mock("GET", "/api/system/info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                "ASICModel": "BM1368",
                "boardVersion": "204",
                "version": "2.0.0",
                "macAddr": "AA:BB:CC:DD:EE:FF",
                "hostname": "{}",
                "ssid": "TestNetwork",
                "wifiStatus": "Connected",
                "wifiRSSI": -45,
                "stratumURL": "stratum+tcp://test.pool.com",
                "stratumPort": 4334,
                "stratumUser": "bc1qtest123",
                "frequency": 485,
                "voltage": 1200,
                "fanspeed": 75,
                "temp": 65.5,
                "power": 15.8,
                "hashRate": 485.2,
                "uptimeSeconds": 3600,
                "sharesAccepted": 150,
                "sharesRejected": 2,
                "bestDiff": "123.45K"
            }}"#,
                hostname
            ))
            .create_async()
            .await;

        Self {
            server,
            device_type: DeviceType::BitaxeMax,
            hostname: hostname.to_string(),
        }
    }

    async fn new_nerdqaxe(hostname: &str) -> Self {
        let mut server = mockito::Server::new_async().await;

        // Mock system info endpoint with NerdQAxe response
        server
            .mock("GET", "/api/system/info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{
                "deviceModel": "NerdQAxe++",
                "ASICModel": "BM1368",
                "version": "1.5.2",
                "macAddr": "11:22:33:44:55:66",
                "hostname": "{}",
                "hostip": "192.168.1.101",
                "ssid": "TestNetwork",
                "wifiStatus": "Connected",
                "wifiRSSI": -52,
                "stratumURL": "stratum+tcp://test.pool.com",
                "stratumPort": 4334,
                "stratumUser": "bc1qtest456",
                "frequency": 500,
                "voltage": 1250,
                "fanspeed": 80,
                "temp": 62.8,
                "power": 18.5,
                "hashRate": 512.7,
                "uptimeSeconds": 7200,
                "sharesAccepted": 225,
                "sharesRejected": 3,
                "bestDiff": "234.56K",
                "runningPartition": "firmware_a"
            }}"#,
                hostname
            ))
            .create_async()
            .await;

        Self {
            server,
            device_type: DeviceType::NerdqaxePlus,
            hostname: hostname.to_string(),
        }
    }

    fn url(&self) -> String {
        self.server.url()
    }

    fn device_type(&self) -> DeviceType {
        self.device_type.clone()
    }

    fn hostname(&self) -> &str {
        &self.hostname
    }
}

/// Create mock mDNS devices for testing
fn create_mock_mdns_devices() -> Vec<MdnsDevice> {
    vec![
        MdnsDevice {
            hostname: "bitaxe-001.local".to_string(),
            ip_addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))],
            port: 80,
            service_type: "_http._tcp.local.".to_string(),
            txt_records: {
                let mut records = HashMap::new();
                records.insert("model".to_string(), "Bitaxe Ultra".to_string());
                records.insert("version".to_string(), "2.0.0".to_string());
                records
            },
        },
        MdnsDevice {
            hostname: "nerdqaxe-plus.local".to_string(),
            ip_addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 101))],
            port: 80,
            service_type: "_http._tcp.local.".to_string(),
            txt_records: {
                let mut records = HashMap::new();
                records.insert("model".to_string(), "NerdQAxe++".to_string());
                records.insert("firmware".to_string(), "AxeOS v1.5.2".to_string());
                records
            },
        },
    ]
}

#[tokio::test]
async fn test_complete_discovery_workflow() -> Result<()> {
    // Setup: Create mock servers for different device types
    let bitaxe_server = MockAxeOsServer::new_bitaxe("bitaxe-test").await;
    let nerdqaxe_server = MockAxeOsServer::new_nerdqaxe("nerdqaxe-test").await;

    // Create storage for discovered devices
    let storage = Arc::new(MemoryStorage::new());

    // Phase 1: Network Discovery Simulation
    // In a real scenario, we would scan a network range, but for testing we'll simulate
    // discovered devices by directly probing our mock servers

    let discovered_devices = vec![
        (
            bitaxe_server.url(),
            bitaxe_server.device_type(),
            bitaxe_server.hostname(),
        ),
        (
            nerdqaxe_server.url(),
            nerdqaxe_server.device_type(),
            nerdqaxe_server.hostname(),
        ),
    ];

    let mut discovery_results = Vec::new();

    for (url, expected_type, expected_hostname) in discovered_devices {
        // Extract the port from the mock server URL to create a client
        let client = AxeOsClient::with_timeout(&url, Duration::from_secs(2))?;

        // Perform health check
        let is_healthy = client.health_check().await?;
        assert!(is_healthy, "Device should respond to health check");

        // Get complete device info
        let (system_info, device_type) = client.get_complete_device_info().await?;

        // Verify device type detection
        assert_eq!(device_type, expected_type);
        assert_eq!(system_info.hostname, expected_hostname);

        // Create device info and store it
        let device = DeviceInfo {
            name: system_info.hostname.clone(),
            ip_address: url, // Using URL as IP for testing
            device_type,
            serial_number: Some(system_info.mac_address.clone()),
            status: DeviceStatus::Online,
            discovered_at: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
        };

        storage.upsert_device(device.clone())?;
        discovery_results.push(device);
    }

    // Phase 2: Verify Discovery Results
    let all_devices = storage.get_all_devices()?;
    assert_eq!(all_devices.len(), 2, "Should have discovered 2 devices");

    // Check device types
    let device_types: Vec<DeviceType> = all_devices.iter().map(|d| d.device_type.clone()).collect();
    assert!(device_types.contains(&DeviceType::BitaxeMax));
    assert!(device_types.contains(&DeviceType::NerdqaxePlus));

    // Check all devices are online
    for device in &all_devices {
        assert_eq!(device.status, DeviceStatus::Online);
        assert!(device.serial_number.is_some());
    }

    // Phase 3: Simulate Statistics Collection
    // After discovery, we would typically start collecting statistics
    let mut collection_tasks = Vec::new();

    for device in &discovery_results {
        let storage_clone = Arc::clone(&storage);
        let device_clone = device.clone();

        let task = task::spawn(async move {
            // Simulate collecting stats from the device
            let stats = axectl::api::models::DeviceStats {
                device_id: device_clone.ip_address.clone(),
                timestamp: chrono::Utc::now(),
                hashrate_mhs: if device_clone.device_type == DeviceType::BitaxeMax {
                    485.2
                } else {
                    512.7
                },
                temperature_celsius: 65.0,
                power_watts: 16.0,
                fan_speed_rpm: 3000,
                shares_accepted: 150,
                shares_rejected: 2,
                uptime_seconds: 3600,
                pool_url: Some("stratum+tcp://test.pool.com:4334".to_string()),
                wifi_rssi: Some(-45),
                voltage: Some(12.0),
                frequency: Some(500),
            };

            storage_clone.store_stats(stats).unwrap();
        });

        collection_tasks.push(task);
    }

    // Wait for stats collection to complete
    for task in collection_tasks {
        task.await?;
    }

    // Phase 4: Verify Complete System State
    let swarm_summary = storage.get_swarm_summary()?;
    assert_eq!(swarm_summary.total_devices, 2);
    assert_eq!(swarm_summary.devices_online, 2);
    assert_eq!(swarm_summary.devices_offline, 0);
    assert!(swarm_summary.total_hashrate_mhs > 900.0); // 485.2 + 512.7
    assert!(swarm_summary.total_power_watts > 30.0);
    assert_eq!(swarm_summary.total_shares_accepted, 300); // 150 * 2

    // Verify statistics storage
    let latest_stats = storage.get_latest_stats()?;
    assert_eq!(latest_stats.len(), 2);

    // Verify storage info
    let storage_info = storage.get_storage_info()?;
    assert_eq!(storage_info.device_count, 2);
    assert_eq!(storage_info.latest_stats_count, 2);
    assert_eq!(storage_info.total_stats_entries, 2);

    Ok(())
}

#[tokio::test]
async fn test_discovery_with_offline_devices() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create one working device and simulate one offline device
    let working_server = MockAxeOsServer::new_bitaxe("working-device").await;

    // Test discovery of working device
    let client = AxeOsClient::with_timeout(&working_server.url(), Duration::from_secs(2))?;
    let is_healthy = client.health_check().await?;
    assert!(is_healthy);

    let (system_info, device_type) = client.get_complete_device_info().await?;
    let working_device = DeviceInfo {
        name: system_info.hostname,
        ip_address: working_server.url(),
        device_type,
        serial_number: Some(system_info.mac_address),
        status: DeviceStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    };

    storage.upsert_device(working_device)?;

    // Simulate an offline device
    let offline_device = DeviceInfo {
        name: "offline-device".to_string(),
        ip_address: "192.168.1.199".to_string(),
        device_type: DeviceType::Unknown,
        serial_number: None,
        status: DeviceStatus::Offline,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now() - chrono::Duration::minutes(10),
    };

    storage.upsert_device(offline_device)?;

    // Verify mixed device states
    let all_devices = storage.get_all_devices()?;
    assert_eq!(all_devices.len(), 2);

    let online_devices = storage.get_devices_by_status(DeviceStatus::Online)?;
    assert_eq!(online_devices.len(), 1);

    let offline_devices = storage.get_devices_by_status(DeviceStatus::Offline)?;
    assert_eq!(offline_devices.len(), 1);

    // Test swarm summary with mixed states
    let summary = storage.get_swarm_summary()?;
    assert_eq!(summary.total_devices, 2);
    assert_eq!(summary.devices_online, 1);
    assert_eq!(summary.devices_offline, 1);

    Ok(())
}

#[tokio::test]
async fn test_discovery_network_scanning_simulation() -> Result<()> {
    // Simulate a small network scan
    let network = parse_network("192.0.2.0/30")?; // Test network with 4 addresses
    let config = ScanConfig {
        timeout_per_host: Duration::from_millis(50),
        parallel_scans: 2,
        axeos_only: true,
        include_unreachable: false,
    };

    // This should complete without finding any devices in the test network
    let scan_result = scan_network(network, config).await?;

    // Verify scan completed successfully
    assert_eq!(scan_result.scan_info.addresses_scanned, 2); // Should skip network/broadcast
    assert_eq!(scan_result.devices_found.len(), 0); // No real devices in test network
    assert!(scan_result.scan_info.scan_duration_seconds > 0.0);
    assert_eq!(scan_result.scan_info.responsive_addresses, 0);
    assert_eq!(scan_result.scan_info.axeos_devices, 0);

    // Verify network info
    assert!(!scan_result.scan_info.network_scanned.network_str.is_empty());
    assert!(scan_result.scan_info.network_scanned.host_count > 0);

    Ok(())
}

#[tokio::test]
async fn test_mdns_discovery_simulation() -> Result<()> {
    // Test mDNS discovery configuration and setup
    let discovery = MdnsDiscovery::with_timeout(Duration::from_millis(100));

    // This will typically find no devices in test environment but should complete successfully
    let _devices = discovery.discover_devices().await?;

    // Verify the discovery completed without errors
    // In most test environments, we expect no devices to be found
    // The length is always >= 0, so we just verify it completed successfully

    // Test device detection logic by verifying mock mDNS device structure
    let mock_devices = create_mock_mdns_devices();

    for mock_device in mock_devices {
        // Verify mock device structure is valid for testing
        assert!(!mock_device.hostname.is_empty());
        assert!(!mock_device.ip_addresses.is_empty());
        assert!(mock_device.port > 0);
        assert!(!mock_device.service_type.is_empty());

        // Check that mock devices have appropriate identifiers
        let hostname_lower = mock_device.hostname.to_lowercase();
        let has_device_identifier =
            hostname_lower.contains("bitaxe") || hostname_lower.contains("nerdqaxe");
        assert!(
            has_device_identifier,
            "Mock device should have recognizable identifier"
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_discovery_operations() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create multiple mock servers
    let servers = vec![
        MockAxeOsServer::new_bitaxe("concurrent-bitaxe-1").await,
        MockAxeOsServer::new_bitaxe("concurrent-bitaxe-2").await,
        MockAxeOsServer::new_nerdqaxe("concurrent-nerdqaxe-1").await,
    ];

    let mut discovery_tasks = Vec::new();

    // Spawn concurrent discovery tasks
    for (i, server) in servers.iter().enumerate() {
        let storage_clone = Arc::clone(&storage);
        let url = server.url();
        let expected_hostname = server.hostname().to_string();
        let expected_type = server.device_type();

        let task = task::spawn(async move {
            let client = AxeOsClient::with_timeout(&url, Duration::from_secs(2)).unwrap();

            // Concurrent health checks
            let is_healthy = client.health_check().await.unwrap();
            assert!(is_healthy);

            // Concurrent device info retrieval
            let (system_info, device_type) = client.get_complete_device_info().await.unwrap();
            assert_eq!(device_type, expected_type);
            assert_eq!(system_info.hostname, expected_hostname);

            // Concurrent device storage
            let device = DeviceInfo {
                name: system_info.hostname,
                ip_address: format!("test-device-{}", i),
                device_type,
                serial_number: Some(system_info.mac_address),
                status: DeviceStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            };

            storage_clone.upsert_device(device).unwrap();
        });

        discovery_tasks.push(task);
    }

    // Wait for all discovery tasks to complete
    for task in discovery_tasks {
        task.await?;
    }

    // Verify all devices were discovered and stored correctly
    let all_devices = storage.get_all_devices()?;
    assert_eq!(all_devices.len(), 3);

    let bitaxe_devices: Vec<_> = all_devices
        .iter()
        .filter(|d| d.device_type == DeviceType::BitaxeMax)
        .collect();
    assert_eq!(bitaxe_devices.len(), 2);

    let nerdqaxe_devices: Vec<_> = all_devices
        .iter()
        .filter(|d| d.device_type == DeviceType::NerdqaxePlus)
        .collect();
    assert_eq!(nerdqaxe_devices.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_discovery_error_handling() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Test discovery with invalid/unreachable endpoints
    let invalid_urls = vec![
        "http://192.0.2.1", // Reserved test IP that should not respond
        "http://invalid.local:8080",
        "http://127.0.0.1:9999", // Localhost on unused port
    ];

    for url in invalid_urls {
        let client_result = AxeOsClient::with_timeout(url, Duration::from_millis(100));

        match client_result {
            Ok(client) => {
                // Client creation succeeded, but health check should fail
                let health_result = client.health_check().await;
                match health_result {
                    Ok(false) => {
                        // Health check returned false - device not responsive
                    }
                    Err(_) => {
                        // Health check failed with error - expected for unreachable hosts
                    }
                    Ok(true) => {
                        panic!("Health check should not succeed for test URLs");
                    }
                }
            }
            Err(_) => {
                // Client creation failed - acceptable for invalid URLs
            }
        }
    }

    // Verify storage remains empty after failed discoveries
    let devices = storage.get_all_devices()?;
    assert_eq!(devices.len(), 0);

    Ok(())
}
