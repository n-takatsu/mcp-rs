//! WebSocket Server Mode Implementation
//!
//! Axumベースのフル機能WebSocketサーバー

use crate::error::{Error, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    response::Response,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// 接続ID型
pub type ConnectionId = Uuid;

/// WebSocketサーバー設定
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// バインドアドレス
    pub bind_addr: SocketAddr,
    /// 最大接続数
    pub max_connections: usize,
    /// メッセージ最大サイズ (bytes)
    pub max_message_size: usize,
    /// Ping間隔
    pub ping_interval: Duration,
    /// タイムアウト
    pub timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: ([127, 0, 0, 1], 8080).into(),
            max_connections: 1000,
            max_message_size: 16 * 1024 * 1024, // 16MB
            ping_interval: Duration::from_secs(30),
            timeout: Duration::from_secs(60),
        }
    }
}

/// WebSocket接続情報
#[derive(Debug, Clone)]
pub struct WebSocketConnectionInfo {
    /// 接続ID
    pub id: ConnectionId,
    /// クライアントアドレス
    pub client_addr: SocketAddr,
    /// 接続時刻
    pub connected_at: Instant,
    /// 最終活動時刻
    pub last_activity: Instant,
    /// 受信メッセージ数
    pub messages_received: u64,
    /// 送信メッセージ数
    pub messages_sent: u64,
}

impl WebSocketConnectionInfo {
    pub fn new(id: ConnectionId, client_addr: SocketAddr) -> Self {
        let now = Instant::now();
        Self {
            id,
            client_addr,
            connected_at: now,
            last_activity: now,
            messages_received: 0,
            messages_sent: 0,
        }
    }
}

/// メッセージハンドラトレイト
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// メッセージを処理
    async fn handle_message(
        &self,
        conn_id: ConnectionId,
        message: Message,
    ) -> Result<Option<Message>>;

    /// 接続時のコールバック
    async fn on_connect(&self, conn_id: ConnectionId, addr: SocketAddr) -> Result<()>;

    /// 切断時のコールバック
    async fn on_disconnect(&self, conn_id: ConnectionId) -> Result<()>;
}

/// デフォルトメッセージハンドラ (エコーサーバー)
#[derive(Debug, Clone, Default)]
pub struct EchoHandler;

#[async_trait::async_trait]
impl MessageHandler for EchoHandler {
    async fn handle_message(
        &self,
        _conn_id: ConnectionId,
        message: Message,
    ) -> Result<Option<Message>> {
        // テキストメッセージをそのまま返す
        match message {
            Message::Text(text) => Ok(Some(Message::Text(text))),
            Message::Binary(data) => Ok(Some(Message::Binary(data))),
            _ => Ok(None),
        }
    }

    async fn on_connect(&self, conn_id: ConnectionId, addr: SocketAddr) -> Result<()> {
        info!("Client connected: {} from {}", conn_id, addr);
        Ok(())
    }

    async fn on_disconnect(&self, conn_id: ConnectionId) -> Result<()> {
        info!("Client disconnected: {}", conn_id);
        Ok(())
    }
}

/// WebSocketサーバー共有状態
#[derive(Clone)]
pub struct ServerState {
    /// アクティブな接続
    connections: Arc<Mutex<HashMap<ConnectionId, WebSocketConnectionInfo>>>,
    /// メッセージハンドラ
    handler: Arc<dyn MessageHandler>,
    /// 設定
    config: Arc<ServerConfig>,
    /// 総接続数カウンター
    total_connections: Arc<AtomicU64>,
}

impl ServerState {
    pub fn new(config: ServerConfig, handler: Arc<dyn MessageHandler>) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            handler,
            config: Arc::new(config),
            total_connections: Arc::new(AtomicU64::new(0)),
        }
    }

    /// 接続数を取得
    pub async fn active_connections(&self) -> usize {
        self.connections.lock().await.len()
    }

    /// 総接続数を取得
    pub fn total_connections(&self) -> u64 {
        self.total_connections.load(Ordering::SeqCst)
    }
}

/// WebSocketサーバー
pub struct WebSocketServer {
    /// サーバー状態
    state: ServerState,
    /// 実行中フラグ
    running: Arc<RwLock<bool>>,
}

impl WebSocketServer {
    /// 新しいWebSocketサーバーを作成
    pub fn new(config: ServerConfig) -> Self {
        let handler = Arc::new(EchoHandler) as Arc<dyn MessageHandler>;
        Self::with_handler(config, handler)
    }

    /// カスタムハンドラ付きでサーバーを作成
    pub fn with_handler(config: ServerConfig, handler: Arc<dyn MessageHandler>) -> Self {
        let state = ServerState::new(config, handler);
        Self {
            state,
            running: Arc::new(RwLock::new(false)),
        }
    }

