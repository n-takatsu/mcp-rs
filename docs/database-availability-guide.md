# データベース接続可用性管理システム

## 概要

mcp-rsプロジェクトのデータベース接続可用性を保つための包括的なシステムです。

## 主要機能

## 1. **ヘルスチェック & 自動復旧**

- 定期的な接続健全性監視
- 段階的復旧戦略
- サーキットブレーカーパターン

## 2. **接続プール管理**

- 動的プールサイズ調整
- 接続の年齢管理
- デッドコネクション検出

## 3. **リトライ戦略**

- 指数バックオフ
- 固定間隔リトライ
- カスタム間隔設定

## 4. **負荷分散**

- ラウンドロビン
- 最小接続数ベース
- レスポンス時間ベース
- 健全性ベース選択

## 5. **読み書き分離**

- マスター/スレーブ構成
- レプリカ優先読み取り
- 自動フェイルオーバー

## 使用例

## 基本的な設定

```rust
use mcp_rs::handlers::database::{
    integrated_availability::{AvailabilitySystemBuilder, IntegratedAvailabilitySystem},
    loadbalancer::{LoadBalancingStrategy, DatabaseEndpoint, DatabaseRole, ReadPreference},
    retry::ExecutionStrategy,
    types::{DatabaseConfig, QueryType, Value},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. システムを構築
    let availability_system = AvailabilitySystemBuilder::new()
        .with_load_balancing_strategy(LoadBalancingStrategy::LeastConnections)
        .with_read_preference(ReadPreference::SecondaryPreferred)
        .with_execution_strategy(ExecutionStrategy::robust())
        .add_database_endpoint(DatabaseEndpoint::new(
            "master".to_string(),
            DatabaseConfig::default(),
            DatabaseRole::Master,
            1,
        ))
        .add_database_endpoint(DatabaseEndpoint::new(
            "replica1".to_string(),
            DatabaseConfig::default(),
            DatabaseRole::Slave,
            1,
        ))
        .add_database_endpoint(DatabaseEndpoint::new(
            "replica2".to_string(),
            DatabaseConfig::default(),
            DatabaseRole::Slave,
            1,
        ))
        .build()
        .await?;

    // 2. 読み取りクエリ実行（自動的にレプリカにルーティング）
    let users = availability_system.execute_query(
        "SELECT * FROM users WHERE active = ?",
        &[Value::Bool(true)],
        QueryType::Select,
    ).await?;

    println!("Found {} users", users.rows.len());

    // 3. 書き込みコマンド実行（自動的にマスターにルーティング）
    let result = availability_system.execute_command(
        "INSERT INTO users (name, email) VALUES (?, ?)",
        &[
            Value::String("John Doe".to_string()),
            Value::String("john@example.com".to_string()),
        ],
    ).await?;

    println!("Inserted user with ID: {:?}", result.last_insert_id);

    // 4. システム統計の確認
    let stats = availability_system.get_system_stats().await;
    println!("System stats: {:?}", stats);

    Ok(())
}
```

## 高可用性構成の例

```rust
use mcp_rs::handlers::database::{
    integrated_availability::AvailabilitySystemBuilder,
    loadbalancer::{LoadBalancingStrategy, ReadPreference},
    retry::{RetryStrategy, TimeoutStrategy, ExecutionStrategy},
    availability::AvailabilityConfig,
};
use std::time::Duration;

async fn setup_high_availability_system() -> Result<IntegratedAvailabilitySystem, DatabaseError> {
    // カスタム実行戦略
    let execution_strategy = ExecutionStrategy {
        retry: RetryStrategy::ExponentialBackoff {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            max_attempts: 5,
        },
        timeout: TimeoutStrategy::robust(),
    };

    // 重み付き負荷分散
    let mut weights = std::collections::HashMap::new();
    weights.insert("master".to_string(), 10);
    weights.insert("replica1".to_string(), 5);
    weights.insert("replica2".to_string(), 5);

    let system = AvailabilitySystemBuilder::new()
        .with_load_balancing_strategy(LoadBalancingStrategy::WeightedRoundRobin(weights))
        .with_read_preference(ReadPreference::SecondaryPreferred)
        .with_execution_strategy(execution_strategy)
        // 本番環境のエンドポイント設定
        .add_database_endpoint(DatabaseEndpoint::new(
            "master".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: ConnectionConfig {
                    host: "primary.db.company.com".to_string(),
                    port: 5432,
                    database: "production".to_string(),
                    username: "app_user".to_string(),
                    password: "secure_password".to_string(),
                    ssl_mode: Some("require".to_string()),
                    timeout_seconds: 30,
                    retry_attempts: 3,
                    options: std::collections::HashMap::new(),
                },
                pool: PoolConfig {
                    max_connections: 50,
                    min_connections: 10,
                    connection_timeout: 30,
                    idle_timeout: 600,
                    max_lifetime: 1800,
                },
                // ... other configs
                ..Default::default()
            },
            DatabaseRole::Master,
            10,
        ))
        .add_database_endpoint(DatabaseEndpoint::new(
            "replica1".to_string(),
            DatabaseConfig {
                // レプリカ1の設定
                ..Default::default()
            },
            DatabaseRole::Slave,
            5,
        ))
        .add_database_endpoint(DatabaseEndpoint::new(
            "replica2".to_string(),
            DatabaseConfig {
                // レプリカ2の設定  
                ..Default::default()
            },
            DatabaseRole::Slave,
            5,
        ))
        .build()
        .await?;

    Ok(system)
}
```

