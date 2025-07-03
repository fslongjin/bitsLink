use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum BitsLinkError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, BitsLinkError>;

/// 协议消息类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    // 客户端消息
    ClientRegister {
        token: String,
        website_name: String,
        local_config: LocalServiceConfig,
    },
    ClientHeartbeat {
        session_id: Uuid,
    },

    // 服务端响应
    RegisterResponse {
        success: bool,
        session_id: Option<Uuid>,
        server_config: Option<ServerSideConfig>,
        error: Option<String>,
    },
    HeartbeatResponse {
        success: bool,
    },

    // 数据传输
    DataPacket {
        session_id: Uuid,
        connection_id: Uuid,
        data: Vec<u8>,
    },

    // 连接管理
    NewConnection {
        session_id: Uuid,
        connection_id: Uuid,
        target_host: String,
        target_port: u16,
    },
    ConnectionEstablished {
        connection_id: Uuid,
        success: bool,
        error: Option<String>,
    },
    CloseConnection {
        connection_id: Uuid,
    },
}

/// 客户端本地服务配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalServiceConfig {
    pub local_ip: String,
    pub local_port: u16,
    pub protocol: String,
}

/// 服务端下发给客户端的配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSideConfig {
    pub domain: String,
    pub relay_server: Option<String>,
}

/// 网站配置（服务端）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteConfig {
    pub token: String,
    pub domain: String,
    pub relay_server: Option<String>,
}

/// 服务端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_http_proxy_addr")]
    pub http_proxy_addr: String,
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub websites: HashMap<String, WebsiteConfig>,
    #[serde(default)]
    pub relays: HashMap<String, RelayConfig>,
}

fn default_listen_addr() -> String {
    "0.0.0.0:7000".to_string()
}

fn default_http_proxy_addr() -> String {
    "0.0.0.0:7001".to_string()
}

/// 单个网站的客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteClientConfig {
    pub website_name: String,
    pub token: String,
    #[serde(flatten)]
    pub local_service: LocalServiceConfig,
}

/// 客户端应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientAppConfig {
    pub server_addr: String,
    pub websites: Vec<WebsiteClientConfig>,
    pub routing: Option<RoutingConfig>,
}

/// TLS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enable: bool,
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
    pub ca_file: Option<String>,
}

/// 中继服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub token: String,
    pub region: String,
    pub addr: String,
}

/// 路由配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub mode: RoutingMode,
    pub direct_timeout: Option<u64>,
    pub relay_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    Direct, // 仅直连
    Relay,  // 仅中继
    Auto,   // 智能选择
}

impl Default for RoutingMode {
    fn default() -> Self {
        RoutingMode::Auto
    }
}

/// 连接状态
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub latency: Option<u64>,
    pub success_rate: f64,
    pub last_check: std::time::Instant,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self {
            latency: None,
            success_rate: 1.0,
            last_check: std::time::Instant::now(),
        }
    }
}

// 保留旧的 ClientConfig 以兼容现有代码，但标记为废弃
#[deprecated(note = "Use ServerSideConfig instead")]
pub type ClientConfig = ServerSideConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let local_config = LocalServiceConfig {
            local_ip: "127.0.0.1".to_string(),
            local_port: 8080,
            protocol: "http".to_string(),
        };

        let msg = Message::ClientRegister {
            token: "test_token".to_string(),
            website_name: "test_site".to_string(),
            local_config,
        };

        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            Message::ClientRegister {
                token,
                website_name,
                local_config,
            } => {
                assert_eq!(token, "test_token");
                assert_eq!(website_name, "test_site");
                assert_eq!(local_config.local_port, 8080);
            }
            _ => panic!("Deserialization failed"),
        }
    }

    #[test]
    fn test_website_config_flattening() {
        let config = WebsiteClientConfig {
            website_name: "test_site".to_string(),
            token: "test_token".to_string(),
            local_service: LocalServiceConfig {
                local_ip: "127.0.0.1".to_string(),
                local_port: 8080,
                protocol: "http".to_string(),
            },
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let json: serde_json::Value = serde_json::from_str(&serialized).unwrap();

        // 验证字段被正确展平
        assert_eq!(json["website_name"], "test_site");
        assert_eq!(json["token"], "test_token");
        assert_eq!(json["local_ip"], "127.0.0.1");
        assert_eq!(json["local_port"], 8080);
        assert_eq!(json["protocol"], "http");

        // 确保local_service字段没有作为独立对象出现
        assert!(json.get("local_service").is_none());
    }
}
