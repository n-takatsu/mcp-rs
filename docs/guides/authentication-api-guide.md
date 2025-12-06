# 認証API統合ガイド

## 概要

MCP-RSの認証APIは、PostgreSQL、Redis、JWTを統合した本番環境対応の認証システムを提供します。

## 機能

### 実装済みエンドポイント

| エンドポイント | メソッド | 説明 |
|---------------|---------|------|
| `/auth/register` | POST | ユーザー登録 |
| `/auth/login` | POST | ログイン（JWT + セッション発行） |
| `/auth/refresh` | POST | トークンリフレッシュ |
| `/auth/logout` | POST | ログアウト |
| `/auth/me` | GET | 現在のユーザー情報取得 |

## セットアップ

### 1. 基本設定

```rust
use mcp_rs::security::auth::{
    create_auth_router, AuthApiState, JwtAuth, JwtConfig,
    MultiAuthProvider, InMemoryUserRepository, UserRepository,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // リポジトリ作成
    let repository: Arc<dyn UserRepository> = 
        Arc::new(InMemoryUserRepository::new());

    // JWT設定
    let jwt_config = JwtConfig {
        secret: std::env::var("JWT_SECRET")?,
        access_token_expiration: 3600,    // 1時間
        refresh_token_expiration: 86400,  // 24時間
        issuer: "mcp-rs".to_string(),
        audience: Some("mcp-rs-api".to_string()),
        algorithm: "HS256".to_string(),
    };

    let jwt_auth = Arc::new(JwtAuth::new(jwt_config.clone()));

    // 認証プロバイダー作成
    let provider = Arc::new(MultiAuthProvider::new(
        Some(jwt_config),
        None,  // OAuth2
        None,  // API Key
        None,  // MFA
        12,    // Argon2 cost
        repository,
    ));

    // API状態作成
    let state = AuthApiState {
        provider,
        jwt_auth,
        #[cfg(feature = "redis-backend")]
        session_store: None,
    };

    // ルーター作成
    let app = create_auth_router(state);

    // サーバー起動
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
```

### 2. PostgreSQL統合

```rust
#[cfg(feature = "postgresql-backend")]
use mcp_rs::security::auth::PostgresUserRepository;

let database_url = std::env::var("DATABASE_URL")?;
let repository: Arc<dyn UserRepository> = Arc::new(
    PostgresUserRepository::new(&database_url).await?
);

// マイグレーション実行
PostgresUserRepository::run_migrations(&database_url).await?;
```

### 3. Redis統合（セッション管理）

```rust
#[cfg(feature = "redis-backend")]
use mcp_rs::security::auth::RedisSessionStore;
use tokio::sync::RwLock;

let redis_url = std::env::var("REDIS_URL")?;
let session_store = Arc::new(RwLock::new(
    RedisSessionStore::new(&redis_url, 3600, "session:".to_string()).await?
));

let state = AuthApiState {
    provider,
    jwt_auth,
    #[cfg(feature = "redis-backend")]
    session_store: Some(session_store),
};
```

## API使用方法

### ユーザー登録

```bash
curl -X POST http://localhost:3000/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "password": "SecurePass123!",
    "email": "alice@example.com"
  }'
```

**レスポンス例:**

```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000",
  "username": "alice",
  "email": "alice@example.com",
  "message": "User registered successfully"
}
```

### ログイン

```bash
curl -X POST http://localhost:3000/login \
  -H "Content-Type: application/json" \
  -d '{
    "email": "alice@example.com",
    "password": "SecurePass123!",
    "remember_me": false
  }'
```

**レスポンス例:**

```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "alice",
    "email": "alice@example.com",
    "roles": ["User"]
  }
}
```

### トークンリフレッシュ

```bash
curl -X POST http://localhost:3000/refresh \
  -H "Content-Type: application/json" \
  -d '{
    "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
  }'
```

**レスポンス例:**

```json
{
  "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "refresh_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "token_type": "Bearer",
  "expires_in": 3600
}
```

### ログアウト

```bash
curl -X POST http://localhost:3000/logout \
  -H "Authorization: Bearer <access_token>"
```

**レスポンス例:**

```json
{
  "message": "Logged out successfully"
}
```

### ユーザー情報取得

