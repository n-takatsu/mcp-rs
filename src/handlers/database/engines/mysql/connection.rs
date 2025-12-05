//! MySQL Connection Implementation
//!
//! Provides actual MySQL database connection functionality using mysql_async

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::{
        ColumnInfo, DatabaseError, DatabaseSchema, ExecuteResult, QueryResult, TableInfo, Value,
    },
};
use async_trait::async_trait;
use mysql_async::{prelude::*, Conn, Opts, OptsBuilder, Pool};
use std::sync::Arc;
use tokio::sync::Mutex;

/// MySQL Database Connection
pub struct MySqlConnection {
    pool: Pool,
    conn: Arc<Mutex<Option<Conn>>>,
    database_name: String,
}

impl MySqlConnection {
    /// Create a new MySQL connection from connection string
    pub async fn new(connection_string: &str) -> Result<Self, DatabaseError> {
        let opts = Opts::from_url(connection_string).map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Invalid connection string: {}", e))
        })?;

        // Extract database name from connection string
        let database_name = connection_string
            .split('/')
            .next_back()
            .and_then(|s| s.split('?').next())
            .unwrap_or("unknown")
            .to_string();

        let pool = Pool::new(opts);

        // Test connection
        let conn = pool.get_conn().await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to connect to MySQL: {}", e))
        })?;

        Ok(Self {
            pool,
            conn: Arc::new(Mutex::new(Some(conn))),
            database_name,
        })
    }

    /// Create connection from OptsBuilder
    pub async fn from_opts(
        opts: OptsBuilder,
        database_name: String,
    ) -> Result<Self, DatabaseError> {
        let pool = Pool::new(opts);

        let conn = pool.get_conn().await.map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to connect to MySQL: {}", e))
        })?;

        Ok(Self {
            pool,
            conn: Arc::new(Mutex::new(Some(conn))),
            database_name,
        })
    }

    /// Get a connection from the pool
    async fn get_conn(&self) -> Result<Conn, DatabaseError> {
        let mut conn_guard = self.conn.lock().await;

        if let Some(conn) = conn_guard.take() {
            Ok(conn)
        } else {
            // Get new connection from pool
            self.pool.get_conn().await.map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Failed to get connection: {}", e))
            })
        }
    }

    /// Return connection to internal storage
    async fn return_conn(&self, conn: Conn) {
        let mut conn_guard = self.conn.lock().await;
        *conn_guard = Some(conn);
    }

    /// Convert mysql_async Value to our Value type
    fn convert_value(mysql_value: mysql_async::Value) -> Value {
        match mysql_value {
            mysql_async::Value::NULL => Value::Null,
            mysql_async::Value::Bytes(bytes) => {
                // Try to convert to UTF-8 string, fall back to raw bytes
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
                    sign, total_hours, minutes, seconds, micros
                );
                Value::String(time_str)
            }
        }
    }

    /// Convert our Value type to mysql_async params::Params
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
impl DatabaseConnection for MySqlConnection {
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let mut conn = self.get_conn().await?;

        let mysql_params = Self::convert_params(params);

        let result: Vec<mysql_async::Row> = conn
            .exec(sql, mysql_params)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Query execution failed: {}", e)))?;

        // Get column information from the first row if available
        let columns = if let Some(first_row) = result.first() {
            first_row
                .columns_ref()
                .iter()
                .map(|col| ColumnInfo {
                    name: col.name_str().to_string(),
                    data_type: format!("{:?}", col.column_type()),
                    nullable: !col
                        .flags()
                        .contains(mysql_async::consts::ColumnFlags::NOT_NULL_FLAG),
                    max_length: None,
                })
                .collect()
        } else {
            vec![]
        };

        // Convert rows to our format
        let rows: Vec<Vec<Value>> = result
            .into_iter()
            .map(|row| {
                let values: Vec<mysql_async::Value> = row.unwrap();
                values.into_iter().map(Self::convert_value).collect()
            })
            .collect();

        let total_rows = Some(rows.len() as u64);

        self.return_conn(conn).await;

        Ok(QueryResult {
            columns,
            rows,
            total_rows,
            execution_time_ms: 0,
        })
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let mut conn = self.get_conn().await?;

