use anyhow::Result;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::ServerHandler,
    model::{
        CallToolResult, Content, ListToolsResult, PaginatedRequestParam, ServerCapabilities,
        ServerInfo, Tool,
    },
    schemars,
    service::{RequestContext, ServiceExt},
    tool,
    transport::stdio,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::api::client::AxeOsClient;
use crate::api::models::DeviceFilter;
use crate::cache::{DeviceCache, get_cache_dir};

/// Configuration for the MCP server
#[derive(Debug, Clone, Default)]
pub struct McpServerConfig {
    pub cache_dir: Option<std::path::PathBuf>,
}

/// The main MCP server for axectl
#[derive(Clone)]
pub struct AxectlMcpServer {
    config: McpServerConfig,
}

// Request structures for tools
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DiscoverDevicesRequest {
    #[schemars(description = "Network range to scan (auto-detected if not specified)")]
    pub network: Option<String>,
    #[schemars(description = "Discovery timeout in seconds")]
    pub timeout: Option<u64>,
    #[schemars(description = "Enable mDNS discovery")]
    pub use_mdns: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListDevicesRequest {
    #[schemars(description = "Include offline devices")]
    pub all: Option<bool>,
    #[schemars(description = "Skip fetching live statistics")]
    pub no_stats: Option<bool>,
    #[schemars(description = "Filter by device type (e.g., bitaxe-gamma, nerdqaxe-plus)")]
    pub device_type: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetDeviceStatsRequest {
    #[schemars(description = "Device name or IP address")]
    pub device: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetDeviceConfigRequest {
    #[schemars(description = "Device name or IP address")]
    pub device: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetFanSpeedRequest {
    #[schemars(description = "Device name or IP address")]
    pub device: String,
    #[schemars(description = "Fan speed percentage (0-100)")]
    pub speed: u8,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RestartDeviceRequest {
    #[schemars(description = "Device name or IP address")]
    pub device: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateSettingsRequest {
    #[schemars(description = "Device name or IP address")]
    pub device: String,
    #[schemars(description = "Settings to update as JSON object")]
    pub settings: serde_json::Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct WifiScanRequest {
    #[schemars(description = "Device name or IP address")]
    pub device: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BulkRestartRequest {
    #[schemars(description = "Device type filter (e.g., bitaxe-gamma)")]
    pub device_type: Option<String>,
    #[schemars(description = "List of IP addresses")]
    pub ip_addresses: Option<Vec<String>>,
    #[schemars(description = "Target all devices")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BulkSetFanSpeedRequest {
    #[schemars(description = "Fan speed percentage (0-100)")]
    pub speed: u8,
    #[schemars(description = "Device type filter")]
    pub device_type: Option<String>,
    #[schemars(description = "List of IP addresses")]
    pub ip_addresses: Option<Vec<String>>,
    #[schemars(description = "Target all devices")]
    pub all: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct BulkUpdateBitcoinAddressRequest {
    #[schemars(description = "Bitcoin address (hostname will be appended automatically)")]
    pub bitcoin_address: String,
    #[schemars(description = "Device type filter")]
    pub device_type: Option<String>,
    #[schemars(description = "List of IP addresses")]
    pub ip_addresses: Option<Vec<String>>,
    #[schemars(description = "Target all devices")]
    pub all: Option<bool>,
}

/// Start the MCP server
pub async fn start_mcp_server(config: McpServerConfig) -> Result<()> {
    let server = AxectlMcpServer::new(config);
    server.run().await
}

impl AxectlMcpServer {
    pub fn new(config: McpServerConfig) -> Self {
        Self { config }
    }

    /// Start the MCP server
    pub async fn run(self) -> Result<()> {
        // Initialize tracing only if not in test mode
        let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
        if log_level != "error" {
            info!("Starting axectl MCP server with stdio transport");
        }

        let service = self.serve(stdio()).await?;
        service.waiting().await?;

        Ok(())
    }

    // Helper method to get device from cache
    #[allow(dead_code)] // Actually used by multiple tool implementations but compiler doesn't detect it
    async fn get_device_from_cache(&self, device_id: &str) -> Result<crate::api::models::Device> {
        let cache_dir = get_cache_dir(self.config.cache_dir.as_deref())?;
        let cache = DeviceCache::load(cache_dir.as_ref()).unwrap_or_default();

        cache
            .find_device(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device not found: {}", device_id))
    }
}

// Implement tool methods
impl AxectlMcpServer {
    #[tool(description = "Discover mining devices on the network")]
    async fn discover_devices(
        &self,
        DiscoverDevicesRequest {
            network,
            timeout,
            use_mdns,
        }: DiscoverDevicesRequest,
    ) -> CallToolResult {
        let timeout_secs = timeout.unwrap_or(5);
        let use_mdns = use_mdns.unwrap_or(true);

        // Use the perform_discovery function from the handlers
        match crate::cli::commands::handlers::discovery::perform_discovery(
            network,
            timeout_secs,
            use_mdns,
            self.config.cache_dir.as_deref(),
            false, // color not needed for MCP
        )
        .await
        {
            Ok(devices) => {
                let result = serde_json::json!({
                    "devices": devices,
                    "count": devices.len(),
                });
                CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
                )])
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("Discovery failed: {e}"))]),
        }
    }

    #[tool(description = "List known devices with optional statistics")]
    async fn list_devices(
        &self,
        ListDevicesRequest {
            all,
            no_stats,
            device_type,
        }: ListDevicesRequest,
    ) -> CallToolResult {
        let cache_dir = match get_cache_dir(self.config.cache_dir.as_deref()) {
            Ok(dir) => dir,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error getting cache dir: {e}"
                ))]);
            }
        };

        let cache = DeviceCache::load(cache_dir.as_ref()).unwrap_or_default();

        // Get devices based on filter
        let mut devices = if let Some(filter_type) = device_type {
            if let Ok(device_filter) = DeviceFilter::from_str(&filter_type) {
                if all.unwrap_or(false) {
                    cache.get_devices_by_filter(device_filter)
                } else {
                    cache.get_online_devices_by_filter(device_filter)
                }
            } else {
                cache.get_all_devices()
            }
        } else if all.unwrap_or(false) {
            cache.get_all_devices()
        } else {
            cache.get_devices_by_status(crate::api::models::DeviceStatus::Online)
        };

        // Fetch stats if not skipped
        if !no_stats.unwrap_or(false) {
            for device in &mut devices {
                if let Ok(client) = AxeOsClient::new(&device.ip_address)
                    && let Ok(_stats) = client.get_system_stats().await
                {
                    // Update device with latest stats
                    device.last_seen = chrono::Utc::now();
                }
            }
        }

        let result = serde_json::json!({
            "devices": devices,
            "count": devices.len(),
            "cached": true,
        });

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
        )])
    }

    #[tool(description = "Get statistics for a specific device")]
    async fn get_device_stats(
        &self,
        GetDeviceStatsRequest { device }: GetDeviceStatsRequest,
    ) -> CallToolResult {
        let device_info = match self.get_device_from_cache(&device).await {
            Ok(d) => d,
            Err(e) => return CallToolResult::error(vec![Content::text(format!("Error: {e}"))]),
        };

        let client = match AxeOsClient::new(&device_info.ip_address) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error creating client: {e}"
                ))]);
            }
        };

        match client.get_system_stats().await {
            Ok(stats) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&stats).unwrap_or_else(|e| e.to_string()),
            )]),
            Err(e) => {
                CallToolResult::error(vec![Content::text(format!("Failed to get stats: {e}"))])
            }
        }
    }

    #[tool(description = "Get configuration for a specific device")]
    async fn get_device_config(
        &self,
        GetDeviceConfigRequest { device }: GetDeviceConfigRequest,
    ) -> CallToolResult {
        let device_info = match self.get_device_from_cache(&device).await {
            Ok(d) => d,
            Err(e) => return CallToolResult::error(vec![Content::text(format!("Error: {e}"))]),
        };

        let client = match AxeOsClient::new(&device_info.ip_address) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error creating client: {e}"
                ))]);
            }
        };

        match client.get_system_info().await {
            Ok(info) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&info).unwrap_or_else(|e| e.to_string()),
            )]),
            Err(e) => {
                CallToolResult::error(vec![Content::text(format!("Failed to get config: {e}"))])
            }
        }
    }

    #[tool(description = "Set fan speed for a device")]
    async fn set_fan_speed(
        &self,
        SetFanSpeedRequest { device, speed }: SetFanSpeedRequest,
    ) -> CallToolResult {
        if speed > 100 {
            return CallToolResult::error(vec![Content::text(
                "Fan speed must be between 0 and 100",
            )]);
        }

        let device_info = match self.get_device_from_cache(&device).await {
            Ok(d) => d,
            Err(e) => return CallToolResult::error(vec![Content::text(format!("Error: {e}"))]),
        };

        let client = match AxeOsClient::new(&device_info.ip_address) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error creating client: {e}"
                ))]);
            }
        };

        match client.set_fan_speed(speed).await {
            Ok(result) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
            )]),
            Err(e) => {
                CallToolResult::error(vec![Content::text(format!("Failed to set fan speed: {e}"))])
            }
        }
    }

    #[tool(description = "Restart a device")]
    async fn restart_device(
        &self,
        RestartDeviceRequest { device }: RestartDeviceRequest,
    ) -> CallToolResult {
        let device_info = match self.get_device_from_cache(&device).await {
            Ok(d) => d,
            Err(e) => return CallToolResult::error(vec![Content::text(format!("Error: {e}"))]),
        };

        let client = match AxeOsClient::new(&device_info.ip_address) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error creating client: {e}"
                ))]);
            }
        };

        match client.restart_system().await {
            Ok(result) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to restart device: {e}"
            ))]),
        }
    }

    #[tool(description = "Update device settings")]
    async fn update_settings(
        &self,
        UpdateSettingsRequest { device, settings }: UpdateSettingsRequest,
    ) -> CallToolResult {
        let device_info = match self.get_device_from_cache(&device).await {
            Ok(d) => d,
            Err(e) => return CallToolResult::error(vec![Content::text(format!("Error: {e}"))]),
        };

        let client = match AxeOsClient::new(&device_info.ip_address) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error creating client: {e}"
                ))]);
            }
        };

        // Parse settings into SystemUpdateRequest
        let update_request: crate::api::models::SystemUpdateRequest =
            match serde_json::from_value(settings) {
                Ok(req) => req,
                Err(e) => {
                    return CallToolResult::error(vec![Content::text(format!(
                        "Invalid settings format: {e}"
                    ))]);
                }
            };

        match client.update_system(update_request).await {
            Ok(result) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
            )]),
            Err(e) => CallToolResult::error(vec![Content::text(format!(
                "Failed to update settings: {e}"
            ))]),
        }
    }

    #[tool(description = "Scan for WiFi networks on a device")]
    async fn wifi_scan(&self, WifiScanRequest { device }: WifiScanRequest) -> CallToolResult {
        let device_info = match self.get_device_from_cache(&device).await {
            Ok(d) => d,
            Err(e) => return CallToolResult::error(vec![Content::text(format!("Error: {e}"))]),
        };

        let client = match AxeOsClient::new(&device_info.ip_address) {
            Ok(c) => c,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error creating client: {e}"
                ))]);
            }
        };

        match client.scan_wifi().await {
            Ok(result) => CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&result).unwrap_or_else(|e| e.to_string()),
            )]),
            Err(e) => {
                CallToolResult::error(vec![Content::text(format!("Failed to scan WiFi: {e}"))])
            }
        }
    }

    #[tool(description = "Restart multiple devices")]
    async fn bulk_restart(
        &self,
        BulkRestartRequest {
            device_type,
            ip_addresses,
            all,
        }: BulkRestartRequest,
    ) -> CallToolResult {
        let cache_dir = match get_cache_dir(self.config.cache_dir.as_deref()) {
            Ok(dir) => dir,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error getting cache dir: {e}"
                ))]);
            }
        };

        let cache = DeviceCache::load(cache_dir.as_ref()).unwrap_or_default();
        let mut target_devices = cache.get_all_devices();

        // Apply filters
        if let Some(ref filter_type) = device_type
            && let Ok(device_filter) = DeviceFilter::from_str(filter_type)
        {
            target_devices.retain(|d| device_filter.matches(d.device_type));
        }

        if let Some(ref ips) = ip_addresses {
            target_devices.retain(|d| ips.contains(&d.ip_address));
        }

        if !all.unwrap_or(false) && device_type.is_none() && ip_addresses.is_none() {
            return CallToolResult::error(vec![Content::text(
                "Must specify --all, --device-type, or --ip-address",
            )]);
        }

        let mut results = Vec::new();
        for device in target_devices {
            let client = match AxeOsClient::new(&device.ip_address) {
                Ok(c) => c,
                Err(e) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": false,
                        "error": e.to_string(),
                    }));
                    continue;
                }
            };

            match client.restart_system().await {
                Ok(_) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": true,
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": false,
                        "error": e.to_string(),
                    }));
                }
            }
        }

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&serde_json::json!({
                "results": results,
                "total": results.len(),
            }))
            .unwrap_or_else(|e| e.to_string()),
        )])
    }

    #[tool(description = "Set fan speed on multiple devices")]
    async fn bulk_set_fan_speed(
        &self,
        BulkSetFanSpeedRequest {
            speed,
            device_type,
            ip_addresses,
            all,
        }: BulkSetFanSpeedRequest,
    ) -> CallToolResult {
        if speed > 100 {
            return CallToolResult::error(vec![Content::text(
                "Fan speed must be between 0 and 100",
            )]);
        }

        let cache_dir = match get_cache_dir(self.config.cache_dir.as_deref()) {
            Ok(dir) => dir,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error getting cache dir: {e}"
                ))]);
            }
        };

        let cache = DeviceCache::load(cache_dir.as_ref()).unwrap_or_default();
        let mut target_devices = cache.get_all_devices();

        // Apply filters
        if let Some(ref filter_type) = device_type
            && let Ok(device_filter) = DeviceFilter::from_str(filter_type)
        {
            target_devices.retain(|d| device_filter.matches(d.device_type));
        }

        if let Some(ref ips) = ip_addresses {
            target_devices.retain(|d| ips.contains(&d.ip_address));
        }

        if !all.unwrap_or(false) && device_type.is_none() && ip_addresses.is_none() {
            return CallToolResult::error(vec![Content::text(
                "Must specify --all, --device-type, or --ip-address",
            )]);
        }

        let mut results = Vec::new();
        for device in target_devices {
            let client = match AxeOsClient::new(&device.ip_address) {
                Ok(c) => c,
                Err(e) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": false,
                        "error": e.to_string(),
                    }));
                    continue;
                }
            };

            match client.set_fan_speed(speed).await {
                Ok(_) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": true,
                        "fan_speed": speed,
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": false,
                        "error": e.to_string(),
                    }));
                }
            }
        }

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&serde_json::json!({
                "results": results,
                "total": results.len(),
            }))
            .unwrap_or_else(|e| e.to_string()),
        )])
    }

    #[tool(
        description = "Update bitcoin address on multiple devices (appends hostname automatically)"
    )]
    async fn bulk_update_bitcoin_address(
        &self,
        BulkUpdateBitcoinAddressRequest {
            bitcoin_address,
            device_type,
            ip_addresses,
            all,
        }: BulkUpdateBitcoinAddressRequest,
    ) -> CallToolResult {
        let cache_dir = match get_cache_dir(self.config.cache_dir.as_deref()) {
            Ok(dir) => dir,
            Err(e) => {
                return CallToolResult::error(vec![Content::text(format!(
                    "Error getting cache dir: {e}"
                ))]);
            }
        };

        let cache = DeviceCache::load(cache_dir.as_ref()).unwrap_or_default();
        let mut target_devices = cache.get_all_devices();

        // Apply filters
        if let Some(ref filter_type) = device_type
            && let Ok(device_filter) = DeviceFilter::from_str(filter_type)
        {
            target_devices.retain(|d| device_filter.matches(d.device_type));
        }

        if let Some(ref ips) = ip_addresses {
            target_devices.retain(|d| ips.contains(&d.ip_address));
        }

        if !all.unwrap_or(false) && device_type.is_none() && ip_addresses.is_none() {
            return CallToolResult::error(vec![Content::text(
                "Must specify --all, --device-type, or --ip-address",
            )]);
        }

        let mut results = Vec::new();
        for device in target_devices {
            let client = match AxeOsClient::new(&device.ip_address) {
                Ok(c) => c,
                Err(e) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": false,
                        "error": e.to_string(),
                    }));
                    continue;
                }
            };

            // Get device's hostname
            match client.get_system_info().await {
                Ok(system_info) => {
                    // Construct pool_user with bitcoin_address.hostname format
                    let pool_user = format!("{}.{}", bitcoin_address, system_info.hostname);

                    // Create update request with only pool_user field
                    let update_request = crate::api::models::SystemUpdateRequest {
                        pool_user: Some(pool_user.clone()),
                        ..Default::default()
                    };

                    match client.update_system(update_request).await {
                        Ok(_) => {
                            results.push(serde_json::json!({
                                "device": device.name,
                                "success": true,
                                "pool_user": pool_user,
                            }));
                        }
                        Err(e) => {
                            results.push(serde_json::json!({
                                "device": device.name,
                                "success": false,
                                "error": e.to_string(),
                            }));
                        }
                    }
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "device": device.name,
                        "success": false,
                        "error": format!("Failed to get device info: {}", e),
                    }));
                }
            }
        }

        CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&serde_json::json!({
                "results": results,
                "total": results.len(),
                "bitcoin_address": bitcoin_address,
            }))
            .unwrap_or_else(|e| e.to_string()),
        )])
    }
}

