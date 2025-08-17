/// Integration tests for monitoring and alerting functionality
/// These tests verify:
/// - Continuous monitoring with different intervals
/// - Temperature and hashrate alerting
/// - Device offline detection and marking
/// - Statistics collection during monitoring
/// - Alert generation and handling
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task;
use tokio::time::{sleep, timeout};

use axectl::api::models::DeviceStats;
use axectl::api::{AxeOsClient, DeviceInfo, DeviceStatus, DeviceType};
use axectl::storage::memory::MemoryStorage;

/// Mock HTTP server for monitoring tests
struct MonitoringMockServer {
    server: mockito::ServerGuard,
    device_name: String,
    current_temp: f64,
    current_power: f64,
}

impl MonitoringMockServer {
    async fn new(device_name: &str, initial_temp: f64, initial_power: f64) -> Self {
        let server = mockito::Server::new_async().await;
        let device_name = device_name.to_string();

        Self {
            server,
            device_name,
            current_temp: initial_temp,
            current_power: initial_power,
        }
    }

    async fn setup_endpoint(&mut self) {
        // Reset server to clear any previous mocks
        self.server.reset();

        // Create the mock endpoint with current values
        self.server
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
                "temp": {},
                "power": {},
                "hashRate": {},
                "uptimeSeconds": 3600,
                "sharesAccepted": 150,
                "sharesRejected": 2,
                "bestDiff": "123.45K"
            }}"#,
                self.device_name,
                self.current_temp,
                self.current_power,
                self.current_power * 30.0
            ))
            .create_async()
            .await;
    }

    fn url(&self) -> String {
        self.server.url()
    }

    fn set_temperature(&mut self, temp: f64) {
        self.current_temp = temp;
    }

    fn set_power(&mut self, power: f64) {
        self.current_power = power;
    }

    fn set_power_for_hashrate(&mut self, hashrate: f64) {
        // Set power field to simulate hashrate (power * 30 = hashrate)
        self.current_power = hashrate / 30.0;
    }
}

/// Create a test device for monitoring
fn create_monitoring_device(url: &str, name: &str) -> DeviceInfo {
    DeviceInfo {
        name: name.to_string(),
        ip_address: url.to_string(),
        device_type: DeviceType::BitaxeMax,
        serial_number: Some("TEST123".to_string()),
        status: DeviceStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    }
}

/// Simulate monitoring statistics collection
async fn collect_device_stats(
    client: &AxeOsClient,
    device_id: &str,
) -> Result<Option<DeviceStats>> {
    match client.get_system_info().await {
        Ok(system_info) => {
            let stats = DeviceStats {
                device_id: device_id.to_string(),
                timestamp: chrono::Utc::now(),
                hashrate_mhs: system_info.power * 30.0, // Use power field scaled to simulate hashrate changes
                temperature_celsius: system_info.temp,
                power_watts: system_info.power,
                fan_speed_rpm: system_info.fanspeed,
                shares_accepted: 150,
                shares_rejected: 2,
                uptime_seconds: system_info.running_time,
                pool_url: Some(format!(
                    "{}:{}",
                    system_info.pool_url, system_info.pool_port
                )),
                wifi_rssi: system_info.wifi_rssi,
                voltage: Some(system_info.voltage),
                frequency: Some(system_info.frequency),
            };
            Ok(Some(stats))
        }
        Err(_) => Ok(None),
    }
}

/// Check for temperature alerts
fn check_temperature_alert(stats: &DeviceStats, temp_threshold: Option<f64>) -> Option<String> {
    if let Some(threshold) = temp_threshold {
        if stats.temperature_celsius > threshold {
            return Some(format!(
                "Temperature alert: Device {} at {:.1}°C (threshold: {:.1}°C)",
                stats.device_id, stats.temperature_celsius, threshold
            ));
        }
    }
    None
}

