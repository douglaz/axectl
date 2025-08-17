use crate::cli::commands::OutputFormat;
use anyhow::Result;
use std::path::Path;
use tabled::Tabled;

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
    #[tabled(rename = "Last Seen")]
    last_seen: String,
}

pub async fn list(
    all: bool,
    format: OutputFormat,
    color: bool,
    cache_dir: Option<&Path>,
) -> Result<()> {
    use crate::api::DeviceStatus;
    use crate::cache::DeviceCache;
    use crate::output::{format_table, print_info, print_json, print_warning};
    use crate::storage::GLOBAL_STORAGE;

    let mut devices = if all {
        GLOBAL_STORAGE.get_all_devices()?
    } else {
        GLOBAL_STORAGE.get_devices_by_status(DeviceStatus::Online)?
    };

    // If no devices in memory storage and cache is available, use cache
    if devices.is_empty() {
        if let Some(cache_path) = cache_dir {
            match DeviceCache::load(cache_path) {
                Ok(cache) => {
                    if !cache.is_empty() {
                        devices = if all {
                            cache.get_all_devices()
                        } else {
                            cache.get_devices_by_status(DeviceStatus::Online)
                        };

                        if !devices.is_empty() && format == OutputFormat::Text {
                            let age_minutes = cache.age_seconds() / 60;
                            print_warning(
                                &format!("📦 Showing cached devices ({} minutes old)", age_minutes),
                                color,
                            );
                            print_info("Run 'axectl discover' to refresh from network", color);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to load cache: {}", e);
                }
            }
        }
    }

    if devices.is_empty() {
        match format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "devices": [],
                    "total": 0,
                    "filter": if all { "all" } else { "online_only" },
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                if all {
                    print_info("No devices found in storage", color);
                    print_info("Run 'axectl discover' to find devices", color);
                } else {
                    print_info("No online devices found", color);
                    print_info("Use --all to show offline devices", color);
                }
            }
        }
        return Ok(());
    }

    match format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "devices": devices,
                "total": devices.len(),
                "filter": if all { "all" } else { "online_only" },
                "timestamp": chrono::Utc::now()
            });
            print_json(&output, true)?;
        }
        OutputFormat::Text => {
            let table_rows: Vec<DeviceTableRow> = devices
                .iter()
                .map(|device| DeviceTableRow {
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

            println!("{}", format_table(table_rows, color));
            print_info(
                &format!(
                    "Total: {} device(s) {}",
                    devices.len(),
                    if all { "" } else { "(online only)" }
                ),
                color,
            );
        }
    }

    Ok(())
}
