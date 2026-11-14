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
//!
//! #539で、次を足した:
//!
//! - 受け取る結果のバイト数の上限。カーソル（`DECLARE … CURSOR`・`FETCH`）で
//!   少しずつ受け取り、上限を超えたら止める。データベースも、それ以上の行を
//!   作らない
//! - 接続をプールに戻すとき、`set_config`等で変えたセッションの設定・ロールを
//!   元に戻す
//! - `get_schema`も、読み取り専用のトランザクションと時間の上限の中で実行する

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
    /// `query()`で受け取る結果のだいたいのバイト数（`Value::approx_json_size`
    /// の合計）の上限（`None`は無制限、#539）。超えた行まで受け取って止める
    /// ので、ハンドラーは上限を超えたかを知れる
    byte_limit: Option<usize>,
}

/// `query()`のカーソルの名前（トランザクションごとに1つ）
const CURSOR_NAME: &str = "mcp_rs_cursor";

/// 1回の`FETCH`で受け取る行数の上限
const MAX_FETCH_BATCH: usize = 500;

/// 最初の`FETCH`で受け取る行数。1行の大きさが分かるまでは1行だけにする
/// （1行が大きいと、途中で止めても、その`FETCH`の残りの行は送られてくる）
const FIRST_FETCH_BATCH: usize = 1;

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
            byte_limit: None,
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

    /// `query()`で受け取る結果のバイト数の上限を設定する（#539）
    pub fn with_byte_limit(mut self, limit: Option<usize>) -> Self {
        self.byte_limit = limit;
        self
    }

    /// `SET LOCAL statement_timeout`の値（ミリ秒、0は上限なし）
    fn statement_timeout_ms(&self) -> u128 {
        self.statement_timeout.as_millis()
    }

    /// 読み取り専用のトランザクションを始め、1つのSQLの時間の上限を付ける。
    /// 最後はロールバックする（コミットしない）
    async fn begin_read_only(&self) -> Result<sqlx::Transaction<'_, Postgres>, DatabaseError> {
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
        Ok(tx)
    }

    /// 次の`FETCH`で受け取る行数。行数・バイト数の上限の残りに合わせ、
    /// 上限を大きく超えて行を作らせないようにする
    fn next_fetch_batch(&self, rows: usize, bytes: usize) -> usize {
        let mut batch = if rows == 0 {
            FIRST_FETCH_BATCH
        } else {
            MAX_FETCH_BATCH
        };
        if let Some(limit) = self.fetch_limit {
            batch = batch.min(limit.saturating_sub(rows));
        }
        if let (Some(limit), true) = (self.byte_limit, rows > 0) {
            let average = (bytes / rows).max(1);
            batch = batch.min(limit.saturating_sub(bytes) / average + 1);
        }
        batch.max(1)
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
                Some(sqlx::Either::Left(types)) => types.iter().map(base_type).collect(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        params
            .iter()
            .enumerate()
            .map(|(i, value)| {
                let param = PgParam::from_value(value, expected.get(i), i + 1)?
                    .coerce(expected.get(i).map(|t| t.name()), i + 1)?;
                match expected.get(i) {
                    Some(ty) => param.check_fits(ty, i + 1),
                    None => Ok(param),
                }
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
        // 値は文に入れない。エラーの文は監査ログにも書くため（#539）
        let invalid = |what: &str| {
            DatabaseError::QueryFailed(format!("parameter ${position} is not a valid {what}"))
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
                Some("UUID") => {
                    Self::Uuid(uuid::Uuid::parse_str(text.trim()).map_err(|_| invalid("uuid"))?)
                }
                Some("DATE") => Self::Date(
                    chrono::NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d")
                        .map_err(|_| invalid("date (YYYY-MM-DD)"))?,
                ),
                Some("TIME") => Self::Time(
                    chrono::NaiveTime::parse_from_str(text.trim(), "%H:%M:%S%.f")
                        .map_err(|_| invalid("time (HH:MM:SS)"))?,
                ),
                Some("TIMESTAMP") => Self::Timestamp(
                    parse_naive_datetime(text)
                        .ok_or_else(|| invalid("timestamp (YYYY-MM-DDTHH:MM:SS)"))?,
                ),
                Some("TIMESTAMPTZ") => Self::TimestampTz(
                    parse_datetime_tz(text)
                        .ok_or_else(|| invalid("timestamp with time zone (RFC 3339)"))?,
                ),
                Some("NUMERIC") => {
                    Self::Numeric(string_to_numeric(text).ok_or_else(|| invalid("number"))?)
                }
                Some("INT2" | "INT4" | "INT8") => {
                    Self::Int(text.trim().parse().map_err(|_| invalid("integer"))?)
                }
                Some("FLOAT4" | "FLOAT8") => {
                    Self::Float(text.trim().parse().map_err(|_| invalid("number"))?)
                }
                Some("BOOL") => Self::Bool(match text.trim().to_ascii_lowercase().as_str() {
                    "true" | "t" | "yes" | "1" => true,
                    "false" | "f" | "no" | "0" => false,
                    _ => return Err(invalid("boolean")),
                }),
                Some("JSON" | "JSONB") => {
                    Self::Json(serde_json::from_str(text).map_err(|_| invalid("JSON"))?)
                }
                // 結果ではbyteaをbase64で返すので、同じ形で受け取る
                Some("BYTEA") => Self::Bytes(
                    base64::Engine::decode(&base64::prelude::BASE64_STANDARD, text.trim())
                        .map_err(|_| invalid("base64 value"))?,
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
                "NUMERIC" => {
                    // RustのDisplayは`NaN`・`inf`なので、numericの綴りに直す
                    let text = if f.is_nan() {
                        "NaN".to_string()
                    } else if f.is_infinite() {
                        if f > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
                    } else {
                        f.to_string()
                    };
                    Self::Numeric(string_to_numeric(&text).ok_or_else(|| out_of_range("numeric"))?)
                }
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

    /// 送る値のバイナリ形式が、PostgreSQLが期待する型（`expected`、ドメインは
    /// 元の型に直したもの）と同じかを確かめる。違う形式を送ると、バイト数が
    /// 偶然合ったときにエラーにならず、別の値として保存されるため（#453の
    /// 再レビュー: moneyに`"$1000.00"`、intervalに16バイトの文字列）
    fn check_fits(
        self,
        expected: &sqlx::postgres::PgTypeInfo,
        position: usize,
    ) -> Result<Self, DatabaseError> {
        use sqlx::postgres::PgTypeKind;
        use sqlx::Encode;
        use sqlx::TypeInfo;
        let expected_name = expected.name();
        let fits = match &self {
            Self::Null(_) => true,
            // 文字列のバイト列がそのままバイナリ形式になる型だけ
            Self::Text(_) => {
                ["TEXT", "VARCHAR", "BPCHAR", "NAME", "CITEXT"]
                    .iter()
                    .any(|name| expected_name.eq_ignore_ascii_case(name))
                    || matches!(expected.kind(), PgTypeKind::Enum(_))
            }
            Self::Json(_) => matches!(expected_name, "JSON" | "JSONB"),
            other => other
                .produces()
                .is_some_and(|produced| produced.name() == expected_name),
        };
        if fits {
            return Ok(self);
        }
        let display_name = expected_name.to_ascii_lowercase();
        Err(DatabaseError::QueryFailed(format!(
            "parameter ${position}: mcp-rs cannot send this value as {display_name}. \
             Pass it as a string and cast it through text in the SQL, e.g. ${position}::text::{display_name}"
        )))
    }
}

/// ドメインを、元の型まで直す（バイナリ形式は元の型と同じ）
fn base_type(ty: &sqlx::postgres::PgTypeInfo) -> sqlx::postgres::PgTypeInfo {
    let mut ty = ty.clone();
    while let sqlx::postgres::PgTypeKind::Domain(base) = ty.kind() {
        ty = base.clone();
    }
    ty
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

/// 結果の列の名前と型
fn column_infos(columns: &[sqlx::postgres::PgColumn]) -> Vec<ColumnInfo> {
    columns
        .iter()
        .map(|column| ColumnInfo {
            name: column.name().to_string(),
            data_type: column.type_info().name().to_string(),
            nullable: true,
            max_length: None,
        })
        .collect()
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
    // 証明書を確かめるCA（#539）。非公開のCA（RDS等）で`verify-ca`・
    // `verify-full`を使うため
    if let Some(path) = config.options.get(super::OPTION_SSL_ROOT_CERT) {
        options = options.ssl_root_cert(path);
    }
    Ok(options)
}

/// TLSが、通信の盗み見・なりすましを防げない設定のとき、その理由を返す
/// （#539）。ループバック（`localhost`・`127.0.0.1`・`::1`）とUnixソケット
/// では返さない。`options`は、環境変数（`PGSSLMODE`等）を反映した後の値
pub fn weak_tls_warning(options: &PgConnectOptions) -> Option<String> {
    let host = options.get_host();
    let loopback = options.get_socket().is_some()
        || host.starts_with('/')
        || host.eq_ignore_ascii_case("localhost")
        || host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback());
    if loopback {
        return None;
    }
    let unencrypted = "the connection may not be encrypted";
    let (mode, risk) = match options.get_ssl_mode() {
        PgSslMode::VerifyFull => return None,
        PgSslMode::VerifyCa => ("verify-ca", "the server's host name is not checked"),
        PgSslMode::Require => ("require", "the server's certificate is not checked"),
        PgSslMode::Prefer => ("prefer", unencrypted),
        PgSslMode::Allow => ("allow", unencrypted),
        PgSslMode::Disable => ("disable", "the connection is not encrypted"),
    };
    Some(format!(
        "database ssl_mode is \"{mode}\" for {host}: {risk}. Use ssl_mode = \"verify-full\"          (with ssl_root_cert for a private CA)"
    ))
}

/// プールに戻す接続で実行する、セッションを元に戻すSQL（#539）
pub const RESET_SESSION_SQL: &str = "SELECT pg_advisory_unlock_all();      SET SESSION AUTHORIZATION DEFAULT; RESET ROLE; RESET ALL";

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
        // プールに戻す接続を、次の利用者のために元に戻す。失敗したら、その
        // 接続は閉じる
        // - セッション単位の勧告ロック（`pg_advisory_lock`）は、ロールバック
        //   しても外れない（#453のレビュー）
        // - `execute_command`の中の`set_config('role', …, false)`等で変えた
        //   設定は、コミットされると残る（#539）。`RESET ALL`は`role`・
        //   `session_authorization`を戻さないので、別に戻す。`DISCARD ALL`は、
        //   sqlxの準備済みの文のキャッシュと合わなくなるので使わない
        .after_release(|conn, _meta| {
            Box::pin(async move {
                // 文字列のまま渡すと、複数の文を1度に送れる（パラメーターの
                // 無い問い合わせ）
                conn.execute(RESET_SESSION_SQL).await.map(|_| true)
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

/// 表1つの列と主キー（`public`のスキーマ）
async fn table_schema(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    table_name: &str,
) -> Result<TableInfo, DatabaseError> {
    // Get column information
    let rows = sqlx::query(
        "SELECT column_name, data_type, is_nullable, character_maximum_length
         FROM information_schema.columns
         WHERE table_name = $1 AND table_schema = 'public'
         ORDER BY ordinal_position",
    )
    .bind(table_name)
    .fetch_all(&mut **tx)
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
    // 含む名前でも別の表を指さないようにする。一覧を読んだ後に消された表で
    // エラーにならないよう、`to_regclass`（無ければNULL）を使う。エラーに
    // なると、トランザクションの残りの問い合わせも失敗する
    let pk_rows = sqlx::query(
        "SELECT a.attname
         FROM pg_index i
         JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey)
         WHERE i.indrelid = to_regclass(format('public.%I', $1::text)) AND i.indisprimary",
    )
    .bind(table_name)
    .fetch_all(&mut **tx)
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

#[async_trait]
impl DatabaseConnection for PostgreSqlConnection {
    /// 読み取り専用のトランザクションの中で実行し、ロールバックする。行は
    /// カーソルで少しずつ受け取り、行数・バイト数の上限で止める（#539）
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let started = std::time::Instant::now();
        let mut tx = self.begin_read_only().await?;

        let params = Self::pg_params(&mut *tx, sql, params).await?;
        // 末尾の`;`は、カーソルの中には書けない
        let body = sql.trim_end().trim_end_matches(';').trim_end();
        let declare = format!("DECLARE {CURSOR_NAME} NO SCROLL CURSOR FOR {body}");
        bind_params(sqlx::query(&declare), params)
            .execute(&mut *tx)
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut columns: Option<Vec<ColumnInfo>> = None;
        let mut result_rows: Vec<Vec<Value>> = Vec::new();
        let mut bytes = 0usize;
        'fetch: loop {
            let batch = self.next_fetch_batch(result_rows.len(), bytes);
            let fetch = format!("FETCH {batch} FROM {CURSOR_NAME}");
            // 準備済みの文として使い回さない。同じ`FETCH`の文でも、カーソルの
            // SQLごとに列が違うので、使い回すと前の列の情報で読んでしまう
            let mut stream = sqlx::query(&fetch).persistent(false).fetch(&mut *tx);
            let mut received = 0usize;
            while let Some(row) = stream
                .try_next()
                .await
                .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?
            {
                received += 1;
                if columns.is_none() {
                    columns = Some(column_infos(row.columns()));
                }
                let values = Self::row_to_values(&row);
                bytes += values.iter().map(Value::approx_json_size).sum::<usize>();
                result_rows.push(values);
                let rows_full = self
                    .fetch_limit
                    .is_some_and(|limit| result_rows.len() >= limit);
                let bytes_full = self.byte_limit.is_some_and(|limit| bytes > limit);
                if rows_full || bytes_full {
                    break 'fetch;
                }
            }
            if received < batch {
                break;
            }
        }

        let columns = match columns {
            Some(columns) => columns,
            // 行が無くても列の名前と型を返す
            None => (&mut *tx)
                .describe(sql)
                .await
                .map(|described| column_infos(described.columns()))
                .unwrap_or_default(),
        };
        // 読むだけなので、コミットしない。カーソルも閉じ、データベースは
        // 残りの行を作らない
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
        // `pg_params`のエラーは、包み直さずにそのまま返す（前置きが二重になる）
        let params = Self::pg_params(&mut *tx, sql, params).await?;
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

    /// 読み取り専用のトランザクションと時間の上限の中で読む（#539）
    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let mut tx = self.begin_read_only().await?;
        // Get all tables in the current database
        let rows = sqlx::query(
            "SELECT table_name FROM information_schema.tables
             WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
             ORDER BY table_name",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        let mut tables = Vec::new();
        for row in rows {
            let table_name: String = row.get(0);
            // 1つの表で失敗しても、トランザクションは使えなくなる。時間切れ等の
            // エラーは、そのまま返す
            tables.push(table_schema(&mut tx, &table_name).await?);
        }
        tx.rollback()
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;

        Ok(DatabaseSchema {
            database_name: self.database_name.clone(),
            tables,
            views: Vec::new(),
            procedures: Vec::new(),
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        let mut tx = self.begin_read_only().await?;
        let table = table_schema(&mut tx, table_name).await?;
        tx.rollback()
            .await
            .map_err(|e| DatabaseError::QueryFailed(e.to_string()))?;
        Ok(table)
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
        // 値はエラーの文に入れない（監査ログにも書くため、#539）
        assert!(!err.contains("not-a-uuid"), "{err}");
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
        // floatのNaN・Infinityも、numericの形式で送る
        for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                coerce(Value::Float(f), "NUMERIC"),
                Ok(PgParam::Numeric(_))
            ));
        }

        // 形式の合わない型には送らず、textを経由するキャストを案内する
        // （money・intervalに文字列のバイトを送ると、別の値として保存されうる）
        fn type_of<T: sqlx::Type<Postgres>>() -> PgTypeInfo {
            T::type_info()
        }
        let interval = type_of::<sqlx::postgres::types::PgInterval>();
        let money = type_of::<sqlx::postgres::types::PgMoney>();
        let err = PgParam::Text("1 day 2 hours 3m".into())
            .check_fits(&interval, 2)
            .unwrap_err()
            .to_string();
        assert!(err.contains("$2::text::interval"), "{err}");
        assert!(PgParam::Int(1000).check_fits(&money, 1).is_err());
        assert!(PgParam::Text("x".into())
            .check_fits(&type_of::<String>(), 1)
            .is_ok());
        assert!(PgParam::Int4(1).check_fits(&type_of::<i32>(), 1).is_ok());
        assert!(PgParam::Json(serde_json::json!({}))
            .check_fits(&type_of::<serde_json::Value>(), 1)
            .is_ok());
        assert!(PgParam::Null(money.clone()).check_fits(&money, 1).is_ok());

        // 型が分からないNULLはOID 0（PostgreSQLに推論させる）
        assert!(matches!(
            PgParam::from_value(&Value::Null, None, 1),
            Ok(PgParam::Null(_))
        ));
    }

    /// ループバック以外で`verify-full`より弱いTLSなら、警告する（#539）
    #[test]
    fn test_weak_tls_warning() {
        let options = |host: &str, mode: Option<&str>| {
            let mut config = ConnectionConfig {
                host: host.to_string(),
                ..ConnectionConfig::default()
            };
            config.ssl_mode = mode.map(str::to_string);
            connect_options(&config).unwrap()
        };
        for host in [
            "localhost",
            "127.0.0.1",
            "::1",
            "[::1]",
            "/var/run/postgresql",
        ] {
            assert_eq!(
                weak_tls_warning(&options(host, Some("disable"))),
                None,
                "{host}"
            );
        }
        assert_eq!(
            weak_tls_warning(&options("db.example.com", Some("verify-full"))),
            None
        );
        for (mode, risk) in [
            ("verify-ca", "host name"),
            ("require", "certificate"),
            ("prefer", "may not be encrypted"),
            ("disable", "not encrypted"),
        ] {
            let warning = weak_tls_warning(&options("db.example.com", Some(mode)))
                .unwrap_or_else(|| panic!("{mode}"));
            assert!(
                warning.contains(mode) && warning.contains(risk),
                "{warning}"
            );
        }
    }

    /// `FETCH`の行数は、行数・バイト数の上限の残りに合わせる（#539）
    #[tokio::test]
    async fn test_next_fetch_batch_follows_the_limits() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://localhost/x")
            .unwrap();
        let conn = PostgreSqlConnection::with_pool(pool, "x".into(), "x".into());
        // 上限なし: 最初は1行、その後はまとめて
        assert_eq!(conn.next_fetch_batch(0, 0), 1);
        assert_eq!(conn.next_fetch_batch(1, 10), MAX_FETCH_BATCH);
        let conn = conn
            .with_fetch_limit(Some(1001))
            .with_byte_limit(Some(10_000));
        assert_eq!(conn.next_fetch_batch(0, 0), 1);
        // 行数の残り
        assert_eq!(conn.next_fetch_batch(1000, 1000), 1);
        // 1行100バイト、残り9000バイト: 90行で上限に届き、1行多く読む
        assert_eq!(conn.next_fetch_batch(10, 1000), 91);
        // 上限を超えていても、0にはしない
        assert_eq!(conn.next_fetch_batch(1, 20_000), 1);
    }
}
