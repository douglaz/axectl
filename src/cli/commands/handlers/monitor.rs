use crate::cli::commands::{DeviceFilterArg, OutputFormat};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::collections::HashMap;
use std::io::{Write as IoWrite, stdout};
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

pub struct MonitorConfig<'a> {
    pub interval: u64,
    pub temp_alert: Option<f64>,
    pub hashrate_alert: Option<f64>,
    pub type_filter: Option<DeviceFilterArg>,
    pub type_summary: bool,
    pub format: OutputFormat,
    pub color: bool,
    pub cache_dir: Option<&'a Path>,
}

pub async fn monitor(config: MonitorConfig<'_>) -> Result<()> {
    use crate::api::{DeviceStatus, SwarmSummary};
    use crate::cache::DeviceCache;
    use crate::output::{print_error, print_info, print_json, print_warning};

    // Set up alternate screen for text mode to prevent flicker.
    // The alternate screen is a separate buffer that doesn't affect the main terminal scrollback.
    let use_alternate_screen = matches!(config.format, OutputFormat::Text);

    if use_alternate_screen {
        let mut stdout_handle = stdout();
        // EnterAlternateScreen: Switch to a separate screen buffer (like vim or less does)
        // Hide: Hide the cursor for cleaner display during updates
        execute!(stdout_handle, EnterAlternateScreen, Hide)?;
    }

    // Define a local RAII guard that will automatically restore the terminal when dropped.
    // RAII (Resource Acquisition Is Initialization) ensures cleanup happens automatically.
    struct CleanupGuard {
        use_alternate_screen: bool,
    }

    // The Drop trait is Rust's destructor mechanism. This code runs automatically
    // when the CleanupGuard instance is destroyed (goes out of scope).
    impl Drop for CleanupGuard {
        fn drop(&mut self) {
            if self.use_alternate_screen {
                let mut stdout_handle = stdout();
                // Restore the terminal to its original state:
                // - LeaveAlternateScreen: Switch back to the main terminal buffer
                // - Show: Make the cursor visible again
                // We use `let _ =` to explicitly ignore errors because:
                // 1. We're in a destructor, so we can't propagate errors
                // 2. We want cleanup to be best-effort
                // 3. The terminal will reset when the process ends anyway
                let _ = execute!(stdout_handle, LeaveAlternateScreen, Show);
                let _ = stdout_handle.flush();
            }
        }
    }

    // Create the guard. The underscore prefix (_cleanup) tells Rust we won't use this variable,
    // but we want to keep it alive until the function ends.
    // This guard ensures the terminal is restored even if the function panics or returns early.
    let _cleanup = CleanupGuard {
        use_alternate_screen,
    };

    // Require cache_dir for monitor command
    let cache_path = match config.cache_dir {
        Some(path) => path,
        None => {
            match config.format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "error": "Cache directory required",
                        "message": "Use --cache-dir to specify where device data is stored",
                        "example": "axectl monitor --cache-dir ~/devices"
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    print_error("Cache directory required for monitor command", config.color);
                    print_info(
                        "Use --cache-dir to specify where device data is stored",
                        config.color,
                    );
                    print_info(
                        "Example: axectl monitor --cache-dir ~/devices",
                        config.color,
                    );
                }
            }
            return Ok(());
        }
    };

    // Track previous hashrates for drop detection
    let mut previous_hashrates: HashMap<String, f64> = HashMap::new();
    let mut alert_count = 0;

    // Load cache
    let mut cache = DeviceCache::load(cache_path)?;
    if !cache.is_empty() && matches!(config.format, OutputFormat::Text) {
        print_info(
            &format!("📦 Loaded {} device(s) from cache", cache.device_count()),
            config.color,
        );
    }

    loop {
        // Get devices from cache based on filter
        let devices = if let Some(ref device_filter_arg) = config.type_filter {
            let filter = device_filter_arg.0;
            cache.get_online_devices_by_filter(filter)
        } else {
            cache.get_devices_by_status(DeviceStatus::Online)
        };

        if devices.is_empty() {
            if matches!(config.format, OutputFormat::Text) {
                if let Some(ref type_filter) = config.type_filter {
                    print_warning(
                        &format!("No online {} devices found", type_filter),
                        config.color,
                    );
                } else {
                    print_warning("No online devices found", config.color);
                }
                print_info(
                    &format!(
                        "Run 'axectl discover --cache-dir {}' to find devices",
                        cache_path.display()
                    ),
                    config.color,
                );
            }
            sleep(Duration::from_secs(config.interval)).await;
            continue;
        }

        // Collect stats from all devices
        let mut device_stats = Vec::new();
        let mut alerts = Vec::new();

        for device in &devices {
            match collect_device_stats(device).await {
                Ok(stats) => {
                    // Check for alerts
                    if let Some(temp_threshold) = config.temp_alert
                        && stats.temperature_celsius > temp_threshold
                    {
                        alerts.push(format!(
                            "🌡️ {} temperature alert: {:.1}°C > {:.1}°C",
                            device.name, stats.temperature_celsius, temp_threshold
                        ));
                    }

                    if let Some(hashrate_threshold) = config.hashrate_alert {
                        if let Some(previous_hashrate) = previous_hashrates.get(&device.ip_address)
                        {
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
                        previous_hashrates.insert(device.ip_address.clone(), stats.hashrate_mhs);
                    }

                    // Update stats in cache
                    cache.update_device_stats(&device.ip_address, stats.clone());

                    device_stats.push(Some(stats));
                }
                Err(e) => {
                    tracing::warn!("Failed to collect stats from {}: {}", device.ip_address, e);

                    // Mark device as probed but offline
                    cache.mark_device_probed(&device.ip_address, false);
                    device_stats.push(None);

                    // Add offline alert
                    alerts.push(format!("🔌 {} went offline", device.name));
                }
            }
        }

        // Save cache after collecting stats
        if let Err(e) = cache.save(cache_path) {
            tracing::warn!("Failed to save cache: {}", e);
        }

        // Update alert count
        alert_count += alerts.len();

        // Output results
        match config.format {
            OutputFormat::Json => {
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

                // Calculate swarm summary from collected stats
                let online_devices: Vec<_> = devices
                    .iter()
                    .zip(device_stats.iter())
                    .filter_map(|(d, stats)| stats.as_ref().map(|s| (d, s)))
                    .collect();

                let swarm_summary = if online_devices.is_empty() {
                    SwarmSummary::default()
                } else {
                    SwarmSummary {
                        total_devices: devices.len(),
                        devices_online: online_devices.len(),
                        devices_offline: devices.len() - online_devices.len(),
                        total_hashrate_mhs: online_devices
                            .iter()
                            .map(|(_, s)| s.hashrate_mhs)
                            .sum(),
                        total_power_watts: online_devices.iter().map(|(_, s)| s.power_watts).sum(),
                        average_temperature: online_devices
                            .iter()
                            .map(|(_, s)| s.temperature_celsius)
                            .sum::<f64>()
                            / online_devices.len() as f64,
                        average_efficiency: if online_devices
                            .iter()
                            .map(|(_, s)| s.power_watts)
                            .sum::<f64>()
                            > 0.0
                        {
                            online_devices
                                .iter()
                                .map(|(_, s)| s.hashrate_mhs)
                                .sum::<f64>()
                                / online_devices
                                    .iter()
                                    .map(|(_, s)| s.power_watts)
                                    .sum::<f64>()
                        } else {
                            0.0
                        },
                    }
                };

                let mut output = serde_json::json!({
                    "devices": devices_with_stats,
                    "summary": swarm_summary,
                    "timestamp": chrono::Utc::now()
                });

                if !alerts.is_empty() {
                    output["alerts"] = serde_json::json!(alerts);
                    output["alert_count"] = serde_json::json!(alert_count);
                }

                if config.type_summary {
                    let type_summaries = cache.get_type_summaries();
                    output["type_summaries"] = serde_json::to_value(type_summaries)?;
                }

                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                use crate::output::{
                    ColoredTemperature, format_hashrate, format_power, format_table, format_uptime,
                };
                use std::fmt::Write as FmtWrite;
                use tabled::Tabled;

                // Buffer all output to reduce flickering
                let mut output_buffer = String::new();

                #[derive(Tabled)]
                struct MonitorTableRow {
                    #[tabled(rename = "Name")]
                    name: String,
                    #[tabled(rename = "IP")]
                    ip_address: String,
                    #[tabled(rename = "Type")]
                    device_type: String,
                    #[tabled(rename = "Hashrate")]
                    hashrate: String,
                    #[tabled(rename = "Temp")]
                    temperature: String,
                    #[tabled(rename = "Power")]
                    power: String,
                    #[tabled(rename = "Uptime")]
                    uptime: String,
                }

                let table_rows: Vec<MonitorTableRow> = devices
                    .iter()
                    .zip(device_stats.iter())
                    .map(|(device, stats)| {
                        if let Some(stats) = stats {
                            MonitorTableRow {
                                name: device.name.clone(),
                                ip_address: device.ip_address.clone(),
                                device_type: device.device_type.as_str().to_string(),
                                hashrate: format_hashrate(stats.hashrate_mhs),
                                temperature: ColoredTemperature::new(
                                    stats.temperature_celsius,
                                    config.color,
                                )
                                .to_string(),
                                power: format_power(stats.power_watts),
                                uptime: format_uptime(stats.uptime_seconds),
                            }
                        } else {
                            MonitorTableRow {
                                name: device.name.clone(),
                                ip_address: device.ip_address.clone(),
                                device_type: device.device_type.as_str().to_string(),
                                hashrate: "-".to_string(),
                                temperature: "-".to_string(),
                                power: "-".to_string(),
                                uptime: "-".to_string(),
                            }
                        }
                    })
                    .collect();

                writeln!(
                    &mut output_buffer,
                    "{}",
                    format_table(table_rows, config.color)
                )?;

                // Show summary
                let online_stats: Vec<_> = device_stats.iter().filter_map(|s| s.as_ref()).collect();
                if !online_stats.is_empty() {
                    let total_hashrate: f64 = online_stats.iter().map(|s| s.hashrate_mhs).sum();
                    let total_power: f64 = online_stats.iter().map(|s| s.power_watts).sum();
                    let avg_temp: f64 = online_stats
                        .iter()
                        .map(|s| s.temperature_celsius)
                        .sum::<f64>()
                        / online_stats.len() as f64;

                    writeln!(&mut output_buffer)?;
                    writeln!(
                        &mut output_buffer,
                        "ℹ Summary: {} devices, {} total, {:.1}W total, {:.1}°C avg",
                        online_stats.len(),
                        format_hashrate(total_hashrate),
                        total_power,
                        avg_temp
                    )?;
                }

                // Show alerts if any
                if !alerts.is_empty() {
                    writeln!(&mut output_buffer)?;
                    writeln!(&mut output_buffer, "🚨 ALERTS:")?;
                    for alert in &alerts {
                        writeln!(&mut output_buffer, "⚠️ {}", alert)?;
                    }
                }

                // Show type summaries if requested
                if config.type_summary {
                    writeln!(&mut output_buffer)?;
                    writeln!(&mut output_buffer, "📊 Device Type Summaries:")?;
                    writeln!(
                        &mut output_buffer,
                        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    )?;

                    let type_summaries = cache.get_type_summaries();
                    {
                        if type_summaries.is_empty() {
                            writeln!(&mut output_buffer, "   No devices found")?;
                        } else {
                            for summary in type_summaries {
                                let status_indicator = if summary.devices_online > 0 {
                                    "🟢"
                                } else {
                                    "🔴"
                                };
                                writeln!(
                                    &mut output_buffer,
                                    "{} {} ({}/{} online) | {} | {:.1}W | Avg: {:.1}°C",
                                    status_indicator,
                                    summary.type_name,
                                    summary.devices_online,
                                    summary.total_devices,
                                    format_hashrate(summary.total_hashrate_mhs),
                                    summary.total_power_watts,
                                    summary.average_temperature
                                )?;
                            }
                        }
                    }
                }

                writeln!(
                    &mut output_buffer,
                    "ℹ Updating in {}s... (Ctrl+C to stop) | {} total alerts",
                    config.interval, alert_count
                )?;

                // Now write everything to screen at once
                let mut stdout_handle = stdout();
                execute!(
                    stdout_handle,
                    MoveTo(0, 0),
                    Clear(ClearType::FromCursorDown)
                )?;
                write!(stdout_handle, "{}", output_buffer)?;
                stdout_handle.flush()?;
            }
        }

        sleep(Duration::from_secs(config.interval)).await;
    }
}

async fn collect_device_stats(device: &crate::api::Device) -> Result<crate::api::DeviceStats> {
    let client = crate::api::AxeOsClient::with_timeout(&device.ip_address, Duration::from_secs(5))?;
    let (info, stats) = client.get_complete_info().await?;
    Ok(crate::api::DeviceStats::from_api_responses(&info, &stats))
}
