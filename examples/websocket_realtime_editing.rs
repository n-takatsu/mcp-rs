//! WebSocket Real-time Editing Demo
//!
//! This example demonstrates real-time collaborative editing capabilities
//! using WebSocket connections and session management.

use mcp_rs::{
    SecurityConfig, SessionId, SessionManager, SessionSecurityMiddleware, SessionState,
    SessionWebSocketHandler, WebSocketHandlerConfig,
};
use std::sync::Arc;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌐 WebSocket Real-time Editing Demo");

    // セッションマネージャーの初期化
    let manager = Arc::new(SessionManager::new());

    // 複数のユーザーセッションを作成
    let user1_session = manager.create_session("user1".to_string()).await?;
    let user2_session = manager.create_session("user2".to_string()).await?;

    println!("👤 User1 session: {}", user1_session.id.as_str());
    println!("👤 User2 session: {}", user2_session.id.as_str());

    // セッションをアクティブ化
    manager.activate_session(&user1_session.id).await?;
    manager.activate_session(&user2_session.id).await?;

    // セキュリティミドルウェアの初期化
    let security_middleware = Arc::new(SessionSecurityMiddleware::new(SecurityConfig::default()));

    // WebSocketハンドラーの初期化
    let ws_handler = SessionWebSocketHandler::new(
        manager.clone(),
        security_middleware,
        WebSocketHandlerConfig::default(),
    );

    println!("🔄 Real-time editing system initialized!");
    println!("📡 WebSocket handler ready for connections");
    println!("🔒 Security middleware enabled");
    println!("✅ Both users are now ready for collaborative editing");

    // 実際のWebSocketサーバーはaxumと組み合わせて使用
    println!("💡 To start the WebSocket server, integrate with axum web framework");

    Ok(())
}
