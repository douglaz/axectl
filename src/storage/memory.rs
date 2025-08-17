use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

use crate::api::models::{
    DeviceGroup, DeviceInfo, DeviceStats, DeviceStatus, DeviceType, SwarmSummary, TypeSummary,
};

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
    groups: HashMap<String, DeviceGroup>,
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
                groups: HashMap::new(),
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

    /// Get devices by device type
    pub fn get_devices_by_type(&self, device_type: DeviceType) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner
            .devices
            .values()
            .filter(|device| device.device_type == device_type)
            .cloned()
            .collect())
    }

    /// Get devices by device type and status
    pub fn get_devices_by_type_and_status(
        &self,
        device_type: DeviceType,
        status: DeviceStatus,
    ) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner
            .devices
            .values()
            .filter(|device| device.device_type == device_type && device.status == status)
            .cloned()
            .collect())
    }

    /// Get type summary for a specific device type
    pub fn get_type_summary(&self, device_type: DeviceType) -> Result<TypeSummary> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        let devices: Vec<DeviceInfo> = inner.devices.values().cloned().collect();
        let stats: Vec<DeviceStats> = inner.latest_stats.values().cloned().collect();

        Ok(TypeSummary::from_devices_and_stats(
            device_type,
            &devices,
            &stats,
        ))
    }

    /// Get type summaries for all device types that have devices
    pub fn get_all_type_summaries(&self) -> Result<Vec<TypeSummary>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        let devices: Vec<DeviceInfo> = inner.devices.values().cloned().collect();
        let stats: Vec<DeviceStats> = inner.latest_stats.values().cloned().collect();

        Ok(TypeSummary::from_all_devices_and_stats(&devices, &stats))
    }

    /// Get devices filtered by type name (supports CLI filter names)
    pub fn get_devices_by_type_filter(&self, filter: &str) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        if filter == "all" {
            return Ok(inner.devices.values().cloned().collect());
        }

        Ok(inner
            .devices
            .values()
            .filter(|device| device.device_type.matches_filter(filter))
            .cloned()
            .collect())
    }

    /// Get online devices filtered by type name
    pub fn get_online_devices_by_type_filter(&self, filter: &str) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        if filter == "all" {
            return Ok(inner
                .devices
                .values()
                .filter(|device| matches!(device.status, DeviceStatus::Online))
                .cloned()
                .collect());
        }

        Ok(inner
            .devices
            .values()
            .filter(|device| {
                device.device_type.matches_filter(filter)
                    && matches!(device.status, DeviceStatus::Online)
            })
            .cloned()
            .collect())
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
            group_count: inner.groups.len(),
            uptime_seconds: (Utc::now() - inner.created_at).num_seconds() as u64,
        })
    }

    // Group management methods

    /// Create a new device group
    pub fn create_group(&self, name: String, description: Option<String>) -> Result<DeviceGroup> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        let group = DeviceGroup::new(name, description);
        inner.groups.insert(group.id.clone(), group.clone());
        Ok(group)
    }

    /// Get all groups
    pub fn get_all_groups(&self) -> Result<Vec<DeviceGroup>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner.groups.values().cloned().collect())
    }

    /// Get a specific group by ID
    pub fn get_group(&self, group_id: &str) -> Result<Option<DeviceGroup>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;
        Ok(inner.groups.get(group_id).cloned())
    }

    /// Find group by name or ID
    pub fn find_group(&self, identifier: &str) -> Result<Option<DeviceGroup>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        // Try ID first
        if let Some(group) = inner.groups.get(identifier) {
            return Ok(Some(group.clone()));
        }

        // Try name match
        for group in inner.groups.values() {
            if group.name == identifier {
                return Ok(Some(group.clone()));
            }
        }

        Ok(None)
    }

    /// Update group details (name, description, tags)
    pub fn update_group(
        &self,
        group_id: &str,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        if let Some(group) = inner.groups.get_mut(group_id) {
            if let Some(new_name) = name {
                group.name = new_name;
            }
            if let Some(new_description) = description {
                group.description = Some(new_description);
            }
            group.updated_at = Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete a group
    pub fn delete_group(&self, group_id: &str) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;
        Ok(inner.groups.remove(group_id).is_some())
    }

    /// Add device to group
    pub fn add_device_to_group(&self, group_id: &str, device_id: String) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        // Check if device exists
        if !inner.devices.contains_key(&device_id) {
            return Err(anyhow::anyhow!("Device not found: {}", device_id));
        }

        if let Some(group) = inner.groups.get_mut(group_id) {
            group.add_device(device_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Remove device from group
    pub fn remove_device_from_group(&self, group_id: &str, device_id: &str) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        if let Some(group) = inner.groups.get_mut(group_id) {
            Ok(group.remove_device(device_id))
        } else {
            Ok(false)
        }
    }

    /// Add tag to group
    pub fn add_tag_to_group(&self, group_id: &str, tag: String) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        if let Some(group) = inner.groups.get_mut(group_id) {
            group.add_tag(tag);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Remove tag from group
    pub fn remove_tag_from_group(&self, group_id: &str, tag: &str) -> Result<bool> {
        let mut inner = self
            .inner
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;

        if let Some(group) = inner.groups.get_mut(group_id) {
            Ok(group.remove_tag(tag))
        } else {
            Ok(false)
        }
    }

    /// Get devices in a specific group
    pub fn get_group_devices(&self, group_id: &str) -> Result<Vec<DeviceInfo>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        if let Some(group) = inner.groups.get(group_id) {
            let devices: Vec<DeviceInfo> = group
                .device_ids
                .iter()
                .filter_map(|device_id| inner.devices.get(device_id).cloned())
                .collect();
            Ok(devices)
        } else {
            Err(anyhow::anyhow!("Group not found: {}", group_id))
        }
    }

    // NOTE: Removed get_group_summary - replaced with type-based summaries

    /// Get devices filtered by group and status
    pub fn get_group_devices_by_status(
        &self,
        group_id: &str,
        status: DeviceStatus,
    ) -> Result<Vec<DeviceInfo>> {
        let devices = self.get_group_devices(group_id)?;
        Ok(devices
            .into_iter()
            .filter(|device| device.status == status)
            .collect())
    }

    /// Find groups containing a specific device
    pub fn find_device_groups(&self, device_id: &str) -> Result<Vec<DeviceGroup>> {
        let inner = self
            .inner
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock"))?;

        let groups: Vec<DeviceGroup> = inner
            .groups
            .values()
            .filter(|group| group.device_ids.contains(&device_id.to_string()))
            .cloned()
            .collect();

        Ok(groups)
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
        inner.groups.clear();

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
            groups: inner.groups.values().cloned().collect(),
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
    pub group_count: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportData {
    pub session_id: String,
    pub exported_at: DateTime<Utc>,
    pub devices: Vec<DeviceInfo>,
    pub latest_stats: Vec<DeviceStats>,
    pub groups: Vec<DeviceGroup>,
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

    #[test]
    fn test_concurrent_device_operations() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());
        let mut handles = vec![];

        // Spawn multiple threads performing device operations
        for i in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let ip = format!("192.168.1.{}", 100 + i);
                let name = format!("Bitaxe-{:03}", i);

                // Add device
                let device = create_test_device(&ip, &name);
                storage_clone.upsert_device(device)?;

                // Update status multiple times
                for _ in 0..5 {
                    storage_clone.update_device_status(&ip, DeviceStatus::Online)?;
                    storage_clone.update_device_status(&ip, DeviceStatus::Offline)?;
                }

                // Read device
                let retrieved = storage_clone.get_device(&ip)?;
                assert!(retrieved.is_some());

                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify final state
        let all_devices = storage.get_all_devices()?;
        assert_eq!(all_devices.len(), 10);

        Ok(())
    }

    #[test]
    fn test_concurrent_stats_operations() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());
        let mut handles = vec![];

        // Spawn multiple threads storing stats
        for i in 0..5 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let device_id = format!("192.168.1.{}", 100 + i);

                // Store multiple stats entries per device
                for j in 0..20 {
                    let mut stats = create_test_stats(&device_id);
                    stats.hashrate_mhs = 500000.0 + (j as f64 * 1000.0);
                    stats.timestamp = Utc::now() - chrono::Duration::seconds(j * 10);
                    storage_clone.store_stats(stats)?;
                }

                Ok(())
            });
            handles.push(handle);
        }

        // Spawn additional threads reading stats concurrently
        for i in 0..3 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let device_id = format!("192.168.1.{}", 100 + i);

                // Read operations
                for _ in 0..50 {
                    let _ = storage_clone.get_device_latest_stats(&device_id)?;
                    let _ = storage_clone.get_device_stats_history(&device_id, Some(10))?;
                    let _ = storage_clone.get_latest_stats()?;
                }

                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify final state
        let all_stats = storage.get_latest_stats()?;
        assert_eq!(all_stats.len(), 5);

        // Check that each device has the full history
        for i in 0..5 {
            let device_id = format!("192.168.1.{}", 100 + i);
            let history = storage.get_device_stats_history(&device_id, None)?;
            assert_eq!(history.len(), 20);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_async_concurrent_operations() -> Result<()> {
        use std::sync::Arc;
        use tokio::task;

        let storage = Arc::new(MemoryStorage::new());
        let mut tasks = vec![];

        // Spawn async tasks performing mixed operations
        for i in 0..20 {
            let storage_clone = Arc::clone(&storage);
            let task = task::spawn(async move {
                let ip = format!("192.168.1.{}", 100 + i);
                let name = format!("Device-{:03}", i);

                // Add device
                let device = create_test_device(&ip, &name);
                storage_clone.upsert_device(device).unwrap();

                // Store stats
                for j in 0..5 {
                    let mut stats = create_test_stats(&ip);
                    stats.hashrate_mhs = 400000.0 + (j as f64 * 10000.0);
                    storage_clone.store_stats(stats).unwrap();
                }

                // Mixed read/write operations
                for _ in 0..10 {
                    let _ = storage_clone.get_device(&ip).unwrap();
                    let _ = storage_clone.get_device_latest_stats(&ip).unwrap();
                    storage_clone
                        .update_device_status(&ip, DeviceStatus::Online)
                        .unwrap();
                }

                // Cleanup operation
                if i % 5 == 0 {
                    let _ = storage_clone.cleanup_old_stats(3).unwrap();
                }
            });
            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            task.await.unwrap();
        }

        // Verify final state
        let summary = storage.get_swarm_summary()?;
        assert_eq!(summary.total_devices, 20);

        // Test storage info
        let info = storage.get_storage_info()?;
        assert_eq!(info.device_count, 20);
        assert!(info.total_stats_entries > 0);

        Ok(())
    }

    #[test]
    fn test_concurrent_cleanup_operations() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());

        // First, populate with data
        for i in 0..5 {
            let device_id = format!("192.168.1.{}", 100 + i);
            let device = create_test_device(&device_id, &format!("Device-{}", i));
            storage.upsert_device(device)?;

            // Add many stats entries
            for j in 0..50 {
                let mut stats = create_test_stats(&device_id);
                stats.timestamp = Utc::now() - chrono::Duration::seconds(j * 30);
                storage.store_stats(stats)?;
            }
        }

        let mut handles = vec![];

        // Spawn multiple threads performing cleanup operations
        for _ in 0..5 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                // Perform cleanup operations
                for _ in 0..10 {
                    let _ = storage_clone.cleanup_old_stats(20)?;
                    let _ = storage_clone.mark_stale_devices_offline(3600)?;
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Spawn threads performing read operations during cleanup
        for i in 0..3 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let device_id = format!("192.168.1.{}", 100 + i);

                for _ in 0..20 {
                    let _ = storage_clone.get_device_stats_history(&device_id, Some(15))?;
                    let _ = storage_clone.get_swarm_summary()?;
                    let _ = storage_clone.get_storage_info()?;
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify data integrity after concurrent operations
        let all_devices = storage.get_all_devices()?;
        assert_eq!(all_devices.len(), 5);

        let all_stats = storage.get_latest_stats()?;
        assert_eq!(all_stats.len(), 5);

        Ok(())
    }

    #[test]
    fn test_concurrent_find_operations() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());

        // Add devices
        for i in 0..10 {
            let ip = format!("192.168.1.{}", 100 + i);
            let name = format!("Miner-{:03}", i);
            let device = create_test_device(&ip, &name);
            storage.upsert_device(device)?;
        }

        let mut handles = vec![];

        // Spawn multiple threads performing find operations
        for i in 0..20 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                // Find by IP
                let ip = format!("192.168.1.{}", 100 + (i % 10));
                let found_by_ip = storage_clone.find_device(&ip)?;
                assert!(found_by_ip.is_some());

                // Find by name
                let name = format!("Miner-{:03}", i % 10);
                let found_by_name = storage_clone.find_device(&name)?;
                assert!(found_by_name.is_some());

                // Try non-existent device
                let non_existent = storage_clone.find_device(&format!("NonExistent-{}", i))?;
                assert!(non_existent.is_none());

                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        Ok(())
    }

    #[test]
    fn test_storage_export_concurrent() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());

        // Add test data
        for i in 0..5 {
            let ip = format!("192.168.1.{}", 100 + i);
            let name = format!("Export-Test-{}", i);
            let device = create_test_device(&ip, &name);
            storage.upsert_device(device)?;

            let stats = create_test_stats(&ip);
            storage.store_stats(stats)?;
        }

        let mut handles = vec![];

        // Spawn multiple threads performing export operations
        for _ in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let export_data = storage_clone.export_data()?;
                assert_eq!(export_data.devices.len(), 5);
                assert_eq!(export_data.latest_stats.len(), 5);
                assert!(!export_data.session_id.is_empty());
                Ok(())
            });
            handles.push(handle);
        }

        // Concurrent reads with exports
        for i in 0..5 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let device_id = format!("192.168.1.{}", 100 + i);
                for _ in 0..20 {
                    let _ = storage_clone.get_device(&device_id)?;
                    let _ = storage_clone.get_device_latest_stats(&device_id)?;
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        Ok(())
    }

    #[test]
    fn test_concurrent_remove_operations() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());

        // Add initial devices
        for i in 0..20 {
            let ip = format!("192.168.1.{}", 100 + i);
            let name = format!("Remove-Test-{}", i);
            let device = create_test_device(&ip, &name);
            storage.upsert_device(device)?;

            // Add stats for each device
            let stats = create_test_stats(&ip);
            storage.store_stats(stats)?;
        }

        let mut handles = vec![];

        // Spawn threads that remove devices
        for i in 0..10 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let ip = format!("192.168.1.{}", 100 + i);
                let removed = storage_clone.remove_device(&ip)?;
                // Should return true since device exists
                assert!(removed);

                // Try to remove again (should return false)
                let removed_again = storage_clone.remove_device(&ip)?;
                assert!(!removed_again);

                Ok(())
            });
            handles.push(handle);
        }

        // Spawn threads that read devices concurrently
        for i in 10..20 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let ip = format!("192.168.1.{}", 100 + i);
                for _ in 0..10 {
                    let device = storage_clone.get_device(&ip)?;
                    // Device should still exist until removed
                    if device.is_some() {
                        assert_eq!(device.unwrap().ip_address, ip);
                    }
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify final state: should have 10 devices remaining
        let remaining_devices = storage.get_all_devices()?;
        assert_eq!(remaining_devices.len(), 10);

        // Verify that stats were also removed for deleted devices
        let all_stats = storage.get_latest_stats()?;
        assert_eq!(all_stats.len(), 10);

        Ok(())
    }

    #[test]
    fn test_concurrent_clear_operations() -> Result<()> {
        use std::sync::Arc;
        use std::thread;

        let storage = Arc::new(MemoryStorage::new());

        // Add initial data
        for i in 0..5 {
            let ip = format!("192.168.1.{}", 100 + i);
            let device = create_test_device(&ip, &format!("Clear-Test-{}", i));
            storage.upsert_device(device)?;

            let stats = create_test_stats(&ip);
            storage.store_stats(stats)?;
        }

        let mut handles = vec![];

        // Spawn a thread that clears all data
        let storage_clone = Arc::clone(&storage);
        let clear_handle = thread::spawn(move || -> Result<()> {
            // Wait a bit to let other operations start
            thread::sleep(std::time::Duration::from_millis(10));
            storage_clone.clear_all()?;
            Ok(())
        });

        // Spawn threads performing read operations
        for i in 0..3 {
            let storage_clone = Arc::clone(&storage);
            let handle = thread::spawn(move || -> Result<()> {
                let device_id = format!("192.168.1.{}", 100 + i);

                // Perform operations that might run during clear
                for _ in 0..50 {
                    let _ = storage_clone.get_device(&device_id)?;
                    let _ = storage_clone.get_all_devices()?;
                    let _ = storage_clone.get_latest_stats()?;

                    // Add small delay to increase chance of interleaving with clear
                    thread::sleep(std::time::Duration::from_millis(1));
                }
                Ok(())
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        clear_handle.join().unwrap()?;
        for handle in handles {
            handle.join().unwrap()?;
        }

        // Verify storage is empty after clear
        let devices = storage.get_all_devices()?;
        assert_eq!(devices.len(), 0);

        let stats = storage.get_latest_stats()?;
        assert_eq!(stats.len(), 0);

        let info = storage.get_storage_info()?;
        assert_eq!(info.device_count, 0);
        assert_eq!(info.latest_stats_count, 0);
        assert_eq!(info.total_stats_entries, 0);

        Ok(())
    }

    #[test]
    fn test_type_based_filtering() -> Result<()> {
        use crate::api::models::DeviceType;

        let storage = MemoryStorage::new();

        // Add devices of different types
        let mut bitaxe_device = create_test_device("192.168.1.100", "bitaxe-1");
        bitaxe_device.device_type = DeviceType::BitaxeMax;
        storage.upsert_device(bitaxe_device)?;

        let mut nerdqaxe_device = create_test_device("192.168.1.101", "nerdqaxe-1");
        nerdqaxe_device.device_type = DeviceType::NerdqaxePlus;
        storage.upsert_device(nerdqaxe_device)?;

        let mut ultra_device = create_test_device("192.168.1.102", "bitaxe-ultra-1");
        ultra_device.device_type = DeviceType::BitaxeUltra;
        storage.upsert_device(ultra_device)?;

        // Test get_devices_by_type
        let bitaxe_max_devices = storage.get_devices_by_type(DeviceType::BitaxeMax)?;
        assert_eq!(bitaxe_max_devices.len(), 1);
        assert_eq!(bitaxe_max_devices[0].name, "bitaxe-1");

        let nerdqaxe_devices = storage.get_devices_by_type(DeviceType::NerdqaxePlus)?;
        assert_eq!(nerdqaxe_devices.len(), 1);
        assert_eq!(nerdqaxe_devices[0].name, "nerdqaxe-1");

        // Test get_devices_by_type_filter
        let all_devices = storage.get_devices_by_type_filter("all")?;
        assert_eq!(all_devices.len(), 3);

        let bitaxe_family = storage.get_devices_by_type_filter("bitaxe")?;
        assert_eq!(bitaxe_family.len(), 2); // BitaxeMax and BitaxeUltra

        let specific_type = storage.get_devices_by_type_filter("bitaxe-max")?;
        assert_eq!(specific_type.len(), 1);
        assert_eq!(specific_type[0].name, "bitaxe-1");

        let nerdqaxe_filter = storage.get_devices_by_type_filter("nerdqaxe")?;
        assert_eq!(nerdqaxe_filter.len(), 1);
        assert_eq!(nerdqaxe_filter[0].name, "nerdqaxe-1");

        // Test get_online_devices_by_type_filter
        let online_all = storage.get_online_devices_by_type_filter("all")?;
        assert_eq!(online_all.len(), 3); // All are online by default

        let online_bitaxe = storage.get_online_devices_by_type_filter("bitaxe")?;
        assert_eq!(online_bitaxe.len(), 2);

        Ok(())
    }

    #[test]
    fn test_type_summary_generation() -> Result<()> {
        use crate::api::models::DeviceType;

        let storage = MemoryStorage::new();

        // Add devices and stats
        let mut bitaxe_device = create_test_device("192.168.1.100", "bitaxe-1");
        bitaxe_device.device_type = DeviceType::BitaxeMax;
        storage.upsert_device(bitaxe_device)?;

        let mut nerdqaxe_device = create_test_device("192.168.1.101", "nerdqaxe-1");
        nerdqaxe_device.device_type = DeviceType::NerdqaxePlus;
        storage.upsert_device(nerdqaxe_device)?;

        // Add stats for both devices
        storage.store_stats(create_test_stats("192.168.1.100"))?;
        storage.store_stats(create_test_stats("192.168.1.101"))?;

        // Test get_type_summary
        let bitaxe_summary = storage.get_type_summary(DeviceType::BitaxeMax)?;
        assert_eq!(bitaxe_summary.device_type, DeviceType::BitaxeMax);
        assert_eq!(bitaxe_summary.type_name, "Bitaxe Max");
        assert_eq!(bitaxe_summary.total_devices, 1);
        assert_eq!(bitaxe_summary.devices_online, 1);

        let nerdqaxe_summary = storage.get_type_summary(DeviceType::NerdqaxePlus)?;
        assert_eq!(nerdqaxe_summary.device_type, DeviceType::NerdqaxePlus);
        assert_eq!(nerdqaxe_summary.type_name, "NerdQaxe++");
        assert_eq!(nerdqaxe_summary.total_devices, 1);
        assert_eq!(nerdqaxe_summary.devices_online, 1);

        // Test get_all_type_summaries
        let all_summaries = storage.get_all_type_summaries()?;
        assert_eq!(all_summaries.len(), 2); // Only types with devices

        let has_bitaxe = all_summaries
            .iter()
            .any(|s| s.device_type == DeviceType::BitaxeMax);
        let has_nerdqaxe = all_summaries
            .iter()
            .any(|s| s.device_type == DeviceType::NerdqaxePlus);
        assert!(has_bitaxe);
        assert!(has_nerdqaxe);

        // Should not include types without devices
        let has_ultra = all_summaries
            .iter()
            .any(|s| s.device_type == DeviceType::BitaxeUltra);
        assert!(!has_ultra);

        Ok(())
    }

    #[test]
    fn test_type_based_status_filtering() -> Result<()> {
        use crate::api::models::{DeviceStatus, DeviceType};

        let storage = MemoryStorage::new();

        // Add online and offline devices
        let mut online_device = create_test_device("192.168.1.100", "bitaxe-online");
        online_device.device_type = DeviceType::BitaxeMax;
        online_device.status = DeviceStatus::Online;
        storage.upsert_device(online_device)?;

        let mut offline_device = create_test_device("192.168.1.101", "bitaxe-offline");
        offline_device.device_type = DeviceType::BitaxeMax;
        offline_device.status = DeviceStatus::Offline;
        storage.upsert_device(offline_device)?;

        // Test get_devices_by_type_and_status
        let online_bitaxe =
            storage.get_devices_by_type_and_status(DeviceType::BitaxeMax, DeviceStatus::Online)?;
        assert_eq!(online_bitaxe.len(), 1);
        assert_eq!(online_bitaxe[0].name, "bitaxe-online");

        let offline_bitaxe =
            storage.get_devices_by_type_and_status(DeviceType::BitaxeMax, DeviceStatus::Offline)?;
        assert_eq!(offline_bitaxe.len(), 1);
        assert_eq!(offline_bitaxe[0].name, "bitaxe-offline");

        // Test get_online_devices_by_type_filter
        let online_filtered = storage.get_online_devices_by_type_filter("bitaxe-max")?;
        assert_eq!(online_filtered.len(), 1);
        assert_eq!(online_filtered[0].name, "bitaxe-online");

        Ok(())
    }
}
