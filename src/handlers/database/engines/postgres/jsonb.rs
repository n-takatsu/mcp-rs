//! PostgreSQL JSONB Operations
//!
//! Provides optimized JSONB manipulation and query capabilities

use crate::handlers::database::types::{DatabaseError, ExecuteResult, QueryResult, Value};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};
use std::sync::Arc;

/// JSONB query builder for PostgreSQL
///
/// Provides fluent API for constructing JSONB queries with operators
#[derive(Clone)]
pub struct JsonbQueryBuilder {
    table: String,
    column: String,
    conditions: Vec<String>,
    params: Vec<JsonValue>,
}

impl JsonbQueryBuilder {
    /// Create a new JSONB query builder
    pub fn new(table: impl Into<String>, column: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            column: column.into(),
            conditions: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Add a path extraction condition: column->'key'
    pub fn extract_path(mut self, path: &str) -> Self {
        self.conditions
            .push(format!("{}->>'{}'", self.column, path));
        self
    }

    /// Add a containment check: column @> value
    pub fn contains(mut self, value: JsonValue) -> Self {
        let idx = self.params.len() + 1;
        self.conditions.push(format!("{} @> ${}", self.column, idx));
        self.params.push(value);
        self
    }

    /// Add a contained-by check: column <@ value
    pub fn contained_by(mut self, value: JsonValue) -> Self {
        let idx = self.params.len() + 1;
        self.conditions.push(format!("{} <@ ${}", self.column, idx));
        self.params.push(value);
        self
    }

    /// Check if JSONB contains a key: column ? 'key'
    pub fn has_key(mut self, key: &str) -> Self {
        self.conditions.push(format!("{} ? '{}'", self.column, key));
        self
    }

    /// Check if JSONB contains any of the keys: column ?| array
    pub fn has_any_key(mut self, keys: &[&str]) -> Self {
        let keys_str = keys
            .iter()
            .map(|k| format!("'{}'", k))
            .collect::<Vec<_>>()
            .join(",");
        self.conditions
            .push(format!("{} ?| ARRAY[{}]", self.column, keys_str));
        self
    }

    /// Check if JSONB contains all keys: column ?& array
    pub fn has_all_keys(mut self, keys: &[&str]) -> Self {
        let keys_str = keys
            .iter()
            .map(|k| format!("'{}'", k))
            .collect::<Vec<_>>()
            .join(",");
        self.conditions
            .push(format!("{} ?& ARRAY[{}]", self.column, keys_str));
        self
    }

    /// Build the WHERE clause
    pub fn build_where(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.conditions.join(" AND "))
        }
    }

    /// Build the complete SELECT query
    pub fn build_select(&self) -> String {
        let where_clause = self.build_where();
        if where_clause.is_empty() {
            format!("SELECT * FROM {}", self.table)
        } else {
            format!("SELECT * FROM {} {}", self.table, where_clause)
        }
    }

    /// Get the parameters
    pub fn params(&self) -> &[JsonValue] {
        &self.params
    }
}

/// JSONB Handler for PostgreSQL operations
#[derive(Clone)]
pub struct JsonbHandler {
    pool: PgPool,
}

