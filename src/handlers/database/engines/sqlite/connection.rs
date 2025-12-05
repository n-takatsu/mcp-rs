//! SQLite Connection Implementation
//!
//! Provides SQLite-specific database connection with sqlx integration

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::{DatabaseConfig, DatabaseError, DatabaseSchema, QueryResult, TableInfo, Value},
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::{Column, TypeInfo};
use std::sync::Arc;

use super::{SqlitePreparedStatement, SqliteTransaction};

/// SQLite Connection
///
/// Manages SQLite database connection using sqlx
pub struct SqliteConnection {
    pool: Arc<SqlitePool>,
    config: DatabaseConfig,
}

impl SqliteConnection {
    /// Create a new SQLite connection
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        let database_url = Self::build_connection_url(&config);

        let pool = SqlitePoolOptions::new()
            .max_connections(config.pool.max_connections)
            .connect(&database_url)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            pool: Arc::new(pool),
            config,
        })
    }

    /// Build SQLite connection URL
    fn build_connection_url(config: &DatabaseConfig) -> String {
        // SQLite uses file path in the host field
        // Special cases: ":memory:" for in-memory database
        let path = &config.connection.host;

        if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}", path)
        }
    }

    /// Get the underlying connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Convert sqlx row to QueryResult
    async fn rows_to_query_result(
        rows: Vec<sqlx::sqlite::SqliteRow>,
        execution_time_ms: u64,
    ) -> Result<QueryResult, DatabaseError> {
        use sqlx::Row;

        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                total_rows: Some(0),
                execution_time_ms,
            });
        }

        // Extract column information from the first row
        let columns = rows[0]
            .columns()
            .iter()
            .map(|col| crate::handlers::database::types::ColumnInfo {
                name: col.name().to_string(),
                data_type: format!("{:?}", col.type_info()),
                nullable: true,
                max_length: None,
            })
            .collect();

        // Convert rows to Value vectors
        let mut result_rows = Vec::new();
        for row in rows {
            let mut values = Vec::new();
            for (idx, column) in row.columns().iter().enumerate() {
                let value = Self::extract_value(&row, idx, column.type_info())?;
                values.push(value);
            }
            result_rows.push(values);
        }

        let total_rows = result_rows.len();

        Ok(QueryResult {
            columns,
            rows: result_rows,
            total_rows: Some(total_rows as u64),
            execution_time_ms,
        })
    }

    /// Extract value from SQLite row
    fn extract_value(
        row: &sqlx::sqlite::SqliteRow,
        idx: usize,
        type_info: &sqlx::sqlite::SqliteTypeInfo,
    ) -> Result<Value, DatabaseError> {
        use sqlx::Row;
        use sqlx::ValueRef;

        let value_ref = row.try_get_raw(idx).map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to get value at index {}: {}", idx, e))
        })?;

        if value_ref.is_null() {
            return Ok(Value::Null);
        }

        // SQLite uses dynamic typing, so we check the actual type
        let type_name = type_info.name();

        match type_name {
            "INTEGER" => {
                let val: i64 = row.try_get(idx).map_err(|e| {
                    DatabaseError::QueryFailed(format!("Failed to get INTEGER: {}", e))
                })?;
                Ok(Value::Int(val))
            }
            "REAL" => {
                let val: f64 = row.try_get(idx).map_err(|e| {
                    DatabaseError::QueryFailed(format!("Failed to get REAL: {}", e))
                })?;
                Ok(Value::Float(val))
            }
            "TEXT" => {
                let val: String = row.try_get(idx).map_err(|e| {
                    DatabaseError::QueryFailed(format!("Failed to get TEXT: {}", e))
                })?;
                Ok(Value::String(val))
            }
            "BLOB" => {
                let val: Vec<u8> = row.try_get(idx).map_err(|e| {
                    DatabaseError::QueryFailed(format!("Failed to get BLOB: {}", e))
                })?;
                Ok(Value::Binary(val))
            }
            _ => {
                // Try as string first, then fallback to Null
                if let Ok(val) = row.try_get::<String, _>(idx) {
                    Ok(Value::String(val))
                } else if let Ok(val) = row.try_get::<i64, _>(idx) {
                    Ok(Value::Int(val))
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    /// Convert Value to sqlx parameter
    fn value_to_param(value: &Value) -> Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + '_> {
        match value {
            Value::Null => Box::new(None::<i64>),
            Value::Bool(b) => Box::new(*b as i64),
            Value::Int(i) => Box::new(*i),
            Value::Float(f) => Box::new(*f),
            Value::String(s) => Box::new(s.as_str()),
            Value::Binary(b) => Box::new(b.as_slice()),
            Value::DateTime(dt) => Box::new(dt.to_rfc3339()),
            Value::Json(j) => Box::new(j.to_string()),
            // Uuid not supported in current Value enum
        }
    }
}

#[async_trait]
impl DatabaseConnection for SqliteConnection {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let start = std::time::Instant::now();

        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                Value::Null => query.bind(None::<i64>),
                Value::Bool(b) => query.bind(*b as i64),
                // Removed duplicate pattern
                Value::Int(i) => query.bind(*i),
                Value::Float(f) => query.bind(*f),
                Value::String(s) => query.bind(s),
                Value::Binary(b) => query.bind(b),
                Value::DateTime(dt) => query.bind(dt.to_rfc3339()),
                Value::Json(j) => query.bind(j.to_string()),
                // Uuid not supported
            };
        }

        let rows = query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let elapsed = start.elapsed();
        Self::rows_to_query_result(rows, elapsed.as_millis() as u64).await
    }

    async fn execute(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<crate::handlers::database::types::ExecuteResult, DatabaseError> {
        let start = std::time::Instant::now();

        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                Value::Null => query.bind(None::<i64>),
                Value::Bool(b) => query.bind(*b as i64),
                // Removed duplicate pattern
                Value::Int(i) => query.bind(*i),
                Value::Float(f) => query.bind(*f),
                Value::String(s) => query.bind(s),
                Value::Binary(b) => query.bind(b),
                Value::DateTime(dt) => query.bind(dt.to_rfc3339()),
                Value::Json(j) => query.bind(j.to_string()),
                // Uuid not supported
            };
        }

        let result = query
            .execute(&*self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let elapsed = start.elapsed();

        Ok(crate::handlers::database::types::ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: Some(Value::Int(result.last_insert_rowid())),
            execution_time_ms: elapsed.as_millis() as u64,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let transaction = SqliteTransaction::new(self.pool.clone()).await?;
        Ok(Box::new(transaction))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let query = "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name";
        let result = self.query(query, &[]).await?;

        let mut tables = Vec::new();
        for row in &result.rows {
            if let Some(Value::String(name)) = row.first() {
                if let Ok(table_info) = self.get_table_schema(name).await {
                    tables.push(table_info);
                }
            }
        }

        Ok(DatabaseSchema {
            database_name: self.config.connection.database.clone(),
            tables,
            views: Vec::new(),
            procedures: Vec::new(),
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        let query = format!("PRAGMA table_info({})", table_name);
        let result = self.query(&query, &[]).await?;

        let columns = result
            .rows
            .iter()
            .filter_map(|row| {
                // PRAGMA table_info returns: cid, name, type, notnull, dflt_value, pk
                // Indices: 0=cid(INT), 1=name(TEXT), 2=type(TEXT), 3=notnull(INT), 4=dflt_value, 5=pk(INT)
                if row.len() >= 6 {
                    // Try different indices and types since PRAGMA results can vary
                    let name = match &row[1] {
                        Value::String(s) => Some(s.clone()),
                        Value::Int(i) => Some(i.to_string()),
                        _ => None,
                    };

                    let data_type = match &row[2] {
                        Value::String(s) => Some(s.clone()),
                        Value::Int(i) => Some(i.to_string()),
                        _ => Some("UNKNOWN".to_string()),
                    };

                    if let (Some(n), Some(dt)) = (name, data_type) {
                        Some(crate::handlers::database::types::ColumnInfo {
                            name: n,
                            data_type: dt,
                            nullable: true,
                            max_length: None,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        Ok(TableInfo {
            schema: None,
            name: table_name.to_string(),
            columns,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            primary_keys: Vec::new(),
        })
    }

    async fn prepare(&self, sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        let prepared = SqlitePreparedStatement::new(self.pool.clone(), sql.to_string()).await?;
        Ok(Box::new(prepared))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1")
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        self.pool.close().await;
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: "sqlite".to_string(),
            database_name: self.config.connection.database.clone(),
            user_name: String::new(),
            server_version: "SQLite".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::ConnectionConfig;

    fn create_test_config() -> DatabaseConfig {
        DatabaseConfig {
            connection: ConnectionConfig {
                host: ":memory:".to_string(),
                port: 0,
                username: String::new(),
                password: String::new(),
                database: "test".to_string(),
                ssl_mode: None,
                connect_timeout: None,
                pool_size: None,
            },
            security: None,
        }
    }

    #[tokio::test]
    async fn test_connection_creation() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_ping() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();
        assert!(conn.ping().await.is_ok());
    }

    #[tokio::test]
    async fn test_query() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        let result = conn.query("SELECT 1 as num", &[]).await;
        assert!(result.is_ok());

        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
        assert_eq!(query_result.columns.len(), 1);
    }

    #[tokio::test]
    async fn test_execute() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)", &[])
            .await
            .unwrap();

        let result = conn
            .execute(
                "INSERT INTO test (name) VALUES (?)",
                &[Value::String("test".to_string())],
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().rows_affected, 1);
    }

    #[tokio::test]
    async fn test_get_schema() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY)", &[])
            .await
            .unwrap();

        let schema = conn.get_schema().await.unwrap();
        assert!(schema.tables.contains(&"users".to_string()));
    }

    #[tokio::test]
    async fn test_get_table_schema() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        conn.execute("CREATE TABLE test_table (id INTEGER, name TEXT)", &[])
            .await
            .unwrap();

        let table_info = conn.get_table_schema("test_table").await.unwrap();
        assert_eq!(table_info.name, "test_table");
        assert_eq!(table_info.columns.len(), 2);
    }
}
