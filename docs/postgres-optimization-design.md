# PostgreSQL最適化 設計書

## 📋 Issue #214: Database Integration Phase 2 - PostgreSQL最適化

**優先度**: P1 (High)  
**マイルストーン**: v0.2.0-beta  
**期限**: 2026年1月31日  
**依存**: Phase 1 MySQL Foundation (完了)

---

## 🎯 プロジェクト目標

PostgreSQL統合の最適化を通じて、以下を実現：

1. **高パフォーマンス**: 接続プール最適化でスループット向上
2. **PostgreSQL固有機能活用**: JSON/JSONB、LISTEN/NOTIFY、CTEsなど
3. **スケーラビリティ**: 大量データ処理の最適化
4. **運用性**: クエリ分析、スロークエリ検出、パフォーマンスモニタリング

---

## 📐 システムアーキテクチャ

### レイヤー構成

```
┌─────────────────────────────────────────┐
│   Application Layer (MCP Handlers)     │
├─────────────────────────────────────────┤
│   PostgreSQL Engine Abstraction         │
│   - Query Builder                       │
│   - Transaction Manager                 │
│   - JSON Handler                        │
├─────────────────────────────────────────┤
│   Connection Pool Layer                 │
│   - Optimized Pool (sqlx::PgPool)      │
│   - Health Check                        │
│   - Dynamic Scaling                     │
├─────────────────────────────────────────┤
│   PostgreSQL-Specific Features          │
│   - LISTEN/NOTIFY                       │
│   - JSONB Operations                    │
│   - Full-Text Search                    │
│   - CTEs & Window Functions             │
├─────────────────────────────────────────┤
│   Performance & Monitoring              │
│   - Query Analyzer                      │
│   - Index Suggestions                   │
│   - Slow Query Detection                │
│   - Batch Processor                     │
└─────────────────────────────────────────┘
```

---

## 🗂️ ディレクトリ構造

```
src/handlers/database/engines/postgres/
├── mod.rs                      # PostgresEngine実装
├── config.rs                   # PostgreSQL設定
├── pool.rs                     # 最適化された接続プール
├── transaction.rs              # トランザクション管理
├── json.rs                     # JSON/JSONB操作
├── notify.rs                   # LISTEN/NOTIFY実装
├── cte.rs                      # CTE・Window Functions
├── fulltext.rs                 # Full-Text Search
├── performance.rs              # クエリ分析・最適化
├── batch.rs                    # バッチ処理
└── error.rs                    # PostgreSQL専用エラー型

tests/
├── postgres_integration_test.rs
├── postgres_jsonb_test.rs
├── postgres_notify_test.rs
├── postgres_performance_test.rs
└── postgres_batch_test.rs

examples/
├── postgres_jsonb_demo.rs
├── postgres_notify_demo.rs
├── postgres_performance_demo.rs
└── postgres_batch_demo.rs

docs/
├── postgres-optimization-guide.md
├── postgres-json-guide.md
├── postgres-performance-tuning.md
└── postgres-best-practices.md

configs/database/
└── postgres.toml
```

---

## 🔧 主要コンポーネント設計

### 1. PostgresEngine (Core)

**ファイル**: `src/handlers/database/engines/postgres/mod.rs`

```rust
use sqlx::PgPool;
use serde_json::Value;

pub struct PostgresEngine {
    pool: PgPool,
    config: PostgresConfig,
    metrics: PostgresMetrics,
}

impl PostgresEngine {
    pub async fn new(config: PostgresConfig) -> Result<Self> {
        let pool = create_optimized_pool(&config).await?;
        Ok(Self {
            pool,
            config,
            metrics: PostgresMetrics::new(),
        })
    }

    pub async fn execute_query(&self, query: &str, params: &[Value]) -> Result<QueryResult> {
        // Prepared statement execution
    }

    pub async fn begin_transaction(&self) -> Result<PostgresTransaction> {
        // Transaction with PostgreSQL extensions
    }

    pub fn jsonb_handler(&self) -> JsonbHandler {
        JsonbHandler::new(self.pool.clone())
    }

    pub fn notifier(&self) -> PostgresNotifier {
        PostgresNotifier::new(self.pool.clone())
    }

    pub fn performance_analyzer(&self) -> QueryAnalyzer {
        QueryAnalyzer::new(self.pool.clone())
    }
}

impl DatabaseEngine for PostgresEngine {
    async fn execute_query(&self, query: &str, params: &[Value]) -> Result<QueryResult>;
    async fn begin_transaction(&self) -> Result<Transaction>;
    async fn health_check(&self) -> Result<HealthStatus>;
}
```

---

### 2. 接続プール最適化

