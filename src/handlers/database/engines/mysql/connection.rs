//! MySQL・MariaDBの接続（`[database]`の`engine = "mysql"`・`"mariadb"`、#527）
//!
//! PostgreSQL（`pg_connection`）と同じ安全性にそろえる:
//!
//! - `query()`は、毎回、読み取り専用のトランザクション（`START TRANSACTION
//!   READ ONLY`）の中で実行し、最後にロールバックする。書き込む関数等、
//!   SELECTに見えて書き込むSQLも、データベースが拒否する
//! - 1つのSQLの時間の上限。SELECTは、サーバーの`MAX_EXECUTION_TIME`
//!   （MariaDBは`max_statement_time`）で止める。それを超えて終わらないとき（書き込みや、ロックの待ち）は、
//!   別の接続から`KILL QUERY`で止め、その接続は閉じる。閉じるのにも時間の
//!   上限を付ける（`KILL QUERY`が効かない構成でも待ち続けない）
//! - 受け取る行数・結果のだいたいのバイト数の上限。上限に達したら、残りの
//!   行は受け取らずに、接続を閉じる（サーバーも、送るのをやめる）。MySQLの
//!   プロトコルには、PostgreSQLのカーソル（`FETCH`）のような、少しずつ
//!   受け取る仕組みが無いため
//! - パラメーターは文字列に埋め込まず、`?`にバインドする
//! - 接続をプールに戻すとき、`COM_RESET_CONNECTION`で、セッションの変数・
//!   ロック（`GET_LOCK`）・一時表を元に戻す（`mysql_async`のプールの既定）
//!
//! MariaDBは、MySQLと同じプロトコルなので、同じ実装を使う。違いは、時間の
//! 上限の変数（`ServerKind`）だけ。どちらかは、つないだサーバーの版で決める
//!
//! 以前の実装（#527の前）は、接続のたびにプールを作り、TLSを使えず、
//! 全部の行をメモリに読み、時間の上限も無かった。

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    types::{
        ColumnInfo, DatabaseError, DatabaseSchema, ExecuteResult, ForeignKeyInfo, IndexInfo,
        QueryResult, TableInfo, Value, ViewInfo,
    },
};
use async_trait::async_trait;
use mysql_async::consts::{ColumnFlags, ColumnType};
use mysql_async::{prelude::*, Column, Conn, Opts, Pool};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// 1つのSQLの時間の上限の既定（秒）
const DEFAULT_STATEMENT_TIMEOUT_SECS: u64 = 30;

/// サーバーの時間の上限（`MAX_EXECUTION_TIME`）より、こちらで待つ時間を
/// 少し長くする。サーバーが止めたときは、そのエラーを返したいため
const CLIENT_TIMEOUT_GRACE: Duration = Duration::from_secs(2);

/// `KILL QUERY`のための接続を作るときの待ち時間の上限
const KILL_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 止めた接続を閉じるときの待ち時間の上限（PR #572のレビュー）。
/// `disconnect`は、閉じる前に読み残しの結果を最後まで受信するので、
/// `KILL QUERY`が効かないと、いつまでも終わらない
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// プールから接続を受け取るときの待ち時間の上限の既定（秒）
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// 文字コードの番号のうち、バイナリ（`binary`）
const BINARY_CHARSET: u16 = 63;

/// つないだサーバーの種類。時間の上限の変数が違う
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerKind {
    MySql,
    MariaDb,
}

impl ServerKind {
    /// `SELECT VERSION()`の値から決める（MariaDBは`11.4.2-MariaDB-...`）
    pub fn from_version(version: &str) -> Self {
        if version.to_ascii_lowercase().contains("mariadb") {
            Self::MariaDb
        } else {
            Self::MySql
        }
    }

    /// ログ・エラーの文に出す名前
    pub fn name(self) -> &'static str {
        match self {
            Self::MySql => "MySQL",
            Self::MariaDb => "MariaDB",
        }
    }

    /// SELECTの時間の上限を付けるSQL（`timeout_ms`はミリ秒、0は上限なし）。
    /// MySQLの`MAX_EXECUTION_TIME`はミリ秒でSELECTだけに効く。MariaDBには
    /// それが無く、`max_statement_time`（秒、小数も可）を使う
    fn statement_timeout_sql(self, timeout_ms: u128) -> String {
        match self {
            Self::MySql => format!("SET SESSION MAX_EXECUTION_TIME = {timeout_ms}"),
            Self::MariaDb => format!(
                "SET SESSION max_statement_time = {}.{:03}",
                timeout_ms / 1000,
                timeout_ms % 1000
            ),
        }
    }
}

