//! 認証API統合デモ
//!
//! PostgreSQL + Redis + JWT認証APIの完全動作デモ
//! - ユーザー登録
//! - ログイン/ログアウト
//! - トークンリフレッシュ
//! - セッション管理

use mcp_rs::security::auth::{
    create_auth_router, AuthApiState, JwtAuth, JwtConfig, MultiAuthProvider,
    InMemoryUserRepository, UserRepository,
};
use std::sync::Arc;

#[cfg(feature = "redis-backend")]
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ロギング初期化
    env_logger::init();

    println!("=== MCP-RS 認証API統合デモ ===\n");

    // リポジトリ作成（In-Memory版）
    let repository: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepository::new());
    println!("✓ UserRepository作成完了 (In-Memory)");

    // JWT設定
    let jwt_config = JwtConfig {
        secret: "demo-secret-key-change-in-production".to_string(),
        access_token_expiration: 3600,  // 1時間
        refresh_token_expiration: 86400, // 24時間
        issuer: "mcp-rs-demo".to_string(),
        audience: Some("mcp-rs-api".to_string()),
        algorithm: "HS256".to_string(),
    };
    let jwt_auth = Arc::new(JwtAuth::new(jwt_config.clone()));
    println!("✓ JWT認証ハンドラー作成完了");

    // MultiAuthProvider作成
    let provider = Arc::new(MultiAuthProvider::new(
        Some(jwt_config),
        None, // OAuth2なし
        None, // API Keyなし
        None, // MFAなし
        12,   // Argon2 cost
        repository.clone(),
    ));
    println!("✓ MultiAuthProvider作成完了");

    // Redis セッションストア（オプション）
    #[cfg(feature = "redis-backend")]
    let session_store = {
        use mcp_rs::security::auth::RedisSessionStore;
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        
        match RedisSessionStore::new(&redis_url, 3600, "demo_session:".to_string()).await {
            Ok(store) => {
                println!("✓ Redis セッションストア作成完了");
                Some(Arc::new(RwLock::new(store)))
            }
            Err(e) => {
                println!("⚠ Redis接続失敗（スキップ）: {}", e);
                None
            }
        }
    };

    #[cfg(not(feature = "redis-backend"))]
    let session_store: Option<Arc<tokio::sync::RwLock<()>>> = None;

    // API状態作成
    let state = AuthApiState {
        provider: provider.clone(),
        jwt_auth: jwt_auth.clone(),
        #[cfg(feature = "redis-backend")]
        session_store,
    };

    println!("✓ API状態作成完了\n");

    // ルーター作成
    let app = create_auth_router(state);
    println!("✓ 認証APIルーター作成完了");
    println!("\n利用可能なエンドポイント:");
    println!("  POST /register  - ユーザー登録");
    println!("  POST /login     - ログイン");
    println!("  POST /refresh   - トークンリフレッシュ");
    println!("  POST /logout    - ログアウト");
    println!("  GET  /me        - 現在のユーザー情報");

    // サーバー起動
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await?;
    let addr = listener.local_addr()?;
    
    println!("\n🚀 認証APIサーバー起動: http://{}", addr);
    println!("\n使用例:");
    println!("  # ユーザー登録");
    println!(r#"  curl -X POST http://{}/register \"#, addr);
    println!(r#"    -H "Content-Type: application/json" \"#);
    println!(r#"    -d '{{"username":"demo","password":"SecurePass123!","email":"demo@example.com"}}'"#);
    println!("\n  # ログイン");
    println!(r#"  curl -X POST http://{}/login \"#, addr);
    println!(r#"    -H "Content-Type: application/json" \"#);
    println!(r#"    -d '{{"email":"demo@example.com","password":"SecurePass123!","remember_me":false}}'"#);
    println!("\nCtrl+C で終了");

    axum::serve(listener, app).await?;

    Ok(())
}
