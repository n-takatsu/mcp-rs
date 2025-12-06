# Redis セッションストア 使用ガイド

Redisを使用した高速セッション管理の実装ガイドです。

## 目次

1. [概要](#概要)
2. [セットアップ](#セットアップ)
3. [基本的な使い方](#基本的な使い方)
4. [高度な機能](#高度な機能)
5. [本番環境設定](#本番環境設定)
6. [パフォーマンス最適化](#パフォーマンス最適化)

## 概要

### 特徴

- ✅ **高速アクセス**: メモリベースのキー・バリューストア
- ✅ **自動期限切れ**: TTL（Time To Live）による自動セッション削除
- ✅ **スケーラビリティ**: 複数サーバー間でのセッション共有
- ✅ **永続化オプション**: RDB/AOFによるデータ永続化
- ✅ **接続管理**: ConnectionManagerによる自動再接続

### ユースケース

- Webアプリケーションのセッション管理
- APIトークンのキャッシング
- レート制限カウンター
- リアルタイムアプリケーションの状態管理

## セットアップ

### 1. 依存関係

`Cargo.toml`でRedis機能を有効化：

```toml
[dependencies]
mcp-rs = { version = "0.15", features = ["redis-backend"] }
```

### 2. Redisインストール

```bash
# Ubuntu/Debian
sudo apt install redis-server

# macOS (Homebrew)
brew install redis

# Docker
docker run -d -p 6379:6379 redis:7-alpine
```

### 3. Redis起動確認

```bash
# Redisサーバー起動
redis-server

# 接続確認
redis-cli ping
# 出力: PONG
```

### 4. 環境変数設定

```bash
# .env ファイル
REDIS_URL=redis://localhost:6379
SESSION_TTL=3600  # 1時間
SESSION_KEY_PREFIX=session:
```

## 基本的な使い方

### セッションストア初期化

```rust
use mcp_rs::security::auth::{RedisSessionStore, AuthUser};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Redisセッションストア作成
    let mut store = RedisSessionStore::new(
        "redis://localhost:6379",
        3600,              // デフォルトTTL（秒）
        "session:".to_string()  // キープレフィックス
    ).await?;

    println!("Redis session store initialized!");
    Ok(())
}
```

### セッション作成

```rust
use uuid::Uuid;

async fn create_session(
    store: &mut RedisSessionStore,
    user: &AuthUser
) -> Result<String, Box<dyn std::error::Error>> {
    // セッションIDを生成
    let session_id = Uuid::new_v4().to_string();

    // セッション作成（デフォルトTTL使用）
    store.create_session(&session_id, user, None).await?;

    println!("Session created: {}", session_id);
    Ok(session_id)
}
```

### カスタムTTLでセッション作成

```rust
async fn create_session_with_ttl(
    store: &mut RedisSessionStore,
    user: &AuthUser,
    ttl_seconds: u64
) -> Result<String, Box<dyn std::error::Error>> {
    let session_id = Uuid::new_v4().to_string();

    // カスタムTTL（例: 7200秒 = 2時間）
    store.create_session(&session_id, user, Some(ttl_seconds)).await?;

    println!("Session created with TTL: {} seconds", ttl_seconds);
    Ok(session_id)
}
```

### セッション取得

```rust
async fn get_session(
    store: &mut RedisSessionStore,
    session_id: &str
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(session) = store.get_session(session_id).await? {
        println!("User: {}", session.user.username);
        println!("Created at: {}", session.created_at);
        println!("Last accessed: {}", session.last_accessed_at);
    } else {
        println!("Session not found or expired");
    }
    Ok(())
}
```

### セッション更新（TTLリフレッシュ）

```rust
async fn refresh_session(
    store: &mut RedisSessionStore,
    session_id: &str
) -> Result<(), Box<dyn std::error::Error>> {
    // セッションの有効期限を延長
    store.refresh_session(session_id, None).await?;
    println!("Session refreshed");
    Ok(())
}
```

### セッション削除

```rust
async fn logout(
    store: &mut RedisSessionStore,
    session_id: &str
) -> Result<(), Box<dyn std::error::Error>> {
    store.destroy_session(session_id).await?;
    println!("Session destroyed");
    Ok(())
}
```

## 高度な機能

### ユーザーの全セッション管理

```rust
async fn manage_user_sessions(
    store: &mut RedisSessionStore,
    user_id: &str
) -> Result<(), Box<dyn std::error::Error>> {
    // アクティブセッション数を確認
    let count = store.count_user_sessions(user_id).await?;
    println!("Active sessions: {}", count);

    // セッション数の制限（例: 最大5セッション）
    if count > 5 {
        println!("Too many sessions, destroying all...");
        let deleted = store.destroy_user_sessions(user_id).await?;
        println!("Destroyed {} sessions", deleted);
    }

    Ok(())
}
```

### セッションメタデータ管理

```rust
use std::collections::HashMap;

async fn update_session_metadata(
    store: &mut RedisSessionStore,
    session_id: &str
) -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = HashMap::new();
    metadata.insert("ip_address".to_string(), "192.168.1.100".to_string());
    metadata.insert("user_agent".to_string(), "Mozilla/5.0...".to_string());
    metadata.insert("login_time".to_string(), chrono::Utc::now().to_rfc3339());

    store.update_session_metadata(session_id, metadata).await?;
    println!("Session metadata updated");
    Ok(())
}
```

### TTL監視

```rust
async fn monitor_session_ttl(
    store: &mut RedisSessionStore,
    session_id: &str
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ttl) = store.get_session_ttl(session_id).await? {
        println!("Session expires in {} seconds", ttl);
        
        // 残り時間が少ない場合は警告
        if ttl < 300 {  // 5分未満
            println!("Warning: Session expiring soon!");
        }
    } else {
        println!("Session not found");
    }
    Ok(())
}
```

### ヘルスチェック

```rust
async fn check_redis_health(
    store: &mut RedisSessionStore
) -> Result<(), Box<dyn std::error::Error>> {
    let is_healthy = store.health_check().await?;
    
    if is_healthy {
        println!("✓ Redis is healthy");
    } else {
        println!("✗ Redis connection problem");
    }
    
    Ok(())
}
```

## 本番環境設定

### 1. Redis設定ファイル

`redis.conf`:

```conf
# ネットワーク
bind 0.0.0.0
port 6379
protected-mode yes
requirepass YOUR_STRONG_PASSWORD

# メモリ管理
maxmemory 2gb
maxmemory-policy allkeys-lru

# 永続化（オプション）
save 900 1
save 300 10
save 60 10000
appendonly yes
appendfsync everysec

# セキュリティ
rename-command FLUSHDB ""
rename-command FLUSHALL ""
rename-command CONFIG ""
```

### 2. 環境変数設定

```bash
# .env.production
REDIS_URL=redis://:password@redis.example.com:6379
SESSION_TTL=7200           # 2時間
SESSION_KEY_PREFIX=prod:session:
MAX_CONNECTIONS=20
```

### 3. SSL/TLS接続

```bash
# RedisS（Redis over SSL）
REDIS_URL=rediss://:password@redis.example.com:6380
```

### 4. クラスタ構成

```rust
// Redis Cluster接続（複数ノード）
let redis_urls = vec![
    "redis://node1.example.com:6379",
    "redis://node2.example.com:6379",
    "redis://node3.example.com:6379",
];
```

## パフォーマンス最適化

### 1. コネクションプーリング

`ConnectionManager`が自動的に管理しますが、設定を調整可能：

```rust
use redis::ConnectionInfo;

let connection_info = ConnectionInfo::from_str(&redis_url)?;
// ConnectionManagerが内部でコネクションを管理
```

### 2. パイプライニング

複数コマンドを一度に実行：

```rust
// 複数セッションの一括取得例（将来実装）
async fn batch_get_sessions(
    store: &mut RedisSessionStore,
    session_ids: Vec<String>
) -> Result<Vec<Option<SessionData>>, Box<dyn std::error::Error>> {
    let mut sessions = Vec::new();
    for id in session_ids {
        sessions.push(store.get_session(&id).await?);
    }
    Ok(sessions)
}
```

### 3. TTL戦略

```rust
// 用途別のTTL設定
const SHORT_SESSION_TTL: u64 = 900;      // 15分（モバイルアプリ）
const NORMAL_SESSION_TTL: u64 = 3600;    // 1時間（Webアプリ）
const LONG_SESSION_TTL: u64 = 86400;     // 24時間（Remember Me）

// 使用例
store.create_session(&session_id, &user, Some(LONG_SESSION_TTL)).await?;
```

### 4. メモリ使用量監視

```bash
# Redis CLIで監視
redis-cli INFO memory

# 使用メモリ確認
redis-cli INFO stats | grep used_memory_human
```

## トラブルシューティング

### 接続エラー

```rust
match RedisSessionStore::new(&redis_url, 3600, "session:".to_string()).await {
    Ok(store) => println!("Connected to Redis"),
    Err(e) => {
        eprintln!("Redis connection failed: {}", e);
        eprintln!("Check REDIS_URL and Redis server status");
    }
}
```

### メモリ不足

```bash
# メモリ使用量確認
redis-cli INFO memory | grep maxmemory

# キーの削除ポリシー確認
redis-cli CONFIG GET maxmemory-policy
```

### セッション期限切れ

```rust
// 期限切れセッションの確認
if let None = store.get_session(session_id).await? {
    println!("Session expired or not found");
    // リダイレクトやエラーハンドリング
}
```

## セキュリティベストプラクティス

1. **パスワード認証**: 必ず`requirepass`を設定
2. **ネットワーク制限**: `bind`でアクセス元IPを制限
3. **危険コマンドの無効化**: `rename-command`で削除
4. **SSL/TLS**: 本番環境では暗号化通信必須
5. **定期バックアップ**: RDB/AOFファイルのバックアップ

## 参考資料

- [Redis Documentation](https://redis.io/documentation)
- [redis-rs GitHub](https://github.com/redis-rs/redis-rs)
- [Redis Security Guide](https://redis.io/topics/security)
- [Redis Persistence](https://redis.io/topics/persistence)
