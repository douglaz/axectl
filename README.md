# axectl

> **CLI tool for managing Bitaxe and NerdQAxe miners**

`axectl` is a command-line interface for discovering, monitoring, and controlling Bitcoin ASIC miners running AxeOS firmware. Designed for automation, scripting, and efficient fleet management of home mining operations.

## 🚀 Quick Start

```bash
# Discover miners on your network
axectl discover

# List all known devices
axectl list

# Show real-time statistics
axectl stats

# Monitor continuously with alerts
axectl monitor --temp-alert 75 --hashrate-alert 10
```

## 📦 Installation

### Static Binary Downloads (Recommended)
Download pre-built static binaries from the [releases page](https://github.com/master/axectl/releases).

**Linux users**: Download the musl static binary - it works on any Linux distribution without dependencies:
```bash
# Download latest release (replace VERSION with actual version)
curl -L -o axectl https://github.com/master/axectl/releases/download/vVERSION/axectl-x86_64-linux-musl.tar.gz
tar -xzf axectl-x86_64-linux-musl.tar.gz
chmod +x axectl
./axectl --help
```

**Benefits of static binaries:**
- ✅ **Universal compatibility** - works on any Linux distribution
- ✅ **No dependencies** - single file that just works
- ✅ **Container-friendly** - perfect for Docker/containers
- ✅ **Security** - reduced attack surface with static linking

### Building from Source

#### Quick Build (Static Binary)
```bash
git clone https://github.com/master/axectl.git
cd axectl
cargo build --release
# Automatically builds static musl binary: ./target/x86_64-unknown-linux-musl/release/axectl
```

#### Using Nix (Recommended)
```bash
# Run directly from GitHub
nix run 'git+https://github.com/master/axectl.git'

# Or build locally with proper toolchain
git clone https://github.com/master/axectl.git
cd axectl
nix develop  # Enters environment with musl toolchain
cargo build --release  # Builds static binary by default
```

#### Development Builds
For faster development iterations or dynamic linking:
```bash
# Override default musl target for dynamic linking
cargo build --target x86_64-unknown-linux-gnu

# Or temporarily override in environment
CARGO_BUILD_TARGET=x86_64-unknown-linux-gnu cargo build
```

## 🎯 Why axectl?

### Unique Position in the Mining Ecosystem

The Bitcoin mining management landscape offers various solutions for different needs:

- **🔧 [bitaxetool](https://pypi.org/project/bitaxetool/)** - Python tool for flashing Bitaxe firmware
- **🌐 AxeOS Web Interface** - Built-in web management interface on each device
- **🏢 [Awesome Miner](https://www.awesomeminer.com/)** - Enterprise GUI for large mining operations (200,000+ miners)
- **📊 [Minerstat](https://minerstat.com/)** - Cloud-based mining farm management platform
- **⚙️ CGMiner/BFGMiner** - Traditional ASIC mining software with device control

**axectl fills the gap for:**
- **CLI-first approach** for automation and scripting
- **Zero-configuration discovery** - works out of the box
- **Dual output formats** (human-readable text + machine-readable JSON)
- **Native support** for both Bitaxe and NerdQAxe devices
- **Lightweight operations** - no web interfaces or heavy dependencies
- **Home mining focus** - optimized for 1-50 device deployments

## ✨ Features

### 🔍 Auto-Discovery
- **mDNS discovery** for AxeOS devices
- **Network scanning** with intelligent IP range detection
- **Device type detection** (Bitaxe vs NerdQAxe)
- **Optional caching** for faster subsequent scans

### 📊 Monitoring & Statistics
- **Real-time metrics**: hashrate, temperature, power consumption, fan speed
- **Continuous monitoring** with customizable alerts
- **Swarm summaries** for fleet-wide statistics
- **Historical tracking** with in-memory storage

### 🎛️ Device Control
- **Fan speed control** (0-100%)
- **System restart** commands
- **Settings updates** via JSON
- **WiFi network scanning**
- **OTA firmware updates**

### 🔧 Automation Ready
- **JSON output** for all commands (`--format json`)
- **Machine-readable data** for integration with monitoring systems
- **Scriptable interface** following Unix tool conventions
- **Error handling** with proper exit codes

### 🤖 MCP Server (Model Context Protocol)
axectl includes an MCP server that allows AI assistants to interact with your mining devices:

- **AI Integration**: Use with Claude, ChatGPT, or other AI assistants that support MCP
- **Full API Access**: All axectl functionality exposed through standardized protocol
- **Type-Safe Tools**: JSON Schema validation for all operations
- **Async Operations**: Efficient handling of multiple device operations

Start the MCP server:
```bash
# Run MCP server (uses stdio transport for AI assistant integration)
axectl mcp-server
```

Available MCP tools:
- `discover_devices` - Find miners on the network
- `list_devices` - List known devices with statistics
- `get_device_stats` - Get detailed device statistics
- `get_device_config` - Retrieve device configuration
- `set_fan_speed` - Control fan speed
- `restart_device` - Restart a device
- `update_settings` - Update device settings
- `wifi_scan` - Scan for WiFi networks
- `bulk_restart` - Restart multiple devices
- `bulk_set_fan_speed` - Set fan speed on multiple devices
- `bulk_update_bitcoin_address` - Update Bitcoin address on multiple devices

## 📖 Usage Guide

### Discovery

Find all miners on your network:

```bash
# Basic discovery (auto-detects network)
axectl discover

# Scan specific network range
axectl discover --network 192.168.1.0/24

# Fast discovery with cache
axectl discover --cache-dir ~/.axectl-cache

# JSON output for scripts
axectl discover --format json
```

### Device Management

```bash
# List all devices
axectl list

# Show detailed statistics
axectl stats

# Monitor specific device
axectl stats --device bitaxe-401

# Continuous monitoring with watch mode
axectl stats --watch --interval 30
```

### Fleet Monitoring

```bash
# Monitor all devices with temperature alerts
axectl monitor --temp-alert 75.0

# Comprehensive monitoring with hashrate drop detection
axectl monitor --temp-alert 75 --hashrate-alert 15 --interval 60

# Save monitoring data to JSON
axectl monitor --format json > monitoring_log.json
```

### Device Control

```bash
# Show current device configuration
axectl control bitaxe-401 show-config

# Set fan speed to 80%
axectl control bitaxe-401 set-fan-speed 80

# Restart a device
axectl control 192.168.1.100 restart

# Scan for WiFi networks
axectl control bitaxe-gamma wifi-scan

# Update device settings
axectl control bitaxe-401 update-settings '{"pool_url": "stratum+tcp://new.pool:4334"}'
```

### Bulk Operations

Manage multiple devices at once with bulk commands:

```bash
# View configuration for all devices
axectl bulk show-config --all

# View configuration for specific device types
axectl bulk show-config --device-type bitaxe-ultra

# View configuration for specific devices
axectl bulk show-config --ip-address 192.168.1.100 --ip-address 192.168.1.101

# Update bitcoin address across all devices (automatically appends hostname)
axectl bulk update-bitcoin-address bc1qnewaddress --all --force
# Result: Each device's pool_user becomes "bc1qnewaddress.hostname"

# Update bitcoin address for specific device type
axectl bulk update-bitcoin-address bc1qnewaddress --device-type bitaxe-gamma --force

# Check configuration before making bulk changes
axectl bulk show-config --device-type bitaxe-ultra
axectl bulk update-settings '{"frequency": 500}' --device-type bitaxe-ultra --force

# Restart all devices of a specific type
axectl bulk restart --device-type nerdqaxe-plus --force

# Set fan speed for multiple devices
axectl bulk set-fan-speed 80 --all --force

# Update firmware on all devices (with parallel execution)
axectl bulk update-firmware http://example.com/firmware.bin --all --parallel 5 --force
```

**Bitcoin Address Management:**
```bash
# The update-bitcoin-address command follows mining pool conventions
# It automatically appends the device hostname to your bitcoin address
# This ensures each device has a unique worker identifier

# Example: Update all devices to a new bitcoin address
axectl bulk update-bitcoin-address bc1qyouraddress --all --force

# Result for each device:
# Device "bitaxe" → pool_user = "bc1qyouraddress.bitaxe"
# Device "nerdqaxe4" → pool_user = "bc1qyouraddress.nerdqaxe4"
# Device "Bitaxe3" → pool_user = "bc1qyouraddress.Bitaxe3"

# This convention is standard for mining pools to track individual workers
```

**Configuration Management Workflow:**
```bash
# 1. First, check current settings across your fleet
axectl bulk show-config --device-type bitaxe-ultra

# 2. Review the configuration output
# 3. Make informed decisions about what to change
# 4. Apply updates with confidence
axectl bulk update-settings '{"pool_url": "stratum+tcp://new.pool:4334"}' --device-type bitaxe-ultra --force
```

## 🔧 Advanced Usage

### Caching for Performance

Enable caching to speed up discovery on subsequent runs:

```bash
# First run - discovers and caches devices
axectl discover --cache-dir ~/.axectl-cache

# Subsequent runs - uses cache for faster scanning
axectl discover --cache-dir ~/.axectl-cache --timeout 2
```

Cache automatically expires devices after 7 days and updates with newly discovered devices.

### JSON Integration

Perfect for monitoring systems, dashboards, and automation:

```bash
# Get device statistics as JSON
axectl stats --format json | jq '.statistics[0].hashrate_mhs'

# Monitor and log to file
axectl monitor --format json --interval 300 >> mining_log.jsonl

# Extract device IPs for other scripts
axectl list --format json | jq -r '.devices[].ip_address'
```

### Scripting Examples

**Health Check Script:**
```bash
#!/bin/bash
devices=$(axectl list --format json | jq -r '.devices[].ip_address')
for device in $devices; do
    temp=$(axectl stats --device $device --format json | jq '.statistics[0].temperature_celsius')
    if (( $(echo "$temp > 80" | bc -l) )); then
        echo "WARNING: $device temperature is ${temp}°C"
    fi
done
```

**Automated Discovery and Monitoring:**
```bash
#!/bin/bash
# Discover devices and start monitoring
axectl discover --cache-dir ~/.axectl-cache
axectl monitor --temp-alert 75 --hashrate-alert 20 --interval 120
```

## 🏗️ Architecture

axectl is built with modularity and performance in mind:

- **🦀 Rust** - Memory safe, fast, and reliable
- **⚡ Async/await** - Efficient network operations
- **🌐 HTTP/JSON** - Standard AxeOS API integration
- **🔍 mDNS** - Zero-configuration device discovery
- **💾 Optional caching** - Smart performance optimization
- **📊 Structured output** - JSON + human-readable formats

## 🤝 Comparison with Similar Tools

| Tool | Focus | Interface | Device Support | Use Case |
|------|-------|-----------|----------------|----------|
| **axectl** | CLI automation | Command-line | Bitaxe, NerdQAxe | Home mining automation |
| bitaxetool | Firmware flashing | CLI (Python) | Bitaxe only | Device setup/recovery |
| AxeOS WebUI | Device management | Web browser | Single device | Manual configuration |
| Awesome Miner | Enterprise mining | Desktop GUI | 200+ ASIC models | Large mining farms |
| Minerstat | Cloud management | Web dashboard | Multi-platform | Commercial operations |
| CGMiner | ASIC mining | CLI/config files | Generic ASICs | Traditional mining |

## 🛠️ Development

### Prerequisites

- Rust 1.70+ or Nix with flakes enabled
- Network access to AxeOS devices

### Building

```bash
# Clone repository
git clone https://github.com/master/axectl.git
cd axectl

# Build with Nix (recommended)
nix develop
cargo build --release

# Or build with Cargo directly
cargo build --release
```

### Testing

```bash
# Run tests
cargo test

# Test with real devices (requires network setup)
cargo run -- discover --timeout 10

# Lint and format
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

### Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes and add tests
4. Ensure all tests pass: `cargo test --all-targets --all-features`
5. Format code: `cargo fmt`
6. Check lints: `cargo clippy --all-targets --all-features -- -D warnings`
7. Commit and push changes
8. Open a Pull Request

## 📋 Roadmap

- [ ] **Device Groups** - Organize devices into logical groups
- [x] **Bulk Operations** - Apply settings to multiple devices
- [x] **Configuration Viewing** - View current device configuration before changes
- [ ] **Configuration Profiles** - Save and apply device configurations
- [ ] **Alerting Integration** - Webhook/email notifications
- [ ] **Historical Data** - SQLite storage for long-term analytics
- [ ] **Pool Management** - Switch pools across multiple devices
- [ ] **Overclocking Profiles** - Safe overclocking with automatic rollback

## 📄 License

MIT License - see [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- **Bitaxe Team** - For creating open-source Bitcoin mining hardware
- **NerdQAxe Team** - For extending the Bitaxe ecosystem
- **AxeOS Contributors** - For the robust firmware and API
- **Rust Community** - For excellent async networking libraries

## 🆘 Support

- **Issues**: [GitHub Issues](https://github.com/master/axectl/issues)
- **Discussions**: [GitHub Discussions](https://github.com/master/axectl/discussions)
- **Discord**: Join the [Bitaxe Community](https://discord.gg/3E8ca2dkcC)

---

**Happy Mining! ⛏️**