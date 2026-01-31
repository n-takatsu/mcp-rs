//! PostgreSQL Database Connection Implementation
//!
//! Implements DatabaseConnection trait for PostgreSQL

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::{
        ColumnInfo, DatabaseError, DatabaseSchema, ExecuteResult, IndexInfo, QueryResult,
        TableInfo, Value,
    },
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{Column, PgPool, Postgres, Row, TypeInfo};
use std::sync::Arc;

use super::PostgresTransaction;

/// PostgreSQL database connection
#[derive(Clone)]
pub struct PostgresConnection {
    /// Connection pool
    pool: PgPool,
    /// Connection ID
    connection_id: String,
}

impl PostgresConnection {
    /// Create a new PostgreSQL connection
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            connection_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    /// Get the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl DatabaseConnection for PostgresConnection {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let start = std::time::Instant::now();

        let mut query = sqlx::query(sql);

        // Bind parameters
        for param in params {
            query = bind_value(query, param)?;
        }

        // Execute query
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Query execution failed: {}", e)))?;

        // Convert to QueryResult
        let columns: Vec<ColumnInfo> = if let Some(first_row) = rows.first() {
            first_row
                .columns()
                .iter()
                .map(|c| ColumnInfo {
                    name: Column::name(c).to_string(),
                    data_type: Column::type_info(c).name().to_string(),
                    nullable: true,
                    max_length: None,
                })
                .collect()
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
                        } else if let Ok(val) = row.try_get::<serde_json::Value, _>(i) {
                            Value::Json(val)
                        } else {
                            Value::Null
                        }
                    })
                    .collect()
            })
            .collect();

        let execution_time = start.elapsed().as_millis() as u64;

        Ok(QueryResult {
            columns,
            rows: data,
            total_rows: Some(rows.len() as u64),
            execution_time_ms: execution_time,
        })
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let start = std::time::Instant::now();

        let mut query = sqlx::query(sql);

        // Bind parameters
        for param in params {
            query = bind_value(query, param)?;
        }

        // Execute command
        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Execute failed: {}", e)))?;

        let execution_time = start.elapsed().as_millis() as u64;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None, // PostgreSQL uses RETURNING clause instead
            execution_time_ms: execution_time,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let tx = PostgresTransaction::begin(&self.pool).await?;
        Ok(Box::new(tx))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        // Query PostgreSQL information_schema
        let tables_query = r#"
            SELECT 
                table_name,
                table_type
            FROM information_schema.tables
            WHERE table_schema = 'public'
            ORDER BY table_name
        "#;

        let rows = sqlx::query(tables_query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get schema: {}", e)))?;

        let mut tables = Vec::new();
        for row in rows {
            let table_name: String = row
                .try_get("table_name")
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

            let table_info = self.get_table_schema(&table_name).await?;
            tables.push(table_info);
        }

        Ok(DatabaseSchema {
            database_name: "public".to_string(), // Default schema
            tables,
            views: Vec::new(),
            procedures: Vec::new(),
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        // Query columns for the table
        let columns_query = r#"
            SELECT 
                column_name,
                data_type,
                is_nullable,
                character_maximum_length,
                column_default
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = $1
            ORDER BY ordinal_position
        "#;

        let rows = sqlx::query(columns_query)
            .bind(table_name)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to get table schema: {}", e))
            })?;

        let columns: Vec<ColumnInfo> = rows
            .iter()
            .map(|row| {
                let name: String = row.try_get("column_name").unwrap_or_default();
                let data_type: String = row.try_get("data_type").unwrap_or_default();
                let is_nullable: String = row.try_get("is_nullable").unwrap_or_default();
                let max_length: Option<i32> = row.try_get("character_maximum_length").ok();

                ColumnInfo {
                    name,
                    data_type,
                    nullable: is_nullable == "YES",
                    max_length,
                }
            })
            .collect();

        // Query primary key
        let pk_query = r#"
            SELECT a.attname
            FROM pg_index i
            JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
            WHERE i.indrelid = $1::regclass AND i.indisprimary
        "#;

        let pk_rows = sqlx::query(pk_query)
            .bind(table_name)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let primary_key: Vec<String> = pk_rows
            .iter()
            .filter_map(|row| row.try_get::<String, _>("attname").ok())
            .collect();

        // Query indexes
        let index_query = r#"
            SELECT 
                i.indexname,
                ix.indisunique::bool,
                ix.indisprimary::bool
            FROM pg_indexes i
            JOIN pg_class c ON c.relname = i.indexname
            JOIN pg_index ix ON ix.indexrelid = c.oid
            WHERE i.schemaname = 'public' AND i.tablename = $1
        "#;

        let index_rows = sqlx::query(index_query)
            .bind(table_name)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();

        let indexes: Vec<IndexInfo> = index_rows
            .iter()
            .filter_map(|row| {
                let name = row.try_get::<String, _>("indexname").ok()?;
                let is_unique = row.try_get::<bool, _>("indisunique").unwrap_or(false);
                let is_primary = row.try_get::<bool, _>("indisprimary").unwrap_or(false);
                Some(IndexInfo {
                    name,
                    columns: Vec::new(), // Column info requires additional query
                    is_unique,
                    is_primary,
                })
            })
            .collect();

        Ok(TableInfo {
            name: table_name.to_string(),
            schema: Some("public".to_string()),
            columns,
            primary_keys: primary_key,
            foreign_keys: Vec::new(), // TODO: Implement foreign key detection
            indexes,
        })
    }

    async fn prepare(&self, sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        let stmt = PostgresPreparedStatement::new(self.pool.clone(), sql.to_string()).await?;
        Ok(Box::new(stmt))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Ping failed: {}", e)))?;
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        // Connection pool manages connections automatically
        // Individual connections can't be closed directly
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: self.connection_id.clone(),
            database_name: String::new(),
            user_name: String::new(),
            server_version: String::new(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

/// PostgreSQL Prepared Statement
pub struct PostgresPreparedStatement {
    pool: PgPool,
    sql: String,
    parameter_count: usize,
}

impl PostgresPreparedStatement {
    /// Create a new prepared statement
    pub async fn new(pool: PgPool, sql: String) -> Result<Self, DatabaseError> {
        // Count parameter placeholders ($1, $2, etc.)
        let parameter_count = sql.matches('$').count();

        Ok(Self {
            pool,
            sql,
            parameter_count,
        })
    }
}

#[async_trait]
impl PreparedStatement for PostgresPreparedStatement {
    async fn query(&self, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let start = std::time::Instant::now();

        if params.len() != self.parameter_count {
            return Err(DatabaseError::InvalidQuery(format!(
                "Expected {} parameters, got {}",
                self.parameter_count,
                params.len()
            )));
        }

        let mut query = sqlx::query(&self.sql);

        // Bind parameters
        for param in params {
            query = bind_value(query, param)?;
        }

        // Execute query
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Prepared query failed: {}", e)))?;

        // Convert to QueryResult
        let columns: Vec<ColumnInfo> = if let Some(first_row) = rows.first() {
            first_row
                .columns()
                .iter()
                .map(|c| ColumnInfo {
                    name: Column::name(c).to_string(),
                    data_type: Column::type_info(c).name().to_string(),
                    nullable: true,
                    max_length: None,
                })
                .collect()
        } else {
            Vec::new()
        };

        let data: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| {
                (0..columns.len())
                    .map(|i| {
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

        let execution_time = start.elapsed().as_millis() as u64;

        Ok(QueryResult {
            columns,
            rows: data,
            total_rows: Some(rows.len() as u64),
            execution_time_ms: execution_time,
        })
    }

    async fn execute(&self, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let start = std::time::Instant::now();

        if params.len() != self.parameter_count {
            return Err(DatabaseError::InvalidQuery(format!(
                "Expected {} parameters, got {}",
                self.parameter_count,
                params.len()
            )));
        }

        let mut query = sqlx::query(&self.sql);

        // Bind parameters
        for param in params {
            query = bind_value(query, param)?;
        }

        // Execute command
        let result = query
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Prepared execute failed: {}", e)))?;

        let execution_time = start.elapsed().as_millis() as u64;

        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: execution_time,
        })
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        // sqlx automatically manages prepared statement lifecycle
        Ok(())
    }

    fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    fn get_sql(&self) -> &str {
        &self.sql
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
    fn test_parameter_count() {
        let sql1 = "SELECT * FROM users WHERE id = $1";
        let count1 = sql1.matches('$').count();
        assert_eq!(count1, 1);

        let sql2 = "SELECT * FROM users WHERE id = $1 AND name = $2";
        let count2 = sql2.matches('$').count();
        assert_eq!(count2, 2);
    }
}
