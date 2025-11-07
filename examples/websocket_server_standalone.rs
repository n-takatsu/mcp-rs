//! Axum WebSocket Server for Real-time Editing
//!
//! 実際のWebSocketサーバー実装

use crate::session::{
    SecurityConfig, SessionId, SessionManager, SessionMiddleware, SessionSecurityMiddleware,
    SessionState, SessionWebSocketHandler, WebSocketHandlerConfig,
};
use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    middleware,
    response::{Html, Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{debug, error, info};
use uuid::Uuid;

/// Axum WebSocketサーバー
#[derive(Debug, Clone)]
pub struct AxumWebSocketServer {
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// WebSocketハンドラー
    websocket_handler: Arc<SessionWebSocketHandler>,
    /// セッションミドルウェア
    session_middleware: Arc<SessionMiddleware>,
    /// サーバー設定
    config: ServerConfig,
}

/// サーバー設定
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// サーバーアドレス
    pub bind_addr: SocketAddr,
    /// 静的ファイルのパス
    pub static_path: Option<String>,
    /// CORS設定
    pub enable_cors: bool,
    /// ログ設定
    pub enable_tracing: bool,
}

/// セッション作成リクエスト
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub user_id: String,
    pub client_info: Option<String>,
}

/// セッション作成レスポンス
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
    pub state: String,
    pub websocket_url: String,
}

/// セッション情報レスポンス
#[derive(Debug, Serialize)]
pub struct SessionInfoResponse {
    pub session_id: String,
    pub user_id: String,
    pub state: String,
    pub created_at: String,
    pub expires_at: String,
}

/// WebSocket接続クエリパラメータ
#[derive(Debug, Deserialize)]
pub struct WebSocketQuery {
    pub session_id: Option<String>,
    pub client_type: Option<String>,
}

impl AxumWebSocketServer {
    /// 新しいサーバーを作成
    pub fn new(config: ServerConfig) -> Self {
        let session_manager = Arc::new(SessionManager::new());
        let security_middleware =
            Arc::new(SessionSecurityMiddleware::new(SecurityConfig::default()));
        let websocket_handler = Arc::new(SessionWebSocketHandler::new(
            session_manager.clone(),
            security_middleware,
            WebSocketHandlerConfig::default(),
        ));
        let session_middleware = Arc::new(SessionMiddleware::new(session_manager.clone()));

        Self {
            session_manager,
            websocket_handler,
            session_middleware,
            config,
        }
    }

    /// サーバー起動
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 リアルタイム編集WebSocketサーバー開始");
        info!("📡 バインドアドレス: {}", self.config.bind_addr);

        let app = self.create_router();

