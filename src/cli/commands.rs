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

#[derive(Clone, Copy, ValueEnum, PartialEq)]
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

        /// Monitor only devices of a specific type
        #[arg(long, value_name = "TYPE")]
        device_type: Option<String>,

        /// Show per-type summaries
        #[arg(long)]
        type_summary: bool,
    },

    /// Manage device groups
    Group {
        #[command(subcommand)]
        action: GroupAction,
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
pub enum GroupAction {
    /// Create a new device group
    Create {
        /// Group name
        name: String,
        /// Optional description
        #[arg(long, short)]
        description: Option<String>,
    },

    /// List all groups
    List {
        /// Show detailed information
        #[arg(long, short)]
        detailed: bool,
    },

    /// Show group details
    Show {
        /// Group name or ID
        group: String,
    },

    /// Delete a group
    Delete {
        /// Group name or ID
        group: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Add device to group
    AddDevice {
        /// Group name or ID
        group: String,
        /// Device name or IP
        device: String,
    },

    /// Remove device from group
    RemoveDevice {
        /// Group name or ID
        group: String,
        /// Device name or IP
        device: String,
    },

    /// Add tag to group
    AddTag {
        /// Group name or ID
        group: String,
        /// Tag to add
        tag: String,
    },

    /// Remove tag from group
    RemoveTag {
        /// Group name or ID
        group: String,
        /// Tag to remove
        tag: String,
    },

    /// Update group details
    Update {
        /// Group name or ID
        group: String,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New description
        #[arg(long)]
        description: Option<String>,
    },

    /// Show group statistics
    Stats {
        /// Group name or ID
        group: String,
        /// Enable continuous monitoring
        #[arg(long, short)]
        watch: bool,
        /// Update interval in seconds (with --watch)
        #[arg(long, default_value = "30")]
        interval: u64,
    },
}

#[derive(Subcommand)]
pub enum BulkAction {
    /// Restart all devices in a group
    Restart {
        /// Group name or ID
        group: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Set fan speed for all devices in a group
    SetFanSpeed {
        /// Group name or ID
        group: String,
        /// Fan speed percentage (0-100)
        speed: u8,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Update settings for all devices in a group
    UpdateSettings {
        /// Group name or ID
        group: String,
        /// JSON string with settings to update
        settings: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Scan WiFi on all devices in a group
    WifiScan {
        /// Group name or ID
        group: String,
    },

    /// Update firmware on all devices in a group
    UpdateFirmware {
        /// Group name or ID
        group: String,
        /// Firmware URL or file path
        firmware: String,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Maximum parallel operations
        #[arg(long, default_value = "5")]
        parallel: usize,
    },

    /// Update AxeOS on all devices in a group
    UpdateAxeOs {
        /// Group name or ID
        group: String,
        /// AxeOS update URL or file path
        axeos: String,
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
            tracing_subscriber::fmt::init();
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
            Commands::List { all } => {
                handlers::list(all, self.format, !self.no_color, self.cache_dir.as_deref()).await
            }
            Commands::Stats {
                device,
                watch,
                interval,
            } => {
                handlers::stats(
                    device,
                    watch,
                    interval,
                    self.format,
                    !self.no_color,
                    self.cache_dir.as_deref(),
                )
                .await
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
                db,
                device_type,
                type_summary,
            } => {
                handlers::monitor(handlers::monitor::MonitorConfig {
                    interval,
                    temp_alert,
                    hashrate_alert,
                    db,
                    type_filter: device_type,
                    type_summary,
                    format: self.format,
                    color: !self.no_color,
                    cache_dir: self.cache_dir.as_deref(),
                })
                .await
            }
            Commands::Group { action } => {
                handlers::group(
                    action,
                    self.format,
                    !self.no_color,
                    self.cache_dir.as_deref(),
                )
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
