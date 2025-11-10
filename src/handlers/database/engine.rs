//! Database Engine Abstraction Layer
//!
//! 異なるデータベースエンジンに対する統一インターフェース

use super::types::{
    DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType, ExecuteResult,
    HealthStatus, QueryContext, QueryResult, ValidationResult, Value,
};
use async_trait::async_trait;
use std::sync::Arc;

/// データベースエンジン抽象化トレイト
///
/// 各データベース（PostgreSQL、MySQL等）の基本機能を抽象化
#[async_trait]
pub trait DatabaseEngine: Send + Sync {
    /// データベースタイプを返す
    fn engine_type(&self) -> DatabaseType;

    /// 新しい接続を作成
    async fn connect(
        &self,
        config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError>;

    /// エンジンの健全性をチェック
    async fn health_check(&self) -> Result<HealthStatus, DatabaseError>;

    /// サポートされる機能一覧を返す
    fn supported_features(&self) -> Vec<DatabaseFeature>;

    /// エンジン固有の設定を検証
    fn validate_config(&self, config: &DatabaseConfig) -> Result<(), DatabaseError>;

    /// エンジンのバージョン情報を取得
    async fn get_version(&self) -> Result<String, DatabaseError>;
}

/// データベース接続抽象化トレイト
///
/// 実際のデータベース操作を提供
#[async_trait]
pub trait DatabaseConnection: Send + Sync {
    /// SELECTクエリを実行
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError>;

    /// INSERT/UPDATE/DELETEコマンドを実行
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;

    /// トランザクションを開始
    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError>;

    /// データベーススキーマ情報を取得
    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError>;

    /// 特定のテーブルスキーマを取得
    async fn get_table_schema(
        &self,
        table_name: &str,
    ) -> Result<super::types::TableInfo, DatabaseError>;

    /// プリペアドステートメントを準備
    async fn prepare(&self, sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError>;

    /// 接続の健全性をチェック
    async fn ping(&self) -> Result<(), DatabaseError>;

    /// 接続を明示的に閉じる
    async fn close(&self) -> Result<(), DatabaseError>;

    /// 接続固有の情報を取得
    fn connection_info(&self) -> ConnectionInfo;
}

/// データベーストランザクション抽象化トレイト
///
/// トランザクション処理を提供
#[async_trait]
pub trait DatabaseTransaction: Send + Sync {
    /// トランザクション内でクエリを実行
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError>;

    /// トランザクション内でコマンドを実行
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;

    /// セーブポイントを作成
    async fn savepoint(&self, name: &str) -> Result<(), DatabaseError>;

    /// セーブポイントまでロールバック
    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DatabaseError>;

    /// セーブポイントを解放
    async fn release_savepoint(&self, name: &str) -> Result<(), DatabaseError>;

    /// トランザクションをコミット
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError>;

    /// トランザクションをロールバック
    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError>;

    /// 分離レベルを設定
    async fn set_isolation_level(&self, level: IsolationLevel) -> Result<(), DatabaseError>;

    /// トランザクション情報を取得
    fn transaction_info(&self) -> TransactionInfo;
}

/// プリペアドステートメント抽象化トレイト
#[async_trait]
pub trait PreparedStatement: Send + Sync {
    /// プリペアドステートメントを実行（SELECT）
    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError>;

    /// プリペアドステートメントを実行（INSERT/UPDATE/DELETE）
    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError>;

    /// ステートメントを破棄
    async fn close(&self) -> Result<(), DatabaseError>;
}

/// 分離レベル
#[derive(Debug, Clone, Copy)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

impl std::fmt::Display for IsolationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IsolationLevel::ReadUncommitted => write!(f, "READ UNCOMMITTED"),
            IsolationLevel::ReadCommitted => write!(f, "READ COMMITTED"),
            IsolationLevel::RepeatableRead => write!(f, "REPEATABLE READ"),
            IsolationLevel::Serializable => write!(f, "SERIALIZABLE"),
        }
    }
}

/// 接続情報
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub connection_id: String,
    pub database_name: String,
    pub user_name: String,
    pub server_version: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

/// トランザクション情報
#[derive(Debug, Clone)]
pub struct TransactionInfo {
    pub transaction_id: String,
    pub isolation_level: IsolationLevel,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub savepoints: Vec<String>,
    pub is_read_only: bool,
}

/// データベースエンジンビルダー
///
/// 設定からデータベースエンジンを構築
pub struct DatabaseEngineBuilder;

