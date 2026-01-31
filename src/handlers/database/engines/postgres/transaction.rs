//! PostgreSQL Transaction Management
//!
//! Advanced transaction features including savepoints, isolation levels,
//! and deadlock detection.

use crate::handlers::database::{
    engine::{DatabaseTransaction, IsolationLevel},
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use sqlx::{Column, PgPool, Postgres, Row, Transaction as SqlxTransaction, TypeInfo};
use std::sync::Arc;
use tokio::sync::Mutex;

/// PostgreSQL transaction with advanced features
pub struct PostgresTransaction {
    /// Underlying sqlx transaction
    tx: Arc<Mutex<Option<SqlxTransaction<'static, Postgres>>>>,
    /// Active savepoints
    savepoints: Vec<String>,
    /// Transaction isolation level
    isolation_level: Option<IsolationLevel>,
    /// Transaction start time
    start_time: std::time::Instant,
}

impl PostgresTransaction {
    /// Create a new PostgreSQL transaction
    pub async fn begin(pool: &PgPool) -> Result<Self, DatabaseError> {
        let tx = pool
            .begin()
            .await
            .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to begin transaction: {}", e)))?;

        Ok(Self {
            tx: Arc::new(Mutex::new(Some(tx))),
            savepoints: Vec::new(),
            isolation_level: None,
            start_time: std::time::Instant::now(),
        })
    }

    /// Create a transaction with specific isolation level
    pub async fn begin_with_isolation(
        pool: &PgPool,
        level: IsolationLevel,
    ) -> Result<Self, DatabaseError> {
        let tx = Self::begin(pool).await?;
        tx.set_isolation_level(level).await?;
        Ok(tx)
    }

    /// Get transaction duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    /// Check if transaction is still active
    pub async fn is_active(&self) -> bool {
        self.tx.lock().await.is_some()
    }

    /// Create a savepoint
    ///
    /// # Arguments
    ///
    /// * `name` - Savepoint name
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use mcp_rs::handlers::database::engines::postgres::PostgresTransaction;
    /// # async fn example(tx: &mut PostgresTransaction) -> Result<(), Box<dyn std::error::Error>> {
    /// tx.create_savepoint("sp1").await?;
    /// // ... perform operations
    /// tx.rollback_to_savepoint("sp1").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_savepoint(&mut self, name: &str) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            sqlx::query(&format!("SAVEPOINT {}", name))
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to create savepoint: {}", e)))?;

            self.savepoints.push(name.to_string());
            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    /// Rollback to a savepoint
    pub async fn rollback_to_savepoint(&mut self, name: &str) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            sqlx::query(&format!("ROLLBACK TO SAVEPOINT {}", name))
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to rollback to savepoint: {}", e)))?;

            // Remove savepoints after the rolled back one
            if let Some(pos) = self.savepoints.iter().position(|sp| sp == name) {
                self.savepoints.truncate(pos + 1);
            }

            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    /// Release a savepoint
    pub async fn release_savepoint(&mut self, name: &str) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            sqlx::query(&format!("RELEASE SAVEPOINT {}", name))
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to release savepoint: {}", e)))?;

            // Remove the savepoint and all after it
            if let Some(pos) = self.savepoints.iter().position(|sp| sp == name) {
                self.savepoints.truncate(pos);
            }

            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    /// Set transaction isolation level
    ///
    /// Must be called before any queries in the transaction
    pub async fn set_isolation_level(&mut self, level: IsolationLevel) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            let sql = match level {
                IsolationLevel::ReadUncommitted => "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
                IsolationLevel::ReadCommitted => "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
                IsolationLevel::RepeatableRead => "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
                IsolationLevel::Serializable => "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
            };

            sqlx::query(sql)
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to set isolation level: {}", e)))?;

            self.isolation_level = Some(level);
            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    /// Execute a query within the transaction
    async fn execute_internal(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            let mut query = sqlx::query(sql);
            
            // Bind parameters
            for param in params {
                query = bind_value(query, param)?;
            }

            let result = query
                .execute(&mut **tx)
                .await
                .map_err(|e| {
                    // Check for deadlock
                    if e.to_string().contains("deadlock detected") {
                        DatabaseError::DeadlockDetected(e.to_string())
                    } else {
                        DatabaseError::QueryFailed(e.to_string())
                    }
                })?;

            Ok(ExecuteResult {
                rows_affected: result.rows_affected(),
                last_insert_id: None,
                execution_time_ms: 0,
            })
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    /// Query within the transaction
    async fn query_internal(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            let mut query = sqlx::query(sql);
            
            // Bind parameters
            for param in params {
                query = bind_value(query, param)?;
            }

            let rows = query
                .fetch_all(&mut **tx)
                .await
                .map_err(|e| {
                    // Check for deadlock
                    if e.to_string().contains("deadlock detected") {
                        DatabaseError::DeadlockDetected(e.to_string())
                    } else {
                        DatabaseError::QueryFailed(e.to_string())
                    }
                })?;

            // Convert to QueryResult
            let columns: Vec<crate::handlers::database::types::ColumnInfo> = if let Some(first_row) = rows.first() {
                first_row.columns().iter().map(|c| {
                    crate::handlers::database::types::ColumnInfo {
                        name: Column::name(c).to_string(),
                        data_type: Column::type_info(c).name().to_string(),
                        nullable: true, // PostgreSQL doesn't provide this info easily from query result
                        max_length: None,
                    }
                }).collect()
            } else {
                Vec::new()
            };

            let data: Vec<Vec<Value>> = rows
                .iter()
                .map(|row| {
                    (0..columns.len())
                        .map(|i| {
                            // Try to get value as different types
                            if let Ok(val) = row.try_get::<String, _>(i) {
                                Value::String(val)
                            } else if let Ok(val) = row.try_get::<i64, _>(i) {
                                Value::Int(val)
                            } else if let Ok(val) = row.try_get::<i32, _>(i) {
                                Value::Int(val as i64)
                            } else if let Ok(val) = row.try_get::<f64, _>(i) {
                                Value::Float(val)
                            } else if let Ok(val) = row.try_get::<bool, _>(i) {
                                Value::Bool(val)
                            } else if let Ok(val) = row.try_get::<Vec<u8>, _>(i) {
                                Value::Binary(val)
                            } else {
                                Value::Null
                            }
                        })
                        .collect()
                })
                .collect();

            Ok(QueryResult {
                columns,
                rows: data,
                total_rows: Some(rows.len() as u64),
                execution_time_ms: 0, // Will be set by caller if needed
            })
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    /// Get number of active savepoints
    pub fn savepoint_count(&self) -> usize {
        self.savepoints.len()
    }

    /// Get current isolation level
    pub fn isolation_level(&self) -> Option<IsolationLevel> {
        self.isolation_level
    }
}

#[async_trait]
impl DatabaseTransaction for PostgresTransaction {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        self.query_internal(sql, params).await
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        self.execute_internal(sql, params).await
    }

    async fn savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        // Need mutable access, but trait requires &self
        // This is a limitation - we'll document that create_savepoint should be used instead
        Err(DatabaseError::UnsupportedOperation(
            "Use create_savepoint instead".to_string()
        ))
    }

    async fn rollback_to_savepoint(&self, name: &str) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            sqlx::query(&format!("ROLLBACK TO SAVEPOINT {}", name))
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to rollback to savepoint: {}", e)))?;

            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    async fn release_savepoint(&self, name: &str) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            sqlx::query(&format!("RELEASE SAVEPOINT {}", name))
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to release savepoint: {}", e)))?;

            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.take() {
            tx.commit()
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to commit: {}", e)))?;
            
            tracing::debug!("Transaction committed after {}ms", self.duration_ms());
            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.take() {
            tx.rollback()
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to rollback: {}", e)))?;
            
            tracing::debug!("Transaction rolled back after {}ms", self.duration_ms());
            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    async fn set_isolation_level(&self, level: IsolationLevel) -> Result<(), DatabaseError> {
        let mut tx_guard = self.tx.lock().await;
        
        if let Some(tx) = tx_guard.as_mut() {
            let sql = match level {
                IsolationLevel::ReadUncommitted => "SET TRANSACTION ISOLATION LEVEL READ UNCOMMITTED",
                IsolationLevel::ReadCommitted => "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
                IsolationLevel::RepeatableRead => "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ",
                IsolationLevel::Serializable => "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
            };

            sqlx::query(sql)
                .execute(&mut **tx)
                .await
                .map_err(|e| DatabaseError::TransactionFailed(format!("Failed to set isolation level: {}", e)))?;

            Ok(())
        } else {
            Err(DatabaseError::TransactionFailed("Transaction is not active".to_string()))
        }
    }

    fn transaction_info(&self) -> crate::handlers::database::engine::TransactionInfo {
        crate::handlers::database::engine::TransactionInfo {
            transaction_id: uuid::Uuid::new_v4().to_string(),
            isolation_level: self.isolation_level.unwrap_or(IsolationLevel::ReadCommitted),
            started_at: chrono::Utc::now() - chrono::Duration::milliseconds(self.duration_ms() as i64),
            savepoints: self.savepoints.clone(),
            is_read_only: false,
        }
    }
}

/// Helper function to bind a value to a query
fn bind_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &'q Value,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>, DatabaseError> {
    match value {
        Value::Null => Ok(query.bind(None::<String>)),
        Value::Bool(b) => Ok(query.bind(b)),
        Value::Int(i) => Ok(query.bind(i)),
        Value::Float(f) => Ok(query.bind(f)),
        Value::String(s) => Ok(query.bind(s.as_str())),
        Value::Binary(b) => Ok(query.bind(b.as_slice())),
        Value::Json(j) => Ok(query.bind(sqlx::types::Json(j))),
        Value::DateTime(dt) => Ok(query.bind(dt)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolation_level_display() {
        assert_eq!(IsolationLevel::ReadUncommitted.to_string(), "READ UNCOMMITTED");
        assert_eq!(IsolationLevel::ReadCommitted.to_string(), "READ COMMITTED");
        assert_eq!(IsolationLevel::RepeatableRead.to_string(), "REPEATABLE READ");
        assert_eq!(IsolationLevel::Serializable.to_string(), "SERIALIZABLE");
    }
}