/// MySQL Database Connection（MariaDBも）
pub struct MySqlConnection {
    pool: Pool,
    /// `KILL QUERY`のための接続を作るときの設定
    opts: Opts,
    database_name: String,
    user_name: String,
    /// 1つのSQLの時間の上限
    statement_timeout: Duration,
    /// `query()`で受け取る行数の上限（`None`は無制限）。ハンドラーは
    /// `max_rows + 1`を渡し、上限を超えたかを知る
    fetch_limit: Option<usize>,
    /// `query()`で受け取る結果のだいたいのバイト数の上限（`None`は無制限）
    byte_limit: Option<usize>,
    /// プールから接続を受け取るときの待ち時間の上限。データベースが応答
    /// しないとき・プールの接続がすべて使用中のときに、待ち続けない
    connect_timeout: Duration,
    /// つないだサーバーの種類
    server: ServerKind,
}

/// SQLを実行した結果。`Stopped`のときは、その接続をプールに戻さずに閉じる
enum Outcome<T> {
    Done(T),
    /// 行数・バイト数の上限で、残りの行を受け取らずに止めた
    Stopped(T),
}

impl MySqlConnection {
    /// プールを共有する接続を作る（プールは`MySqlEngine`が1つだけ作る）
    pub fn with_pool(pool: Pool, opts: Opts, database_name: String, user_name: String) -> Self {
        Self {
            pool,
            opts,
            database_name,
            user_name,
            statement_timeout: Duration::from_secs(DEFAULT_STATEMENT_TIMEOUT_SECS),
            fetch_limit: None,
            byte_limit: None,
            connect_timeout: Duration::from_secs(DEFAULT_CONNECT_TIMEOUT_SECS),
            server: ServerKind::MySql,
        }
    }

    /// つないだサーバーの種類（`MySqlEngine`が、最初に版を見て決める）
    pub fn with_server(mut self, server: ServerKind) -> Self {
        self.server = server;
        self
    }

    /// プールから接続を受け取るときの待ち時間の上限
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout.max(Duration::from_secs(1));
        self
    }

    /// 1つのSQLの時間の上限（0は上限なし）
    pub fn with_statement_timeout(mut self, timeout: Duration) -> Self {
        self.statement_timeout = timeout;
        self
    }

    /// `query()`で受け取る行数の上限
    pub fn with_fetch_limit(mut self, limit: Option<usize>) -> Self {
        self.fetch_limit = limit;
        self
    }

    /// `query()`で受け取る結果のだいたいのバイト数の上限
    pub fn with_byte_limit(mut self, limit: Option<usize>) -> Self {
        self.byte_limit = limit;
        self
    }

    async fn get_conn(&self) -> Result<Conn, DatabaseError> {
        match tokio::time::timeout(self.connect_timeout, self.pool.get_conn()).await {
            Ok(conn) => conn.map_err(|e| DatabaseError::ConnectionFailed(e.to_string())),
            Err(_) => Err(DatabaseError::ConnectionFailed(format!(
                "no {} connection within {} seconds (the database did not respond, or all {} connections are in use)",
                self.server.name(),
                self.connect_timeout.as_secs(),
                self.pool_size(),
            ))),
        }
    }

    /// プールの接続の数の上限（エラーの文に出す）
    fn pool_size(&self) -> usize {
        self.opts.pool_opts().constraints().max()
    }

    /// SELECTの時間の上限（ミリ秒、0は上限なし）
    fn statement_timeout_ms(&self) -> u128 {
        self.statement_timeout.as_millis()
    }

    /// こちらで待つ時間の上限（`None`は上限なし）
    fn client_timeout(&self) -> Option<Duration> {
        (!self.statement_timeout.is_zero()).then(|| self.statement_timeout + CLIENT_TIMEOUT_GRACE)
    }

    /// `conn`で`work`を実行する。時間の上限を超えたら、別の接続から
    /// `KILL QUERY`で止め、`conn`は閉じる。上限で止めたとき（`Stopped`）も
    /// 閉じる。それ以外は、プールに戻す（戻すときにセッションを元に戻す）
    async fn run<T>(
        &self,
        mut conn: Conn,
        work: impl for<'c> FnOnce(
            &'c mut Conn,
        ) -> futures::future::BoxFuture<
            'c,
            Result<Outcome<T>, mysql_async::Error>,
        >,
        map_err: fn(String) -> DatabaseError,
    ) -> Result<T, DatabaseError> {
        let id = conn.id();
        let future = work(&mut conn);
        let result = match self.client_timeout() {
            Some(limit) => match tokio::time::timeout(limit, future).await {
                Ok(result) => result,
                Err(_) => {
                    self.abandon(conn, id).await;
                    return Err(DatabaseError::TimeoutError(format!(
                        "the statement did not finish within {} seconds and was stopped",
                        self.statement_timeout.as_secs()
                    )));
                }
            },
            None => future.await,
        };
        match result {
            Ok(Outcome::Done(value)) => Ok(value),
            Ok(Outcome::Stopped(value)) => {
                // 残りの行は受け取らない
                self.abandon(conn, id).await;
                Ok(value)
            }
            Err(e) => {
                // エラーの後は、プールに戻すときにセッションを元に戻す
                // （ロールバックもされる）
                Err(map_err(e.to_string()))
            }
        }
    }

    /// 実行中の文を止めて、`conn`を閉じる。別の接続から`KILL QUERY`で
    /// 止め（サーバーも送るのをやめる）、閉じる。`KILL QUERY`が効かない
    /// とき（接続の数の上限、プロキシで接続のIDがMySQLのスレッドと違う等）、
    /// `disconnect`は読み残しを受信し続けるので、`DISCONNECT_TIMEOUT`で
    /// 打ち切る。打ち切った接続は`disconnected`の印が付いているので、
    /// プールは読み残しを受信せずに捨てる（PR #572のレビュー）
    async fn abandon(&self, conn: Conn, id: u32) {
        let killed = self.kill_query(id).await;
        let closed = tokio::time::timeout(DISCONNECT_TIMEOUT, conn.disconnect())
            .await
            .is_ok();
        if !killed && !closed {
            warn!(
                "Could not stop a {} statement (KILL QUERY {} did not work): the connection was dropped, and the statement may keep running on the database server until it finishes",
                self.server.name(),
                id
            );
        }
    }

    /// 別の接続から、`id`の接続で実行中のSQLを止める。止めたら`true`
    async fn kill_query(&self, id: u32) -> bool {
        let kill = async {
            let mut conn = Conn::new(self.opts.clone()).await?;
            conn.query_drop(format!("KILL QUERY {id}")).await?;
            conn.disconnect().await
        };
        match tokio::time::timeout(KILL_CONNECT_TIMEOUT, kill).await {
            Ok(Ok(())) => true,
            Ok(Err(e)) => {
                debug!("KILL QUERY {} failed: {}", id, e);
                false
            }
            Err(_) => {
                debug!("KILL QUERY {} timed out", id);
                false
            }
        }
    }
}