        let listener = TcpListener::bind(&self.config.bind_addr).await?;
        info!("🌐 サーバー起動完了: http://{}", self.config.bind_addr);

        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    }

    /// ルーター作成
    fn create_router(&self) -> Router {
        let mut app = Router::new()
            // WebSocketエンドポイント
            .route("/ws", get(Self::websocket_handler))
            // セッション管理API
            .route("/api/sessions", post(Self::create_session))
            .route("/api/sessions/:session_id", get(Self::get_session))
            .route(
                "/api/sessions/:session_id/activate",
                post(Self::activate_session),
            )
            // 健全性チェック
            .route("/health", get(Self::health_check))
            // デモ用静的ページ
            .route("/", get(Self::demo_page))
            .with_state(self.clone());

        // ミドルウェア設定
        let service_builder = ServiceBuilder::new();

        if self.config.enable_tracing {
            app = app.layer(service_builder.layer(TraceLayer::new_for_http()));
        }

        if self.config.enable_cors {
            app = app.layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            );
        }

        app
    }

    /// WebSocketハンドラー
    async fn websocket_handler(
        ws: WebSocketUpgrade,
        ConnectInfo(addr): ConnectInfo<SocketAddr>,
        Query(params): Query<WebSocketQuery>,
        mut headers: HeaderMap,
        State(server): State<AxumWebSocketServer>,
    ) -> Result<Response, StatusCode> {
        info!("WebSocket接続要求: addr={}, params={:?}", addr, params);

        // クエリパラメータからセッションIDをヘッダーに追加
        if let Some(session_id) = params.session_id {
            headers.insert(
                "x-session-id",
                session_id.parse().map_err(|_| StatusCode::BAD_REQUEST)?,
            );
        }

        match server
            .websocket_handler
            .handle_websocket_connection(ws, headers, Some(addr))
            .await
        {
            Ok(response) => Ok(response),
            Err(_response) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    /// セッション作成
    async fn create_session(
        State(server): State<AxumWebSocketServer>,
        Json(request): Json<CreateSessionRequest>,
    ) -> Result<Json<CreateSessionResponse>, StatusCode> {
        debug!("セッション作成要求: user_id={}", request.user_id);

        match server
            .session_manager
            .create_session(request.user_id.clone())
            .await
        {
            Ok(session) => {
                // セッションを自動的にアクティベート
                let activated_session =
                    match server.session_manager.activate_session(&session.id).await {
                        Ok(Some(active_session)) => active_session,
                        _ => {
                            error!(
                                "セッションアクティベート失敗: session_id={}",
                                session.id.as_str()
                            );
                            return Err(StatusCode::INTERNAL_SERVER_ERROR);
                        }
                    };

                let response = CreateSessionResponse {
                    session_id: activated_session.id.as_str().to_string(),
                    state: format!("{:?}", activated_session.state),
                    websocket_url: format!(
                        "ws://{}/ws?session_id={}",
                        server.config.bind_addr,
                        activated_session.id.as_str()
                    ),
                };
                info!(
                    "セッション作成・アクティベート成功: session_id={}",
                    activated_session.id.as_str()
                );
                Ok(Json(response))
            }
            Err(e) => {
                error!("セッション作成失敗: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    /// セッション情報取得
    async fn get_session(
        Path(session_id): Path<String>,
        State(server): State<AxumWebSocketServer>,
    ) -> Result<Json<SessionInfoResponse>, StatusCode> {
        let session_id = SessionId::from_string(session_id);

        match server.session_manager.get_session(&session_id).await {
            Ok(Some(session)) => {
                let response = SessionInfoResponse {
                    session_id: session.id.as_str().to_string(),
                    user_id: session.user_id.clone(),
                    state: format!("{:?}", session.state),
                    created_at: session.created_at.to_rfc3339(),
                    expires_at: session.expires_at.to_rfc3339(),
                };
                Ok(Json(response))
            }
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(e) => {
                error!("セッション取得失敗: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    /// セッションアクティベート
    async fn activate_session(
        Path(session_id): Path<String>,
        State(server): State<AxumWebSocketServer>,
    ) -> Result<Json<SessionInfoResponse>, StatusCode> {
        let session_id = SessionId::from_string(session_id);

        match server.session_manager.activate_session(&session_id).await {
            Ok(Some(session)) => {
                let response = SessionInfoResponse {
                    session_id: session.id.as_str().to_string(),
                    user_id: session.user_id.clone(),
                    state: format!("{:?}", session.state),
                    created_at: session.created_at.to_rfc3339(),
                    expires_at: session.expires_at.to_rfc3339(),
                };
                info!(
                    "セッションアクティベート成功: session_id={}",
                    session.id.as_str()
                );
                Ok(Json(response))
            }
            Ok(None) => Err(StatusCode::NOT_FOUND),
            Err(e) => {
                error!("セッションアクティベート失敗: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }

    /// 健全性チェック
    async fn health_check() -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "status": "healthy",
            "service": "mcp-rs-realtime-editing",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "version": env!("CARGO_PKG_VERSION")
        }))
    }

    /// デモページ
    async fn demo_page() -> Html<&'static str> {
        Html(include_str!("../../static/demo.html"))
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:3000".parse().unwrap(),
            static_path: None,
            enable_cors: true,
            enable_tracing: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_server_creation() {
        let config = ServerConfig::default();
        let server = AxumWebSocketServer::new(config);

        // サーバーが正常に作成されることを確認
        assert_eq!(server.config.bind_addr.port(), 3000);
        assert!(server.config.enable_cors);
    }

    #[tokio::test]
    async fn test_health_check() {
        let response = AxumWebSocketServer::health_check().await;
        let json_value = response.0;

        assert_eq!(json_value["status"], "healthy");
        assert_eq!(json_value["service"], "mcp-rs-realtime-editing");
    }
}
