use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::api::models::{DeviceInfo, DeviceStats, DeviceStatus, SwarmSummary};

/// In-memory storage for devices and statistics
/// This provides the default storage without requiring any external dependencies
#[derive(Debug, Clone)]
pub struct MemoryStorage {
    inner: Arc<RwLock<MemoryStorageInner>>,
}

#[derive(Debug)]
struct MemoryStorageInner {
    devices: HashMap<String, DeviceInfo>,
    latest_stats: HashMap<String, DeviceStats>,
    stats_history: HashMap<String, Vec<DeviceStats>>,
    session_id: String,
    created_at: DateTime<Utc>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MemoryStorageInner {
                devices: HashMap::new(),
                latest_stats: HashMap::new(),
                stats_history: HashMap::new(),
                session_id: Uuid::new_v4().to_string(),
                created_at: Utc::now(),
            })),
        }
    }

    /// Add or update a device
    pub fn upsert_device(&self, mut device: DeviceInfo) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        // If device exists, preserve discovery time but update last_seen
        if let Some(existing) = inner.devices.get(&device.ip_address) {
            device.discovered_at = existing.discovered_at;
        }
        device.last_seen = Utc::now();

        inner.devices.insert(device.ip_address.clone(), device);
        Ok(())
    }

    /// Get all devices
    pub fn get_all_devices(&self) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner.devices.values().cloned().collect())
    }

    /// Get devices by status
    pub fn get_devices_by_status(&self, status: DeviceStatus) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner
            .devices
            .values()
            .filter(|device| device.status == status)
            .cloned()
            .collect())
    }

    /// Get a specific device by IP address
    pub fn get_device(&self, ip_address: &str) -> Result<Option<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner.devices.get(ip_address).cloned())
    }

    /// Find device by name or IP
    pub fn find_device(&self, identifier: &str) -> Result<Option<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        // Try IP address first
        if let Some(device) = inner.devices.get(identifier) {
            return Ok(Some(device.clone()));
        }

        // Try name match
        for device in inner.devices.values() {
            if device.name == identifier {
                return Ok(Some(device.clone()));
            }
        }

        Ok(None)
    }

    /// Remove a device
    pub fn remove_device(&self, ip_address: &str) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        let removed_device = inner.devices.remove(ip_address).is_some();
        if removed_device {
            inner.latest_stats.remove(ip_address);
            inner.stats_history.remove(ip_address);
        }

        Ok(removed_device)
    }

    /// Update device status
    pub fn update_device_status(&self, ip_address: &str, status: DeviceStatus) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        if let Some(device) = inner.devices.get_mut(ip_address) {
            device.status = status;
            device.last_seen = Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Store device statistics
    pub fn store_stats(&self, stats: DeviceStats) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        let device_id = stats.device_id.clone();

        // Update latest stats
        inner.latest_stats.insert(device_id.clone(), stats.clone());

        // Add to history
        inner
            .stats_history
            .entry(device_id)
            .or_insert_with(Vec::new)
            .push(stats);

        Ok(())
    }

    /// Get latest statistics for all devices
    pub fn get_latest_stats(&self) -> Result<Vec<DeviceStats>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner.latest_stats.values().cloned().collect())
    }

    /// Get latest statistics for a specific device
    pub fn get_device_latest_stats(&self, device_id: &str) -> Result<Option<DeviceStats>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner.latest_stats.get(device_id).cloned())
    }

    /// Get statistics history for a device
    pub fn get_device_stats_history(
        &self,
        device_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<DeviceStats>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        if let Some(history) = inner.stats_history.get(device_id) {
            let mut stats = history.clone();
            stats.sort_by(|a, b| b.timestamp.cmp(&a.timestamp)); // Most recent first

            if let Some(limit) = limit {
                stats.truncate(limit);
            }

            Ok(stats)
        } else {
            Ok(Vec::new())
        }
    }

    /// Get swarm summary
    pub fn get_swarm_summary(&self) -> Result<SwarmSummary> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        let devices: Vec<DeviceInfo> = inner.devices.values().cloned().collect();
        let stats: Vec<DeviceStats> = inner.latest_stats.values().cloned().collect();

        Ok(SwarmSummary::from_devices_and_stats(&devices, &stats))
    }

    /// Clean up old statistics (keep only recent entries)
    pub fn cleanup_old_stats(&self, max_entries_per_device: usize) -> Result<usize> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;
        let mut cleaned_count = 0;

        for history in inner.stats_history.values_mut() {
            if history.len() > max_entries_per_device {
                // Sort by timestamp (newest first) and keep only the most recent entries
                history.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                let removed = history.len() - max_entries_per_device;
                history.truncate(max_entries_per_device);
                cleaned_count += removed;
            }
        }

        Ok(cleaned_count)
    }

    /// Get storage statistics
    pub fn get_storage_info(&self) -> Result<StorageInfo> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        let total_stats_entries = inner.stats_history.values().map(|v| v.len()).sum();

        Ok(StorageInfo {
            session_id: inner.session_id.clone(),
            created_at: inner.created_at,
            device_count: inner.devices.len(),
            latest_stats_count: inner.latest_stats.len(),
            total_stats_entries,
            uptime_seconds: (Utc::now() - inner.created_at).num_seconds() as u64,
        })
    }

    /// Clear all data
    pub fn clear_all(&self) -> Result<()> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        inner.devices.clear();
        inner.latest_stats.clear();
        inner.stats_history.clear();

        Ok(())
    }

    /// Export all data for backup/sharing
    pub fn export_data(&self) -> Result<ExportData> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        Ok(ExportData {
            session_id: inner.session_id.clone(),
            exported_at: Utc::now(),
            devices: inner.devices.values().cloned().collect(),
            latest_stats: inner.latest_stats.values().cloned().collect(),
        })
    }

    /// Mark devices as offline if they haven't been seen recently
    pub fn mark_stale_devices_offline(&self, timeout_seconds: u64) -> Result<usize> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;
        let cutoff_time = Utc::now() - chrono::Duration::seconds(timeout_seconds as i64);
        let mut marked_offline = 0;

        for device in inner.devices.values_mut() {
            if device.last_seen < cutoff_time && matches!(device.status, DeviceStatus::Online) {
                device.status = DeviceStatus::Offline;
                marked_offline += 1;
            }
        }

        Ok(marked_offline)
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StorageInfo {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub device_count: usize,
    pub latest_stats_count: usize,
    pub total_stats_entries: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportData {
    pub session_id: String,
    pub exported_at: DateTime<Utc>,
    pub devices: Vec<DeviceInfo>,
    pub latest_stats: Vec<DeviceStats>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::{DeviceStatus, DeviceType};

    fn create_test_device(ip: &str, name: &str) -> DeviceInfo {
        DeviceInfo {
            name: name.to_string(),
            ip_address: ip.to_string(),
            device_type: DeviceType::BitaxeUltra,
            serial_number: Some("TEST123".to_string()),
            status: DeviceStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
        }
    }

    fn create_test_stats(device_id: &str) -> DeviceStats {
        DeviceStats {
            device_id: device_id.to_string(),
            timestamp: Utc::now(),
            hashrate_mhs: 500000.0,
            temperature_celsius: 65.0,
            power_watts: 15.0,
            fan_speed_rpm: 3000,
            shares_accepted: 1000,
            shares_rejected: 5,
            uptime_seconds: 3600,
            pool_url: Some("pool.example.com:4334".to_string()),
            wifi_rssi: Some(-45),
            voltage: Some(12.0),
            frequency: Some(600),
        }
    }

    #[test]
    fn test_device_operations() -> Result<()> {
        let storage = MemoryStorage::new();

        // Add device
        let device = create_test_device("192.168.1.100", "Bitaxe-001");
        storage.upsert_device(device.clone())?;

        // Get device
        let retrieved = storage.get_device("192.168.1.100")?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Bitaxe-001");

        // Find by name
        let found = storage.find_device("Bitaxe-001")?;
        assert!(found.is_some());

        // Update status
        storage.update_device_status("192.168.1.100", DeviceStatus::Offline)?;
        let updated = storage.get_device("192.168.1.100")?;
        assert!(matches!(updated.unwrap().status, DeviceStatus::Offline));

        Ok(())
    }

    #[test]
    fn test_stats_operations() -> Result<()> {
        let storage = MemoryStorage::new();

        // Store stats
        let stats = create_test_stats("192.168.1.100");
        storage.store_stats(stats.clone())?;

        // Get latest stats
        let latest = storage.get_device_latest_stats("192.168.1.100")?;
        assert!(latest.is_some());
        assert_eq!(latest.unwrap().hashrate_mhs, 500000.0);

        // Store another entry
        let mut stats2 = create_test_stats("192.168.1.100");
        stats2.hashrate_mhs = 550000.0;
        storage.store_stats(stats2)?;

        // Check history
        let history = storage.get_device_stats_history("192.168.1.100", None)?;
        assert_eq!(history.len(), 2);

        Ok(())
    }

    #[test]
    fn test_swarm_summary() -> Result<()> {
        let storage = MemoryStorage::new();

        // Add devices and stats
        storage.upsert_device(create_test_device("192.168.1.100", "Bitaxe-001"))?;
        storage.upsert_device(create_test_device("192.168.1.101", "Bitaxe-002"))?;

        storage.store_stats(create_test_stats("192.168.1.100"))?;
        storage.store_stats(create_test_stats("192.168.1.101"))?;

        // Get summary
        let summary = storage.get_swarm_summary()?;
        assert_eq!(summary.total_devices, 2);
        assert_eq!(summary.devices_online, 2);
        assert_eq!(summary.total_hashrate_mhs, 1000000.0);

        Ok(())
    }

    #[test]
    fn test_cleanup() -> Result<()> {
        let storage = MemoryStorage::new();

        // Add many stats entries
        for i in 0..10 {
            let mut stats = create_test_stats("192.168.1.100");
            stats.timestamp = Utc::now() - chrono::Duration::seconds(i * 60);
            storage.store_stats(stats)?;
        }

        // Cleanup to keep only 5 entries
        let cleaned = storage.cleanup_old_stats(5)?;
        assert_eq!(cleaned, 5);

        let history = storage.get_device_stats_history("192.168.1.100", None)?;
        assert_eq!(history.len(), 5);

        Ok(())
    }
}
