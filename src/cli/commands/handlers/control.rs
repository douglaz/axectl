use crate::cli::commands::{ControlAction, OutputFormat};
use anyhow::Result;
use std::path::Path;
use std::time::Duration;

pub async fn control(
    device: String,
    action: ControlAction,
    format: OutputFormat,
    color: bool,
    cache_dir: Option<&Path>,
) -> Result<()> {
    use crate::api::{AxeOsClient, SystemUpdateRequest};
    use crate::cache::{DeviceCache, get_cache_dir};
    use crate::output::{print_error, print_info, print_json, print_success};

    // Get cache directory, using default if not provided
    let cache_path = get_cache_dir(cache_dir)?;
    let cache_path_ref = cache_path.as_ref();

    // Load cache to find device
    let cache = DeviceCache::load(cache_path_ref).unwrap_or_else(|_| DeviceCache::new());

    // Find the device
    let device_info = if let Some(dev) = cache.find_device(&device) {
        dev
    } else {
        match format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "success": false,
                    "error": format!("Device not found: {}", device),
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                print_error(&format!("Device not found: {}", device), color);
                print_info("Use 'axectl list' to see available devices", color);
            }
        }
        return Ok(());
    };

    let client = AxeOsClient::with_timeout(&device_info.ip_address, Duration::from_secs(10))?;

    let result = match action {
        ControlAction::SetFanSpeed { speed } => {
            print_info(
                &format!("Setting fan speed to {}% on {}", speed, device_info.name),
                color,
            );
            client.set_fan_speed(speed).await
        }
        ControlAction::Restart => {
            print_info(&format!("Restarting device {}", device_info.name), color);
            client.restart_system().await
        }
        ControlAction::UpdateSettings { settings } => {
            print_info(&format!("Updating settings on {}", device_info.name), color);
            match serde_json::from_str::<SystemUpdateRequest>(&settings) {
                Ok(update_request) => client.update_system(update_request).await,
                Err(e) => Ok(crate::api::CommandResult {
                    success: false,
                    message: format!("Invalid settings JSON: {}", e),
                    data: None,
                    timestamp: chrono::Utc::now(),
                }),
            }
        }
        ControlAction::WifiScan => {
            print_info(
                &format!("Scanning WiFi networks on {}", device_info.name),
                color,
            );
            match client.scan_wifi().await {
                Ok(scan_result) => Ok(crate::api::CommandResult {
                    success: true,
                    message: format!("Found {} WiFi networks", scan_result.networks.len()),
                    data: Some(serde_json::to_value(&scan_result).unwrap()),
                    timestamp: chrono::Utc::now(),
                }),
                Err(e) => Ok(crate::api::CommandResult {
                    success: false,
                    message: format!("WiFi scan failed: {}", e),
                    data: None,
                    timestamp: chrono::Utc::now(),
                }),
            }
        }
        ControlAction::UpdateFirmware { firmware } => {
            print_info(
                &format!(
                    "Updating firmware on {} from {}",
                    device_info.name, firmware
                ),
                color,
            );
            client.update_firmware(&firmware).await
        }
        ControlAction::UpdateAxeOs { axeos } => {
            print_info(
                &format!("Updating AxeOS on {} from {}", device_info.name, axeos),
                color,
            );
            client.update_axeos(&axeos).await
        }
    };

    match result {
        Ok(command_result) => match format {
            OutputFormat::Json => {
                print_json(&command_result, true)?;
            }
            OutputFormat::Text => {
                if command_result.success {
                    print_success(&command_result.message, color);
                    if let Some(data) = &command_result.data
                        && let Some(networks) = data.get("networks")
                    {
                        println!("WiFi Networks:");
                        if let Some(networks_array) = networks.as_array() {
                            for network in networks_array {
                                if let (Some(ssid), Some(rssi)) =
                                    (network.get("ssid"), network.get("rssi"))
                                {
                                    println!(
                                        "  {} ({}dBm)",
                                        ssid.as_str().unwrap_or("Unknown"),
                                        rssi.as_i64().unwrap_or(0)
                                    );
                                }
                            }
                        }
                    }
                } else {
                    print_error(&command_result.message, color);
                }
            }
        },
        Err(e) => match format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                    "device": device_info.name,
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                print_error(&format!("Control command failed: {}", e), color);
            }
        },
    }

    Ok(())
}
