use crate::api::{DeviceFilter, DeviceType};
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

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

#[derive(Clone, Copy, ValueEnum, PartialEq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Wrapper type for device filtering in CLI that can parse both
/// specific device types (bitaxe-ultra, nerdqaxe-plus) and
/// group filters (bitaxe, nerdqaxe, all)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeviceFilterArg(pub DeviceFilter);

impl FromStr for DeviceFilterArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        DeviceFilter::from_str(s).map(DeviceFilterArg)
    }
}

impl std::fmt::Display for DeviceFilterArg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
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

    /// List known devices with statistics
    List {
        /// Include offline devices
        #[arg(long)]
        all: bool,

        /// Skip fetching live statistics (faster)
        #[arg(long)]
        no_stats: bool,

        /// Enable continuous monitoring
        #[arg(long, short)]
        watch: bool,

        /// Update interval in seconds (with --watch)
        #[arg(long, default_value = "30")]
        interval: u64,

        /// Perform network discovery before listing
        #[arg(long)]
        discover: bool,

        /// Network range to scan (auto-detected if not specified, only with --discover)
        #[arg(long)]
        network: Option<String>,

        /// Discovery timeout in seconds (only with --discover)
        #[arg(long, default_value = "30")]
        timeout: u64,

        /// Skip mDNS discovery (only with --discover)
        #[arg(long)]
        no_mdns: bool,

        /// Filter devices by type (e.g., bitaxe-ultra, bitaxe-max, nerdqaxe, bitaxe, all)
        #[arg(long, value_name = "TYPE")]
        device_type: Option<DeviceFilterArg>,

        /// Alert on high temperature (celsius, only with --watch)
        #[arg(long)]
        temp_alert: Option<f64>,

        /// Alert on hashrate drop (percentage, only with --watch)
        #[arg(long)]
        hashrate_alert: Option<f64>,

        /// Show per-type summaries
        #[arg(long)]
        type_summary: bool,
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

        /// Monitor only devices of a specific type
        #[arg(long, value_name = "TYPE")]
        device_type: Option<DeviceFilterArg>,

        /// Show per-type summaries
        #[arg(long)]
        type_summary: bool,
    },

    /// Bulk operations on groups of devices
    Bulk {
        #[command(subcommand)]
        action: BulkAction,
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

#[derive(Subcommand)]
pub enum BulkAction {
    /// Restart selected devices
    Restart {
        /// Filter by device type (can be specified multiple times)
        #[arg(long = "device-type", value_name = "TYPE")]
        device_types: Vec<DeviceType>,
        /// Target specific IP addresses (can be specified multiple times)
        #[arg(long = "ip-address", value_name = "IP")]
        ip_addresses: Vec<String>,
        /// Target all devices
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Set fan speed for selected devices
    SetFanSpeed {
        /// Fan speed percentage (0-100)
        speed: u8,
        /// Filter by device type (can be specified multiple times)
        #[arg(long = "device-type", value_name = "TYPE")]
        device_types: Vec<DeviceType>,
        /// Target specific IP addresses (can be specified multiple times)
        #[arg(long = "ip-address", value_name = "IP")]
        ip_addresses: Vec<String>,
        /// Target all devices
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Update settings for selected devices
    UpdateSettings {
        /// JSON string with settings to update
        settings: String,
        /// Filter by device type (can be specified multiple times)
        #[arg(long = "device-type", value_name = "TYPE")]
        device_types: Vec<DeviceType>,
        /// Target specific IP addresses (can be specified multiple times)
        #[arg(long = "ip-address", value_name = "IP")]
        ip_addresses: Vec<String>,
        /// Target all devices
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Scan WiFi on selected devices
    WifiScan {
        /// Filter by device type (can be specified multiple times)
        #[arg(long = "device-type", value_name = "TYPE")]
        device_types: Vec<DeviceType>,
        /// Target specific IP addresses (can be specified multiple times)
        #[arg(long = "ip-address", value_name = "IP")]
        ip_addresses: Vec<String>,
        /// Target all devices
        #[arg(long)]
        all: bool,
    },

    /// Update firmware on selected devices
    UpdateFirmware {
        /// Firmware URL or file path
        firmware: String,
        /// Filter by device type (can be specified multiple times)
        #[arg(long = "device-type", value_name = "TYPE")]
        device_types: Vec<DeviceType>,
        /// Target specific IP addresses (can be specified multiple times)
        #[arg(long = "ip-address", value_name = "IP")]
        ip_addresses: Vec<String>,
        /// Target all devices
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Maximum parallel operations
        #[arg(long, default_value = "5")]
        parallel: usize,
    },

    /// Update AxeOS on selected devices
    UpdateAxeOs {
        /// AxeOS update URL or file path
        axeos: String,
        /// Filter by device type (can be specified multiple times)
        #[arg(long = "device-type", value_name = "TYPE")]
        device_types: Vec<DeviceType>,
        /// Target specific IP addresses (can be specified multiple times)
        #[arg(long = "ip-address", value_name = "IP")]
        ip_addresses: Vec<String>,
        /// Target all devices
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Maximum parallel operations
        #[arg(long, default_value = "5")]
        parallel: usize,
    },
}

impl Cli {
    pub async fn run(self) -> Result<()> {
        // Initialize logging
        if self.verbose {
            // Configure tracing to output to stderr instead of stdout
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .init();
        }

        match self.command {
            Commands::Discover {
                network,
                timeout,
                no_mdns,
            } => {
                handlers::discover(
                    network,
                    timeout,
                    !no_mdns,
                    self.format,
                    !self.no_color,
                    self.cache_dir.as_deref(),
                )
                .await
            }
            Commands::List {
                all,
                no_stats,
                watch,
                interval,
                discover,
                network,
                timeout,
                no_mdns,
                device_type,
                temp_alert,
                hashrate_alert,
                type_summary,
            } => {
                let args = handlers::ListArgs {
                    all,
                    no_stats,
                    watch,
                    interval,
                    discover,
                    network,
                    timeout,
                    no_mdns,
                    device_type,
                    temp_alert,
                    hashrate_alert,
                    type_summary,
                    format: self.format,
                    color: !self.no_color,
                    cache_dir: self.cache_dir.as_deref(),
                };
                handlers::list(args).await
            }
            Commands::Control { device, action } => {
                handlers::control(
                    device,
                    action,
                    self.format,
                    !self.no_color,
                    self.cache_dir.as_deref(),
                )
                .await
            }
            Commands::Monitor {
                interval,
                temp_alert,
                hashrate_alert,
                device_type,
                type_summary,
            } => {
                handlers::monitor(handlers::monitor::MonitorConfig {
                    interval,
                    temp_alert,
                    hashrate_alert,
                    type_filter: device_type,
                    type_summary,
                    format: self.format,
                    color: !self.no_color,
                    cache_dir: self.cache_dir.as_deref(),
                })
                .await
            }
            Commands::Bulk { action } => {
                handlers::bulk(
                    action,
                    self.format,
                    !self.no_color,
                    self.cache_dir.as_deref(),
                )
                .await
            }
        }
    }
}

pub mod handlers;