**ファイル**: `src/handlers/database/engines/postgres/pool.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedPoolConfig {
    /// 最小接続数
    pub min_connections: u32,
    /// 最大接続数
    pub max_connections: u32,
    /// 接続タイムアウト
    pub connection_timeout: Duration,
    /// アイドルタイムアウト
    pub idle_timeout: Option<Duration>,
    /// 最大接続生存時間
    pub max_lifetime: Option<Duration>,
    /// ヘルスチェック間隔
    pub health_check_interval: Duration,
    /// Prepared Statement キャッシュサイズ
    pub statement_cache_capacity: usize,
}

impl Default for OptimizedPoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 100,
            connection_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(600)),
            max_lifetime: Some(Duration::from_secs(1800)),
            health_check_interval: Duration::from_secs(60),
            statement_cache_capacity: 1000,
        }
    }
}

pub async fn create_optimized_pool(config: &PostgresConfig) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .min_connections(config.pool.min_connections)
        .max_connections(config.pool.max_connections)
        .acquire_timeout(config.pool.connection_timeout)
        .idle_timeout(config.pool.idle_timeout)
        .max_lifetime(config.pool.max_lifetime)
        .test_before_acquire(true) // ヘルスチェック
        .connect(&config.connection_string)
        .await?;

    Ok(pool)
}
```

---

### 3. JSONB操作

**ファイル**: `src/handlers/database/engines/postgres/json.rs`

```rust
pub struct JsonbHandler {
    pool: PgPool,
}

impl JsonbHandler {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// JSONBデータを挿入
    pub async fn insert_jsonb(&self, table: &str, data: &Value) -> Result<i64> {
        let query = format!("INSERT INTO {} (data) VALUES ($1) RETURNING id", table);
        let id: (i64,) = sqlx::query_as(&query)
            .bind(data)
            .fetch_one(&self.pool)
            .await?;
        Ok(id.0)
    }

    /// JSONパス式でクエリ
    pub async fn query_jsonb_path(&self, table: &str, path: &str, value: &Value) -> Result<Vec<Value>> {
        let query = format!("SELECT data FROM {} WHERE data @> $1", table);
        let rows: Vec<(Value,)> = sqlx::query_as(&query)
            .bind(value)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// JSONBフィールドを更新
    pub async fn update_jsonb_field(
        &self,
        table: &str,
        id: i64,
        path: &str,
        value: &Value,
    ) -> Result<()> {
        let query = format!(
            "UPDATE {} SET data = jsonb_set(data, $1, $2) WHERE id = $3",
            table
        );
        sqlx::query(&query)
            .bind(path)
            .bind(value)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// JSONB集約
    pub async fn aggregate_jsonb(&self, table: &str, condition: &str) -> Result<Value> {
        let query = format!("SELECT jsonb_agg(data) FROM {} WHERE {}", table, condition);
        let result: (Value,) = sqlx::query_as(&query)
            .fetch_one(&self.pool)
            .await?;
        Ok(result.0)
    }
}
```

---

### 4. LISTEN/NOTIFY

**ファイル**: `src/handlers/database/engines/postgres/notify.rs`

```rust
use tokio_postgres::{AsyncMessage, Connection};
use futures::StreamExt;

pub struct PostgresNotifier {
    pool: PgPool,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub channel: String,
    pub payload: String,
    pub pid: u32,
}

impl PostgresNotifier {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// チャンネルをリッスン
    pub async fn listen(&self, channel: &str) -> Result<impl Stream<Item = Notification>> {
        let (client, mut connection) = tokio_postgres::connect(&self.connection_string(), NoTls).await?;
        
        client.execute(&format!("LISTEN {}", channel), &[]).await?;
        
        let stream = async_stream::stream! {
            while let Some(msg) = connection.next().await {
                if let AsyncMessage::Notification(notif) = msg {
                    yield Notification {
                        channel: notif.channel().to_string(),
                        payload: notif.payload().to_string(),
                        pid: notif.process_id(),
                    };
                }
            }
        };
        
        Ok(stream)
    }

    /// 通知を送信
    pub async fn notify(&self, channel: &str, payload: &str) -> Result<()> {
        let query = format!("NOTIFY {}, '{}'", channel, payload);
        sqlx::query(&query)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

---

### 5. パフォーマンス分析

**ファイル**: `src/handlers/database/engines/postgres/performance.rs`

```rust
pub struct QueryAnalyzer {
    pool: PgPool,
}