/// 列の型の名前（`get_schema`・`execute_query`の`type`）
fn type_name(column: &Column) -> String {
    let binary = column.character_set() == BINARY_CHARSET;
    let name = match column.column_type() {
        ColumnType::MYSQL_TYPE_TINY => "tinyint",
        ColumnType::MYSQL_TYPE_SHORT => "smallint",
        ColumnType::MYSQL_TYPE_INT24 => "mediumint",
        ColumnType::MYSQL_TYPE_LONG => "int",
        ColumnType::MYSQL_TYPE_LONGLONG => "bigint",
        ColumnType::MYSQL_TYPE_FLOAT => "float",
        ColumnType::MYSQL_TYPE_DOUBLE => "double",
        ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => "decimal",
        ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE => "date",
        ColumnType::MYSQL_TYPE_TIME | ColumnType::MYSQL_TYPE_TIME2 => "time",
        ColumnType::MYSQL_TYPE_DATETIME | ColumnType::MYSQL_TYPE_DATETIME2 => "datetime",
        ColumnType::MYSQL_TYPE_TIMESTAMP | ColumnType::MYSQL_TYPE_TIMESTAMP2 => "timestamp",
        ColumnType::MYSQL_TYPE_YEAR => "year",
        ColumnType::MYSQL_TYPE_BIT => "bit",
        ColumnType::MYSQL_TYPE_JSON => "json",
        ColumnType::MYSQL_TYPE_ENUM => "enum",
        ColumnType::MYSQL_TYPE_SET => "set",
        ColumnType::MYSQL_TYPE_GEOMETRY => "geometry",
        ColumnType::MYSQL_TYPE_STRING if binary => "binary",
        ColumnType::MYSQL_TYPE_STRING => "char",
        ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR if binary => "varbinary",
        ColumnType::MYSQL_TYPE_VAR_STRING | ColumnType::MYSQL_TYPE_VARCHAR => "varchar",
        ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB
        | ColumnType::MYSQL_TYPE_BLOB
            if binary =>
        {
            "blob"
        }
        ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB
        | ColumnType::MYSQL_TYPE_BLOB => "text",
        ColumnType::MYSQL_TYPE_NULL => "null",
        _ => "unknown",
    };
    let unsigned = column.flags().contains(ColumnFlags::UNSIGNED_FLAG)
        && matches!(
            column.column_type(),
            ColumnType::MYSQL_TYPE_TINY
                | ColumnType::MYSQL_TYPE_SHORT
                | ColumnType::MYSQL_TYPE_INT24
                | ColumnType::MYSQL_TYPE_LONG
                | ColumnType::MYSQL_TYPE_LONGLONG
        );
    if unsigned {
        format!("{name} unsigned")
    } else {
        name.to_string()
    }
}

