//! Database Types and Common Structures
//!
//! データベースハンドラーで使用される共通の型定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// データベースタイプ
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    SQLite,
    MongoDB,
    Redis,
    ClickHouse,
}

impl std::fmt::Display for DatabaseType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseType::PostgreSQL => write!(f, "postgresql"),
            DatabaseType::MySQL => write!(f, "mysql"),
            DatabaseType::SQLite => write!(f, "sqlite"),
            DatabaseType::MongoDB => write!(f, "mongodb"),
            DatabaseType::Redis => write!(f, "redis"),
            DatabaseType::ClickHouse => write!(f, "clickhouse"),
        }
    }
}

/// データベース設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// データベースタイプ
    pub database_type: DatabaseType,
    /// 接続設定
    pub connection: ConnectionConfig,
    /// プール設定
    pub pool: PoolConfig,
    /// セキュリティ設定
    pub security: SecurityConfig,
    /// 機能設定
    pub features: FeatureConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            database_type: DatabaseType::PostgreSQL,
            connection: ConnectionConfig::default(),
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }
}

/// 接続設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: Option<String>,
    pub timeout_seconds: u32,
    pub retry_attempts: u8,
    pub options: HashMap<String, String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            database: "postgres".to_string(),
            username: "postgres".to_string(),
            password: "".to_string(),
            ssl_mode: None,
            timeout_seconds: 30,
            retry_attempts: 3,
            options: HashMap::new(),
        }
    }
}

/// 接続プール設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout: u32,
    pub idle_timeout: u32,
    pub max_lifetime: u32,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 20,
            min_connections: 5,
            connection_timeout: 30,
            idle_timeout: 300,
            max_lifetime: 3600,
        }
    }
}

/// セキュリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub enable_sql_injection_detection: bool,
    pub enable_query_whitelist: bool,
    pub enable_audit_logging: bool,
    pub threat_intelligence_enabled: bool,
    pub max_query_length: usize,
    pub allowed_operations: Vec<QueryType>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_sql_injection_detection: true,
            enable_query_whitelist: false,
            enable_audit_logging: true,
            threat_intelligence_enabled: true,
            max_query_length: 10000,
            allowed_operations: vec![
                QueryType::Select,
                QueryType::Insert,
                QueryType::Update,
                QueryType::Delete,
            ],
        }
    }
}

/// 機能設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConfig {
    pub enable_transactions: bool,
    pub enable_prepared_statements: bool,
    pub enable_stored_procedures: bool,
    pub query_timeout: u32,
    pub enable_query_caching: bool,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            enable_transactions: true,
            enable_prepared_statements: true,
            enable_stored_procedures: true,
            query_timeout: 30,
            enable_query_caching: false,
        }
    }
}

/// データベース機能
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseFeature {
    Transactions,
    PreparedStatements,
    StoredProcedures,
    JsonSupport,
    FullTextSearch,
    Replication,
    Sharding,
    Acid,
    EventualConsistency,
}

/// クエリタイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryType {
    Select,
    Insert,
    Update,
    Delete,
    Create,
    Drop,
    Alter,
    StoredProcedure,
    Transaction, // 新しく追加
    Ddl,         // 新しく追加
    Unknown,     // 新しく追加
    Custom(String),
}

/// データベース値
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Binary(Vec<u8>),
    Json(serde_json::Value),
    DateTime(DateTime<Utc>),
}

impl Value {
    /// Rustの標準型からValueに変換
    pub fn from_string(s: String) -> Self {
        Value::String(s)
    }

    pub fn from_i32(i: i32) -> Self {
        Value::Int(i as i64)
    }

    pub fn from_i64(i: i64) -> Self {
        Value::Int(i)
    }

    pub fn from_f64(f: f64) -> Self {
        Value::Float(f)
    }

    pub fn from_bool(b: bool) -> Self {
        Value::Bool(b)
    }
}

/// クエリ結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// カラム情報
    pub columns: Vec<ColumnInfo>,
    /// 行データ
    pub rows: Vec<Vec<Value>>,
    /// 総行数（ページング用）
    pub total_rows: Option<u64>,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
}

/// カラム情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub max_length: Option<i32>,
}

/// コマンド実行結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// 影響を受けた行数
    pub rows_affected: u64,
    /// 最後に挿入されたID（AUTO_INCREMENTなど）
    pub last_insert_id: Option<Value>,
    /// 実行時間（ミリ秒）
    pub execution_time_ms: u64,
}

/// データベーススキーマ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSchema {
    /// データベース名
    pub database_name: String,
    /// テーブル情報
    pub tables: Vec<TableInfo>,
    /// ビュー情報
    pub views: Vec<ViewInfo>,
    /// ストアドプロシージャ情報
    pub procedures: Vec<ProcedureInfo>,
}

/// テーブル情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub schema: Option<String>,
    pub columns: Vec<ColumnInfo>,
    pub primary_keys: Vec<String>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub indexes: Vec<IndexInfo>,
}

/// ビュー情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewInfo {
    pub name: String,
    pub schema: Option<String>,
    pub definition: String,
    pub columns: Vec<ColumnInfo>,
}

/// ストアドプロシージャ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcedureInfo {
    pub name: String,
    pub schema: Option<String>,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
}

/// 外部キー情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyInfo {
    pub name: String,
    pub column: String,
    pub referenced_table: String,
    pub referenced_column: String,
}

/// インデックス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
}

/// パラメータ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInfo {
    pub name: String,
    pub data_type: String,
    pub direction: ParameterDirection,
    pub default_value: Option<Value>,
}

/// パラメータ方向
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterDirection {
    In,
    Out,
    InOut,
}

/// 健全性ステータス
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: HealthStatusType,
    pub last_check: DateTime<Utc>,
    pub response_time_ms: u64,
    pub error_message: Option<String>,
    pub connection_count: u32,
    pub active_transactions: u32,
}

/// 健全性ステータスタイプ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatusType {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// クエリコンテキスト
#[derive(Debug, Clone)]
pub struct QueryContext {
    pub query_type: QueryType,
    pub user_id: Option<String>,
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub source_ip: Option<String>,
    pub client_info: Option<String>,
}

impl QueryContext {
    pub fn new(query_type: QueryType) -> Self {
        Self {
            query_type,
            user_id: None,
            session_id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            source_ip: None,
            client_info: None,
        }
    }
}

/// データベースエラー
#[derive(Debug, Clone, thiserror::Error)]
pub enum DatabaseError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Query execution failed: {0}")]
    QueryFailed(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    #[error("Configuration validation failed: {0}")]
    ConfigValidationError(String),

    #[error("Pool error: {0}")]
    PoolError(String),

    #[error("Timeout error: {0}")]
    TimeoutError(String),

    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    // 新しく追加するエラーバリアント
    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),

    #[error("Failover failed: {0}")]
    FailoverFailed(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("No available endpoints: {0}")]
    NoAvailableEndpoints(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Server unavailable: {0}")]
    ServerUnavailable(String),

    #[error("Deadlock detected: {0}")]
    DeadlockDetected(String),

    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("SQL syntax error: {0}")]
    SqlSyntaxError(String),

    #[error("Data conversion error: {0}")]
    ConversionError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// セキュリティエラー
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("SQL injection detected: {0}")]
    SqlInjectionDetected(String),

    #[error("Query not in whitelist: {0}")]
    QueryNotWhitelisted(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    #[error("Threat detected: {0}")]
    ThreatDetected(String),
}

/// 検証結果
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Approved,
    Denied(String),
    Warning(String),
}
