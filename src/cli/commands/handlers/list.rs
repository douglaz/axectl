use crate::cli::commands::OutputFormat;
use anyhow::Result;
use std::path::Path;
use std::time::Duration;
use tabled::Tabled;
use tokio::time::sleep;

/// Arguments for the list command
pub struct ListArgs<'a> {
    pub all: bool,
    pub no_stats: bool,
    pub watch: bool,
    pub interval: u64,
    pub discover: bool,
    pub network: Option<String>,
    pub timeout: u64,
    pub no_mdns: bool,
    pub format: OutputFormat,
    pub color: bool,
    pub cache_dir: Option<&'a Path>,
}

#[derive(Tabled)]
struct DeviceTableRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "IP Address")]
    ip_address: String,
    #[tabled(rename = "Type")]
    device_type: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Hashrate")]
    hashrate: String,
    #[tabled(rename = "Temp")]
    temperature: String,
    #[tabled(rename = "Power")]
    power: String,
    #[tabled(rename = "Fan")]
    fan_speed: String,
    #[tabled(rename = "Uptime")]
    uptime: String,
    #[tabled(rename = "Pool")]
    pool: String,
}

#[derive(Tabled)]
struct BasicDeviceTableRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "IP Address")]
    ip_address: String,
    #[tabled(rename = "Type")]
    device_type: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Last Seen")]
    last_seen: String,
}

