use crate::api::{
    AxeOsClient, Device, DeviceFilter, DeviceStatus, DeviceType, SystemUpdateRequest,
};
use crate::cache::DeviceCache;
use crate::cli::commands::{BulkAction, OutputFormat};
use crate::output::{print_error, print_info, print_json, print_success, print_warning};
use anyhow::{Context, Result};
use futures::future::join_all;
use std::io::{self, Write};
use std::path::Path;

pub async fn bulk(
    action: BulkAction,
    format: OutputFormat,
    color: bool,
    cache_dir: Option<&Path>,
) -> Result<()> {
    // Require cache directory for bulk operations
    let cache_path = cache_dir
        .context("Cache directory required for bulk operations. Use --cache-dir to specify.")?;

    // Load cache
    let cache = DeviceCache::load(cache_path)?;

    if cache.is_empty() {
        match format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "success": false,
                    "error": "No devices in cache",
                    "message": "Run 'axectl discover' first to find devices",
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                print_warning(
                    "No devices in cache. Run 'axectl discover' first to find devices.",
                    color,
                );
            }
        }
        return Ok(());
    }

    // Filter devices based on action parameters
    let target_devices = match &action {
        BulkAction::Restart {
            device_types,
            ip_addresses,
            all,
            ..
        }
        | BulkAction::SetFanSpeed {
            device_types,
            ip_addresses,
            all,
            ..
        }
        | BulkAction::UpdateSettings {
            device_types,
            ip_addresses,
            all,
            ..
        }
        | BulkAction::WifiScan {
            device_types,
            ip_addresses,
            all,
        }
        | BulkAction::UpdateFirmware {
            device_types,
            ip_addresses,
            all,
            ..
        }
        | BulkAction::UpdateAxeOs {
            device_types,
            ip_addresses,
            all,
            ..
        } => filter_devices(&cache, device_types, ip_addresses, *all)?,
    };

    if target_devices.is_empty() {
        match format {
            OutputFormat::Json => {
                let output = serde_json::json!({
                    "success": false,
                    "error": "No matching devices found",
                    "message": "Check your filters and try again",
                    "timestamp": chrono::Utc::now()
                });
                print_json(&output, true)?;
            }
            OutputFormat::Text => {
                print_warning(
                    "No matching devices found with the specified filters.",
                    color,
                );
            }
        }
        return Ok(());
    }

    // Get confirmation if not forced
    let force = match &action {
        BulkAction::Restart { force, .. }
        | BulkAction::SetFanSpeed { force, .. }
        | BulkAction::UpdateSettings { force, .. }
        | BulkAction::UpdateFirmware { force, .. }
        | BulkAction::UpdateAxeOs { force, .. } => *force,
        BulkAction::WifiScan { .. } => true, // WifiScan doesn't need confirmation
    };

    if !force && format == OutputFormat::Text {
        print_info(
            &format!(
                "About to perform action on {count} device(s):",
                count = target_devices.len()
            ),
            color,
        );
        for device in &target_devices {
            eprintln!(
                "  - {name} ({device_type:?}) at {ip_address}",
                name = device.name,
                device_type = device.device_type,
                ip_address = device.ip_address
            );
        }
        eprintln!();

        eprint!("Continue? [y/N]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            print_info("Operation cancelled.", color);
            return Ok(());
        }
    }

    // Execute the action
    match action {
        BulkAction::Restart { .. } => execute_restart(&target_devices, format, color).await,
        BulkAction::SetFanSpeed { speed, .. } => {
            execute_set_fan_speed(&target_devices, speed, format, color).await
        }
        BulkAction::UpdateSettings { settings, .. } => {
            execute_update_settings(&target_devices, &settings, format, color).await
        }
        BulkAction::WifiScan { .. } => execute_wifi_scan(&target_devices, format, color).await,
        BulkAction::UpdateFirmware {
            firmware, parallel, ..
        } => execute_update_firmware(&target_devices, &firmware, parallel, format, color).await,
        BulkAction::UpdateAxeOs {
            axeos, parallel, ..
        } => execute_update_axeos(&target_devices, &axeos, parallel, format, color).await,
    }
}

/// Filter devices based on criteria
fn filter_devices(
    cache: &DeviceCache,
    device_types: &[DeviceType],
    ip_addresses: &[String],
    all: bool,
) -> Result<Vec<Device>> {
    if all {
        // Return all online devices
        return Ok(cache.get_devices_by_status(DeviceStatus::Online));
    }

    let mut devices = Vec::new();

    // Filter by device types
    for device_type in device_types {
        let filter = DeviceFilter::from(*device_type);
        let type_devices = cache.get_online_devices_by_filter(filter);
        for device in type_devices {
            if !devices
                .iter()
                .any(|d: &Device| d.ip_address == device.ip_address)
            {
                devices.push(device);
            }
        }
    }

    // Filter by IP addresses
    for ip_address in ip_addresses {
        if let Some(cached) = cache.get_device(ip_address) {
            if cached.device.status == DeviceStatus::Online
                && !devices
                    .iter()
                    .any(|d: &Device| d.ip_address == cached.device.ip_address)
            {
                devices.push(cached.device.clone());
            }
        }
    }

    Ok(devices)
}

