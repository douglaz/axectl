use crate::cli::commands::OutputFormat;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;

pub struct MonitorConfig<'a> {
    pub interval: u64,
    pub temp_alert: Option<f64>,
    pub hashrate_alert: Option<f64>,
    pub db: Option<PathBuf>,
    pub type_filter: Option<String>,
    pub type_summary: bool,
    pub format: OutputFormat,
    pub color: bool,
    pub cache_dir: Option<&'a Path>,
}

pub async fn monitor(config: MonitorConfig<'_>) -> Result<()> {
    use crate::api::DeviceStatus;
    use crate::cache::DeviceCache;
    use crate::output::{print_error, print_info, print_json, print_warning};
    use crate::storage::GLOBAL_STORAGE;

    let monitoring_scope = if let Some(ref type_name) = config.type_filter {
        format!("devices of type '{}'", type_name)
    } else {
        "all devices".to_string()
    };

    print_info(
        &format!(
            "Starting continuous monitoring of {} ({}s interval)",
            monitoring_scope, config.interval
        ),
        config.color,
    );
    if let Some(temp) = config.temp_alert {
        print_info(
            &format!("Temperature alert threshold: {:.1}°C", temp),
            config.color,
        );
    }
    if let Some(hashrate) = config.hashrate_alert {
        print_info(
            &format!("Hashrate drop alert threshold: {:.1}%", hashrate),
            config.color,
        );
    }
    if config.type_summary {
        print_info("Showing per-type summaries", config.color);
    }
    print_info("Press Ctrl+C to stop monitoring", config.color);

    let mut previous_hashrates: HashMap<String, f64> = HashMap::new();
    let mut alert_count = 0;
    let mut cache_in_use = false;
    let mut cache_instance: Option<DeviceCache> = None;

    // Load cache if available and storage is empty
    if let Some(cache_path) = config.cache_dir {
        match DeviceCache::load(cache_path) {
            Ok(cache) => {
                if !cache.is_empty() {
                    cache_instance = Some(cache);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load cache: {}", e);
            }
        }
    }

    loop {
        let devices = if let Some(ref type_name) = config.type_filter {
            // Monitor only devices of the specified type
            match GLOBAL_STORAGE.get_online_devices_by_type_filter(type_name) {
                Ok(devices) => {
                    if devices.is_empty() {
                        // Fallback to cache if storage is empty
                        if let Some(ref cache) = cache_instance {
                            let cached_devices = cache.get_online_devices_by_type_filter(type_name);
                            if !cached_devices.is_empty() {
                                cache_in_use = true;
                                cached_devices
                            } else {
                                devices
                            }
                        } else {
                            devices
                        }
                    } else {
                        devices
                    }
                }
                Err(e) => {
                    if matches!(config.format, OutputFormat::Text) {
                        print_error(&format!("Failed to get devices by type: {}", e), config.color);
                    }
                    sleep(Duration::from_secs(config.interval)).await;
                    continue;
                }
            }
        } else {
            // Monitor all online devices
            match GLOBAL_STORAGE.get_devices_by_status(DeviceStatus::Online) {
                Ok(devices) => {
                    if devices.is_empty() {
                        // Fallback to cache if storage is empty
                        if let Some(ref cache) = cache_instance {
                            let cached_devices = cache.get_devices_by_status(DeviceStatus::Online);
                            if !cached_devices.is_empty() {
                                cache_in_use = true;
                                cached_devices
                            } else {
                                devices
                            }
                        } else {
                            devices
                        }
                    } else {
                        devices
                    }
                }
                Err(e) => {
                    if matches!(config.format, OutputFormat::Text) {
                        print_error(&format!("Failed to get devices: {}", e), config.color);
                    }
                    sleep(Duration::from_secs(config.interval)).await;
                    continue;
                }
            }
        };

        // Show cache warning on first iteration if using cache
        if cache_in_use && alert_count == 0 && matches!(config.format, OutputFormat::Text) {
            if let Some(ref cache) = cache_instance {
                let age_minutes = cache.age_seconds() / 60;
                print_warning(
                    &format!("📦 Using cached devices ({} minutes old)", age_minutes),
                    config.color,
                );
                print_info("Device stats will be fetched live from network", config.color);
            }
        }

        if devices.is_empty() {
            if matches!(config.format, OutputFormat::Text) {
                if let Some(ref type_name) = config.type_filter {
                    // Check if type filter is valid by looking in both storage and cache
                    let all_devices_storage = GLOBAL_STORAGE
                        .get_devices_by_type_filter(type_name)
                        .unwrap_or_default();
                    let all_devices_cache = if let Some(ref cache) = cache_instance {
                        cache.get_devices_by_type_filter(type_name)
                    } else {
                        Vec::new()
                    };

                    if all_devices_storage.is_empty() && all_devices_cache.is_empty() {
                        print_error(
                            &format!("No devices found for type filter: '{}'", type_name),
                            config.color,
                        );
                        print_info("Available types: bitaxe-ultra, bitaxe-max, bitaxe-gamma, nerdqaxe, bitaxe (all bitaxe), all", config.color);
                        return Ok(());
                    } else {
                        print_info("No online devices of the specified type", config.color);
                    }
                } else {
                    print_info("No online devices to monitor", config.color);
                    print_info("Run 'axectl discover' to find devices", config.color);
                }
            }
            sleep(Duration::from_secs(config.interval)).await;
            continue;
        }

        let mut all_stats = Vec::new();
        let mut alerts = Vec::new();

        // Collect stats from all devices
        for device in &devices {
            match collect_device_stats(device).await {
                Ok(stats) => {
                    // Store stats in global storage
                    if let Err(e) = GLOBAL_STORAGE.store_stats(stats.clone()) {
                        tracing::warn!("Failed to store stats: {}", e);
                    }

                    // Store stats in cache if available
                    if let Some(ref mut cache) = cache_instance {
                        cache.update_device_stats(&device.ip_address, stats.clone());
                    }

                    // Check for alerts
                    if let Some(temp_threshold) = config.temp_alert {
                        if stats.temperature_celsius > temp_threshold {
                            alerts.push(format!(
                                "🌡️ {} temperature alert: {:.1}°C > {:.1}°C",
                                device.name, stats.temperature_celsius, temp_threshold
                            ));
                        }
                    }

                    if let Some(hashrate_threshold) = config.hashrate_alert {
                        if let Some(previous_hashrate) = previous_hashrates.get(&stats.device_id) {
                            let drop_percent = ((previous_hashrate - stats.hashrate_mhs)
                                / previous_hashrate)
                                * 100.0;
                            if drop_percent > hashrate_threshold {
                                alerts.push(format!(
                                    "📉 {} hashrate drop: {:.1}% ({} -> {})",
                                    device.name,
                                    drop_percent,
                                    crate::output::format_hashrate(*previous_hashrate),
                                    crate::output::format_hashrate(stats.hashrate_mhs)
                                ));
                            }
                        }
                        previous_hashrates.insert(stats.device_id.clone(), stats.hashrate_mhs);
                    }

                    all_stats.push(stats);
                }
                Err(e) => {
                    tracing::warn!("Failed to collect stats from {}: {}", device.ip_address, e);
                    // Mark device as offline
                    let _ = GLOBAL_STORAGE
                        .update_device_status(&device.ip_address, DeviceStatus::Offline);

                    // Mark device as offline in cache if available
                    if let Some(ref mut cache) = cache_instance {
                        cache.mark_device_probed(&device.ip_address, false);
                    }

                    alerts.push(format!("🔌 {} went offline: {}", device.name, e));
                }
            }
        }

        // Update device offline detection
        let _ = GLOBAL_STORAGE.mark_stale_devices_offline(config.interval * 3);

        // Output monitoring data
        match config.format {
            OutputFormat::Json => {
                let mut output = serde_json::json!({
                    "monitoring": {
                        "interval_seconds": config.interval,
                        "alerts": alerts,
                        "alert_count": alert_count,
                        "devices_monitored": devices.len(),
                        "devices_responding": all_stats.len()
                    },
                    "statistics": all_stats,
                    "timestamp": chrono::Utc::now()
                });

                // Add type information if filtering by type
                if let Some(ref type_name) = config.type_filter {
                    output["type_filter"] = serde_json::json!({
                        "type_name": type_name,
                        "filter_applied": true
                    });
                } else {
                    // Show overall swarm summary when not filtering by type
                    let swarm_summary = GLOBAL_STORAGE.get_swarm_summary()?;
                    output["summary"] = serde_json::to_value(swarm_summary)?;
                }

                // Add per-type summaries if requested
                if config.type_summary {
                    let type_summaries = GLOBAL_STORAGE.get_all_type_summaries()?;
                    output["type_summaries"] = serde_json::to_value(type_summaries)?;
                }

                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                // Clear screen for monitoring updates
                print!("\x1B[2J\x1B[1;1H");

                let title = if let Some(ref type_name) = config.type_filter {
                    format!(
                        "🔍 Type Monitor '{}' - {}",
                        type_name,
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    )
                } else {
                    format!(
                        "🔍 Swarm Monitor - {}",
                        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
                    )
                };
                println!("{}", title);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                if !all_stats.is_empty() {
                    // Show current monitoring scope summary
                    let total_hashrate: f64 = all_stats.iter().map(|s| s.hashrate_mhs).sum();
                    let total_power: f64 = all_stats.iter().map(|s| s.power_watts).sum();
                    let avg_temp: f64 =
                        all_stats.iter().map(|s| s.temperature_celsius).sum::<f64>()
                            / all_stats.len() as f64;
                    let max_temp: f64 = all_stats
                        .iter()
                        .map(|s| s.temperature_celsius)
                        .fold(0.0, f64::max);

                    let scope_label = if config.type_filter.is_some() {
                        "Type"
                    } else {
                        "Total"
                    };
                    println!(
                        "📊 {} Summary: {} devices | {} | {:.1}W | Avg: {:.1}°C | Max: {:.1}°C",
                        scope_label,
                        all_stats.len(),
                        crate::output::format_hashrate(total_hashrate),
                        total_power,
                        avg_temp,
                        max_temp
                    );

                    // Show individual devices
                    println!();
                    for stats in &all_stats {
                        let temp_color = if stats.temperature_celsius >= 80.0 {
                            "🔥"
                        } else if stats.temperature_celsius >= 70.0 {
                            "🌡️"
                        } else {
                            "🟢"
                        };

                        println!(
                            "{} {} | {} | {:.1}°C | {} | {}",
                            temp_color,
                            stats.device_id,
                            crate::output::format_hashrate(stats.hashrate_mhs),
                            stats.temperature_celsius,
                            crate::output::format_power(stats.power_watts),
                            crate::output::format_uptime(stats.uptime_seconds)
                        );
                    }

                    // Show per-type summaries if requested
                    if config.type_summary {
                        println!();
                        println!("📊 Device Type Summaries:");
                        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                        if let Ok(type_summaries) = GLOBAL_STORAGE.get_all_type_summaries() {
                            if type_summaries.is_empty() {
                                println!("   No devices found");
                            } else {
                                for summary in type_summaries {
                                    let status_indicator = if summary.devices_online > 0 {
                                        "🟢"
                                    } else {
                                        "🔴"
                                    };

                                    println!(
                                        "{} {} ({}/{} online) | {} | {:.1}W | Avg: {:.1}°C",
                                        status_indicator,
                                        summary.type_name,
                                        summary.devices_online,
                                        summary.total_devices,
                                        crate::output::format_hashrate(summary.total_hashrate_mhs),
                                        summary.total_power_watts,
                                        summary.average_temperature
                                    );
                                }
                            }
                        } else {
                            println!("   Failed to retrieve type summaries");
                        }
                    }
                } else {
                    print_error("No devices responding", config.color);
                }

                // Show alerts
                if !alerts.is_empty() {
                    println!();
                    println!("🚨 ALERTS:");
                    for alert in &alerts {
                        print_warning(alert, config.color);
                    }
                    alert_count += alerts.len();
                }

                if alert_count > 0 {
                    println!();
                    print_info(
                        &format!("Total alerts this session: {}", alert_count),
                        config.color,
                    );
                }

                println!();
                print_info(
                    &format!("Next update in {}s... (Ctrl+C to stop)", config.interval),
                    config.color,
                );
            }
        }

        // Save cache if available and updated
        if let (Some(ref cache), Some(cache_path)) = (&cache_instance, config.cache_dir) {
            if let Err(e) = cache.save(cache_path) {
                tracing::warn!("Failed to save cache: {}", e);
            }
        }

        sleep(Duration::from_secs(config.interval)).await;
    }
}

async fn collect_device_stats(device: &crate::api::DeviceInfo) -> Result<crate::api::DeviceStats> {
    let client = crate::api::AxeOsClient::with_timeout(&device.ip_address, Duration::from_secs(5))?;

    let (info, stats) = client.get_complete_info().await?;

    Ok(crate::api::DeviceStats::from_api_responses(
        device.ip_address.clone(),
        &info,
        &stats,
    ))
}
