use anyhow::{Context, Result};
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::api::DeviceInfo;

/// Cached device information for faster discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDevice {
    pub name: String,
    pub device_type: String,
    pub mac_address: String,
    pub last_seen: DateTime<Utc>,
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
            version: 1,
            last_updated: Utc::now(),
            devices: HashMap::new(),
        }
    }

    /// Load cache from directory (returns empty cache if file doesn't exist)
    pub fn load(cache_dir: &Path) -> Result<Self> {
        let cache_file = cache_dir.join("devices.json");
        
        if !cache_file.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(&cache_file)
            .with_context(|| format!("Failed to read cache file: {}", cache_file.display()))?;

        let cache: DeviceCache = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse cache file: {}", cache_file.display()))?;

        // Validate cache version
        if cache.version != 1 {
            // If version mismatch, return empty cache (forward compatibility)
            return Ok(Self::new());
        }

        Ok(cache)
    }

    /// Save cache to directory
    pub fn save(&self, cache_dir: &Path) -> Result<()> {
        // Create cache directory if it doesn't exist
        fs::create_dir_all(cache_dir)
            .with_context(|| format!("Failed to create cache directory: {}", cache_dir.display()))?;

        let cache_file = cache_dir.join("devices.json");
        
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize cache")?;

        fs::write(&cache_file, content)
            .with_context(|| format!("Failed to write cache file: {}", cache_file.display()))?;

        Ok(())
    }

    /// Get list of known IP addresses
    pub fn get_known_ips(&self) -> Vec<String> {
        self.devices.keys().cloned().collect()
    }

    /// Update cache with discovered device
    pub fn update_device(&mut self, device: &DeviceInfo) {
        let cached_device = CachedDevice {
            name: device.name.clone(),
            device_type: format!("{:?}", device.device_type),
            mac_address: device.serial_number.clone().unwrap_or_default(),
            last_seen: Utc::now(),
        };

        self.devices.insert(device.ip_address.clone(), cached_device);
        self.last_updated = Utc::now();
    }

    /// Remove old devices that haven't been seen in a while
    pub fn prune_old(&mut self, max_age: Duration) {
        let cutoff = Utc::now() - max_age;
        self.devices.retain(|_, device| device.last_seen > cutoff);
        
        if !self.devices.is_empty() {
            self.last_updated = Utc::now();
        }
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Get cache age in seconds
    pub fn age_seconds(&self) -> i64 {
        (Utc::now() - self.last_updated).num_seconds()
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
    use crate::api::{DeviceStatus, DeviceType};
    use tempfile::TempDir;

    #[test]
    fn test_empty_cache() {
        let cache = DeviceCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.device_count(), 0);
        assert!(cache.get_known_ips().is_empty());
    }

    #[test]
    fn test_cache_save_load() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache_dir = temp_dir.path();

        // Create cache with test device
        let mut cache = DeviceCache::new();
        let device = DeviceInfo {
            name: "test-device".to_string(),
            ip_address: "192.168.1.100".to_string(),
            device_type: DeviceType::BitaxeGamma,
            serial_number: Some("AA:BB:CC:DD:EE:FF".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
        };

        cache.update_device(&device);
        assert_eq!(cache.device_count(), 1);

        // Save cache
        cache.save(cache_dir)?;

        // Load cache
        let loaded_cache = DeviceCache::load(cache_dir)?;
        assert_eq!(loaded_cache.device_count(), 1);
        assert!(loaded_cache.get_known_ips().contains(&"192.168.1.100".to_string()));

        let cached_device = &loaded_cache.devices["192.168.1.100"];
        assert_eq!(cached_device.name, "test-device");
        assert_eq!(cached_device.mac_address, "AA:BB:CC:DD:EE:FF");

        Ok(())
    }

    #[test]
    fn test_cache_load_nonexistent() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache_dir = temp_dir.path().join("nonexistent");

        let cache = DeviceCache::load(&cache_dir)?;
        assert!(cache.is_empty());
        assert_eq!(cache.version, 1);

        Ok(())
    }

    #[test]
    fn test_cache_prune() -> Result<()> {
        let mut cache = DeviceCache::new();
        
        // Add device with old timestamp
        let old_device = CachedDevice {
            name: "old-device".to_string(),
            device_type: "BitaxeGamma".to_string(),
            mac_address: "11:22:33:44:55:66".to_string(),
            last_seen: Utc::now() - Duration::days(8), // 8 days old
        };
        
        // Add device with recent timestamp
        let recent_device = CachedDevice {
            name: "recent-device".to_string(),
            device_type: "NerdqaxePlus".to_string(),
            mac_address: "AA:BB:CC:DD:EE:FF".to_string(),
            last_seen: Utc::now() - Duration::hours(1), // 1 hour old
        };

        cache.devices.insert("192.168.1.100".to_string(), old_device);
        cache.devices.insert("192.168.1.101".to_string(), recent_device);
        assert_eq!(cache.device_count(), 2);

        // Prune devices older than 7 days
        cache.prune_old(Duration::days(7));
        assert_eq!(cache.device_count(), 1);
        assert!(cache.devices.contains_key("192.168.1.101"));
        assert!(!cache.devices.contains_key("192.168.1.100"));

        Ok(())
    }
}