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
# Produces static binary: ./target/x86_64-unknown-linux-musl/release/axectl
```

#### Using Nix
```bash
# Run directly from GitHub
nix run 'git+https://github.com/master/axectl.git'

# Or build locally
git clone https://github.com/master/axectl.git
cd axectl
nix develop
cargo build --release
```

#### Development Builds
For faster development iterations (non-static):
```bash
cargo build --target x86_64-unknown-linux-gnu  # Dynamic linking for faster builds
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
# Set fan speed to 80%
axectl control bitaxe-401 set-fan-speed 80

# Restart a device
axectl control 192.168.1.100 restart

# Scan for WiFi networks
axectl control bitaxe-gamma wifi-scan

# Update device settings
axectl control bitaxe-401 update-settings '{"pool_url": "stratum+tcp://new.pool:4334"}'
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
- [ ] **Bulk Operations** - Apply settings to multiple devices
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