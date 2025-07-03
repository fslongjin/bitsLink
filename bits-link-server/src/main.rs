use bits_link_common::*;
use bytes::Bytes;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info, warn};
use uuid::Uuid;

/// 客户端会话信息
#[derive(Debug, Clone)]
struct ClientSession {
    session_id: Uuid,
    token: String,
    website_name: String,
    local_config: LocalServiceConfig,
    server_config: ServerSideConfig,
    // 用于向客户端发送消息的通道
    message_sender: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    // HTTP 连接管理
    http_connections: Arc<DashMap<Uuid, mpsc::UnboundedSender<Vec<u8>>>>,
}

/// 服务端状态
struct ServerState {
    config: ServerConfig,
    sessions: Arc<DashMap<Uuid, ClientSession>>,
    token_to_session: Arc<DashMap<String, Uuid>>,
    domain_to_session: Arc<DashMap<String, Uuid>>,
}

impl ServerState {
    fn new(config: ServerConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(DashMap::new()),
            token_to_session: Arc::new(DashMap::new()),
            domain_to_session: Arc::new(DashMap::new()),
        }
    }

    fn register_client(
        &self,
        token: String,
        _website_name: String,
        local_config: LocalServiceConfig,
        message_sender: mpsc::UnboundedSender<Message>,
    ) -> Result<(Uuid, ServerSideConfig)> {
        // 根据 token 查找对应的网站配置
        let (website_name, website_config) = self
            .config
            .websites
            .iter()
            .find(|(_, cfg)| cfg.token == token)
            .ok_or_else(|| BitsLinkError::Auth("Invalid token".to_string()))?;

        // 检查是否已有会话
        if let Some(existing_session_id) = self.token_to_session.get(&token) {
            if let Some(session) = self.sessions.get_mut(&existing_session_id) {
                // 更新消息发送器
                *session.message_sender.blocking_lock() = Some(message_sender);
                return Ok((session.session_id, session.server_config.clone()));
            }
        }

        // 创建新会话
        let session_id = Uuid::new_v4();
        let server_config = ServerSideConfig {
            domain: website_config.domain.clone(),
            relay_server: website_config.relay_server.clone(),
        };

        let session = ClientSession {
            session_id,
            token: token.clone(),
            website_name: website_name.clone(),
            local_config,
            server_config: server_config.clone(),
            message_sender: Arc::new(Mutex::new(Some(message_sender))),
            http_connections: Arc::new(DashMap::new()),
        };

        self.sessions.insert(session_id, session);
        self.token_to_session.insert(token.clone(), session_id);
        self.domain_to_session
            .insert(website_config.domain.clone(), session_id);

        info!(
            "Client registered: session_id={}, domain={}",
            session_id, server_config.domain
        );
        Ok((session_id, server_config))
    }

    fn get_session(&self, session_id: &Uuid) -> Option<ClientSession> {
        self.sessions.get(session_id).map(|s| s.clone())
    }

    fn get_session_by_domain(&self, domain: &str) -> Option<ClientSession> {
        if let Some(session_id) = self.domain_to_session.get(domain) {
            self.get_session(&session_id)
        } else {
            None
        }
    }

    fn remove_session(&self, session_id: &Uuid) {
        if let Some((_, session)) = self.sessions.remove(session_id) {
            self.token_to_session.remove(&session.token);
            self.domain_to_session.remove(&session.server_config.domain);
            info!("Session {} removed", session_id);
        }
    }
}

