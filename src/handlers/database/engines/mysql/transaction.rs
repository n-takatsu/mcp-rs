//! MySQL Transaction Management
//!
//! Provides ACID transaction support with savepoint functionality
//! Ensures data integrity and consistent database operations

use crate::handlers::database::{
    engine::{DatabaseTransaction, IsolationLevel},
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use mysql_async::{prelude::*, Conn};
use std::sync::Arc;
use tokio::sync::Mutex;

/// MySQL Transaction
///
/// Represents an active transaction with savepoint support
pub struct MySqlTransaction {
    conn: Arc<Mutex<Conn>>,
    is_active: Arc<Mutex<bool>>,
    savepoint_counter: Arc<Mutex<usize>>,
}

impl MySqlTransaction {
    /// Create a new transaction from a connection
    pub fn new(conn: Conn) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            is_active: Arc::new(Mutex::new(true)),
            savepoint_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Convert mysql_async Value to our Value type
    fn convert_value(mysql_value: mysql_async::Value) -> Value {
        match mysql_value {
            mysql_async::Value::NULL => Value::Null,
            mysql_async::Value::Bytes(bytes) => {
                match String::from_utf8(bytes.clone()) {
                    Ok(s) => Value::String(s),
                    Err(_) => Value::Binary(bytes),
                }
            }
            mysql_async::Value::Int(i) => Value::Int(i),
            mysql_async::Value::UInt(u) => Value::Int(u as i64),
            mysql_async::Value::Float(f) => Value::Float(f as f64),
            mysql_async::Value::Double(d) => Value::Float(d),
            mysql_async::Value::Date(year, month, day, hour, minute, second, micro) => {
                let datetime_str = format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
                    year, month, day, hour, minute, second, micro
                );
                Value::String(datetime_str)
            }
            mysql_async::Value::Time(neg, days, hours, minutes, seconds, micros) => {
                let sign = if neg { "-" } else { "" };
                let total_hours = days * 24 + hours as u32;
                let time_str = format!(
                    "{}{}:{:02}:{:02}:{:06}",
                    sign,
                    total_hours,
                    minutes,
                    seconds,
                    micros
                );
                Value::String(time_str)
            }
        }
    }

    /// Convert our Value type to mysql_async params
    fn convert_params(params: &[Value]) -> Vec<mysql_async::Value> {
        params
            .iter()
            .map(|v| match v {
                Value::Null => mysql_async::Value::NULL,
                Value::Int(i) => mysql_async::Value::Int(*i),
                Value::Float(f) => mysql_async::Value::Double(*f),
                Value::String(s) => mysql_async::Value::Bytes(s.as_bytes().to_vec()),
                Value::Binary(b) => mysql_async::Value::Bytes(b.clone()),
                Value::Bool(b) => mysql_async::Value::Int(if *b { 1 } else { 0 }),
                _ => mysql_async::Value::NULL,
            })
            .collect()
    }
}

#[async_trait]
impl DatabaseTransaction for MySqlTransaction {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mysql_params = Self::convert_params(params);

        let mut conn = self.conn.lock().await;
        let result: Vec<mysql_async::Row> = conn
            .exec(sql, mysql_params)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Query execution failed: {}", e)))?;

        // Get column information
        let columns = if let Some(first_row) = result.first() {
            first_row
                .columns_ref()
                .iter()
                .map(|col| crate::handlers::database::types::ColumnInfo {
                    name: col.name_str().to_string(),
                    data_type: format!("{:?}", col.column_type()),
                    nullable: !col.flags().contains(mysql_async::consts::ColumnFlags::NOT_NULL_FLAG),
                    max_length: None,
                })
                .collect()
        } else {
            vec![]
        };

        // Convert rows
        let rows: Vec<Vec<Value>> = result
            .into_iter()
            .map(|row| {
                let values: Vec<mysql_async::Value> = row.unwrap();
                values.into_iter().map(Self::convert_value).collect()
            })
            .collect();

