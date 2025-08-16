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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize, Default)]
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
