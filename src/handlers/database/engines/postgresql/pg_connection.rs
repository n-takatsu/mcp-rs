//! PostgreSQL Database Connection Implementation
//!
//! Provides real PostgreSQL connectivity using sqlx.
//!
//! #453で、本体（`mcp-rs`）のツールから使えるように直した:
//!
//! - パラメーターは文字列に埋め込まず、`$1`等にバインドする（以前は
//!   文字列の置き換えで、`$1`が`$10`の中にも当たり、SQLインジェクションの
//!   おそれもあった）
//! - `query()`は、毎回、読み取り専用のトランザクション（`SET TRANSACTION
//!   READ ONLY`）の中で実行し、最後にロールバックする。`WITH … DELETE`・
//!   書き込む関数・`set_config`等、SELECTに見えて書き込むSQLも、
//!   データベースが拒否する
//! - 1つのSQLの時間の上限（`statement_timeout`）と、受け取る行数の上限
//! - 値は列の型で読み取る。NUMERICは桁を落とさず文字列にし、読めない型は
//!   NULLにせず、そう分かる文字列にする（以前は黙ってNULLにしていた）

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::*,
};
use async_trait::async_trait;
use chrono::Utc;
use futures::TryStreamExt;
use sqlx::{
    postgres::{PgArguments, PgConnectOptions, PgPool, PgPoolOptions, PgRow, PgSslMode},
    query::Query,
    Column, Executor, Postgres, Row, TypeInfo, ValueRef,
};
use std::str::FromStr;
use std::time::Duration;

/// 1つのSQLの時間の上限の既定（秒）
const DEFAULT_STATEMENT_TIMEOUT_SECS: u64 = 30;

/// PostgreSQL Database Connection
pub struct PostgreSqlConnection {
    pool: PgPool,
    database_name: String,
    user_name: String,
    /// 1つのSQLの時間の上限
    statement_timeout: Duration,
    /// `query()`で受け取る行数の上限（`None`は無制限）。ハンドラーは
    /// `max_rows + 1`を渡し、上限を超えたかを知る
    fetch_limit: Option<usize>,
}

impl PostgreSqlConnection {
    /// Create a new PostgreSQL connection from connection string
    pub async fn new(connection_string: &str) -> Result<Self, DatabaseError> {
        let options = PgConnectOptions::from_str(connection_string)
            .map_err(|e| DatabaseError::ConfigurationError(e.to_string()))?;
        let database_name = options.get_database().unwrap_or("unknown").to_string();
        let user_name = options.get_username().to_string();
        let pool = PgPool::connect_with(options)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
        Ok(Self::with_pool(pool, database_name, user_name))
    }

    /// Create from database config
    pub async fn from_config(config: &ConnectionConfig) -> Result<Self, DatabaseError> {
        let options = connect_options(config)?;
        let pool = PgPool::connect_with(options)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
        Ok(Self::with_pool(
            pool,
            config.database.clone(),
            config.username.clone(),
        ))
    }

    /// 既にあるプールを使う接続（`PostgreSqlEngine`が、1つのプールを接続の
    /// 間で共有するため）
    pub fn with_pool(pool: PgPool, database_name: String, user_name: String) -> Self {
        Self {
            pool,
            database_name,
            user_name,
            statement_timeout: Duration::from_secs(DEFAULT_STATEMENT_TIMEOUT_SECS),
            fetch_limit: None,
        }
    }

    /// 1つのSQLの時間の上限を設定する
    pub fn with_statement_timeout(mut self, timeout: Duration) -> Self {
        self.statement_timeout = timeout;
        self
    }

    /// `query()`で受け取る行数の上限を設定する
    pub fn with_fetch_limit(mut self, limit: Option<usize>) -> Self {
        self.fetch_limit = limit;
        self
    }

    /// `SET LOCAL statement_timeout`の値（ミリ秒、0は上限なし）
    fn statement_timeout_ms(&self) -> u128 {
        self.statement_timeout.as_millis()
    }

    /// Convert sqlx Row to our Value vec
    fn row_to_values(row: &PgRow) -> Vec<Value> {
        (0..row.len())
            .map(|i| Self::extract_value(row, i))
            .collect()
    }