/// Execute restart on all target devices
async fn execute_restart(devices: &[Device], format: OutputFormat, color: bool) -> Result<()> {
    if format == OutputFormat::Text {
        print_info(
            &format!("Restarting {count} device(s)...", count = devices.len()),
            color,
        );
    }

    let mut results = Vec::new();

    for device in devices {
        let client = AxeOsClient::new(&device.ip_address)?;
        let result = client.restart_system().await;

        let success = result.is_ok();
        let message = result.as_ref().err().map(|e| e.to_string());

        if format == OutputFormat::Text {
            if success {
                print_success(&format!("✓ {name} restarted", name = device.name), color);
            } else {
                print_error(
                    &format!(
                        "✗ {name} failed: {msg}",
                        name = device.name,
                        msg = message.as_ref().unwrap()
                    ),
                    color,
                );
            }
        }

        results.push(serde_json::json!({
            "device": device.name,
            "ip": device.ip_address,
            "success": success,
            "error": message
        }));
    }

    if format == OutputFormat::Json {
        let output = serde_json::json!({
            "action": "restart",
            "total_devices": devices.len(),
            "results": results,
            "timestamp": chrono::Utc::now()
        });
        print_json(&output, true)?;
    }

    Ok(())
}

/// Execute set fan speed on all target devices
async fn execute_set_fan_speed(
    devices: &[Device],
    speed: u8,
    format: OutputFormat,
    color: bool,
) -> Result<()> {
    if format == OutputFormat::Text {
        print_info(
            &format!(
                "Setting fan speed to {speed}% on {count} device(s)...",
                count = devices.len()
            ),
            color,
        );
    }

    let mut results = Vec::new();

    for device in devices {
        let client = AxeOsClient::new(&device.ip_address)?;
        let result = client.set_fan_speed(speed).await;

        let success = result.is_ok();
        let message = result.as_ref().err().map(|e| e.to_string());

        if format == OutputFormat::Text {
            if success {
                print_success(
                    &format!("✓ {name} fan speed set to {speed}%", name = device.name),
                    color,
                );
            } else {
                print_error(
                    &format!(
                        "✗ {name} failed: {msg}",
                        name = device.name,
                        msg = message.as_ref().unwrap()
                    ),
                    color,
                );
            }
        }

        results.push(serde_json::json!({
            "device": device.name,
            "ip": device.ip_address,
            "success": success,
            "error": message
        }));
    }

    if format == OutputFormat::Json {
        let output = serde_json::json!({
            "action": "set_fan_speed",
            "speed": speed,
            "total_devices": devices.len(),
            "results": results,
            "timestamp": chrono::Utc::now()
        });
        print_json(&output, true)?;
    }

    Ok(())
}

/// Execute update settings on all target devices
async fn execute_update_settings(
    devices: &[Device],
    settings: &str,
    format: OutputFormat,
    color: bool,
) -> Result<()> {
    // Parse settings JSON
    let settings_json: serde_json::Value =
        serde_json::from_str(settings).context("Failed to parse settings JSON")?;

    if format == OutputFormat::Text {
        print_info(
            &format!(
                "Updating settings on {count} device(s)...",
                count = devices.len()
            ),
            color,
        );
    }

    let mut results = Vec::new();

    for device in devices {
        let client = AxeOsClient::new(&device.ip_address)?;
        // Parse settings into SystemUpdateRequest
        let update_request: SystemUpdateRequest = serde_json::from_value(settings_json.clone())
            .context("Failed to parse settings into SystemUpdateRequest")?;
        let result = client.update_system(update_request).await;

        let success = result.is_ok();
        let message = result.as_ref().err().map(|e| e.to_string());

        if format == OutputFormat::Text {
            if success {
                print_success(
                    &format!("✓ {name} settings updated", name = device.name),
                    color,
                );
            } else {
                print_error(
                    &format!(
                        "✗ {name} failed: {msg}",
                        name = device.name,
                        msg = message.as_ref().unwrap()
                    ),
                    color,
                );
            }
        }

        results.push(serde_json::json!({
            "device": device.name,
            "ip": device.ip_address,
            "success": success,
            "error": message
        }));
    }

    if format == OutputFormat::Json {
        let output = serde_json::json!({
            "action": "update_settings",
            "settings": settings_json,
            "total_devices": devices.len(),
            "results": results,
            "timestamp": chrono::Utc::now()
        });
        print_json(&output, true)?;
    }

    Ok(())
}

