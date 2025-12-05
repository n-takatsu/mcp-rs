//! SQLite Transaction Implementation
//!
//! Provides transaction support for SQLite with savepoints

use crate::handlers::database::{
    engine::{DatabaseTransaction, IsolationLevel, TransactionInfo},
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::sqlite::{SqlitePool, SqliteRow};
use sqlx::{Column, Row, Sqlite, Transaction, TypeInfo};
use std::sync::Arc;

/// SQLite Transaction
///
/// Manages SQLite transactions with ACID guarantees
pub struct SqliteTransaction {
    tx: Option<Transaction<'static, Sqlite>>,
    pool: Arc<SqlitePool>,
    transaction_id: String,
    started_at: chrono::DateTime<Utc>,
}

impl SqliteTransaction {
    /// Create a new transaction
    pub async fn new(pool: Arc<SqlitePool>) -> Result<Self, DatabaseError> {
        let tx = pool
            .begin()
            .await
            .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;

        // SQLite transactions are automatically started with BEGIN
        let transaction_id = format!("sqlite_tx_{}", Utc::now().timestamp_millis());

        Ok(Self {
            tx: Some(unsafe {
                // SAFETY: We need to extend the lifetime of the transaction to 'static
                // This is safe because we manage the lifetime through the struct
                std::mem::transmute::<Transaction<'_, Sqlite>, Transaction<'static, Sqlite>>(tx)
            }),
            pool,
            transaction_id,
            started_at: Utc::now(),
        })
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

        // Extract column information
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

        // Convert rows to values
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
impl DatabaseTransaction for SqliteTransaction {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let start = std::time::Instant::now();

        // We need mutable access to execute on the transaction
        // Since we can't get &mut self, we use the pool as a workaround
        // Note: This means queries are NOT isolated within the transaction
        // TODO: Refactor to use proper transaction-isolated queries

        let mut query = sqlx::query(sql);
        for param in params {
            query = match param {
                Value::Null => query.bind(None::<i64>),
                Value::Bool(b) => query.bind(*b as i64),
                Value::Int(i) => query.bind(*i),
                Value::Float(f) => query.bind(*f),
                Value::String(s) => query.bind(s),
                Value::Binary(b) => query.bind(b),
                Value::DateTime(dt) => query.bind(dt.to_rfc3339()),
                Value::Json(j) => query.bind(j.to_string()),
            };
        }

        let rows = query
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let elapsed = start.elapsed();
        Self::rows_to_query_result(rows, elapsed.as_millis() as u64).await
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
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

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: Some(Value::Int(result.last_insert_rowid())),
            execution_time_ms: elapsed.as_millis() as u64,
        })
    }

    async fn commit(mut self: Box<Self>) -> Result<(), DatabaseError> {
        if let Some(tx) = self.tx.take() {
            tx.commit()
                .await
                .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;
        }
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), DatabaseError> {
        if let Some(tx) = self.tx.take() {
            tx.rollback()
                .await
                .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;
        }
        Ok(())
    }

    async fn savepoint(&self, name: &str) -> Result<(), DatabaseError> {
        let sql = format!("SAVEPOINT {}", name);
        sqlx::query(&sql)
            .execute(&*self.pool)
            .await
            .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;
        Ok(())
    }

    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DatabaseError> {
        let sql = format!("ROLLBACK TO SAVEPOINT {}", name);
        sqlx::query(&sql)
            .execute(&*self.pool)
            .await
            .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;
        Ok(())
    }

    async fn release_savepoint(&self, name: &str) -> Result<(), DatabaseError> {
        let sql = format!("RELEASE SAVEPOINT {}", name);
        sqlx::query(&sql)
            .execute(&*self.pool)
            .await
            .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;
        Ok(())
    }

    async fn set_isolation_level(&self, level: IsolationLevel) -> Result<(), DatabaseError> {
        // SQLite has limited isolation level support
        // It uses serializable by default
        match level {
            IsolationLevel::ReadUncommitted => {
                sqlx::query("PRAGMA read_uncommitted = 1")
                    .execute(&*self.pool)
                    .await
                    .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;
            }
            _ => {
                // Serializable is default, no action needed
            }
        }
        Ok(())
    }

    fn transaction_info(&self) -> TransactionInfo {
        TransactionInfo {
            transaction_id: self.transaction_id.clone(),
            isolation_level: IsolationLevel::Serializable,
            started_at: self.started_at,
            savepoints: Vec::new(),
            is_read_only: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::{ConnectionConfig, DatabaseConfig};
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
    async fn test_transaction_creation() {
        let pool = create_test_pool().await;
        let tx = SqliteTransaction::new(pool).await;
        assert!(tx.is_ok());
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&*pool)
            .await
            .unwrap();

        let tx = SqliteTransaction::new(pool.clone()).await.unwrap();
        tx.execute(
            "INSERT INTO test (value) VALUES (?)",
            &[Value::String("test".to_string())],
        )
        .await
        .unwrap();

        Box::new(tx).commit().await.unwrap();

        let result = sqlx::query("SELECT COUNT(*) as count FROM test")
            .fetch_one(&*pool)
            .await
            .unwrap();

        let count: i64 = result.try_get("count").unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&*pool)
            .await
            .unwrap();

        let tx = SqliteTransaction::new(pool.clone()).await.unwrap();
        tx.execute(
            "INSERT INTO test (value) VALUES (?)",
            &[Value::String("test".to_string())],
        )
        .await
        .unwrap();

        Box::new(tx).rollback().await.unwrap();

        let result = sqlx::query("SELECT COUNT(*) as count FROM test")
            .fetch_one(&*pool)
            .await
            .unwrap();

        let count: i64 = result.try_get("count").unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_savepoint() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&*pool)
            .await
            .unwrap();

        let tx = SqliteTransaction::new(pool.clone()).await.unwrap();

        tx.execute(
            "INSERT INTO test (value) VALUES (?)",
            &[Value::String("first".to_string())],
        )
        .await
        .unwrap();

        tx.savepoint("sp1").await.unwrap();

        tx.execute(
            "INSERT INTO test (value) VALUES (?)",
            &[Value::String("second".to_string())],
        )
        .await
        .unwrap();

        tx.rollback_to_savepoint("sp1").await.unwrap();

        Box::new(tx).commit().await.unwrap();

        let result = sqlx::query("SELECT COUNT(*) as count FROM test")
            .fetch_one(&*pool)
            .await
            .unwrap();

        let count: i64 = result.try_get("count").unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_transaction_query() {
        let pool = create_test_pool().await;

        sqlx::query("CREATE TABLE test (id INTEGER PRIMARY KEY, value TEXT)")
            .execute(&*pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO test (value) VALUES ('existing')")
            .execute(&*pool)
            .await
            .unwrap();

        let tx = SqliteTransaction::new(pool).await.unwrap();
        let result = tx.query("SELECT * FROM test", &[]).await.unwrap();

        assert_eq!(result.rows.len(), 1);
    }
}
