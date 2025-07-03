# Bits-Link

[![🚧 Development Status](https://img.shields.io/badge/Status-In%20Development-orange?style=for-the-badge)](#development-status)
[![Rust](https://img.shields.io/badge/Language-Rust-red?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](#license)

> **Advanced NAT traversal System with Intelligent Route Selection**

Bits-Link is a next-generation NAT traversal system built with Rust, designed to overcome the challenges of packet loss and network congestion in cross-border or unreliable network environments. Unlike traditional solutions, Bits-Link features intelligent path selection, multi-mode architecture, and fine-grained access control to ensure optimal performance and high availability.

## ✨ Key Features

### 🧠 Intelligent Route Selection
- **Adaptive Path Selection**: Automatically chooses between direct and relay connections based on real-time network conditions
- **Multi-Path Monitoring**: Continuous health checks with latency and success rate analysis
- **Smooth Failover**: Seamless switching between connection paths without service interruption

### 🏗️ Multi-Mode Architecture
- **Direct Mode**: Low-latency client-to-server connections for optimal performance
- **Relay Mode**: Server-Relay-Client architecture optimized for cross-border access
- **Hybrid Mode**: Intelligent switching between direct and relay modes based on traffic patterns

### 🔐 Enterprise-Grade Security
- **Per-Website API Keys**: Granular access control with independent authentication
- **TLS 1.3 Encryption**: End-to-end encrypted communication with certificate validation
- **Token-Based Authentication**: Multi-layer security with dynamic session tokens

### ⚡ High Performance
- **Rust-Powered**: Memory-safe and blazingly fast performance
- **Connection Pooling**: Efficient multi-path connection management
- **Traffic Optimization**: Smart buffering and protocol optimization in relay mode

## 🏛️ System Architecture

```
Internet ←→ Server ←Cloud Network→ Relay ←→ Client ←→ Internal Services
    ↑         ↑      ↘            ↑     ↗    ↑
   Users    Server         Relay Node        Client
             ↑                               ↑
             └─────────Direct Path───────────┘
```

### Components

- **🖥️ bits-link-server**: Central control plane for configuration management, authentication, and traffic routing
- **🔄 bits-link-relay**: Smart relay nodes providing traffic forwarding and optimization across regions  
- **📱 bits-link-client**: Local client deployed in private networks, interfacing with internal services
- **🛠️ bits-link-common**: Shared library containing protocol definitions, encryption, and serialization

## 🚀 Quick Start

> **⚠️ Development Notice**: Bits-Link is currently under active development. The following instructions will be available upon the first stable release.

### Prerequisites
- Rust 1.70+ 
- OpenSSL development libraries

### Installation
```bash
# Clone the repository
git clone https://github.com/fslongjin/bits-link.git
cd bits-link

# Build all components
cargo build --release

# Run components (configurations required)
./target/release/bits-link-server --config server.toml
./target/release/bits-link-relay --config relay.toml  
./target/release/bits-link-client --config client.toml
```

### Configuration

**Server Configuration** (`server.toml`):
```toml
[server]
listen_addr = "0.0.0.0:7000"

[server.tls]
enable = true
cert_file = "server.crt" 
key_file = "server.key"

[relays.relay-us-west]
token = "relay_token_west_xxx"
region = "us-west"
addr = "relay.us-west.example.com:7001"

[websites.app1]
api_key = "ak_app1_xxx"
domain = "app1.example.com"
relay_server = "relay-us-west"
```

**Client Configuration** (`client.toml`):
```toml
[client]
api_key = "ak_app1_xxx"
website_name = "app1"

[server]
addr = "server.example.com:7000"

[routing]
mode = "auto"  # direct, relay, or auto

[routing.auto]
prefer_direct = true
latency_threshold_ms = 200
success_rate_threshold = 0.95
```

## 📖 Documentation

Comprehensive documentation is available in the [`docs/`](./docs/) directory:

- [📋 Product Requirements Document](./docs/prd.md) - Detailed technical specifications and system design
- [🏗️ Architecture Guide](./docs/architecture.md) *(Coming Soon)*
- [⚙️ Configuration Reference](./docs/configuration.md) *(Coming Soon)*
- [🔌 API Documentation](./docs/api.md) *(Coming Soon)*

## 🛣️ Roadmap

### Phase 1: Core Infrastructure (July 2025)
- [ ] Basic server-client architecture
- [ ] TLS encryption and authentication
- [ ] Configuration management system
- [ ] Protocol definition and message handling

### Phase 2: Relay System (July 2025)  
- [ ] Relay server implementation
- [ ] Multi-region relay support
- [ ] Traffic forwarding and optimization
- [ ] Health monitoring and reporting

### Phase 3: Intelligent Routing (August 2025)
- [ ] Path quality assessment algorithms
- [ ] Intelligent route selection
- [ ] Automatic failover mechanisms
- [ ] Performance optimization

### Phase 4: Enterprise Features (Q4 2025)
- [ ] Web-based management interface
- [ ] Advanced monitoring and alerting
- [ ] Load balancing and scaling
- [ ] Plugin system and extensibility

## 🤝 Contributing

We welcome contributions from the community! Please read our [Contributing Guidelines](CONTRIBUTING.md) before submitting pull requests.

### Development Setup
```bash
# Clone and setup development environment
git clone https://github.com/fslongjin/bits-link.git
cd bits-link

# Install development dependencies
cargo install cargo-watch cargo-audit

# Run tests
cargo test

# Run with hot reload (development)
cargo watch -x run
```

### Code Standards
- Follow Rust official style guidelines
- Ensure all tests pass: `cargo test`
- Run security audit: `cargo audit`
- Use conventional commit messages

## 🔧 Development Status

**🚧 This project is currently in active development.**

- ✅ Project architecture and specification completed
- 🔄 Core components implementation in progress  
- ⏳ Testing and integration pending
- ⏳ Documentation and examples pending

**Expected Timeline**: First stable release targeting Q3 2025.

For development updates and discussions, please check our [Issues](https://github.com/fslongjin/bits-link/issues) and [Discussions](https://github.com/fslongjin/bits-link/discussions) sections.

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🌟 Why Bits-Link?

Existing network penetration solutions often struggle with:
- ❌ Poor performance in high-latency, lossy network environments
- ❌ Lack of intelligent routing capabilities  
- ❌ Limited scalability and configuration flexibility
- ❌ Insufficient security controls for enterprise use

Bits-Link addresses these challenges with:
- ✅ **Intelligent Route Selection**: Adapts to network conditions in real-time
- ✅ **Multi-Mode Architecture**: Optimizes for both performance and reliability
- ✅ **Enterprise Security**: Fine-grained access control and encryption
- ✅ **Modern Rust Implementation**: Memory safety and high performance

---

<div align="center">

**Built with ❤️ using Rust**

[Documentation](./docs/) • [Issues](https://github.com/fslongjin/bits-link/issues) • [Discussions](https://github.com/fslongjin/bits-link/discussions)

</div> 