/// Execute WiFi scan on all target devices
async fn execute_wifi_scan(devices: &[Device], format: OutputFormat, color: bool) -> Result<()> {
    if format == OutputFormat::Text {
        print_info(
            &format!(
                "Scanning WiFi on {count} device(s)...",
                count = devices.len()
            ),
            color,
        );
    }

    let mut results = Vec::new();

    for device in devices {
        let client = AxeOsClient::new(&device.ip_address)?;
        let result = client.scan_wifi().await;

        match result {
            Ok(scan_response) => {
                if format == OutputFormat::Text {
                    print_success(
                        &format!(
                            "✓ {name} found {count} networks",
                            name = device.name,
                            count = scan_response.networks.len()
                        ),
                        color,
                    );
                    for network in &scan_response.networks {
                        eprintln!(
                            "    - {ssid} ({rssi}dBm)",
                            ssid = if network.ssid.is_empty() {
                                "<hidden>"
                            } else {
                                &network.ssid
                            },
                            rssi = network.rssi
                        );
                    }
                }

                results.push(serde_json::json!({
                    "device": device.name,
                    "ip": device.ip_address,
                    "success": true,
                    "networks": scan_response.networks
                }));
            }
            Err(e) => {
                if format == OutputFormat::Text {
                    print_error(&format!("✗ {name} failed: {e}", name = device.name), color);
                }

                results.push(serde_json::json!({
                    "device": device.name,
                    "ip": device.ip_address,
                    "success": false,
                    "error": e.to_string()
                }));
            }
        }
    }

    if format == OutputFormat::Json {
        let output = serde_json::json!({
            "action": "wifi_scan",
            "total_devices": devices.len(),
            "results": results,
            "timestamp": chrono::Utc::now()
        });
        print_json(&output, true)?;
    }

    Ok(())
}

/// Execute firmware update on all target devices
async fn execute_update_firmware(
    devices: &[Device],
    firmware: &str,
    parallel: usize,
    format: OutputFormat,
    color: bool,
) -> Result<()> {
    if format == OutputFormat::Text {
        print_info(
            &format!(
                "Updating firmware on {count} device(s) (max {parallel} parallel)...",
                count = devices.len()
            ),
            color,
        );
    }

    let mut results = Vec::new();

    // Process in batches for parallel updates
    for batch in devices.chunks(parallel) {
        let mut batch_futures = Vec::new();

        for device in batch {
            let client = AxeOsClient::new(&device.ip_address)?;
            let fw = firmware.to_string();
            let name = device.name.clone();
            let ip = device.ip_address.clone();

            batch_futures.push(async move {
                let result = client.update_firmware(&fw).await;
                (name, ip, result)
            });
        }

        let batch_results = join_all(batch_futures).await;

        for (name, ip, result) in batch_results {
            let success = result.is_ok();
            let message = result.as_ref().err().map(|e| e.to_string());

            if format == OutputFormat::Text {
                if success {
                    print_success(&format!("✓ {name} firmware update started"), color);
                } else {
                    print_error(
                        &format!("✗ {name} failed: {msg}", msg = message.as_ref().unwrap()),
                        color,
                    );
                }
            }

            results.push(serde_json::json!({
                "device": name,
                "ip": ip,
                "success": success,
                "error": message
            }));
        }
    }

    if format == OutputFormat::Json {
        let output = serde_json::json!({
            "action": "update_firmware",
            "firmware": firmware,
            "total_devices": devices.len(),
            "parallel": parallel,
            "results": results,
            "timestamp": chrono::Utc::now()
        });
        print_json(&output, true)?;
    }

    Ok(())
}

/// Execute AxeOS update on all target devices
async fn execute_update_axeos(
    devices: &[Device],
    axeos: &str,
    parallel: usize,
    format: OutputFormat,
    color: bool,
) -> Result<()> {
    if format == OutputFormat::Text {
        print_info(
            &format!(
                "Updating AxeOS on {count} device(s) (max {parallel} parallel)...",
                count = devices.len()
            ),
            color,
        );
    }

    let mut results = Vec::new();

    // Process in batches for parallel updates
    for batch in devices.chunks(parallel) {
        let mut batch_futures = Vec::new();

        for device in batch {
            let client = AxeOsClient::new(&device.ip_address)?;
            let axe = axeos.to_string();
            let name = device.name.clone();
            let ip = device.ip_address.clone();

            batch_futures.push(async move {
                let result = client.update_axeos(&axe).await;
                (name, ip, result)
            });
        }

        let batch_results = join_all(batch_futures).await;

        for (name, ip, result) in batch_results {
            let success = result.is_ok();
            let message = result.as_ref().err().map(|e| e.to_string());

            if format == OutputFormat::Text {
                if success {
                    print_success(&format!("✓ {name} AxeOS update started"), color);
                } else {
                    print_error(
                        &format!("✗ {name} failed: {msg}", msg = message.as_ref().unwrap()),
                        color,
                    );
                }
            }

            results.push(serde_json::json!({
                "device": name,
                "ip": ip,
                "success": success,
                "error": message
            }));
        }
    }

    if format == OutputFormat::Json {
        let output = serde_json::json!({
            "action": "update_axeos",
            "axeos": axeos,
            "total_devices": devices.len(),
            "parallel": parallel,
            "results": results,
            "timestamp": chrono::Utc::now()
        });
        print_json(&output, true)?;
    }

    Ok(())
}