    /// 列の型に合わせて値を読む。読めない型は、NULLと区別できる文字列にする
    fn extract_value(row: &PgRow, index: usize) -> Value {
        let Ok(raw) = row.try_get_raw(index) else {
            return Value::Null;
        };
        if raw.is_null() {
            return Value::Null;
        }
        let type_name = raw.type_info().name().to_string();
        let decoded = match type_name.as_str() {
            "BOOL" => row.try_get::<bool, _>(index).map(Value::Bool).ok(),
            "INT2" => row
                .try_get::<i16, _>(index)
                .map(|v| Value::Int(v.into()))
                .ok(),
            "INT4" => row
                .try_get::<i32, _>(index)
                .map(|v| Value::Int(v.into()))
                .ok(),
            "INT8" => row.try_get::<i64, _>(index).map(Value::Int).ok(),
            "FLOAT4" => row
                .try_get::<f32, _>(index)
                .map(|v| Value::Float(v.into()))
                .ok(),
            "FLOAT8" => row.try_get::<f64, _>(index).map(Value::Float).ok(),
            "NUMERIC" => raw
                .as_bytes()
                .ok()
                .and_then(numeric_to_string)
                .map(Value::String),
            "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "CITEXT" | "UNKNOWN" => {
                row.try_get::<String, _>(index).map(Value::String).ok()
            }
            "UUID" => row
                .try_get::<uuid::Uuid, _>(index)
                .map(|v| Value::String(v.to_string()))
                .ok(),
            "JSON" | "JSONB" => row
                .try_get::<serde_json::Value, _>(index)
                .map(Value::Json)
                .ok(),
            "TIMESTAMPTZ" => row
                .try_get::<chrono::DateTime<Utc>, _>(index)
                .map(Value::DateTime)
                .ok(),
            // タイムゾーンの無い時刻は、UTCだと決めつけずにそのまま文字列にする
            "TIMESTAMP" => row
                .try_get::<chrono::NaiveDateTime, _>(index)
                .map(|v| Value::String(v.format("%Y-%m-%dT%H:%M:%S%.f").to_string()))
                .ok(),
            "DATE" => row
                .try_get::<chrono::NaiveDate, _>(index)
                .map(|v| Value::String(v.to_string()))
                .ok(),
            "TIME" => row
                .try_get::<chrono::NaiveTime, _>(index)
                .map(|v| Value::String(v.to_string()))
                .ok(),
            "BYTEA" => row.try_get::<Vec<u8>, _>(index).map(Value::Binary).ok(),
            "TEXT[]" | "VARCHAR[]" => row
                .try_get::<Vec<String>, _>(index)
                .map(|v| Value::Json(serde_json::json!(v)))
                .ok(),
            "INT4[]" => row
                .try_get::<Vec<i32>, _>(index)
                .map(|v| Value::Json(serde_json::json!(v)))
                .ok(),
            "INT8[]" => row
                .try_get::<Vec<i64>, _>(index)
                .map(|v| Value::Json(serde_json::json!(v)))
                .ok(),
            _ => None,
        };
        decoded.unwrap_or_else(|| {
            Value::String(format!(
                "[{} value not shown: cast it to text in the query, e.g. column::text]",
                type_name
            ))
        })
    }

    /// パラメーターを`$1`・`$2`…にバインドする
    fn bind_params<'q>(
        mut query: Query<'q, Postgres, PgArguments>,
        params: &'q [Value],
    ) -> Query<'q, Postgres, PgArguments> {
        for param in params {
            query = match param {
                Value::Null => query.bind(None::<String>),
                Value::Bool(b) => query.bind(*b),
                Value::Int(i) => query.bind(*i),
                Value::Float(f) => query.bind(*f),
                Value::String(s) => query.bind(s.as_str()),
                Value::Binary(bytes) => query.bind(bytes.as_slice()),
                Value::Json(json) => query.bind(json),
                Value::DateTime(dt) => query.bind(*dt),
            };
        }
        query
    }
}

