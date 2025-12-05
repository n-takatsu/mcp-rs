//! Redis transaction support using MULTI/EXEC
//!
//! Redis transactions are atomic but do not support rollback.
//! Commands are queued during MULTI and executed atomically with EXEC.

use super::types::RedisValue;
use crate::handlers::database::{
    engine::{DatabaseTransaction, IsolationLevel, TransactionInfo},
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use chrono::Utc;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Pipeline};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Redis transaction wrapper using MULTI/EXEC
pub struct RedisTransaction {
    connection: Arc<Mutex<Option<MultiplexedConnection>>>,
    pipeline: Arc<Mutex<Pipeline>>,
    isolation_level: String,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl RedisTransaction {
    /// Create a new Redis transaction
    pub async fn new(connection: MultiplexedConnection) -> Result<Self, DatabaseError> {
        // Initialize pipeline for MULTI/EXEC
        let mut pipeline = redis::pipe();
        pipeline.atomic(); // This enables MULTI/EXEC mode

        Ok(RedisTransaction {
            connection: Arc::new(Mutex::new(Some(connection))),
            pipeline: Arc::new(Mutex::new(pipeline)),
            isolation_level: "ATOMIC".to_string(),
            started_at: Utc::now(),
        })
    }

    /// Execute a raw Redis command and add to pipeline
    async fn execute_redis_command(
        &self,
        command: &str,
        args: &[&str],
    ) -> Result<(), DatabaseError> {
        let mut pipeline = self.pipeline.lock().await;

        let mut redis_cmd = redis::cmd(command);
        for arg in args {
            redis_cmd.arg(*arg);
        }

        pipeline.add_command(redis_cmd);
        Ok(())
    }

    /// Parse simple SQL into Redis commands
    fn parse_sql_to_redis(
        &self,
        sql: &str,
        params: &[Value],
    ) -> Result<(String, Vec<String>), DatabaseError> {
        let sql_lower = sql.trim().to_lowercase();

        if sql_lower.starts_with("select") {
            // SELECT value FROM key -> GET key
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() >= 4 && parts[2].to_lowercase() == "from" {
                let key = parts[3].to_string();
                Ok(("GET".to_string(), vec![key]))
            } else {
                Err(DatabaseError::QueryFailed(
                    "Invalid SELECT syntax".to_string(),
                ))
            }
        } else if sql_lower.starts_with("insert") || sql_lower.starts_with("update") {
            // INSERT INTO key VALUES (value) -> SET key value
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() >= 4 && parts[1].to_lowercase() == "into" {
                let key = parts[2].to_string();
                let value = if !params.is_empty() {
                    match &params[0] {
                        Value::String(s) => s.clone(),
                        Value::Int(i) => i.to_string(),
                        Value::Float(f) => f.to_string(),
                        _ => String::new(),
                    }
                } else {
                    parts
                        .get(4)
                        .map(|s| s.trim_matches(|c| c == '\'' || c == '"').to_string())
                        .unwrap_or_default()
                };
                Ok(("SET".to_string(), vec![key, value]))
            } else {
                Err(DatabaseError::QueryFailed(
                    "Invalid INSERT/UPDATE syntax".to_string(),
                ))
            }
        } else if sql_lower.starts_with("delete") {
            // DELETE FROM key -> DEL key
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() >= 3 && parts[1].to_lowercase() == "from" {
                let key = parts[2].to_string();
                Ok(("DEL".to_string(), vec![key]))
            } else {
                Err(DatabaseError::QueryFailed(
                    "Invalid DELETE syntax".to_string(),
                ))
            }
        } else {
            Err(DatabaseError::UnsupportedOperation(format!(
                "Unsupported SQL command in transaction: {}",
                sql
            )))
        }
    }
}

#[async_trait]
impl DatabaseTransaction for RedisTransaction {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // Parse SQL to Redis command
        let (cmd, args) = self.parse_sql_to_redis(sql, params)?;

        // Add to pipeline
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.execute_redis_command(&cmd, &args_refs).await?;

        // Return placeholder result (actual result comes from EXEC)
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            total_rows: Some(0),
            execution_time_ms: 0,
        })
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        // Parse SQL to Redis command
        let (cmd, args) = self.parse_sql_to_redis(sql, params)?;

        // Add to pipeline
        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.execute_redis_command(&cmd, &args_refs).await?;

        // Return placeholder result (actual result comes from EXEC)
        Ok(ExecuteResult {
            rows_affected: 1,
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        let mut conn_guard = self.connection.lock().await;
        let mut conn = conn_guard.take().ok_or_else(|| {
            DatabaseError::TransactionFailed("Transaction already completed".to_string())
        })?;

        let pipeline = self.pipeline.lock().await;

        // Execute pipeline with MULTI/EXEC
        let _: Vec<redis::Value> = pipeline
            .query_async(&mut conn)
            .await
            .map_err(|e| DatabaseError::TransactionFailed(format!("EXEC failed: {}", e)))?;

        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        // Redis doesn't support rollback in MULTI/EXEC
        // We just discard the pipeline
        let mut conn_guard = self.connection.lock().await;
        let _conn = conn_guard.take().ok_or_else(|| {
            DatabaseError::TransactionFailed("Transaction already completed".to_string())
        })?;

        // Pipeline is automatically discarded when dropped
        Ok(())
    }

    async fn savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Redis does not support savepoints".to_string(),
        ))
    }

    async fn rollback_to_savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Redis does not support savepoints".to_string(),
        ))
    }

    async fn release_savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Redis does not support savepoints".to_string(),
        ))
    }

    async fn set_isolation_level(&self, _level: IsolationLevel) -> Result<(), DatabaseError> {
        // Redis transactions are always atomic, cannot change isolation
        Err(DatabaseError::UnsupportedOperation(
            "Redis transactions always use ATOMIC isolation".to_string(),
        ))
    }

    fn transaction_info(&self) -> TransactionInfo {
        TransactionInfo {
            transaction_id: format!("redis-tx-{}", self.started_at.timestamp()),
            isolation_level: IsolationLevel::Serializable, // Redis MULTI/EXEC is serializable
            started_at: self.started_at,
            savepoints: vec![],
            is_read_only: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sql_to_redis() {
        // This would require actual transaction instance
        // Placeholder for future unit tests
    }
}
