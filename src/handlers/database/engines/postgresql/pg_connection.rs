//! PostgreSQL Database Connection Implementation
//!
//! Provides real PostgreSQL connectivity using sqlx

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::*,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::{
    postgres::{PgPool, PgPoolOptions, PgRow},
    Column, Row,
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// PostgreSQL Database Connection
pub struct PostgreSqlConnection {
    pool: PgPool,
    database_name: String,
}

impl PostgreSqlConnection {
    /// Create a new PostgreSQL connection from connection string
    pub async fn new(connection_string: &str) -> Result<Self, DatabaseError> {
        // Extract database name from connection string
        let database_name = connection_string
            .split('/')
            .next_back()
            .and_then(|s| s.split('?').next())
            .unwrap_or("unknown")
            .to_string();

        let pool = PgPool::connect(connection_string)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            pool,
            database_name,
        })
    }

    /// Create from database config
    pub async fn from_config(config: &ConnectionConfig) -> Result<Self, DatabaseError> {
        let connection_string = format!(
            "postgresql://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        );

        Self::new(&connection_string).await
    }

    /// Convert sqlx Row to our Value vec
    fn row_to_values(row: &PgRow) -> Vec<Value> {
        let mut values = Vec::new();
        for i in 0..row.len() {
            let value = Self::extract_value(row, i);
            values.push(value);
        }
        values
    }

    /// Extract value from PostgreSQL row at specific index
    fn extract_value(row: &PgRow, index: usize) -> Value {
        // Try different types in order

        // Try boolean
        if let Ok(val) = row.try_get::<bool, _>(index) {
            return Value::Bool(val);
        }

        // Try integers
        if let Ok(val) = row.try_get::<i32, _>(index) {
            return Value::Int(val as i64);
        }
        if let Ok(val) = row.try_get::<i64, _>(index) {
            return Value::Int(val);
        }

        // Try float
        if let Ok(val) = row.try_get::<f64, _>(index) {
            return Value::Float(val);
        }

        // Try string
        if let Ok(val) = row.try_get::<String, _>(index) {
            return Value::String(val);
        }

        // Try bytes
        if let Ok(val) = row.try_get::<Vec<u8>, _>(index) {
            return Value::Binary(val);
        }

        // Try timestamp
        if let Ok(val) = row.try_get::<chrono::NaiveDateTime, _>(index) {
            let dt = chrono::DateTime::<Utc>::from_naive_utc_and_offset(val, Utc);
            return Value::DateTime(dt);
        }

        // Default to Null
        Value::Null
    }

    /// Convert our Value to sqlx argument
    fn value_to_argument(value: &Value) -> String {
        match value {
            Value::Null => "NULL".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::String(s) => format!("'{}'", s.replace('\'', "''")),
            Value::Binary(_) => "''".to_string(), // Binary handling simplified
            Value::Json(j) => format!("'{}'", j.to_string().replace('\'', "''")),
            Value::DateTime(dt) => format!("'{}'", dt.format("%Y-%m-%d %H:%M:%S")),
        }
    }
}

#[async_trait]
impl DatabaseConnection for PostgreSqlConnection {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // For simplicity, we'll use string interpolation for parameters
        // In production, use proper parameterized queries with sqlx::query!
        let mut query_str = sql.to_string();
        for (i, param) in params.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            let value_str = Self::value_to_argument(param);
            query_str = query_str.replace(&placeholder, &value_str);
        }

        let rows = sqlx::query(&query_str)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut result_rows = Vec::new();
        let mut columns = Vec::new();

        if let Some(first_row) = rows.first() {
            // Extract column information from first row
            for i in 0..first_row.len() {
                columns.push(ColumnInfo {
                    name: first_row.column(i).name().to_string(),
                    data_type: "TEXT".to_string(), // Simplified
                    nullable: true,
                    max_length: None,
                });
            }
        }

        for row in &rows {
            result_rows.push(Self::row_to_values(row));
        }

        Ok(QueryResult {
            columns,
            rows: result_rows,
            total_rows: Some(rows.len() as u64),
            execution_time_ms: 0,
        })
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        // Similar parameter handling as query
        let mut query_str = sql.to_string();
        for (i, param) in params.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            let value_str = Self::value_to_argument(param);
            query_str = query_str.replace(&placeholder, &value_str);
        }

        let result = sqlx::query(&query_str)
            .execute(&self.pool)
            .await
            .map_err(|e| DatabaseError::OperationFailed(e.to_string()))?;

        let rows_affected = result.rows_affected();

        // PostgreSQL doesn't have last_insert_id like MySQL
        // Would need RETURNING clause for that
        Ok(ExecuteResult {
            rows_affected,
            last_insert_id: None,
            execution_time_ms: 0,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DatabaseError::TransactionFailed(e.to_string()))?;

        Ok(Box::new(super::transaction::PostgreSqlTransaction::new(tx)))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        // Get all tables in the current database
        let rows = sqlx::query(
            "SELECT table_name FROM information_schema.tables 
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut tables = Vec::new();
        for row in rows {
            let table_name: String = row.get(0);
            if let Ok(table_info) = self.get_table_schema(&table_name).await {
                tables.push(table_info);
            }
        }

        Ok(DatabaseSchema {
            database_name: self.database_name.clone(),
            tables,
            views: Vec::new(),
            procedures: Vec::new(),
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        // Get column information
        let rows = sqlx::query(
            "SELECT column_name, data_type, is_nullable, character_maximum_length
             FROM information_schema.columns
             WHERE table_name = $1 AND table_schema = 'public'
             ORDER BY ordinal_position",
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut columns = Vec::new();
        for row in rows {
            let name: String = row.get(0);
            let data_type: String = row.get(1);
            let is_nullable: String = row.get(2);
            let max_length: Option<i32> = row.try_get(3).ok();

            columns.push(ColumnInfo {
                name,
                data_type,
                nullable: is_nullable == "YES",
                max_length,
            });
        }

        // Get primary keys
        let pk_rows = sqlx::query(
            "SELECT a.attname
             FROM pg_index i
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
             WHERE i.indrelid = $1::regclass AND i.indisprimary",
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let primary_keys: Vec<String> = pk_rows.iter().map(|row| row.get::<String, _>(0)).collect();

        Ok(TableInfo {
            name: table_name.to_string(),
            schema: Some("public".to_string()),
            columns,
            primary_keys,
            foreign_keys: Vec::new(),
            indexes: Vec::new(),
        })
    }

    async fn prepare(&self, _sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Prepared statements not yet implemented for PostgreSQL".to_string(),
        ))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
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
            connection_id: "pg-connection".to_string(),
            database_name: self.database_name.clone(),
            user_name: "postgres".to_string(),
            server_version: "PostgreSQL (sqlx)".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}
