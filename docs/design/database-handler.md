# データベースハンドラー設計仕様書

## 📋 概要

MCP-RSにおけるデータベースハンドラーは、様々なデータベースエンジンに対する統一的なインターフェースを提供し、安全で効率的なデータベース操作を可能にします。

## 設計目標

- **多様性**: PostgreSQL、MySQL、SQLite、MongoDB、Redisなど複数のDB対応
- **セキュリティ**: SQLインジェクション対策、認証・認可、監査ログ
- **パフォーマンス**: 接続プール、クエリ最適化、キャッシュ機能
- **拡張性**: 新しいDBエンジンの容易な追加
- **統一性**: 共通MCP Tool/Resourceインターフェース

## 🏗️ アーキテクチャ設計

## レイヤー構造

```text

│        MCP Protocol Layer          │ ← 統一MCPインターフェース
├─────────────────────────────────────┤
│      Database Handler Layer        │ ← DB操作抽象化
├─────────────────────────────────────┤
│      Database Engine Layer         │ ← DB固有実装
├─────────────────────────────────────┤
│     Connection Pool Layer          │ ← 接続管理
├─────────────────────────────────────┤
│       Security Layer              │ ← セキュリティ機能
└─────────────────────────────────────┘

```

## コンポーネント設計

### 1. データベース抽象化トレイト

```rust

pub trait DatabaseEngine: Send + Sync {
    /// データベースタイプを返す
    fn engine_type(&self) -> DatabaseType;
    
    /// 接続確立
    async fn connect(&self, config: &DatabaseConfig) -> Result<Box<dyn DatabaseConnection>, DatabaseError>;
    
    /// 健全性チェック
    async fn health_check(&self) -> Result<HealthStatus, DatabaseError>;
    
    /// サポートされる機能を返す
    fn supported_features(&self) -> Vec<DatabaseFeature>;
}

#[async_trait]
pub trait DatabaseConnection: Send + Sync {
    /// クエリ実行（SELECT）
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError>;
    
    /// コマンド実行（INSERT/UPDATE/DELETE）
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;
    
    /// トランザクション開始
    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError>;
    
    /// スキーマ情報取得
    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError>;
    
    /// 接続終了
    async fn close(&self) -> Result<(), DatabaseError>;
}

#[async_trait]
pub trait DatabaseTransaction: Send + Sync {
    /// クエリ実行
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError>;
    
    /// コマンド実行
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;
    
    /// コミット
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError>;
    
    /// ロールバック
    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError>;
}

```

### 2. セキュリティレイヤー

```rust

    /// SQLインジェクション検知
    sql_injection_detector: SqlInjectionDetector,
    /// クエリ許可リスト
    query_whitelist: QueryWhitelist,
    /// 監査ログ
    audit_logger: AuditLogger,
    /// 脅威インテリジェンス
    threat_intelligence: Arc<ThreatIntelligenceEngine>,
}

impl DatabaseSecurity {
    /// クエリの安全性チェック
    pub async fn validate_query(&self, sql: &str, context: &QueryContext) -> Result<ValidationResult, SecurityError> {
        // 1. SQLインジェクション検知
        self.sql_injection_detector.scan(sql)?;
        
        // 2. 許可リストチェック
        self.query_whitelist.validate(sql, context)?;
        
        // 3. 脅威インテリジェンス照会
        self.threat_intelligence.analyze_query(sql).await?;
        
        // 4. 監査ログ記録
        self.audit_logger.log_query_attempt(sql, context).await?;
        
        Ok(ValidationResult::Approved)
    }
}

```

## 🔧 実装計画

## Phase 1: 基盤システム実装

### 1.1 データベース抽象化レイヤー

**ファイル**: `src/handlers/database/engine.rs`

- `DatabaseEngine` トレイト実装
- `DatabaseConnection` トレイト実装
- `DatabaseTransaction` トレイト実装
- 共通エラーハンドリング

### 1.2 接続プール管理

**ファイル**: `src/handlers/database/pool.rs`

- 接続プール実装
- 接続ライフサイクル管理
- 負荷分散とフェイルオーバー

### 1.3 セキュリティシステム

**ファイル**: `src/handlers/database/security.rs`

- SQLインジェクション検知
- クエリホワイトリスト
- 監査ログ機能

## Phase 2: PostgreSQL実装

### 2.1 PostgreSQLエンジン

**ファイル**: `src/handlers/database/engines/postgresql.rs`

```rust

    pool: Arc<deadpool_postgres::Pool>,
    config: PostgreSqlConfig,
    security: Arc<DatabaseSecurity>,
}

#[async_trait]
impl DatabaseEngine for PostgreSqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }
    
    async fn connect(&self, config: &DatabaseConfig) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let conn = self.pool.get().await?;
        Ok(Box::new(PostgreSqlConnection::new(conn, self.security.clone())))
    }
    
    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::PreparedStatements,
            DatabaseFeature::JsonSupport,
            DatabaseFeature::FullTextSearch,
            DatabaseFeature::StoredProcedures,
        ]
    }
}

```

### 2.2 PostgreSQL接続実装

**ファイル**: `src/handlers/database/engines/postgresql.rs`

