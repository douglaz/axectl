/// Mock HTTP responses for testing
/// These are based on real AxeOS API responses but sanitized for testing

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
    "vrTemp": 68.2,
    "hashRate": 485.2,
    "bestDiff": "123.45K",
    "bestSessionDiff": "567.89K",
    "freeHeap": 123456,
    "coreVoltage": 1.2,
    "coreVoltageActual": 1.19,
    "uptimeSeconds": 3600,
    "ssid": "TestNetwork",
    "sharesAccepted": 150,
    "sharesRejected": 2
}"#;

pub const NERDQAXE_SYSTEM_INFO_RESPONSE: &str = r#"{
    "asic_model": "BM1368",
    "board_version": "v1.0",
    "firmware_version": "1.5.2",
    "mac_address": "11:22:33:44:55:66",
    "hostname": "nerdqaxe-test",
    "wifi": {
        "ssid": "TestNetwork",
        "status": "connected",
        "rssi": -52
    },
    "pool": {
        "url": "stratum+tcp://test.pool.com",
        "port": 4334,
        "user": "bc1qtest456"
    },
    "settings": {
        "frequency": 500,
        "voltage": 1250,
        "fan_speed": 80
    },
    "sensors": {
        "temperature": 62.8,
        "vr_temperature": 65.1,
        "fan_rpm": 3600
    },
    "mining": {
        "hashrate": 512.7,
        "best_diff": "234.56K",
        "shares": {
            "accepted": 225,
            "rejected": 3
        },
        "uptime": 7200
    },
    "system": {
        "free_heap": 98765,
        "core_voltage": 1.25,
        "actual_voltage": 1.24
    }
}"#;

pub const BITAXE_ERROR_RESPONSE: &str = r#"{
    "error": "Invalid command",
    "code": 400
}"#;

pub const NETWORK_TIMEOUT_ERROR: &str = "Network timeout after 5 seconds";

/// Helper function to create a mock server response
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

    pub struct MockMdnsDevice {
        pub name: String,
        pub ip_addresses: Vec<IpAddr>,
        pub port: u16,
        pub txt_records: Vec<(String, String)>,
    }

    pub fn create_mock_bitaxe() -> MockMdnsDevice {
        MockMdnsDevice {
            name: "bitaxe-test._http._tcp.local.".to_string(),
            ip_addresses: vec!["192.168.1.100".parse().unwrap()],
            port: 80,
            txt_records: vec![
                ("version".to_string(), "2.0.0".to_string()),
                ("model".to_string(), "bitaxe".to_string()),
            ],
        }
    }

    pub fn create_mock_nerdqaxe() -> MockMdnsDevice {
        MockMdnsDevice {
            name: "nerdqaxe-test._http._tcp.local.".to_string(),
            ip_addresses: vec!["192.168.1.101".parse().unwrap()],
            port: 80,
            txt_records: vec![
                ("version".to_string(), "1.5.2".to_string()),
                ("model".to_string(), "nerdqaxe".to_string()),
            ],
        }
    }
}
