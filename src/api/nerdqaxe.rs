use serde::Deserialize;

/// NerdQAxe-specific API response structure
#[derive(Debug, Clone, Deserialize)]
pub struct NerdQaxeInfoResponse {
    #[serde(rename = "deviceModel")]
    pub device_model: String,
    #[serde(rename = "ASICModel")]
    pub asic_model: String,
    #[serde(rename = "version")]
    pub version: Option<String>,
    #[serde(rename = "macAddr")]
    pub mac_address: String,
    pub hostname: String,
    #[serde(rename = "hostip")]
    pub host_ip: Option<String>,
    pub ssid: Option<String>,
    #[serde(rename = "wifiStatus")]
    pub wifi_status: Option<String>,
    #[serde(rename = "wifiRSSI")]
    pub wifi_rssi: Option<i32>,
    #[serde(rename = "stratumURL")]
    pub pool_url: String,
    #[serde(rename = "stratumPort")]
    pub pool_port: u16,
    #[serde(rename = "stratumUser")]
    pub pool_user: String,
    pub frequency: u32,
    pub voltage: f64,
    pub fanspeed: u32,
    pub temp: f64,
    pub power: f64,
    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: u64,
    #[serde(rename = "hashRate")]
    pub hash_rate: f64,
    #[serde(rename = "sharesAccepted")]
    pub shares_accepted: u64,
    #[serde(rename = "sharesRejected")]
    pub shares_rejected: u64,
    #[serde(rename = "bestDiff")]
    pub best_difficulty: Option<String>,
    #[serde(rename = "runningPartition")]
    pub running_partition: Option<String>,
}

impl NerdQaxeInfoResponse {
    /// Convert to unified SystemInfoResponse
    pub fn to_unified_info(&self) -> super::SystemInfoResponse {
        super::SystemInfoResponse {
            asic_model: self.asic_model.clone(),
            board_version: "unknown".to_string(), // NerdQAxe doesn't provide board version
            firmware_version: self.version.as_deref().unwrap_or("unknown").to_string(),
            mac_address: self.mac_address.clone(),
            hostname: self.hostname.clone(),
            wifi_ssid: self.ssid.clone(),
            wifi_status: self.wifi_status.clone(),
            wifi_rssi: self.wifi_rssi,
            pool_url: self.pool_url.clone(),
            pool_port: self.pool_port,
            pool_user: self.pool_user.clone(),
            frequency: self.frequency,
            voltage: self.voltage,
            fanspeed: self.fanspeed,
            temp: self.temp,
            power: self.power,
            running_time: self.uptime_seconds,
        }
    }

    /// Convert to unified SystemStatsResponse
    pub fn to_unified_stats(&self) -> super::SystemStatsResponse {
        super::SystemStatsResponse {
            hashrate: self.hash_rate,
            temp: self.temp,
            power: self.power,
            fanspeed: self.fanspeed,
            shares_accepted: self.shares_accepted,
            shares_rejected: self.shares_rejected,
            uptime: self.uptime_seconds,
            best_difficulty: self.best_difficulty.clone(),
            session_id: self.running_partition.clone(),
        }
    }
}
