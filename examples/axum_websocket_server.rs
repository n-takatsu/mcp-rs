//! Axum WebSocket Server Example
//!
//! リアルタイム編集WebSocketサーバーの実行例

use mcp_rs::{AxumWebSocketServer, ServerConfig};
use std::net::SocketAddr;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // ログ初期化
    tracing_subscriber::fmt()
        .with_env_filter("info,mcp_rs=debug")
        .init();

    println!("🚀 MCP-RS リアルタイム編集WebSocketサーバー");

    // サーバー設定
    let config = ServerConfig {
        bind_addr: "0.0.0.0:3000".parse::<SocketAddr>()?,
        static_path: None,
        enable_cors: true,
        enable_tracing: true,
    };

    // サーバー作成と起動
    let server = AxumWebSocketServer::new(config);

    println!("🌐 デモページ: http://localhost:3000/");
    println!("📡 WebSocket: ws://localhost:3000/ws");
    println!("🔧 API: http://localhost:3000/api/sessions");
    println!("💚 Health: http://localhost:3000/health");
    println!("📝 Ctrl+C で停止");

    server.start().await?;

    Ok(())
}