pub async fn list(args: ListArgs<'_>) -> Result<()> {
    use crate::api::DeviceStatus;
    use crate::cache::DeviceCache;
    use crate::output::{
        format_hashrate, format_power, format_table, format_uptime, print_error, print_info,
        print_json, print_success, print_warning, ColoredTemperature,
    };
    use crate::storage::GLOBAL_STORAGE;

    // Require cache_dir for list command
    let cache_path = match args.cache_dir {
        Some(path) => path,
        None => {
            match args.format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "error": "Cache directory required",
                        "message": "Use --cache-dir to specify where device data is stored",
                        "example": "axectl list --cache-dir ~/devices"
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    print_error("Cache directory required for list command", args.color);
                    print_info(
                        "Use --cache-dir to specify where device data is stored",
                        args.color,
                    );
                    print_info("Example: axectl list --cache-dir ~/devices", args.color);
                }
            }
            return Ok(());
        }
    };

    loop {
        // Perform discovery if requested
        if args.discover {
            eprintln!(); // Add spacing
            print_info("🔍 Performing network discovery...", args.color);
            match super::discovery::perform_discovery(
                args.network.clone(),
                args.timeout,
                !args.no_mdns,
                Some(cache_path),
                args.color,
            )
            .await
            {
                Ok(discovered) => {
                    print_success(
                        &format!("✓ Discovery complete, found {} device(s)", discovered.len()),
                        args.color,
                    );
                    eprintln!(); // Add spacing
                }
                Err(e) => {
                    print_warning(&format!("⚠️ Discovery failed: {}", e), args.color);
                }
            }
        }

        // Try to load from cache first
        let mut devices = Vec::new();
        let mut _from_cache = false;
        let cache_age_minutes;

        match DeviceCache::load(cache_path) {
            Ok(cache) => {
                if !cache.is_empty() {
                    devices = if args.all {
                        cache.get_all_devices()
                    } else {
                        cache.get_devices_by_status(DeviceStatus::Online)
                    };
                    _from_cache = true;
                    cache_age_minutes = cache.age_seconds() / 60;

                    if !devices.is_empty() && args.format == OutputFormat::Text && !args.watch {
                        print_warning(
                            &format!(
                                "📦 Showing cached devices ({} minutes old)",
                                cache_age_minutes
                            ),
                            args.color,
                        );
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to load cache: {}", e);
                // Try memory storage as fallback
                devices = if args.all {
                    GLOBAL_STORAGE.get_all_devices()?
                } else {
                    GLOBAL_STORAGE.get_devices_by_status(DeviceStatus::Online)?
                };
            }
        }

        if devices.is_empty() {
            match args.format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "devices": [],
                        "total": 0,
                        "filter": if args.all { "all" } else { "online_only" },
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    if args.all {
                        print_info("No devices found", args.color);
                    } else {
                        print_info("No online devices found", args.color);
                        print_info("Use --all to show offline devices", args.color);
                    }
                    print_info(
                        &format!(
                            "Run 'axectl discover --cache-dir {}' to find devices",
                            cache_path.display()
                        ),
                        args.color,
                    );
                }
            }
            return Ok(());
        }

        // Collect stats if not in no-stats mode
        let mut device_stats = Vec::new();
        if !args.no_stats {
            for device in &devices {
                if device.status == DeviceStatus::Online {
                    match collect_device_stats(device).await {
                        Ok(stats) => {
                            // Store stats in global storage
                            if let Err(e) = GLOBAL_STORAGE.store_stats(stats.clone()) {
                                tracing::warn!("Failed to store stats: {}", e);
                            }
                            device_stats.push(Some(stats));
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to collect stats from {}: {}",
                                device.ip_address,
                                e
                            );
                            // Mark device as offline if we can't get stats
                            let _ = GLOBAL_STORAGE
                                .update_device_status(&device.ip_address, DeviceStatus::Offline);
                            device_stats.push(None);
                        }
                    }
                } else {
                    device_stats.push(None);
                }
            }
        }

        // Output results
        match args.format {
            OutputFormat::Json => {
                if args.no_stats {
                    let output = serde_json::json!({
                        "devices": devices,
                        "total": devices.len(),
                        "filter": if args.all { "all" } else { "online_only" },
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                } else {
                    let devices_with_stats: Vec<serde_json::Value> = devices
                        .iter()
                        .zip(device_stats.iter())
                        .map(|(device, stats)| {
                            let mut device_json = serde_json::to_value(device).unwrap();
                            if let Some(stats) = stats {
                                device_json["stats"] = serde_json::to_value(stats).unwrap();
                            }
                            device_json
                        })
                        .collect();

                    let swarm_summary = GLOBAL_STORAGE.get_swarm_summary()?;
                    let output = serde_json::json!({
                        "devices": devices_with_stats,
                        "total": devices.len(),
                        "filter": if args.all { "all" } else { "online_only" },
                        "summary": swarm_summary,
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
            }
            OutputFormat::Text => {
                if args.watch {
                    // Clear screen for watch mode
                    print!("\x1B[2J\x1B[1;1H");
                }

                if args.no_stats {
                    // Basic table without stats
                    let table_rows: Vec<BasicDeviceTableRow> = devices
                        .iter()
                        .map(|device| BasicDeviceTableRow {
                            name: device.name.clone(),
                            ip_address: device.ip_address.clone(),
                            device_type: device.device_type.as_str().to_string(),
                            status: format!("{:?}", device.status),
                            last_seen: {
                                let duration = chrono::Utc::now() - device.last_seen;
                                if duration.num_seconds() < 60 {
                                    "Just now".to_string()
                                } else if duration.num_minutes() < 60 {
                                    format!("{}m ago", duration.num_minutes())
                                } else if duration.num_hours() < 24 {
                                    format!("{}h ago", duration.num_hours())
                                } else {
                                    format!("{}d ago", duration.num_days())
                                }
                            },
                        })
                        .collect();

                    println!("{}", format_table(table_rows, args.color));
                } else {
                    // Full table with stats
                    let table_rows: Vec<DeviceTableRow> = devices
                        .iter()
                        .zip(device_stats.iter())
                        .map(|(device, stats)| {
                            if let Some(stats) = stats {
                                DeviceTableRow {
                                    name: device.name.clone(),
                                    ip_address: device.ip_address.clone(),
                                    device_type: device.device_type.as_str().to_string(),
                                    status: format!("{:?}", device.status),
                                    hashrate: format_hashrate(stats.hashrate_mhs),
                                    temperature: ColoredTemperature::new(
                                        stats.temperature_celsius,
                                        args.color,
                                    )
                                    .to_string(),
                                    power: format_power(stats.power_watts),
                                    fan_speed: format!("{}", stats.fan_speed_rpm),
                                    uptime: format_uptime(stats.uptime_seconds),
                                    pool: stats.pool_url.as_deref().unwrap_or("-").to_string(),
                                }
                            } else {
                                DeviceTableRow {
                                    name: device.name.clone(),
                                    ip_address: device.ip_address.clone(),
                                    device_type: device.device_type.as_str().to_string(),
                                    status: format!("{:?}", device.status),
                                    hashrate: "-".to_string(),
                                    temperature: "-".to_string(),
                                    power: "-".to_string(),
                                    fan_speed: "-".to_string(),
                                    uptime: "-".to_string(),
                                    pool: "-".to_string(),
                                }
                            }
                        })
                        .collect();

                    println!("{}", format_table(table_rows, args.color));

                    // Show summary if we have stats for multiple devices
                    let online_stats: Vec<_> =
                        device_stats.iter().filter_map(|s| s.as_ref()).collect();
                    if online_stats.len() > 1 {
                        let total_hashrate: f64 = online_stats.iter().map(|s| s.hashrate_mhs).sum();
                        let total_power: f64 = online_stats.iter().map(|s| s.power_watts).sum();
                        let avg_temp: f64 = online_stats
                            .iter()
                            .map(|s| s.temperature_celsius)
                            .sum::<f64>()
                            / online_stats.len() as f64;

                        println!();
                        print_info(
                            &format!(
                                "Summary: {} devices, {} total, {:.1}W total, {:.1}°C avg",
                                online_stats.len(),
                                format_hashrate(total_hashrate),
                                total_power,
                                avg_temp
                            ),
                            args.color,
                        );
                    }
                }

                print_info(
                    &format!(
                        "Total: {} device(s) {}",
                        devices.len(),
                        if args.all { "" } else { "(online only)" }
                    ),
                    args.color,
                );
            }
        }

        if !args.watch {
            break;
        }

        if !matches!(args.format, OutputFormat::Json) {
            print_info(
                &format!(
                    "Updating in {interval}s... (Ctrl+C to stop)",
                    interval = args.interval
                ),
                args.color,
            );
        }
        sleep(Duration::from_secs(args.interval)).await;
    }

    Ok(())
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
