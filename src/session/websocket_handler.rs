use super::security_integration_basic::{
    SecurityEventType, SecuritySeverity, SessionSecurityMiddleware,
};
use crate::error::SessionError;
use crate::session::{SessionId, SessionManager, SessionState};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket},
        ConnectInfo, State, WebSocketUpgrade,
    },
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// WebSocketセッション管理ハンドラー
///
/// リアルタイム編集システムの基盤として、セッション管理統合WebSocketハンドラーを提供
#[derive(Debug)]
pub struct SessionWebSocketHandler {
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// セキュリティミドルウェア
    security_middleware: Arc<SessionSecurityMiddleware>,
    /// 接続管理
    connection_manager: Arc<WebSocketConnectionManager>,
    /// 設定
    config: WebSocketHandlerConfig,
}

/// WebSocketハンドラー設定
#[derive(Debug, Clone)]
pub struct WebSocketHandlerConfig {
    /// セッション必須接続
    pub require_session: bool,
    /// 最大接続数（セッションあたり）
    pub max_connections_per_session: usize,
    /// ハートビート間隔
    pub heartbeat_interval_secs: u64,
    /// 接続タイムアウト
    pub connection_timeout_secs: u64,
    /// メッセージサイズ制限
    pub max_message_size_bytes: usize,
    /// メッセージレート制限
    pub max_messages_per_second: u32,
}

/// WebSocket接続管理
#[derive(Debug)]
pub struct WebSocketConnectionManager {
    /// セッションID -> 接続リスト
    connections: RwLock<HashMap<SessionId, Vec<WebSocketConnection>>>,
    /// ブロードキャストチャンネル
    broadcast_tx: broadcast::Sender<BroadcastMessage>,
}

/// WebSocket接続情報
#[derive(Debug, Clone)]
pub struct WebSocketConnection {
    /// 接続ID
    pub connection_id: Uuid,
    /// セッションID
    pub session_id: SessionId,
    /// クライアントアドレス
    pub client_addr: Option<SocketAddr>,
    /// 接続開始時刻
    pub connected_at: DateTime<Utc>,
    /// 最終アクティビティ時刻
    pub last_activity: DateTime<Utc>,
    /// 接続メタデータ
    pub metadata: ConnectionMetadata,
}

/// 接続メタデータ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetadata {
    /// User-Agent
    pub user_agent: Option<String>,
    /// 接続タイプ
    pub connection_type: ConnectionType,
    /// クライアント機能
    pub client_capabilities: ClientCapabilities,
}

/// 接続タイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConnectionType {
    /// 編集クライアント
    Editor,
    /// 監視クライアント
    Observer,
    /// 管理クライアント
    Admin,
    /// API クライアント
    Api,
}

/// クライアント機能
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    /// リアルタイム編集対応
    pub supports_realtime_editing: bool,
    /// ファイル同期対応
    pub supports_file_sync: bool,
    /// 音声チャット対応
    pub supports_voice_chat: bool,
    /// 画面共有対応
    pub supports_screen_share: bool,
    /// コラボレーション機能対応
    pub supports_collaboration: bool,
}

/// ブロードキャストメッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcastMessage {
    /// メッセージID
    pub message_id: Uuid,
    /// 送信者セッションID
    pub sender_session_id: Option<SessionId>,
    /// 対象セッションID（Noneの場合は全体ブロードキャスト）
    pub target_session_id: Option<SessionId>,
    /// メッセージタイプ
    pub message_type: BroadcastMessageType,
    /// ペイロード
    pub payload: Value,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
}

/// ブロードキャストメッセージタイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BroadcastMessageType {
    /// セッション通知
    SessionNotification,
    /// リアルタイム編集
    RealtimeEdit,
    /// ファイル同期
    FileSync,
    /// システム通知
    SystemNotification,
    /// チャットメッセージ
    ChatMessage,
    /// コラボレーション制御
    CollaborationControl,
    /// 脅威通知
    ThreatNotification,
}

/// WebSocketメッセージプロトコル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessageProtocol {
    /// メッセージID
    pub id: Uuid,
    /// メッセージタイプ
    pub r#type: String,
    /// ペイロード
    pub payload: Value,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// セッション情報
    pub session_info: Option<SessionInfo>,
}

/// セッション情報（WebSocketメッセージ用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// セッションID
    pub session_id: String,
    /// ユーザーID
    pub user_id: Option<String>,
    /// セッション状態
    pub state: String,
}

