use crate::cli::commands::{BulkAction, OutputFormat};
use anyhow::Result;
use std::path::Path;
use std::time::Duration;
use tabled::Tabled;

#[derive(Tabled)]
struct BulkResultTableRow {
    #[tabled(rename = "Device")]
    device_name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Message")]
    message: String,
}

pub async fn bulk(
    action: BulkAction,
    format: OutputFormat,
    color: bool,
    _cache_dir: Option<&Path>,
) -> Result<()> {
    use crate::api::{AxeOsClient, DeviceStatus, SystemUpdateRequest};
    use crate::output::{
        format_table, print_error, print_info, print_json, print_success, print_warning,
    };
    use crate::storage::GLOBAL_STORAGE;
    use futures::future::join_all;

    match action {
        BulkAction::Restart { group, force } => {
            // Find group
            let group_info = match GLOBAL_STORAGE.find_group(&group) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": format!("Group not found: {}", group),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Group not found: {}", group), color);
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to find group: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            // Get online devices in group
            let devices = match GLOBAL_STORAGE
                .get_group_devices_by_status(&group_info.id, DeviceStatus::Online)
            {
                Ok(devices) => devices,
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to get group devices: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            if devices.is_empty() {
                match format {
                    OutputFormat::Json => {
                        let output = serde_json::json!({
                            "success": false,
                            "error": "No online devices found in group",
                            "timestamp": chrono::Utc::now()
                        });
                        print_json(&output, true)?;
                    }
                    OutputFormat::Text => {
                        print_warning("No online devices found in group", color);
                    }
                }
                return Ok(());
            }

            // Confirmation prompt
            if !force {
                print_info(
                    &format!(
                        "Are you sure you want to restart {} devices in group '{}'? [y/N]: ",
                        devices.len(),
                        group_info.name
                    ),
                    color,
                );
                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_ok() {
                    let input = input.trim().to_lowercase();
                    if input != "y" && input != "yes" {
                        print_info("Bulk restart cancelled", color);
                        return Ok(());
                    }
                } else {
                    print_error(
                        "Failed to read input, use --force to skip confirmation",
                        color,
                    );
                    return Ok(());
                }
            }

            print_info(
                &format!(
                    "Restarting {} devices in group '{}'...",
                    devices.len(),
                    group_info.name
                ),
                color,
            );

            // Execute restart operations in parallel
            let restart_tasks: Vec<_> = devices
                .iter()
                .map(|device| {
                    let device_clone = device.clone();
                    async move {
                        let client = AxeOsClient::with_timeout(
                            &device_clone.ip_address,
                            Duration::from_secs(10),
                        );
                        match client {
                            Ok(client) => match client.restart_system().await {
                                Ok(result) => {
                                    (device_clone.name.clone(), result.success, result.message)
                                }
                                Err(e) => (device_clone.name.clone(), false, e.to_string()),
                            },
                            Err(e) => (
                                device_clone.name.clone(),
                                false,
                                format!("Failed to create client: {}", e),
                            ),
                        }
                    }
                })
                .collect();

            let results = join_all(restart_tasks).await;

            // Process results
            let mut successful = 0;
            let mut failed = 0;
            let mut table_rows = Vec::new();

            for (device_name, success, message) in results {
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }

                table_rows.push(BulkResultTableRow {
                    device_name,
                    status: if success {
                        "✓ Success".to_string()
                    } else {
                        "✗ Failed".to_string()
                    },
                    message,
                });
            }

            match format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "operation": "restart",
                        "group": group_info.name,
                        "total_devices": devices.len(),
                        "successful": successful,
                        "failed": failed,
                        "results": table_rows.iter().map(|row| serde_json::json!({
                            "device": row.device_name,
                            "success": row.status.contains("Success"),
                            "message": row.message
                        })).collect::<Vec<_>>(),
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    println!("{}", format_table(table_rows, color));
                    println!();
                    if failed == 0 {
                        print_success(
                            &format!("Successfully restarted all {} devices", successful),
                            color,
                        );
                    } else {
                        print_warning(
                            &format!(
                                "Completed with {} successful, {} failed",
                                successful, failed
                            ),
                            color,
                        );
                    }
                }
            }
        }

        BulkAction::SetFanSpeed {
            group,
            speed,
            force,
        } => {
            // Find group
            let group_info = match GLOBAL_STORAGE.find_group(&group) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": format!("Group not found: {}", group),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Group not found: {}", group), color);
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to find group: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            // Get online devices in group
            let devices = match GLOBAL_STORAGE
                .get_group_devices_by_status(&group_info.id, DeviceStatus::Online)
            {
                Ok(devices) => devices,
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to get group devices: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            if devices.is_empty() {
                match format {
                    OutputFormat::Json => {
                        let output = serde_json::json!({
                            "success": false,
                            "error": "No online devices found in group",
                            "timestamp": chrono::Utc::now()
                        });
                        print_json(&output, true)?;
                    }
                    OutputFormat::Text => {
                        print_warning("No online devices found in group", color);
                    }
                }
                return Ok(());
            }

            // Confirmation prompt
            if !force {
                print_info(&format!("Are you sure you want to set fan speed to {}% on {} devices in group '{}'? [y/N]: ", 
                    speed, devices.len(), group_info.name), color);
                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_ok() {
                    let input = input.trim().to_lowercase();
                    if input != "y" && input != "yes" {
                        print_info("Bulk fan speed change cancelled", color);
                        return Ok(());
                    }
                } else {
                    print_error(
                        "Failed to read input, use --force to skip confirmation",
                        color,
                    );
                    return Ok(());
                }
            }

            print_info(
                &format!(
                    "Setting fan speed to {}% on {} devices in group '{}'...",
                    speed,
                    devices.len(),
                    group_info.name
                ),
                color,
            );

            // Execute fan speed operations in parallel
            let fanspeed_tasks: Vec<_> = devices
                .iter()
                .map(|device| {
                    let device_clone = device.clone();
                    let speed_copy = speed;
                    async move {
                        let client = AxeOsClient::with_timeout(
                            &device_clone.ip_address,
                            Duration::from_secs(10),
                        );
                        match client {
                            Ok(client) => match client.set_fan_speed(speed_copy).await {
                                Ok(result) => {
                                    (device_clone.name.clone(), result.success, result.message)
                                }
                                Err(e) => (device_clone.name.clone(), false, e.to_string()),
                            },
                            Err(e) => (
                                device_clone.name.clone(),
                                false,
                                format!("Failed to create client: {}", e),
                            ),
                        }
                    }
                })
                .collect();

            let results = join_all(fanspeed_tasks).await;

            // Process results (same pattern as restart)
            let mut successful = 0;
            let mut failed = 0;
            let mut table_rows = Vec::new();

            for (device_name, success, message) in results {
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }

                table_rows.push(BulkResultTableRow {
                    device_name,
                    status: if success {
                        "✓ Success".to_string()
                    } else {
                        "✗ Failed".to_string()
                    },
                    message,
                });
            }

            match format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "operation": "set_fan_speed",
                        "group": group_info.name,
                        "fan_speed": speed,
                        "total_devices": devices.len(),
                        "successful": successful,
                        "failed": failed,
                        "results": table_rows.iter().map(|row| serde_json::json!({
                            "device": row.device_name,
                            "success": row.status.contains("Success"),
                            "message": row.message
                        })).collect::<Vec<_>>(),
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    println!("{}", format_table(table_rows, color));
                    println!();
                    if failed == 0 {
                        print_success(
                            &format!("Successfully set fan speed on all {} devices", successful),
                            color,
                        );
                    } else {
                        print_warning(
                            &format!(
                                "Completed with {} successful, {} failed",
                                successful, failed
                            ),
                            color,
                        );
                    }
                }
            }
        }

        BulkAction::UpdateSettings {
            group,
            settings,
            force,
        } => {
            // Parse settings first to validate JSON
            let update_request = match serde_json::from_str::<SystemUpdateRequest>(&settings) {
                Ok(req) => req,
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": format!("Invalid settings JSON: {}", e),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Invalid settings JSON: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            // Find group and get devices (same pattern as above)
            let group_info = match GLOBAL_STORAGE.find_group(&group) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": format!("Group not found: {}", group),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Group not found: {}", group), color);
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to find group: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            let devices = match GLOBAL_STORAGE
                .get_group_devices_by_status(&group_info.id, DeviceStatus::Online)
            {
                Ok(devices) => devices,
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to get group devices: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            if devices.is_empty() {
                match format {
                    OutputFormat::Json => {
                        let output = serde_json::json!({
                            "success": false,
                            "error": "No online devices found in group",
                            "timestamp": chrono::Utc::now()
                        });
                        print_json(&output, true)?;
                    }
                    OutputFormat::Text => {
                        print_warning("No online devices found in group", color);
                    }
                }
                return Ok(());
            }

            // Confirmation prompt
            if !force {
                print_info(&format!("Are you sure you want to update settings on {} devices in group '{}'? [y/N]: ", 
                    devices.len(), group_info.name), color);
                let mut input = String::new();
                if std::io::stdin().read_line(&mut input).is_ok() {
                    let input = input.trim().to_lowercase();
                    if input != "y" && input != "yes" {
                        print_info("Bulk settings update cancelled", color);
                        return Ok(());
                    }
                } else {
                    print_error(
                        "Failed to read input, use --force to skip confirmation",
                        color,
                    );
                    return Ok(());
                }
            }

            print_info(
                &format!(
                    "Updating settings on {} devices in group '{}'...",
                    devices.len(),
                    group_info.name
                ),
                color,
            );

            // Execute update operations in parallel
            let update_tasks: Vec<_> = devices
                .iter()
                .map(|device| {
                    let device_clone = device.clone();
                    let update_req_clone = update_request.clone();
                    async move {
                        let client = AxeOsClient::with_timeout(
                            &device_clone.ip_address,
                            Duration::from_secs(10),
                        );
                        match client {
                            Ok(client) => match client.update_system(update_req_clone).await {
                                Ok(result) => {
                                    (device_clone.name.clone(), result.success, result.message)
                                }
                                Err(e) => (device_clone.name.clone(), false, e.to_string()),
                            },
                            Err(e) => (
                                device_clone.name.clone(),
                                false,
                                format!("Failed to create client: {}", e),
                            ),
                        }
                    }
                })
                .collect();

            let results = join_all(update_tasks).await;

            // Process results (same pattern)
            let mut successful = 0;
            let mut failed = 0;
            let mut table_rows = Vec::new();

            for (device_name, success, message) in results {
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }

                table_rows.push(BulkResultTableRow {
                    device_name,
                    status: if success {
                        "✓ Success".to_string()
                    } else {
                        "✗ Failed".to_string()
                    },
                    message,
                });
            }

            match format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "operation": "update_settings",
                        "group": group_info.name,
                        "total_devices": devices.len(),
                        "successful": successful,
                        "failed": failed,
                        "results": table_rows.iter().map(|row| serde_json::json!({
                            "device": row.device_name,
                            "success": row.status.contains("Success"),
                            "message": row.message
                        })).collect::<Vec<_>>(),
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    println!("{}", format_table(table_rows, color));
                    println!();
                    if failed == 0 {
                        print_success(
                            &format!(
                                "Successfully updated settings on all {} devices",
                                successful
                            ),
                            color,
                        );
                    } else {
                        print_warning(
                            &format!(
                                "Completed with {} successful, {} failed",
                                successful, failed
                            ),
                            color,
                        );
                    }
                }
            }
        }

        BulkAction::WifiScan { group } => {
            // Find group and get devices
            let group_info = match GLOBAL_STORAGE.find_group(&group) {
                Ok(Some(g)) => g,
                Ok(None) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": format!("Group not found: {}", group),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Group not found: {}", group), color);
                        }
                    }
                    return Ok(());
                }
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to find group: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            let devices = match GLOBAL_STORAGE
                .get_group_devices_by_status(&group_info.id, DeviceStatus::Online)
            {
                Ok(devices) => devices,
                Err(e) => {
                    match format {
                        OutputFormat::Json => {
                            let output = serde_json::json!({
                                "success": false,
                                "error": e.to_string(),
                                "timestamp": chrono::Utc::now()
                            });
                            print_json(&output, true)?;
                        }
                        OutputFormat::Text => {
                            print_error(&format!("Failed to get group devices: {}", e), color);
                        }
                    }
                    return Ok(());
                }
            };

            if devices.is_empty() {
                match format {
                    OutputFormat::Json => {
                        let output = serde_json::json!({
                            "success": false,
                            "error": "No online devices found in group",
                            "timestamp": chrono::Utc::now()
                        });
                        print_json(&output, true)?;
                    }
                    OutputFormat::Text => {
                        print_warning("No online devices found in group", color);
                    }
                }
                return Ok(());
            }

            print_info(
                &format!(
                    "Scanning WiFi on {} devices in group '{}'...",
                    devices.len(),
                    group_info.name
                ),
                color,
            );

            // Execute WiFi scan operations in parallel
            let scan_tasks: Vec<_> = devices
                .iter()
                .map(|device| {
                    let device_clone = device.clone();
                    async move {
                        let client = AxeOsClient::with_timeout(
                            &device_clone.ip_address,
                            Duration::from_secs(10),
                        );
                        match client {
                            Ok(client) => match client.scan_wifi().await {
                                Ok(scan_result) => (
                                    device_clone.name.clone(),
                                    true,
                                    format!("Found {} networks", scan_result.networks.len()),
                                    Some(scan_result),
                                ),
                                Err(e) => (device_clone.name.clone(), false, e.to_string(), None),
                            },
                            Err(e) => (
                                device_clone.name.clone(),
                                false,
                                format!("Failed to create client: {}", e),
                                None,
                            ),
                        }
                    }
                })
                .collect();

            let results = join_all(scan_tasks).await;

            // Process WiFi scan results
            let mut successful = 0;
            let mut failed = 0;
            let mut all_scan_results = Vec::new();

            for (device_name, success, message, scan_data) in results {
                if success {
                    successful += 1;
                } else {
                    failed += 1;
                }

                all_scan_results.push(serde_json::json!({
                    "device": device_name,
                    "success": success,
                    "message": message,
                    "wifi_networks": scan_data
                }));
            }

            match format {
                OutputFormat::Json => {
                    let output = serde_json::json!({
                        "operation": "wifi_scan",
                        "group": group_info.name,
                        "total_devices": devices.len(),
                        "successful": successful,
                        "failed": failed,
                        "results": all_scan_results,
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    for result in &all_scan_results {
                        let device = result["device"].as_str().unwrap_or("Unknown");
                        let success = result["success"].as_bool().unwrap_or(false);
                        let message = result["message"].as_str().unwrap_or("");

                        if success {
                            println!("✓ {}: {}", device, message);
                            if let Some(networks) = result["wifi_networks"]["networks"].as_array() {
                                for network in networks {
                                    if let (Some(ssid), Some(rssi)) =
                                        (network["ssid"].as_str(), network["rssi"].as_i64())
                                    {
                                        println!("    {} ({}dBm)", ssid, rssi);
                                    }
                                }
                                println!();
                            }
                        } else {
                            println!("✗ {}: {}", device, message);
                        }
                    }

                    if failed == 0 {
                        print_success(
                            &format!("Successfully scanned WiFi on all {} devices", successful),
                            color,
                        );
                    } else {
                        print_warning(
                            &format!(
                                "Completed with {} successful, {} failed",
                                successful, failed
                            ),
                            color,
                        );
                    }
                }
            }
        }

        BulkAction::UpdateFirmware {
            group: _,
            firmware: _,
            force: _,
            parallel,
        } => {
            // Similar to other operations but with parallel control
            print_info(
                &format!(
                    "Firmware update operations would run with {} parallel connections",
                    parallel
                ),
                color,
            );
            print_info("Firmware update implementation would go here", color);
            // Implementation would be similar to other bulk operations but with semaphore for parallel control
        }

        BulkAction::UpdateAxeOs {
            group: _,
            axeos: _,
            force: _,
            parallel,
        } => {
            // Similar to firmware update
            print_info(
                &format!(
                    "AxeOS update operations would run with {} parallel connections",
                    parallel
                ),
                color,
            );
            print_info("AxeOS update implementation would go here", color);
            // Implementation would be similar to other bulk operations but with semaphore for parallel control
        }
    }

    Ok(())
}
