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
            // 値の無い型（`pg_advisory_lock`等の戻り値）
            "VOID" => Some(Value::Null),
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

    /// パラメーターを、PostgreSQLが期待する型に合わせて作る。文字列とNULLが
    /// あれば、先にSQLを`describe`して、各パラメーターの型を聞く（#453の
    /// レビュー）。文字列を`text`のまま送ると、uuid・日付・numeric等の列と
    /// 比べられず（`operator does not exist: uuid = text`）、結果で返した値を
    /// そのままパラメーターに戻せないため
    async fn pg_params<'e, E>(
        executor: E,
        sql: &str,
        params: &[Value],
    ) -> Result<Vec<PgParam>, DatabaseError>
    where
        E: Executor<'e, Database = Postgres>,
    {
        // パラメーターがあれば、必ず型を聞く。sqlxは同じSQLの準備済みの文を
        // 使い回すので、一度`describe`した文は、推論された型のまま使われる。
        // 送る値の型も、それに正確に合わせる（int4のパラメーターにint8を送ると
        // `incorrect binary data format`になる）
        let needs_types = !params.is_empty();
        let expected: Vec<sqlx::postgres::PgTypeInfo> = if needs_types {
            let described = executor
                .describe(sql)
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
            match described.parameters() {
                Some(sqlx::Either::Left(types)) => types.to_vec(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        params
            .iter()
            .enumerate()
            .map(|(i, value)| {
                PgParam::from_value(value, expected.get(i), i + 1)?
                    .coerce(expected.get(i).map(|t| t.name()), i + 1)
            })
            .collect()
    }
}

/// PostgreSQLに送るパラメーター1つ。`produces`で型を伝える
#[derive(Debug, Clone)]
enum PgParam {
    /// 型の分かったNULL（分からなければOID 0で、PostgreSQLに推論させる）
    Null(sqlx::postgres::PgTypeInfo),
    Bool(bool),
    Int2(i16),
    Int4(i32),
    Int(i64),
    Float4(f32),
    Float(f64),
    Text(String),
    Uuid(uuid::Uuid),
    Date(chrono::NaiveDate),
    Time(chrono::NaiveTime),
    Timestamp(chrono::NaiveDateTime),
    TimestampTz(chrono::DateTime<Utc>),
    /// `numeric_send`の形式のバイト列
    Numeric(Vec<u8>),
    Json(serde_json::Value),
    Bytes(Vec<u8>),
}

impl PgParam {
    /// `value`を、`expected`（PostgreSQLが期待する型）に合わせる。`position`
    /// はエラーの文に出す`$n`の番号
    fn from_value(
        value: &Value,
        expected: Option<&sqlx::postgres::PgTypeInfo>,
        position: usize,
    ) -> Result<Self, DatabaseError> {
        let expected_name = expected.map(|t| t.name().to_string());
        let invalid = |what: &str, text: &str| {
            DatabaseError::QueryFailed(format!(
                "parameter ${position}: '{}' is not a valid {what}",
                text.chars().take(100).collect::<String>()
            ))
        };
        Ok(match value {
            Value::Null => Self::Null(expected.cloned().unwrap_or_else(|| {
                sqlx::postgres::PgTypeInfo::with_oid(sqlx::postgres::types::Oid(0))
            })),
            Value::Bool(b) => Self::Bool(*b),
            Value::Int(i) => Self::Int(*i),
            Value::Float(f) => Self::Float(*f),
            Value::Binary(bytes) => Self::Bytes(bytes.clone()),
            Value::Json(json) => Self::Json(json.clone()),
            Value::DateTime(dt) => Self::TimestampTz(*dt),
            Value::String(text) => match expected_name.as_deref() {
                Some("UUID") => Self::Uuid(
                    uuid::Uuid::parse_str(text.trim()).map_err(|_| invalid("uuid", text))?,
                ),
                Some("DATE") => Self::Date(
                    chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
                        .map_err(|_| invalid("date (YYYY-MM-DD)", text))?,
                ),
                Some("TIME") => Self::Time(
                    chrono::NaiveTime::parse_from_str(text.trim(), "%H:%M:%S%.f")
                        .map_err(|_| invalid("time (HH:MM:SS)", text))?,
                ),
                Some("TIMESTAMP") => Self::Timestamp(
                    parse_naive_datetime(text)
                        .ok_or_else(|| invalid("timestamp (YYYY-MM-DDTHH:MM:SS)", text))?,
                ),
                Some("TIMESTAMPTZ") => Self::TimestampTz(
                    parse_datetime_tz(text)
                        .ok_or_else(|| invalid("timestamp with time zone (RFC 3339)", text))?,
                ),
                Some("NUMERIC") => {
                    Self::Numeric(string_to_numeric(text).ok_or_else(|| invalid("number", text))?)
                }
                Some("INT2" | "INT4" | "INT8") => {
                    Self::Int(text.trim().parse().map_err(|_| invalid("integer", text))?)
                }
                Some("FLOAT4" | "FLOAT8") => {
                    Self::Float(text.trim().parse().map_err(|_| invalid("number", text))?)
                }
                Some("BOOL") => Self::Bool(match text.trim().to_ascii_lowercase().as_str() {
                    "true" | "t" | "yes" | "1" => true,
                    "false" | "f" | "no" | "0" => false,
                    _ => return Err(invalid("boolean", text)),
                }),
                Some("JSON" | "JSONB") => {
                    Self::Json(serde_json::from_str(text).map_err(|_| invalid("JSON", text))?)
                }
                // 結果ではbyteaをbase64で返すので、同じ形で受け取る
                Some("BYTEA") => Self::Bytes(
                    base64::Engine::decode(&base64::prelude::BASE64_STANDARD, text.trim())
                        .map_err(|_| invalid("base64 value", text))?,
                ),
                // text・varchar等と、ここで変換できない型（列挙型等）。後者は
                // SQLで`$1::型`とキャストしてもらう
                _ => Self::Text(text.clone()),
            },
        })
    }
}

impl PgParam {
    /// 送る型を、PostgreSQLが推論した型（`expected`）に正確に合わせる。
    /// 準備済みの文はその型で使われるので、幅の違う整数等も変換する。
    /// 合わせられない組み合わせは、そのまま送る（PostgreSQLがエラーにする）
    fn coerce(self, expected: Option<&str>, position: usize) -> Result<Self, DatabaseError> {
        let Some(expected) = expected else {
            return Ok(self);
        };
        let out_of_range = |what: &str| {
            DatabaseError::QueryFailed(format!(
                "parameter ${position}: the value does not fit in {what}"
            ))
        };
        let is_text = matches!(expected, "TEXT" | "VARCHAR" | "BPCHAR" | "NAME");
        Ok(match self {
            Self::Int(i) => match expected {
                "INT2" => Self::Int2(i16::try_from(i).map_err(|_| out_of_range("smallint"))?),
                "INT4" => Self::Int4(i32::try_from(i).map_err(|_| out_of_range("integer"))?),
                "FLOAT4" => Self::Float4(i as f32),
                "FLOAT8" => Self::Float(i as f64),
                "NUMERIC" => Self::Numeric(
                    string_to_numeric(&i.to_string()).ok_or_else(|| out_of_range("numeric"))?,
                ),
                _ if is_text => Self::Text(i.to_string()),
                _ => Self::Int(i),
            },
            Self::Float(f) => match expected {
                "FLOAT4" => Self::Float4(f as f32),
                "NUMERIC" if f.is_finite() => Self::Numeric(
                    string_to_numeric(&f.to_string()).ok_or_else(|| out_of_range("numeric"))?,
                ),
                "INT2" | "INT4" | "INT8" if f.fract() == 0.0 => {
                    Self::Int(f as i64).coerce(Some(expected), position)?
                }
                _ if is_text => Self::Text(f.to_string()),
                _ => Self::Float(f),
            },
            Self::Bool(b) if is_text => Self::Text(b.to_string()),
            Self::Json(json) if is_text => Self::Text(match json {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            }),
            Self::TimestampTz(dt) => match expected {
                "TIMESTAMP" => Self::Timestamp(dt.naive_utc()),
                "DATE" => Self::Date(dt.date_naive()),
                _ if is_text => Self::Text(dt.to_rfc3339()),
                _ => Self::TimestampTz(dt),
            },
            other => other,
        })
    }
}

impl sqlx::Type<Postgres> for PgParam {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <String as sqlx::Type<Postgres>>::type_info()
    }

    fn compatible(_ty: &sqlx::postgres::PgTypeInfo) -> bool {
        true
    }
}

impl<'q> sqlx::Encode<'q, Postgres> for PgParam {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        use sqlx::Encode;
        match self {
            Self::Null(_) => Ok(sqlx::encode::IsNull::Yes),
            Self::Bool(v) => <bool as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Int2(v) => <i16 as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Int4(v) => <i32 as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Float4(v) => <f32 as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Int(v) => <i64 as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Float(v) => <f64 as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Text(v) => <String as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Uuid(v) => <uuid::Uuid as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Date(v) => <chrono::NaiveDate as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Time(v) => <chrono::NaiveTime as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Timestamp(v) => {
                <chrono::NaiveDateTime as Encode<Postgres>>::encode_by_ref(v, buf)
            }
            Self::TimestampTz(v) => {
                <chrono::DateTime<Utc> as Encode<Postgres>>::encode_by_ref(v, buf)
            }
            Self::Numeric(bytes) => {
                buf.extend_from_slice(bytes);
                Ok(sqlx::encode::IsNull::No)
            }
            Self::Json(v) => <serde_json::Value as Encode<Postgres>>::encode_by_ref(v, buf),
            Self::Bytes(v) => <Vec<u8> as Encode<Postgres>>::encode_by_ref(v, buf),
        }
    }

    fn produces(&self) -> Option<sqlx::postgres::PgTypeInfo> {
        use sqlx::Type;
        Some(match self {
            Self::Null(ty) => ty.clone(),
            Self::Bool(_) => <bool as Type<Postgres>>::type_info(),
            Self::Int2(_) => <i16 as Type<Postgres>>::type_info(),
            Self::Int4(_) => <i32 as Type<Postgres>>::type_info(),
            Self::Float4(_) => <f32 as Type<Postgres>>::type_info(),
            Self::Int(_) => <i64 as Type<Postgres>>::type_info(),
            Self::Float(_) => <f64 as Type<Postgres>>::type_info(),
            Self::Text(_) => <String as Type<Postgres>>::type_info(),
            Self::Uuid(_) => <uuid::Uuid as Type<Postgres>>::type_info(),
            Self::Date(_) => <chrono::NaiveDate as Type<Postgres>>::type_info(),
            Self::Time(_) => <chrono::NaiveTime as Type<Postgres>>::type_info(),
            Self::Timestamp(_) => <chrono::NaiveDateTime as Type<Postgres>>::type_info(),
            Self::TimestampTz(_) => <chrono::DateTime<Utc> as Type<Postgres>>::type_info(),
            Self::Numeric(_) => sqlx::postgres::PgTypeInfo::with_name("NUMERIC"),
            Self::Json(_) => <serde_json::Value as Type<Postgres>>::type_info(),
            Self::Bytes(_) => <Vec<u8> as Type<Postgres>>::type_info(),
        })
    }
}

