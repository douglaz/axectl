use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;
use tokio::time::timeout;

use crate::api::{AxeOsClient, DeviceInfo, DeviceStatus, DeviceType};

#[derive(Debug, Clone)]
pub struct MdnsDiscovery {
    service_names: Vec<String>,
    discovery_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct MdnsDevice {
    pub hostname: String,
    pub ip_addresses: Vec<IpAddr>,
    pub port: u16,
    pub service_type: String,
    pub txt_records: HashMap<String, String>,
}

impl Default for MdnsDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

impl MdnsDiscovery {
    pub fn new() -> Self {
        Self {
            service_names: vec![
                "_http._tcp.local.".to_string(),
                "_https._tcp.local.".to_string(),
                "_axeos._tcp.local.".to_string(), // If AxeOS advertises a specific service
                "_bitaxe._tcp.local.".to_string(), // Custom service names
                "_nerdqaxe._tcp.local.".to_string(), // Custom service names
            ],
            discovery_timeout: Duration::from_secs(5),
        }
    }

    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            discovery_timeout: timeout,
            ..Self::new()
        }
    }

    pub fn with_service_names(service_names: Vec<String>) -> Self {
        Self {
            service_names,
            ..Self::new()
        }
    }

    /// Discover devices using mDNS
    pub async fn discover_devices(&self) -> Result<Vec<DeviceInfo>> {
        let mdns_devices = self.scan_mdns_services().await?;
        let mut devices = Vec::new();

        // Convert mDNS discoveries to device info by probing them
        for mdns_device in mdns_devices {
            for ip in &mdns_device.ip_addresses {
                if let Ok(Some(device)) = self.probe_mdns_device(ip, &mdns_device).await {
                    devices.push(device);
                    break; // Use first working IP
                }
            }
        }

        Ok(devices)
    }

    async fn scan_mdns_services(&self) -> Result<Vec<MdnsDevice>> {
        let mdns = ServiceDaemon::new().context("Failed to create mDNS daemon")?;
        let mut discovered_devices = Vec::new();

        for service_name in &self.service_names {
            let receiver = mdns
                .browse(service_name)
                .context("Failed to browse mDNS service")?;

            // Use timeout to limit discovery time
            let discovery_result = timeout(
                self.discovery_timeout,
                self.collect_service_events(receiver),
            )
            .await;

            match discovery_result {
                Ok(devices) => discovered_devices.extend(devices),
                Err(_) => {
                    // Timeout is expected - mDNS discovery has a time limit
                    tracing::debug!("mDNS discovery timeout for service: {}", service_name);
                }
            }
        }

        Ok(discovered_devices)
    }

    async fn collect_service_events(
        &self,
        receiver: flume::Receiver<ServiceEvent>,
    ) -> Vec<MdnsDevice> {
        let mut devices = HashMap::new();

        // Collect events for a short period
        while let Ok(Ok(event)) = timeout(Duration::from_millis(500), receiver.recv_async()).await {
            self.process_service_event(event, &mut devices);
        }

        devices.into_values().collect()
    }

    fn process_service_event(
        &self,
        event: ServiceEvent,
        devices: &mut HashMap<String, MdnsDevice>,
    ) {
        match event {
            ServiceEvent::ServiceResolved(info) => {
                let device = MdnsDevice {
                    hostname: info.get_hostname().to_string(),
                    ip_addresses: info.get_addresses().iter().copied().collect(),
                    port: info.get_port(),
                    service_type: info.get_type().to_string(),
                    txt_records: info
                        .get_properties()
                        .iter()
                        .map(|prop| {
                            (
                                prop.key().to_string(),
                                String::from_utf8_lossy(prop.val().unwrap_or(b"")).to_string(),
                            )
                        })
                        .collect(),
                };

                devices.insert(info.get_fullname().to_string(), device);
            }
            ServiceEvent::ServiceRemoved(_, _) => {
                // Handle service removal if needed
            }
            _ => {
                // Other events (ServiceFound, etc.)
            }
        }
    }

    async fn probe_mdns_device(
        &self,
        ip: &IpAddr,
        mdns_device: &MdnsDevice,
    ) -> Result<Option<DeviceInfo>> {
        let ip_str = ip.to_string();

        // Check if this looks like it could be an AxeOS device
        if !self.is_potential_axeos_device(mdns_device) {
            return Ok(None);
        }

        // Try to connect to the device
        let client = AxeOsClient::with_timeout(&ip_str, Duration::from_secs(2))?;

        // Quick health check
        match timeout(Duration::from_secs(3), client.health_check()).await {
            Ok(Ok(true)) => {
                // Try to get detailed info
                if let Ok(Ok(system_info)) =
                    timeout(Duration::from_secs(3), client.get_system_info()).await
                {
                    let device_type = DeviceType::from(&system_info);

                    let device = DeviceInfo {
                        name: system_info.hostname,
                        ip_address: ip_str,
                        device_type,
                        serial_number: Some(system_info.mac_address),
                        status: DeviceStatus::Online,
                        discovered_at: chrono::Utc::now(),
                        last_seen: chrono::Utc::now(),
                    };

                    Ok(Some(device))
                } else {
                    // Health check passed but couldn't get system info
                    // Still might be an AxeOS device
                    let device = DeviceInfo {
                        name: mdns_device.hostname.clone(),
                        ip_address: ip_str,
                        device_type: DeviceType::Unknown,
                        serial_number: None,
                        status: DeviceStatus::Online,
                        discovered_at: chrono::Utc::now(),
                        last_seen: chrono::Utc::now(),
                    };

                    Ok(Some(device))
                }
            }
            _ => {
                // Not reachable or not AxeOS
                Ok(None)
            }
        }
    }

    fn is_potential_axeos_device(&self, device: &MdnsDevice) -> bool {
        // Check hostname patterns
        let hostname_lower = device.hostname.to_lowercase();
        if hostname_lower.contains("bitaxe")
            || hostname_lower.contains("nerdqaxe")
            || hostname_lower.contains("axe")
        {
            return true;
        }

        // Check service type
        if device.service_type.contains("_axeos._tcp")
            || device.service_type.contains("_bitaxe._tcp")
            || device.service_type.contains("_nerdqaxe._tcp")
        {
            return true;
        }

        // Check TXT records for hints
        for (key, value) in &device.txt_records {
            let key_lower = key.to_lowercase();
            let value_lower = value.to_lowercase();

            if key_lower.contains("model")
                && (value_lower.contains("bitaxe") || value_lower.contains("nerdqaxe"))
            {
                return true;
            }

            if key_lower.contains("firmware") && value_lower.contains("axeos") {
                return true;
            }
        }

        // Check port - AxeOS typically runs on port 80
        if device.port == 80 {
            return true;
        }

        // Default to true for _http services to allow broader discovery
        device.service_type.contains("_http._tcp")
    }
}