/// 处理客户端控制连接
async fn handle_client_control(
    stream: TcpStream,
    addr: SocketAddr,
    state: Arc<ServerState>,
) -> anyhow::Result<()> {
    info!("New client control connection from: {}", addr);

    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    let mut current_session: Option<Uuid> = None;
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    loop {
        tokio::select! {
            // 接收来自客户端的消息
            frame_result = framed.next() => {
                match frame_result {
                    Some(Ok(data)) => {
                        let message: Message = match serde_json::from_slice(&data[..]) {
                            Ok(msg) => msg,
                            Err(e) => {
                                error!("Failed to parse message: {}", e);
                                continue;
                            }
                        };

                        let response = match message {
                            Message::ClientRegister { token, website_name, local_config } => {
                                match state.register_client(token, website_name, local_config, tx.clone()) {
                                    Ok((session_id, server_config)) => {
                                        current_session = Some(session_id);
                                        Message::RegisterResponse {
                                            success: true,
                                            session_id: Some(session_id),
                                            server_config: Some(server_config),
                                            error: None,
                                        }
                                    }
                                    Err(e) => {
                                        error!("Client registration failed: {}", e);
                                        Message::RegisterResponse {
                                            success: false,
                                            session_id: None,
                                            server_config: None,
                                            error: Some(e.to_string()),
                                        }
                                    }
                                }
                            }
                            Message::ClientHeartbeat { session_id } => {
                                let success = state.get_session(&session_id).is_some();
                                if !success {
                                    warn!("Heartbeat from unknown session: {}", session_id);
                                }
                                Message::HeartbeatResponse { success }
                            }
                            Message::ConnectionEstablished { connection_id, success, error } => {
                                // 客户端响应连接建立结果
                                info!("Connection {} established: {}", connection_id, success);
                                if let Some(error) = error {
                                    error!("Connection error: {}", error);
                                }
                                continue; // 不需要响应
                            }
                            Message::DataPacket { session_id, connection_id, data } => {
                                // 将数据转发到对应的HTTP连接
                                if let Some(session) = state.get_session(&session_id) {
                                    if let Some(response_sender) = session.http_connections.get(&connection_id) {
                                        if let Err(e) = response_sender.send(data) {
                                            error!("Failed to send response data: {}", e);
                                            session.http_connections.remove(&connection_id);
                                        }
                                    }
                                }
                                continue; // 数据包不需要响应
                            }
                            Message::CloseConnection { connection_id } => {
                                if let Some(session_id) = current_session {
                                    if let Some(session) = state.get_session(&session_id) {
                                        session.http_connections.remove(&connection_id);
                                    }
                                }
                                continue;
                            }
                            _ => {
                                warn!("Unexpected message type");
                                continue;
                            }
                        };

                        let response_data = serde_json::to_vec(&response)?;
                        if let Err(e) = framed.send(Bytes::from(response_data)).await {
                            error!("Failed to send response: {}", e);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        error!("Connection error: {}", e);
                        break;
                    }
                    None => {
                        info!("Client disconnected: {}", addr);
                        break;
                    }
                }
            }
            // 发送消息给客户端
            msg = rx.recv() => {
                if let Some(message) = msg {
                    let msg_data = serde_json::to_vec(&message)?;
                    if let Err(e) = framed.send(Bytes::from(msg_data)).await {
                        error!("Failed to send message to client: {}", e);
                        break;
                    }
                } else {
                    // 通道关闭
                    break;
                }
            }
        }
    }

    // 清理会话
    if let Some(session_id) = current_session {
        state.remove_session(&session_id);
    }

    Ok(())
}

/// 处理单个 HTTP 连接
async fn handle_http_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    state: Arc<ServerState>,
) -> anyhow::Result<()> {
    info!("New HTTP connection from: {}", addr);

    // 读取 HTTP 请求的第一行来获取 Host 头
    let mut buffer = [0u8; 4096];
    let mut request_data = Vec::new();
    let mut host = String::new();

    // 读取并解析 HTTP 请求头
    loop {
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Ok(()); // 连接关闭
        }

        request_data.extend_from_slice(&buffer[..n]);

        // 检查是否收到完整的请求头
        if let Some(headers_end) = find_headers_end(&request_data) {
            // 解析 Host 头
            if let Some(parsed_host) = parse_host_header(&request_data[..headers_end]) {
                host = parsed_host;
            }
            break;
        }

        // 防止恶意大请求
        if request_data.len() > 5 * 1024 * 1024 {
            error!("HTTP request too large");
            return Ok(());
        }
    }

    if host.is_empty() {
        host = "localhost".to_string();
    }

    info!("HTTP request from {} to host: {}", addr, host);

    // 根据 Host 头找到对应的客户端会话
    let session = match state.get_session_by_domain(&host) {
        Some(session) => session,
        None => {
            warn!("No client session found for domain: {}", host);
            let resp_body = format!(
                r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <title>404 - Service Not Available</title>
    <style>
        body{{ font-family: Arial, Helvetica, sans-serif; background:#f5f5f5; color:#333; text-align:center; margin:0; }}
        .container{{ padding: 8% 1rem; }}
        h1{{ font-size:72px; margin:0 0 20px; }}
        p{{ font-size:18px; margin:5px 0; }}
        a{{ color:#0066cc; text-decoration:none; }}
        a:hover{{ text-decoration:underline; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>404</h1>
        <p>Oops! Service for <strong>{}</strong> is not available.</p>
        <p>Powered by <a href="https://github.com/fslongjin/bitsLink" target="_blank" rel="noopener">bits-link</a></p>
    </div>
</body>
</html>"#,
                host
            );
            let _ = stream
                .write_all(
                    format!(
                        "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n{}",
                        resp_body.len(),
                        resp_body
                    )
                    .as_bytes(),
                )
                .await;
            return Ok(());
        }
    };

    // 创建连接ID和响应通道
    let connection_id = Uuid::new_v4();
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // 注册响应通道
    session.http_connections.insert(connection_id, response_tx);

    // 向客户端发送新连接请求
    let new_conn_msg = Message::NewConnection {
        session_id: session.session_id,
        connection_id,
        target_host: session.local_config.local_ip.clone(),
        target_port: session.local_config.local_port,
    };

    // 发送消息给客户端
    if let Some(sender) = session.message_sender.lock().await.as_ref() {
        if let Err(e) = sender.send(new_conn_msg) {
            error!("Failed to send message to client: {}", e);
            let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 20\r\n\r\nClient not available").await;
            return Ok(());
        }
    } else {
        let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 19\r\n\r\nClient not connected").await;
        return Ok(());
    }

    // 发送原始HTTP请求数据给客户端
    let data_msg = Message::DataPacket {
        session_id: session.session_id,
        connection_id,
        data: request_data,
    };

    if let Some(sender) = session.message_sender.lock().await.as_ref() {
        if let Err(e) = sender.send(data_msg) {
            error!("Failed to send HTTP data to client: {}", e);
        }
    }

    // 等待并转发响应
    loop {
        tokio::select! {
            Some(chunk) = response_rx.recv() => {
                stream.write_all(&chunk).await?;
            }
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                // 超时处理
                stream.write_all(b"HTTP/1.1 504 Gateway Timeout\r\nContent-Length: 15\r\n\r\nRequest timeout").await?;
                break;
            }
            else => break, // response_rx 已关闭
        }
    }

    // 清理连接
    session.http_connections.remove(&connection_id);

    Ok(())
}

/// 查找 HTTP 请求头结束位置
fn find_headers_end(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(3) {
        if &data[i..i + 4] == b"\r\n\r\n" {
            return Some(i + 4);
        }
    }
    None
}

/// 解析 Host 头
fn parse_host_header(headers: &[u8]) -> Option<String> {
    let headers_str = String::from_utf8_lossy(headers);
    for line in headers_str.lines() {
        if line.to_lowercase().starts_with("host:") {
            return line.split(':').nth(1).map(|h| h.trim().to_string());
        }
    }
    None
}

/// 启动 HTTP 代理服务器
async fn start_http_proxy(listen_addr: &str, state: Arc<ServerState>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen_addr).await?;
    info!("HTTP proxy server started on: {}", listen_addr);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                let state_clone = Arc::clone(&state);
                tokio::spawn(async move {
                    if let Err(e) = handle_http_connection(stream, addr, state_clone).await {
                        error!("Error handling HTTP connection {}: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept HTTP connection: {}", e);
            }
        }
    }
}

/// 加载配置文件
fn load_config() -> anyhow::Result<ServerConfig> {
    let config_content = std::fs::read_to_string("server.toml")
        .or_else(|_| std::fs::read_to_string("config/server.toml"))
        .unwrap_or_else(|_| default_server_config());

    let config: ServerConfig = toml::from_str(&config_content)?;
    Ok(config)
}

/// 默认配置
fn default_server_config() -> String {
    r#"
listen_addr = "0.0.0.0:7000"
http_proxy_addr = "0.0.0.0:7001"

[websites.demo]
token = "demo_token_123"
domain = "demo.example.com"

[websites.test]
token = "test_token_456"
domain = "test.example.com"
"#
    .to_string()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 加载配置
    let config = load_config()?;

    // HTTP 代理地址
    let http_proxy_addr = config.http_proxy_addr.clone();

    info!("Server config loaded");
    info!("  Control server: {}", config.listen_addr);
    info!("  HTTP proxy: {}", http_proxy_addr);

    // 创建服务端状态
    let state = Arc::new(ServerState::new(config.clone()));

    // 启动控制协议服务器
    let control_state = Arc::clone(&state);
    let control_addr = config.listen_addr.clone();
    tokio::spawn(async move {
        let listener = TcpListener::bind(&control_addr).await.unwrap();
        info!("Control server started on {}", control_addr);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let state_clone = Arc::clone(&control_state);
                    tokio::spawn(async move {
                        if let Err(e) = handle_client_control(stream, addr, state_clone).await {
                            error!("Error handling client control connection {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept control connection: {}", e);
                }
            }
        }
    });

    // 启动 HTTP 代理服务器
    start_http_proxy(&http_proxy_addr, state).await?;

    Ok(())
}
