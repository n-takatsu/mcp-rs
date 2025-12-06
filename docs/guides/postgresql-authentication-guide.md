# PostgreSQL認証リポジトリ 使用ガイド

PostgreSQL実装の認証システム使用方法を説明します。

## 目次

1. [セットアップ](#セットアップ)
2. [基本的な使い方](#基本的な使い方)
3. [MultiAuthProviderとの統合](#multiauthproviderとの統合)
4. [本番環境設定](#本番環境設定)
5. [パフォーマンスチューニング](#パフォーマンスチューニング)

## セットアップ

### 1. 依存関係

`Cargo.toml`でPostgreSQL機能を有効化：

```toml
[dependencies]
mcp-rs = { version = "0.15", features = ["postgresql-backend"] }
```

### 2. データベース準備

```bash
# PostgreSQLインストール（Ubuntu/Debian）
sudo apt install postgresql postgresql-contrib

# データベース作成
sudo -u postgres createdb mcp_rs

# マイグレーション実行
cd /path/to/mcp-rs
sqlx migrate run --source migrations
```

### 3. 環境変数設定

```bash
# .env ファイル
DATABASE_URL=postgresql://username:password@localhost/mcp_rs
DATABASE_MAX_CONNECTIONS=20
ARGON2_SALT_ROUNDS=12
```

## 基本的な使い方

### ユーザーリポジトリ初期化

```rust
use mcp_rs::security::auth::{
    PostgresUserRepository,
    AuthUser,
    Role,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // リポジトリ作成
    let repo = PostgresUserRepository::new(
        &std::env::var("DATABASE_URL")?,
        20,  // max_connections
        12   // argon2_salt_rounds
    ).await?;

    // マイグレーション実行（初回のみ）
    repo.run_migrations().await?;

    println!("PostgreSQL repository initialized!");
    Ok(())
}
```

### ユーザー作成

```rust
use uuid::Uuid;

async fn create_user(repo: &PostgresUserRepository) -> Result<(), AuthError> {
    // ユーザー情報作成
    let mut user = AuthUser::new(
        Uuid::new_v4().to_string(),
        "john_doe".to_string()
    );
    user.email = Some("john@example.com".to_string());
    user.roles.insert(Role::User);

    // パスワードハッシュ化
    let password_hasher = PasswordHasher::new(12);
    let password_hash = password_hasher.hash("SecurePassword123!")?;

    // データベースに保存
    repo.create_user(&user, Some(password_hash)).await?;
    
    println!("User created: {}", user.id);
    Ok(())
}
```

### ユーザー検索

```rust
async fn find_user(repo: &PostgresUserRepository) -> Result<(), AuthError> {
    // メールアドレスで検索
    if let Some(user) = repo.find_by_email("john@example.com").await? {
        println!("Found user: {} ({})", user.username, user.id);
        println!("Roles: {:?}", user.roles);
    }

    // IDで検索
    if let Some(user) = repo.find_by_id("user-uuid-here").await? {
        println!("Found user by ID: {}", user.username);
    }

    Ok(())
}
```

### パスワード認証

```rust
async fn authenticate_user(
    repo: &PostgresUserRepository,
    email: &str,
    password: &str
) -> Result<(), AuthError> {
    if let Some(user) = repo.verify_password(email, password).await? {
        println!("Authentication successful: {}", user.username);
        println!("User roles: {:?}", user.roles);
        Ok(())
    } else {
        println!("Authentication failed: invalid credentials");
        Err(AuthError::InvalidCredentials)
    }
}
```

### ユーザー更新

```rust
async fn update_user(repo: &PostgresUserRepository) -> Result<(), AuthError> {
    // ユーザー取得
    let mut user = repo.find_by_email("john@example.com")
        .await?
        .ok_or(AuthError::UserNotFound("john@example.com".to_string()))?;

    // 情報更新
    user.username = "john_updated".to_string();
    user.roles.insert(Role::Admin);

    // データベースに保存
    repo.update_user(&user).await?;
    
    println!("User updated!");
    Ok(())
}
```

## MultiAuthProviderとの統合

### 統合パターン

```rust
use mcp_rs::security::auth::{
    PostgresUserRepository,
    MultiAuthProvider,
    JwtConfig,
    UserRepository,
};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // PostgreSQLリポジトリ作成
    let repository: Arc<dyn UserRepository> = Arc::new(
        PostgresUserRepository::new(
            &std::env::var("DATABASE_URL")?,
            20,
            12
        ).await?
    );

    // マイグレーション実行
    if let Some(postgres_repo) = Arc::clone(&repository)
        .downcast_ref::<PostgresUserRepository>() 
    {
        postgres_repo.run_migrations().await?;
    }

    // MultiAuthProvider作成
    let provider = MultiAuthProvider::new(
        Some(JwtConfig::default()),
        None,  // OAuth2Config
        None,  // SessionConfig
        None,  // ApiKeyConfig
        12,    // password_salt_rounds
        repository,
    );

    // ユーザー登録
    let user = provider.register_user(
        "new_user".to_string(),
        "SecurePass123!".to_string(),
        "newuser@example.com".to_string(),
    ).await?;

    println!("User registered: {}", user.id);

    // 認証
    let credentials = Credentials::Password {
        username: "newuser@example.com".to_string(),
        password: "SecurePass123!".to_string(),
    };

    let auth_user = provider.authenticate(credentials).await?;
    println!("Authenticated: {}", auth_user.username);

    Ok(())
}
```

## 本番環境設定

### 1. 環境変数設定

```bash
# .env.production
DATABASE_URL=postgresql://prod_user:${DB_PASSWORD}@db.example.com:5432/mcp_rs?sslmode=require
DATABASE_MAX_CONNECTIONS=50
DATABASE_MIN_CONNECTIONS=5
DATABASE_IDLE_TIMEOUT=600
ARGON2_SALT_ROUNDS=14
```

### 2. 接続プールチューニング

```rust
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

let pool = PgPoolOptions::new()
    .max_connections(50)
    .min_connections(5)
    .idle_timeout(Duration::from_secs(600))
    .acquire_timeout(Duration::from_secs(30))
    .connect(&database_url)
    .await?;
```

### 3. SSL/TLS設定

```bash
# SSL必須モード
DATABASE_URL="postgresql://user:pass@host/db?sslmode=require"

# クライアント証明書使用
DATABASE_URL="postgresql://user:pass@host/db?sslmode=verify-full&sslrootcert=/path/to/ca.crt&sslcert=/path/to/client.crt&sslkey=/path/to/client.key"
```

### 4. バックアップ戦略

```bash
# 毎日バックアップ (cron)
0 2 * * * pg_dump mcp_rs | gzip > /backups/mcp_rs_$(date +\%Y\%m\%d).sql.gz

# リストア
gunzip < /backups/mcp_rs_20250106.sql.gz | psql mcp_rs
```

## パフォーマンスチューニング

### 1. インデックス最適化

```sql
-- 使用状況確認
SELECT schemaname, tablename, indexname, idx_scan
FROM pg_stat_user_indexes
WHERE tablename = 'users'
ORDER BY idx_scan DESC;

-- 不要なインデックス削除
DROP INDEX IF NOT EXISTS unused_index_name;
```

### 2. クエリパフォーマンス分析

```sql
-- EXPLAIN ANALYZE実行
EXPLAIN ANALYZE
SELECT * FROM users WHERE email = 'test@example.com';
```

### 3. 接続プール監視

```rust
println!("Active connections: {}", pool.size());
println!("Idle connections: {}", pool.num_idle());
```

### 4. バッチ処理

複数ユーザーの一括作成：

```rust
async fn batch_create_users(
    repo: &PostgresUserRepository,
    users: Vec<(AuthUser, Option<String>)>
) -> Result<(), AuthError> {
    for (user, password_hash) in users {
        repo.create_user(&user, password_hash).await?;
    }
    Ok(())
}
```

## トラブルシューティング

### 接続エラー

```rust
// エラー処理例
match PostgresUserRepository::new(&database_url, 20, 12).await {
    Ok(repo) => println!("Connected successfully"),
    Err(e) => {
        eprintln!("Database connection failed: {}", e);
        eprintln!("Check DATABASE_URL and PostgreSQL server status");
    }
}
```

### デッドロック対策

```rust
// トランザクションタイムアウト設定
sqlx::query("SET statement_timeout = 5000")  // 5秒
    .execute(&pool)
    .await?;
```

## セキュリティベストプラクティス

1. **最小権限の原則**: データベースユーザーに必要最小限の権限のみ付与
2. **パスワードハッシュ**: Argon2使用（bcryptより安全）
3. **接続暗号化**: 本番環境では必ずSSL/TLS使用
4. **監査ログ**: PostgreSQLの監査機能を有効化
5. **定期的なバックアップ**: 毎日バックアップ取得

## 参考資料

- [sqlx Documentation](https://docs.rs/sqlx/)
- [PostgreSQL Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)
- [Argon2 Password Hashing](https://github.com/P-H-C/phc-winner-argon2)
