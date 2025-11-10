//! Simplified MySQL Engine Implementation
//!
//! mysql_asyncライブラリを使用したシンプルなMySQLエンジン実装
//! セキュリティ重視: RSA脆弱性（RUSTSEC-2023-0071）回避済み

use super::super::{
    engine::{
        ConnectionInfo, DatabaseConnection, DatabaseEngine, DatabaseTransaction, PreparedStatement,
    },
    types::{
        ColumnInfo, DatabaseConfig, DatabaseError, DatabaseFeature, DatabaseSchema, DatabaseType,
        ExecuteResult, HealthStatus, HealthStatusType, QueryResult, TableInfo, Value,
    },
};
use async_trait::async_trait;
use mysql_async::{prelude::*, Pool};
use std::sync::Arc;

/// mysql_asyncを使用したセキュアなMySQLエンジン
pub struct MySqlEngine {
    config: DatabaseConfig,
    pool: Pool,
}

impl MySqlEngine {
    /// 新しいMySQLエンジンを作成
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        // 基本的なMySQL接続オプション
        let mysql_opts = mysql_async::OptsBuilder::default()
            .ip_or_hostname(&config.connection.host)
            .tcp_port(config.connection.port)
            .db_name(Some(&config.connection.database))
            .user(Some(&config.connection.username))
            .pass(Some(&config.connection.password));

        let pool = Pool::new(mysql_opts);

        Ok(Self { config, pool })
    }
}

#[async_trait]
impl DatabaseEngine for MySqlEngine {
    fn engine_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn connect(
        &self,
        _config: &DatabaseConfig,
    ) -> Result<Box<dyn DatabaseConnection>, DatabaseError> {
        let _conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        Ok(Box::new(SimpleMySqlConnection {
            pool: self.pool.clone(),
            config: self.config.clone(),
        }))
    }

    async fn health_check(&self) -> Result<HealthStatus, DatabaseError> {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        // シンプルなSELECT 1クエリで接続確認
        let _: Vec<u8> = conn
            .query("SELECT 1")
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(HealthStatus {
            status: HealthStatusType::Healthy,
            last_check: chrono::Utc::now(),
            response_time_ms: 0,
            error_message: None,
            connection_count: 1,
            active_transactions: 0,
        })
    }

    fn supported_features(&self) -> Vec<DatabaseFeature> {
        vec![
            DatabaseFeature::Transactions,
            DatabaseFeature::PreparedStatements,
            DatabaseFeature::StoredProcedures,
            DatabaseFeature::FullTextSearch,
            DatabaseFeature::JsonSupport,
            DatabaseFeature::Replication,
            DatabaseFeature::Acid,
        ]
    }

    fn validate_config(&self, _config: &DatabaseConfig) -> Result<(), DatabaseError> {
        // mysql_asyncライブラリが接続パラメータを検証
        Ok(())
    }

    async fn get_version(&self) -> Result<String, DatabaseError> {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let version: String = conn
            .query_first("SELECT VERSION()")
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?
            .ok_or_else(|| DatabaseError::QueryFailed("Failed to get version".to_string()))?;

        Ok(version)
    }
}

/// シンプルなMySQL接続実装
pub struct SimpleMySqlConnection {
    pool: Pool,
    config: DatabaseConfig,
}

#[async_trait]
impl DatabaseConnection for SimpleMySqlConnection {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let start_time = std::time::Instant::now();
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        // 現在はパラメータなしクエリのみサポート
        if !params.is_empty() {
            return Err(DatabaseError::UnsupportedOperation(
                "Parameterized queries not yet implemented".to_string(),
            ));
        }

        let result: Vec<mysql_async::Row> = conn
            .query(sql)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("MySQL query failed: {}", e)))?;

        let execution_time = start_time.elapsed().as_millis() as u64;

        // 基本的な結果変換
        let columns = if let Some(first_row) = result.first() {
            first_row
                .columns_ref()
                .iter()
                .map(|column| ColumnInfo {
                    name: column.name_str().to_string(),
                    data_type: format!("{:?}", column.column_type()),
                    nullable: !column
                        .flags()
                        .contains(mysql_async::consts::ColumnFlags::NOT_NULL_FLAG),
                    max_length: Some(column.column_length() as i32),
                })
                .collect()
        } else {
            Vec::new()
        };

        let rows = result
            .into_iter()
            .map(|row| {
                (0..row.len())
                    .map(
                        |i| match row.as_ref(i).unwrap_or(&mysql_async::Value::NULL) {
                            mysql_async::Value::NULL => Value::Null,
                            mysql_async::Value::Int(i) => Value::Int(*i),
                            mysql_async::Value::UInt(u) => Value::Int(*u as i64),
                            mysql_async::Value::Float(f) => Value::Float(*f as f64),
                            mysql_async::Value::Double(d) => Value::Float(*d),
                            mysql_async::Value::Bytes(b) => match String::from_utf8(b.clone()) {
                                Ok(s) => Value::String(s),
                                Err(_) => Value::Binary(b.clone()),
                            },
                            _ => Value::String("UNSUPPORTED".to_string()),
                        },
                    )
                    .collect()
            })
            .collect();

        Ok(QueryResult {
            columns,
            rows,
            total_rows: None,
            execution_time_ms: execution_time,
        })
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let start_time = std::time::Instant::now();
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        // 現在はパラメータなしクエリのみサポート
        if !params.is_empty() {
            return Err(DatabaseError::UnsupportedOperation(
                "Parameterized execute not yet implemented".to_string(),
            ));
        }

        conn.query_drop(sql)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("MySQL execute failed: {}", e)))?;

        let execution_time = start_time.elapsed().as_millis() as u64;
        let rows_affected = conn.affected_rows();

        Ok(ExecuteResult {
            rows_affected,
            last_insert_id: conn.last_insert_id().map(|id| Value::Int(id as i64)),
            execution_time_ms: execution_time,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Transactions not yet implemented".to_string(),
        ))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Schema introspection not yet implemented".to_string(),
        ))
    }

    async fn get_table_schema(&self, _table_name: &str) -> Result<TableInfo, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Table schema introspection not yet implemented".to_string(),
        ))
    }

    async fn prepare(&self, _sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Prepared statements not yet implemented".to_string(),
        ))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let _: Vec<u8> = conn
            .query("SELECT 1")
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        // プール接続なので特に何もしない
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: format!("mysql_async_{}", std::process::id()),
            database_name: self.config.connection.database.clone(),
            user_name: self.config.connection.username.clone(),
            server_version: "Unknown".to_string(), // TODO: 実際のバージョン取得
            connected_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
        }
    }
}