/// 設定から、接続の設定を作る。パスワードはURLに埋め込まないので、記号を
/// 含んでいてもよい
pub fn connect_options(config: &ConnectionConfig) -> Result<PgConnectOptions, DatabaseError> {
    let mut options = PgConnectOptions::new()
        .host(&config.host)
        .port(config.port)
        .database(&config.database)
        .username(&config.username)
        .application_name("mcp-rs");
    if !config.password.is_empty() {
        options = options.password(&config.password);
    }
    if let Some(mode) = &config.ssl_mode {
        let mode = PgSslMode::from_str(mode).map_err(|e| {
            DatabaseError::ConfigurationError(format!("invalid ssl_mode '{}': {}", mode, e))
        })?;
        options = options.ssl_mode(mode);
    }
    Ok(options)
}

/// 設定から、接続のプールを作る（接続できるかも確かめる）
pub async fn connect_pool(config: &DatabaseConfig) -> Result<PgPool, DatabaseError> {
    let options = connect_options(&config.connection)?;
    PgPoolOptions::new()
        .max_connections(config.pool.max_connections.max(1))
        .min_connections(0)
        .acquire_timeout(Duration::from_secs(
            u64::from(config.connection.timeout_seconds).max(1),
        ))
        .idle_timeout(Some(Duration::from_secs(u64::from(
            config.pool.idle_timeout.max(1),
        ))))
        .connect_with(options)
        .await
        .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))
}

/// NUMERICのバイナリ形式（`numeric_send`）を、桁を落とさずに10進の文字列に
/// する。形式: 桁の数・weight・符号・表示する小数の桁数（各2バイト）と、
/// 1万進の桁（各2バイト）
pub fn numeric_to_string(bytes: &[u8]) -> Option<String> {
    let read = |i: usize| -> Option<i16> {
        bytes
            .get(i * 2..i * 2 + 2)
            .map(|b| i16::from_be_bytes([b[0], b[1]]))
    };
    let ndigits = usize::try_from(read(0)?).ok()?;
    let weight = i32::from(read(1)?);
    let sign = read(2)? as u16;
    let dscale = usize::try_from(read(3)?).ok()?;
    match sign {
        0xC000 => return Some("NaN".to_string()),
        0xD000 => return Some("Infinity".to_string()),
        0xF000 => return Some("-Infinity".to_string()),
        0x0000 | 0x4000 => {}
        _ => return None,
    }
    let digits: Vec<i16> = (0..ndigits).map(|i| read(4 + i)).collect::<Option<_>>()?;
    let digit = |group: i32| -> i16 {
        usize::try_from(group)
            .ok()
            .and_then(|g| digits.get(g).copied())
            .unwrap_or(0)
    };

    let mut text = String::new();
    if sign == 0x4000 {
        text.push('-');
    }
    if weight < 0 {
        text.push('0');
    } else {
        for group in 0..=weight {
            if group == 0 {
                text.push_str(&digit(group).to_string());
            } else {
                text.push_str(&format!("{:04}", digit(group)));
            }
        }
    }
    if dscale > 0 {
        let mut fraction = String::new();
        let mut group = weight + 1;
        while fraction.len() < dscale {
            fraction.push_str(&format!("{:04}", digit(group)));
            group += 1;
        }
        fraction.truncate(dscale);
        text.push('.');
        text.push_str(&fraction);
    }
    Some(text)
}