/// パラメーターを`$1`・`$2`…にバインドする
fn bind_params<'q>(
    mut query: Query<'q, Postgres, PgArguments>,
    params: Vec<PgParam>,
) -> Query<'q, Postgres, PgArguments> {
    for param in params {
        query = query.bind(param);
    }
    query
}

/// `2026-11-11T12:34:56.5`・`2026-11-11 12:34:56`等（タイムゾーン無し）
fn parse_naive_datetime(text: &str) -> Option<chrono::NaiveDateTime> {
    let text = text.trim().replacen(' ', "T", 1);
    chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%dT%H:%M:%S%.f").ok()
}

/// RFC 3339（`2026-11-11T03:34:56+00:00`・`…Z`・日付と時刻の間が空白でもよい）
fn parse_datetime_tz(text: &str) -> Option<chrono::DateTime<Utc>> {
    let text = text.trim().replacen(' ', "T", 1);
    chrono::DateTime::parse_from_rfc3339(&text)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// 10進の文字列を、`numeric_send`の形式にする（`numeric_to_string`の逆）。
/// `-123.4500`・`.5`・`NaN`・`Infinity`。指数（`1e5`）は受け付けない
pub fn string_to_numeric(text: &str) -> Option<Vec<u8>> {
    let header = |ndigits: i16, weight: i16, sign: u16, dscale: i16| -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [ndigits, weight, sign as i16, dscale] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes
    };
    let text = text.trim();
    match text {
        "NaN" => return Some(header(0, 0, 0xC000, 0)),
        "Infinity" | "+Infinity" => return Some(header(0, 0, 0xD000, 0)),
        "-Infinity" => return Some(header(0, 0, 0xF000, 0)),
        _ => {}
    }
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (int_part, frac_part) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if (int_part.is_empty() && frac_part.is_empty())
        || !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let dscale = i16::try_from(frac_part.len()).ok()?;
    let int_part = int_part.trim_start_matches('0');
    // 1万進の桁: 整数部は左を、小数部は右を0で埋めて4桁ずつに
    let int_padded = format!("{}{}", "0".repeat((4 - int_part.len() % 4) % 4), int_part);
    let frac_padded = format!("{}{}", frac_part, "0".repeat((4 - frac_part.len() % 4) % 4));
    let mut groups: Vec<i16> = int_padded
        .as_bytes()
        .chunks(4)
        .chain(frac_padded.as_bytes().chunks(4))
        .map(|chunk| std::str::from_utf8(chunk).ok()?.parse::<i16>().ok())
        .collect::<Option<_>>()?;
    let mut weight = i16::try_from(int_padded.len() / 4).ok()? - 1;
    // 先頭と末尾の0の桁を除く
    let leading = groups.iter().take_while(|g| **g == 0).count();
    groups.drain(..leading);
    weight -= i16::try_from(leading).ok()?;
    while groups.last() == Some(&0) {
        groups.pop();
    }
    if groups.is_empty() {
        return Some(header(0, 0, 0, dscale));
    }
    let mut bytes = header(
        i16::try_from(groups.len()).ok()?,
        weight,
        if negative { 0x4000 } else { 0 },
        dscale,
    );
    for group in groups {
        bytes.extend_from_slice(&group.to_be_bytes());
    }
    Some(bytes)
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
        // セッション単位の勧告ロック（`pg_advisory_lock`）は、ロールバックしても
        // 外れない。プールに戻す接続がロックを持ち続けないよう、外す（#453の
        // レビュー）。失敗したら、その接続は閉じる
        .after_release(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SELECT pg_advisory_unlock_all()")
                    .execute(&mut *conn)
                    .await
                    .map(|_| true)
            })
        })
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
            let params = Self::pg_params(&mut *tx, sql, params).await?;
            let mut stream = bind_params(sqlx::query(sql), params).fetch(&mut *tx);
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
        let params = Self::pg_params(&mut *tx, sql, params)
            .await
            .map_err(|e| DatabaseError::OperationFailed(e.to_string()))?;
        let result = bind_params(sqlx::query(sql), params)
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

    /// 文字列→`numeric_send`→文字列で、同じ値に戻る
    #[test]
    fn test_string_to_numeric_round_trips() {
        for text in [
            "0",
            "0.00",
            "42",
            "-123.4500",
            "12345",
            "0.001",
            "0.000012",
            "0.00000012",
            "10000000000000000",
            "19.99",
            "12345678901234.5678",
            "-0.0001",
            "1.50",
            "99999999",
            "NaN",
            "Infinity",
            "-Infinity",
        ] {
            let bytes = string_to_numeric(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(numeric_to_string(&bytes).as_deref(), Some(text), "{text}");
        }
        // 書き方の違い（先頭の0・+・小数点の前が無い）
        let normalized = |text: &str| numeric_to_string(&string_to_numeric(text).unwrap());
        assert_eq!(normalized("007").as_deref(), Some("7"));
        assert_eq!(normalized("+5").as_deref(), Some("5"));
        assert_eq!(normalized(".5").as_deref(), Some("0.5"));
        for invalid in ["", "-", ".", "1e5", "1.2.3", "abc", "1,000"] {
            assert_eq!(string_to_numeric(invalid), None, "{invalid}");
        }
    }

    /// 文字列のパラメーターを、PostgreSQLが期待する型に合わせる
    #[test]
    fn test_pg_param_from_string_follows_the_expected_type() {
        use sqlx::postgres::PgTypeInfo;
        let convert = |text: &str, expected: &'static str| {
            PgParam::from_value(
                &Value::String(text.to_string()),
                Some(&PgTypeInfo::with_name(expected)),
                1,
            )
        };
        assert!(matches!(
            convert("6f1c2a3e-9b2d-4c1a-8f00-123456789abc", "UUID"),
            Ok(PgParam::Uuid(_))
        ));
        assert!(matches!(
            convert("2026-11-11", "DATE"),
            Ok(PgParam::Date(_))
        ));
        assert!(matches!(
            convert("2026-11-11T12:34:56.500", "TIMESTAMP"),
            Ok(PgParam::Timestamp(_))
        ));
        assert!(matches!(
            convert("2026-11-11T03:34:56+00:00", "TIMESTAMPTZ"),
            Ok(PgParam::TimestampTz(_))
        ));
        assert!(matches!(
            convert("1.50", "NUMERIC"),
            Ok(PgParam::Numeric(_))
        ));
        assert!(matches!(convert("42", "INT4"), Ok(PgParam::Int(42))));
        assert!(matches!(convert("true", "BOOL"), Ok(PgParam::Bool(true))));
        assert!(matches!(
            convert("{\"a\":1}", "JSONB"),
            Ok(PgParam::Json(_))
        ));
        assert!(matches!(convert("AQI=", "BYTEA"), Ok(PgParam::Bytes(ref b)) if b == &[1, 2]));
        assert!(matches!(convert("hello", "TEXT"), Ok(PgParam::Text(_))));
        // 変換できない値は、どのパラメーターかを示すエラー
        let err = convert("not-a-uuid", "UUID").unwrap_err().to_string();
        assert!(err.contains("$1") && err.contains("uuid"), "{err}");
        // 数値は、推論された型の幅に合わせる（準備済みの文はその型で使われる）
        let coerce = |value: Value, expected: &'static str| {
            PgParam::from_value(&value, Some(&PgTypeInfo::with_name(expected)), 1)
                .unwrap()
                .coerce(Some(expected), 1)
        };
        assert!(matches!(
            coerce(Value::Int(7), "INT4"),
            Ok(PgParam::Int4(7))
        ));
        assert!(matches!(
            coerce(Value::Int(7), "INT2"),
            Ok(PgParam::Int2(7))
        ));
        assert!(matches!(coerce(Value::Int(7), "INT8"), Ok(PgParam::Int(7))));
        assert!(matches!(
            coerce(Value::Int(7), "NUMERIC"),
            Ok(PgParam::Numeric(_))
        ));
        assert!(matches!(
            coerce(Value::Float(1.5), "NUMERIC"),
            Ok(PgParam::Numeric(_))
        ));
        assert!(matches!(coerce(Value::Int(7), "TEXT"), Ok(PgParam::Text(ref s)) if s == "7"));
        assert!(matches!(
            coerce(Value::Float(2.0), "INT4"),
            Ok(PgParam::Int4(2))
        ));
        assert!(matches!(
            coerce(Value::String("7".into()), "INT2"),
            Ok(PgParam::Int2(7))
        ));
        let err = coerce(Value::Int(100_000), "INT2").unwrap_err().to_string();
        assert!(err.contains("$1") && err.contains("smallint"), "{err}");

        // 型が分からないNULLはOID 0（PostgreSQLに推論させる）
        assert!(matches!(
            PgParam::from_value(&Value::Null, None, 1),
            Ok(PgParam::Null(_))
        ));
    }
}
