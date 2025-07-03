# Bits-Link 内网穿透系统 PRD

## 1. 产品概述

Bits-Link 是一个基于 Rust 开发的内网穿透系统，类似于 FRP，但针对client-Server之间存在丢包&拥塞的网络环境进行了优化。系统支持中转模式和直连模式，并提供智能选路功能，能够根据网络状况自动选择最优连接路径，确保服务的高可用性和最佳性能。通过引入中转服务器（Relay）解决境内外网络连接质量问题，支持多网站精细化管理和安全访问控制。

### 1.1 核心特性

- **多模式架构**：支持 Server-Relay-Client 中继模式和 Server-Client 直连模式
- **智能选路**：客户端可配置路由策略，支持直连、中继或混合模式
- **精细化权限管理**：每个网站使用独立的 API 密钥
- **灵活的路由配置**：支持为不同网站指定不同的连接路径
- **配置文件驱动**：无需数据库，基于 TOML 配置文件管理
- **安全可靠**：Token 认证机制，确保只转发授权网站流量

## 2. 系统架构

### 2.1 整体架构图

```
Internet ←→ Server ←云联网→ Relay ←→ Client ←→ Internal Services
    ↑         ↑      ↘      ↑     ↗    ↑
   用户      服务端         中转节点    客户端
             ↑                         ↑
             └─────────直连路径─────────┘
```

**支持两种连接模式：**
- **中继模式**：Client ←→ Relay ←→ Server（优化跨境访问）
- **直连模式**：Client ←→ Server（低延迟访问）
- **智能选路**：Client 根据配置智能选择连接路径

### 2.2 组件说明

- **bits-link-server**：服务端，负责配置管理、认证、流量路由
- **bits-link-relay**：中转服务器，负责流量转发和缓冲
- **bits-link-client**：客户端，部署在内网，对接本地服务
- **bits-link-common**：公共库，包含协议定义、加密、序列化等

## 3. 功能详细设计

### 3.1 服务端 (bits-link-server)

#### 核心功能
1. **配置管理**
   - 维护所有网站配置信息
   - 动态下发配置到 Relay 和 Client
   - 支持配置热更新

2. **认证服务**
   - Relay 服务器 Token 认证
   - Client API 密钥管理
   - 网站访问权限控制

3. **路由管理**
   - 为每个网站指定中转服务器
   - 负载均衡策略
   - 故障切换机制

#### 配置文件结构
```toml
# server.toml
[server]
listen_addr = "0.0.0.0:7000"

[server.tls]
enable = true
cert_file = "server.crt"
key_file = "server.key"

# 中转服务器管理
[relays.relay-cn-beijing]
token = "relay_token_beijing_xxx"
region = "cn-beijing"
addr = "relay.cn-beijing.example.com:7001"

[relays.relay-cn-shanghai]
token = "relay_token_shanghai_xxx"
region = "cn-shanghai"
addr = "relay.cn-shanghai.example.com:7001"

# 网站配置
[websites.website1]
api_key = "ak_website1_xxx"
domain = "app1.example.com"
relay_server = "relay-cn-beijing"  # 可选，不配置则支持直连

[websites.website1.client_config]
local_ip = "127.0.0.1"
local_port = 8080
protocol = "http"

[websites.website2]
api_key = "ak_website2_xxx"
domain = "app2.example.com"
relay_server = "relay-cn-shanghai"

[websites.website2.client_config]
local_ip = "127.0.0.1"
local_port = 8081
protocol = "http"

# 支持直连的网站配置示例
[websites.website3]
api_key = "ak_website3_xxx"
domain = "app3.example.com"
# 不配置 relay_server，表示支持直连

[websites.website3.client_config]
local_ip = "127.0.0.1"
local_port = 8082
protocol = "http"
```

### 3.2 中转服务器 (bits-link-relay)

#### 核心功能
1. **注册认证**
   - 使用 Token 向 Server 注册
   - 定期心跳保持连接
   - 接收配置更新

2. **流量转发**
   - 只转发已配置网站的流量
   - 支持多种协议 (HTTP/HTTPS/TCP)
   - 流量缓冲和优化

3. **监控报告**
   - 连接状态监控
   - 流量统计报告
   - 异常情况告警

#### 配置文件结构
```toml
# relay.toml
[relay]
name = "relay-cn-beijing"
token = "relay_token_beijing_xxx"
region = "cn-beijing"
listen_addr = "0.0.0.0:7001"

[server]
addr = "server.example.com:7000"

[server.tls]
enable = true
ca_file = "ca.crt"

# 动态配置，由 Server 下发
# websites 配置初始为空，由 Server 下发
```

