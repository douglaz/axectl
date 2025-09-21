use anyhow::{Context, Result, anyhow};
use reqwest::{Client, ClientBuilder};
use std::time::Duration;
use url::Url;

use super::models::*;

#[derive(Debug, Clone)]
pub struct AxeOsClient {
    client: Client,
    base_url: String,
    timeout: Duration,
}

impl AxeOsClient {
    pub fn new(ip_address: &str) -> Result<Self> {
        Self::with_timeout(ip_address, Duration::from_secs(60))
    }

    pub fn with_timeout(ip_address: &str, timeout: Duration) -> Result<Self> {
        let base_url = if ip_address.starts_with("http://") || ip_address.starts_with("https://") {
            ip_address.to_string()
        } else {
            format!("http://{ip_address}")
        };

        // Validate URL
        Url::parse(&base_url).context("Invalid IP address or URL")?;

        let client = ClientBuilder::new()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(5))
            .user_agent("axectl/0.1.0")
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            base_url,
            timeout,
        })
    }

    // Test if the device is reachable and running AxeOS
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/system/info", self.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    // Get system information with device type detection
    pub async fn get_system_info(&self) -> Result<SystemInfoResponse> {
        let url = format!("{}/api/system/info", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to device")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let json_text = response
            .text()
            .await
            .context("Failed to get response text")?;

        // Parse using device detection
        let device_response =
            DeviceResponse::from_json(&json_text).context("Failed to parse device response")?;

        Ok(device_response.to_unified_info())
    }

    /// Get complete device info with proper device type detection
    pub async fn get_complete_device_info(&self) -> Result<(SystemInfoResponse, DeviceType)> {
        let url = format!("{}/api/system/info", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to device")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let json_text = response
            .text()
            .await
            .context("Failed to get response text")?;

        // Parse using device detection
        let device_response =
            DeviceResponse::from_json(&json_text).context("Failed to parse device response")?;

        let device_type = device_response.get_device_type();
        let system_info = device_response.to_unified_info();

        Ok((system_info, device_type))
    }

    // Get system statistics with device type detection
    pub async fn get_system_stats(&self) -> Result<SystemStatsResponse> {
        let url = format!("{}/api/system/info", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to device")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let json_text = response
            .text()
            .await
            .context("Failed to get response text")?;

        // Parse using device detection
        let device_response =
            DeviceResponse::from_json(&json_text).context("Failed to parse device response")?;

        Ok(device_response.to_unified_stats())
    }

    // Get dashboard statistics (usually more detailed)
    pub async fn get_dashboard_stats(&self) -> Result<SystemStatsResponse> {
        let url = format!("{}/api/system/statistics/dashboard", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to device")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let stats: SystemStatsResponse = response
            .json()
            .await
            .context("Failed to parse dashboard stats response")?;

        Ok(stats)
    }

    // Get ASIC information
    pub async fn get_asic_info(&self) -> Result<AsicResponse> {
        let url = format!("{}/api/system/asic", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send request to device")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let asic: AsicResponse = response
            .json()
            .await
            .context("Failed to parse ASIC info response")?;

        Ok(asic)
    }

    // Update system settings
    pub async fn update_system(&self, request: SystemUpdateRequest) -> Result<CommandResult> {
        let url = format!("{}/api/system", self.base_url);

        let response = self
            .client
            .patch(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send update request to device")?;

        let result = if response.status().is_success() {
            CommandResult {
                success: true,
                message: "System settings updated successfully".to_string(),
                data: None,
                timestamp: chrono::Utc::now(),
            }
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            CommandResult {
                success: false,
                message: format!("Failed to update system settings: {error_text}"),
                data: None,
                timestamp: chrono::Utc::now(),
            }
        };

        Ok(result)
    }

    // Restart the system
    pub async fn restart_system(&self) -> Result<CommandResult> {
        let url = format!("{}/api/system/restart", self.base_url);

        let response = self
            .client
            .post(&url)
            .send()
            .await
            .context("Failed to send restart request to device")?;

        let result = if response.status().is_success() {
            CommandResult {
                success: true,
                message: "System restart initiated".to_string(),
                data: None,
                timestamp: chrono::Utc::now(),
            }
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            CommandResult {
                success: false,
                message: format!("Failed to restart system: {error_text}"),
                data: None,
                timestamp: chrono::Utc::now(),
            }
        };

        Ok(result)
    }

    // Scan for WiFi networks
    pub async fn scan_wifi(&self) -> Result<WifiScanResponse> {
        let url = format!("{}/api/system/wifi/scan", self.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to send WiFi scan request to device")?;

        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }

        let scan_result: WifiScanResponse = response
            .json()
            .await
            .context("Failed to parse WiFi scan response")?;

        Ok(scan_result)
    }

    // Update firmware via OTA
    pub async fn update_firmware(&self, firmware_url: &str) -> Result<CommandResult> {
        let url = format!("{}/api/system/OTA", self.base_url);

        let body = serde_json::json!({
            "url": firmware_url
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send firmware update request to device")?;

        let result = if response.status().is_success() {
            CommandResult {
                success: true,
                message: "Firmware update initiated".to_string(),
                data: Some(serde_json::json!({"firmware_url": firmware_url})),
                timestamp: chrono::Utc::now(),
            }
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            CommandResult {
                success: false,
                message: format!("Failed to update firmware: {error_text}"),
                data: None,
                timestamp: chrono::Utc::now(),
            }
        };

        Ok(result)
    }

    // Update AxeOS web interface
    pub async fn update_axeos(&self, axeos_url: &str) -> Result<CommandResult> {
        let url = format!("{}/api/system/OTAWWW", self.base_url);

        let body = serde_json::json!({
            "url": axeos_url
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send AxeOS update request to device")?;

        let result = if response.status().is_success() {
            CommandResult {
                success: true,
                message: "AxeOS update initiated".to_string(),
                data: Some(serde_json::json!({"axeos_url": axeos_url})),
                timestamp: chrono::Utc::now(),
            }
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            CommandResult {
                success: false,
                message: format!("Failed to update AxeOS: {error_text}"),
                data: None,
                timestamp: chrono::Utc::now(),
            }
        };

        Ok(result)
    }

    // Helper method to set fan speed
    pub async fn set_fan_speed(&self, speed_percent: u8) -> Result<CommandResult> {
        if speed_percent > 100 {
            return Ok(CommandResult {
                success: false,
                message: "Fan speed must be between 0 and 100 percent".to_string(),
                data: None,
                timestamp: chrono::Utc::now(),
            });
        }

        let request = SystemUpdateRequest {
            fan_speed: Some(speed_percent as u32),
            ..Default::default()
        };

        self.update_system(request).await
    }

    // Helper method to get complete device info and stats
    pub async fn get_complete_info(&self) -> Result<(SystemInfoResponse, SystemStatsResponse)> {
        let (info_result, stats_result) =
            tokio::try_join!(self.get_system_info(), self.get_system_stats())?;

        Ok((info_result, stats_result))
    }

    // Get the base URL for this client
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    // Get the timeout for this client
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() -> Result<()> {
        let client = AxeOsClient::new("192.168.1.100")?;
        assert_eq!(client.base_url(), "http://192.168.1.100");

        let client = AxeOsClient::new("http://192.168.1.100")?;
        assert_eq!(client.base_url(), "http://192.168.1.100");

        Ok(())
    }

    #[test]
    fn test_invalid_url() {
        let result = AxeOsClient::new("not a valid url");
        assert!(result.is_err());
    }

    #[test]
    fn test_fan_speed_validation() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let client = AxeOsClient::new("http://example.com").unwrap();
            let result = client.set_fan_speed(150).await.unwrap();
            assert!(!result.success);
            assert!(result.message.contains("must be between 0 and 100"));
        });
    }

    #[tokio::test]
    async fn test_get_system_info_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/system/info")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ASICModel": "BM1368",
                "boardVersion": "204",
                "version": "2.0.0",
                "macAddr": "AA:BB:CC:DD:EE:FF",
                "hostname": "bitaxe-test",
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
            }"#,
            )
            .create_async()
            .await;

        let client = AxeOsClient::new(&server.url()).unwrap();
        let result = client.get_system_info().await;

        mock.assert_async().await;
        assert!(result.is_ok());

        let info = result.unwrap();
        assert_eq!(info.hostname, "bitaxe-test");
        assert_eq!(info.asic_model, "BM1368");
    }

    #[tokio::test]
    async fn test_get_system_info_network_error() {
        let client = AxeOsClient::new("http://nonexistent.local").unwrap();
        let result = client.get_system_info().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_fan_speed_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("PATCH", "/api/system")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success": true, "message": "Fan speed set to 75%"}"#)
            .create_async()
            .await;

        let client = AxeOsClient::new(&server.url()).unwrap();
        let result = client.set_fan_speed(75).await.unwrap();

        mock.assert_async().await;
        assert!(result.success);
        assert!(
            result
                .message
                .contains("System settings updated successfully")
        );
    }

    #[tokio::test]
    async fn test_http_error_codes() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/system/info")
            .with_status(404)
            .create_async()
            .await;

        let client = AxeOsClient::new(&server.url()).unwrap();
        let result = client.get_system_info().await;

        mock.assert_async().await;
        assert!(result.is_err());
    }
}