// Helper function to create a Tool with proper types
fn create_tool(name: &'static str, description: &'static str, schema: serde_json::Value) -> Tool {
    Tool {
        name: name.into(),
        description: Some(description.into()),
        input_schema: Arc::new(schema.as_object().unwrap().clone()),
        output_schema: None,
        annotations: None,
    }
}

// Implement ServerHandler trait
impl ServerHandler for AxectlMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = vec![
            create_tool(
                "discover_devices",
                "Discover mining devices on the network",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "network": {
                            "type": "string",
                            "description": "Network range to scan (auto-detected if not specified)"
                        },
                        "timeout": {
                            "type": "integer",
                            "description": "Discovery timeout in seconds"
                        },
                        "use_mdns": {
                            "type": "boolean",
                            "description": "Enable mDNS discovery"
                        }
                    }
                }),
            ),
            create_tool(
                "list_devices",
                "List known devices with optional statistics",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "all": {
                            "type": "boolean",
                            "description": "Include offline devices"
                        },
                        "no_stats": {
                            "type": "boolean",
                            "description": "Skip fetching live statistics"
                        },
                        "device_type": {
                            "type": "string",
                            "description": "Filter by device type (e.g., bitaxe-gamma, nerdqaxe-plus)"
                        }
                    }
                }),
            ),
            create_tool(
                "get_device_stats",
                "Get statistics for a specific device",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device name or IP address"
                        }
                    },
                    "required": ["device"]
                }),
            ),
            create_tool(
                "get_device_config",
                "Get configuration for a specific device",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device name or IP address"
                        }
                    },
                    "required": ["device"]
                }),
            ),
            create_tool(
                "set_fan_speed",
                "Set fan speed for a device",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device name or IP address"
                        },
                        "speed": {
                            "type": "integer",
                            "description": "Fan speed percentage (0-100)"
                        }
                    },
                    "required": ["device", "speed"]
                }),
            ),
            create_tool(
                "restart_device",
                "Restart a device",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device name or IP address"
                        }
                    },
                    "required": ["device"]
                }),
            ),
            create_tool(
                "update_settings",
                "Update device settings",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device name or IP address"
                        },
                        "settings": {
                            "type": "object",
                            "description": "Settings to update as JSON object"
                        }
                    },
                    "required": ["device", "settings"]
                }),
            ),
            create_tool(
                "wifi_scan",
                "Scan for WiFi networks on a device",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device": {
                            "type": "string",
                            "description": "Device name or IP address"
                        }
                    },
                    "required": ["device"]
                }),
            ),
            create_tool(
                "bulk_restart",
                "Restart multiple devices",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "device_type": {
                            "type": "string",
                            "description": "Device type filter (e.g., bitaxe-gamma)"
                        },
                        "ip_addresses": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of IP addresses"
                        },
                        "all": {
                            "type": "boolean",
                            "description": "Target all devices"
                        }
                    }
                }),
            ),
            create_tool(
                "bulk_set_fan_speed",
                "Set fan speed on multiple devices",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "speed": {
                            "type": "integer",
                            "description": "Fan speed percentage (0-100)"
                        },
                        "device_type": {
                            "type": "string",
                            "description": "Device type filter"
                        },
                        "ip_addresses": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of IP addresses"
                        },
                        "all": {
                            "type": "boolean",
                            "description": "Target all devices"
                        }
                    },
                    "required": ["speed"]
                }),
            ),
            create_tool(
                "bulk_update_bitcoin_address",
                "Update bitcoin address on multiple devices (appends hostname automatically)",
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bitcoin_address": {
                            "type": "string",
                            "description": "Bitcoin address (hostname will be appended automatically)"
                        },
                        "device_type": {
                            "type": "string",
                            "description": "Device type filter"
                        },
                        "ip_addresses": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "List of IP addresses"
                        },
                        "all": {
                            "type": "boolean",
                            "description": "Target all devices"
                        }
                    },
                    "required": ["bitcoin_address"]
                }),
            ),
        ];

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    // The call_tool method is automatically generated by the #[tool] macros
}

// DeviceFilter already has FromStr implementation in api::models
use std::str::FromStr;