impl SessionWebSocketHandler {
    /// 新しいWebSocketハンドラーを作成
    pub fn new(
        session_manager: Arc<SessionManager>,
        security_middleware: Arc<SessionSecurityMiddleware>,
        config: WebSocketHandlerConfig,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);
        let connection_manager = Arc::new(WebSocketConnectionManager::new(broadcast_tx.clone()));

        Self {
            session_manager,
            security_middleware,
            connection_manager,
            config,
        }
    }

    /// WebSocket接続ハンドラー
    #[instrument(skip(self, ws, headers))]
    pub async fn handle_websocket_connection(
        &self,
        ws: WebSocketUpgrade,
        headers: HeaderMap,
        client_addr: Option<SocketAddr>,
    ) -> Result<Response, axum::response::Response> {
        debug!("WebSocket接続要求を処理中");

        // セッション情報を抽出
        let session_id = match self.extract_session_from_headers(&headers).await {
            Ok(session_id) => {
                debug!("セッションID抽出成功: {:?}", session_id);
                session_id
            }
            Err(e) => {
                error!("セッションID抽出エラー: {:?}", e);
                return Err(e);
            }
        };

        if self.config.require_session && session_id.is_none() {
            return self
                .create_rejection_response("Session required for WebSocket connection")
                .await;
        }

        // セッション検証
        if let Some(ref session_id) = session_id {
            debug!("セッション検証開始: {:?}", session_id);
            match self.validate_session_for_websocket(session_id).await {
                Ok(false) => {
                    warn!("無効なセッション: {:?}", session_id);
                    return self
                        .create_rejection_response("Invalid session for WebSocket connection")
                        .await;
                }
                Err(e) => {
                    error!("セッション検証エラー: {}", e);
                    return self
                        .create_rejection_response("Session validation failed")
                        .await;
                }
                Ok(true) => {
                    info!("セッション検証成功: {:?}", session_id);
                }
            }
        } else {
            debug!("セッションIDなしで接続");
        }

        // 接続メタデータを構築
        let metadata = self.build_connection_metadata(&headers);

        // WebSocket接続を受諾
        let handler = Arc::new(self.clone());
        Ok(ws.on_upgrade(move |socket| {
            handler.handle_socket(socket, session_id, client_addr, metadata)
        }))
    }

    /// WebSocketソケット処理
    #[instrument(skip(self, socket, metadata))]
    async fn handle_socket(
        self: Arc<Self>,
        mut socket: WebSocket,
        session_id: Option<SessionId>,
        client_addr: Option<SocketAddr>,
        metadata: ConnectionMetadata,
    ) {
        let connection_id = Uuid::new_v4();
        info!(
            "WebSocket接続開始: connection_id={}, session_id={:?}",
            connection_id, session_id
        );

        // 接続を登録
        if let Some(ref session_id) = session_id {
            let connection = WebSocketConnection {
                connection_id,
                session_id: session_id.clone(),
                client_addr,
                connected_at: Utc::now(),
                last_activity: Utc::now(),
                metadata: metadata.clone(),
            };

            if let Err(e) = self
                .connection_manager
                .register_connection(connection)
                .await
            {
                error!("接続登録失敗: {}", e);
                let _ = socket.close().await;
                return;
            }
        }

        // ブロードキャスト受信設定
        let mut broadcast_rx = self.connection_manager.subscribe_broadcast();

        // ハートビート設定
        let mut heartbeat_interval = tokio::time::interval(std::time::Duration::from_secs(
            self.config.heartbeat_interval_secs,
        ));

        // メッセージ処理ループ
        loop {
            tokio::select! {
                // クライアントからのメッセージ
                msg = socket.recv() => {
                    match msg {
                        Some(Ok(WsMessage::Text(text))) => {
                            if let Err(e) = self.handle_text_message(
                                &connection_id,
                                &session_id,
                                &text
                            ).await {
                                error!("テキストメッセージ処理エラー: {}", e);
                                break;
                            }
                        }
                        Some(Ok(WsMessage::Binary(data))) => {
                            if let Err(e) = self.handle_binary_message(
                                &connection_id,
                                &session_id,
                                &data
                            ).await {
                                error!("バイナリメッセージ処理エラー: {}", e);
                                break;
                            }
                        }
                        Some(Ok(WsMessage::Close(_))) => {
                            info!("WebSocket接続終了: connection_id={}", connection_id);
                            break;
                        }
                        Some(Ok(WsMessage::Ping(data))) => {
                            if let Err(e) = socket.send(WsMessage::Pong(data)).await {
                                error!("Pong送信エラー: {}", e);
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            error!("WebSocketエラー: {}", e);
                            break;
                        }
                        None => {
                            debug!("WebSocket接続が閉じられました");
                            break;
                        }
                        _ => {}
                    }
                }

                // ブロードキャストメッセージ
                broadcast_msg = broadcast_rx.recv() => {
                    match broadcast_msg {
                        Ok(msg) => {
                            if self.should_forward_broadcast(&msg, &session_id) {
                                let ws_msg = self.convert_broadcast_to_websocket(&msg);
                                if let Err(e) = socket.send(WsMessage::Text(ws_msg.into())).await {
                                    error!("ブロードキャストメッセージ送信エラー: {}", e);
                                    break;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            debug!("ブロードキャストチャンネルが閉じられました");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            warn!("ブロードキャストメッセージが遅延しました");
                        }
                    }
                }

                // ハートビート
                _ = heartbeat_interval.tick() => {
                    if let Err(e) = socket.send(WsMessage::Ping(vec![].into())).await {
                        error!("ハートビート送信エラー: {}", e);
                        break;
                    }
                }
            }
        }

        // 接続を解除
        if let Some(session_id) = session_id {
            if let Err(e) = self
                .connection_manager
                .unregister_connection(&session_id, &connection_id)
                .await
            {
                error!("接続解除エラー: {}", e);
            }
        }

        info!("WebSocket接続処理完了: connection_id={}", connection_id);
    }

    /// テキストメッセージ処理
    async fn handle_text_message(
        &self,
        connection_id: &Uuid,
        session_id: &Option<SessionId>,
        text: &str,
    ) -> Result<(), SessionError> {
        debug!(
            "テキストメッセージ受信: connection_id={}, length={}",
            connection_id,
            text.len()
        );

        // メッセージサイズチェック
        if text.len() > self.config.max_message_size_bytes {
            return Err(SessionError::InvalidState("Message too large".to_string()));
        }

        // JSONパース
        let protocol_msg: WebSocketMessageProtocol =
            serde_json::from_str(text).map_err(SessionError::Serialization)?;

        // セッション入力検証
        if let Some(_session_id) = session_id {
            // セキュリティ検証は必要に応じて実装
            // self.security_middleware.validate_session_input(_session_id, text, "websocket").await?;
        }

        // メッセージタイプ別処理
        match protocol_msg.r#type.as_str() {
            "heartbeat" => {
                self.handle_heartbeat_message(connection_id, &protocol_msg)
                    .await
            }
            "realtime_edit" => {
                self.handle_realtime_edit_message(session_id, &protocol_msg)
                    .await
            }
            "file_sync" => {
                self.handle_file_sync_message(session_id, &protocol_msg)
                    .await
            }
            "chat" => self.handle_chat_message(session_id, &protocol_msg).await,
            "threat_subscription" => {
                self.handle_threat_subscription_message(session_id, &protocol_msg)
                    .await
            }
            _ => {
                warn!("未知のメッセージタイプ: {}", protocol_msg.r#type);
                Ok(())
            }
        }
    }

    /// バイナリメッセージ処理
    async fn handle_binary_message(
        &self,
        connection_id: &Uuid,
        _session_id: &Option<SessionId>,
        data: &[u8],
    ) -> Result<(), SessionError> {
        debug!(
            "バイナリメッセージ受信: connection_id={}, size={}",
            connection_id,
            data.len()
        );

        // バイナリデータの処理（ファイル転送、音声データなど）
        // 実装は用途に応じて詳細化

        Ok(())
    }

    /// ハートビートメッセージ処理
    async fn handle_heartbeat_message(
        &self,
        connection_id: &Uuid,
        _msg: &WebSocketMessageProtocol,
    ) -> Result<(), SessionError> {
        debug!("ハートビート受信: connection_id={}", connection_id);
        // 最終アクティビティ時刻を更新
        Ok(())
    }

    /// リアルタイム編集メッセージ処理
    async fn handle_realtime_edit_message(
        &self,
        session_id: &Option<SessionId>,
        msg: &WebSocketMessageProtocol,
    ) -> Result<(), SessionError> {
        debug!("リアルタイム編集メッセージ: session_id={:?}", session_id);

        // 編集イベントをブロードキャスト
        let broadcast_msg = BroadcastMessage {
            message_id: Uuid::new_v4(),
            sender_session_id: session_id.clone(),
            target_session_id: None, // 全セッションに配信
            message_type: BroadcastMessageType::RealtimeEdit,
            payload: msg.payload.clone(),
            timestamp: Utc::now(),
        };

        self.connection_manager
            .broadcast_message(broadcast_msg)
            .await?;
        Ok(())
    }

    /// ファイル同期メッセージ処理
    async fn handle_file_sync_message(
        &self,
        session_id: &Option<SessionId>,
        msg: &WebSocketMessageProtocol,
    ) -> Result<(), SessionError> {
        debug!("ファイル同期メッセージ: session_id={:?}", session_id);

        let broadcast_msg = BroadcastMessage {
            message_id: Uuid::new_v4(),
            sender_session_id: session_id.clone(),
            target_session_id: None,
            message_type: BroadcastMessageType::FileSync,
            payload: msg.payload.clone(),
            timestamp: Utc::now(),
        };

        self.connection_manager
            .broadcast_message(broadcast_msg)
            .await?;
        Ok(())
    }

    /// チャットメッセージ処理
    async fn handle_chat_message(
        &self,
        session_id: &Option<SessionId>,
        msg: &WebSocketMessageProtocol,
    ) -> Result<(), SessionError> {
        debug!("チャットメッセージ: session_id={:?}", session_id);

        let broadcast_msg = BroadcastMessage {
            message_id: Uuid::new_v4(),
            sender_session_id: session_id.clone(),
            target_session_id: None,
            message_type: BroadcastMessageType::ChatMessage,
            payload: msg.payload.clone(),
            timestamp: Utc::now(),
        };

        self.connection_manager
            .broadcast_message(broadcast_msg)
            .await?;
        Ok(())
    }

    /// 脅威サブスクリプションメッセージ処理
    async fn handle_threat_subscription_message(
        &self,
        session_id: &Option<SessionId>,
        msg: &WebSocketMessageProtocol,
    ) -> Result<(), SessionError> {
        debug!(
            "脅威サブスクリプションメッセージ: session_id={:?}",
            session_id
        );

        // 脅威フィード通知として配信
        let broadcast_msg = BroadcastMessage {
            message_id: Uuid::new_v4(),
            sender_session_id: session_id.clone(),
            target_session_id: None,
            message_type: BroadcastMessageType::ThreatNotification,
            payload: msg.payload.clone(),
            timestamp: Utc::now(),
        };

        self.connection_manager
            .broadcast_message(broadcast_msg)
            .await?;
        Ok(())
    }

    /// ヘッダーからセッション抽出
    async fn extract_session_from_headers(
        &self,
        headers: &HeaderMap,
    ) -> Result<Option<SessionId>, axum::response::Response> {
        debug!("ヘッダーからセッション抽出開始");
        debug!(
            "利用可能なヘッダー: {:?}",
            headers.keys().collect::<Vec<_>>()
        );

        // セッションID抽出ロジック（Cookieまたはカスタムヘッダー）
        if let Some(session_header) = headers.get("x-session-id") {
            debug!("x-session-idヘッダー発見: {:?}", session_header);
            if let Ok(session_str) = session_header.to_str() {
                debug!("セッション文字列: {}", session_str);
                return Ok(Some(SessionId::from_string(session_str.to_string())));
            } else {
                error!("x-session-idヘッダーの文字列変換に失敗");
            }
        } else {
            warn!("x-session-idヘッダーが見つかりません");
        }

        Ok(None)
    }

    /// WebSocket用セッション検証
    async fn validate_session_for_websocket(
        &self,
        session_id: &SessionId,
    ) -> Result<bool, SessionError> {
        debug!("セッション取得試行: {:?}", session_id);
        match self.session_manager.get_session(session_id).await? {
            Some(session) => {
                debug!("セッション発見: state={:?}", session.state);
                match session.state {
                    SessionState::Active => {
                        debug!("アクティブセッション確認");
                        Ok(true)
                    }
                    _ => {
                        debug!("非アクティブセッション: {:?}", session.state);
                        Ok(false)
                    }
                }
            }
            None => {
                error!("セッションが見つかりません: {:?}", session_id);
                Ok(false)
            }
        }
    }

    /// 接続メタデータ構築
    fn build_connection_metadata(&self, headers: &HeaderMap) -> ConnectionMetadata {
        let user_agent = headers
            .get("user-agent")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        ConnectionMetadata {
            user_agent,
            connection_type: ConnectionType::Editor, // デフォルト
            client_capabilities: ClientCapabilities::default(),
        }
    }

    /// ブロードキャスト転送判定
    fn should_forward_broadcast(
        &self,
        msg: &BroadcastMessage,
        session_id: &Option<SessionId>,
    ) -> bool {
        // 送信者自身には送信しない
        if let (Some(sender), Some(receiver)) = (&msg.sender_session_id, session_id) {
            if sender == receiver {
                return false;
            }
        }

        // ターゲット指定がある場合はチェック
        if let Some(target) = &msg.target_session_id {
            return session_id.as_ref() == Some(target);
        }

        true
    }

    /// ブロードキャストメッセージをWebSocketメッセージに変換
    fn convert_broadcast_to_websocket(&self, msg: &BroadcastMessage) -> String {
        let ws_msg = WebSocketMessageProtocol {
            id: msg.message_id,
            r#type: format!("{:?}", msg.message_type).to_lowercase(),
            payload: msg.payload.clone(),
            timestamp: msg.timestamp,
            session_info: msg.sender_session_id.as_ref().map(|id| SessionInfo {
                session_id: id.as_str().to_string(),
                user_id: None,
                state: "active".to_string(),
            }),
        };

        serde_json::to_string(&ws_msg).unwrap_or_default()
    }

    /// 拒否レスポンス作成
    async fn create_rejection_response(
        &self,
        reason: &str,
    ) -> Result<Response, axum::response::Response> {
        Err(axum::response::Response::builder()
            .status(axum::http::StatusCode::UNAUTHORIZED)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(
                json!({ "error": reason }).to_string(),
            ))
            .unwrap()
            .into_response())
    }
}

impl Clone for SessionWebSocketHandler {
    fn clone(&self) -> Self {
        Self {
            session_manager: self.session_manager.clone(),
            security_middleware: self.security_middleware.clone(),
            connection_manager: self.connection_manager.clone(),
            config: self.config.clone(),
        }
    }
}

impl WebSocketConnectionManager {
    fn new(broadcast_tx: broadcast::Sender<BroadcastMessage>) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            broadcast_tx,
        }
    }

    async fn register_connection(
        &self,
        connection: WebSocketConnection,
    ) -> Result<(), SessionError> {
        let mut connections = self.connections.write().unwrap();
        connections
            .entry(connection.session_id.clone())
            .or_default()
            .push(connection);
        Ok(())
    }

    async fn unregister_connection(
        &self,
        session_id: &SessionId,
        connection_id: &Uuid,
    ) -> Result<(), SessionError> {
        let mut connections = self.connections.write().unwrap();
        if let Some(session_connections) = connections.get_mut(session_id) {
            session_connections.retain(|conn| conn.connection_id != *connection_id);
            if session_connections.is_empty() {
                connections.remove(session_id);
            }
        }
        Ok(())
    }

    async fn broadcast_message(&self, message: BroadcastMessage) -> Result<(), SessionError> {
        self.broadcast_tx
            .send(message)
            .map_err(|e| SessionError::Internal(format!("Broadcast failed: {}", e)))?;
        Ok(())
    }

    fn subscribe_broadcast(&self) -> broadcast::Receiver<BroadcastMessage> {
        self.broadcast_tx.subscribe()
    }
}

impl Default for WebSocketHandlerConfig {
    fn default() -> Self {
        Self {
            require_session: true,
            max_connections_per_session: 10,
            heartbeat_interval_secs: 30,
            connection_timeout_secs: 300,
            max_message_size_bytes: 1024 * 1024, // 1MB
            max_messages_per_second: 100,
        }
    }
}

impl Default for ClientCapabilities {
    fn default() -> Self {
        Self {
            supports_realtime_editing: true,
            supports_file_sync: true,
            supports_voice_chat: false,
            supports_screen_share: false,
            supports_collaboration: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_handler_config_default() {
        let config = WebSocketHandlerConfig::default();
        assert!(config.require_session);
        assert_eq!(config.max_connections_per_session, 10);
        assert_eq!(config.heartbeat_interval_secs, 30);
    }

    #[test]
    fn test_client_capabilities_default() {
        let capabilities = ClientCapabilities::default();
        assert!(capabilities.supports_realtime_editing);
        assert!(capabilities.supports_file_sync);
        assert!(!capabilities.supports_voice_chat);
    }
}