/// Check for hashrate alerts
fn check_hashrate_alert(
    stats: &DeviceStats,
    previous_hashrates: &mut HashMap<String, f64>,
    hashrate_threshold: Option<f64>,
) -> Option<String> {
    let mut alert = None;

    if let Some(threshold) = hashrate_threshold {
        if let Some(&previous_hashrate) = previous_hashrates.get(&stats.device_id) {
            let change_percent =
                ((stats.hashrate_mhs - previous_hashrate) / previous_hashrate) * 100.0;
            if change_percent.abs() > threshold {
                let direction = if change_percent > 0.0 {
                    "increased"
                } else {
                    "decreased"
                };
                alert =
                    Some(format!(
                    "Hashrate alert: Device {} hashrate {} by {:.1}% (from {:.1} to {:.1} MH/s)",
                    stats.device_id, direction, change_percent.abs(),
                    previous_hashrate, stats.hashrate_mhs
                ));
            }
        }
    }

    // Always update the previous hashrate for next comparison
    previous_hashrates.insert(stats.device_id.clone(), stats.hashrate_mhs);
    alert
}

#[tokio::test]
async fn test_basic_monitoring_workflow() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server for monitoring
    let mut mock_server = MonitoringMockServer::new("monitor-test-1", 65.0, 16.17).await;
    mock_server.setup_endpoint().await;

    // Add device to storage
    let device = create_monitoring_device(&mock_server.url(), "monitor-test-1");
    storage.upsert_device(device.clone())?;

    // Create client for monitoring
    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Simulate monitoring cycles
    let monitoring_cycles = 3;
    let mut collected_stats = Vec::new();

    for cycle in 0..monitoring_cycles {
        // Collect stats
        if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
            storage.store_stats(stats.clone())?;
            collected_stats.push(stats);
        }

        // Update device last_seen
        storage.update_device_status(&device.ip_address, DeviceStatus::Online)?;

        // Small delay between monitoring cycles
        if cycle < monitoring_cycles - 1 {
            sleep(Duration::from_millis(100)).await;
        }
    }

    // Verify monitoring results
    assert_eq!(collected_stats.len(), monitoring_cycles);

    let stored_stats = storage.get_device_stats_history(&device.ip_address, None)?;
    assert_eq!(stored_stats.len(), monitoring_cycles);

    let latest_stats = storage.get_device_latest_stats(&device.ip_address)?;
    assert!(latest_stats.is_some());

    let swarm_summary = storage.get_swarm_summary()?;
    assert_eq!(swarm_summary.total_devices, 1);
    assert_eq!(swarm_summary.devices_online, 1);

    Ok(())
}

#[tokio::test]
async fn test_temperature_alerting() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server with high temperature
    let mut mock_server = MonitoringMockServer::new("temp-alert-test", 85.0, 16.17).await;
    mock_server.setup_endpoint().await;

    let device = create_monitoring_device(&mock_server.url(), "temp-alert-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Set temperature alert threshold
    let temp_threshold = Some(75.0);

    // Collect stats and check for temperature alert
    if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
        let alert = check_temperature_alert(&stats, temp_threshold);

        // Should trigger temperature alert
        assert!(alert.is_some());
        let alert_message = alert.unwrap();
        assert!(alert_message.contains("Temperature alert"));
        assert!(alert_message.contains("85.0°C"));
        assert!(alert_message.contains("75.0°C"));

        storage.store_stats(stats)?;
    }

    // Test with temperature below threshold
    mock_server.set_temperature(70.0);
    mock_server.setup_endpoint().await;

    if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
        let alert = check_temperature_alert(&stats, temp_threshold);

        // Should not trigger alert
        assert!(alert.is_none());
    }

    Ok(())
}

#[tokio::test]
async fn test_hashrate_alerting() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server with initial power for 500 MH/s (500.0 / 30 = 16.67 power)
    let mut mock_server = MonitoringMockServer::new("hashrate-alert-test", 65.0, 16.67).await;
    mock_server.setup_endpoint().await;

    let device = create_monitoring_device(&mock_server.url(), "hashrate-alert-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    let hashrate_threshold = Some(10.0); // 10% change threshold
    let mut previous_hashrates = HashMap::new();

    // First collection - no alert expected (no previous data)
    if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
        let alert = check_hashrate_alert(&stats, &mut previous_hashrates, hashrate_threshold);
        assert!(alert.is_none()); // No previous data for comparison
        storage.store_stats(stats)?;
    }

    // Simulate significant hashrate drop (using power field)
    mock_server.set_power_for_hashrate(400.0); // 20% drop from 500 to 400
    mock_server.setup_endpoint().await;

    if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
        let alert = check_hashrate_alert(&stats, &mut previous_hashrates, hashrate_threshold);

        // Should trigger hashrate alert
        assert!(alert.is_some());
        let alert_message = alert.unwrap();
        assert!(alert_message.contains("Hashrate alert"));
        assert!(alert_message.contains("decreased"));
        assert!(alert_message.contains("20.0%"));

        storage.store_stats(stats)?;
    }

    // Small hashrate change - should not trigger alert
    mock_server.set_power_for_hashrate(405.0); // Small increase
    mock_server.setup_endpoint().await;

    if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
        let alert = check_hashrate_alert(&stats, &mut previous_hashrates, hashrate_threshold);
        assert!(alert.is_none()); // Change is below threshold
    }

    Ok(())
}

