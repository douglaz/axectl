use crate::api::{Device, DeviceStats, DeviceStatus, SwarmSummary};
use crate::cache::{DeviceCache, get_cache_dir};
use crate::cli::commands::handlers::discovery::perform_discovery;
use crate::cli::commands::{DeviceFilterArg, OutputFormat};
use crate::output::{
    ColoredTemperature, format_hashrate, format_power, format_table, format_uptime, print_info,
    print_json, print_success, print_warning,
};
use alphanumeric_sort::compare_str;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::future::join_all;
use std::collections::HashMap;
use std::future::Future;
use std::io::{Write as IoWrite, stdout};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tabled::Tabled;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{interval, timeout};

/// Shared state for the monitor
#[derive(Debug, Clone)]
pub struct MonitorState {
    pub devices: HashMap<String, Device>,
    pub alerts: Vec<Alert>,
    pub discovery_active: bool,
    pub last_discovery: Option<DateTime<Utc>>,
    pub alert_count: usize,
    pub previous_hashrates: HashMap<String, f64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Alert {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub device_ip: String,
}

/// Configuration for the async monitor
pub struct AsyncMonitorConfig<'a> {
    pub interval: u64,
    pub temp_alert: Option<f64>,
    pub hashrate_alert: Option<f64>,
    pub type_filter: Option<DeviceFilterArg>,
    pub type_summary: bool,
    pub format: OutputFormat,
    pub color: bool,
    pub cache_dir: Option<&'a Path>,
    pub all: bool,
    pub no_stats: bool,
    pub discover: bool,
    pub discover_interval: u64,
    pub network: Option<String>,
    pub no_mdns: bool,
}

/// Message types for communication between tasks
#[derive(Debug)]
enum MonitorMessage {
    NewDevices(Vec<Device>),
    DiscoveryComplete(usize),
}

struct DiscoveryLoopContext {
    discover_interval: u64,
    shutdown: Arc<AtomicBool>,
    state: Arc<RwLock<MonitorState>>,
    cache: Arc<RwLock<DeviceCache>>,
    tx_discovery: mpsc::Sender<MonitorMessage>,
    network: Option<String>,
    no_mdns: bool,
    color: bool,
    cache_path_buf: PathBuf,
}

/// Table row for full stats display
#[derive(Tabled)]
struct MonitorTableRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "IP")]
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

/// Table row for no-stats display
#[derive(Tabled)]
struct BasicMonitorTableRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "IP")]
    ip_address: String,
    #[tabled(rename = "Type")]
    device_type: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Last Seen")]
    last_seen: String,
}

pub async fn monitor_async(config: AsyncMonitorConfig<'_>) -> Result<()> {
    // Set up alternate screen for text mode to prevent flicker.
    // The alternate screen is a separate buffer that doesn't affect the main terminal scrollback.
    let use_alternate_screen = matches!(config.format, OutputFormat::Text);

    if use_alternate_screen {
        let mut stdout_handle = stdout();
        // EnterAlternateScreen: Switch to a separate screen buffer (like vim or less does)
        // Hide: Hide the cursor for cleaner display during updates
        execute!(stdout_handle, EnterAlternateScreen, Hide)?;
    }

    // Create a guard that will automatically restore the terminal when this function exits.
    // The underscore prefix (_cleanup) tells Rust we won't use this variable directly,
    // but we want to keep it alive until the function ends.
    // This guard ensures the terminal is restored even if the function panics or returns early.
    let _cleanup = CleanupGuard::new(use_alternate_screen);

    // Set up signal handling for graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));

    monitor_async_impl(config, shutdown).await
}

/// RAII guard that automatically restores the terminal to its original state when dropped.
/// This ensures the terminal is never left in alternate screen mode or with a hidden cursor,
/// even if the program panics or exits unexpectedly.
///
/// RAII (Resource Acquisition Is Initialization) is a pattern where cleanup happens
/// automatically when a value goes out of scope, similar to try/finally in other languages.
struct CleanupGuard {
    use_alternate_screen: bool,
}