impl JsonbHandler {
    /// Create a new JSONB handler
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Get a reference to the underlying connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Insert JSONB data into a table
    ///
    /// # Example
    /// ```ignore
    /// let data = serde_json::json!({"name": "Alice", "age": 30});
    /// handler.insert_jsonb("users", "data", &data).await?;
    /// ```
    pub async fn insert_jsonb(
        &self,
        table: &str,
        column: &str,
        data: &JsonValue,
    ) -> Result<ExecuteResult, DatabaseError> {
        let sql = format!("INSERT INTO {} ({}) VALUES ($1)", table, column);

        let result = sqlx::query(&sql)
            .bind(data)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("JSONB insert failed: {}", e)))?;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    /// Update JSONB field using jsonb_set
    ///
    /// # Example
    /// ```ignore
    /// // UPDATE users SET data = jsonb_set(data, '{age}', '31')
    /// handler.update_jsonb_field("users", "data", "{age}", json!(31), None).await?;
    /// ```
    pub async fn update_jsonb_field(
        &self,
        table: &str,
        column: &str,
        path: &str,
        value: &JsonValue,
        condition: Option<&str>,
    ) -> Result<ExecuteResult, DatabaseError> {
        let where_clause = condition
            .map(|c| format!(" WHERE {}", c))
            .unwrap_or_default();
        let sql = format!(
            "UPDATE {} SET {} = jsonb_set({}, '{}', $1){}",
            table, column, column, path, where_clause
        );

        let result = sqlx::query(&sql)
            .bind(value)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("JSONB update failed: {}", e)))?;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    /// Delete path from JSONB using jsonb_set with null or #-
    ///
    /// # Example
    /// ```ignore
    /// // UPDATE users SET data = data #- '{address,city}'
    /// handler.delete_jsonb_path("users", "data", "{address,city}", None).await?;
    /// ```
    pub async fn delete_jsonb_path(
        &self,
        table: &str,
        column: &str,
        path: &str,
        condition: Option<&str>,
    ) -> Result<ExecuteResult, DatabaseError> {
        let where_clause = condition
            .map(|c| format!(" WHERE {}", c))
            .unwrap_or_default();
        let sql = format!(
            "UPDATE {} SET {} = {} #- '{}'{}",
            table, column, column, path, where_clause
        );

        let result = sqlx::query(&sql).execute(&self.pool).await.map_err(|e| {
            DatabaseError::QueryFailed(format!("JSONB path deletion failed: {}", e))
        })?;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    /// Query JSONB using path expression
    ///
    /// # Example
    /// ```ignore
    /// // SELECT data->'user'->>'name' FROM events
    /// let results = handler.query_jsonb_path("events", "data", "user.name").await?;
    /// ```
    pub async fn query_jsonb_path(
        &self,
        table: &str,
        column: &str,
        path: &str,
        condition: Option<&str>,
    ) -> Result<Vec<JsonValue>, DatabaseError> {
        let path_expr = path
            .split('.')
            .enumerate()
            .fold(column.to_string(), |acc, (i, p)| {
                if i == path.split('.').count() - 1 {
                    format!("{}->>'{}' ", acc, p)
                } else {
                    format!("{}->'{}' ", acc, p)
                }
            });

        let where_clause = condition
            .map(|c| format!(" WHERE {}", c))
            .unwrap_or_default();
        let sql = format!(
            "SELECT {} as value FROM {}{}",
            path_expr, table, where_clause
        );

        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("JSONB path query failed: {}", e)))?;

        let results: Vec<JsonValue> = rows
            .iter()
            .filter_map(|row| {
                let val: Option<String> = row.try_get("value").ok()?;
                val.and_then(|v| serde_json::from_str(v.as_str()).ok())
            })
            .collect();

        Ok(results)
    }

    /// Aggregate JSONB data using jsonb_agg
    ///
    /// # Example
    /// ```ignore
    /// // SELECT jsonb_agg(data) FROM logs WHERE created_at > NOW() - INTERVAL '1 day'
    /// let agg = handler.aggregate_jsonb("logs", "data", Some("created_at > NOW() - INTERVAL '1 day'")).await?;
    /// ```
    pub async fn aggregate_jsonb(
        &self,
        table: &str,
        column: &str,
        condition: Option<&str>,
    ) -> Result<JsonValue, DatabaseError> {
        let where_clause = condition
            .map(|c| format!(" WHERE {}", c))
            .unwrap_or_default();
        let sql = format!(
            "SELECT jsonb_agg({}) as agg FROM {}{}",
            column, table, where_clause
        );

        let row = sqlx::query(&sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("JSONB aggregation failed: {}", e)))?;

        let result: JsonValue = row.try_get("agg").unwrap_or(JsonValue::Null);

        Ok(result)
    }

    /// Build JSONB object from key-value pairs
    ///
    /// # Example
    /// ```ignore
    /// // SELECT jsonb_build_object('name', name, 'age', age) FROM users
    /// let objects = handler.build_jsonb_object("users", &[("name", "name"), ("age", "age")], None).await?;
    /// ```
    pub async fn build_jsonb_object(
        &self,
        table: &str,
        fields: &[(&str, &str)],
        condition: Option<&str>,
    ) -> Result<Vec<JsonValue>, DatabaseError> {
        let field_pairs: Vec<String> = fields
            .iter()
            .map(|(key, col)| format!("'{}', {}", key, col))
            .collect();

        let where_clause = condition
            .map(|c| format!(" WHERE {}", c))
            .unwrap_or_default();
        let sql = format!(
            "SELECT jsonb_build_object({}) as obj FROM {}{}",
            field_pairs.join(", "),
            table,
            where_clause
        );

        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("JSONB object build failed: {}", e)))?;

        let results: Vec<JsonValue> = rows
            .iter()
            .filter_map(|row| row.try_get::<JsonValue, _>("obj").ok())
            .collect();

        Ok(results)
    }

    /// Create GIN index on JSONB column
    ///
    /// # Example
    /// ```ignore
    /// handler.create_gin_index("users", "data", None).await?;
    /// // Creates: CREATE INDEX idx_users_data_gin ON users USING GIN (data)
    /// ```
    pub async fn create_gin_index(
        &self,
        table: &str,
        column: &str,
        index_name: Option<&str>,
    ) -> Result<ExecuteResult, DatabaseError> {
        let default_name = format!("idx_{}_{}_gin", table, column);
        let idx_name = index_name.unwrap_or(&default_name);
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} USING GIN ({})",
            idx_name, table, column
        );

        let result = sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("GIN index creation failed: {}", e)))?;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    /// Create GIN index on JSONB path
    ///
    /// # Example
    /// ```ignore
    /// handler.create_gin_path_index("users", "data", "user->>'name'", None).await?;
    /// ```
    pub async fn create_gin_path_index(
        &self,
        table: &str,
        column: &str,
        path_expr: &str,
        index_name: Option<&str>,
    ) -> Result<ExecuteResult, DatabaseError> {
        let default_name = format!("idx_{}_path_gin", table);
        let idx_name = index_name.unwrap_or(&default_name);
        let sql = format!(
            "CREATE INDEX IF NOT EXISTS {} ON {} USING GIN (({}->>{}))",
            idx_name, table, column, path_expr
        );

        let result = sqlx::query(&sql).execute(&self.pool).await.map_err(|e| {
            DatabaseError::QueryFailed(format!("GIN path index creation failed: {}", e))
        })?;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    /// Check if JSONB column has GIN index
    pub async fn has_gin_index(&self, table: &str, column: &str) -> Result<bool, DatabaseError> {
        let sql = r#"
            SELECT EXISTS (
                SELECT 1 FROM pg_indexes 
                WHERE tablename = $1 
                AND indexdef LIKE '%USING gin%' 
                AND indexdef LIKE '%' || $2 || '%'
            ) as has_index
        "#;

        let row = sqlx::query(sql)
            .bind(table)
            .bind(column)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Index check failed: {}", e)))?;

        let has_index: bool = row.try_get("has_index").unwrap_or(false);
        Ok(has_index)
    }

    /// Suggest GIN indexes for JSONB columns
    ///
    /// Analyzes query patterns and suggests indexes
    pub async fn suggest_gin_indexes(&self, table: &str) -> Result<Vec<String>, DatabaseError> {
        let sql = r#"
            SELECT column_name 
            FROM information_schema.columns 
            WHERE table_name = $1 
            AND data_type = 'jsonb'
        "#;

        let rows = sqlx::query(sql)
            .bind(table)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Column query failed: {}", e)))?;

        let mut suggestions = Vec::new();
        for row in rows {
            let column: String = row.try_get("column_name").unwrap_or_default();
            let has_index = self.has_gin_index(table, &column).await?;
            if !has_index {
                suggestions.push(format!(
                    "CREATE INDEX idx_{}_{}_gin ON {} USING GIN ({})",
                    table, column, table, column
                ));
            }
        }

        Ok(suggestions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonb_query_builder() {
        let builder = JsonbQueryBuilder::new("users", "data")
            .has_key("email")
            .contains(serde_json::json!({"active": true}));

        let sql = builder.build_select();
        assert!(sql.contains("data ? 'email'"));
        assert!(sql.contains("data @> $1"));
    }

    #[test]
    fn test_jsonb_query_builder_keys() {
        let builder =
            JsonbQueryBuilder::new("events", "metadata").has_any_key(&["user_id", "session_id"]);

        let where_clause = builder.build_where();
        assert!(where_clause.contains("?|"));
        assert!(where_clause.contains("ARRAY"));
    }
}
