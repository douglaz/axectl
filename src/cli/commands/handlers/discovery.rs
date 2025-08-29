use crate::cli::commands::OutputFormat;
use anyhow::Result;
use std::time::Duration;
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
}

/// Perform network discovery and return discovered devices
pub async fn perform_discovery(
    network: Option<String>,
    timeout: u64,
    mdns_enabled: bool,
    cache_dir: Option<&std::path::Path>,
    color: bool,
) -> Result<Vec<crate::api::DeviceInfo>> {
    use crate::cache::get_cache_dir;
    use crate::discovery::{mdns, network as net_utils, scanner};
    use crate::output::print_info;

    let discovery_timeout = Duration::from_secs(timeout);
    let mut all_devices = Vec::new();

    // Get cache directory, using default if not provided
    let cache_path = get_cache_dir(cache_dir)?;
    let cache_path_ref = cache_path.as_ref();

    // Load cache
    let mut cache = match crate::cache::DeviceCache::load(cache_path_ref) {
        Ok(cache) => {
            if !cache.is_empty() {
                print_info(
                    &format!(
                        "Loaded cache with {} devices ({}s old)",
                        cache.device_count(),
                        cache.age_seconds()
                    ),
                    color,
                );
            }
            Some(cache)
        }
        Err(e) => {
            // It's normal for cache not to exist on first run
            tracing::debug!("Cache not loaded: {}", e);
            Some(crate::cache::DeviceCache::new())
        }
    };

    // Determine network to scan
    let target_network = if let Some(net_str) = network {
        net_utils::parse_network(&net_str)?
    } else {
        print_info("Auto-detecting local network...", color);
        net_utils::auto_detect_network()?
    };

    let network_info = net_utils::get_network_info(&target_network);
    print_info(
        &format!(
            "Scanning network: {} ({} hosts)",
            network_info.network_str, network_info.host_count
        ),
        color,
    );

    // Run mDNS discovery if enabled
    if mdns_enabled {
        print_info("Running mDNS discovery...", color);
        match mdns::discover_axeos_devices(discovery_timeout).await {
            Ok(mdns_devices) => {
                print_info(
                    &format!("Found {} devices via mDNS", mdns_devices.len()),
                    color,
                );
                all_devices.extend(mdns_devices);
            }
            Err(e) => {
                tracing::warn!("mDNS discovery failed: {}", e);
            }
        }
    }

    // Quick probe cached devices if available
    if let Some(ref cache) = cache {
        let known_ips = cache.get_known_ips();
        if !known_ips.is_empty() {
            print_info(
                &format!("Quick probe of {} cached devices...", known_ips.len()),
                color,
            );

            // Probe known IPs with shorter timeout for speed
            for ip in &known_ips {
                if let Ok(Some(device)) =
                    scanner::probe_single_device(ip, Duration::from_millis(500)).await
                    && !all_devices
                        .iter()
                        .any(|d| d.ip_address == device.ip_address)
                {
                    all_devices.push(device);
                }
            }

            print_info(
                &format!(
                    "Found {} devices from cache probe",
                    all_devices
                        .iter()
                        .filter(|d| known_ips.contains(&d.ip_address))
                        .count()
                ),
                color,
            );
        }
    }

    // Run IP scan
    print_info("Running IP network scan...", color);
    let scan_config = scanner::ScanConfig {
        timeout_per_host: Duration::from_millis(2000),
        parallel_scans: 20,
        axeos_only: true,
        include_unreachable: false,
    };

    match scanner::scan_network(target_network, scan_config).await {
        Ok(scan_result) => {
            print_info(
                &format!(
                    "Scanned {} addresses in {:.1}s, found {} devices",
                    scan_result.scan_info.addresses_scanned,
                    scan_result.scan_info.scan_duration_seconds,
                    scan_result.devices_found.len()
                ),
                color,
            );

            // Merge with mDNS results, avoiding duplicates
            for device in scan_result.devices_found {
                if !all_devices
                    .iter()
                    .any(|d| d.ip_address == device.ip_address)
                {
                    all_devices.push(device);
                }
            }
        }
        Err(e) => {
            tracing::warn!("IP scan failed: {}", e);
        }
    }

    // Note: Devices will be saved to cache below, no need for separate storage

    // Update cache with discovered devices
    if let Some(ref mut cache) = cache.as_mut() {
        for device in &all_devices {
            cache.update_device(device.clone());
        }

        // Prune old devices (older than 7 days)
        cache.prune_old(chrono::Duration::days(7));

        if let Err(e) = cache.save(cache_path_ref) {
            tracing::warn!("Failed to save cache: {}", e);
        } else if !all_devices.is_empty() {
            tracing::debug!("Updated cache with {} devices", all_devices.len());
        }
    }

    Ok(all_devices)
}

pub async fn discover(
    network: Option<String>,
    timeout: u64,
    mdns_enabled: bool,
    format: OutputFormat,
    color: bool,
    cache_dir: Option<&std::path::Path>,
) -> Result<()> {
    use crate::output::{format_table, print_info, print_json, print_success};

    // Perform discovery using the shared function
    let all_devices =
        perform_discovery(network.clone(), timeout, mdns_enabled, cache_dir, color).await?;

    // Get network info for output
    let network_info = if let Some(net_str) = network {
        crate::discovery::network::get_network_info(&crate::discovery::network::parse_network(
            &net_str,
        )?)
    } else {
        crate::discovery::network::get_network_info(
            &crate::discovery::network::auto_detect_network()?,
        )
    };

    // Output results
    match format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "devices": all_devices,
                "total": all_devices.len(),
                "network_scanned": network_info.network_str,
                "discovery_methods": {
                    "mdns": mdns_enabled,
                    "ip_scan": true
                },
                "timestamp": chrono::Utc::now()
            });
            print_json(&output, true)?;
        }
        OutputFormat::Text => {
            if all_devices.is_empty() {
                print_info("No devices found", color);
            } else {
                let table_rows: Vec<DeviceTableRow> = all_devices
                    .iter()
                    .map(|device| DeviceTableRow {
                        name: device.name.clone(),
                        ip_address: device.ip_address.clone(),
                        device_type: device.device_type.as_str().to_string(),
                        status: format!("{:?}", device.status),
                    })
                    .collect();

                println!("{}", format_table(table_rows, color));
                print_success(&format!("Found {} device(s)", all_devices.len()), color);

                // Show cache location if using default
                if cache_dir.is_none() && !all_devices.is_empty() {
                    eprintln!();
                    let default_cache = crate::cache::get_default_cache_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| "~/.cache/axectl/devices".to_string());
                    print_info(&format!("💾 Devices cached in: {}", default_cache), color);
                }
            }
        }
    }

    Ok(())
}
