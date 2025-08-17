use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::api::{DeviceInfo, DeviceStats, DeviceStatus, DeviceType};

/// Cached device information for faster discovery (legacy v1 format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDeviceV1 {
    pub name: String,
    pub device_type: String,
    pub mac_address: String,
    pub last_seen: DateTime<Utc>,
}

/// Enhanced cached device with full info and stats (v2 format)
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

        // Handle different cache versions
        match cache.version {
            1 => {
                // Migrate v1 cache to v2 format
                tracing::debug!("Migrating cache from v1 to v2 format");
                return Self::migrate_from_v1(&content);
            }
            2 => {
                // Current version
            }
            _ => {
                // Unsupported version, return empty cache
                tracing::warn!(
                    "Unsupported cache version {}, starting fresh",
                    cache.version
                );
                return Ok(Self::new());
            }
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

    /// Migrate v1 cache format to v2
    fn migrate_from_v1(content: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct CacheV1 {
            devices: HashMap<String, CachedDeviceV1>,
        }

        let old_cache: CacheV1 =
            serde_json::from_str(content).context("Failed to parse v1 cache for migration")?;

        let mut new_cache = Self::new();
        for (ip, old_device) in old_cache.devices {
            // Convert old format to new DeviceInfo
            let device_type = match old_device.device_type.as_str() {
                "BitaxeUltra" => DeviceType::BitaxeUltra,
                "BitaxeMax" => DeviceType::BitaxeMax,
                "BitaxeGamma" => DeviceType::BitaxeGamma,
                "NerdqaxePlus" => DeviceType::NerdqaxePlus,
                _ => DeviceType::Unknown,
            };

            let device_info = DeviceInfo {
                name: old_device.name,
                ip_address: ip.clone(),
                device_type,
                serial_number: Some(old_device.mac_address),
                status: DeviceStatus::Offline, // Assume offline for migrated devices
                discovered_at: old_device.last_seen,
                last_seen: old_device.last_seen,
            };

            let cached_device = CachedDevice {
                info: device_info,
                latest_stats: None,
                stats_history: Vec::new(),
                last_seen: old_device.last_seen,
                last_probed: old_device.last_seen,
            };

            new_cache.devices.insert(ip, cached_device);
        }

        Ok(new_cache)
    }

    /// Get list of known IP addresses
    pub fn get_known_ips(&self) -> Vec<String> {
        self.devices.keys().cloned().collect()
    }

    /// Update cache with discovered device
    pub fn update_device(&mut self, device: &DeviceInfo) {
        let now = Utc::now();
        let cached_device = CachedDevice {
            info: device.clone(),
            latest_stats: None,
            stats_history: Vec::new(),
            last_seen: now,
            last_probed: now,
        };

        self.devices
            .insert(device.ip_address.clone(), cached_device);
        self.last_updated = now;
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

    /// Update device with fresh statistics
    pub fn update_device_stats(&mut self, device_id: &str, stats: DeviceStats) {
        if let Some(cached_device) = self.devices.get_mut(device_id) {
            // Update latest stats
            cached_device.latest_stats = Some(stats.clone());

            // Add to history (keep last 10 entries)
            cached_device.stats_history.push(stats);
            if cached_device.stats_history.len() > 10 {
                cached_device.stats_history.remove(0);
            }

            // Update timestamps
            cached_device.last_seen = Utc::now();
            cached_device.last_probed = Utc::now();

            // Update device status to online
            cached_device.info.status = DeviceStatus::Online;
            cached_device.info.last_seen = Utc::now();

            self.last_updated = Utc::now();
        }
    }

    /// Mark device as probed (even if it failed)
    pub fn mark_device_probed(&mut self, device_id: &str, success: bool) {
        if let Some(cached_device) = self.devices.get_mut(device_id) {
            cached_device.last_probed = Utc::now();

            if success {
                cached_device.last_seen = Utc::now();
                cached_device.info.status = DeviceStatus::Online;
                cached_device.info.last_seen = Utc::now();
            } else {
                cached_device.info.status = DeviceStatus::Offline;
            }

            self.last_updated = Utc::now();
        }
    }

    /// Get all devices as DeviceInfo list
    pub fn get_all_devices(&self) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get devices filtered by status
    pub fn get_devices_by_status(&self, status: DeviceStatus) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .filter(|cached| cached.info.status == status)
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get devices filtered by type
    pub fn get_devices_by_type(&self, device_type: DeviceType) -> Vec<DeviceInfo> {
        self.devices
            .values()
            .filter(|cached| cached.info.device_type == device_type)
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get devices filtered by type filter string (supports CLI filter names)
    pub fn get_devices_by_type_filter(&self, filter: &str) -> Vec<DeviceInfo> {
        if filter == "all" {
            return self.get_all_devices();
        }

        self.devices
            .values()
            .filter(|cached| cached.info.device_type.matches_filter(filter))
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Get online devices filtered by type filter string
    pub fn get_online_devices_by_type_filter(&self, filter: &str) -> Vec<DeviceInfo> {
        if filter == "all" {
            return self.get_devices_by_status(DeviceStatus::Online);
        }

        self.devices
            .values()
            .filter(|cached| {
                cached.info.status == DeviceStatus::Online
                    && cached.info.device_type.matches_filter(filter)
            })
            .map(|cached| cached.info.clone())
            .collect()
    }

    /// Find device by name or IP address
    pub fn find_device(&self, identifier: &str) -> Option<&DeviceInfo> {
        // First try exact IP match
        if let Some(cached) = self.devices.get(identifier) {
            return Some(&cached.info);
        }

        // Then try name match
        self.devices
            .values()
            .find(|cached| cached.info.name == identifier)
            .map(|cached| &cached.info)
    }

    /// Get latest stats for a device
    pub fn get_device_stats(&self, device_id: &str) -> Option<&DeviceStats> {
        self.devices
            .get(device_id)
            .and_then(|cached| cached.latest_stats.as_ref())
    }

    /// Get stats history for a device
    pub fn get_device_stats_history(&self, device_id: &str) -> Vec<DeviceStats> {
        self.devices
            .get(device_id)
            .map(|cached| cached.stats_history.clone())
            .unwrap_or_default()
    }

    /// Check if a device needs refresh (based on staleness)
    pub fn device_needs_refresh(&self, device_id: &str, max_age_seconds: i64) -> bool {
        if let Some(cached) = self.devices.get(device_id) {
            let age = (Utc::now() - cached.last_probed).num_seconds();
            age > max_age_seconds
        } else {
            true // Unknown device needs refresh
        }
    }

    /// Get devices that need refresh
    pub fn get_stale_devices(&self, max_age_seconds: i64) -> Vec<String> {
        self.devices
            .iter()
            .filter(|(_, cached)| {
                let age = (Utc::now() - cached.last_probed).num_seconds();
                age > max_age_seconds
            })
            .map(|(ip, _)| ip.clone())
            .collect()
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
        assert!(loaded_cache
            .get_known_ips()
            .contains(&"192.168.1.100".to_string()));

        let cached_device = &loaded_cache.devices["192.168.1.100"];
        assert_eq!(cached_device.info.name, "test-device");
        assert_eq!(
            cached_device.info.serial_number,
            Some("AA:BB:CC:DD:EE:FF".to_string())
        );

        Ok(())
    }

    #[test]
    fn test_cache_load_nonexistent() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let cache_dir = temp_dir.path().join("nonexistent");

        let cache = DeviceCache::load(&cache_dir)?;
        assert!(cache.is_empty());
        assert_eq!(cache.version, 2);

        Ok(())
    }

    #[test]
    fn test_cache_prune() -> Result<()> {
        let mut cache = DeviceCache::new();

        // Add device with old timestamp
        let old_device_info = DeviceInfo {
            name: "old-device".to_string(),
            ip_address: "192.168.1.100".to_string(),
            device_type: DeviceType::BitaxeGamma,
            serial_number: Some("11:22:33:44:55:66".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now() - Duration::days(8),
            last_seen: Utc::now() - Duration::days(8),
        };

        let old_device = CachedDevice {
            info: old_device_info,
            latest_stats: None,
            stats_history: Vec::new(),
            last_seen: Utc::now() - Duration::days(8), // 8 days old
            last_probed: Utc::now() - Duration::days(8),
        };

        // Add device with recent timestamp
        let recent_device_info = DeviceInfo {
            name: "recent-device".to_string(),
            ip_address: "192.168.1.101".to_string(),
            device_type: DeviceType::NerdqaxePlus,
            serial_number: Some("AA:BB:CC:DD:EE:FF".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now() - Duration::hours(1),
            last_seen: Utc::now() - Duration::hours(1),
        };

        let recent_device = CachedDevice {
            info: recent_device_info,
            latest_stats: None,
            stats_history: Vec::new(),
            last_seen: Utc::now() - Duration::hours(1), // 1 hour old
            last_probed: Utc::now() - Duration::hours(1),
        };

        cache
            .devices
            .insert("192.168.1.100".to_string(), old_device);
        cache
            .devices
            .insert("192.168.1.101".to_string(), recent_device);
        assert_eq!(cache.device_count(), 2);

        // Prune devices older than 7 days
        cache.prune_old(Duration::days(7));
        assert_eq!(cache.device_count(), 1);
        assert!(cache.devices.contains_key("192.168.1.101"));
        assert!(!cache.devices.contains_key("192.168.1.100"));

        Ok(())
    }
}