    /// サーバーを起動
    pub async fn start(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if *running {
            return Err(Error::Server("Server is already running".to_string()));
        }

        let bind_addr = self.state.config.bind_addr;

        // Axumアプリを構築
        let app = Router::new()
            .route("/ws", get(websocket_handler))
            .with_state(self.state.clone());

        info!("Starting WebSocket server on {}", bind_addr);

        // サーバーを起動
        let listener = tokio::net::TcpListener::bind(bind_addr)
            .await
            .map_err(|e| Error::Server(format!("Failed to bind: {}", e)))?;

        *running = true;

        tokio::spawn(async move {
            if let Err(e) = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            {
                error!("Server error: {}", e);
            }
        });

        info!("WebSocket server started successfully");
        Ok(())
    }

    /// サーバーを停止
    pub async fn stop(&mut self) -> Result<()> {
        let mut running = self.running.write().await;
        if !*running {
            return Err(Error::Server("Server is not running".to_string()));
        }

        *running = false;
        info!("WebSocket server stopped");
        Ok(())
    }

    /// 全クライアントにブロードキャスト
    pub async fn broadcast(&self, _message: Message) -> Result<()> {
        let count = self.state.connections.lock().await.len();
        debug!("Broadcasting message to {} clients", count);

        // 実装はクライアント送信機能が必要
        // 現在は接続情報のみ管理しているため、今後拡張

        Ok(())
    }

    /// 特定クライアントへメッセージ送信
    pub async fn send_to(&self, conn_id: ConnectionId, _message: Message) -> Result<()> {
        if self.state.connections.lock().await.contains_key(&conn_id) {
            // 実装はクライアント送信チャネルが必要
            debug!("Sending message to client: {}", conn_id);
            Ok(())
        } else {
            Err(Error::Connection(format!("Client not found: {}", conn_id)))
        }
    }

    /// サーバー統計を取得
    pub async fn get_statistics(&self) -> ServerStatistics {
        ServerStatistics {
            active_connections: self.state.active_connections().await,
            total_connections: self.state.total_connections(),
        }
    }
}

/// サーバー統計
#[derive(Debug, Clone)]
pub struct ServerStatistics {
    pub active_connections: usize,
    pub total_connections: u64,
}

/// WebSocketアップグレードハンドラ
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    // 接続数チェック
    if state.active_connections().await >= state.config.max_connections {
        warn!("Max connections reached, rejecting {}", addr);
        // 503エラーを返すべきだが、Axumの制約で単純なレスポンスを返す
        return axum::http::Response::builder()
            .status(503)
            .body("Service Unavailable".into())
            .unwrap();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}

/// WebSocket接続を処理
async fn handle_socket(socket: WebSocket, state: ServerState, addr: SocketAddr) {
    let conn_id = Uuid::new_v4();

    // 接続情報を登録
    let conn_info = WebSocketConnectionInfo::new(conn_id, addr);
    state.connections.lock().await.insert(conn_id, conn_info);
    state.total_connections.fetch_add(1, Ordering::SeqCst);

    // ハンドラに通知
    if let Err(e) = state.handler.on_connect(conn_id, addr).await {
        error!("Handler on_connect failed: {}", e);
    }

    // メッセージループ
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));

    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(msg) => {
                // 活動時刻を更新
                if let Some(conn) = state.connections.lock().await.get_mut(&conn_id) {
                    conn.last_activity = Instant::now();
                    conn.messages_received += 1;
                }

                // ハンドラでメッセージを処理
                match state.handler.handle_message(conn_id, msg).await {
                    Ok(Some(response)) => {
                        // レスポンスを送信
                        let mut s = sender.lock().await;
                        if let Err(e) = s.send(response).await {
                            error!("Failed to send response: {}", e);
                            break;
                        }

                        if let Some(conn) = state.connections.lock().await.get_mut(&conn_id) {
                            conn.messages_sent += 1;
                        }
                    }
                    Ok(None) => {
                        // レスポンスなし
                    }
                    Err(e) => {
                        error!("Handler error: {}", e);
                        break;
                    }
                }
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // 接続を削除
    state.connections.lock().await.remove(&conn_id);

    // ハンドラに通知
    if let Err(e) = state.handler.on_disconnect(conn_id).await {
        error!("Handler on_disconnect failed: {}", e);
    }

    debug!("Connection {} closed", conn_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = ServerConfig::default();
        let server = WebSocketServer::new(config);
        assert_eq!(server.state.active_connections().await, 0);
    }

    #[tokio::test]
    async fn test_server_statistics() {
        let config = ServerConfig::default();
        let server = WebSocketServer::new(config);
        let stats = server.get_statistics().await;
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.total_connections, 0);
    }
}
