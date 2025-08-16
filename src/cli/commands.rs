use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "axectl")]
#[command(about = "CLI tool for managing Bitaxe and NerdQAxe miners")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output format (text or json)
    #[arg(long, value_enum, default_value = "text", global = true)]
    pub format: OutputFormat,

    /// Disable colored output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Enable debug logging
    #[arg(long, short, global = true)]
    pub verbose: bool,

    /// Optional cache directory for faster discovery
    #[arg(long, global = true)]
    pub cache_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Discover miners on the network
    Discover {
        /// Specific network range to scan (auto-detected if not specified)
        #[arg(long)]
        network: Option<String>,

        /// Discovery timeout in seconds
        #[arg(long, default_value = "5")]
        timeout: u64,

        /// Disable mDNS discovery
        #[arg(long)]
        no_mdns: bool,
    },

    /// List known devices
    List {
        /// Include offline devices
        #[arg(long)]
        all: bool,
    },

    /// Show device statistics
    Stats {
        /// Specific device name or IP (all devices if not specified)
        device: Option<String>,

        /// Enable continuous monitoring
        #[arg(long, short)]
        watch: bool,

        /// Update interval in seconds (with --watch)
        #[arg(long, default_value = "30")]
        interval: u64,
    },

    /// Control a device
    Control {
        /// Device name or IP
        device: String,

        /// Action to perform
        #[command(subcommand)]
        action: ControlAction,
    },

    /// Monitor devices continuously
    Monitor {
        /// Update interval in seconds
        #[arg(long, default_value = "30")]
        interval: u64,

        /// Alert on high temperature (celsius)
        #[arg(long)]
        temp_alert: Option<f64>,

        /// Alert on low hashrate (percentage drop)
        #[arg(long)]
        hashrate_alert: Option<f64>,

        /// Save data to SQLite database
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum ControlAction {
    /// Set fan speed percentage (0-100)
    SetFanSpeed { speed: u8 },

    /// Restart the device
    Restart,

    /// Update system settings
    UpdateSettings {
        /// JSON string with settings to update
        settings: String,
    },

    /// Scan for WiFi networks
    WifiScan,

    /// Update firmware via OTA
    UpdateFirmware {
        /// Firmware URL or file path
        firmware: String,
    },

    /// Update AxeOS web interface
    UpdateAxeOs {
        /// AxeOS update URL or file path
        axeos: String,
    },
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        // Initialize logging
        if self.verbose {
            tracing_subscriber::fmt::init();
        }

        match self.command {
            Commands::Discover { network, timeout, no_mdns } => {
                handlers::discover(network, timeout, !no_mdns, self.format, !self.no_color, self.cache_dir.as_deref()).await
            }
            Commands::List { all } => {
                handlers::list(all, self.format, !self.no_color).await
            }
            Commands::Stats { device, watch, interval } => {
                handlers::stats(device, watch, interval, self.format, !self.no_color).await
            }
            Commands::Control { device, action } => {
                handlers::control(device, action, self.format, !self.no_color).await
            }
            Commands::Monitor { interval, temp_alert, hashrate_alert, db } => {
                handlers::monitor(interval, temp_alert, hashrate_alert, db, self.format, !self.no_color).await
            }
        }
    }
}

pub mod handlers {
    use super::*;