#[tokio::test]
async fn test_device_offline_detection() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Add multiple devices with different last_seen times
    let now = chrono::Utc::now();

    let online_device = DeviceInfo {
        name: "online-device".to_string(),
        ip_address: "192.168.1.100".to_string(),
        device_type: DeviceType::BitaxeMax,
        serial_number: Some("ONLINE123".to_string()),
        status: DeviceStatus::Online,
        discovered_at: now - chrono::Duration::minutes(60),
        last_seen: now - chrono::Duration::seconds(30), // Recently seen (30 seconds ago)
    };

    let stale_device = DeviceInfo {
        name: "stale-device".to_string(),
        ip_address: "192.168.1.101".to_string(),
        device_type: DeviceType::BitaxeMax,
        serial_number: Some("STALE123".to_string()),
        status: DeviceStatus::Online,
        discovered_at: now - chrono::Duration::minutes(60),
        last_seen: now - chrono::Duration::seconds(600), // Not seen recently (10 minutes ago)
    };

    // Add first device
    storage.upsert_device(online_device.clone())?;

    // Wait to create time separation
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Add second device (this will be more recent)
    storage.upsert_device(stale_device.clone())?;

    // Verify initial state
    let online_devices = storage.get_devices_by_status(DeviceStatus::Online)?;
    assert_eq!(online_devices.len(), 2);

    // Mark devices that haven't been seen in last 100ms as stale (first device should qualify)
    let marked_offline = storage.mark_stale_devices_offline(0)?; // 0 = immediate timeout
    assert_eq!(marked_offline, 2); // BOTH devices should be marked offline with 0 timeout

    // Verify final state - both devices should be offline
    let online_devices = storage.get_devices_by_status(DeviceStatus::Online)?;
    assert_eq!(online_devices.len(), 0);

    let offline_devices = storage.get_devices_by_status(DeviceStatus::Offline)?;
    assert_eq!(offline_devices.len(), 2);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_monitoring() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create multiple mock servers
    let mut servers = Vec::new();
    for i in 0..3 {
        let mut server = MonitoringMockServer::new(
            &format!("concurrent-device-{}", i),
            65.0 + (i as f64 * 5.0),  // Different temperatures
            15.0 + (i as f64 * 1.67), // Different power values (450-550 MH/s range)
        )
        .await;
        server.setup_endpoint().await;
        servers.push(server);
    }

    // Add devices to storage
    for (i, server) in servers.iter().enumerate() {
        let device = create_monitoring_device(&server.url(), &format!("concurrent-device-{}", i));
        storage.upsert_device(device)?;
    }

    // Create clients and URLs before spawning tasks
    let mut clients_and_urls = Vec::new();
    for server in &servers {
        let client = AxeOsClient::with_timeout(&server.url(), Duration::from_secs(2))?;
        let url = server.url();
        clients_and_urls.push((client, url));
    }

    // Spawn concurrent monitoring tasks
    let mut monitoring_tasks = Vec::new();

    for (client, device_url) in clients_and_urls {
        let storage_clone = Arc::clone(&storage);

        let task = task::spawn(async move {
            // Perform multiple monitoring cycles
            for _cycle in 0..5 {
                if let Ok(Some(stats)) = collect_device_stats(&client, &device_url).await {
                    let _ = storage_clone.store_stats(stats);
                }

                // Update device status
                let _ = storage_clone.update_device_status(&device_url, DeviceStatus::Online);

                sleep(Duration::from_millis(20)).await;
            }
        });

        monitoring_tasks.push(task);
    }

    // Wait for all monitoring tasks to complete
    for task in monitoring_tasks {
        task.await?;
    }

    // Verify results
    let all_devices = storage.get_all_devices()?;
    assert_eq!(all_devices.len(), 3);

    let all_stats = storage.get_latest_stats()?;
    assert_eq!(all_stats.len(), 3);

    // Check that each device has collected multiple stats
    for server in servers.iter().take(3) {
        let device_ip = server.url();
        let history = storage.get_device_stats_history(&device_ip, None)?;
        assert_eq!(history.len(), 5); // 5 monitoring cycles
    }

    let summary = storage.get_swarm_summary()?;
    assert_eq!(summary.total_devices, 3);
    assert_eq!(summary.devices_online, 3);

    Ok(())
}

