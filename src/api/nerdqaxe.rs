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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_NERDQAXE_RESPONSE: &str = r#"{
        "deviceModel": "NerdQAxe++",
        "ASICModel": "BM1368",
        "version": "1.5.2",
        "macAddr": "11:22:33:44:55:66",
        "hostname": "nerdqaxe-test",
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
    }"#;

    #[test]
    fn test_nerdqaxe_parsing() {
        let response: NerdQaxeInfoResponse =
            serde_json::from_str(SAMPLE_NERDQAXE_RESPONSE).unwrap();

        assert_eq!(response.device_model, "NerdQAxe++");
        assert_eq!(response.asic_model, "BM1368");
        assert_eq!(response.version, Some("1.5.2".to_string()));
        assert_eq!(response.mac_address, "11:22:33:44:55:66");
        assert_eq!(response.hostname, "nerdqaxe-test");
        assert_eq!(response.host_ip, Some("192.168.1.101".to_string()));
        assert_eq!(response.ssid, Some("TestNetwork".to_string()));
        assert_eq!(response.pool_url, "stratum+tcp://test.pool.com");
        assert_eq!(response.pool_port, 4334);
        assert_eq!(response.frequency, 500);
        assert_eq!(response.voltage, 1250.0);
        assert_eq!(response.temp, 62.8);
        assert_eq!(response.hash_rate, 512.7);
        assert_eq!(response.shares_accepted, 225);
        assert_eq!(response.shares_rejected, 3);
    }

    #[test]
    fn test_nerdqaxe_minimal_response() {
        let minimal_response = r#"{
            "deviceModel": "NerdQAxe+",
            "ASICModel": "BM1368",
            "macAddr": "11:22:33:44:55:66",
            "hostname": "nerdqaxe-minimal",
            "stratumURL": "stratum+tcp://pool.com",
            "stratumPort": 4334,
            "stratumUser": "user456",
            "frequency": 450,
            "voltage": 1200,
            "fanspeed": 70,
            "temp": 58.0,
            "power": 16.2,
            "hashRate": 450.0,
            "uptimeSeconds": 3600,
            "sharesAccepted": 180,
            "sharesRejected": 2
        }"#;

        let response: NerdQaxeInfoResponse = serde_json::from_str(minimal_response).unwrap();
        assert_eq!(response.hostname, "nerdqaxe-minimal");
        assert_eq!(response.version, None);
        assert_eq!(response.ssid, None);
        assert_eq!(response.host_ip, None);
    }

    #[test]
    fn test_nerdqaxe_to_unified_info() {
        let response: NerdQaxeInfoResponse =
            serde_json::from_str(SAMPLE_NERDQAXE_RESPONSE).unwrap();
        let unified = response.to_unified_info();

        assert_eq!(unified.hostname, "nerdqaxe-test");
        assert_eq!(unified.asic_model, "BM1368");
        assert_eq!(unified.board_version, "unknown"); // NerdQAxe doesn't provide board version
        assert_eq!(unified.firmware_version, "1.5.2");
        assert_eq!(unified.mac_address, "11:22:33:44:55:66");
        assert_eq!(unified.wifi_ssid, Some("TestNetwork".to_string()));
        assert_eq!(unified.pool_url, "stratum+tcp://test.pool.com");
        assert_eq!(unified.pool_port, 4334);
        assert_eq!(unified.frequency, 500);
        assert_eq!(unified.voltage, 1250.0);
    }

    #[test]
    fn test_nerdqaxe_to_unified_stats() {
        let response: NerdQaxeInfoResponse =
            serde_json::from_str(SAMPLE_NERDQAXE_RESPONSE).unwrap();
        let stats = response.to_unified_stats();

        assert_eq!(stats.hashrate, 512.7);
        assert_eq!(stats.temp, 62.8);
        assert_eq!(stats.power, 18.5);
        assert_eq!(stats.fanspeed, 80);
        assert_eq!(stats.shares_accepted, 225);
        assert_eq!(stats.shares_rejected, 3);
        assert_eq!(stats.uptime, 7200);
        assert_eq!(stats.best_difficulty, Some("234.56K".to_string()));
        assert_eq!(stats.session_id, Some("firmware_a".to_string()));
    }

    #[test]
    fn test_nerdqaxe_invalid_json() {
        let invalid_json = r#"{"invalid": "json"}"#;
        let result: Result<NerdQaxeInfoResponse, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_nerdqaxe_partial_json() {
        // Test with missing required fields
        let partial_json = r#"{
            "deviceModel": "NerdQAxe++",
            "ASICModel": "BM1368"
        }"#;
        let result: Result<NerdQaxeInfoResponse, _> = serde_json::from_str(partial_json);
        assert!(result.is_err());
    }
}
