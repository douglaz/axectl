use crate::cli::commands::{ControlAction, OutputFormat};
use anyhow::{Context, Result};
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
                    "error": format!("Device not found: {device}"),
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                print_error(&format!("Device not found: {device}"), color);
                print_info("Use 'axectl list' to see available devices", color);
            }
        }
        return Ok(());
    };

    let client = AxeOsClient::with_timeout(&device_info.ip_address, Duration::from_secs(60))?;

    let result = match action {
        ControlAction::SetFanSpeed { speed } => {
            print_info(
                &format!(
                    "Setting fan speed to {speed}% on {name}",
                    name = device_info.name
                ),
                color,
            );
            client.set_fan_speed(speed).await
        }
        ControlAction::Restart => {
            print_info(
                &format!("Restarting device {name}", name = device_info.name),
                color,
            );
            client.restart_system().await
        }
        ControlAction::UpdateSettings { settings } => {
            print_info(
                &format!("Updating settings on {name}", name = device_info.name),
                color,
            );
            match serde_json::from_str::<SystemUpdateRequest>(&settings) {
                Ok(update_request) => client.update_system(update_request).await,
                Err(e) => Ok(crate::api::CommandResult {
                    success: false,
                    message: format!("Invalid settings JSON: {e}"),
                    data: None,
                    timestamp: chrono::Utc::now(),
                }),
            }
        }
        ControlAction::WifiScan => {
            print_info(
                &format!("Scanning WiFi networks on {name}", name = device_info.name),
                color,
            );
            match client.scan_wifi().await {
                Ok(scan_result) => Ok(crate::api::CommandResult {
                    success: true,
                    message: format!("Found {} WiFi networks", scan_result.networks.len()),
                    data: Some(
                        serde_json::to_value(&scan_result)
                            .context("Failed to serialize WiFi scan result")?,
                    ),
                    timestamp: chrono::Utc::now(),
                }),
                Err(e) => Ok(crate::api::CommandResult {
                    success: false,
                    message: format!("WiFi scan failed: {e}"),
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
                &format!(
                    "Updating AxeOS on {name} from {axeos}",
                    name = device_info.name
                ),
                color,
            );
            client.update_axeos(&axeos).await
        }
        ControlAction::ShowConfig => {
            // Get the full system info which contains all configuration
            let system_info = client.get_system_info().await?;

            // Create a command result with the configuration data
            Ok(crate::api::CommandResult {
                success: true,
                message: format!("Configuration for {}", device_info.name),
                data: Some(
                    serde_json::to_value(&system_info).context("Failed to serialize config")?,
                ),
                timestamp: chrono::Utc::now(),
            })
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

                    // Special handling for configuration display
                    if let Some(data) = &command_result.data {
                        // Check if this is a configuration response
                        if data.get("hostname").is_some() && data.get("pool_url").is_some() {
                            println!("\n📋 Device Configuration:");
                            println!("────────────────────────");

                            // Device Info
                            println!("📱 Device Info:");
                            if let Some(hostname) = data.get("hostname") {
                                println!("  Hostname:    {}", hostname.as_str().unwrap_or("-"));
                            }
                            if let Some(fw) = data.get("firmware_version") {
                                println!("  Firmware:    {}", fw.as_str().unwrap_or("-"));
                            }
                            if let Some(model) = data.get("asic_model") {
                                println!("  ASIC Model:  {}", model.as_str().unwrap_or("-"));
                            }

                            // Network
                            println!("\n🌐 Network:");
                            if let Some(ssid) = data.get("wifi_ssid") {
                                println!("  WiFi SSID:   {}", ssid.as_str().unwrap_or("-"));
                            }
                            if let Some(status) = data.get("wifi_status") {
                                println!("  WiFi Status: {}", status.as_str().unwrap_or("-"));
                            }

                            // Pool Configuration
                            println!("\n⛏️  Mining Pool:");
                            if let Some(url) = data.get("pool_url") {
                                println!("  URL:         {}", url.as_str().unwrap_or("-"));
                            }
                            if let Some(port) = data.get("pool_port") {
                                println!("  Port:        {}", port);
                            }
                            if let Some(user) = data.get("pool_user") {
                                println!("  User:        {}", user.as_str().unwrap_or("-"));
                            }

                            // Hardware Settings
                            println!("\n⚙️  Hardware Settings:");
                            if let Some(freq) = data.get("frequency") {
                                println!("  Frequency:   {} MHz", freq);
                            }
                            if let Some(volt) = data.get("voltage") {
                                println!("  Voltage:     {} mV", volt);
                            }
                            if let Some(fan) = data.get("fanspeed") {
                                println!("  Fan Speed:   {} RPM", fan);
                            }
                            println!("────────────────────────");
                        }
                        // WiFi scan results
                        else if let Some(networks) = data.get("networks") {
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
                print_error(&format!("Control command failed: {e}"), color);
            }
        },
    }

    Ok(())
}