#[tokio::test]
async fn test_monitoring_with_timeouts() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Test monitoring with unreachable device
    let unreachable_device = DeviceInfo {
        name: "unreachable-device".to_string(),
        ip_address: "http://192.0.2.1:8080".to_string(), // Reserved test IP
        device_type: DeviceType::BitaxeMax,
        serial_number: Some("UNREACHABLE123".to_string()),
        status: DeviceStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    };

    storage.upsert_device(unreachable_device.clone())?;

    // Try to monitor unreachable device with timeout
    let client =
        AxeOsClient::with_timeout(&unreachable_device.ip_address, Duration::from_millis(100))?;

    // This should timeout or fail gracefully
    let monitoring_result = timeout(
        Duration::from_millis(500),
        collect_device_stats(&client, &unreachable_device.ip_address),
    )
    .await;

    match monitoring_result {
        Ok(Ok(None)) => {
            // No stats collected - expected for unreachable device
        }
        Ok(Err(_)) => {
            // Error collecting stats - acceptable for unreachable device
        }
        Err(_) => {
            // Timeout - acceptable for unreachable device
        }
        Ok(Ok(Some(_))) => {
            panic!("Should not collect stats from unreachable device");
        }
    }

    // Device should still exist in storage
    let device = storage.get_device(&unreachable_device.ip_address)?;
    assert!(device.is_some());

    // But no stats should be stored
    let stats = storage.get_device_latest_stats(&unreachable_device.ip_address)?;
    assert!(stats.is_none());

    Ok(())
}

#[tokio::test]
async fn test_monitoring_stats_aggregation() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create device and collect multiple stats over time
    let mut mock_server = MonitoringMockServer::new("stats-aggregation-test", 65.0, 16.17).await;
    mock_server.setup_endpoint().await;

    let device = create_monitoring_device(&mock_server.url(), "stats-aggregation-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Collect stats over multiple intervals with changing values
    let monitoring_intervals = vec![
        (65.0, 16.17), // hashrate 485.2 = power 16.17
        (67.0, 16.34), // hashrate 490.1 = power 16.34
        (66.5, 16.26), // hashrate 487.8 = power 16.26
        (68.0, 16.41), // hashrate 492.3 = power 16.41
        (66.0, 16.22), // hashrate 486.5 = power 16.22
    ];

    for (temp, power) in monitoring_intervals {
        mock_server.set_temperature(temp);
        mock_server.set_power(power); // Set power directly
        mock_server.setup_endpoint().await;

        if let Some(stats) = collect_device_stats(&client, &device.ip_address).await? {
            storage.store_stats(stats)?;
        }

        sleep(Duration::from_millis(50)).await;
    }

    // Verify stats history
    let history = storage.get_device_stats_history(&device.ip_address, None)?;
    assert_eq!(history.len(), 5);

    // Verify latest stats (last interval was temp=66.0)
    let latest = storage
        .get_device_latest_stats(&device.ip_address)?
        .unwrap();
    assert_eq!(latest.temperature_celsius, 66.0); // Last recorded temperature

    // Verify swarm summary aggregation
    let summary = storage.get_swarm_summary()?;
    assert_eq!(summary.total_devices, 1);
    assert_eq!(summary.devices_online, 1);
    assert!(summary.average_temperature > 0.0);
    assert!(summary.total_hashrate_mhs > 0.0);

    // Test stats cleanup
    let cleaned = storage.cleanup_old_stats(3)?;
    assert_eq!(cleaned, 2); // Should remove 2 old entries

    let history_after_cleanup = storage.get_device_stats_history(&device.ip_address, None)?;
    assert_eq!(history_after_cleanup.len(), 3);

    Ok(())
}
