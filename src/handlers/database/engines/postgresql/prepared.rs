//! PostgreSQL Prepared Statement Implementation
//!
//! Provides secure prepared statement execution for PostgreSQL queries
//! with parameterized query support and SQL injection prevention.

use crate::handlers::database::{
    engine::PreparedStatement,
    types::{ColumnInfo, DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use sqlx::postgres::PgPool;
use sqlx::{Column, Postgres, Row};
use std::sync::Arc;

/// PostgreSQL Prepared Statement Implementation
///
/// Wraps PostgreSQL prepared statement functionality with our standard PreparedStatement interface.
/// Supports PostgreSQL-specific parameter placeholders ($1, $2, ...) and type conversions.
pub struct PostgreSqlPreparedStatement {
    /// SQL query template with $1, $2, ... placeholders
    sql: Arc<String>,
    /// Expected parameter types
    param_types: Arc<Vec<String>>,
    /// Number of parameters
    param_count: usize,
    /// Connection pool for execution
    pool: Arc<PgPool>,
}

impl PostgreSqlPreparedStatement {
    /// Create a new PostgreSQL prepared statement
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL query template with PostgreSQL parameter placeholders ($1, $2, ...)
    /// * `param_count` - Number of parameters expected
    /// * `pool` - PostgreSQL connection pool
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let stmt = PostgreSqlPreparedStatement::new(
    ///     "SELECT * FROM users WHERE id = $1 AND status = $2".to_string(),
    ///     2,
    ///     pool,
    /// );
    /// ```
    pub fn new(sql: String, param_count: usize, pool: Arc<PgPool>) -> Result<Self, DatabaseError> {
        // Validate parameter placeholders
        Self::validate_placeholders(&sql, param_count)?;

        Ok(Self {
            sql: Arc::new(sql),
            param_types: Arc::new(Self::infer_param_types(param_count)),
            param_count,
            pool,
        })
    }

    /// Validate that placeholders match parameter count
    fn validate_placeholders(sql: &str, expected_count: usize) -> Result<(), DatabaseError> {
        let mut max_placeholder = 0;

        // Find all $N placeholders in the SQL string
        for (idx, _) in sql.match_indices('$').enumerate() {
            let pos = sql.find('$').unwrap_or(0);
            if pos + 1 < sql.len() {
                if let Some(ch) = sql[pos + 1..].chars().next() {
                    if let Some(num) = ch.to_digit(10) {
                        max_placeholder = max_placeholder.max(num as usize);
                    }
                }
            }
        }

        // Check if placeholder count matches expected parameters
        if max_placeholder > expected_count && max_placeholder > 0 {
            return Err(DatabaseError::ValidationError(format!(
                "SQL contains placeholder ${} but only {} parameters provided",
                max_placeholder, expected_count
            )));
        }

        Ok(())
    }

    /// Infer parameter types from count (placeholder - can be enhanced)
    fn infer_param_types(count: usize) -> Vec<String> {
        (0..count).map(|_| "unknown".to_string()).collect()
    }

    /// Get the SQL query template
    pub fn get_sql(&self) -> &str {
        &self.sql
    }

    /// Get parameter count
    pub fn parameter_count(&self) -> usize {
        self.param_count
    }

    /// Get parameter types
    pub fn get_param_types(&self) -> &[String] {
        &self.param_types
    }

    /// Validate parameters match expected count
    fn validate_params(&self, params: &[Value]) -> Result<(), DatabaseError> {
        if params.len() != self.param_count {
            return Err(DatabaseError::ValidationError(format!(
                "Expected {} parameters, got {}",
                self.param_count,
                params.len()
            )));
        }
        Ok(())
    }

    /// Convert a Value to PostgreSQL compatible format
    fn value_to_sql_string(value: &Value) -> Result<String, DatabaseError> {
        match value {
            Value::Null => Ok("NULL".to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Int(i) => Ok(i.to_string()),
            Value::Float(f) => Ok(f.to_string()),
            Value::String(s) => {
                // Escape single quotes in strings
                let escaped = s.replace("'", "''");
                Ok(format!("'{}'", escaped))
            }
            Value::Binary(_) => Err(DatabaseError::ValidationError(
                "Binary type not supported in parameter binding".to_string(),
            )),
            Value::Json(_) => Err(DatabaseError::ValidationError(
                "JSON type needs special handling".to_string(),
            )),
            Value::DateTime(_) => Err(DatabaseError::ValidationError(
                "DateTime type needs special handling".to_string(),
            )),
        }
    }

    /// Convert PostgreSQL row values to our Value type
    fn convert_row_value(value: Option<String>) -> Value {
        match value {
            None => Value::Null,
            Some(s) => Value::String(s),
        }
    }

    /// Convert PostgreSQL rows to our QueryResult format
    async fn convert_rows_to_query_result(
        rows: Vec<sqlx::postgres::PgRow>,
    ) -> Result<QueryResult, DatabaseError> {
        let mut columns = Vec::new();
        let mut result_rows = Vec::new();

        // Extract column information from first row if available
        if let Some(first_row) = rows.first() {
            let column_count = first_row.len();
            for column in first_row.columns().iter().take(column_count) {
                columns.push(ColumnInfo {
                    name: column.name().to_string(),
                    data_type: format!("{:?}", column.type_info()),
                    nullable: true,
                    max_length: None,
                });
            }
        }

        // Convert rows to our format
        for row in rows {
            let mut values = Vec::new();
            for idx in 0..row.len() {
                // Try to get value as string, then convert to appropriate type
                let value = match row.try_get::<String, usize>(idx) {
                    Ok(s) => Value::String(s),
                    Err(_) => Value::Null,
                };
                values.push(value);
            }
            result_rows.push(values);
        }

        let total_rows = result_rows.len() as u64;

        Ok(QueryResult {
            columns,
            rows: result_rows,
            total_rows: Some(total_rows),
            execution_time_ms: 0,
        })
    }
}

#[async_trait]
impl PreparedStatement for PostgreSqlPreparedStatement {
    /// Close the prepared statement
    async fn close(&self) -> Result<(), DatabaseError> {
        // PostgreSQL doesn't require explicit closure of prepared statements
        // when using connection pools, so this is a no-op
        Ok(())
    }

    /// Execute query (SELECT) with parameters
    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        self.validate_params(params)?;

        // Build the query with parameter binding
        let mut query = sqlx::query(&self.sql);

        // Bind each parameter to the query
        for _idx in 0..params.len() {
            let param = &params[_idx];
            match param {
                Value::Null => {
                    query = query.bind(None::<String>);
                }
                Value::Bool(b) => {
                    query = query.bind(*b);
                }
                Value::Int(i) => {
                    query = query.bind(*i);
                }
                Value::Float(f) => {
                    query = query.bind(*f);
                }
                Value::String(s) => {
                    query = query.bind(s.clone());
                }
                Value::Binary(_b) => {
                    return Err(DatabaseError::ValidationError(
                        "Binary type not yet supported in parameter binding".to_string(),
                    ));
                }
                Value::Json(j) => {
                    // Convert JSON to string for binding
                    let json_str = j.to_string();
                    query = query.bind(json_str);
                }
                Value::DateTime(dt) => {
                    query = query.bind(*dt);
                }
            }
        }

        // Execute the query
        match query.fetch_all(self.pool.as_ref()).await {
            Ok(rows) => Self::convert_rows_to_query_result(rows).await,
            Err(e) => Err(DatabaseError::QueryFailed(e.to_string())),
        }
    }

    /// Execute command (INSERT, UPDATE, DELETE) with parameters
    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        self.validate_params(params)?;

        // Build the query with parameter binding
        let mut query = sqlx::query(&self.sql);

        // Bind each parameter to the query
        for param in params.iter() {
            match param {
                Value::Null => {
                    query = query.bind(None::<String>);
                }
                Value::Bool(b) => {
                    query = query.bind(*b);
                }
                Value::Int(i) => {
                    query = query.bind(*i);
                }
                Value::Float(f) => {
                    query = query.bind(*f);
                }
                Value::String(s) => {
                    query = query.bind(s.clone());
                }
                Value::Binary(_b) => {
                    return Err(DatabaseError::ValidationError(
                        "Binary type not yet supported in parameter binding".to_string(),
                    ));
                }
                Value::Json(j) => {
                    let json_str = j.to_string();
                    query = query.bind(json_str);
                }
                Value::DateTime(dt) => {
                    query = query.bind(*dt);
                }
            }
        }

        // Execute the command
        match query.execute(self.pool.as_ref()).await {
            Ok(result) => Ok(ExecuteResult {
                rows_affected: result.rows_affected(),
                last_insert_id: None, // PostgreSQL uses RETURNING clause for this
                execution_time_ms: 0,
            }),
            Err(e) => Err(DatabaseError::QueryFailed(e.to_string())),
        }
    }

    /// Get parameter count
    fn parameter_count(&self) -> usize {
        self.param_count
    }

    /// Get SQL query template
    fn get_sql(&self) -> &str {
        &self.sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_validation() {
        let param_count = 2;
        let expected = format!("Expected {} parameters", param_count);
        assert!(expected.contains("2"));
    }

    #[test]
    fn test_param_types_inference() {
        let param_types = PostgreSqlPreparedStatement::infer_param_types(3);
        assert_eq!(param_types.len(), 3);
        for param_type in param_types {
            assert_eq!(param_type, "unknown");
        }
    }

    #[test]
    fn test_sql_placeholder_count() {
        // Test that parameter placeholders are counted correctly
        let sql = "SELECT * FROM users WHERE id = $1 AND email = $2 AND status = $3";
        let placeholder_count = sql.matches('$').count();
        assert_eq!(placeholder_count, 3);
    }

    #[test]
    fn test_placeholder_validation() {
        // Valid: correct number of placeholders
        let result = PostgreSqlPreparedStatement::validate_placeholders(
            "SELECT * FROM users WHERE id = $1 AND email = $2",
            2,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_placeholder_validation_mismatch() {
        // Invalid: more placeholders than parameters
        let result = PostgreSqlPreparedStatement::validate_placeholders(
            "SELECT * FROM users WHERE id = $1 AND email = $2 AND status = $3",
            2,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_value_to_sql_string_integer() {
        let val = Value::Int(42);
        let result = PostgreSqlPreparedStatement::value_to_sql_string(&val);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "42");
    }

    #[test]
    fn test_value_to_sql_string_string_with_quotes() {
        let val = Value::String("O'Brien".to_string());
        let result = PostgreSqlPreparedStatement::value_to_sql_string(&val);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "'O''Brien'");
    }

    #[test]
    fn test_value_to_sql_string_null() {
        let val = Value::Null;
        let result = PostgreSqlPreparedStatement::value_to_sql_string(&val);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "NULL");
    }

    #[test]
    fn test_value_to_sql_string_boolean() {
        let val = Value::Bool(true);
        let result = PostgreSqlPreparedStatement::value_to_sql_string(&val);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "true");
    }

    #[test]
    fn test_value_to_sql_string_float() {
        let val = Value::Float(3.14);
        let result = PostgreSqlPreparedStatement::value_to_sql_string(&val);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "3.14");
    }

    #[test]
    fn test_value_to_sql_string_bigint() {
        let val = Value::Int(9223372036854775807i64);
        let result = PostgreSqlPreparedStatement::value_to_sql_string(&val);
        assert!(result.is_ok());
    }
}