fn column_infos(columns: &[Column]) -> Vec<ColumnInfo> {
    columns
        .iter()
        .map(|column| ColumnInfo {
            name: column.name_str().to_string(),
            data_type: type_name(column),
            nullable: !column.flags().contains(ColumnFlags::NOT_NULL_FLAG),
            max_length: None,
        })
        .collect()
}

/// 文字の列か（バイナリでない）。バイナリの列は`Value::Binary`にする
fn is_text_column(column: &Column) -> bool {
    match column.column_type() {
        ColumnType::MYSQL_TYPE_STRING
        | ColumnType::MYSQL_TYPE_VAR_STRING
        | ColumnType::MYSQL_TYPE_VARCHAR
        | ColumnType::MYSQL_TYPE_TINY_BLOB
        | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
        | ColumnType::MYSQL_TYPE_LONG_BLOB
        | ColumnType::MYSQL_TYPE_BLOB => column.character_set() != BINARY_CHARSET,
        ColumnType::MYSQL_TYPE_BIT | ColumnType::MYSQL_TYPE_GEOMETRY => false,
        // DECIMAL・ENUM・SET・JSON等は、文字として送られる
        _ => true,
    }
}

/// `COMMIT`を送った後に、応答を確かめられなかったときのエラー（#717）。
/// サーバーはコミットしているかもしれないので、「止めた」とは言わず、
/// やり直す前にデータを確かめるよう知らせる（やり直すと、行が重複し得る）
fn commit_unconfirmed(error: DatabaseError, server: ServerKind) -> DatabaseError {
    let check = "so the change may or may not have been saved. Check the data before running the statement again";
    match error {
        DatabaseError::TimeoutError(_) => DatabaseError::TimeoutError(format!(
            "the statement ran, but {} did not confirm the COMMIT in time, {check}",
            server.name()
        )),
        other => DatabaseError::OperationFailed(format!(
            "the statement ran, but the COMMIT did not complete ({other}), {check}"
        )),
    }
}

/// `mysql_async`の値を、列の型に合わせて変換する
fn convert_value(value: mysql_async::Value, column: &Column) -> Value {
    match value {
        mysql_async::Value::NULL => Value::Null,
        mysql_async::Value::Bytes(bytes) => {
            if column.column_type() == ColumnType::MYSQL_TYPE_JSON {
                if let Ok(json) = serde_json::from_slice(&bytes) {
                    return Value::Json(json);
                }
            }
            if !is_text_column(column) {
                return Value::Binary(bytes);
            }
            match String::from_utf8(bytes) {
                Ok(text) => Value::String(text),
                Err(e) => Value::Binary(e.into_bytes()),
            }
        }
        mysql_async::Value::Int(i) => Value::Int(i),
        // i64に入らない符号なしの値は、桁を落とさないよう文字列にする
        mysql_async::Value::UInt(u) => match i64::try_from(u) {
            Ok(i) => Value::Int(i),
            Err(_) => Value::String(u.to_string()),
        },
        mysql_async::Value::Float(f) => Value::Float(f64::from(f)),
        mysql_async::Value::Double(d) => Value::Float(d),
        mysql_async::Value::Date(year, month, day, hour, minute, second, micro) => {
            let date = format!("{year:04}-{month:02}-{day:02}");
            let date_only = matches!(
                column.column_type(),
                ColumnType::MYSQL_TYPE_DATE | ColumnType::MYSQL_TYPE_NEWDATE
            );
            if date_only {
                Value::String(date)
            } else if micro == 0 {
                Value::String(format!("{date} {hour:02}:{minute:02}:{second:02}"))
            } else {
                Value::String(format!(
                    "{date} {hour:02}:{minute:02}:{second:02}.{micro:06}"
                ))
            }
        }
        mysql_async::Value::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if negative { "-" } else { "" };
            let hours = days * 24 + u32::from(hours);
            if micros == 0 {
                Value::String(format!("{sign}{hours:02}:{minutes:02}:{seconds:02}"))
            } else {
                Value::String(format!(
                    "{sign}{hours:02}:{minutes:02}:{seconds:02}.{micros:06}"
                ))
            }
        }
    }
}