/// Simple mDNS discovery function for quick use
pub async fn discover_axeos_devices(timeout: Duration) -> Result<Vec<DeviceInfo>> {
    let discovery = MdnsDiscovery::with_timeout(timeout);
    discovery.discover_devices().await
}

/// Discover devices with specific service names
pub async fn discover_with_services(
    services: Vec<String>,
    timeout: Duration,
) -> Result<Vec<DeviceInfo>> {
    let mut discovery = MdnsDiscovery::with_service_names(services);
    discovery.discovery_timeout = timeout;
    discovery.discover_devices().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mdns_discovery_creation() {
        let discovery = MdnsDiscovery::new();
        assert!(!discovery.service_names.is_empty());
        assert_eq!(discovery.discovery_timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_mdns_discovery_with_timeout() {
        let discovery = MdnsDiscovery::with_timeout(Duration::from_secs(10));
        assert_eq!(discovery.discovery_timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_mdns_discovery_with_services() {
        let services = vec!["_test._tcp.local.".to_string()];
        let discovery = MdnsDiscovery::with_service_names(services.clone());
        assert_eq!(discovery.service_names, services);
    }

    #[test]
    fn test_is_potential_axeos_device() {
        let discovery = MdnsDiscovery::new();

        // Test hostname detection
        let device = MdnsDevice {
            hostname: "bitaxe-001.local".to_string(),
            ip_addresses: vec![],
            port: 80,
            service_type: "_http._tcp.local.".to_string(),
            txt_records: HashMap::new(),
        };
        assert!(discovery.is_potential_axeos_device(&device));

        // Test service type detection
        let device = MdnsDevice {
            hostname: "unknown.local".to_string(),
            ip_addresses: vec![],
            port: 80,
            service_type: "_axeos._tcp.local.".to_string(),
            txt_records: HashMap::new(),
        };
        assert!(discovery.is_potential_axeos_device(&device));

        // Test TXT record detection
        let mut txt_records = HashMap::new();
        txt_records.insert("model".to_string(), "Bitaxe Ultra".to_string());
        let device = MdnsDevice {
            hostname: "device.local".to_string(),
            ip_addresses: vec![],
            port: 8080,
            service_type: "_http._tcp.local.".to_string(),
            txt_records,
        };
        assert!(discovery.is_potential_axeos_device(&device));
    }

    #[tokio::test]
    async fn test_discover_axeos_devices() {
        // This test will typically find no devices in CI environments
        // but verifies the function doesn't panic
        let result = discover_axeos_devices(Duration::from_millis(100)).await;
        assert!(result.is_ok());

        let devices = result.unwrap();
        // In most test environments, we expect to find no devices
        // but the function should complete successfully
        println!("Found {} devices via mDNS", devices.len());
    }
}
