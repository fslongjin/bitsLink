use bits_link_common::*;
use bytes::Bytes;
use futures::future::join_all;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, Duration};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info, warn};
use uuid::Uuid;

/// 客户端整体配置
#[derive(Debug, Clone, serde::Deserialize)]
struct ClientAppConfig {
    /// 服务端地址，如 127.0.0.1:7000
    server_addr: String,
    /// 网站列表配置，数组形式 [[websites]]
    websites: Vec<WebsiteClientConfig>,
    /// 路由配置（暂未使用）
    routing: Option<RoutingConfig>,
}

/// 客户端状态
struct ClientState {
    /// 当前网站配置
    website: WebsiteClientConfig,
    /// 服务端地址
    server_addr: String,
    session_id: Option<Uuid>,
    server_config: Option<ServerSideConfig>,
    // 活跃的连接
    active_connections: Arc<Mutex<HashMap<Uuid, mpsc::UnboundedSender<Vec<u8>>>>>,
}

impl ClientState {
    fn new(website: WebsiteClientConfig, server_addr: String) -> Self {
        Self {
            website,
            server_addr,
            session_id: None,
            server_config: None,
            active_connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// 处理到本地服务的连接
async fn handle_local_service_connection(
    connection_id: Uuid,
    local_config: LocalServiceConfig,
    mut data_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    response_sender: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
    session_id: Uuid,
) -> anyhow::Result<()> {
    // 连接到本地服务
    let local_addr = format!("{}:{}", local_config.local_ip, local_config.local_port);
    let local_stream = match TcpStream::connect(&local_addr).await {
        Ok(stream) => {
            info!("Connected to local service: {}", local_addr);
            stream
        }
        Err(e) => {
            error!("Failed to connect to local service {}: {}", local_addr, e);
            // 通知服务端连接失败
            if let Some(sender) = response_sender.lock().await.as_ref() {
                let _ = sender.send(Message::ConnectionEstablished {
                    connection_id,
                    success: false,
                    error: Some(e.to_string()),
                });
            }
            return Err(e.into());
        }
    };

    // 通知服务端连接成功
    if let Some(sender) = response_sender.lock().await.as_ref() {
        let _ = sender.send(Message::ConnectionEstablished {
            connection_id,
            success: true,
            error: None,
        });
    }

    let (mut local_read, mut local_write) = tokio::io::split(local_stream);
    let response_sender_clone = Arc::clone(&response_sender);

    // 任务1：接收服务端数据并转发到本地服务
    let write_task = tokio::spawn(async move {
        while let Some(data) = data_receiver.recv().await {
            if let Err(e) = local_write.write_all(&data).await {
                error!("Failed to write to local service: {}", e);
                break;
            }
        }
    });

    // 任务2：读取本地服务响应并发送给服务端
    let read_task = tokio::spawn(async move {
        let mut buffer = [0u8; 4096];
        loop {
            match local_read.read(&mut buffer).await {
                Ok(0) => break, // 连接关闭
                Ok(n) => {
                    let response_msg = Message::DataPacket {
                        session_id,
                        connection_id,
                        data: buffer[..n].to_vec(),
                    };

                    if let Some(sender) = response_sender_clone.lock().await.as_ref() {
                        if sender.send(response_msg).is_err() {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    error!("Error reading from local service: {}", e);
                    break;
                }
            }
        }
    });

    // 等待任意一个任务完成
    tokio::select! {
        _ = write_task => {}
        _ = read_task => {}
    }

    // 通知服务端连接关闭
    if let Some(sender) = response_sender.lock().await.as_ref() {
        let _ = sender.send(Message::CloseConnection { connection_id });
    }

    info!("Connection {} to local service closed", connection_id);
    Ok(())
}

/// 连接到服务端
async fn connect_to_server(
    state: Arc<Mutex<ClientState>>,
) -> anyhow::Result<Framed<TcpStream, LengthDelimitedCodec>> {
    let (server_addr, register_msg) = {
        let state = state.lock().await;
        let register_msg = Message::ClientRegister {
            token: state.website.token.clone(),
            website_name: state.website.website_name.clone(),
            local_config: state.website.local_service.clone(),
        };
        (state.server_addr.clone(), register_msg)
    };

    info!("Connecting to server: {}", server_addr);
    let stream = TcpStream::connect(&server_addr).await?;
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

    // 发送注册请求
    let msg_data = serde_json::to_vec(&register_msg)?;
    framed.send(Bytes::from(msg_data)).await?;

    // 等待注册响应
    if let Some(Ok(data)) = framed.next().await {
        let response: Message = serde_json::from_slice(&data[..])?;
        match response {
            Message::RegisterResponse {
                success,
                session_id,
                server_config,
                error,
            } => {
                if success {
                    let mut state = state.lock().await;
                    state.session_id = session_id;
                    state.server_config = server_config.clone();
                    info!(
                        "Successfully registered with server, session_id: {:?}, domain: {:?}",
                        session_id,
                        server_config.as_ref().map(|c| &c.domain)
                    );
                } else {
                    return Err(anyhow::anyhow!("Registration failed: {:?}", error));
                }
            }
            _ => {
                return Err(anyhow::anyhow!("Unexpected response from server"));
            }
        }
    } else {
        return Err(anyhow::anyhow!("No response from server"));
    }

    Ok(framed)
}

/// 处理服务端消息
async fn handle_server_messages(
    state: Arc<Mutex<ClientState>>,
    mut server_framed: Framed<TcpStream, LengthDelimitedCodec>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let response_sender = Arc::new(Mutex::new(Some(tx)));

    // 启动心跳任务
    {
        let state_clone = Arc::clone(&state);
        let sender_clone = Arc::clone(&response_sender);
        tokio::spawn(async move {
            heartbeat_task(state_clone, sender_clone).await;
        });
    }

    loop {
        tokio::select! {
            // 接收服务端消息
            frame_result = server_framed.next() => {
                match frame_result {
                    Some(Ok(data)) => {
                        let message: Message = match serde_json::from_slice(&data[..]) {
                            Ok(msg) => msg,
                            Err(e) => {
                                error!("Failed to parse server message: {}", e);
                                continue;
                            }
                        };

                        match message {
                            Message::NewConnection { session_id, connection_id, target_host: _, target_port: _ } => {
                                info!("Server requests new connection: {}", connection_id);

                                // 创建数据通道
                                let (data_tx, data_rx) = mpsc::unbounded_channel::<Vec<u8>>();

                                // 注册连接
                                {
                                    let state_guard = state.lock().await;
                                    let mut connections = state_guard.active_connections.lock().await;
                                    connections.insert(connection_id, data_tx);
                                }

                                // 启动本地服务连接处理
                                let local_config = state.lock().await.website.local_service.clone();
                                let response_sender_clone = Arc::clone(&response_sender);
                                tokio::spawn(async move {
                                    if let Err(e) = handle_local_service_connection(
                                        connection_id,
                                        local_config,
                                        data_rx,
                                        response_sender_clone,
                                        session_id,
                                    ).await {
                                        error!("Error handling local service connection {}: {}", connection_id, e);
                                    }
                                });
                            }
                            Message::DataPacket { session_id: _, connection_id, data } => {
                                // 转发数据到对应的本地连接
                                let state_guard = state.lock().await;
                                let connections = state_guard.active_connections.lock().await;
                                if let Some(data_sender) = connections.get(&connection_id) {
                                    if let Err(e) = data_sender.send(data) {
                                        error!("Failed to send data to local connection {}: {}", connection_id, e);
                                    }
                                } else {
                                    warn!("Received data for unknown connection: {}", connection_id);
                                }
                            }
                            Message::CloseConnection { connection_id } => {
                                info!("Server requests to close connection: {}", connection_id);
                                let state_guard = state.lock().await;
                                let mut connections = state_guard.active_connections.lock().await;
                                connections.remove(&connection_id);
                            }
                            Message::HeartbeatResponse { success } => {
                                if !success {
                                    warn!("Heartbeat failed");
                                }
                            }
                            _ => {
                                warn!("Unexpected message from server: {:?}", message);
                            }
                        }
                    }
                    Some(Err(e)) => {
                        error!("Server connection error: {}", e);
                        break;
                    }
                    None => {
                        info!("Server disconnected");
                        break;
                    }
                }
            }
            // 发送消息到服务端
            msg = rx.recv() => {
                if let Some(message) = msg {
                    let msg_data = serde_json::to_vec(&message)?;
                    if let Err(e) = server_framed.send(Bytes::from(msg_data)).await {
                        error!("Failed to send message to server: {}", e);
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    Ok(())
}

/// 心跳任务
async fn heartbeat_task(
    state: Arc<Mutex<ClientState>>,
    response_sender: Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>,
) {
    let mut interval = interval(Duration::from_secs(30));

    loop {
        interval.tick().await;

        let session_id = {
            let state = state.lock().await;
            match state.session_id {
                Some(id) => id,
                None => continue,
            }
        };

        let heartbeat_msg = Message::ClientHeartbeat { session_id };

        if let Some(sender) = response_sender.lock().await.as_ref() {
            if sender.send(heartbeat_msg).is_err() {
                warn!("Failed to send heartbeat");
                break;
            }
        } else {
            break;
        }
    }
}

/// 加载配置文件
fn load_config() -> anyhow::Result<ClientAppConfig> {
    let config_content = std::fs::read_to_string("client.toml")
        .or_else(|_| std::fs::read_to_string("config/client.toml"))
        .unwrap_or_else(|_| default_client_config());

    let config: ClientAppConfig = toml::from_str(&config_content)?;
    Ok(config)
}

/// 默认配置
fn default_client_config() -> String {
    r#"
server_addr = "127.0.0.1:7000"

[[websites]]
website_name = "demo"
token = "demo_token_123"
local_ip = "127.0.0.1"
local_port = 8080
protocol = "http"

[routing]
mode = "auto"
"#
    .to_string()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 加载配置
    let config = load_config()?;
    info!("Client config loaded, server: {}", config.server_addr);
    for site in &config.websites {
        info!(
            "  Website: {} => {}:{} ({})",
            site.website_name,
            site.local_service.local_ip,
            site.local_service.local_port,
            site.local_service.protocol,
        );
    }

    // 为每个网站启动独立任务
    let server_addr_global = config.server_addr.clone();
    let mut handles = Vec::new();
    for site in config.websites {
        let server_addr = server_addr_global.clone();
        let handle = tokio::spawn(async move {
            let state = Arc::new(Mutex::new(ClientState::new(site.clone(), server_addr)));

            // 自动重连循环
            loop {
                match connect_to_server(Arc::clone(&state)).await {
                    Ok(server_framed) => {
                        let site_name = {
                            let guard = state.lock().await;
                            guard.website.website_name.clone()
                        };
                        info!(
                            "Connected to server for website {}, starting message handling...",
                            site_name
                        );
                        if let Err(e) =
                            handle_server_messages(Arc::clone(&state), server_framed).await
                        {
                            error!(
                                "Error while handling server messages for {}: {}",
                                site_name, e
                            );
                        } else {
                            warn!(
                                "Server message handler for {} exited. Will attempt to reconnect.",
                                site_name
                            );
                        }
                    }
                    Err(e) => {
                        error!("Failed to connect to server: {}", e);
                    }
                }

                // 清理会话信息与活跃连接
                {
                    let active_arc = {
                        let mut guard = state.lock().await;
                        guard.session_id = None;
                        Arc::clone(&guard.active_connections)
                    };
                    active_arc.lock().await.clear();
                }

                info!("Reconnecting in 5 seconds...");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        handles.push(handle);
    }

    // 等待所有任务（通常不会结束）
    let _ = join_all(handles).await;
    Ok(())
}