/// 値を、`?`にバインドする値にする
fn convert_params(params: &[Value]) -> Vec<mysql_async::Value> {
    params
        .iter()
        .map(|value| match value {
            Value::Null => mysql_async::Value::NULL,
            Value::Bool(b) => mysql_async::Value::Int(i64::from(*b)),
            Value::Int(i) => mysql_async::Value::Int(*i),
            Value::Float(f) => mysql_async::Value::Double(*f),
            Value::String(s) => mysql_async::Value::Bytes(s.as_bytes().to_vec()),
            Value::Binary(b) => mysql_async::Value::Bytes(b.clone()),
            Value::Json(json) => mysql_async::Value::Bytes(json.to_string().into_bytes()),
            // MySQLのDATETIMEは時差を持たないので、UTCの時刻として渡す
            Value::DateTime(at) => mysql_async::Value::Bytes(
                at.naive_utc()
                    .format("%Y-%m-%d %H:%M:%S%.f")
                    .to_string()
                    .into_bytes(),
            ),
        })
        .collect()
}

/// 行数・バイト数の上限まで、行を受け取る。上限に達したら`Stopped`
async fn read_rows(
    conn: &mut Conn,
    sql: &str,
    params: Vec<mysql_async::Value>,
    fetch_limit: Option<usize>,
    byte_limit: Option<usize>,
) -> Result<Outcome<(Vec<ColumnInfo>, Vec<Vec<Value>>)>, mysql_async::Error> {
    let mut result = conn.exec_iter(sql, params).await?;
    let columns: Arc<[Column]> = result.columns().unwrap_or_else(|| Arc::from(Vec::new()));
    let infos = column_infos(&columns);
    let mut rows = Vec::new();
    let mut bytes = 0usize;
    while let Some(row) = result.next().await? {
        let values: Vec<Value> = row
            .unwrap()
            .into_iter()
            .zip(columns.iter())
            .map(|(value, column)| convert_value(value, column))
            .collect();
        bytes += values.iter().map(Value::approx_json_size).sum::<usize>();
        rows.push(values);
        let rows_full = fetch_limit.is_some_and(|limit| rows.len() >= limit);
        let bytes_full = byte_limit.is_some_and(|limit| bytes > limit);
        if rows_full || bytes_full {
            return Ok(Outcome::Stopped((infos, rows)));
        }
    }
    Ok(Outcome::Done((infos, rows)))
}

/// 読み取り専用のトランザクションを始め、SELECTの時間の上限を付ける
async fn begin_read_only(
    conn: &mut Conn,
    timeout_ms: u128,
    server: ServerKind,
) -> Result<(), mysql_async::Error> {
    conn.query_drop(server.statement_timeout_sql(timeout_ms))
        .await?;
    conn.query_drop("START TRANSACTION READ ONLY").await
}

/// `KEY_COLUMN_USAGE`の1行（表・制約・列・参照する表・参照する列）
type KeyColumnRow = (String, String, String, Option<String>, Option<String>);

