use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::api::{DeviceInfo, DeviceStats, DeviceStatus};

/// Enhanced cached device with full info and stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDevice {
    /// Full device information
    pub info: DeviceInfo,
    /// Latest statistics
    pub latest_stats: Option<DeviceStats>,
    /// Recent statistics history (last 10 entries)
    pub stats_history: Vec<DeviceStats>,
    /// Last time device was successfully contacted
    pub last_seen: DateTime<Utc>,
    /// Last time device was probed (even if failed)
    pub last_probed: DateTime<Utc>,
}

/// Device cache file structure
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceCache {
    pub version: u32,
    pub last_updated: DateTime<Utc>,
    pub devices: HashMap<String, CachedDevice>,
}

impl DeviceCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            version: 2,
            last_updated: Utc::now(),
            devices: HashMap::new(),
        }
    }

    /// Load cache from directory
    pub fn load(cache_dir: &Path) -> Result<Self> {
        let cache_file = cache_dir.join("devices.json");

        if !cache_file.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&cache_file)
            .with_context(|| format!("Failed to read cache file: {}", cache_file.display()))?;

        let cache: DeviceCache = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache file: {}", cache_file.display()))?;

        // Only support version 2
        if cache.version != 2 {
            tracing::warn!(
                "Unsupported cache version {}, starting fresh",
                cache.version
            );
            return Ok(Self::new());
        }

        Ok(cache)
    }

    /// Save cache to directory
    pub fn save(&self, cache_dir: &Path) -> Result<()> {
        // Create cache directory if it doesn't exist
        fs::create_dir_all(cache_dir).with_context(|| {
            format!("Failed to create cache directory: {}", cache_dir.display())
        })?;

        let cache_file = cache_dir.join("devices.json");

        let content = serde_json::to_string_pretty(self).context("Failed to serialize cache")?;

        fs::write(&cache_file, content)
            .with_context(|| format!("Failed to write cache file: {}", cache_file.display()))?;

        Ok(())
    }

    /// Add or update a device in the cache
    pub fn add_device(&mut self, device: DeviceInfo) {
        let cached = CachedDevice {
            info: device.clone(),
            latest_stats: None,
            stats_history: Vec::new(),
            last_seen: Utc::now(),
            last_probed: Utc::now(),
        };
        self.devices.insert(device.ip_address.clone(), cached);
        self.last_updated = Utc::now();
    }

    /// Update existing device info
    pub fn update_device(&mut self, device: DeviceInfo) {
        if let Some(cached) = self.devices.get_mut(&device.ip_address) {
            cached.info = device;
            cached.last_seen = Utc::now();
            cached.last_probed = Utc::now();
        } else {
            self.add_device(device);
        }
        self.last_updated = Utc::now();
    }

    /// Update device stats
    pub fn update_device_stats(&mut self, device_id: &str, stats: DeviceStats) {
        if let Some(cached) = self.devices.get_mut(device_id) {
            // Update latest stats
            cached.latest_stats = Some(stats.clone());

            // Add to history (keep last 10 entries)
            cached.stats_history.push(stats);
            if cached.stats_history.len() > 10 {
                cached.stats_history.remove(0);
            }

            cached.last_seen = Utc::now();
            cached.last_probed = Utc::now();
        }
        self.last_updated = Utc::now();
    }

    /// Mark device as probed (even if failed)
    pub fn mark_device_probed(&mut self, ip_address: &str, success: bool) {
        if let Some(cached) = self.devices.get_mut(ip_address) {
            cached.last_probed = Utc::now();
            if success {
                cached.last_seen = Utc::now();
            }
        }
        self.last_updated = Utc::now();
    }

    /// Remove stale devices (not seen in specified duration)
    pub fn prune(&mut self, max_age: Duration) {
        let cutoff = Utc::now() - max_age;
        self.devices.retain(|_, device| device.last_seen > cutoff);
        self.last_updated = Utc::now();
    }

    /// Alias for prune (for backward compatibility)
    pub fn prune_old(&mut self, max_age: Duration) {
        self.prune(max_age);
    }

    /// Get a device by IP address
    pub fn get_device(&self, ip_address: &str) -> Option<&CachedDevice> {
        self.devices.get(ip_address)
    }

    /// Get all devices
    pub fn get_all_devices(&self) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get devices by status
    pub fn get_devices_by_status(&self, status: DeviceStatus) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .filter(|cached| cached.info.status == status)
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get devices by type filter (supports wildcard matching)
    pub fn get_devices_by_type_filter(&self, type_filter: &str) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .filter(|cached| cached.info.device_type.matches_filter(type_filter))
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get online devices by type filter
    pub fn get_online_devices_by_type_filter(&self, type_filter: &str) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .filter(|cached| {
                cached.info.status == DeviceStatus::Online
                    && cached.info.device_type.matches_filter(type_filter)
            })
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Get number of devices in cache
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Get all known IP addresses
    pub fn get_known_ips(&self) -> Vec<String> {
        self.devices.keys().cloned().collect()
    }

    /// Get cache age in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.last_updated).num_seconds()
    }

    /// Get IP addresses that need refresh (older than specified duration)
    pub fn get_stale_addresses(&self, max_age: Duration) -> Vec<String> {
        let cutoff = Utc::now() - max_age;
        self.devices
            .iter()
            .filter(|(_, cached)| cached.last_probed < cutoff)
            .map(|(ip, _)| ip.clone())
            .collect()
    }

    /// Clear all cached data
    pub fn clear(&mut self) {
        self.devices.clear();
        self.last_updated = Utc::now();
    }
}