### 3.3 客户端 (bits-link-client)

#### 核心功能
1. **服务发现**
   - 使用 API 密钥连接指定网站配置
   - 获取中转服务器信息和直连配置
   - 根据路由策略建立连接

2. **智能选路**
   - 支持直连、中继、混合三种模式
   - 可配置路由策略（全局或按请求路径）
   - 连接健康检测和自动切换
   - 延迟和成功率统计

3. **本地代理**
   - 监听本地服务
   - 协议适配 (HTTP/TCP)
   - 智能请求转发

4. **连接管理**
   - 多路径连接池管理
   - 断线重连
   - 健康检查和故障切换

#### 配置文件结构
```toml
# client.toml
[client]
api_key = "ak_website1_xxx"
website_name = "website1"

[server]
addr = "server.example.com:7000"

[server.tls]
enable = true
ca_file = "ca.crt"

[local_service]
addr = "127.0.0.1:8080"
protocol = "http"

# 路由策略配置（智能选路核心配置）
[routing]
# 路由模式: "direct"(仅直连), "relay"(仅中继), "auto"(智能选择)
mode = "auto"

# 直连配置
[routing.direct]
enable = true
timeout_ms = 3000        # 直连超时时间
retry_count = 2          # 重试次数

# 中继配置
[routing.relay]
enable = true
timeout_ms = 5000        # 中继连接超时时间
retry_count = 3          # 重试次数

# 智能选路配置
[routing.auto]
# 健康检查配置
health_check_interval = 30  # 健康检查间隔(秒)
failure_threshold = 3       # 连续失败阈值
recovery_threshold = 2      # 恢复检测阈值

# 路径选择策略
prefer_direct = true        # 优先直连
latency_threshold_ms = 200  # 延迟阈值，超过则切换
success_rate_threshold = 0.95  # 成功率阈值

# 按路径的路由策略（可选）
[[routing.path_rules]]
path_pattern = "/api/*"     # 路径匹配规则
mode = "direct"             # 该路径使用直连
priority = 1                # 优先级

[[routing.path_rules]]
path_pattern = "/upload/*"  # 文件上传路径
mode = "relay"              # 使用中继（更稳定）
priority = 2

# 动态配置，由 Server 下发
# relay 配置初始为空，由 Server 下发
```

### 3.4 智能选路算法设计

#### 路由决策算法
1. **初始化阶段**
   - 启动时同时测试直连和中继路径
   - 记录初始延迟和成功率基准

2. **运行时监控**
   ```rust
   // 路径质量评估
   struct PathQuality {
       latency_ms: u64,           // 平均延迟
       success_rate: f64,         // 成功率
       last_failure_time: u64,    // 上次失败时间
       consecutive_failures: u32,  // 连续失败次数
   }
   ```

3. **路径选择策略**
   - **优先级策略**：prefer_direct 配置优先直连
   - **阈值策略**：延迟超过阈值时切换
   - **可用性策略**：成功率低于阈值时切换
   - **故障恢复**：定期重试已失败路径

4. **路径切换逻辑**
   ```rust
   fn select_optimal_path(direct: &PathQuality, relay: &PathQuality) -> PathType {
       // 检查可用性
       if direct.success_rate < config.success_rate_threshold {
           return PathType::Relay;
       }
       if relay.success_rate < config.success_rate_threshold {
           return PathType::Direct;
       }
       
       // 检查延迟
       if config.prefer_direct && direct.latency_ms <= config.latency_threshold_ms {
           return PathType::Direct;
       }
       
       // 选择延迟更低的路径
       if direct.latency_ms < relay.latency_ms {
           PathType::Direct
       } else {
           PathType::Relay
       }
   }
   ```

#### 健康检查机制
- **主动探测**：定期发送 RouteTest 消息
- **被动监控**：统计正常请求的成功率和延迟
- **快速恢复**：失败路径每30秒重试一次
- **平滑切换**：避免路径抖动，设置切换冷却时间

## 4. 通信协议设计

### 4.1 消息类型定义

