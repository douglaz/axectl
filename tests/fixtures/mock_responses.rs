//! Mock HTTP responses for testing
//! These are based on real AxeOS API responses but sanitized for testing

#[allow(dead_code)]
pub const BITAXE_SYSTEM_INFO_RESPONSE: &str = r#"{
    "ASICModel": "BM1368",
    "boardVersion": "204",
    "firmwareVersion": "2.0.0",
    "macAddress": "AA:BB:CC:DD:EE:FF",
    "hostname": "bitaxe-test",
    "wifiSSID": "TestNetwork",
    "wifiStatus": "Connected",
    "wifiRSSI": -45,
    "poolURL": "stratum+tcp://test.pool.com",
    "poolPort": 4334,
    "poolUser": "bc1qtest123",
    "frequency": 485,
    "voltage": 1200,
    "fanSpeed": 75,
    "fanRPM": 3450,
    "temp": 65.5,
    "hashrate": 450.5,
    "powerConsumption": 10.5,
    "efficiency": 23.3,
    "uptimeSeconds": 3600,
    "sharesAccepted": 42,
    "sharesRejected": 1,
    "bestDifficulty": 123456,
    "difficultyAccepted": 100000,
    "difficultyRejected": 1000,
    "fanspeed": [75],
    "fanrpm": [3450]
}"#;

#[allow(dead_code)]
pub const NERDQAXE_SYSTEM_INFO_RESPONSE: &str = r#"{
    "name": "NerdQAxe++",
    "hostname": "nerdqaxe-test",
    "mac": "BB:CC:DD:EE:FF:AA",
    "version": "1.5.0",
    "network": {
        "ssid": "TestNetwork",
        "status": "connected",
        "rssi": -50
    },
    "mining": {
        "pool": {
            "url": "stratum+tcp://test.pool.com:3333",
            "user": "bc1qtest456"
        },
        "stats": {
            "hashrate": 550.0,
            "shares": {
                "accepted": 100,
                "rejected": 2
            },
            "uptime": 7200
        }
    },
    "hardware": {
        "temp": 68.0,
        "fans": [
            {
                "speed": 80,
                "rpm": 3600
            }
        ],
        "power": 12.0,
        "efficiency": 21.8
    }
}"#;

#[allow(dead_code)]
pub const BITAXE_ERROR_RESPONSE: &str = r#"{
    "error": "Invalid command",
    "code": 400
}"#;

#[allow(dead_code)]
pub const NETWORK_TIMEOUT_ERROR: &str = "Network timeout after 5 seconds";

/// Helper function to create a mock server response
#[allow(dead_code)]
pub fn mock_device_response(device_type: &str) -> &'static str {
    match device_type {
        "bitaxe" => BITAXE_SYSTEM_INFO_RESPONSE,
        "nerdqaxe" => NERDQAXE_SYSTEM_INFO_RESPONSE,
        _ => BITAXE_SYSTEM_INFO_RESPONSE,
    }
}

/// Mock mDNS service discovery responses
pub mod mdns {
    use std::net::IpAddr;

    #[allow(dead_code)]
    pub struct MockMdnsDevice {
        pub service_type: String,
        pub port: u16,
        pub addresses: Vec<IpAddr>,
        pub hostname: String,
    }

    #[allow(dead_code)]
    pub fn create_mock_bitaxe() -> MockMdnsDevice {
        MockMdnsDevice {
            service_type: "_http._tcp.local.".to_string(),
            port: 80,
            addresses: vec!["192.168.1.100".parse().unwrap()],
            hostname: "bitaxe-test.local".to_string(),
        }
    }

    #[allow(dead_code)]
    pub fn create_mock_nerdqaxe() -> MockMdnsDevice {
        MockMdnsDevice {
            service_type: "_http._tcp.local.".to_string(),
            port: 80,
            addresses: vec!["192.168.1.101".parse().unwrap()],
            hostname: "nerdqaxe-test.local".to_string(),
        }
    }
}