impl Default for DeviceCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::DeviceType;
    use tempfile::TempDir;

    #[test]
    fn test_empty_cache() {
        let cache = DeviceCache::new();
        assert_eq!(cache.version, 2);
        assert!(cache.is_empty());
        assert_eq!(cache.get_all_devices().len(), 0);
    }

    #[test]
    fn test_cache_save_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut cache = DeviceCache::new();

        // Add a device
        let device = DeviceInfo {
            name: "Test Device".to_string(),
            ip_address: "192.168.1.100".to_string(),
            device_type: DeviceType::BitaxeUltra,
            serial_number: Some("ABC123".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
        };
        cache.add_device(device.clone());

        // Save cache
        cache.save(temp_dir.path())?;

        // Load cache
        let loaded_cache = DeviceCache::load(temp_dir.path())?;
        assert_eq!(loaded_cache.version, 2);
        assert_eq!(loaded_cache.devices.len(), 1);
        assert!(loaded_cache.get_device("192.168.1.100").is_some());

        Ok(())
    }

    #[test]
    fn test_cache_load_nonexistent() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache = DeviceCache::load(temp_dir.path())?;
        assert!(cache.is_empty());
        Ok(())
    }

    #[test]
    fn test_cache_prune() {
        let mut cache = DeviceCache::new();

        // Add devices with different last_seen times
        let old_device = DeviceInfo {
            name: "Old Device".to_string(),
            ip_address: "192.168.1.100".to_string(),
            device_type: DeviceType::BitaxeUltra,
            serial_number: Some("OLD123".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now() - Duration::days(10),
            last_seen: Utc::now() - Duration::days(10),
        };

        let new_device = DeviceInfo {
            name: "New Device".to_string(),
            ip_address: "192.168.1.101".to_string(),
            device_type: DeviceType::BitaxeMax,
            serial_number: Some("NEW123".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
        };

        cache.add_device(old_device);
        // Manually set the last_seen to be old
        if let Some(cached) = cache.devices.get_mut("192.168.1.100") {
            cached.last_seen = Utc::now() - Duration::days(10);
        }
        cache.add_device(new_device);

        assert_eq!(cache.devices.len(), 2);

        // Prune devices older than 7 days
        cache.prune(Duration::days(7));
        assert_eq!(cache.devices.len(), 1);
        assert!(cache.get_device("192.168.1.101").is_some());
        assert!(cache.get_device("192.168.1.100").is_none());
    }
}
