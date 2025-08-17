use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub ip_address: String,
    pub device_type: DeviceType,
    pub serial_number: Option<String>,
    pub status: DeviceStatus,
    pub discovered_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceType {
    #[serde(rename = "bitaxe_ultra")]
    BitaxeUltra,
    #[serde(rename = "bitaxe_max")]
    BitaxeMax,
    #[serde(rename = "bitaxe_gamma")]
    BitaxeGamma,
    #[serde(rename = "nerdqaxe_plus")]
    NerdqaxePlus,
    #[serde(rename = "unknown")]
    Unknown,
}

impl DeviceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeviceType::BitaxeUltra => "Bitaxe Ultra",
            DeviceType::BitaxeMax => "Bitaxe Max",
            DeviceType::BitaxeGamma => "Bitaxe Gamma",
            DeviceType::NerdqaxePlus => "NerdQaxe++",
            DeviceType::Unknown => "Unknown",
        }
    }

    /// Get CLI-friendly name for device type filtering
    pub fn cli_name(&self) -> &'static str {
        match self {
            DeviceType::BitaxeUltra => "bitaxe-ultra",
            DeviceType::BitaxeMax => "bitaxe-max",
            DeviceType::BitaxeGamma => "bitaxe-gamma",
            DeviceType::NerdqaxePlus => "nerdqaxe",
            DeviceType::Unknown => "unknown",
        }
    }

    /// Parse CLI name to DeviceType
    pub fn from_cli_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "bitaxe-ultra" | "bitaxe_ultra" => Some(DeviceType::BitaxeUltra),
            "bitaxe-max" | "bitaxe_max" => Some(DeviceType::BitaxeMax),
            "bitaxe-gamma" | "bitaxe_gamma" => Some(DeviceType::BitaxeGamma),
            "nerdqaxe" | "nerdqaxe-plus" | "nerdqaxe_plus" => Some(DeviceType::NerdqaxePlus),
            "unknown" => Some(DeviceType::Unknown),
            "bitaxe" => None, // Ambiguous - let user specify which bitaxe
            _ => None,
        }
    }

    /// Get all device types for iteration
    pub fn all_types() -> Vec<Self> {
        vec![
            DeviceType::BitaxeUltra,
            DeviceType::BitaxeMax,
            DeviceType::BitaxeGamma,
            DeviceType::NerdqaxePlus,
            DeviceType::Unknown,
        ]
    }

    /// Check if device type matches a category filter
    pub fn matches_filter(&self, filter: &str) -> bool {
        match filter.to_lowercase().as_str() {
            "all" => true,
            "bitaxe" => matches!(
                self,
                DeviceType::BitaxeUltra | DeviceType::BitaxeMax | DeviceType::BitaxeGamma
            ),
            _ => self.cli_name() == filter.to_lowercase(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceStatus {
    #[serde(rename = "online")]
    Online,
    #[serde(rename = "offline")]
    Offline,
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStats {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub hashrate_mhs: f64,
    pub temperature_celsius: f64,
    pub power_watts: f64,
    pub fan_speed_rpm: u32,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub uptime_seconds: u64,
    pub pool_url: Option<String>,
    pub wifi_rssi: Option<i32>,
    pub voltage: Option<f64>,
    pub frequency: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmSummary {
    pub total_devices: usize,
    pub devices_online: usize,
    pub devices_offline: usize,
    pub total_hashrate_mhs: f64,
    pub total_power_watts: f64,
    pub average_temperature: f64,
    pub total_shares_accepted: u64,
    pub total_shares_rejected: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub device_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSummary {
    pub device_type: DeviceType,
    pub type_name: String,
    pub total_devices: usize,
    pub devices_online: usize,
    pub devices_offline: usize,
    pub total_hashrate_mhs: f64,
    pub total_power_watts: f64,
    pub average_temperature: f64,
    pub total_shares_accepted: u64,
    pub total_shares_rejected: u64,
    pub timestamp: DateTime<Utc>,
}

// API Response Models (matches AxeOS API)

/// Unified device info for internal use - converted from device-specific responses
#[derive(Debug, Clone)]
pub struct SystemInfoResponse {
    pub asic_model: String,
    pub board_version: String,
    pub firmware_version: String,
    pub mac_address: String,
    pub hostname: String,
    pub wifi_ssid: Option<String>,
    pub wifi_status: Option<String>,
    pub wifi_rssi: Option<i32>,
    pub pool_url: String,
    pub pool_port: u16,
    pub pool_user: String,
    pub frequency: u32,
    pub voltage: f64,
    pub fanspeed: u32,
    pub temp: f64,
    pub power: f64,
    pub running_time: u64,
}

/// Unified device stats for internal use - converted from device-specific responses
#[derive(Debug, Clone, Deserialize)]
pub struct SystemStatsResponse {
    pub hashrate: f64,
    pub temp: f64,
    pub power: f64,
    pub fanspeed: u32,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub uptime: u64,
    pub best_difficulty: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AsicResponse {
    #[serde(rename = "frequency")]
    pub frequency: u32,
    #[serde(rename = "voltage")]
    pub voltage: f64,
    #[serde(rename = "asic_count")]
    pub asic_count: u32,
    #[serde(rename = "small_core_count")]
    pub small_core_count: Option<u32>,
    #[serde(rename = "large_core_count")]
    pub large_core_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemUpdateRequest {
    #[serde(rename = "ssid", skip_serializing_if = "Option::is_none")]
    pub ssid: Option<String>,
    #[serde(rename = "password", skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(rename = "hostname", skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(rename = "poolurl", skip_serializing_if = "Option::is_none")]
    pub pool_url: Option<String>,
    #[serde(rename = "poolport", skip_serializing_if = "Option::is_none")]
    pub pool_port: Option<u16>,
    #[serde(rename = "pooluser", skip_serializing_if = "Option::is_none")]
    pub pool_user: Option<String>,
    #[serde(rename = "frequencyvalue", skip_serializing_if = "Option::is_none")]
    pub frequency_value: Option<u32>,
    #[serde(rename = "voltagevalue", skip_serializing_if = "Option::is_none")]
    pub voltage_value: Option<f64>,
    #[serde(rename = "fanspeed", skip_serializing_if = "Option::is_none")]
    pub fan_speed: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WifiScanResponse {
    pub networks: Vec<WifiNetwork>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub rssi: i32,
    pub channel: u8,
    pub encryption: String,
}

// Helper functions for conversions

impl From<&SystemInfoResponse> for DeviceType {
    fn from(info: &SystemInfoResponse) -> Self {
        match info.asic_model.to_lowercase().as_str() {
            s if s.contains("bm1366") => DeviceType::BitaxeUltra,
            s if s.contains("bm1368") => DeviceType::BitaxeMax,
            s if s.contains("bm1370") => DeviceType::BitaxeGamma,
            s if s.contains("s21") || s.contains("nerdqaxe") => DeviceType::NerdqaxePlus,
            _ => DeviceType::Unknown,
        }
    }
}

impl DeviceStats {
    pub fn from_api_responses(
        device_id: String,
        info: &SystemInfoResponse,
        stats: &SystemStatsResponse,
    ) -> Self {
        Self {
            device_id,
            timestamp: Utc::now(),
            hashrate_mhs: stats.hashrate,
            temperature_celsius: stats.temp,
            power_watts: stats.power,
            fan_speed_rpm: stats.fanspeed,
            shares_accepted: stats.shares_accepted,
            shares_rejected: stats.shares_rejected,
            uptime_seconds: stats.uptime,
            pool_url: Some(format!("{}:{}", info.pool_url, info.pool_port)),
            wifi_rssi: info.wifi_rssi,
            voltage: Some(info.voltage),
            frequency: Some(info.frequency),
        }
    }
}

impl SwarmSummary {
    pub fn from_devices_and_stats(devices: &[DeviceInfo], stats: &[DeviceStats]) -> Self {
        let devices_online = devices
            .iter()
            .filter(|d| matches!(d.status, DeviceStatus::Online))
            .count();
        let devices_offline = devices.len() - devices_online;

        let total_hashrate_mhs = stats.iter().map(|s| s.hashrate_mhs).sum();
        let total_power_watts = stats.iter().map(|s| s.power_watts).sum();
        let average_temperature = if !stats.is_empty() {
            stats.iter().map(|s| s.temperature_celsius).sum::<f64>() / stats.len() as f64
        } else {
            0.0
        };
        let total_shares_accepted = stats.iter().map(|s| s.shares_accepted).sum();
        let total_shares_rejected = stats.iter().map(|s| s.shares_rejected).sum();

        Self {
            total_devices: devices.len(),
            devices_online,
            devices_offline,
            total_hashrate_mhs,
            total_power_watts,
            average_temperature,
            total_shares_accepted,
            total_shares_rejected,
            timestamp: Utc::now(),
        }
    }
}

impl DeviceGroup {
    pub fn new(name: String, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            description,
            device_ids: Vec::new(),
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
        }
    }

    pub fn add_device(&mut self, device_id: String) {
        if !self.device_ids.contains(&device_id) {
            self.device_ids.push(device_id);
            self.updated_at = Utc::now();
        }
    }

    pub fn remove_device(&mut self, device_id: &str) -> bool {
        if let Some(pos) = self.device_ids.iter().position(|id| id == device_id) {
            self.device_ids.remove(pos);
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
            self.updated_at = Utc::now();
        }
    }

    pub fn remove_tag(&mut self, tag: &str) -> bool {
        if let Some(pos) = self.tags.iter().position(|t| t == tag) {
            self.tags.remove(pos);
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }
}

impl TypeSummary {
    pub fn from_devices_and_stats(
        device_type: DeviceType,
        devices: &[DeviceInfo],
        stats: &[DeviceStats],
    ) -> Self {
        // Filter devices and stats to only include those of this type
        let type_devices: Vec<&DeviceInfo> = devices
            .iter()
            .filter(|d| d.device_type == device_type)
            .collect();

        let type_stats: Vec<&DeviceStats> = stats
            .iter()
            .filter(|s| {
                // Find device by ID to check type
                devices
                    .iter()
                    .any(|d| d.ip_address == s.device_id && d.device_type == device_type)
            })
            .collect();

        let devices_online = type_devices
            .iter()
            .filter(|d| matches!(d.status, DeviceStatus::Online))
            .count();
        let devices_offline = type_devices.len() - devices_online;

        let total_hashrate_mhs = type_stats.iter().map(|s| s.hashrate_mhs).sum();
        let total_power_watts = type_stats.iter().map(|s| s.power_watts).sum();
        let average_temperature = if !type_stats.is_empty() {
            type_stats
                .iter()
                .map(|s| s.temperature_celsius)
                .sum::<f64>()
                / type_stats.len() as f64
        } else {
            0.0
        };
        let total_shares_accepted = type_stats.iter().map(|s| s.shares_accepted).sum();
        let total_shares_rejected = type_stats.iter().map(|s| s.shares_rejected).sum();

        Self {
            device_type: device_type.clone(),
            type_name: device_type.as_str().to_string(),
            total_devices: type_devices.len(),
            devices_online,
            devices_offline,
            total_hashrate_mhs,
            total_power_watts,
            average_temperature,
            total_shares_accepted,
            total_shares_rejected,
            timestamp: Utc::now(),
        }
    }

    /// Get type summaries for all device types that have devices
    pub fn from_all_devices_and_stats(devices: &[DeviceInfo], stats: &[DeviceStats]) -> Vec<Self> {
        DeviceType::all_types()
            .into_iter()
            .map(|device_type| Self::from_devices_and_stats(device_type, devices, stats))
            .filter(|summary| summary.total_devices > 0) // Only include types with devices
            .collect()
    }
}

// Device detection and response parsing

/// Device type detected from API response
#[derive(Debug, Clone)]
pub enum DetectedDeviceType {
    Bitaxe,
    NerdQaxe,
    Unknown,
}

/// Unified device response that can handle different device types
#[derive(Debug, Clone)]
pub enum DeviceResponse {
    Bitaxe(super::bitaxe::BitaxeInfoResponse),
    NerdQaxe(super::nerdqaxe::NerdQaxeInfoResponse),
}

impl DeviceResponse {
    /// Parse a raw JSON response into the appropriate device response
    pub fn from_json(json: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(json)?;
        let device_type = detect_device_type(&value);

        match device_type {
            DetectedDeviceType::Bitaxe => {
                let response: super::bitaxe::BitaxeInfoResponse = serde_json::from_str(json)?;
                Ok(DeviceResponse::Bitaxe(response))
            }
            DetectedDeviceType::NerdQaxe => {
                let response: super::nerdqaxe::NerdQaxeInfoResponse = serde_json::from_str(json)?;
                Ok(DeviceResponse::NerdQaxe(response))
            }
            DetectedDeviceType::Unknown => Err(anyhow!(
                "Unknown device type - could not identify from JSON response"
            )),
        }
    }

    /// Get the correct DeviceType for this device
    pub fn get_device_type(&self) -> DeviceType {
        match self {
            DeviceResponse::Bitaxe(bitaxe) => match bitaxe.asic_model.to_lowercase().as_str() {
                s if s.contains("bm1366") => DeviceType::BitaxeUltra,
                s if s.contains("bm1368") => DeviceType::BitaxeMax,
                s if s.contains("bm1370") => DeviceType::BitaxeGamma,
                _ => DeviceType::Unknown,
            },
            DeviceResponse::NerdQaxe(_) => DeviceType::NerdqaxePlus,
        }
    }

    /// Convert to unified SystemInfoResponse
    pub fn to_unified_info(&self) -> SystemInfoResponse {
        match self {
            DeviceResponse::Bitaxe(bitaxe) => bitaxe.to_unified_info(),
            DeviceResponse::NerdQaxe(nerdqaxe) => nerdqaxe.to_unified_info(),
        }
    }

    /// Convert to unified SystemStatsResponse
    pub fn to_unified_stats(&self) -> SystemStatsResponse {
        match self {
            DeviceResponse::Bitaxe(bitaxe) => bitaxe.to_unified_stats(),
            DeviceResponse::NerdQaxe(nerdqaxe) => nerdqaxe.to_unified_stats(),
        }
    }

    /// Get the device type
    pub fn device_type(&self) -> DetectedDeviceType {
        match self {
            DeviceResponse::Bitaxe(_) => DetectedDeviceType::Bitaxe,
            DeviceResponse::NerdQaxe(_) => DetectedDeviceType::NerdQaxe,
        }
    }
}

/// Detect device type from JSON response structure
pub fn detect_device_type(json: &Value) -> DetectedDeviceType {
    // Check for NerdQAxe first (most specific) - they have deviceModel field
    if json.get("deviceModel").is_some() {
        return DetectedDeviceType::NerdQaxe;
    }

    // Check for Bitaxe - they have ASICModel but no deviceModel
    if json.get("ASICModel").is_some() && json.get("hostname").is_some() {
        return DetectedDeviceType::Bitaxe;
    }

    DetectedDeviceType::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_type_detection_bitaxe() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "ASICModel": "BM1368",
            "hostname": "bitaxe-test",
            "version": "2.0.0"
        }"#,
        )
        .unwrap();

        let detected = detect_device_type(&json);
        assert!(matches!(detected, DetectedDeviceType::Bitaxe));
    }

    #[test]
    fn test_device_type_detection_nerdqaxe() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "deviceModel": "NerdQAxe++",
            "ASICModel": "BM1368",
            "hostname": "nerdqaxe-test"
        }"#,
        )
        .unwrap();

        let detected = detect_device_type(&json);
        assert!(matches!(detected, DetectedDeviceType::NerdQaxe));
    }

    #[test]
    fn test_device_type_detection_unknown() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "unknown": "device",
            "someField": "value"
        }"#,
        )
        .unwrap();

        let detected = detect_device_type(&json);
        assert!(matches!(detected, DetectedDeviceType::Unknown));
    }

    #[test]
    fn test_device_response_from_bitaxe_json() -> anyhow::Result<()> {
        let bitaxe_json = r#"{
            "ASICModel": "BM1368",
            "boardVersion": "204",
            "version": "2.0.0",
            "macAddr": "AA:BB:CC:DD:EE:FF",
            "hostname": "bitaxe-test",
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
            "sharesRejected": 2
        }"#;

        let device_response = DeviceResponse::from_json(bitaxe_json)?;

        match device_response {
            DeviceResponse::Bitaxe(bitaxe) => {
                assert_eq!(bitaxe.hostname, "bitaxe-test");
                assert_eq!(bitaxe.asic_model, "BM1368");
            }
            _ => panic!("Expected Bitaxe response"),
        }

        Ok(())
    }

    #[test]
    fn test_device_response_from_nerdqaxe_json() -> anyhow::Result<()> {
        let nerdqaxe_json = r#"{
            "deviceModel": "NerdQAxe++",
            "ASICModel": "BM1368",
            "version": "1.5.2",
            "macAddr": "11:22:33:44:55:66",
            "hostname": "nerdqaxe-test",
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
            "sharesRejected": 3
        }"#;

        let device_response = DeviceResponse::from_json(nerdqaxe_json)?;

        match device_response {
            DeviceResponse::NerdQaxe(nerdqaxe) => {
                assert_eq!(nerdqaxe.hostname, "nerdqaxe-test");
                assert_eq!(nerdqaxe.device_model, "NerdQAxe++");
            }
            _ => panic!("Expected NerdQaxe response"),
        }

        Ok(())
    }

    #[test]
    fn test_device_response_from_unknown_json() {
        let unknown_json = r#"{
            "unknown": "device",
            "someField": "value"
        }"#;

        let result = DeviceResponse::from_json(unknown_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_device_response_to_unified_info() -> anyhow::Result<()> {
        let bitaxe_json = r#"{
            "ASICModel": "BM1368",
            "version": "2.0.0",
            "macAddr": "AA:BB:CC:DD:EE:FF",
            "hostname": "bitaxe-test",
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
            "sharesRejected": 2
        }"#;

        let device_response = DeviceResponse::from_json(bitaxe_json)?;
        let unified_info = device_response.to_unified_info();

        assert_eq!(unified_info.hostname, "bitaxe-test");
        assert_eq!(unified_info.asic_model, "BM1368");
        assert_eq!(unified_info.firmware_version, "2.0.0");
        assert_eq!(unified_info.pool_port, 4334);

        Ok(())
    }

    #[test]
    fn test_device_response_to_unified_stats() -> anyhow::Result<()> {
        let bitaxe_json = r#"{
            "ASICModel": "BM1368",
            "version": "2.0.0",
            "macAddr": "AA:BB:CC:DD:EE:FF",
            "hostname": "bitaxe-test",
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
            "sharesRejected": 2
        }"#;

        let device_response = DeviceResponse::from_json(bitaxe_json)?;
        let unified_stats = device_response.to_unified_stats();

        assert_eq!(unified_stats.hashrate, 485.2);
        assert_eq!(unified_stats.temp, 65.5);
        assert_eq!(unified_stats.power, 15.8);
        assert_eq!(unified_stats.fanspeed, 75);
        assert_eq!(unified_stats.shares_accepted, 150);

        Ok(())
    }

    #[test]
    fn test_device_type_conversion_bitaxe() {
        let system_info = SystemInfoResponse {
            asic_model: "BM1368".to_string(),
            board_version: "204".to_string(),
            firmware_version: "2.0.0".to_string(),
            mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
            hostname: "bitaxe-test".to_string(),
            wifi_ssid: None,
            wifi_status: None,
            wifi_rssi: None,
            pool_url: "stratum+tcp://test.pool.com".to_string(),
            pool_port: 4334,
            pool_user: "bc1qtest123".to_string(),
            frequency: 485,
            voltage: 1200.0,
            fanspeed: 75,
            temp: 65.5,
            power: 15.8,
            running_time: 3600,
        };

        let device_type = DeviceType::from(&system_info);
        assert!(matches!(device_type, DeviceType::BitaxeMax));
    }

    #[test]
    fn test_device_stats_from_api_responses() {
        let system_info = SystemInfoResponse {
            asic_model: "BM1368".to_string(),
            board_version: "204".to_string(),
            firmware_version: "2.0.0".to_string(),
            mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
            hostname: "bitaxe-test".to_string(),
            wifi_ssid: Some("TestNetwork".to_string()),
            wifi_status: Some("Connected".to_string()),
            wifi_rssi: Some(-45),
            pool_url: "stratum+tcp://test.pool.com".to_string(),
            pool_port: 4334,
            pool_user: "bc1qtest123".to_string(),
            frequency: 485,
            voltage: 1200.0,
            fanspeed: 75,
            temp: 65.5,
            power: 15.8,
            running_time: 3600,
        };

        let system_stats = SystemStatsResponse {
            hashrate: 485.2,
            temp: 65.5,
            power: 15.8,
            fanspeed: 75,
            shares_accepted: 150,
            shares_rejected: 2,
            uptime: 3600,
            best_difficulty: Some("123.45K".to_string()),
            session_id: Some("session123".to_string()),
        };

        let device_stats =
            DeviceStats::from_api_responses("bitaxe-test".to_string(), &system_info, &system_stats);

        assert_eq!(device_stats.device_id, "bitaxe-test");
        assert_eq!(device_stats.hashrate_mhs, 485.2);
        assert_eq!(device_stats.temperature_celsius, 65.5);
        assert_eq!(device_stats.power_watts, 15.8);
        assert_eq!(device_stats.fan_speed_rpm, 75);
        assert_eq!(device_stats.shares_accepted, 150);
        assert_eq!(device_stats.shares_rejected, 2);
        assert_eq!(device_stats.uptime_seconds, 3600);
        assert_eq!(device_stats.wifi_rssi, Some(-45));
        assert_eq!(device_stats.voltage, Some(1200.0));
        assert_eq!(device_stats.frequency, Some(485));
        assert_eq!(
            device_stats.pool_url,
            Some("stratum+tcp://test.pool.com:4334".to_string())
        );
    }

    #[test]
    fn test_swarm_summary_calculation() {
        let devices = vec![
            DeviceInfo {
                name: "bitaxe-1".to_string(),
                ip_address: "192.168.1.100".to_string(),
                device_type: DeviceType::BitaxeMax,
                serial_number: Some("BX001".to_string()),
                status: DeviceStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
            DeviceInfo {
                name: "bitaxe-2".to_string(),
                ip_address: "192.168.1.101".to_string(),
                device_type: DeviceType::BitaxeMax,
                serial_number: Some("BX002".to_string()),
                status: DeviceStatus::Offline,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
        ];

        let stats = vec![DeviceStats {
            device_id: "bitaxe-1".to_string(),
            timestamp: chrono::Utc::now(),
            hashrate_mhs: 485.2,
            temperature_celsius: 65.5,
            power_watts: 15.8,
            fan_speed_rpm: 75,
            shares_accepted: 150,
            shares_rejected: 2,
            uptime_seconds: 3600,
            pool_url: None,
            wifi_rssi: None,
            voltage: None,
            frequency: None,
        }];

        let summary = SwarmSummary::from_devices_and_stats(&devices, &stats);

        assert_eq!(summary.total_devices, 2);
        assert_eq!(summary.devices_online, 1);
        assert_eq!(summary.devices_offline, 1);
        assert_eq!(summary.total_hashrate_mhs, 485.2);
        assert_eq!(summary.total_power_watts, 15.8);
        assert_eq!(summary.average_temperature, 65.5);
        assert_eq!(summary.total_shares_accepted, 150);
        assert_eq!(summary.total_shares_rejected, 2);
    }

    #[test]
    fn test_device_type_cli_names() {
        assert_eq!(DeviceType::BitaxeUltra.cli_name(), "bitaxe-ultra");
        assert_eq!(DeviceType::BitaxeMax.cli_name(), "bitaxe-max");
        assert_eq!(DeviceType::BitaxeGamma.cli_name(), "bitaxe-gamma");
        assert_eq!(DeviceType::NerdqaxePlus.cli_name(), "nerdqaxe");
        assert_eq!(DeviceType::Unknown.cli_name(), "unknown");
    }

    #[test]
    fn test_device_type_from_cli_name() {
        assert_eq!(
            DeviceType::from_cli_name("bitaxe-ultra"),
            Some(DeviceType::BitaxeUltra)
        );
        assert_eq!(
            DeviceType::from_cli_name("bitaxe_ultra"),
            Some(DeviceType::BitaxeUltra)
        );
        assert_eq!(
            DeviceType::from_cli_name("nerdqaxe"),
            Some(DeviceType::NerdqaxePlus)
        );
        assert_eq!(
            DeviceType::from_cli_name("nerdqaxe-plus"),
            Some(DeviceType::NerdqaxePlus)
        );
        assert_eq!(DeviceType::from_cli_name("bitaxe"), None); // Ambiguous
        assert_eq!(DeviceType::from_cli_name("invalid"), None);
    }

    #[test]
    fn test_device_type_matches_filter() {
        let bitaxe_ultra = DeviceType::BitaxeUltra;
        let nerdqaxe = DeviceType::NerdqaxePlus;

        assert!(bitaxe_ultra.matches_filter("all"));
        assert!(bitaxe_ultra.matches_filter("bitaxe"));
        assert!(bitaxe_ultra.matches_filter("bitaxe-ultra"));
        assert!(!bitaxe_ultra.matches_filter("nerdqaxe"));

        assert!(nerdqaxe.matches_filter("all"));
        assert!(!nerdqaxe.matches_filter("bitaxe"));
        assert!(nerdqaxe.matches_filter("nerdqaxe"));
    }

    #[test]
    fn test_type_summary_calculation() {
        let devices = vec![
            DeviceInfo {
                name: "bitaxe-1".to_string(),
                ip_address: "192.168.1.100".to_string(),
                device_type: DeviceType::BitaxeMax,
                serial_number: Some("BX001".to_string()),
                status: DeviceStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
            DeviceInfo {
                name: "bitaxe-2".to_string(),
                ip_address: "192.168.1.101".to_string(),
                device_type: DeviceType::BitaxeMax,
                serial_number: Some("BX002".to_string()),
                status: DeviceStatus::Offline,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
            DeviceInfo {
                name: "nerdqaxe-1".to_string(),
                ip_address: "192.168.1.102".to_string(),
                device_type: DeviceType::NerdqaxePlus,
                serial_number: Some("NQ001".to_string()),
                status: DeviceStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
        ];

        let stats = vec![
            DeviceStats {
                device_id: "192.168.1.100".to_string(),
                timestamp: chrono::Utc::now(),
                hashrate_mhs: 485.2,
                temperature_celsius: 65.5,
                power_watts: 15.8,
                fan_speed_rpm: 75,
                shares_accepted: 150,
                shares_rejected: 2,
                uptime_seconds: 3600,
                pool_url: None,
                wifi_rssi: None,
                voltage: None,
                frequency: None,
            },
            DeviceStats {
                device_id: "192.168.1.102".to_string(),
                timestamp: chrono::Utc::now(),
                hashrate_mhs: 512.7,
                temperature_celsius: 62.8,
                power_watts: 18.5,
                fan_speed_rpm: 80,
                shares_accepted: 225,
                shares_rejected: 3,
                uptime_seconds: 7200,
                pool_url: None,
                wifi_rssi: None,
                voltage: None,
                frequency: None,
            },
        ];

        let bitaxe_summary =
            TypeSummary::from_devices_and_stats(DeviceType::BitaxeMax, &devices, &stats);

        assert_eq!(bitaxe_summary.device_type, DeviceType::BitaxeMax);
        assert_eq!(bitaxe_summary.type_name, "Bitaxe Max");
        assert_eq!(bitaxe_summary.total_devices, 2);
        assert_eq!(bitaxe_summary.devices_online, 1);
        assert_eq!(bitaxe_summary.devices_offline, 1);
        assert_eq!(bitaxe_summary.total_hashrate_mhs, 485.2);
        assert_eq!(bitaxe_summary.total_power_watts, 15.8);
        assert_eq!(bitaxe_summary.average_temperature, 65.5);
        assert_eq!(bitaxe_summary.total_shares_accepted, 150);
        assert_eq!(bitaxe_summary.total_shares_rejected, 2);

        let nerdqaxe_summary =
            TypeSummary::from_devices_and_stats(DeviceType::NerdqaxePlus, &devices, &stats);

        assert_eq!(nerdqaxe_summary.device_type, DeviceType::NerdqaxePlus);
        assert_eq!(nerdqaxe_summary.type_name, "NerdQaxe++");
        assert_eq!(nerdqaxe_summary.total_devices, 1);
        assert_eq!(nerdqaxe_summary.devices_online, 1);
        assert_eq!(nerdqaxe_summary.devices_offline, 0);
        assert_eq!(nerdqaxe_summary.total_hashrate_mhs, 512.7);
        assert_eq!(nerdqaxe_summary.total_power_watts, 18.5);
        assert_eq!(nerdqaxe_summary.average_temperature, 62.8);
        assert_eq!(nerdqaxe_summary.total_shares_accepted, 225);
        assert_eq!(nerdqaxe_summary.total_shares_rejected, 3);
    }

    #[test]
    fn test_type_summary_from_all_devices() {
        let devices = vec![
            DeviceInfo {
                name: "bitaxe-1".to_string(),
                ip_address: "192.168.1.100".to_string(),
                device_type: DeviceType::BitaxeMax,
                serial_number: Some("BX001".to_string()),
                status: DeviceStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
            DeviceInfo {
                name: "nerdqaxe-1".to_string(),
                ip_address: "192.168.1.101".to_string(),
                device_type: DeviceType::NerdqaxePlus,
                serial_number: Some("NQ001".to_string()),
                status: DeviceStatus::Online,
                discovered_at: chrono::Utc::now(),
                last_seen: chrono::Utc::now(),
            },
        ];

        let stats = vec![];

        let summaries = TypeSummary::from_all_devices_and_stats(&devices, &stats);

        // Should only include types that have devices
        assert_eq!(summaries.len(), 2);
        assert!(summaries
            .iter()
            .any(|s| s.device_type == DeviceType::BitaxeMax));
        assert!(summaries
            .iter()
            .any(|s| s.device_type == DeviceType::NerdqaxePlus));
        assert!(!summaries
            .iter()
            .any(|s| s.device_type == DeviceType::BitaxeUltra));
    }
}
