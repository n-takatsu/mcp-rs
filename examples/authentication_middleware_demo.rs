//! 認証ミドルウェア統合デモ
//!
//! 既存APIエンドポイントに認証を適用する完全な例
//! - 認証必須エンドポイント
//! - オプショナル認証エンドポイント
//! - ロールベースアクセス制御
//! - 認証APIとの統合

use axum::{
    extract::{Json, Request},
    http::StatusCode,
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use mcp_rs::security::auth::{
    create_auth_router, AuthApiState, AuthMiddleware, AuthRequirement,
    InMemoryUserRepository, JwtAuth, JwtConfig, MultiAuthProvider, Role,
    UserRepository,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ============================================================================
// サンプルデータモデル
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
struct Post {
    id: String,
    title: String,
    content: String,
    author_id: String,
    published: bool,
}

#[derive(Debug, Deserialize)]
struct CreatePostRequest {
    title: String,
    content: String,
}

// ============================================================================
// 公開エンドポイント（認証不要）
// ============================================================================

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "healthy",
        "service": "mcp-rs-api"
    })))
}

async fn public_posts() -> impl IntoResponse {
    let posts = vec![
        Post {
            id: "1".to_string(),
            title: "公開投稿".to_string(),
            content: "誰でも見られる内容".to_string(),
            author_id: "system".to_string(),
            published: true,
        },
    ];

    (StatusCode::OK, Json(posts))
}

// ============================================================================
// オプショナル認証エンドポイント
// ============================================================================

/// 認証されている場合はユーザー名を表示、されていない場合は"Guest"
async fn welcome(request: Request) -> impl IntoResponse {
    let user = request.extensions().get::<mcp_rs::security::auth::AuthUser>();
    let username = user
        .as_ref()
        .map(|u| u.username.as_str())
        .unwrap_or("Guest");

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "message": format!("Welcome, {}!", username),
            "authenticated": user.is_some()
        })),
    )
}

// ============================================================================
// 認証必須エンドポイント
// ============================================================================

/// 現在のユーザー情報を取得（/auth/meと同等）
async fn current_user_profile(request: Request) -> impl IntoResponse {
    if let Some(user) = request.extensions().get::<mcp_rs::security::auth::AuthUser>() {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "id": user.id,
                "username": user.username,
                "email": user.email,
                "roles": user.roles,
                "permissions": user.permissions,
            })),
        )
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Unauthorized"
            })),
        )
    }
}

/// ユーザーの投稿一覧を取得
async fn my_posts(request: Request) -> impl IntoResponse {
    if let Some(user) = request.extensions().get::<mcp_rs::security::auth::AuthUser>() {
        let posts = vec![
            Post {
                id: "2".to_string(),
                title: format!("{}の投稿", user.username),
                content: "認証済みユーザーの投稿".to_string(),
                author_id: user.id.clone(),
                published: true,
            },
        ];

        (StatusCode::OK, Json(posts))
    } else {
        (StatusCode::UNAUTHORIZED, Json(vec![]))
    }
}

/// 新しい投稿を作成
async fn create_post(
    request: Request,
    Json(req): Json<CreatePostRequest>,
) -> impl IntoResponse {
    if let Some(user) = request.extensions().get::<mcp_rs::security::auth::AuthUser>() {
        let post = Post {
            id: uuid::Uuid::new_v4().to_string(),
            title: req.title,
            content: req.content,
            author_id: user.id.clone(),
            published: false,
        };

        (StatusCode::CREATED, Json(post))
    } else {
        let empty_post = Post {
            id: String::new(),
            title: String::new(),
            content: String::new(),
            author_id: String::new(),
            published: false,
        };
        (StatusCode::UNAUTHORIZED, Json(empty_post))
    }
}

// ============================================================================
// 管理者専用エンドポイント
// ============================================================================

/// 全ユーザーの投稿を取得（管理者のみ）
async fn admin_all_posts(request: Request) -> impl IntoResponse {
    if let Some(user) = request.extensions().get::<mcp_rs::security::auth::AuthUser>() {
        let posts = vec![
            Post {
                id: "999".to_string(),
                title: "管理者専用データ".to_string(),
                content: format!("閲覧者: {}", user.username),
                author_id: "admin".to_string(),
                published: true,
            },
        ];

        (StatusCode::OK, Json(posts))
    } else {
        (StatusCode::UNAUTHORIZED, Json(vec![]))
    }
}

/// システム統計を取得（管理者のみ）
async fn admin_stats(request: Request) -> impl IntoResponse {
    if let Some(user) = request.extensions().get::<mcp_rs::security::auth::AuthUser>() {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "total_users": 42,
                "total_posts": 128,
                "requested_by": user.username
            })),
        )
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Unauthorized"
            })),
        )
    }
}

// ============================================================================
// アプリケーションセットアップ
// ============================================================================

