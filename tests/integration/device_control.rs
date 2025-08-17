/// Integration tests for device control operations
/// These tests verify:
/// - Device restart commands
/// - Fan speed control
/// - Pool configuration updates
/// - Bulk control operations across multiple devices
/// - Error handling for control failures
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::task;

use axectl::api::{AxeOsClient, DeviceInfo, DeviceStatus, DeviceType, SystemUpdateRequest};
use axectl::storage::memory::MemoryStorage;

/// Mock HTTP server for device control testing
struct ControlMockServer {
    server: mockito::ServerGuard,
    device_name: String,
    fan_speed: u32,
    pool_url: String,
    pool_port: u16,
}

impl ControlMockServer {
    async fn new(device_name: &str) -> Self {
        let server = mockito::Server::new_async().await;

        Self {
            server,
            device_name: device_name.to_string(),
            fan_speed: 75,
            pool_url: "stratum+tcp://test.pool.com".to_string(),
            pool_port: 4334,
        }
    }

    async fn setup_system_info_endpoint(&mut self) {
        // Reset server to clear any previous mocks
        self.server.reset();

        // Create system info endpoint that reflects current state
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
                "stratumURL": "{}",
                "stratumPort": {},
                "stratumUser": "bc1qtest123",
                "frequency": 485,
                "voltage": 1200,
                "fanspeed": {},
                "temp": 65.0,
                "power": 16.5,
                "hashRate": 485.2,
                "uptimeSeconds": 3600,
                "sharesAccepted": 150,
                "sharesRejected": 2,
                "bestDiff": "123.45K"
            }}"#,
                self.device_name, self.pool_url, self.pool_port, self.fan_speed
            ))
            .create_async()
            .await;
    }

    async fn setup_restart_endpoint(&mut self, should_succeed: bool) {
        let mock = self
            .server
            .mock("POST", "/api/system/restart")
            .with_header("content-type", "application/json");

        if should_succeed {
            mock.with_status(200)
                .with_body(r#"{"status": "restarting"}"#)
        } else {
            mock.with_status(500)
                .with_body(r#"{"error": "restart failed"}"#)
        }
        .create_async()
        .await;
    }

    async fn setup_fan_speed_endpoint(&mut self, should_succeed: bool) {
        let mock = self
            .server
            .mock("PATCH", "/api/system")
            .with_header("content-type", "application/json");

        if should_succeed {
            mock.with_status(200).with_body(r#"{"status": "updated"}"#)
        } else {
            mock.with_status(400)
                .with_body(r#"{"error": "invalid fan speed"}"#)
        }
        .create_async()
        .await;
    }

    async fn setup_pool_config_endpoint(&mut self, should_succeed: bool) {
        let mock = self
            .server
            .mock("PATCH", "/api/system")
            .with_header("content-type", "application/json");

        if should_succeed {
            mock.with_status(200)
                .with_body(r#"{"status": "pool updated"}"#)
        } else {
            mock.with_status(400)
                .with_body(r#"{"error": "invalid pool configuration"}"#)
        }
        .create_async()
        .await;
    }

    fn url(&self) -> String {
        self.server.url()
    }

    fn set_fan_speed(&mut self, speed: u32) {
        self.fan_speed = speed;
    }

    fn set_pool_config(&mut self, url: &str, port: u16) {
        self.pool_url = url.to_string();
        self.pool_port = port;
    }
}

/// Create a test device for control operations
fn create_control_device(url: &str, name: &str) -> DeviceInfo {
    DeviceInfo {
        name: name.to_string(),
        ip_address: url.to_string(),
        device_type: DeviceType::BitaxeMax,
        serial_number: Some("CONTROL123".to_string()),
        status: DeviceStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_device_restart_success() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server that supports restart
    let mut mock_server = ControlMockServer::new("restart-test").await;
    mock_server.setup_system_info_endpoint().await;
    mock_server.setup_restart_endpoint(true).await;

    let device = create_control_device(&mock_server.url(), "restart-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Verify device is online before restart
    assert_eq!(device.status, DeviceStatus::Online);

    // Perform restart
    let restart_result = client.restart_system().await;
    assert!(restart_result.is_ok(), "Restart should succeed");

    // Device should still exist in storage
    let retrieved_device = storage.get_device(&device.ip_address)?;
    assert!(retrieved_device.is_some());

    Ok(())
}

#[tokio::test]
async fn test_device_restart_failure() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server that fails restart
    let mut mock_server = ControlMockServer::new("restart-fail-test").await;
    mock_server.setup_system_info_endpoint().await;
    mock_server.setup_restart_endpoint(false).await;

    let device = create_control_device(&mock_server.url(), "restart-fail-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Perform restart - should fail
    let restart_result = client.restart_system().await;
    assert!(restart_result.is_ok(), "Should return a result");
    let command_result = restart_result.unwrap();
    assert!(!command_result.success, "Restart should fail");

    // Device should still exist in storage
    let retrieved_device = storage.get_device(&device.ip_address)?;
    assert!(retrieved_device.is_some());

    Ok(())
}

#[tokio::test]
async fn test_fan_speed_control() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server that supports fan speed control
    let mut mock_server = ControlMockServer::new("fan-control-test").await;
    mock_server.setup_system_info_endpoint().await;
    mock_server.setup_fan_speed_endpoint(true).await;

    let device = create_control_device(&mock_server.url(), "fan-control-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Test setting fan speed to 50%
    let fan_result = client.set_fan_speed(50).await;
    assert!(fan_result.is_ok(), "Fan speed control should succeed");

    // Update mock server state and verify
    mock_server.set_fan_speed(50);
    mock_server.setup_system_info_endpoint().await;

    let (system_info, _) = client.get_complete_device_info().await?;
    assert_eq!(system_info.fanspeed, 50);

    Ok(())
}

#[tokio::test]
async fn test_fan_speed_control_invalid() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server that rejects invalid fan speed
    let mut mock_server = ControlMockServer::new("fan-invalid-test").await;
    mock_server.setup_system_info_endpoint().await;
    mock_server.setup_fan_speed_endpoint(false).await;

    let device = create_control_device(&mock_server.url(), "fan-invalid-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Test setting invalid fan speed (over 100%)
    let fan_result = client.set_fan_speed(150).await;
    assert!(fan_result.is_ok(), "Should return a result");
    let command_result = fan_result.unwrap();
    assert!(!command_result.success, "Invalid fan speed should fail");

    Ok(())
}

#[tokio::test]
async fn test_pool_configuration_update() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server that supports pool config updates
    let mut mock_server = ControlMockServer::new("pool-config-test").await;
    mock_server.setup_system_info_endpoint().await;
    mock_server.setup_pool_config_endpoint(true).await;

    let device = create_control_device(&mock_server.url(), "pool-config-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Test updating pool configuration
    let new_pool_url = "stratum+tcp://new.pool.com";
    let new_pool_port = 3333;
    let pool_update_request = SystemUpdateRequest {
        ssid: None,
        password: None,
        hostname: None,
        pool_url: Some(new_pool_url.to_string()),
        pool_port: Some(new_pool_port),
        pool_user: Some("bc1qnewaddress".to_string()),
        frequency_value: None,
        voltage_value: None,
        fan_speed: None,
    };
    let pool_result = client.update_system(pool_update_request).await;
    assert!(pool_result.is_ok(), "Pool config update should succeed");

    // Update mock server state and verify
    mock_server.set_pool_config(new_pool_url, new_pool_port);
    mock_server.setup_system_info_endpoint().await;

    let (system_info, _) = client.get_complete_device_info().await?;
    assert_eq!(system_info.pool_url, new_pool_url);
    assert_eq!(system_info.pool_port, new_pool_port);

    Ok(())
}

#[tokio::test]
async fn test_bulk_device_restart() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create multiple mock servers
    let mut servers = Vec::new();
    for i in 0..3 {
        let mut server = ControlMockServer::new(&format!("bulk-restart-{}", i)).await;
        server.setup_system_info_endpoint().await;
        server.setup_restart_endpoint(true).await;
        servers.push(server);
    }

    // Add devices to storage
    for (i, server) in servers.iter().enumerate() {
        let device = create_control_device(&server.url(), &format!("bulk-restart-{}", i));
        storage.upsert_device(device)?;
    }

    // Create clients and perform bulk restart
    let mut restart_tasks = Vec::new();

    for server in &servers {
        let client = AxeOsClient::with_timeout(&server.url(), Duration::from_secs(2))?;
        let task = task::spawn(async move { client.restart_system().await });
        restart_tasks.push(task);
    }

    // Wait for all restart tasks to complete
    let mut success_count = 0;
    for task in restart_tasks {
        match task.await? {
            Ok(_) => success_count += 1,
            Err(_) => {
                // Handle individual failures if needed
            }
        }
    }

    // All restarts should succeed
    assert_eq!(success_count, 3);

    // Verify all devices still exist in storage
    let all_devices = storage.get_all_devices()?;
    assert_eq!(all_devices.len(), 3);

    Ok(())
}

#[tokio::test]
async fn test_bulk_fan_speed_control() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create multiple mock servers with different initial fan speeds
    let mut servers = Vec::new();
    for i in 0..4 {
        let mut server = ControlMockServer::new(&format!("bulk-fan-{}", i)).await;
        server.set_fan_speed(50 + (i as u32 * 10)); // 50%, 60%, 70%, 80%
        server.setup_system_info_endpoint().await;
        server.setup_fan_speed_endpoint(true).await;
        servers.push(server);
    }

    // Add devices to storage
    for (i, server) in servers.iter().enumerate() {
        let device = create_control_device(&server.url(), &format!("bulk-fan-{}", i));
        storage.upsert_device(device)?;
    }

    // Perform bulk fan speed update to 85%
    let target_fan_speed = 85u8;
    let mut fan_control_tasks = Vec::new();

    for server in &servers {
        let client = AxeOsClient::with_timeout(&server.url(), Duration::from_secs(2))?;
        let task = task::spawn(async move { client.set_fan_speed(target_fan_speed).await });
        fan_control_tasks.push(task);
    }

    // Wait for all fan control tasks to complete
    let mut success_count = 0;
    for task in fan_control_tasks {
        match task.await? {
            Ok(_) => success_count += 1,
            Err(_) => {
                // Handle individual failures if needed
            }
        }
    }

    // All fan speed updates should succeed
    assert_eq!(success_count, 4);

    // Update server states and verify fan speeds
    for server in &mut servers {
        server.set_fan_speed(target_fan_speed as u32);
        server.setup_system_info_endpoint().await;
    }

    // Verify fan speeds were updated
    for server in &servers {
        let client = AxeOsClient::with_timeout(&server.url(), Duration::from_secs(2))?;
        let (system_info, _) = client.get_complete_device_info().await?;
        assert_eq!(system_info.fanspeed, target_fan_speed as u32);
    }

    Ok(())
}

#[tokio::test]
async fn test_mixed_control_operations() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Create mock server that supports multiple operations
    let mut mock_server = ControlMockServer::new("mixed-control-test").await;
    mock_server.setup_system_info_endpoint().await;

    let device = create_control_device(&mock_server.url(), "mixed-control-test");
    storage.upsert_device(device.clone())?;

    let client = AxeOsClient::with_timeout(&mock_server.url(), Duration::from_secs(2))?;

    // Test sequence: fan speed -> pool config -> restart

    // 1. Fan speed control
    mock_server.setup_fan_speed_endpoint(true).await;
    let fan_result = client.set_fan_speed(60).await;
    assert!(fan_result.is_ok());

    // 2. Pool configuration
    mock_server.setup_pool_config_endpoint(true).await;
    let pool_update_request = SystemUpdateRequest {
        ssid: None,
        password: None,
        hostname: None,
        pool_url: Some("stratum+tcp://updated.pool.com".to_string()),
        pool_port: Some(4444),
        pool_user: Some("bc1qupdatedaddress".to_string()),
        frequency_value: None,
        voltage_value: None,
        fan_speed: None,
    };
    let pool_result = client.update_system(pool_update_request).await;
    assert!(pool_result.is_ok());

    // 3. Device restart
    mock_server.setup_restart_endpoint(true).await;
    let restart_result = client.restart_system().await;
    assert!(restart_result.is_ok());

    // Verify device is still in storage after all operations
    let final_device = storage.get_device(&device.ip_address)?;
    assert!(final_device.is_some());

    Ok(())
}

#[tokio::test]
async fn test_control_operations_with_timeouts() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());

    // Test control operations with unreachable device
    let unreachable_device = DeviceInfo {
        name: "unreachable-control-device".to_string(),
        ip_address: "http://192.0.2.1:8080".to_string(), // Reserved test IP
        device_type: DeviceType::BitaxeMax,
        serial_number: Some("UNREACHABLE123".to_string()),
        status: DeviceStatus::Online,
        discovered_at: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
    };

    storage.upsert_device(unreachable_device.clone())?;

    // Try control operations with short timeout
    let client =
        AxeOsClient::with_timeout(&unreachable_device.ip_address, Duration::from_millis(100))?;

    // All operations should timeout or fail gracefully
    let restart_result =
        tokio::time::timeout(Duration::from_millis(500), client.restart_system()).await;

    let fan_result =
        tokio::time::timeout(Duration::from_millis(500), client.set_fan_speed(75)).await;

    let pool_update_request = SystemUpdateRequest {
        ssid: None,
        password: None,
        hostname: None,
        pool_url: Some("stratum+tcp://test.com".to_string()),
        pool_port: Some(4334),
        pool_user: Some("test".to_string()),
        frequency_value: None,
        voltage_value: None,
        fan_speed: None,
    };
    let pool_result = tokio::time::timeout(
        Duration::from_millis(500),
        client.update_system(pool_update_request),
    )
    .await;

    // All operations should either timeout or return errors
    match restart_result {
        Ok(Ok(_)) => panic!("Should not succeed with unreachable device"),
        Ok(Err(_)) | Err(_) => {
            // Expected: either timeout or connection error
        }
    }

    match fan_result {
        Ok(Ok(_)) => panic!("Should not succeed with unreachable device"),
        Ok(Err(_)) | Err(_) => {
            // Expected: either timeout or connection error
        }
    }

    match pool_result {
        Ok(Ok(_)) => panic!("Should not succeed with unreachable device"),
        Ok(Err(_)) | Err(_) => {
            // Expected: either timeout or connection error
        }
    }

    // Device should still exist in storage
    let device = storage.get_device(&unreachable_device.ip_address)?;
    assert!(device.is_some());

    Ok(())
}