        let mysql_params = Self::convert_params(params);

        conn.exec_drop(sql, mysql_params)
            .await
            .map_err(|e| DatabaseError::OperationFailed(format!("Execute failed: {}", e)))?;

        let affected_rows = conn.affected_rows();
        let last_insert_id = conn.last_insert_id();

        self.return_conn(conn).await;

        Ok(ExecuteResult {
            rows_affected: affected_rows,
            last_insert_id: if let Some(id) = last_insert_id {
                if id > 0 {
                    Some(Value::Int(id as i64))
                } else {
                    None
                }
            } else {
                None
            },
            execution_time_ms: 0,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        use crate::handlers::database::engines::mysql::MySqlTransaction;

        let mut conn = self.get_conn().await?;

        conn.query_drop("START TRANSACTION").await.map_err(|e| {
            DatabaseError::TransactionFailed(format!("Failed to start transaction: {}", e))
        })?;

        Ok(Box::new(MySqlTransaction::new(conn)))
    }

    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let mut conn = self.get_conn().await?;

        // Get all tables in current database
        let tables: Vec<String> = conn
            .query("SHOW TABLES")
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to get tables: {}", e)))?;

        let mut table_info_list = Vec::new();

        for table_name in tables {
            let columns: Vec<mysql_async::Row> = conn
                .exec("SHOW COLUMNS FROM ??", (table_name.clone(),))
                .await
                .map_err(|e| {
                    DatabaseError::QueryFailed(format!(
                        "Failed to get columns for {}: {}",
                        table_name, e
                    ))
                })?;

            let column_info: Vec<ColumnInfo> = columns
                .into_iter()
                .map(|row| {
                    let field: String = row.get("Field").unwrap_or_default();
                    let type_str: String = row.get("Type").unwrap_or_default();
                    let null: String = row.get("Null").unwrap_or_default();

                    ColumnInfo {
                        name: field,
                        data_type: type_str,
                        nullable: null == "YES",
                        max_length: None,
                    }
                })
                .collect();

            table_info_list.push(TableInfo {
                name: table_name,
                schema: None,
                columns: column_info,
                primary_keys: vec![],
                foreign_keys: vec![],
                indexes: vec![],
            });
        }

        self.return_conn(conn).await;

        Ok(DatabaseSchema {
            database_name: self.database_name.clone(),
            tables: table_info_list,
            views: vec![],
            procedures: vec![],
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        let mut conn = self.get_conn().await?;

        let columns: Vec<mysql_async::Row> = conn
            .exec("SHOW COLUMNS FROM ??", (table_name,))
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!(
                    "Failed to get columns for {}: {}",
                    table_name, e
                ))
            })?;

        let column_info: Vec<ColumnInfo> = columns
            .into_iter()
            .map(|row| {
                let field: String = row.get("Field").unwrap_or_default();
                let type_str: String = row.get("Type").unwrap_or_default();
                let null: String = row.get("Null").unwrap_or_default();

                ColumnInfo {
                    name: field,
                    data_type: type_str,
                    nullable: null == "YES",
                    max_length: None,
                }
            })
            .collect();

        self.return_conn(conn).await;

        Ok(TableInfo {
            name: table_name.to_string(),
            schema: None,
            columns: column_info,
            primary_keys: vec![],
            foreign_keys: vec![],
            indexes: vec![],
        })
    }

    async fn prepare(&self, _sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        // MySQL prepared statements implementation
        Err(DatabaseError::UnsupportedOperation(
            "Prepared statements not yet implemented for MySQL".to_string(),
        ))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        let mut conn = self.get_conn().await?;

        conn.ping()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Ping failed: {}", e)))?;

        self.return_conn(conn).await;

        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        let mut conn_guard = self.conn.lock().await;
        if let Some(conn) = conn_guard.take() {
            conn.disconnect().await.map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Failed to close connection: {}", e))
            })?;
        }
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        use chrono::Utc;
        ConnectionInfo {
            connection_id: "mysql-conn".to_string(),
            database_name: self.database_name.clone(),
            user_name: "user".to_string(),
            server_version: "8.0".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

impl Drop for MySqlConnection {
    fn drop(&mut self) {
        // Connection will be returned to pool automatically
    }
}