#[derive(Debug, Serialize)]
pub struct ExecutionPlan {
    pub plan: String,
    pub estimated_cost: f64,
    pub estimated_rows: i64,
    pub actual_time: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct IndexSuggestion {
    pub table: String,
    pub columns: Vec<String>,
    pub index_type: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SlowQuery {
    pub query: String,
    pub execution_time_ms: f64,
    pub calls: i64,
    pub rows: i64,
}

impl QueryAnalyzer {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// クエリの実行プランを分析
    pub async fn analyze_query(&self, query: &str) -> Result<ExecutionPlan> {
        let explain_query = format!("EXPLAIN (FORMAT JSON, ANALYZE) {}", query);
        let result: (Value,) = sqlx::query_as(&explain_query)
            .fetch_one(&self.pool)
            .await?;
        
        // JSONをパース
        let plan_data = &result.0[0]["Plan"];
        Ok(ExecutionPlan {
            plan: plan_data.to_string(),
            estimated_cost: plan_data["Total Cost"].as_f64().unwrap_or(0.0),
            estimated_rows: plan_data["Plan Rows"].as_i64().unwrap_or(0),
            actual_time: plan_data["Actual Total Time"].as_f64(),
        })
    }

    /// インデックス推奨
    pub async fn suggest_indexes(&self, table: &str) -> Result<Vec<IndexSuggestion>> {
        // pg_stat_user_tables、pg_stat_user_indexesを分析
        let query = r#"
            SELECT
                schemaname,
                tablename,
                seq_scan,
                idx_scan,
                n_tup_ins + n_tup_upd + n_tup_del as modifications
            FROM pg_stat_user_tables
            WHERE tablename = $1
        "#;
        
        // 分析ロジック実装
        todo!("Index suggestion logic")
    }

    /// スロークエリを取得
    pub async fn get_slow_queries(&self, threshold_ms: u64) -> Result<Vec<SlowQuery>> {
        // pg_stat_statementsから取得
        let query = r#"
            SELECT
                query,
                total_exec_time / calls as avg_time_ms,
                calls,
                rows
            FROM pg_stat_statements
            WHERE total_exec_time / calls > $1
            ORDER BY avg_time_ms DESC
            LIMIT 100
        "#;
        
        let rows: Vec<(String, f64, i64, i64)> = sqlx::query_as(query)
            .bind(threshold_ms as f64)
            .fetch_all(&self.pool)
            .await?;
        
        Ok(rows.into_iter().map(|(q, t, c, r)| SlowQuery {
            query: q,
            execution_time_ms: t,
            calls: c,
            rows: r,
        }).collect())
    }
}
```

---

### 6. バッチ処理

**ファイル**: `src/handlers/database/engines/postgres/batch.rs`

```rust
pub struct BatchProcessor {
    pool: PgPool,
}

impl BatchProcessor {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// バルクINSERT
    pub async fn bulk_insert(
        &self,
        table: &str,
        rows: &[HashMap<String, Value>],
    ) -> Result<u64> {
        if rows.is_empty() {
            return Ok(0);
        }

        // 動的にINSERTクエリを構築
        let columns: Vec<String> = rows[0].keys().cloned().collect();
        let placeholders: Vec<String> = (0..rows.len())
            .map(|i| {
                let start = i * columns.len() + 1;
                let end = start + columns.len();
                format!(
                    "({})",
                    (start..end).map(|n| format!("${}", n)).collect::<Vec<_>>().join(", ")
                )
            })
            .collect();

        let query = format!(
            "INSERT INTO {} ({}) VALUES {}",
            table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query(&query);
        for row in rows {
            for col in &columns {
                query_builder = query_builder.bind(&row[col]);
            }
        }

        let result = query_builder.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    /// COPYコマンドでCSVインポート
    pub async fn copy_from_csv(&self, table: &str, csv_data: &str) -> Result<u64> {
        let query = format!("COPY {} FROM STDIN WITH (FORMAT CSV, HEADER)", table);
        
        // tokio-postgresを使用したCOPY実装
        // sqlxはCOPYをサポートしていないため、直接接続が必要
        todo!("COPY implementation using tokio-postgres")
    }
}
```

---

## 📝 設定ファイル設計

**ファイル**: `configs/database/postgres.toml`

```toml
[postgres]
# 接続設定
host = "localhost"
port = 5432
database = "mcp_rs"
username = "postgres"
password = "${POSTGRES_PASSWORD}"  # 環境変数から取得
ssl_mode = "prefer"

[connection_pool]
# 接続プール設定
min_connections = 5
max_connections = 100
connection_timeout_secs = 30
idle_timeout_secs = 600
max_lifetime_secs = 1800

[performance]
# パフォーマンス設定
statement_cache_capacity = 1000
prepared_statement_enabled = true
enable_query_logging = true
slow_query_threshold_ms = 1000

[jsonb]
# JSONB設定
enable_jsonb_optimization = true
jsonb_index_type = "gin"  # gin or gist

[monitoring]
# モニタリング設定
enable_pg_stat_statements = true
track_io_timing = true
log_min_duration_statement_ms = 1000
```

---

## 🧪 テスト戦略

### 統合テストカバレッジ

1. **基本機能テスト** (`postgres_integration_test.rs`)
   - 接続・切断
   - クエリ実行
   - トランザクション管理

2. **JSONB操作テスト** (`postgres_jsonb_test.rs`)
   - JSONB挿入・更新・削除
   - JSONパス式クエリ
   - JSONB集約

3. **LISTEN/NOTIFY