    pub async fn discover(
        network: Option<String>,
        timeout: u64,
        mdns_enabled: bool,
        format: OutputFormat,
        color: bool,
        cache_dir: Option<&std::path::Path>,
    ) -> Result<()> {
        use crate::discovery::{scanner, mdns, network as net_utils};
        use crate::output::{print_json, format_table, print_info, print_success};
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

        let discovery_timeout = Duration::from_secs(timeout);
        let mut all_devices = Vec::new();

        // Load cache if cache directory is provided
        let mut cache = if let Some(cache_path) = cache_dir {
            match crate::cache::DeviceCache::load(cache_path) {
                Ok(cache) => {
                    if !cache.is_empty() {
                        print_info(&format!("Loaded cache with {} devices ({}s old)", 
                            cache.device_count(), cache.age_seconds()), color);
                    }
                    Some(cache)
                }
                Err(e) => {
                    tracing::warn!("Failed to load cache: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Determine network to scan
        let target_network = if let Some(net_str) = network {
            net_utils::parse_network(&net_str)?
        } else {
            print_info("Auto-detecting local network...", color);
            net_utils::auto_detect_network()?
        };

        let network_info = net_utils::get_network_info(&target_network);
        print_info(&format!("Scanning network: {} ({} hosts)", 
            network_info.network_str, network_info.host_count), color);

        // Run mDNS discovery if enabled
        if mdns_enabled {
            print_info("Running mDNS discovery...", color);
            match mdns::discover_axeos_devices(discovery_timeout).await {
                Ok(mdns_devices) => {
                    print_info(&format!("Found {} devices via mDNS", mdns_devices.len()), color);
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
                print_info(&format!("Quick probe of {} cached devices...", known_ips.len()), color);
                
                // Probe known IPs with shorter timeout for speed
                for ip in &known_ips {
                    if let Ok(Some(device)) = scanner::probe_single_device(ip, Duration::from_millis(500)).await {
                        if !all_devices.iter().any(|d| d.ip_address == device.ip_address) {
                            all_devices.push(device);
                        }
                    }
                }

                print_info(&format!("Found {} devices from cache probe", 
                    all_devices.iter().filter(|d| known_ips.contains(&d.ip_address)).count()), color);
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
                print_info(&format!("Scanned {} addresses in {:.1}s, found {} devices",
                    scan_result.scan_info.addresses_scanned,
                    scan_result.scan_info.scan_duration_seconds,
                    scan_result.devices_found.len()), color);
                
                // Merge with mDNS results, avoiding duplicates
                for device in scan_result.devices_found {
                    if !all_devices.iter().any(|d| d.ip_address == device.ip_address) {
                        all_devices.push(device);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("IP scan failed: {}", e);
            }
        }

        // Save discovered devices to storage
        for device in &all_devices {
            if let Err(e) = crate::storage::GLOBAL_STORAGE.upsert_device(device.clone()) {
                tracing::warn!("Failed to save device to storage: {}", e);
            }
        }

        // Update cache with discovered devices
        if let (Some(ref mut cache), Some(cache_path)) = (cache.as_mut(), cache_dir) {
            for device in &all_devices {
                cache.update_device(device);
            }
            
            // Prune old devices (older than 7 days)
            cache.prune_old(chrono::Duration::days(7));

            if let Err(e) = cache.save(cache_path) {
                tracing::warn!("Failed to save cache: {}", e);
            } else if !all_devices.is_empty() {
                tracing::debug!("Updated cache with {} devices", all_devices.len());
            }
        }

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
                    let table_rows: Vec<DeviceTableRow> = all_devices.iter().map(|device| {
                        DeviceTableRow {
                            name: device.name.clone(),
                            ip_address: device.ip_address.clone(),
                            device_type: device.device_type.as_str().to_string(),
                            status: format!("{:?}", device.status),
                        }
                    }).collect();

                    println!("{}", format_table(table_rows, color));
                    print_success(&format!("Found {} device(s)", all_devices.len()), color);
                }
            }
        }

        Ok(())
    }

    pub async fn list(
        all: bool,
        format: OutputFormat,
        color: bool,
    ) -> Result<()> {
        use crate::output::{print_json, format_table, print_info};
        use crate::storage::GLOBAL_STORAGE;
        use crate::api::DeviceStatus;
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

        let devices = if all {
            GLOBAL_STORAGE.get_all_devices()?
        } else {
            GLOBAL_STORAGE.get_devices_by_status(DeviceStatus::Online)?
        };

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
                let table_rows: Vec<DeviceTableRow> = devices.iter().map(|device| {
                    DeviceTableRow {
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
                    }
                }).collect();

                println!("{}", format_table(table_rows, color));
                print_info(&format!("Total: {} device(s) {}", 
                    devices.len(), 
                    if all { "" } else { "(online only)" }), color);
            }
        }

        Ok(())
    }

    pub async fn stats(
        device: Option<String>,
        watch: bool,
        interval: u64,
        format: OutputFormat,
        color: bool,
    ) -> Result<()> {
        use crate::output::{print_json, format_table, print_info, print_error, format_hashrate, format_temperature, format_power, format_uptime};
        use crate::storage::GLOBAL_STORAGE;
        use crate::api::DeviceStatus;
        use std::time::Duration;
        use tokio::time::sleep;
        use tabled::Tabled;

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

        loop {
            let devices = if let Some(device_id) = &device {
                // Find specific device
                if let Some(dev) = GLOBAL_STORAGE.find_device(device_id)? {
                    vec![dev]
                } else {
                    print_error(&format!("Device not found: {}", device_id), color);
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
                        let _ = GLOBAL_STORAGE.update_device_status(&device.ip_address, DeviceStatus::Offline);
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
                        let table_rows: Vec<StatsTableRow> = all_stats.iter().map(|stats| {
                            StatsTableRow {
                                name: stats.device_id.clone(),
                                hashrate: format_hashrate(stats.hashrate_mhs),
                                temperature: format_temperature(stats.temperature_celsius, color),
                                power: format_power(stats.power_watts),
                                fan_speed: format!("{}", stats.fan_speed_rpm),
                                uptime: format_uptime(stats.uptime_seconds),
                                pool: stats.pool_url.as_deref().unwrap_or("Unknown").to_string(),
                            }
                        }).collect();

                        println!("{}", format_table(table_rows, color));
                        
                        // Show summary
                        if all_stats.len() > 1 {
                            let total_hashrate: f64 = all_stats.iter().map(|s| s.hashrate_mhs).sum();
                            let total_power: f64 = all_stats.iter().map(|s| s.power_watts).sum();
                            let avg_temp: f64 = all_stats.iter().map(|s| s.temperature_celsius).sum::<f64>() / all_stats.len() as f64;
                            
                            println!();
                            print_info(&format!("Summary: {} devices, {} total, {:.1}W total, {:.1}°C avg", 
                                all_stats.len(),
                                format_hashrate(total_hashrate),
                                total_power,
                                avg_temp), color);
                        }
                    }
                }
            }

            if !watch {
                break;
            }

            if !matches!(format, OutputFormat::Json) {
                print_info(&format!("Updating in {}s... (Ctrl+C to stop)", interval), color);
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
        use std::time::Duration;
        let client = crate::api::AxeOsClient::with_timeout(&device.ip_address, Duration::from_secs(5))?;
        
        let (info, stats) = client.get_complete_info().await?;
        
        Ok(crate::api::DeviceStats::from_api_responses(
            device.ip_address.clone(),
            &info,
            &stats,
        ))
    }

    pub async fn control(
        device: String,
        action: ControlAction,
        format: OutputFormat,
        color: bool,
    ) -> Result<()> {
        use crate::output::{print_json, print_success, print_error, print_info};
        use crate::storage::GLOBAL_STORAGE;
        use crate::api::{AxeOsClient, SystemUpdateRequest};
        use std::time::Duration;

        // Find the device
        let device_info = if let Some(dev) = GLOBAL_STORAGE.find_device(&device)? {
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
                print_info(&format!("Setting fan speed to {}% on {}", speed, device_info.name), color);
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
                    })
                }
            }
            ControlAction::WifiScan => {
                print_info(&format!("Scanning WiFi networks on {}", device_info.name), color);
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
                    })
                }
            }
            ControlAction::UpdateFirmware { firmware } => {
                print_info(&format!("Updating firmware on {} from {}", device_info.name, firmware), color);
                client.update_firmware(&firmware).await
            }
            ControlAction::UpdateAxeOs { axeos } => {
                print_info(&format!("Updating AxeOS on {} from {}", device_info.name, axeos), color);
                client.update_axeos(&axeos).await
            }
        };

        match result {
            Ok(command_result) => {
                match format {
                    OutputFormat::Json => {
                        print_json(&command_result, true)?;
                    }
                    OutputFormat::Text => {
                        if command_result.success {
                            print_success(&command_result.message, color);
                            if let Some(data) = &command_result.data {
                                if let Some(networks) = data.get("networks") {
                                    println!("WiFi Networks:");
                                    if let Some(networks_array) = networks.as_array() {
                                        for network in networks_array {
                                            if let (Some(ssid), Some(rssi)) = (network.get("ssid"), network.get("rssi")) {
                                                println!("  {} ({}dBm)", ssid.as_str().unwrap_or("Unknown"), rssi.as_i64().unwrap_or(0));
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            print_error(&command_result.message, color);
                        }
                    }
                }
            }
            Err(e) => {
                match format {
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
                }
            }
        }

        Ok(())
    }

    pub async fn monitor(
        interval: u64,
        temp_alert: Option<f64>,
        hashrate_alert: Option<f64>,
        _db: Option<PathBuf>,
        format: OutputFormat,
        color: bool,
    ) -> Result<()> {
        use crate::output::{print_json, print_info, print_warning, print_error};
        use crate::storage::GLOBAL_STORAGE;
        use crate::api::DeviceStatus;
        use std::time::Duration;
        use tokio::time::sleep;
        use std::collections::HashMap;

        print_info(&format!("Starting continuous monitoring ({}s interval)", interval), color);
        if let Some(temp) = temp_alert {
            print_info(&format!("Temperature alert threshold: {:.1}°C", temp), color);
        }
        if let Some(hashrate) = hashrate_alert {
            print_info(&format!("Hashrate drop alert threshold: {:.1}%", hashrate), color);
        }
        print_info("Press Ctrl+C to stop monitoring", color);
        
        let mut previous_hashrates: HashMap<String, f64> = HashMap::new();
        let mut alert_count = 0;

        loop {
            let devices = GLOBAL_STORAGE.get_devices_by_status(DeviceStatus::Online)?;
            
            if devices.is_empty() {
                if matches!(format, OutputFormat::Text) {
                    print_info("No online devices to monitor", color);
                    print_info("Run 'axectl discover' to find devices", color);
                }
                sleep(Duration::from_secs(interval)).await;
                continue;
            }

            let mut all_stats = Vec::new();
            let mut alerts = Vec::new();
            
            // Collect stats from all devices
            for device in &devices {
                match collect_device_stats(device).await {
                    Ok(stats) => {
                        // Store stats in global storage
                        if let Err(e) = GLOBAL_STORAGE.store_stats(stats.clone()) {
                            tracing::warn!("Failed to store stats: {}", e);
                        }

                        // Check for alerts
                        if let Some(temp_threshold) = temp_alert {
                            if stats.temperature_celsius > temp_threshold {
                                alerts.push(format!("🌡️ {} temperature alert: {:.1}°C > {:.1}°C", 
                                    device.name, stats.temperature_celsius, temp_threshold));
                            }
                        }

                        if let Some(hashrate_threshold) = hashrate_alert {
                            if let Some(previous_hashrate) = previous_hashrates.get(&stats.device_id) {
                                let drop_percent = ((previous_hashrate - stats.hashrate_mhs) / previous_hashrate) * 100.0;
                                if drop_percent > hashrate_threshold {
                                    alerts.push(format!("📉 {} hashrate drop: {:.1}% ({} -> {})", 
                                        device.name, drop_percent,
                                        crate::output::format_hashrate(*previous_hashrate),
                                        crate::output::format_hashrate(stats.hashrate_mhs)));
                                }
                            }
                            previous_hashrates.insert(stats.device_id.clone(), stats.hashrate_mhs);
                        }

                        all_stats.push(stats);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to collect stats from {}: {}", device.ip_address, e);
                        // Mark device as offline
                        let _ = GLOBAL_STORAGE.update_device_status(&device.ip_address, DeviceStatus::Offline);
                        alerts.push(format!("🔌 {} went offline: {}", device.name, e));
                    }
                }
            }

            // Update device offline detection
            let _ = GLOBAL_STORAGE.mark_stale_devices_offline(interval * 3);

            // Output monitoring data
            match format {
                OutputFormat::Json => {
                    let swarm_summary = GLOBAL_STORAGE.get_swarm_summary()?;
                    let output = serde_json::json!({
                        "monitoring": {
                            "interval_seconds": interval,
                            "alerts": alerts,
                            "alert_count": alert_count,
                            "devices_monitored": devices.len(),
                            "devices_responding": all_stats.len()
                        },
                        "statistics": all_stats,
                        "summary": swarm_summary,
                        "timestamp": chrono::Utc::now()
                    });
                    print_json(&output, true)?;
                }
                OutputFormat::Text => {
                    // Clear screen for monitoring updates
                    print!("\x1B[2J\x1B[1;1H");
                    
                    println!("🔍 Swarm Monitor - {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    
                    if !all_stats.is_empty() {
                        // Show summary
                        let total_hashrate: f64 = all_stats.iter().map(|s| s.hashrate_mhs).sum();
                        let total_power: f64 = all_stats.iter().map(|s| s.power_watts).sum();
                        let avg_temp: f64 = all_stats.iter().map(|s| s.temperature_celsius).sum::<f64>() / all_stats.len() as f64;
                        let max_temp: f64 = all_stats.iter().map(|s| s.temperature_celsius).fold(0.0, f64::max);
                        
                        println!("📊 Summary: {} devices | {} | {:.1}W | Avg: {:.1}°C | Max: {:.1}°C",
                            all_stats.len(),
                            crate::output::format_hashrate(total_hashrate),
                            total_power,
                            avg_temp,
                            max_temp);
                        
                        // Show individual devices
                        println!();
                        for stats in &all_stats {
                            let temp_color = if stats.temperature_celsius >= 80.0 { "🔥" } 
                                else if stats.temperature_celsius >= 70.0 { "🌡️" } 
                                else { "🟢" };
                            
                            println!("{} {} | {} | {:.1}°C | {} | {}",
                                temp_color,
                                stats.device_id,
                                crate::output::format_hashrate(stats.hashrate_mhs),
                                stats.temperature_celsius,
                                crate::output::format_power(stats.power_watts),
                                crate::output::format_uptime(stats.uptime_seconds));
                        }
                    } else {
                        print_error("No devices responding", color);
                    }
                    
                    // Show alerts
                    if !alerts.is_empty() {
                        println!();
                        println!("🚨 ALERTS:");
                        for alert in &alerts {
                            print_warning(alert, color);
                        }
                        alert_count += alerts.len();
                    }
                    
                    if alert_count > 0 {
                        println!();
                        print_info(&format!("Total alerts this session: {}", alert_count), color);
                    }
                    
                    println!();
                    print_info(&format!("Next update in {}s... (Ctrl+C to stop)", interval), color);
                }
            }

            sleep(Duration::from_secs(interval)).await;
        }
    }
}