#[async_trait]
impl DatabaseConnection for PostgreSqlConnection {
    /// 読み取り専用のトランザクションの中で実行し、ロールバックする
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let started = std::time::Instant::now();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        sqlx::query(&format!(
            "SET LOCAL statement_timeout = {}",
            self.statement_timeout_ms()
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut rows: Vec<PgRow> = Vec::new();
        {
            let mut stream = Self::bind_params(sqlx::query(sql), params).fetch(&mut *tx);
            while let Some(row) = stream
                .try_next()
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?
            {
                rows.push(row);
                if self.fetch_limit.is_some_and(|limit| rows.len() >= limit) {
                    break;
                }
            }
        }

        let columns: Vec<ColumnInfo> = match rows.first() {
            Some(first) => first
                .columns()
                .iter()
                .map(|column| ColumnInfo {
                    name: column.name().to_string(),
                    data_type: column.type_info().name().to_string(),
                    nullable: true,
                    max_length: None,
                })
                .collect(),
            // 行が無くても列の名前と型を返す
            None => (&mut *tx)
                .describe(sql)
                .await
                .map(|described| {
                    described
                        .columns()
                        .iter()
                        .map(|column| ColumnInfo {
                            name: column.name().to_string(),
                            data_type: column.type_info().name().to_string(),
                            nullable: true,
                            max_length: None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        let result_rows: Vec<Vec<Value>> = rows.iter().map(Self::row_to_values).collect();
        // 読むだけなので、コミットしない
        tx.rollback()
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(QueryResult {
            columns,
            total_rows: Some(result_rows.len() as u64),
            rows: result_rows,
            execution_time_ms: started.elapsed().as_millis() as u64,
        })
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let started = std::time::Instant::now();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
        sqlx::query(&format!(
            "SET LOCAL statement_timeout = {}",
            self.statement_timeout_ms()
        ))
        .execute(&mut *tx)
        .await
        .map_err(|e| DatabaseError::OperationFailed(e.to_string()))?;
        let result = Self::bind_params(sqlx::query(sql), params)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::OperationFailed(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| DatabaseError::OperationFailed(e.to_string()))?;

        // PostgreSQL doesn't have last_insert_id like MySQL
        // Would need RETURNING clause for that
        Ok(ExecuteResult {
            rows_affected: result.rows_affected(),
            last_insert_id: None,
            execution_time_ms: started.elapsed().as_millis() as u64,
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
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
             ORDER BY table_name",
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
            let max_length: Option<i32> = row.try_get(3).ok().flatten();

            columns.push(ColumnInfo {
                name,
                data_type,
                nullable: is_nullable == "YES",
                max_length,
            });
        }

        // Get primary keys. 表の名前は識別子として引用し、大文字や記号を
        // 含む名前でも別の表を指さないようにする
        let pk_rows = sqlx::query(
            "SELECT a.attname
             FROM pg_index i
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
             WHERE i.indrelid = format('public.%I', $1::text)::regclass AND i.indisprimary",
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
        // プールは`PostgreSqlEngine`の接続の間で共有しているので、1つの
        // 接続を返すだけでは閉じない（閉じると、他の接続も使えなくなる）
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: "pg-connection".to_string(),
            database_name: self.database_name.clone(),
            user_name: self.user_name.clone(),
            server_version: "PostgreSQL (sqlx)".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `numeric_send`の形式を作る
    fn numeric(weight: i16, sign: u16, dscale: i16, digits: &[i16]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [digits.len() as i16, weight, sign as i16, dscale] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        for digit in digits {
            bytes.extend_from_slice(&digit.to_be_bytes());
        }
        bytes
    }

    #[test]
    fn test_numeric_to_string() {
        let cases: &[(Vec<u8>, &str)] = &[
            (numeric(0, 0, 0, &[]), "0"),
            (numeric(0, 0, 2, &[]), "0.00"),
            (numeric(0, 0, 0, &[42]), "42"),
            (numeric(0, 0x4000, 4, &[123, 4500]), "-123.4500"),
            (numeric(1, 0, 0, &[1, 2345]), "12345"),
            (numeric(-1, 0, 3, &[10]), "0.001"),
            (numeric(-2, 0, 6, &[1200]), "0.000012"),
            (numeric(-2, 0, 8, &[12]), "0.00000012"),
            (numeric(4, 0, 0, &[1]), "10000000000000000"),
            (numeric(0, 0, 2, &[19, 9900]), "19.99"),
            (numeric(0, 0xC000, 0, &[]), "NaN"),
            (numeric(0, 0xD000, 0, &[]), "Infinity"),
            (numeric(0, 0xF000, 0, &[]), "-Infinity"),
        ];
        for (bytes, expected) in cases {
            assert_eq!(numeric_to_string(bytes).as_deref(), Some(*expected));
        }
        // 短すぎる・符号が不正
        assert_eq!(numeric_to_string(&[0, 1]), None);
        assert_eq!(numeric_to_string(&numeric(0, 0x1234, 0, &[1])), None);
    }
}
