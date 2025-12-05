//! SQLite Prepared Statement Implementation
//!
//! Provides prepared statement support for SQLite

use crate::handlers::database::{
    engine::PreparedStatement,
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::{Column, Row, TypeInfo};
use std::sync::Arc;

/// SQLite Prepared Statement
pub struct SqlitePreparedStatement {
    pool: Arc<SqlitePool>,
    sql: String,
}

impl SqlitePreparedStatement {
    /// Create a new prepared statement
    pub async fn new(pool: Arc<SqlitePool>, sql: String) -> Result<Self, DatabaseError> {
        Ok(Self { pool, sql })
    }

    /// Convert sqlx rows to QueryResult
    async fn rows_to_query_result(
        rows: Vec<SqliteRow>,
        execution_time_ms: u64,
    ) -> Result<QueryResult, DatabaseError> {
        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                total_rows: Some(0),
                execution_time_ms,
            });
        }

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

    /// Extract value from row
    fn extract_value(
        row: &SqliteRow,
        idx: usize,
        type_info: &sqlx::sqlite::SqliteTypeInfo,
    ) -> Result<Value, DatabaseError> {
        use sqlx::ValueRef;

        let value_ref = row.try_get_raw(idx).map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to get value at index {}: {}", idx, e))
        })?;

        if value_ref.is_null() {
            return Ok(Value::Null);
        }

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
                // Unknown type, return Null
                Ok(Value::Null)
            }
        }
    }
}

#[async_trait]
impl PreparedStatement for SqlitePreparedStatement {
    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let start = std::time::Instant::now();

        let mut query = sqlx::query(&self.sql);
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

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: Some(Value::Int(result.last_insert_rowid())),
            execution_time_ms: elapsed.as_millis() as u64,
        })
    }

    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let start = std::time::Instant::now();

        let mut query = sqlx::query(&self.sql);
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

    async fn close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn create_test_pool() -> Arc<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        Arc::new(pool)
    }

    #[tokio::test]
    async fn test_prepared_statement_creation() {
        let pool = create_test_pool().await;
        let stmt = SqlitePreparedStatement::new(pool, "SELECT 1".to_string()).await;
        assert!(stmt.is_ok());
    }

    #[tokio::test]
    async fn test_prepared_statement_query() {
        let pool = create_test_pool().await;
        let stmt = SqlitePreparedStatement::new(pool, "SELECT ? as value".to_string())
            .await
            .unwrap();

        let result = stmt.query(&[Value::Int(42)]).await;
        assert!(result.is_ok());

        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_prepared_statement_execute() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&*pool)
            .await
            .unwrap();

        let stmt =
            SqlitePreparedStatement::new(pool, "INSERT INTO test (value) VALUES (?)".to_string())
                .await
                .unwrap();

        let result = stmt.execute(&[Value::String("test".to_string())]).await;
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert_eq!(exec_result.rows_affected, 1);
    }

    #[tokio::test]
    async fn test_prepared_statement_multiple_params() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)")
            .execute(&*pool)
            .await
            .unwrap();

        let stmt = SqlitePreparedStatement::new(
            pool.clone(),
            "INSERT INTO test (name, age) VALUES (?, ?)".to_string(),
        )
        .await
        .unwrap();

        let result = stmt
            .execute(&[Value::String("Alice".to_string()), Value::Int(30)])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_prepared_statement_null_value() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&*pool)
            .await
            .unwrap();

        let stmt =
            SqlitePreparedStatement::new(pool, "INSERT INTO test (value) VALUES (?)".to_string())
                .await
                .unwrap();

        let result = stmt.execute(&[Value::Null]).await;
        assert!(result.is_ok());
    }
}