/// 今のデータベースの表・列・主キー・外部キー・インデックス・ビュー
async fn read_schema(
    conn: &mut Conn,
    only_table: Option<&str>,
) -> Result<(Vec<TableInfo>, Vec<ViewInfo>), mysql_async::Error> {
    let filter = if only_table.is_some() {
        " AND TABLE_NAME = ?"
    } else {
        ""
    };
    let table_param = || -> Vec<mysql_async::Value> {
        only_table
            .map(|t| vec![mysql_async::Value::Bytes(t.as_bytes().to_vec())])
            .unwrap_or_default()
    };

    let tables: Vec<(String, String)> = conn
        .exec(
            format!(
                "SELECT TABLE_NAME, TABLE_TYPE FROM information_schema.TABLES \
                 WHERE TABLE_SCHEMA = DATABASE(){filter} ORDER BY TABLE_NAME"
            ),
            table_param(),
        )
        .await?;

    let mut columns: BTreeMap<String, Vec<ColumnInfo>> = BTreeMap::new();
    let rows: Vec<(String, String, String, String, Option<u64>)> = conn
        .exec(
            format!(
                "SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, \
                 CHARACTER_MAXIMUM_LENGTH FROM information_schema.COLUMNS \
                 WHERE TABLE_SCHEMA = DATABASE(){filter} ORDER BY TABLE_NAME, ORDINAL_POSITION"
            ),
            table_param(),
        )
        .await?;
    for (table, name, data_type, nullable, max_length) in rows {
        columns.entry(table).or_default().push(ColumnInfo {
            name,
            data_type,
            nullable: nullable == "YES",
            max_length: max_length.and_then(|n| i32::try_from(n).ok()),
        });
    }

    let mut primary_keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut foreign_keys: BTreeMap<String, Vec<ForeignKeyInfo>> = BTreeMap::new();
    let rows: Vec<KeyColumnRow> = conn
        .exec(
            format!(
                "SELECT TABLE_NAME, CONSTRAINT_NAME, COLUMN_NAME, REFERENCED_TABLE_NAME, \
                 REFERENCED_COLUMN_NAME FROM information_schema.KEY_COLUMN_USAGE \
                 WHERE TABLE_SCHEMA = DATABASE(){filter} \
                 ORDER BY TABLE_NAME, CONSTRAINT_NAME, ORDINAL_POSITION"
            ),
            table_param(),
        )
        .await?;
    for (table, constraint, column, referenced_table, referenced_column) in rows {
        if constraint == "PRIMARY" {
            primary_keys.entry(table).or_default().push(column);
        } else if let (Some(referenced_table), Some(referenced_column)) =
            (referenced_table, referenced_column)
        {
            foreign_keys.entry(table).or_default().push(ForeignKeyInfo {
                name: constraint,
                column,
                referenced_table,
                referenced_column,
            });
        }
    }

    let mut indexes: BTreeMap<String, Vec<IndexInfo>> = BTreeMap::new();
    let rows: Vec<(String, String, String, i64)> = conn
        .exec(
            format!(
                "SELECT TABLE_NAME, INDEX_NAME, COLUMN_NAME, NON_UNIQUE \
                 FROM information_schema.STATISTICS \
                 WHERE TABLE_SCHEMA = DATABASE(){filter} AND COLUMN_NAME IS NOT NULL \
                 ORDER BY TABLE_NAME, INDEX_NAME, SEQ_IN_INDEX"
            ),
            table_param(),
        )
        .await?;
    for (table, index, column, non_unique) in rows {
        let list = indexes.entry(table).or_default();
        match list.iter_mut().find(|i| i.name == index) {
            Some(existing) => existing.columns.push(column),
            None => list.push(IndexInfo {
                is_primary: index == "PRIMARY",
                is_unique: non_unique == 0,
                name: index,
                columns: vec![column],
            }),
        }
    }

    let mut table_infos = Vec::new();
    let mut view_infos = Vec::new();
    for (name, kind) in tables {
        let table_columns = columns.remove(&name).unwrap_or_default();
        if kind == "VIEW" {
            view_infos.push(ViewInfo {
                name,
                schema: None,
                definition: String::new(),
                columns: table_columns,
            });
            continue;
        }
        table_infos.push(TableInfo {
            primary_keys: primary_keys.remove(&name).unwrap_or_default(),
            foreign_keys: foreign_keys.remove(&name).unwrap_or_default(),
            indexes: indexes.remove(&name).unwrap_or_default(),
            columns: table_columns,
            schema: None,
            name,
        });
    }
    Ok((table_infos, view_infos))
}

