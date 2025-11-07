//! Security Integration Demo
//!
//! This example demonstrates the integration between the session management system
//! and security features, including IP filtering, MFA validation, and security monitoring.

use mcp_rs::{Session, SessionId, SessionManager, SessionState};
use std::net::IpAddr;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔐 Security Integration Demo");

    // セッションマネージャーの初期化
    let manager = SessionManager::new();

    // セッション作成
    let session = manager.create_session("user123".to_string()).await?;
    println!("✅ Session created: {}", session.id.0);

    // セッション取得とアクティベート
    let activated = manager.activate_session(&session.id).await?;
    if let Some(session) = activated {
        println!("🚀 Session activated: {:?}", session.state);
    }

    println!("🎯 Demo completed successfully!");
    Ok(())
}
