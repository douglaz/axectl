use crate::cache::get_cache_dir;
use crate::cli::commands::{DeviceFilterArg, OutputFormat};
use alphanumeric_sort::compare_str;
use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::future::join_all;
use std::io::{Write as IoWrite, stdout};
use std::path::Path;
use std::time::Duration;
use tabled::Tabled;
use tokio::time::{sleep, timeout};

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
    pub device_type: Option<DeviceFilterArg>,
    pub temp_alert: Option<f64>,
    pub hashrate_alert: Option<f64>,
    pub type_summary: bool,
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
    use crate::api::{DeviceStatus, SwarmSummary};
    use crate::cache::DeviceCache;
    use crate::output::{
        ColoredTemperature, format_hashrate, format_power, format_table, format_uptime, print_info,
        print_json, print_success, print_warning,
    };
    use std::collections::HashMap;

    // Set up alternate screen for watch mode to prevent flicker.
    // The alternate screen is a separate buffer that doesn't affect the main terminal scrollback.
    // We only use it in watch mode since normal list output should remain in the terminal history.
    let use_alternate_screen = args.watch && matches!(args.format, OutputFormat::Text);

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
    // This guard ensures the terminal is restored even if:
    // - The function returns early with an error (? operator)
    // - The user presses Ctrl+C
    // - The program panics
    let _cleanup = CleanupGuard {
        use_alternate_screen,
    };

    // Track previous hashrates for drop detection
    let mut previous_hashrates: HashMap<String, f64> = HashMap::new();
    let mut alert_count = 0;

    // Get cache directory, using default if not provided
    let cache_path = get_cache_dir(args.cache_dir)?;
    let cache_path = cache_path.as_ref();

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
                    print_warning(&format!("⚠️ Discovery failed: {e}"), args.color);
                }
            }
        }

        // Load from cache
        let mut cache = DeviceCache::load(cache_path)?;

        // Apply type filtering if specified
        let devices = if let Some(ref device_filter_arg) = args.device_type {
            let filter = device_filter_arg.0;
            if args.all {
                cache.get_devices_by_filter(filter)
            } else {
                cache.get_online_devices_by_filter(filter)
            }
        } else if args.all {
            cache.get_all_devices()
        } else {
            cache.get_devices_by_status(DeviceStatus::Online)
        };

        let cache_age_minutes = cache.age_seconds() / 60;
        if !devices.is_empty() && args.format == OutputFormat::Text && !args.watch {
            print_warning(
                &format!(
                    "📦 Showing cached devices ({} minutes old)",
                    cache_age_minutes
                ),
                args.color,
            );
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

        // Note: devices are already in cache, no need to store separately

        // Collect stats if not in no-stats mode
        let mut device_stats = Vec::new();
        let mut alerts = Vec::new();

        if !args.no_stats {
            // Create futures for parallel stats collection
            let stats_futures: Vec<_> = devices
                .iter()
                .map(|device| {
                    let device_clone = device.clone();
                    let watch = args.watch;
                    let temp_alert = args.temp_alert;
                    let hashrate_alert = args.hashrate_alert;
                    let prev_hashrates = previous_hashrates.clone();

                    async move {
                        if device_clone.status != DeviceStatus::Online {
                            return (device_clone, None, Vec::new());
                        }

                        // Use timeout to prevent indefinite waiting (60s for patient timeout)
                        let result =
                            timeout(Duration::from_secs(60), collect_device_stats(&device_clone))
                                .await;

                        let mut local_alerts = Vec::new();

                        match result {
                            Ok(Ok(stats)) => {
                                // Check for alerts if in watch mode
                                if watch {
                                    // Temperature alert
                                    if let Some(temp_threshold) = temp_alert
                                        && stats.temperature_celsius > temp_threshold
                                    {
                                        local_alerts.push(format!(
                                            "🌡️ {} temperature alert: {:.1}°C > {:.1}°C",
                                            device_clone.name,
                                            stats.temperature_celsius,
                                            temp_threshold
                                        ));
                                    }

                                    // Hashrate drop alert
                                    if let Some(hashrate_threshold) = hashrate_alert
                                        && let Some(previous_hashrate) =
                                            prev_hashrates.get(&device_clone.ip_address)
                                    {
                                        let drop_percent = ((previous_hashrate
                                            - stats.hashrate_mhs)
                                            / previous_hashrate)
                                            * 100.0;
                                        if drop_percent > hashrate_threshold {
                                            local_alerts.push(format!(
                                                "📉 {} hashrate drop: {:.1}% ({} -> {})",
                                                device_clone.name,
                                                drop_percent,
                                                format_hashrate(*previous_hashrate),
                                                format_hashrate(stats.hashrate_mhs)
                                            ));
                                        }
                                    }
                                }

                                (device_clone, Some(stats), local_alerts)
                            }
                            Ok(Err(e)) => {
                                tracing::warn!(
                                    "Failed to collect stats from {}: {}",
                                    device_clone.ip_address,
                                    e
                                );

                                // Add offline alert if in watch mode
                                if watch {
                                    local_alerts
                                        .push(format!("🔌 {} went offline", device_clone.name));
                                }

                                (device_clone, None, local_alerts)
                            }
                            Err(_) => {
                                tracing::warn!(
                                    "Timeout collecting stats from {} (60s)",
                                    device_clone.ip_address
                                );

                                // Add timeout alert if in watch mode
                                if watch {
                                    local_alerts
                                        .push(format!("⏱️ {} timeout (60s)", device_clone.name));
                                }

                                (device_clone, None, local_alerts)
                            }
                        }
                    }
                })
                .collect();

            // Execute all stats collection in parallel
            let results = join_all(stats_futures).await;

            // Process results
            for (device, stats_opt, device_alerts) in results {
                if let Some(stats) = &stats_opt {
                    // Update stats in cache
                    cache.update_device_stats(&device.ip_address, stats.clone());

                    // Update previous hashrates for next iteration
                    if args.watch && args.hashrate_alert.is_some() {
                        previous_hashrates.insert(device.ip_address.clone(), stats.hashrate_mhs);
                    }
                } else {
                    // Mark device as offline in cache if failed
                    cache.mark_device_probed(&device.ip_address, false);
                }

                device_stats.push(stats_opt);
                alerts.extend(device_alerts);
            }
        }

        // Update alert count
        alert_count += alerts.len();

        // Output results
        match args.format {
            OutputFormat::Json => {
                if args.no_stats {
                    let mut output = serde_json::json!({
                        "devices": devices,
                        "total": devices.len(),
                        "filter": if args.all { "all" } else { "online_only" },
                        "timestamp": chrono::Utc::now()
                    });

                    // Add type filter info if specified
                    if let Some(ref type_filter) = args.device_type {
                        output["type_filter"] = serde_json::json!(type_filter);
                    }

                    print_json(&output, true)?;
                } else {
                    let devices_with_stats: Vec<serde_json::Value> = devices
                        .iter()
                        .zip(device_stats.iter())
                        .map(|(device, stats)| -> Result<serde_json::Value> {
                            let mut device_json =
                                serde_json::to_value(device).with_context(|| {
                                    format!("Failed to serialize device {name}", name = device.name)
                                })?;
                            if let Some(stats) = stats {
                                device_json["stats"] =
                                    serde_json::to_value(stats).with_context(|| {
                                        format!(
                                            "Failed to serialize stats for device {name}",
                                            name = device.name
                                        )
                                    })?;
                            }
                            Ok(device_json)
                        })
                        .collect::<Result<Vec<_>>>()?;

                    // Calculate swarm summary from devices with stats
                    let online_devices: Vec<_> = devices
                        .iter()
                        .zip(device_stats.iter())
                        .filter_map(|(device, stats)| stats.as_ref().map(|s| (device, s)))
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
                            total_power_watts: online_devices
                                .iter()
                                .map(|(_, s)| s.power_watts)
                                .sum(),
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
                        "total": devices.len(),
                        "filter": if args.all { "all" } else { "online_only" },
                        "summary": swarm_summary,
                        "timestamp": chrono::Utc::now()
                    });

                    // Add type filter info if specified
                    if let Some(ref type_filter) = args.device_type {
                        output["type_filter"] = serde_json::json!(type_filter);
                    }

                    // Add alerts if in watch mode
                    if args.watch && !alerts.is_empty() {
                        output["alerts"] = serde_json::json!(alerts);
                        output["alert_count"] = serde_json::json!(alert_count);
                    }

                    // Add type summaries if requested
                    if args.type_summary {
                        let type_summaries = cache.get_type_summaries();
                        output["type_summaries"] = serde_json::to_value(type_summaries)?;
                    }

                    print_json(&output, true)?;
                }
            }
            OutputFormat::Text => {
                use std::fmt::Write as FmtWrite;

                // Buffer all output if in watch mode to reduce flickering
                let mut output_buffer = if args.watch {
                    Some(String::new())
                } else {
                    None
                };

                if args.no_stats {
                    // Basic table without stats
                    // Sort devices by hostname using natural/alphanumeric sorting
                    let mut sorted_devices: Vec<_> = devices.iter().collect();
                    sorted_devices.sort_by(|a, b| compare_str(&a.name, &b.name));

                    let table_rows: Vec<BasicDeviceTableRow> = sorted_devices
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

                    if let Some(ref mut buffer) = output_buffer {
                        writeln!(buffer, "{}", format_table(table_rows, args.color))?;
                    } else {
                        println!("{}", format_table(table_rows, args.color));
                    }
                } else {
                    // Full table with stats
                    // Sort devices and stats together by hostname using natural/alphanumeric sorting
                    let mut device_stats_pairs: Vec<_> =
                        devices.iter().zip(device_stats.iter()).collect();
                    device_stats_pairs.sort_by(|(a, _), (b, _)| compare_str(&a.name, &b.name));

                    let table_rows: Vec<DeviceTableRow> = device_stats_pairs
                        .iter()
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
                                    fan_speed: format!("{rpm}", rpm = stats.fan_speed_rpm),
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

                    if let Some(ref mut buffer) = output_buffer {
                        writeln!(buffer, "{}", format_table(table_rows, args.color))?;
                    } else {
                        println!("{}", format_table(table_rows, args.color));
                    }

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

                        if let Some(ref mut buffer) = output_buffer {
                            writeln!(buffer)?;
                            writeln!(
                                buffer,
                                "ℹ Summary: {} devices, {} total, {:.1}W total, {:.1}°C avg",
                                online_stats.len(),
                                format_hashrate(total_hashrate),
                                total_power,
                                avg_temp
                            )?;
                        } else {
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
                }

                // Show alerts if any
                if !alerts.is_empty() {
                    if let Some(ref mut buffer) = output_buffer {
                        writeln!(buffer)?;
                        writeln!(buffer, "🚨 ALERTS:")?;
                        for alert in &alerts {
                            writeln!(buffer, "⚠️ {}", alert)?;
                        }
                    } else {
                        println!();
                        println!("🚨 ALERTS:");
                        for alert in &alerts {
                            print_warning(alert, args.color);
                        }
                    }
                }

                // Show type summaries if requested
                if args.type_summary {
                    if args.no_stats {
                        if let Some(ref mut buffer) = output_buffer {
                            writeln!(buffer)?;
                            writeln!(
                                buffer,
                                "ℹ Type summaries require statistics (remove --no-stats flag)"
                            )?;
                        } else {
                            println!();
                            print_info(
                                "Type summaries require statistics (remove --no-stats flag)",
                                args.color,
                            );
                        }
                    } else if let Some(ref mut buffer) = output_buffer {
                        writeln!(buffer)?;
                        writeln!(buffer, "📊 Device Type Summaries:")?;
                        writeln!(
                            buffer,
                            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                        )?;

                        let type_summaries = cache.get_type_summaries();
                        if type_summaries.is_empty() {
                            writeln!(buffer, "   No devices found")?;
                        } else {
                            for summary in type_summaries {
                                let status_indicator = if summary.devices_online > 0 {
                                    "🟢"
                                } else {
                                    "🔴"
                                };

                                writeln!(
                                    buffer,
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
                    } else {
                        println!();
                        println!("📊 Device Type Summaries:");
                        println!(
                            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                        );

                        let type_summaries = cache.get_type_summaries();
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
                                    format_hashrate(summary.total_hashrate_mhs),
                                    summary.total_power_watts,
                                    summary.average_temperature
                                );
                            }
                        }
                    }
                }

                if let Some(ref mut buffer) = output_buffer {
                    writeln!(
                        buffer,
                        "ℹ Total: {} device(s) {}{}",
                        devices.len(),
                        if args.all { "" } else { "(online only)" },
                        if alert_count > 0 {
                            format!(" | {} total alerts this session", alert_count)
                        } else {
                            String::new()
                        }
                    )?;
                } else {
                    print_info(
                        &format!(
                            "Total: {} device(s) {}{}",
                            devices.len(),
                            if args.all { "" } else { "(online only)" },
                            if alert_count > 0 {
                                format!(" | {} total alerts this session", alert_count)
                            } else {
                                String::new()
                            }
                        ),
                        args.color,
                    );
                }

                // If in watch mode, write buffered output to screen all at once
                if let Some(buffer) = output_buffer {
                    let mut stdout_handle = stdout();
                    execute!(
                        stdout_handle,
                        MoveTo(0, 0),
                        Clear(ClearType::FromCursorDown)
                    )?;
                    write!(stdout_handle, "{}", buffer)?;
                    stdout_handle.flush()?;
                }
            }
        }

        if !args.watch {
            break;
        }

        // Save cache if we updated stats
        if !args.no_stats
            && !device_stats.is_empty()
            && let Err(e) = cache.save(cache_path)
        {
            tracing::warn!("Failed to save cache: {}", e);
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
    let client =
        crate::api::AxeOsClient::with_timeout(&device.ip_address, Duration::from_secs(60))?;

    let (info, stats) = client.get_complete_info().await?;

    Ok(crate::api::DeviceStats::from_api_responses(&info, &stats))
}
