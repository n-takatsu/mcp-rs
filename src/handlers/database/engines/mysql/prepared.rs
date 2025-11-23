//! MySQL Prepared Statement Implementation
//!
//! Provides secure prepared statement execution for MySQL queries
//! with full parameterized query support and SQL injection prevention

use super::param_converter::MySqlParamConverter;
use crate::handlers::database::{
    engine::PreparedStatement,
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use mysql_async::{Pool, Statement};
use std::sync::Arc;

/// MySQL Prepared Statement Implementation
///
/// Wraps mysql_async::Statement with our standard PreparedStatement interface
/// Ensures secure parameter binding and type conversion
pub struct MySqlPreparedStatement {
    statement: Arc<Statement>,
    pool: Arc<Pool>,
    sql: String,
    param_count: usize,
}

impl MySqlPreparedStatement {
    /// Create a new prepared statement from SQL
    ///
    /// # Arguments
    /// * `pool` - MySQL connection pool
    /// * `sql` - SQL query with ? placeholders
    ///
    /// # Returns
    /// A new MySqlPreparedStatement or error if preparation fails
    pub async fn new(pool: Arc<Pool>, sql: String) -> Result<Self, DatabaseError> {
        // Validate placeholders
        let param_count = sql.matches('?').count();

        // Attempt to prepare the statement by getting a connection and preparing
        let mut conn = pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let statement = conn
            .prep(&sql)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e))
            })?;

        Ok(Self {
            statement: Arc::new(statement),
            pool,
            sql,
            param_count,
        })
    }

    /// Convert mysql_async rows to our QueryResult format
    async fn convert_rows_to_query_result(
        rows: Vec<mysql_async::Row>,
    ) -> Result<QueryResult, DatabaseError> {
        let mut columns = Vec::new();
        let mut result_rows = Vec::new();

        // Extract column information from first row if available
        if let Some(first_row) = rows.first() {
            for (idx, _) in first_row.iter().enumerate() {
                columns.push(format!("column_{}", idx));
            }
        }

        // Convert rows to our format
        for row in rows {
            let mut values = Vec::new();
            for value in row.iter() {
                values.push(MySqlParamConverter::convert_from_mysql_value(value.clone())?);
            }
            result_rows.push(values);
        }

        Ok(QueryResult {
            columns,
            rows: result_rows,
            total_rows: Some(result_rows.len()),
            execution_time_ms: 0, // TODO: track execution time
        })
    }
}

#[async_trait]
impl PreparedStatement for MySqlPreparedStatement {
    /// Execute query (SELECT) with parameters
    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // Validate parameter count
        if params.len() != self.param_count {
            return Err(DatabaseError::ValidationError(format!(
                "Parameter count mismatch: expected {}, got {}",
                self.param_count,
                params.len()
            )));
        }

        // Get connection from pool
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        // Convert parameters
        let mysql_params = MySqlParamConverter::convert_params(params)?;

        // Execute query
        let start_time = std::time::Instant::now();
        let rows: Vec<mysql_async::Row> = conn
            .exec(self.statement.as_ref(), mysql_params)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to execute query: {}", e)))?;

        let execution_time = start_time.elapsed().as_millis() as u64;

        // Convert results
        let mut result = Self::convert_rows_to_query_result(rows).await?;
        result.execution_time_ms = execution_time;

        Ok(result)
    }

    /// Execute command (INSERT/UPDATE/DELETE) with parameters
    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        // Validate parameter count
        if params.len() != self.param_count {
            return Err(DatabaseError::ValidationError(format!(
                "Parameter count mismatch: expected {}, got {}",
                self.param_count,
                params.len()
            )));
        }

        // Get connection from pool
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        // Convert parameters
        let mysql_params = MySqlParamConverter::convert_params(params)?;

        // Execute command
        let start_time = std::time::Instant::now();
        conn.exec_drop(self.statement.as_ref(), mysql_params)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to execute command: {}", e))
            })?;

        let execution_time = start_time.elapsed().as_millis() as u64;
        let rows_affected = conn.affected_rows();
        let last_insert_id = conn.last_insert_id();

        Ok(ExecuteResult {
            rows_affected,
            last_insert_id: last_insert_id.map(Value::Int),
            execution_time_ms: execution_time,
        })
    }

    /// Get parameter count for this prepared statement
    fn parameter_count(&self) -> usize {
        self.param_count
    }

    /// Get SQL string for this prepared statement
    fn get_sql(&self) -> &str {
        &self.sql
    }

    /// Close the prepared statement
    async fn close(&self) -> Result<(), DatabaseError> {
        // mysql_async handles cleanup automatically via Arc
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_count_parsing() {
        // Test that parameter count is correctly parsed from SQL
        let sql = "SELECT * FROM users WHERE id = ? AND name = ?".to_string();
        let param_count = sql.matches('?').count();
        assert_eq!(param_count, 2);

        let sql_no_params = "SELECT * FROM users".to_string();
        let param_count_no_params = sql_no_params.matches('?').count();
        assert_eq!(param_count_no_params, 0);
    }
}