```rust
// bits-link-common/src/protocol.rs

#[derive(Serialize, Deserialize, Debug)]
pub enum MessageType {
    // 认证相关
    RelayRegister,      // Relay 注册
    ClientAuth,         // Client 认证
    
    // 配置相关
    ConfigSync,         // 配置同步
    ConfigUpdate,       // 配置更新
    
    // 数据传输
    DataForward,        // 数据转发
    DataResponse,       // 数据响应
    DirectConnect,      // 直连请求
    
    // 智能选路相关
    RouteTest,          // 路由测试
    RouteReport,        // 路由报告
    PathSwitch,         // 路径切换通知
    
    // 控制消息
    Heartbeat,          // 心跳
    Disconnect,         // 断开连接
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub msg_type: MessageType,
    pub msg_id: String,
    pub timestamp: u64,
    pub payload: Vec<u8>,
}
```

### 4.2 认证流程

1. **Relay 注册流程**
   ```
   Relay → Server: RelayRegister{token, region, capabilities}
   Server → Relay: ConfigSync{websites}
   ```

2. **Client 认证流程（中继模式）**
   ```
   Client → Server: ClientAuth{api_key, website_name}
   Server → Client: ConfigSync{relay_info, local_config}
   Client → Relay: Connect{auth_token}
   ```

3. **Client 认证流程（直连模式）**
   ```
   Client → Server: ClientAuth{api_key, website_name}
   Server → Client: ConfigSync{direct_enabled, local_config}
   Client → Server: DirectConnect{auth_token}
   ```

4. **智能选路流程**
   ```
   Client → Server/Relay: RouteTest{test_type, timestamp}
   Server/Relay → Client: RouteReport{latency, success_rate}
   Client: 根据报告选择最优路径
   Client → Server: PathSwitch{new_path} (可选)
   ```

### 4.3 数据转发流程

**中继模式：**
```
User → Server → Relay → Client → Local Service
                 ↓
            数据缓冲和优化
```

**直连模式：**
```
User → Server → Client → Local Service
         ↓
    低延迟直接转发
```

**智能选路模式：**
```
User → Server → [智能选择] → Client → Local Service
                ↓        ↗
               Relay ----┘
         (根据实时网络状况选择最优路径)
```

## 5. 安全设计

### 5.1 认证机制
- **Relay Token**：静态配置，用于 Relay 注册认证
- **API Key**：每网站独立，用于 Client 认证
- **动态 Token**：建立连接后的会话认证

### 5.2 加密通信
- TLS 1.3 端到端加密
- 证书验证机制
- 密钥轮换支持

### 5.3 访问控制
- 网站级别的精细化权限控制
- Relay 只转发已配置网站流量
- IP 白名单机制（可选）

## 6. 部署方案

### 6.1 部署架构

```
出口云服务器
├── bits-link-server (服务端)
│   ├── 配置管理
│   ├── 认证服务
│   └── 路由控制

中转云服务器（与出口服务器之间具有可靠连接） (多个可用区)
├── bits-link-relay (中转服务器)
│   ├── 北京可用区
│   ├── 上海可用区
│   └── 深圳可用区

内网环境
├── bits-link-client (客户端)
│   ├── 网站1客户端
│   ├── 网站2客户端
│   └── 网站N客户端
```

### 6.2 配置管理
- 配置文件版本控制
- 配置变更审批流程
- 配置回滚机制

## 7. 监控和运维

### 7.1 监控指标
- 连接状态监控
- 流量统计
- 延迟监控
- 错误率统计

### 7.2 日志设计
- 结构化日志输出
- 日志级别控制
- 日志轮转机制

### 7.3 故障处理
- 自动故障检测
- 服务降级策略
- 告警通知机制

## 8. 开发计划

### 8.1 Phase 1: 基础框架
- [ ] 公共库开发 (bits-link-common)
- [ ] 基础通信协议
- [ ] 配置文件解析

### 8.2 Phase 2: 核心功能
- [ ] Server 端开发（支持直连和中继路由）
- [ ] Relay 端开发
- [ ] Client 端开发（基础连接功能）
- [ ] 直连模式实现

### 8.3 Phase 3: 功能完善
- [ ] 智能选路功能开发
- [ ] 路由健康检测和自动切换
- [ ] 按路径的路由策略
- [ ] 安全机制完善
- [ ] 监控和日志
- [ ] 文档和测试

### 8.4 Phase 4: 优化和扩展
- [ ] 性能优化
- [ ] 高可用性改进
- [ ] 更多协议支持

## 9. 技术选型

- **编程语言**：Rust (性能优秀，内存安全)
- **异步框架**：Tokio (高性能异步IO)
- **网络库**：tokio-tungstenite (WebSocket), rustls (TLS)
- **序列化**：serde + bincode/json
- **配置解析**：toml + serde
- **日志**：tracing + tracing-subscriber
- **错误处理**：thiserror + anyhow