```rust

    client: deadpool_postgres::Client,
    security: Arc<DatabaseSecurity>,
}

#[async_trait]
impl DatabaseConnection for PostgreSqlConnection {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // セキュリティチェック
        let context = QueryContext::new(QueryType::Select, &self.client);
        self.security.validate_query(sql, &context).await?;
        
        // パラメータ変換
        let pg_params = self.convert_params(params)?;
        
        // クエリ実行
        let rows = self.client.query(sql, &pg_params).await?;
        
        // 結果変換
        Ok(self.convert_rows(rows)?)
    }
    
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let context = QueryContext::new(QueryType::Modify, &self.client);
        self.security.validate_query(sql, &context).await?;
        
        let pg_params = self.convert_params(params)?;
        let result = self.client.execute(sql, &pg_params).await?;
        
        Ok(ExecuteResult {
            rows_affected: result,
            last_insert_id: None, // PostgreSQLではRETURNING句で取得
        })
    }
}

```

## Phase 3: MCPインターフェース実装

### 3.1 Database MCPハンドラー

**ファイル**: `src/handlers/database/handler.rs`

```rust

    engines: HashMap<String, Arc<dyn DatabaseEngine>>,
    active_engine: String,
    security: Arc<DatabaseSecurity>,
    threat_intelligence: Arc<ThreatIntelligenceEngine>,
}

#[async_trait]
impl McpHandler for DatabaseHandler {
    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        Ok(vec![
            Tool {
                name: "execute_query".to_string(),
                description: "Execute SELECT query and return results".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": {"type": "string", "description": "SQL query to execute"},
                        "params": {"type": "array", "description": "Query parameters"},
                        "engine": {"type": "string", "description": "Database engine to use"}
                    },
                    "required": ["sql"]
                }),
            },
            Tool {
                name: "execute_command".to_string(),
                description: "Execute INSERT/UPDATE/DELETE command".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": {"type": "string", "description": "SQL command to execute"},
                        "params": {"type": "array", "description": "Command parameters"},
                        "engine": {"type": "string", "description": "Database engine to use"}
                    },
                    "required": ["sql"]
                }),
            },
            Tool {
                name: "get_schema".to_string(),
                description: "Get database schema information".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "engine": {"type": "string", "description": "Database engine to use"},
                        "schema_name": {"type": "string", "description": "Specific schema name (optional)"}
                    }
                }),
            },
            Tool {
                name: "begin_transaction".to_string(),
                description: "Begin database transaction".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "engine": {"type": "string", "description": "Database engine to use"},
                        "isolation_level": {"type": "string", "description": "Transaction isolation level"}
                    }
                }),
            },
        ])
    }
    
    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
        match params.name.as_str() {
            "execute_query" => self.handle_execute_query(params.arguments).await,
            "execute_command" => self.handle_execute_command(params.arguments).await,
            "get_schema" => self.handle_get_schema(params.arguments).await,
            "begin_transaction" => self.handle_begin_transaction(params.arguments).await,
            _ => Err(McpError::InvalidRequest(format!("Unknown tool: {}", params.name))),
        }
    }
}

```

## Phase 4: 設定とテスト

### 4.1 設定拡張

**ファイル**: `mcp-config-database.toml.example`

```toml

type = "database"
database_type = "postgresql"
name = "Main PostgreSQL Database"
enabled = true

[handlers.postgres_main.connection]
host = "${POSTGRES_HOST}"
port = 5432
database = "${POSTGRES_DB}"
username = "${POSTGRES_USER}"
password = "${POSTGRES_PASSWORD}"
ssl_mode = "require"
timeout_seconds = 60
retry_attempts = 3

[handlers.postgres_main.pool]
max_connections = 20
min_connections = 5
connection_timeout = 30
idle_timeout = 300
max_lifetime = 3600

[handlers.postgres_main.security]
enable_sql_injection_detection = true
enable_query_whitelist = true
enable_audit_logging = true
threat_intelligence_enabled = true

[handlers.postgres_main.features]
enable_transactions = true
enable_prepared_statements = true
enable_stored_procedures = true
query_timeout = 30
max_query_length = 10000

```

### 4.2 テスト戦略

**ファイル**: `tests/database_handler_tests.rs`

- 単体テスト：各エンジンの機能テスト
- 統合テスト：MCP経由でのDB操作テスト
- セキュリティテスト：SQLインジェクション対策テスト
- パフォーマンステスト：接続プール、クエリ実行速度
- 負荷テスト：並行接続とトランザクション処理

## 📊 サポート予定のデータベース

## 優先度1（Phase 2で実装）

- **PostgreSQL**: 高機能リレーショナルDB
- **MySQL**: 広く使用されるリレーショナルDB

## 優先度2（Phase 5で実装）

- **SQLite**: 軽量組み込みDB
- **MongoDB**: ドキュメント指向NoSQL

## 優先度3（Phase 6で実装）

- **Redis**: キー・バリューストア
- **ClickHouse**: 分析用カラム型DB

## 🔒 セキュリティ機能

## SQLインジェクション対策

- パラメータ化クエリの強制
- 動的SQL構築の制限
- 入力値サニタイゼーション

## 認証・認可

- データベース接続認証
- テーブル・カラムレベルアクセス制御
- ロールベースアクセス制御（RBAC）

## 監査ログ

- 全クエリ実行履歴
- 接続・切断イベント
- セキュリティ違反検知

## 脅威インテリジェンス連携

- 悪意のあるクエリパターン検知
- 異常なアクセスパターン監視
- リアルタイム脅威分析

## 🚀 次のステップ

1. **設計レビュー**: アーキテクチャの詳細検討
2. **PostgreSQL実装**: 最初のDB対応実装
3. **セキュリティテスト**: 包括的なセキュリティ検証
4. **パフォーマンス最適化**: 接続プールとクエリ最適化
5. **他DBエンジン対応**: MySQL、SQLite等の順次実装

この設計により、安全で高性能なマルチデータベース対応MCPサーバーが実現できます。
