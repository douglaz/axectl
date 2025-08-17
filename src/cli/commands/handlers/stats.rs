use crate::cli::commands::OutputFormat;
use anyhow::Result;
use std::path::Path;
use std::time::Duration;
use tabled::Tabled;
use tokio::time::sleep;

#[derive(Tabled)]
struct StatsTableRow {
    #[tabled(rename = "Device")]
    name: String,
    #[tabled(rename = "Hashrate")]
    hashrate: String,
    #[tabled(rename = "Temp")]
    temperature: String,
    #[tabled(rename = "Power")]
    power: String,
    #[tabled(rename = "Fan RPM")]
    fan_speed: String,
    #[tabled(rename = "Uptime")]
    uptime: String,
    #[tabled(rename = "Pool")]
    pool: String,
}

pub async fn stats(
    device: Option<String>,
    watch: bool,
    interval: u64,
    format: OutputFormat,
    color: bool,
    _cache_dir: Option<&Path>,
) -> Result<()> {
    use crate::api::DeviceStatus;
    use crate::output::{
        format_hashrate, format_power, format_table, format_temperature, format_uptime,
        print_error, print_info, print_json,
    };
    use crate::storage::GLOBAL_STORAGE;

    loop {
        let devices = if let Some(device_id) = &device {
            // Find specific device
            if let Some(dev) = GLOBAL_STORAGE.find_device(device_id)? {
                vec![dev]
            } else {
                print_error(&format!("Device not found: {device_id}"), color);
                print_info("Use 'axectl list' to see available devices", color);
                return Ok(());
            }
        } else {
            // Get all online devices
            GLOBAL_STORAGE.get_devices_by_status(DeviceStatus::Online)?
        };

        if devices.is_empty() {
            match format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "error": "No online devices found",
                        "devices": [],
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    print_info("No online devices found", color);
                    print_info("Run 'axectl discover' to find devices", color);
                }
            }
            return Ok(());
        }

        let mut all_stats = Vec::new();

        // Collect stats from all devices
        for device in &devices {
            match collect_device_stats(device).await {
                Ok(stats) => {
                    // Store stats in global storage
                    if let Err(e) = GLOBAL_STORAGE.store_stats(stats.clone()) {
                        tracing::warn!("Failed to store stats: {}", e);
                    }
                    all_stats.push(stats);
                }
                Err(e) => {
                    tracing::warn!("Failed to collect stats from {}: {}", device.ip_address, e);
                    // Mark device as offline
                    let _ = GLOBAL_STORAGE
                        .update_device_status(&device.ip_address, DeviceStatus::Offline);
                }
            }
        }

        // Output results
        match format {
            OutputFormat::Json => {
                let swarm_summary = GLOBAL_STORAGE.get_swarm_summary()?;
                let output = serde_json::json!({
                    "statistics": all_stats,
                    "summary": swarm_summary,
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                if all_stats.is_empty() {
                    print_error("Failed to collect statistics from any device", color);
                } else {
                    let table_rows: Vec<StatsTableRow> = all_stats
                        .iter()
                        .map(|stats| StatsTableRow {
                            name: stats.device_id.clone(),
                            hashrate: format_hashrate(stats.hashrate_mhs),
                            temperature: format_temperature(stats.temperature_celsius, color),
                            power: format_power(stats.power_watts),
                            fan_speed: format!("{fan_speed}", fan_speed = stats.fan_speed_rpm),
                            uptime: format_uptime(stats.uptime_seconds),
                            pool: stats.pool_url.as_deref().unwrap_or("Unknown").to_string(),
                        })
                        .collect();

                    println!("{}", format_table(table_rows, color));

                    // Show summary
                    if all_stats.len() > 1 {
                        let total_hashrate: f64 = all_stats.iter().map(|s| s.hashrate_mhs).sum();
                        let total_power: f64 = all_stats.iter().map(|s| s.power_watts).sum();
                        let avg_temp: f64 =
                            all_stats.iter().map(|s| s.temperature_celsius).sum::<f64>()
                                / all_stats.len() as f64;

                        println!();
                        print_info(
                            &format!(
                                "Summary: {} devices, {} total, {:.1}W total, {:.1}°C avg",
                                all_stats.len(),
                                format_hashrate(total_hashrate),
                                total_power,
                                avg_temp
                            ),
                            color,
                        );
                    }
                }
            }
        }

        if !watch {
            break;
        }

        if !matches!(format, OutputFormat::Json) {
            print_info(
                &format!(
                    "Updating in {interval}s... (Ctrl+C to stop)",
                    interval = interval
                ),
                color,
            );
        }
        sleep(Duration::from_secs(interval)).await;

        if matches!(format, OutputFormat::Text) {
            // Clear screen for watch mode
            print!("\x1B[2J\x1B[1;1H");
        }
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