async fn create_app_state() -> (AuthApiState, Arc<MultiAuthProvider>) {
    // リポジトリ作成
    let repository: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepository::new());

    // JWT設定
    let jwt_config = JwtConfig {
        secret: "demo-secret-key-change-in-production".to_string(),
        access_token_expiration: 3600,
        refresh_token_expiration: 86400,
        issuer: "mcp-rs-middleware-demo".to_string(),
        audience: Some("mcp-rs-api".to_string()),
        algorithm: "HS256".to_string(),
    };

    let jwt_auth = Arc::new(JwtAuth::new(jwt_config.clone()));

    // MultiAuthProvider作成
    let provider = Arc::new(MultiAuthProvider::new(
        Some(jwt_config),
        None,
        None,
        None,
        12,
        repository.clone(),
    ));

    let auth_state = AuthApiState {
        provider: provider.clone(),
        jwt_auth,
        #[cfg(feature = "redis-backend")]
        session_store: None,
    };

    (auth_state, provider)
}

fn create_app(auth_state: AuthApiState, provider: Arc<MultiAuthProvider>) -> Router {
    // 公開ルート（認証不要）
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/posts", get(public_posts))
        .route("/welcome", get(welcome)); // オプショナル認証

    // 認証APIルート
    let auth_routes = create_auth_router(auth_state);

    // 認証必須ルート
    let protected_routes = Router::new()
        .route("/me", get(current_user_profile))
        .route("/my-posts", get(my_posts))
        .route("/posts", post(create_post))
        .layer(middleware::from_fn_with_state(
            provider.clone(),
            |state, request, next| async move {
                AuthMiddleware::new(state, AuthRequirement::Required)
                    .handle(request, next)
                    .await
            },
        ));

    // 管理者専用ルート
    let admin_routes = Router::new()
        .route("/posts", get(admin_all_posts))
        .route("/stats", get(admin_stats))
        .layer(middleware::from_fn_with_state(
            provider.clone(),
            |state, request, next| async move {
                AuthMiddleware::new(state, AuthRequirement::Role(Role::Admin))
                    .handle(request, next)
                    .await
            },
        ));

    // 全ルート統合
    Router::new()
        .nest("/api", public_routes)
        .nest("/auth", auth_routes)
        .nest("/api/user", protected_routes)
        .nest("/api/admin", admin_routes)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    println!("=== MCP-RS 認証ミドルウェア統合デモ ===\n");

    let (auth_state, provider) = create_app_state().await;
    println!("✓ 認証システム初期化完了\n");

    let app = create_app(auth_state, provider);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002").await?;
    let addr = listener.local_addr()?;

    println!("🚀 サーバー起動: http://{}\n", addr);
    println!("エンドポイント一覧:");
    println!("\n📖 公開エンドポイント（認証不要）:");
    println!("  GET  http://{}/api/health", addr);
    println!("  GET  http://{}/api/posts", addr);
    println!("  GET  http://{}/api/welcome", addr);

    println!("\n🔐 認証API:");
    println!("  POST http://{}/auth/register", addr);
    println!("  POST http://{}/auth/login", addr);
    println!("  POST http://{}/auth/refresh", addr);
    println!("  POST http://{}/auth/logout", addr);
    println!("  GET  http://{}/auth/me", addr);

    println!("\n🔒 認証必須エンドポイント:");
    println!("  GET  http://{}/api/user/me", addr);
    println!("  GET  http://{}/api/user/my-posts", addr);
    println!("  POST http://{}/api/user/posts", addr);

    println!("\n👑 管理者専用エンドポイント:");
    println!("  GET  http://{}/api/admin/posts", addr);
    println!("  GET  http://{}/api/admin/stats", addr);

    println!("\n使用例:");
    println!("\n1. ユーザー登録:");
    println!(r#"curl -X POST http://{}/auth/register \"#, addr);
    println!(r#"  -H "Content-Type: application/json" \"#);
    println!(r#"  -d '{{"username":"alice","password":"SecurePass123!","email":"alice@example.com"}}'"#);

    println!("\n2. ログイン:");
    println!(r#"curl -X POST http://{}/auth/login \"#, addr);
    println!(r#"  -H "Content-Type: application/json" \"#);
    println!(r#"  -d '{{"email":"alice@example.com","password":"SecurePass123!","remember_me":false}}'"#);

    println!("\n3. 認証必須エンドポイントへアクセス:");
    println!(r#"TOKEN="<access_token from login response>""#);
    println!(r#"curl http://{}/api/user/me \"#, addr);
    println!(r#"  -H "Authorization: Bearer $TOKEN""#);

    println!("\n4. 投稿作成:");
    println!(r#"curl -X POST http://{}/api/user/posts \"#, addr);
    println!(r#"  -H "Authorization: Bearer $TOKEN" \"#);
    println!(r#"  -H "Content-Type: application/json" \"#);
    println!(r#"  -d '{{"title":"My Post","content":"Hello, world!"}}'"#);

    println!("\nCtrl+C で終了");

    axum::serve(listener, app).await?;

    Ok(())
}