```bash
curl -X GET http://localhost:3000/me \
  -H "Authorization: Bearer <access_token>"
```

**レスポンス例:**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "username": "alice",
  "email": "alice@example.com",
  "roles": ["User"]
}
```

## セキュリティ

### パスワード要件

- 最小長: 8文字
- 最大長: 128文字
- 必須: 大文字、小文字、数字、特殊文字

### トークンセキュリティ

- アクセストークン: 短期間有効（デフォルト: 1時間）
- リフレッシュトークン: 長期間有効（デフォルト: 24時間）
- HS256アルゴリズム使用

### セッション管理（Redis有効時）

- TTL（Time To Live）自動管理
- `remember_me`フラグでTTL延長（30日）
- セッション削除機能

## エラーハンドリング

### エラーレスポンス形式

```json
{
  "error": "ErrorType",
  "message": "Detailed error message"
}
```

### HTTPステータスコード

| コード | 説明 |
|-------|------|
| 200 OK | 成功 |
| 201 Created | ユーザー登録成功 |
| 400 Bad Request | 不正なリクエスト（弱いパスワードなど） |
| 401 Unauthorized | 認証失敗 |
| 404 Not Found | リソース未検出 |
| 409 Conflict | ユーザー重複 |
| 500 Internal Server Error | サーバーエラー |
| 501 Not Implemented | 未実装機能 |

## パフォーマンス最適化

### データベース

```rust
// コネクションプール設定
let pool = PgPoolOptions::new()
    .max_connections(20)
    .min_connections(5)
    .acquire_timeout(Duration::from_secs(30))
    .connect(&database_url)
    .await?;
```

### Redis

```rust
// ConnectionManager使用（自動再接続）
let client = redis::Client::open(redis_url)?;
let manager = ConnectionManager::new(client).await?;
```

## 統合例

### 既存Axumアプリへの統合

```rust
use axum::Router;
use mcp_rs::security::auth::create_auth_router;

async fn create_app(auth_state: AuthApiState) -> Router {
    Router::new()
        // 認証APIをマウント
        .nest("/auth", create_auth_router(auth_state))
        // 他のルート
        .route("/api/health", get(health_check))
        .route("/api/users", get(list_users))
}
```

### 認証ミドルウェアの適用

```rust
use mcp_rs::security::auth::{AuthMiddleware, AuthRequirement};

let protected_routes = Router::new()
    .route("/api/admin", get(admin_only))
    .layer(AuthMiddleware::new(
        jwt_auth.clone(),
        AuthRequirement::Authenticated,
    ));
```

## トラブルシューティング

### PostgreSQL接続エラー

```bash
# DATABASE_URL環境変数を確認
export DATABASE_URL="postgresql://user:password@localhost:5432/mcp_rs"

# マイグレーション実行
cargo run --example postgres_migration
```

### Redis接続エラー

```bash
# REDIS_URL環境変数を確認
export REDIS_URL="redis://localhost:6379"

# Redis接続テスト
redis-cli ping
```

### JWT検証エラー

- JWT_SECRET環境変数が設定されているか確認
- トークンの有効期限が切れていないか確認
- アルゴリズムが一致しているか確認（HS256）

## テスト

```bash
# 単体テスト
cargo test --lib auth::api

# 統合テスト
cargo test --features "postgresql-backend,redis-backend"

# デモアプリ起動
cargo run --example authentication_api_demo
```

## 本番環境デプロイ

### 環境変数

```bash
export JWT_SECRET="your-secure-secret-key-min-32-chars"
export DATABASE_URL="postgresql://user:password@localhost:5432/mcp_rs"
export REDIS_URL="redis://localhost:6379"
```

### TLS/SSL設定

```rust
use axum_server::tls_rustls::RustlsConfig;

let config = RustlsConfig::from_pem_file("cert.pem", "key.pem").await?;
axum_server::bind_rustls(addr, config)
    .serve(app.into_make_service())
    .await?;
```

### ロギング

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
    ))
    .with(tracing_subscriber::fmt::layer())
    .init();
```

## 参照

- [PostgreSQL認証ガイド](./postgresql-authentication-guide.md)
- [Redisセッションガイド](./redis-session-guide.md)
- [JWT設定リファレンス](../api/jwt-config.md)
- [MultiAuthProviderガイド](../api/multi-auth-provider.md)