#[async_trait]
impl DatabaseConnection for MySqlConnection {
    /// 読み取り専用のトランザクションの中で実行し、ロールバックする。行数・
    /// バイト数の上限で止める
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let started = std::time::Instant::now();
        let conn = self.get_conn().await?;
        let timeout_ms = self.statement_timeout_ms();
        let server = self.server;
        let params = convert_params(params);
        let (fetch_limit, byte_limit) = (self.fetch_limit, self.byte_limit);
        let sql = sql.to_string();
        let (columns, rows) = self
            .run(
                conn,
                move |conn| {
                    Box::pin(async move {
                        begin_read_only(conn, timeout_ms, server).await?;
                        let outcome =
                            read_rows(conn, &sql, params, fetch_limit, byte_limit).await?;
                        if let Outcome::Done(_) = &outcome {
                            conn.query_drop("ROLLBACK").await?;
                        }
                        Ok(outcome)
                    })
                },
                DatabaseError::QueryFailed,
            )
            .await?;
        Ok(QueryResult {
            columns,
            total_rows: Some(rows.len() as u64),
            rows,
            execution_time_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// 1つの文を、トランザクションの中で実行してコミットする
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let started = std::time::Instant::now();
        let conn = self.get_conn().await?;
        let params = convert_params(params);
        let sql = sql.to_string();
        // `COMMIT`を送ったか（#717）。送った後に応答を確かめられなければ、
        // サーバーはコミットしているかもしれないので、「止めた」とは返さない
        let commit_sent = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sent = commit_sent.clone();
        let result = self
            .run(
                conn,
                move |conn| {
                    Box::pin(async move {
                        conn.query_drop("START TRANSACTION").await?;
                        conn.exec_drop(sql, params).await?;
                        let affected = conn.affected_rows();
                        let last_insert_id = conn.last_insert_id();
                        sent.store(true, std::sync::atomic::Ordering::SeqCst);
                        conn.query_drop("COMMIT").await?;
                        Ok(Outcome::Done((affected, last_insert_id)))
                    })
                },
                DatabaseError::OperationFailed,
            )
            .await;
        let (rows_affected, last_insert_id) = match result {
            Ok(value) => value,
            Err(e) if commit_sent.load(std::sync::atomic::Ordering::SeqCst) => {
                return Err(commit_unconfirmed(e, self.server))
            }
            Err(e) => return Err(e),
        };
        Ok(ExecuteResult {
            rows_affected,
            last_insert_id: last_insert_id
                .filter(|id| *id > 0)
                .map(|id| match i64::try_from(id) {
                    Ok(id) => Value::Int(id),
                    Err(_) => Value::String(id.to_string()),
                }),
            execution_time_ms: started.elapsed().as_millis() as u64,
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

    /// 読み取り専用のトランザクションと時間の上限の中で読む
    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let conn = self.get_conn().await?;
        let timeout_ms = self.statement_timeout_ms();
        let server = self.server;
        let (tables, views) = self
            .run(
                conn,
                move |conn| {
                    Box::pin(async move {
                        begin_read_only(conn, timeout_ms, server).await?;
                        let schema = read_schema(conn, None).await?;
                        conn.query_drop("ROLLBACK").await?;
                        Ok(Outcome::Done(schema))
                    })
                },
                DatabaseError::QueryFailed,
            )
            .await?;
        Ok(DatabaseSchema {
            database_name: self.database_name.clone(),
            tables,
            views,
            procedures: vec![],
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        let conn = self.get_conn().await?;
        let timeout_ms = self.statement_timeout_ms();
        let server = self.server;
        let table = table_name.to_string();
        let (tables, _) = self
            .run(
                conn,
                move |conn| {
                    Box::pin(async move {
                        begin_read_only(conn, timeout_ms, server).await?;
                        let schema = read_schema(conn, Some(&table)).await?;
                        conn.query_drop("ROLLBACK").await?;
                        Ok(Outcome::Done(schema))
                    })
                },
                DatabaseError::QueryFailed,
            )
            .await?;
        tables
            .into_iter()
            .next()
            .ok_or_else(|| DatabaseError::QueryFailed(format!("table not found: {table_name}")))
    }

    async fn prepare(&self, _sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "Prepared statements are not available through this connection; pass params to query() or execute() instead".to_string(),
        ))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        let mut conn = self.get_conn().await?;
        conn.ping()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Ping failed: {}", e)))
    }

    /// プールは`MySqlEngine`が持ち、ほかの接続と共有するので、ここでは閉じない
    async fn close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        use chrono::Utc;
        ConnectionInfo {
            connection_id: "mysql".to_string(),
            database_name: self.database_name.clone(),
            user_name: self.user_name.clone(),
            server_version: String::new(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// utf8mb4_general_ci
    const UTF8MB4: u16 = 45;

    /// `COMMIT`の応答を確かめられなければ、「止めた」とは言わず、保存されたか
    /// 分からないと返す（#717）
    #[test]
    fn test_commit_unconfirmed() {
        let timeout = commit_unconfirmed(
            DatabaseError::TimeoutError("stopped".to_string()),
            ServerKind::MariaDb,
        );
        let DatabaseError::TimeoutError(message) = timeout else {
            panic!("{timeout:?}")
        };
        assert!(
            message.contains("MariaDB did not confirm the COMMIT"),
            "{message}"
        );
        assert!(
            message.contains("may or may not have been saved"),
            "{message}"
        );
        assert!(!message.contains("stopped"), "{message}");
        let lost = commit_unconfirmed(
            DatabaseError::OperationFailed("connection reset".to_string()),
            ServerKind::MySql,
        );
        let DatabaseError::OperationFailed(message) = lost else {
            panic!("{lost:?}")
        };
        assert!(message.contains("connection reset"), "{message}");
        assert!(
            message.contains("may or may not have been saved"),
            "{message}"
        );
    }

    #[test]
    fn server_kind_is_decided_by_the_version() {
        assert_eq!(ServerKind::from_version("8.4.3"), ServerKind::MySql);
        assert_eq!(
            ServerKind::from_version("11.4.2-MariaDB-ubu2404"),
            ServerKind::MariaDb
        );
        assert_eq!(
            ServerKind::from_version("5.5.5-10.11.6-MariaDB-log"),
            ServerKind::MariaDb
        );
    }

    #[test]
    fn statement_timeout_uses_the_variable_of_the_server() {
        assert_eq!(
            ServerKind::MySql.statement_timeout_sql(30_000),
            "SET SESSION MAX_EXECUTION_TIME = 30000"
        );
        assert_eq!(
            ServerKind::MariaDb.statement_timeout_sql(30_000),
            "SET SESSION max_statement_time = 30.000"
        );
        assert_eq!(
            ServerKind::MariaDb.statement_timeout_sql(1_500),
            "SET SESSION max_statement_time = 1.500"
        );
        // 0は、どちらも上限なし
        assert_eq!(
            ServerKind::MariaDb.statement_timeout_sql(0),
            "SET SESSION max_statement_time = 0.000"
        );
    }

    /// `Value`は`PartialEq`を持たないので、`Debug`の表示で比べる
    fn show(value: Value) -> String {
        format!("{value:?}")
    }

    fn column(column_type: ColumnType, charset: u16, flags: ColumnFlags) -> Column {
        Column::new(column_type)
            .with_character_set(charset)
            .with_flags(flags)
    }

    #[test]
    fn test_convert_value_follows_the_column_type() {
        let text = column(
            ColumnType::MYSQL_TYPE_VAR_STRING,
            UTF8MB4,
            ColumnFlags::empty(),
        );
        let binary = column(
            ColumnType::MYSQL_TYPE_VAR_STRING,
            BINARY_CHARSET,
            ColumnFlags::empty(),
        );
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Bytes(b"abc".to_vec()),
                &text
            )),
            show(Value::String("abc".to_string()))
        );
        // バイナリの列は、UTF-8として読めても文字列にしない
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Bytes(b"abc".to_vec()),
                &binary
            )),
            show(Value::Binary(b"abc".to_vec()))
        );
        // DECIMALは文字として送られ、桁を落とさない
        let decimal = column(
            ColumnType::MYSQL_TYPE_NEWDECIMAL,
            BINARY_CHARSET,
            ColumnFlags::empty(),
        );
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Bytes(b"12345678901234567890.123".to_vec()),
                &decimal
            )),
            show(Value::String("12345678901234567890.123".to_string()))
        );
        let json = column(
            ColumnType::MYSQL_TYPE_JSON,
            BINARY_CHARSET,
            ColumnFlags::empty(),
        );
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Bytes(br#"{"a":1}"#.to_vec()),
                &json
            )),
            show(Value::Json(serde_json::json!({"a": 1})))
        );
        // i64に入らない符号なしの値は、文字列
        let big = column(
            ColumnType::MYSQL_TYPE_LONGLONG,
            BINARY_CHARSET,
            ColumnFlags::UNSIGNED_FLAG,
        );
        assert_eq!(
            show(convert_value(mysql_async::Value::UInt(u64::MAX), &big)),
            show(Value::String(u64::MAX.to_string()))
        );
        assert_eq!(
            show(convert_value(mysql_async::Value::UInt(7), &big)),
            show(Value::Int(7))
        );
    }

    #[test]
    fn test_dates_and_times() {
        let date = column(
            ColumnType::MYSQL_TYPE_DATE,
            BINARY_CHARSET,
            ColumnFlags::empty(),
        );
        let datetime = column(
            ColumnType::MYSQL_TYPE_DATETIME,
            BINARY_CHARSET,
            ColumnFlags::empty(),
        );
        let time = column(
            ColumnType::MYSQL_TYPE_TIME,
            BINARY_CHARSET,
            ColumnFlags::empty(),
        );
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Date(2026, 11, 16, 0, 0, 0, 0),
                &date
            )),
            show(Value::String("2026-11-16".to_string()))
        );
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Date(2026, 11, 16, 9, 5, 7, 0),
                &datetime
            )),
            show(Value::String("2026-11-16 09:05:07".to_string()))
        );
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Date(2026, 11, 16, 9, 5, 7, 120),
                &datetime
            )),
            show(Value::String("2026-11-16 09:05:07.000120".to_string()))
        );
        // 以前は秒とマイクロ秒の間が`:`だった
        assert_eq!(
            show(convert_value(
                mysql_async::Value::Time(true, 1, 2, 3, 4, 5),
                &time
            )),
            show(Value::String("-26:03:04.000005".to_string()))
        );
    }

    #[test]
    fn test_type_name() {
        assert_eq!(
            type_name(&column(
                ColumnType::MYSQL_TYPE_LONG,
                BINARY_CHARSET,
                ColumnFlags::UNSIGNED_FLAG
            )),
            "int unsigned"
        );
        assert_eq!(
            type_name(&column(
                ColumnType::MYSQL_TYPE_BLOB,
                UTF8MB4,
                ColumnFlags::empty()
            )),
            "text"
        );
        assert_eq!(
            type_name(&column(
                ColumnType::MYSQL_TYPE_BLOB,
                BINARY_CHARSET,
                ColumnFlags::empty()
            )),
            "blob"
        );
    }

    #[test]
    fn test_convert_params() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-11-16T09:05:07Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(
            convert_params(&[
                Value::Null,
                Value::Bool(true),
                Value::Json(serde_json::json!([1])),
                Value::DateTime(at),
            ]),
            vec![
                mysql_async::Value::NULL,
                mysql_async::Value::Int(1),
                mysql_async::Value::Bytes(b"[1]".to_vec()),
                mysql_async::Value::Bytes(b"2026-11-16 09:05:07".to_vec()),
            ]
        );
    }
}