## 動的エンドポイント管理

```rust
async fn manage_endpoints_dynamically(system: &IntegratedAvailabilitySystem) -> Result<(), DatabaseError> {
    // 新しいレプリカを追加
    let new_replica = DatabaseEndpoint::new(
        "replica3".to_string(),
        DatabaseConfig::default(),
        DatabaseRole::Slave,
        5,
    );
    
    system.add_endpoint(new_replica).await?;
    println!("Added new replica endpoint");

    // 統計確認
    let stats = system.get_system_stats().await;
    println!("Current endpoints: {}", stats.total_endpoints);

    // 問題のあるエンドポイントを削除
    system.remove_endpoint("replica1").await?;
    println!("Removed problematic endpoint");

    // 手動フェイルオーバー
    system.manual_failover("master", "replica2").await?;
    println!("Executed manual failover");

    Ok(())
}
```

## 監視とアラート

```rust
use tokio::time::{interval, Duration};

async fn monitor_system_health(system: Arc<IntegratedAvailabilitySystem>) {
    let mut interval = interval(Duration::from_secs(60));
    
    loop {
        interval.tick().await;
        
        let stats = system.get_system_stats().await;
        
        // アラート条件をチェック
        if stats.available_endpoints < stats.total_endpoints / 2 {
            eprintln!("ALERT: Less than 50% of endpoints are available!");
        }
        
        if stats.avg_response_time_ms > 1000 {
            eprintln!("ALERT: Average response time is over 1 second!");
        }
        
        if stats.avg_health_score < 70 {
            eprintln!("ALERT: Average health score is below threshold!");
        }
        
        // 統計をログ出力
        println!("System Health Report:");
        println!("  Available Endpoints: {}/{}", stats.available_endpoints, stats.total_endpoints);
        println!("  Avg Response Time: {}ms", stats.avg_response_time_ms);
        println!("  Avg Health Score: {}", stats.avg_health_score);
        println!("  Total Connections: {}", stats.total_connections);
    }
}
```

## 設定オプション

## 負荷分散戦略

```rust
// ラウンドロビン
LoadBalancingStrategy::RoundRobin

// 重み付きラウンドロビン
LoadBalancingStrategy::WeightedRoundRobin(weights)

// 最小接続数
LoadBalancingStrategy::LeastConnections

// ランダム
LoadBalancingStrategy::Random

// レスポンス時間ベース
LoadBalancingStrategy::ResponseTime

// 健全性ベース
LoadBalancingStrategy::HealthBased
```

## 読み取り設定

```rust
// 常にマスターから読み取り
ReadPreference::Primary

// 可能な限りレプリカから読み取り
ReadPreference::Secondary

// レプリカ優先、利用不可時はマスター
ReadPreference::SecondaryPreferred

// マスター優先、利用不可時はレプリカ
ReadPreference::PrimaryPreferred

// 最も近い（低レイテンシ）から読み取り
ReadPreference::Nearest
```

## リトライ戦略

```rust
// 固定間隔
RetryStrategy::FixedInterval {
    interval: Duration::from_secs(1),
    max_attempts: 3,
}

// 指数バックオフ
RetryStrategy::ExponentialBackoff {
    initial_delay: Duration::from_millis(100),
    max_delay: Duration::from_secs(30),
    multiplier: 2.0,
    max_attempts: 5,
}

// リニアバックオフ
RetryStrategy::LinearBackoff {
    initial_delay: Duration::from_millis(500),
    increment: Duration::from_millis(500),
    max_attempts: 3,
}
```

## ベストプラクティス

## 1. **適切な戦略選択**

- **レスポンス重視**: `ExecutionStrategy::fast()` + `LoadBalancingStrategy::ResponseTime`
- **安定性重視**: `ExecutionStrategy::robust()` + `LoadBalancingStrategy::HealthBased`
- **負荷分散重視**: `LoadBalancingStrategy::LeastConnections`

## 2. **監視とアラート**

- 定期的な統計確認
- しきい値ベースのアラート
- ログとメトリクス収集

## 3. **フェイルオーバー戦略**

- 自動フェイルオーバーの設定
- 手動フェイルオーバーの準備
- フェイルバック戦略

## 4. **パフォーマンス最適化**

- 接続プールサイズの調整
- タイムアウト値の最適化
- リトライ回数の調整

## トラブルシューティング

## よくある問題と対策

1. **接続エラーが頻発する**
   - タイムアウト値を増加
   - リトライ回数を増加
   - エンドポイントの健全性を確認

2. **レスポンスが遅い**
   - 負荷分散戦略を`ResponseTime`に変更
   - 接続プールサイズを増加
   - 読み取り専用クエリをレプリカに分散

3. **フェイルオーバーが正しく動作しない**
   - ヘルスチェック設定を見直し
   - エンドポイントの設定を確認
   - 手動フェイルオーバーをテスト
