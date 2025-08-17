use serde::Deserialize;

/// Bitaxe-specific API response structure
#[derive(Debug, Clone, Deserialize)]
pub struct BitaxeInfoResponse {
    #[serde(rename = "ASICModel")]
    pub asic_model: String,
    #[serde(rename = "boardVersion")]
    pub board_version: Option<String>,
    #[serde(rename = "version")]
    pub firmware_version: String,
    #[serde(rename = "macAddr")]
    pub mac_address: String,
    pub hostname: String,
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
}

impl BitaxeInfoResponse {
    /// Convert to unified SystemInfoResponse
    pub fn to_unified_info(&self) -> super::SystemInfoResponse {
        super::SystemInfoResponse {
            asic_model: self.asic_model.clone(),
            board_version: self
                .board_version
                .as_deref()
                .unwrap_or("unknown")
                .to_string(),
            firmware_version: self.firmware_version.clone(),
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
            session_id: Some(self.firmware_version.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BITAXE_RESPONSE: &str = r#"{
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
    }"#;

    #[test]
    fn test_bitaxe_parsing() {
        let response: BitaxeInfoResponse = serde_json::from_str(SAMPLE_BITAXE_RESPONSE).unwrap();

        assert_eq!(response.asic_model, "BM1368");
        assert_eq!(response.board_version, Some("204".to_string()));
        assert_eq!(response.firmware_version, "2.0.0");
        assert_eq!(response.mac_address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(response.hostname, "bitaxe-test");
        assert_eq!(response.ssid, Some("TestNetwork".to_string()));
        assert_eq!(response.pool_url, "stratum+tcp://test.pool.com");
        assert_eq!(response.pool_port, 4334);
        assert_eq!(response.frequency, 485);
        assert_eq!(response.voltage, 1200.0);
        assert_eq!(response.temp, 65.5);
        assert_eq!(response.hash_rate, 485.2);
        assert_eq!(response.shares_accepted, 150);
        assert_eq!(response.shares_rejected, 2);
    }

    #[test]
    fn test_bitaxe_minimal_response() {
        let minimal_response = r#"{
            "ASICModel": "BM1368",
            "version": "2.0.0",
            "macAddr": "AA:BB:CC:DD:EE:FF",
            "hostname": "bitaxe-minimal",
            "stratumURL": "stratum+tcp://pool.com",
            "stratumPort": 4334,
            "stratumUser": "user123",
            "frequency": 400,
            "voltage": 1100,
            "fanspeed": 50,
            "temp": 60.0,
            "power": 12.5,
            "hashRate": 400.0,
            "uptimeSeconds": 1800,
            "sharesAccepted": 100,
            "sharesRejected": 1
        }"#;

        let response: BitaxeInfoResponse = serde_json::from_str(minimal_response).unwrap();
        assert_eq!(response.hostname, "bitaxe-minimal");
        assert_eq!(response.board_version, None);
        assert_eq!(response.ssid, None);
    }

    #[test]
    fn test_bitaxe_to_unified_info() {
        let response: BitaxeInfoResponse = serde_json::from_str(SAMPLE_BITAXE_RESPONSE).unwrap();
        let unified = response.to_unified_info();

        assert_eq!(unified.hostname, "bitaxe-test");
        assert_eq!(unified.asic_model, "BM1368");
        assert_eq!(unified.board_version, "204");
        assert_eq!(unified.firmware_version, "2.0.0");
        assert_eq!(unified.mac_address, "AA:BB:CC:DD:EE:FF");
        assert_eq!(unified.wifi_ssid, Some("TestNetwork".to_string()));
        assert_eq!(unified.pool_url, "stratum+tcp://test.pool.com");
        assert_eq!(unified.pool_port, 4334);
        assert_eq!(unified.frequency, 485);
        assert_eq!(unified.voltage, 1200.0);
    }

    #[test]
    fn test_bitaxe_to_unified_stats() {
        let response: BitaxeInfoResponse = serde_json::from_str(SAMPLE_BITAXE_RESPONSE).unwrap();
        let stats = response.to_unified_stats();

        assert_eq!(stats.hashrate, 485.2);
        assert_eq!(stats.temp, 65.5);
        assert_eq!(stats.power, 15.8);
        assert_eq!(stats.fanspeed, 75);
        assert_eq!(stats.shares_accepted, 150);
        assert_eq!(stats.shares_rejected, 2);
        assert_eq!(stats.uptime, 3600);
        assert_eq!(stats.best_difficulty, Some("123.45K".to_string()));
        assert_eq!(stats.session_id, Some("2.0.0".to_string()));
    }

    #[test]
    fn test_bitaxe_invalid_json() {
        let invalid_json = r#"{"invalid": "json"}"#;
        let result: Result<BitaxeInfoResponse, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_bitaxe_partial_json() {
        // Test with missing required fields
        let partial_json = r#"{
            "ASICModel": "BM1368",
            "version": "2.0.0"
        }"#;
        let result: Result<BitaxeInfoResponse, _> = serde_json::from_str(partial_json);
        assert!(result.is_err());
    }
}