impl CleanupGuard {
    fn new(use_alternate_screen: bool) -> Self {
        Self {
            use_alternate_screen,
        }
    }
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

async fn monitor_async_impl(
    config: AsyncMonitorConfig<'_>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    // Get cache directory, using default if not provided
    let cache_path = get_cache_dir(config.cache_dir)?;
    let cache_path = cache_path.as_ref();

    // Initialize shared state
    let state = Arc::new(RwLock::new(MonitorState {
        devices: HashMap::new(),
        alerts: Vec::new(),
        discovery_active: false,
        last_discovery: None,
        alert_count: 0,
        previous_hashrates: HashMap::new(),
    }));

    // Load initial cache
    let cache = Arc::new(RwLock::new(DeviceCache::load(cache_path)?));
    {
        let cache_guard = cache.read().await;
        if !cache_guard.is_empty() && matches!(config.format, OutputFormat::Text) {
            print_info(
                &format!(
                    "📦 Loaded {count} device(s) from cache",
                    count = cache_guard.device_count()
                ),
                config.color,
            );
        }

        // Initialize state with cached devices
        let mut state_guard = state.write().await;
        let devices = if let Some(ref filter_arg) = config.type_filter {
            if config.all {
                cache_guard.get_devices_by_filter(filter_arg.0)
            } else {
                cache_guard.get_online_devices_by_filter(filter_arg.0)
            }
        } else if config.all {
            cache_guard.get_all_devices()
        } else {
            cache_guard.get_devices_by_status(DeviceStatus::Online)
        };

        for device in devices {
            state_guard
                .devices
                .insert(device.ip_address.clone(), device);
        }
    }

    // Create communication channel
    let (tx, mut rx) = mpsc::channel::<MonitorMessage>(100);

    // Spawn background discovery task if enabled
    let _discovery_handle = if config.discover {
        Some(tokio::spawn(run_discovery_loop(
            DiscoveryLoopContext {
                discover_interval: config.discover_interval,
                shutdown: shutdown.clone(),
                state: state.clone(),
                cache: cache.clone(),
                tx_discovery: tx.clone(),
                network: config.network.clone(),
                no_mdns: config.no_mdns,
                color: config.color,
                cache_path_buf: cache_path.to_path_buf(),
            },
            |network, no_mdns, cache_path_buf, color| async move {
                perform_discovery(network, 30, !no_mdns, Some(cache_path_buf.as_path()), color)
                    .await
            },
        )))
    } else {
        None
    };

    // Main monitoring loop
    let mut monitor_timer = interval(Duration::from_secs(config.interval));

    loop {
        tokio::select! {
            // Check for shutdown signal with high priority
            _ = tokio::signal::ctrl_c() => {
                if matches!(config.format, OutputFormat::Text) {
                    print_info("\n✅ Shutting down gracefully...", config.color);
                }
                break;
            }

            _ = monitor_timer.tick() => {
                // Check shutdown flag set by signal handler
                if shutdown.load(Ordering::SeqCst) {
                    if matches!(config.format, OutputFormat::Text) {
                        print_info("\n✅ Shutting down gracefully...", config.color);
                    }
                    break;
                }

                // Collect and display stats
                update_and_display(
                    &state,
                    &cache,
                    cache_path,
                    &config,
                    &tx,
                ).await?;
            }

            Some(msg) = rx.recv() => {
                if handle_monitor_message(&state, &config, msg).await {
                    update_and_display(&state, &cache, cache_path, &config, &tx).await?;
                }
            }
        }
    }

    Ok(())
}

async fn run_discovery_loop<F, Fut>(ctx: DiscoveryLoopContext, perform_discovery_fn: F)
where
    F: Fn(Option<String>, bool, PathBuf, bool) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<Device>>> + Send + 'static,
{
    let mut discovery_timer = interval(Duration::from_secs(ctx.discover_interval));

    loop {
        if ctx.shutdown.load(Ordering::SeqCst) {
            break;
        }

        discovery_timer.tick().await;

        {
            let mut state_guard = ctx.state.write().await;
            state_guard.discovery_active = true;
        }

        match perform_discovery_fn(
            ctx.network.clone(),
            ctx.no_mdns,
            ctx.cache_path_buf.clone(),
            ctx.color,
        )
        .await
        {
            Ok(discovered) => {
                let count = discovered.len();
                let mut new_devices = Vec::new();

                {
                    let mut cache_guard = ctx.cache.write().await;
                    let state_guard = ctx.state.read().await;

                    for device in discovered {
                        if !state_guard.devices.contains_key(&device.ip_address) {
                            new_devices.push(device.clone());
                        }
                        cache_guard.update_device(device);
                    }

                    if let Err(e) = cache_guard.save(&ctx.cache_path_buf) {
                        tracing::warn!("Failed to save cache after discovery: {e}");
                    }
                }

                if !new_devices.is_empty() {
                    let _ = ctx
                        .tx_discovery
                        .send(MonitorMessage::NewDevices(new_devices))
                        .await;
                }

                let _ = ctx
                    .tx_discovery
                    .send(MonitorMessage::DiscoveryComplete(count))
                    .await;

                {
                    let mut state_guard = ctx.state.write().await;
                    state_guard.discovery_active = false;
                    state_guard.last_discovery = Some(Utc::now());
                }
            }
            Err(e) => {
                tracing::warn!("Background discovery failed: {e}");
                let mut state_guard = ctx.state.write().await;
                state_guard.discovery_active = false;
            }
        }
    }
}

async fn handle_monitor_message(
    state: &Arc<RwLock<MonitorState>>,
    config: &AsyncMonitorConfig<'_>,
    msg: MonitorMessage,
) -> bool {
    match msg {
        MonitorMessage::NewDevices(devices) => {
            let mut state_guard = state.write().await;
            for device in devices {
                if matches!(config.format, OutputFormat::Text) {
                    print_success(
                        &format!(
                            "🆕 New device discovered: {name} ({ip})",
                            name = device.name,
                            ip = device.ip_address
                        ),
                        config.color,
                    );
                }
                state_guard
                    .devices
                    .insert(device.ip_address.clone(), device);
            }
            true
        }
        MonitorMessage::DiscoveryComplete(count) => {
            if matches!(config.format, OutputFormat::Text) {
                print_info(
                    &format!("✓ Background discovery complete, {count} total devices found"),
                    config.color,
                );
            }
            false
        }
    }
}

async fn update_and_display(
    state: &Arc<RwLock<MonitorState>>,
    cache: &Arc<RwLock<DeviceCache>>,
    cache_path: &Path,
    config: &AsyncMonitorConfig<'_>,
    _tx: &mpsc::Sender<MonitorMessage>,
) -> Result<()> {
    // Get current devices based on filter
    let devices: Vec<Device> = {
        let state_guard = state.read().await;
        let mut devices: Vec<_> = state_guard.devices.values().cloned().collect();

        // Apply filters
        if let Some(ref filter_arg) = config.type_filter {
            devices.retain(|d| filter_arg.0.matches(d.device_type));
        }

        if !config.all {
            devices.retain(|d| d.status == DeviceStatus::Online);
        }

        devices
    };

    if devices.is_empty() {
        if matches!(config.format, OutputFormat::Text) {
            if let Some(ref type_filter) = config.type_filter {
                print_warning(&format!("No {type_filter} devices found"), config.color);
            } else if config.all {
                print_warning("No devices found", config.color);
            } else {
                print_warning("No online devices found", config.color);
                print_info("Use --all to show offline devices", config.color);
            }

            print_info(
                &format!(
                    "Run 'axectl discover --cache-dir {path}' to find devices",
                    path = cache_path.display()
                ),
                config.color,
            );
        }
        return Ok(());
    }

    // Collect stats asynchronously if not in no-stats mode
    let mut device_stats = Vec::new();
    let mut alerts = Vec::new();

    if !config.no_stats {
        // Create futures for all device stats collection
        let stats_futures: Vec<_> = devices
            .iter()
            .filter(|d| d.status == DeviceStatus::Online)
            .map(|device| {
                let device_clone = device.clone();
                async move {
                    let result =
                        timeout(Duration::from_secs(60), collect_device_stats(&device_clone)).await;

                    (device_clone.ip_address.clone(), result)
                }
            })
            .collect();

        // Execute all stats collection in parallel
        let results = join_all(stats_futures).await;

        // Process results and update state
        let mut state_guard = state.write().await;
        let mut cache_guard = cache.write().await;

        for (ip, result) in results {
            match result {
                Ok(Ok(stats)) => {
                    // Check for alerts
                    if let Some(temp_threshold) = config.temp_alert
                        && stats.temperature_celsius > temp_threshold
                        && let Some(device) = state_guard.devices.get(&ip)
                    {
                        alerts.push(Alert {
                            timestamp: Utc::now(),
                            message: format!(
                                "🌡️ {name} temperature alert: {temp:.1}°C > {threshold:.1}°C",
                                name = device.name,
                                temp = stats.temperature_celsius,
                                threshold = temp_threshold
                            ),
                            device_ip: ip.clone(),
                        });
                    }

                    if let Some(hashrate_threshold) = config.hashrate_alert {
                        if let Some(previous_hashrate) = state_guard.previous_hashrates.get(&ip) {
                            let drop_percent = ((previous_hashrate - stats.hashrate_mhs)
                                / previous_hashrate)
                                * 100.0;
                            if drop_percent > hashrate_threshold
                                && let Some(device) = state_guard.devices.get(&ip)
                            {
                                alerts.push(Alert {
                                    timestamp: Utc::now(),
                                    message: format!(
                                        "📉 {name} hashrate drop: {drop:.1}% ({prev} -> {curr})",
                                        name = device.name,
                                        drop = drop_percent,
                                        prev = format_hashrate(*previous_hashrate),
                                        curr = format_hashrate(stats.hashrate_mhs)
                                    ),
                                    device_ip: ip.clone(),
                                });
                            }
                        }
                        state_guard
                            .previous_hashrates
                            .insert(ip.clone(), stats.hashrate_mhs);
                    }

                    // Update device with stats
                    if let Some(device) = state_guard.devices.get_mut(&ip) {
                        device.stats = Some(stats.clone());
                        device.status = DeviceStatus::Online;
                        device.last_seen = Utc::now();
                    }

                    // Update cache
                    cache_guard.update_device_stats(&ip, stats.clone());
                    device_stats.push(Some(stats));
                }
                Ok(Err(e)) => {
                    tracing::warn!("Failed to collect stats from {ip}: {e}");

                    // Mark device as offline
                    if let Some(device) = state_guard.devices.get_mut(&ip) {
                        device.status = DeviceStatus::Offline;
                        alerts.push(Alert {
                            timestamp: Utc::now(),
                            message: format!("🔌 {name} went offline", name = device.name),
                            device_ip: ip.clone(),
                        });
                    }

                    cache_guard.mark_device_probed(&ip, false);
                    device_stats.push(None);
                }
                Err(_) => {
                    tracing::warn!("Failed to collect stats from {ip} (timeout)");

                    // Mark device as offline
                    if let Some(device) = state_guard.devices.get_mut(&ip) {
                        device.status = DeviceStatus::Offline;
                        alerts.push(Alert {
                            timestamp: Utc::now(),
                            message: format!("🔌 {name} went offline", name = device.name),
                            device_ip: ip.clone(),
                        });
                    }

                    cache_guard.mark_device_probed(&ip, false);
                    device_stats.push(None);
                }
            }
        }

        // Add alerts to state
        state_guard.alerts.extend(alerts.clone());
        state_guard.alert_count += alerts.len();

        // Save cache
        if let Err(e) = cache_guard.save(cache_path) {
            tracing::warn!("Failed to save cache: {e}");
        }
    }

    // Display results
    display_results(state, cache, &devices, &device_stats, &alerts, config).await?;

    Ok(())
}

async fn display_results(
    state: &Arc<RwLock<MonitorState>>,
    cache: &Arc<RwLock<DeviceCache>>,
    devices: &[Device],
    device_stats: &[Option<DeviceStats>],
    alerts: &[Alert],
    config: &AsyncMonitorConfig<'_>,
) -> Result<()> {
    let state_guard = state.read().await;

    match config.format {
        OutputFormat::Json => {
            let devices_with_stats: Vec<serde_json::Value> = if config.no_stats {
                let mut serialized_devices = Vec::new();
                for d in devices.iter() {
                    let value = serde_json::to_value(d).with_context(|| {
                        format!("Failed to serialize device {name}", name = d.name)
                    })?;
                    serialized_devices.push(value);
                }
                serialized_devices
            } else {
                let mut serialized_devices = Vec::new();
                for (device, stats) in devices.iter().zip(device_stats.iter()) {
                    let mut device_json = serde_json::to_value(device).with_context(|| {
                        format!("Failed to serialize device {name}", name = device.name)
                    })?;
                    if let Some(stats) = stats {
                        device_json["stats"] = serde_json::to_value(stats).with_context(|| {
                            format!(
                                "Failed to serialize stats for device {name}",
                                name = device.name
                            )
                        })?;
                    }
                    serialized_devices.push(device_json);
                }
                serialized_devices
            };

            // Calculate swarm summary
            let online_devices: Vec<_> = devices
                .iter()
                .filter(|d| d.status == DeviceStatus::Online && d.stats.is_some())
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
                        .filter_map(|d| d.stats.as_ref())
                        .map(|s| s.hashrate_mhs)
                        .sum(),
                    total_power_watts: online_devices
                        .iter()
                        .filter_map(|d| d.stats.as_ref())
                        .map(|s| s.power_watts)
                        .sum(),
                    average_temperature: online_devices
                        .iter()
                        .filter_map(|d| d.stats.as_ref())
                        .map(|s| s.temperature_celsius)
                        .sum::<f64>()
                        / online_devices.len() as f64,
                    average_efficiency: {
                        let total_power: f64 = online_devices
                            .iter()
                            .filter_map(|d| d.stats.as_ref())
                            .map(|s| s.power_watts)
                            .sum();
                        if total_power > 0.0 {
                            let total_hashrate: f64 = online_devices
                                .iter()
                                .filter_map(|d| d.stats.as_ref())
                                .map(|s| s.hashrate_mhs)
                                .sum();
                            total_hashrate / total_power
                        } else {
                            0.0
                        }
                    },
                }
            };

            let mut output = serde_json::json!({
                "devices": devices_with_stats,
                "summary": swarm_summary,
                "timestamp": chrono::Utc::now(),
                "discovery_active": state_guard.discovery_active,
            });

            if !alerts.is_empty() {
                output["alerts"] = serde_json::json!(alerts);
                output["alert_count"] = serde_json::json!(state_guard.alert_count);
            }

            if config.type_summary {
                let cache_guard = cache.read().await;
                let type_summaries = cache_guard.get_type_summaries();
                output["type_summaries"] = serde_json::to_value(type_summaries)?;
            }

            if let Some(last_discovery) = state_guard.last_discovery {
                output["last_discovery"] = serde_json::json!(last_discovery);
            }

            print_json(&output, true)?;
        }
        OutputFormat::Text => {
            // Buffer all output to reduce flickering
            let mut output_buffer = String::new();
            use std::fmt::Write as FmtWrite;

            if config.no_stats {
                // Basic table without stats
                // Sort devices by hostname using natural/alphanumeric sorting
                let mut sorted_devices: Vec<_> = devices.iter().collect();
                sorted_devices.sort_by(|a, b| compare_str(&a.name, &b.name));

                let table_rows: Vec<BasicMonitorTableRow> = sorted_devices
                    .iter()
                    .map(|device| BasicMonitorTableRow {
                        name: device.name.clone(),
                        ip_address: device.ip_address.clone(),
                        device_type: device.device_type.as_str().to_string(),
                        status: format!("{status:?}", status = device.status),
                        last_seen: format_last_seen(device.last_seen),
                    })
                    .collect();

                writeln!(
                    &mut output_buffer,
                    "{}",
                    format_table(table_rows, config.color)
                )?;
            } else {
                // Full table with stats
                // Sort devices by hostname using natural/alphanumeric sorting
                let mut sorted_devices: Vec<_> = devices.iter().collect();
                sorted_devices.sort_by(|a, b| compare_str(&a.name, &b.name));

                let table_rows: Vec<MonitorTableRow> = sorted_devices
                    .iter()
                    .map(|device| {
                        if let Some(ref stats) = device.stats {
                            MonitorTableRow {
                                name: device.name.clone(),
                                ip_address: device.ip_address.clone(),
                                device_type: device.device_type.as_str().to_string(),
                                status: format!("{status:?}", status = device.status),
                                hashrate: format_hashrate(stats.hashrate_mhs),
                                temperature: ColoredTemperature::new(
                                    stats.temperature_celsius,
                                    config.color,
                                )
                                .to_string(),
                                power: format_power(stats.power_watts),
                                fan_speed: format!("{rpm}", rpm = stats.fan_speed_rpm),
                                uptime: format_uptime(stats.uptime_seconds),
                                pool: stats.pool_url.as_deref().unwrap_or("-").to_string(),
                            }
                        } else {
                            MonitorTableRow {
                                name: device.name.clone(),
                                ip_address: device.ip_address.clone(),
                                device_type: device.device_type.as_str().to_string(),
                                status: format!("{status:?}", status = device.status),
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

                writeln!(
                    &mut output_buffer,
                    "{}",
                    format_table(table_rows, config.color)
                )?;

                // Show summary
                let online_stats: Vec<_> =
                    devices.iter().filter_map(|d| d.stats.as_ref()).collect();

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
                        "ℹ Summary: {count} devices, {hashrate} total, {power:.1}W total, {temp:.1}°C avg",
                        count = online_stats.len(),
                        hashrate = format_hashrate(total_hashrate),
                        power = total_power,
                        temp = avg_temp
                    )?;
                }
            }

            // Show alerts if any
            if !alerts.is_empty() {
                writeln!(&mut output_buffer)?;
                writeln!(&mut output_buffer, "🚨 ALERTS:")?;
                for alert in alerts {
                    writeln!(&mut output_buffer, "⚠️ {}", alert.message)?;
                }
            }

            // Show type summaries if requested
            if config.type_summary && !config.no_stats {
                writeln!(&mut output_buffer)?;
                writeln!(&mut output_buffer, "📊 Device Type Summaries:")?;
                writeln!(
                    &mut output_buffer,
                    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                )?;

                let cache_guard = cache.read().await;
                let type_summaries = cache_guard.get_type_summaries();
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

            // Status line
            let discovery_status = if state_guard.discovery_active {
                " | 🔍 Discovery active"
            } else if let Some(last) = state_guard.last_discovery {
                let mins_ago = (Utc::now() - last).num_minutes();
                &format!(" | Last discovery: {mins_ago}m ago")
            } else {
                ""
            };

            writeln!(
                &mut output_buffer,
                "ℹ Updating in {interval}s... (Ctrl+C to stop) | {count} total alerts{status}",
                interval = config.interval,
                count = state_guard.alert_count,
                status = discovery_status
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

    Ok(())
}

async fn collect_device_stats(device: &Device) -> Result<DeviceStats> {
    let client =
        crate::api::AxeOsClient::with_timeout(&device.ip_address, Duration::from_secs(60))?;
    let (info, stats) = client.get_complete_info().await?;
    Ok(DeviceStats::from_api_responses(&info, &stats))
}

fn format_last_seen(last_seen: DateTime<Utc>) -> String {
    let duration = Utc::now() - last_seen;
    if duration.num_seconds() < 60 {
        "Just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{mins}m ago", mins = duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{hours}h ago", hours = duration.num_hours())
    } else {
        format!("{days}d ago", days = duration.num_days())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::DeviceType;
    use tempfile::tempdir;

    fn test_device(ip_address: &str) -> Device {
        Device {
            name: "test-device".to_string(),
            ip_address: ip_address.to_string(),
            device_type: DeviceType::BitaxeUltra,
            serial_number: None,
            status: DeviceStatus::Online,
            discovered_at: Utc::now(),
            last_seen: Utc::now(),
            stats: None,
        }
    }

    fn test_state() -> Arc<RwLock<MonitorState>> {
        Arc::new(RwLock::new(MonitorState {
            devices: HashMap::new(),
            alerts: Vec::new(),
            discovery_active: false,
            last_discovery: None,
            alert_count: 0,
            previous_hashrates: HashMap::new(),
        }))
    }

    #[tokio::test]
    async fn discovery_loop_starts_without_waiting_for_discover_interval() {
        let tempdir = tempdir().expect("tempdir");
        let state = test_state();
        let cache = Arc::new(RwLock::new(DeviceCache::new()));
        let (tx, mut rx) = mpsc::channel(10);
        let shutdown = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let device = test_device("192.168.1.10");

        let handle = tokio::spawn(run_discovery_loop(
            DiscoveryLoopContext {
                discover_interval: 60,
                shutdown: shutdown.clone(),
                state,
                cache,
                tx_discovery: tx,
                network: None,
                no_mdns: false,
                color: false,
                cache_path_buf: tempdir.path().to_path_buf(),
            },
            {
                let calls = calls.clone();
                let device = device.clone();
                move |_network, _no_mdns, _cache_path, _color| {
                    let calls = calls.clone();
                    let device = device.clone();
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok(vec![device])
                    }
                }
            },
        ));

        let msg = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("discovery loop should emit promptly")
            .expect("channel should stay open");

        match msg {
            MonitorMessage::NewDevices(devices) => {
                assert_eq!(devices.len(), 1);
                assert_eq!(devices[0].ip_address, "192.168.1.10");
            }
            MonitorMessage::DiscoveryComplete(_) => {
                panic!("expected new devices before discovery completion")
            }
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);

        shutdown.store(true, Ordering::SeqCst);
        handle.abort();
    }

    #[tokio::test]
    async fn new_devices_message_requests_immediate_refresh() {
        let tempdir = tempdir().expect("tempdir");
        let state = test_state();
        let device = test_device("192.168.1.11");
        let config = AsyncMonitorConfig {
            interval: 1,
            temp_alert: None,
            hashrate_alert: None,
            type_filter: None,
            type_summary: false,
            format: OutputFormat::Json,
            color: false,
            cache_dir: Some(tempdir.path()),
            all: false,
            no_stats: true,
            discover: false,
            discover_interval: 60,
            network: None,
            no_mdns: false,
        };

        let refresh_requested =
            handle_monitor_message(&state, &config, MonitorMessage::NewDevices(vec![device])).await;

        assert!(refresh_requested);
        let state_guard = state.read().await;
        assert!(state_guard.devices.contains_key("192.168.1.11"));
    }
}