impl DatabaseEngineBuilder {
    /// 設定からエンジンを構築
    pub async fn build(config: &DatabaseConfig) -> Result<Arc<dyn DatabaseEngine>, DatabaseError> {
        match config.database_type {
            DatabaseType::PostgreSQL => {
                // PostgreSQLエンジンを作成
                let engine =
                    super::engines::postgresql::PostgreSqlEngine::new(config.clone()).await?;
                Ok(Arc::new(engine))
            }
            #[cfg(feature = "mysql-backend")]
            DatabaseType::MySQL | DatabaseType::MariaDB => {
                // MySQL/MariaDB: mysql_asyncライブラリを使用（RSA脆弱性フリー）
                let engine = super::engines::mysql::MySqlEngine::new(config.clone()).await?;
                Ok(Arc::new(engine))
            }
            #[cfg(not(feature = "mysql-backend"))]
            DatabaseType::MySQL | DatabaseType::MariaDB => {
                Err(DatabaseError::UnsupportedOperation(
                    "MySQL support not compiled. Enable mysql-backend feature.".to_string(),
                ))
            }
            DatabaseType::SQLite => {
                // TODO: SQLite実装
                Err(DatabaseError::UnsupportedOperation(
                    "SQLite engine not yet implemented".to_string(),
                ))
            }
            DatabaseType::MongoDB => {
                // MongoDB実装
                let engine = super::engines::mongodb::MongoEngine::new(config.clone()).await?;
                Ok(Arc::new(engine))
            }
            DatabaseType::Redis => {
                // Redis実装
                let engine = super::engines::redis::RedisEngine::new(config.clone()).await?;
                Ok(Arc::new(engine))
            }
            DatabaseType::ClickHouse => {
                // TODO: ClickHouse実装
                Err(DatabaseError::UnsupportedOperation(
                    "ClickHouse engine not yet implemented".to_string(),
                ))
            }
        }
    }
}

/// エンジンレジストリ
///
/// 利用可能なデータベースエンジンを管理
pub struct EngineRegistry {
    engines: std::collections::HashMap<DatabaseType, Arc<dyn DatabaseEngine>>,
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRegistry {
    pub fn new() -> Self {
        Self {
            engines: std::collections::HashMap::new(),
        }
    }

    /// エンジンを登録
    pub fn register(&mut self, engine: Arc<dyn DatabaseEngine>) {
        let engine_type = engine.engine_type();
        self.engines.insert(engine_type, engine);
    }

    /// エンジンを取得
    pub fn get(&self, db_type: &DatabaseType) -> Option<Arc<dyn DatabaseEngine>> {
        self.engines.get(db_type).cloned()
    }

    /// 利用可能なエンジンタイプを取得
    pub fn available_types(&self) -> Vec<DatabaseType> {
        self.engines.keys().cloned().collect()
    }
}

/// クエリビルダー補助関数
pub mod query_builder {
    use super::*;

    /// SELECTクエリビルダー
    pub struct SelectBuilder {
        table: String,
        columns: Vec<String>,
        where_clause: Option<String>,
        order_by: Option<String>,
        limit: Option<u32>,
        offset: Option<u32>,
    }

    impl SelectBuilder {
        pub fn new(table: &str) -> Self {
            Self {
                table: table.to_string(),
                columns: vec!["*".to_string()],
                where_clause: None,
                order_by: None,
                limit: None,
                offset: None,
            }
        }

        pub fn columns(mut self, columns: &[&str]) -> Self {
            self.columns = columns.iter().map(|s| s.to_string()).collect();
            self
        }

        pub fn where_clause(mut self, clause: &str) -> Self {
            self.where_clause = Some(clause.to_string());
            self
        }

        pub fn order_by(mut self, order: &str) -> Self {
            self.order_by = Some(order.to_string());
            self
        }

        pub fn limit(mut self, limit: u32) -> Self {
            self.limit = Some(limit);
            self
        }

        pub fn offset(mut self, offset: u32) -> Self {
            self.offset = Some(offset);
            self
        }

        pub fn build(self) -> String {
            let mut sql = format!("SELECT {} FROM {}", self.columns.join(", "), self.table);

            if let Some(where_clause) = self.where_clause {
                sql.push_str(&format!(" WHERE {}", where_clause));
            }

            if let Some(order_by) = self.order_by {
                sql.push_str(&format!(" ORDER BY {}", order_by));
            }

            if let Some(limit) = self.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }

            if let Some(offset) = self.offset {
                sql.push_str(&format!(" OFFSET {}", offset));
            }

            sql
        }
    }
}