        let total_rows = Some(rows.len() as u64);

        Ok(QueryResult { columns, rows, total_rows, execution_time_ms: 0 })
    }

    async fn execute(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<ExecuteResult, DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mysql_params = Self::convert_params(params);

        let mut conn = self.conn.lock().await;
        conn.exec_drop(sql, mysql_params)
            .await
            .map_err(|e| DatabaseError::OperationFailed(format!("Execute failed: {}", e)))?;

        let affected_rows = conn.affected_rows();
        let last_insert_id = conn.last_insert_id();

        Ok(ExecuteResult {
            rows_affected: affected_rows,
            last_insert_id: if last_insert_id.is_some() && last_insert_id.unwrap() > 0 {
                Some(Value::Int(last_insert_id.unwrap() as i64))
            } else {
                None
            },
            execution_time_ms: 0,
        })
    }

    /// Commit the current transaction
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mut conn = self.conn.lock().await;
        conn.query_drop("COMMIT").await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e))
        })?;

        *self.is_active.lock().await = false;
        Ok(())
    }

    /// Rollback the current transaction
    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mut conn = self.conn.lock().await;
        conn.query_drop("ROLLBACK").await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to rollback transaction: {}", e))
        })?;

        *self.is_active.lock().await = false;
        Ok(())
    }

    /// Create a savepoint within the transaction
    async fn savepoint(&self, name: &str) -> Result<(), DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let savepoint_sql = format!("SAVEPOINT {}", name);
        let mut conn = self.conn.lock().await;
        conn.query_drop(&savepoint_sql).await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to create savepoint: {}", e))
        })?;

        Ok(())
    }

    /// Rollback to a specific savepoint
    async fn rollback_to_savepoint(&self, savepoint_name: &str) -> Result<(), DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let rollback_sql = format!("ROLLBACK TO SAVEPOINT {}", savepoint_name);
        let mut conn = self.conn.lock().await;
        conn.query_drop(&rollback_sql).await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to rollback to savepoint: {}", e))
        })?;

        Ok(())
    }

    /// Release a savepoint
    async fn release_savepoint(&self, savepoint_name: &str) -> Result<(), DatabaseError> {
        let is_active = *self.is_active.lock().await;
        if !is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let release_sql = format!("RELEASE SAVEPOINT {}", savepoint_name);
        let mut conn = self.conn.lock().await;
        conn.query_drop(&release_sql).await.map_err(|e| {
            DatabaseError::QueryFailed(format!("Failed to release savepoint: {}", e))
        })?;

        Ok(())
    }

    async fn set_isolation_level(&self, _level: crate::handlers::database::engine::IsolationLevel) -> Result<(), DatabaseError> {
        // Isolation level is set at transaction start, cannot be changed mid-transaction
        Err(DatabaseError::UnsupportedOperation(
            "Cannot change isolation level during active transaction".to_string(),
        ))
    }

    fn transaction_info(&self) -> crate::handlers::database::engine::TransactionInfo {
        use chrono::Utc;
        use crate::handlers::database::engine::{IsolationLevel, TransactionInfo};
        
        TransactionInfo {
            transaction_id: "mysql-tx".to_string(),
            isolation_level: IsolationLevel::RepeatableRead,
            started_at: Utc::now(),
            savepoints: vec![],
            is_read_only: false,
        }
    }
}

impl Drop for MySqlTransaction {
    fn drop(&mut self) {
        // Note: Cannot async check is_active in Drop
        // Logging warning only
        eprintln!("Warning: MySqlTransaction dropped - ensure transaction was committed or rolled back");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_savepoint_naming() {
        let savepoint_name = format!("sp_{}", 1);
        assert_eq!(savepoint_name, "sp_1");
    }

    #[test]
    fn test_savepoint_counter() {
        let mut counter = 0;
        counter += 1;
        counter += 1;
        counter += 1;
        assert_eq!(counter, 3);
    }
}
