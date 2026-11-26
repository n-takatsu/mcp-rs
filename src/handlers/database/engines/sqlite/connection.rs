//! SQLiteの接続（`[database]`の`engine = "sqlite"`、#528）
//!
//! PostgreSQL（`pg_connection`）・MySQL（`mysql::connection`）と同じ安全性に
//! そろえる:
//!
//! - 読み取り（`query()`・スキーマ）は、ファイルを読み取り専用
//!   （`SQLITE_OPEN_READONLY`）で開いた接続で行う。さらに、トランザクションの
//!   中で読み、最後にロールバックする
//! - 書き込みの接続は、`OPTION_READ_ONLY`が無いとき（`[database]`の
//!   `allow_write = true`）だけ作る
//! - ファイルが無ければ作らない（パスの書き間違いで、空のデータベースを
//!   作って読まない）
//! - 1つのSQLの時間の上限。SQLiteにはサーバーが無いので、SQLiteの
//!   進み具合のコールバック（`sqlite3_progress_handler`）で、時間を過ぎたら
//!   止める。ロックの待ちは`busy_timeout`で止める
//! - 受け取る行数・結果のだいたいのバイト数の上限。上限に達したら、残りの
//!   行は読まない
//! - パラメーターは文字列に埋め込まず、`?`にバインドする
//!
//! `:memory:`（ライブラリ・テストで使う）は、接続ごとに別のデータベースに
//! なるので、1つの接続を読み書きに共有する（読み取り専用では開かない）。
//! `[database]`からは使えない（`DatabaseSettings::validate`）
//!
//! 以前の実装（#528の前）は、接続のたびにプールを作り、全部の行をメモリに
//! 読み、時間の上限も無く、`PRAGMA table_info`に表の名前をそのまま
//! 埋め込んでいた。

use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection, DatabaseTransaction, PreparedStatement},
    engines::{OPTION_BYTE_LIMIT, OPTION_FETCH_LIMIT, OPTION_READ_ONLY},
    types::{
        ColumnInfo, DatabaseConfig, DatabaseError, DatabaseSchema, ExecuteResult, ForeignKeyInfo,
        IndexInfo, QueryResult, TableInfo, Value, ViewInfo,
    },
};
use async_trait::async_trait;
use chrono::Utc;
use futures::TryStreamExt;
use sqlx::sqlite::{
    SqliteArguments, SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow,
};
use sqlx::{Column, Connection, Executor, Row, Statement, TypeInfo, ValueRef};
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{SqlitePreparedStatement, SqliteTransaction};

/// SQLiteの仮想機械の命令をこれだけ進めるたびに、時間を確かめる
const PROGRESS_STEPS: i32 = 1000;

/// 1つのSQLの時間の上限の既定（秒）
const DEFAULT_STATEMENT_TIMEOUT_SECS: u64 = 30;

/// 接続を受け取るときの待ち時間の上限の既定（秒）
const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// SQLiteの、読み取りと書き込みの接続のプール（#528）。エンジンが1つ持ち、
/// 接続の間で共有する
pub(crate) struct SqlitePools {
    /// 読み取りの接続。ファイルは読み取り専用で開く（`:memory:`は`write`と同じ）
    read: SqlitePool,
    /// 書き込みの接続。`OPTION_READ_ONLY`のときは無い
    write: Option<SqlitePool>,
    /// 開いたときの設定（`pools_key`）
    pub(crate) key: PoolsKey,
}

/// `:memory:`か
fn is_memory(path: &str) -> bool {
    path == ":memory:"
}

/// 設定から、SQLiteのプールを開く（#528）。ファイルは作らない
pub(crate) async fn open_pools(config: &DatabaseConfig) -> Result<Arc<SqlitePools>, DatabaseError> {
    let connection = &config.connection;
    let path = connection.host.trim();
    if path.is_empty() {
        return Err(DatabaseError::ConfigurationError(
            "Database path is required".to_string(),
        ));
    }
    let read_only = connection
        .options
        .get(OPTION_READ_ONLY)
        .is_some_and(|v| v == "true");
    // sqlxは、ミリ秒が`i32`に収まらない`busy_timeout`でパニックする（PR #798の
    // レビュー）。収まる長さ（約24日）までにする
    let busy = statement_timeout(config).min(Duration::from_millis(i32::MAX as u64));
    let acquire = Duration::from_secs(if connection.timeout_seconds > 0 {
        u64::from(connection.timeout_seconds)
    } else {
        DEFAULT_CONNECT_TIMEOUT_SECS
    });
    // プールに戻した接続の時間の上限は、いつも外す（PR #798のレビュー）。途中で
    // 取り消された呼び出しは`disarm_deadline`まで進まないので、残った上限で、
    // 後の呼び出し（準備済みの文・トランザクション）が止まらないように
    let pool_options = |max: u32| {
        SqlitePoolOptions::new()
            .max_connections(max.max(1))
            .min_connections(0)
            .acquire_timeout(acquire)
            .after_release(|conn, _| {
                Box::pin(async move {
                    disarm_deadline(conn).await;
                    Ok(true)
                })
            })
    };
    let open_error = |e: sqlx::Error| {
        DatabaseError::ConnectionFailed(format!("cannot open the SQLite database {path}: {e}"))
    };

    if is_memory(path) {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(open_error)?
            .busy_timeout(busy);
        // 接続ごとに別のデータベースになるので、1つだけにする
        let pool = pool_options(1)
            .connect_with(options)
            .await
            .map_err(open_error)?;
        return Ok(Arc::new(SqlitePools {
            write: (!read_only).then(|| pool.clone()),
            read: pool,
            key: pools_key(config),
        }));
    }

    if !std::path::Path::new(path).is_file() {
        return Err(DatabaseError::ConnectionFailed(format!(
            "the SQLite database file {path} was not found (mcp-rs does not create it)"
        )));
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .busy_timeout(busy);
    let max = config.pool.max_connections;
    let read = pool_options(max)
        .connect_with(options.clone().read_only(true))
        .await
        .map_err(open_error)?;
    let write = if read_only {
        None
    } else {
        Some(
            pool_options(max)
                .connect_with(options)
                .await
                .map_err(open_error)?,
        )
    };
    let pools = SqlitePools {
        read,
        write,
        key: pools_key(config),
    };
    // SQLiteのファイルかを、中身を読んで確かめる（PR #798のレビュー。開くだけ
    // では、ほかのファイルでも通る）
    let mut conn = pools.read.acquire().await.map_err(open_error)?;
    sqlx::query("SELECT count(*) FROM sqlite_schema")
        .fetch_one(&mut *conn)
        .await
        .map_err(open_error)?;
    drop(conn);
    Ok(Arc::new(pools))
}

/// プールの開き方を決める設定（ファイル・書き込みを許すか・接続を待つ時間・
/// ロックを待つ時間）。エンジンは、同じ設定の接続にだけプールを共有する
/// （PR #798のレビュー）
pub(crate) type PoolsKey = (String, bool, u32, u32);

pub(crate) fn pools_key(config: &DatabaseConfig) -> PoolsKey {
    (
        config.connection.host.trim().to_string(),
        config
            .connection
            .options
            .get(OPTION_READ_ONLY)
            .is_some_and(|v| v == "true"),
        config.connection.timeout_seconds,
        config.features.query_timeout,
    )
}

/// 1つのSQLの時間の上限
fn statement_timeout(config: &DatabaseConfig) -> Duration {
    Duration::from_secs(if config.features.query_timeout > 0 {
        u64::from(config.features.query_timeout)
    } else {
        DEFAULT_STATEMENT_TIMEOUT_SECS
    })
}

/// 接続に時間の上限を付ける。時間を過ぎると、SQLiteが実行を止める
/// （`SQLITE_INTERRUPT`）。止めたかは、返す印で分かる。前の呼び出しの
/// コールバックは置き換わる
async fn arm_deadline(
    conn: &mut sqlx::SqliteConnection,
    timeout: Duration,
) -> Result<Arc<AtomicBool>, sqlx::Error> {
    let deadline = Instant::now() + timeout;
    let fired = Arc::new(AtomicBool::new(false));
    let flag = fired.clone();
    conn.lock_handle()
        .await?
        .set_progress_handler(PROGRESS_STEPS, move || {
            if Instant::now() >= deadline {
                flag.store(true, Ordering::SeqCst);
                false
            } else {
                true
            }
        });
    Ok(fired)
}

/// 時間の上限を外す（プールに戻した接続が、後で止まらないように）
async fn disarm_deadline(conn: &mut sqlx::SqliteConnection) {
    if let Ok(mut handle) = conn.lock_handle().await {
        handle.remove_progress_handler();
    }
}

/// sqlxのエラーを、時間の上限で止めたかを見て、mcp-rsのエラーにする
fn map_error(
    e: sqlx::Error,
    fired: &AtomicBool,
    timeout: Duration,
    kind: fn(String) -> DatabaseError,
) -> DatabaseError {
    if fired.load(Ordering::SeqCst) {
        return DatabaseError::TimeoutError(format!(
            "the SQL did not finish within {} seconds and was stopped",
            timeout.as_secs()
        ));
    }
    kind(e.to_string())
}

/// 値を、`?`にバインドする
fn bind_params<'q>(
    mut query: sqlx::query::Query<'q, sqlx::Sqlite, SqliteArguments<'q>>,
    params: &'q [Value],
) -> sqlx::query::Query<'q, sqlx::Sqlite, SqliteArguments<'q>> {
    for param in params {
        query = match param {
            Value::Null => query.bind(None::<i64>),
            Value::Bool(b) => query.bind(i64::from(*b)),
            Value::Int(i) => query.bind(*i),
            Value::Float(f) => query.bind(*f),
            Value::String(s) => query.bind(s.as_str()),
            Value::Binary(b) => query.bind(b.as_slice()),
            Value::DateTime(dt) => query.bind(dt.to_rfc3339()),
            Value::Json(j) => query.bind(j.to_string()),
        };
    }
    query
}

/// 列の情報（型は、表で宣言した型。式の列は空）
fn column_infos(columns: &[sqlx::sqlite::SqliteColumn]) -> Vec<ColumnInfo> {
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

/// 1つの値。SQLiteは値ごとに型を持つので、宣言した型ではなく、値の型で読む
fn read_value(row: &SqliteRow, index: usize) -> Result<Value, sqlx::Error> {
    let raw = row.try_get_raw(index)?;
    if raw.is_null() {
        return Ok(Value::Null);
    }
    let kind = raw.type_info().name().to_string();
    Ok(match kind.as_str() {
        "INTEGER" => Value::Int(row.try_get_unchecked::<i64, _>(index)?),
        "REAL" => Value::Float(row.try_get_unchecked::<f64, _>(index)?),
        "BLOB" => Value::Binary(row.try_get_unchecked::<Vec<u8>, _>(index)?),
        _ => Value::String(row.try_get_unchecked::<String, _>(index)?),
    })
}

/// 読み取りの結果: 上限まで読み終えたか、上限で止めたか
type Rows = (Vec<ColumnInfo>, Vec<Vec<Value>>);

/// トランザクションの中で、行数・バイト数の上限まで読み、ロールバックする
async fn read_rows(
    conn: &mut sqlx::SqliteConnection,
    sql: &str,
    params: &[Value],
    fetch_limit: Option<usize>,
    byte_limit: Option<usize>,
) -> Result<Rows, sqlx::Error> {
    let mut tx = conn.begin().await?;
    let columns = column_infos(tx.prepare(sql).await?.columns());
    let mut rows = Vec::new();
    {
        let mut stream = bind_params(sqlx::query(sql), params).fetch(&mut *tx);
        let mut bytes = 0usize;
        while let Some(row) = stream.try_next().await? {
            let values = (0..row.len())
                .map(|index| read_value(&row, index))
                .collect::<Result<Vec<_>, _>>()?;
            bytes += values.iter().map(Value::approx_json_size).sum::<usize>();
            rows.push(values);
            let rows_full = fetch_limit.is_some_and(|limit| rows.len() >= limit);
            let bytes_full = byte_limit.is_some_and(|limit| bytes > limit);
            if rows_full || bytes_full {
                break;
            }
        }
    }
    tx.rollback().await?;
    Ok((columns, rows))
}

/// 名前で絞った、表・ビューの一覧（名前・種類・定義のSQL）
type SchemaObject = (String, String, Option<String>);

/// 表・ビューの、列・主キー・外部キー・インデックス。`only`があれば、その
/// 名前のものだけ。SQLite自身の表（`sqlite_`で始まる）は除く。表の名前は
/// SQLに埋め込まず、`pragma_table_info(?)`等にバインドする
async fn read_schema(
    conn: &mut sqlx::SqliteConnection,
    only: Option<&str>,
) -> Result<(Vec<TableInfo>, Vec<ViewInfo>), sqlx::Error> {
    let mut tx = conn.begin().await?;
    // SQLiteは、名前を大文字と小文字を区別せずに比べる（PR #798のレビュー）
    let filter = if only.is_some() {
        " AND name = ? COLLATE NOCASE"
    } else {
        ""
    };
    let list_sql = format!(
        "SELECT name, type, sql FROM sqlite_schema \
         WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'{filter} \
         ORDER BY name"
    );
    let mut objects = sqlx::query_as::<_, SchemaObject>(&list_sql);
    if let Some(name) = only {
        objects = objects.bind(name);
    }
    let objects = objects.fetch_all(&mut *tx).await?;

    let mut tables = Vec::new();
    let mut views = Vec::new();
    for (name, kind, sql) in objects {
        // 列（名前・型・NOT NULL・主キーの順番）。生成列も含める
        // （`table_xinfo`。PR #798のレビュー）。仮想表の隠れた列（hidden = 1）は除く
        let rows: Vec<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT name, type, \"notnull\", pk FROM pragma_table_xinfo(?) \
             WHERE hidden <> 1 ORDER BY cid",
        )
        .bind(&name)
        .fetch_all(&mut *tx)
        .await?;
        // `WITHOUT ROWID`の表は、主キーの列にNULLを入れられない
        let without_rowid: bool = kind == "table"
            && sqlx::query_scalar::<_, i64>("SELECT wr FROM pragma_table_list(?)")
                .bind(&name)
                .fetch_optional(&mut *tx)
                .await?
                .is_some_and(|wr| wr != 0);
        // 主キーが`INTEGER`の1列だけなら、その列はrowidの別名で、NULLを入れられ
        // ない。複合の主キーの列は、`NOT NULL`と書かなければNULLを入れられる
        // （SQLiteの古い動き。PR #798のレビュー）
        let primary_count = rows.iter().filter(|(_, _, _, pk)| *pk > 0).count();
        // rowidの別名の主キーには、主キーのための索引が無い。
        // `INTEGER PRIMARY KEY DESC`は別名ではなく、索引がある（PR #798の
        // レビュー）
        let primary_index: i64 = if kind == "table" {
            sqlx::query_scalar("SELECT count(*) FROM pragma_index_list(?) WHERE origin = 'pk'")
                .bind(&name)
                .fetch_one(&mut *tx)
                .await?
        } else {
            0
        };
        let mut primary: BTreeMap<i64, String> = BTreeMap::new();
        let columns = rows
            .into_iter()
            .map(|(column, data_type, not_null, pk)| {
                if pk > 0 {
                    primary.insert(pk, column.clone());
                }
                let rowid_alias = pk > 0
                    && primary_count == 1
                    && primary_index == 0
                    && !without_rowid
                    && data_type.eq_ignore_ascii_case("INTEGER");
                ColumnInfo {
                    nullable: not_null == 0 && !(pk > 0 && (rowid_alias || without_rowid)),
                    name: column,
                    data_type,
                    max_length: None,
                }
            })
            .collect();
        if kind == "view" {
            views.push(ViewInfo {
                name,
                schema: None,
                definition: sql.unwrap_or_default(),
                columns,
            });
            continue;
        }

        // 外部キー（id・seq・参照する表・列・参照する列）
        let rows: Vec<(i64, i64, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, seq, \"table\", \"from\", \"to\" FROM pragma_foreign_key_list(?) \
             ORDER BY id, seq",
        )
        .bind(&name)
        .fetch_all(&mut *tx)
        .await?;
        let mut foreign_keys = Vec::new();
        for (id, seq, referenced_table, column, referenced_column) in rows {
            // 参照する列を書かなければ、参照する表の主キーの、同じ順番の列
            // （PR #798のレビュー）
            let referenced_column = match referenced_column {
                Some(column) => column,
                None => sqlx::query_scalar::<_, String>(
                    "SELECT name FROM pragma_table_info(?) WHERE pk > 0 ORDER BY pk \
                     LIMIT 1 OFFSET ?",
                )
                .bind(&referenced_table)
                .bind(seq)
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or_default(),
            };
            foreign_keys.push(ForeignKeyInfo {
                name: format!("fk_{name}_{id}"),
                column,
                referenced_table,
                referenced_column,
            });
        }

        // インデックス。式を含むインデックス（列の名前が無い）は、全体を除く
        // （MySQLと同じ。#717）
        let list: Vec<(String, i64, String)> =
            sqlx::query_as("SELECT name, \"unique\", origin FROM pragma_index_list(?)")
                .bind(&name)
                .fetch_all(&mut *tx)
                .await?;
        let mut indexes = Vec::new();
        for (index, unique, origin) in list {
            let columns: Vec<(Option<String>,)> =
                sqlx::query_as("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
                    .bind(&index)
                    .fetch_all(&mut *tx)
                    .await?;
            let Some(columns) = columns
                .into_iter()
                .map(|(column,)| column)
                .collect::<Option<Vec<String>>>()
            else {
                continue;
            };
            indexes.push(IndexInfo {
                name: index,
                columns,
                is_unique: unique != 0,
                is_primary: origin == "pk",
            });
        }

        tables.push(TableInfo {
            name,
            schema: None,
            columns,
            primary_keys: primary.into_values().collect(),
            foreign_keys,
            indexes,
        });
    }
    tx.rollback().await?;
    Ok((tables, views))
}

/// SQLが、1つのINSERT（REPLACEも）か
fn is_insert(sql: &str) -> bool {
    matches!(
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::SQLiteDialect {}, sql).as_deref(),
        Ok([sqlparser::ast::Statement::Insert(_)])
    )
}

/// プールから受け取った接続
type PooledSqlite = sqlx::pool::PoolConnection<sqlx::Sqlite>;

/// `pool`から接続を受け取り、時間の上限を付ける
async fn acquire_with_deadline(
    pool: &SqlitePool,
    timeout: Duration,
) -> Result<(PooledSqlite, Arc<AtomicBool>), DatabaseError> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
    let fired = arm_deadline(&mut conn, timeout)
        .await
        .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;
    Ok((conn, fired))
}

/// SQLite Connection
///
/// エンジンが持つプールを使う（#528。以前は、接続ごとにプールを作っていた）
pub struct SqliteConnection {
    pools: Arc<SqlitePools>,
    /// `new`で開いた、この接続だけのプールか。`close`は、そのときだけプールを
    /// 閉じる（エンジンのプールは、ほかの接続も使う。PR #798のレビュー）
    owns_pools: bool,
    config: DatabaseConfig,
    fetch_limit: Option<usize>,
    byte_limit: Option<usize>,
    timeout: Duration,
}

impl SqliteConnection {
    /// 新しいプールを開いて、接続を作る。エンジン（`SqliteEngine`）からは、
    /// エンジンのプールを共有する
    pub async fn new(config: DatabaseConfig) -> Result<Self, DatabaseError> {
        let pools = open_pools(&config).await?;
        Ok(Self {
            owns_pools: true,
            ..Self::with_pools(pools, config)
        })
    }

    /// 開いたプールで、接続を作る
    pub(crate) fn with_pools(pools: Arc<SqlitePools>, config: DatabaseConfig) -> Self {
        let limit = |key: &str| {
            config
                .connection
                .options
                .get(key)
                .and_then(|v| v.parse::<usize>().ok())
        };
        Self {
            fetch_limit: limit(OPTION_FETCH_LIMIT),
            byte_limit: limit(OPTION_BYTE_LIMIT),
            timeout: statement_timeout(&config),
            owns_pools: false,
            pools,
            config,
        }
    }

    /// 書き込みのプール（トランザクション・準備済みの文）。読み取り専用なら、
    /// 読み取りのプール（書き込もうとすると、SQLiteが断る）
    fn write_or_read_pool(&self) -> &SqlitePool {
        self.pools.write.as_ref().unwrap_or(&self.pools.read)
    }

    /// 読み取りのプールから接続を受け取り、時間の上限を付ける
    async fn read_conn(&self) -> Result<(PooledSqlite, Arc<AtomicBool>), DatabaseError> {
        acquire_with_deadline(&self.pools.read, self.timeout).await
    }

    /// 受け取った接続の時間の上限を外し、結果のエラーをmcp-rsのエラーにする
    async fn finish<T>(
        &self,
        mut conn: PooledSqlite,
        fired: Arc<AtomicBool>,
        result: Result<T, sqlx::Error>,
        kind: fn(String) -> DatabaseError,
    ) -> Result<T, DatabaseError> {
        disarm_deadline(&mut conn).await;
        result.map_err(|e| map_error(e, &fired, self.timeout, kind))
    }
}

#[async_trait]
impl DatabaseConnection for SqliteConnection {
    /// 読み取り専用の接続で、トランザクションの中で読み、ロールバックする。
    /// 行数・バイト数・時間の上限で止める
    async fn query(&self, sql: &str, params: &[Value]) -> Result<QueryResult, DatabaseError> {
        let started = Instant::now();
        let (fetch_limit, byte_limit) = (self.fetch_limit, self.byte_limit);
        let (mut conn, fired) = self.read_conn().await?;
        let result = read_rows(&mut conn, sql, params, fetch_limit, byte_limit).await;
        let (columns, rows) = self
            .finish(conn, fired, result, DatabaseError::QueryFailed)
            .await?;
        Ok(QueryResult {
            columns,
            total_rows: Some(rows.len() as u64),
            rows,
            execution_time_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// 1つの文を、トランザクションの中で実行してコミットする。読み取り専用で
    /// 開いた（`allow_write = false`）なら断る
    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        let started = Instant::now();
        let pool = self.pools.write.as_ref().ok_or_else(|| {
            DatabaseError::UnsupportedOperation(
                "the SQLite database was opened read-only (allow_write is not set)".to_string(),
            )
        })?;
        let (mut conn, fired) = acquire_with_deadline(pool, self.timeout).await?;
        let result = async {
            let mut tx = conn.begin().await?;
            // 「最後に挿入したrowid」は、`WITHOUT ROWID`の表への挿入や、
            // `ON CONFLICT DO UPDATE`で既にある行を変えたときは変わらない。
            // 前と後で比べ、変わったときだけ、この文の挿入とみなす（PR #798の
            // レビュー）
            let before: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
                .fetch_one(&mut *tx)
                .await?;
            let done = bind_params(sqlx::query(sql), params)
                .execute(&mut *tx)
                .await?;
            let after: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
                .fetch_one(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok::<_, sqlx::Error>((done, after != before))
        }
        .await;
        let (done, inserted_rowid) = self
            .finish(conn, fired, result, DatabaseError::OperationFailed)
            .await?;
        // 挿入の文で、rowidのある行を足したときだけ返す。接続の「最後に挿入した
        // rowid」は、UPDATE・DELETEでは前の挿入のまま（PR #798のレビュー）。
        // 前と同じrowidの行を挿入したとき（消した行のrowidを使い直したとき）も
        // 返さない（違うIDを返すよりよい）
        Ok(ExecuteResult {
            rows_affected: done.rows_affected(),
            last_insert_id: (done.rows_affected() > 0 && inserted_rowid && is_insert(sql))
                .then(|| Value::Int(done.last_insert_rowid())),
            execution_time_ms: started.elapsed().as_millis() as u64,
        })
    }

    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError> {
        let transaction =
            SqliteTransaction::new(Arc::new(self.write_or_read_pool().clone())).await?;
        Ok(Box::new(transaction))
    }

    /// 読み取り専用の接続で、トランザクションの中で読む
    async fn get_schema(&self) -> Result<DatabaseSchema, DatabaseError> {
        let (mut conn, fired) = self.read_conn().await?;
        let result = read_schema(&mut conn, None).await;
        let (tables, views) = self
            .finish(conn, fired, result, DatabaseError::QueryFailed)
            .await?;
        Ok(DatabaseSchema {
            database_name: self.config.connection.database.clone(),
            tables,
            views,
            procedures: Vec::new(),
        })
    }

    async fn get_table_schema(&self, table_name: &str) -> Result<TableInfo, DatabaseError> {
        let (mut conn, fired) = self.read_conn().await?;
        let result = read_schema(&mut conn, Some(table_name)).await;
        let (tables, views) = self
            .finish(conn, fired, result, DatabaseError::QueryFailed)
            .await?;
        // ビューも、列を返す（MySQLと同じ。キー・インデックスは無い）
        tables
            .into_iter()
            .next()
            .or_else(|| {
                views.into_iter().next().map(|view| TableInfo {
                    name: view.name,
                    schema: None,
                    columns: view.columns,
                    primary_keys: Vec::new(),
                    foreign_keys: Vec::new(),
                    indexes: Vec::new(),
                })
            })
            .ok_or_else(|| DatabaseError::QueryFailed(format!("table not found: {table_name}")))
    }

    async fn prepare(&self, sql: &str) -> Result<Box<dyn PreparedStatement>, DatabaseError> {
        let prepared = SqlitePreparedStatement::new(
            Arc::new(self.write_or_read_pool().clone()),
            sql.to_string(),
        )
        .await?;
        Ok(Box::new(prepared))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        let (mut conn, fired) = self.read_conn().await?;
        // ファイルの中身を読む（`ping`は、接続の仕組みが応えるかだけ）
        let result = sqlx::query("SELECT count(*) FROM sqlite_schema")
            .fetch_one(&mut *conn)
            .await
            .map(|_| ());
        self.finish(conn, fired, result, DatabaseError::ConnectionFailed)
            .await
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        if !self.owns_pools {
            return Ok(());
        }
        self.pools.read.close().await;
        if let Some(write) = &self.pools.write {
            write.close().await;
        }
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: "sqlite".to_string(),
            database_name: self.config.connection.host.clone(),
            user_name: String::new(),
            server_version: "SQLite".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::database::types::{
        ConnectionConfig, DatabaseType, FeatureConfig, PoolConfig, SecurityConfig,
    };
    use std::collections::HashMap;

    fn config_for(path: &str, options: &[(&str, &str)]) -> DatabaseConfig {
        DatabaseConfig {
            database_type: DatabaseType::SQLite,
            connection: ConnectionConfig {
                host: path.to_string(),
                port: 0,
                database: path.to_string(),
                username: String::new(),
                password: String::new(),
                ssl_mode: None,
                timeout_seconds: 30,
                retry_attempts: 0,
                options: options
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<HashMap<_, _>>(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }

    fn create_test_config() -> DatabaseConfig {
        config_for(":memory:", &[])
    }

    /// `Value`は`PartialEq`を持たないので、`Debug`の形で比べる
    fn debug(value: &impl std::fmt::Debug) -> String {
        format!("{value:?}")
    }

    /// 表`items`（3行）のあるデータベースのファイルを作る
    async fn database_file() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.db");
        let path = path.to_string_lossy().into_owned();
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let mut conn = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        for sql in [
            "CREATE TABLE owners (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
            "CREATE TABLE items (id INTEGER PRIMARY KEY, owner_id INTEGER REFERENCES owners, \
             name TEXT NOT NULL, price REAL, data BLOB)",
            "CREATE UNIQUE INDEX items_name ON items (name)",
            "CREATE INDEX items_lower ON items (lower(name))",
            "CREATE VIEW cheap AS SELECT name FROM items WHERE price < 2",
            "CREATE TABLE descending (a INTEGER PRIMARY KEY DESC)",
            "CREATE TABLE pairs (a INTEGER, b TEXT, total INTEGER GENERATED ALWAYS AS (a * 2) VIRTUAL, \
             PRIMARY KEY (a, b))",
            "INSERT INTO owners (name) VALUES ('o')",
            "INSERT INTO items (owner_id, name, price, data) VALUES \
             (1, 'a', 1.5, x'00ff'), (1, 'b', 2.5, NULL), (NULL, 'c', NULL, NULL)",
        ] {
            sqlx::query(sql).execute(&mut conn).await.unwrap();
        }
        conn.close().await.unwrap();
        (dir, path)
    }

    #[tokio::test]
    async fn test_connection_creation() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_ping() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();
        assert!(conn.ping().await.is_ok());
    }

    #[tokio::test]
    async fn test_query() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        let result = conn.query("SELECT 1 as num", &[]).await;
        assert!(result.is_ok());

        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
        assert_eq!(query_result.columns.len(), 1);
    }

    #[tokio::test]
    async fn test_execute() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        conn.execute("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)", &[])
            .await
            .unwrap();

        let result = conn
            .execute(
                "INSERT INTO test (name) VALUES (?)",
                &[Value::String("test".to_string())],
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().rows_affected, 1);
    }

    #[tokio::test]
    async fn test_get_schema() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY)", &[])
            .await
            .unwrap();

        let schema = conn.get_schema().await.unwrap();
        assert!(schema.tables.iter().any(|t| &t.name == "users"));
    }

    #[tokio::test]
    async fn test_get_table_schema() {
        let config = create_test_config();
        let conn = SqliteConnection::new(config).await.unwrap();

        conn.execute("CREATE TABLE test_table (id INTEGER, name TEXT)", &[])
            .await
            .unwrap();

        let table_info = conn.get_table_schema("test_table").await.unwrap();
        assert_eq!(table_info.name, "test_table");
        assert_eq!(table_info.columns.len(), 2);
    }

    /// 無いファイルは作らない（#528）
    #[tokio::test]
    async fn test_missing_file_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.db");
        let path = path.to_string_lossy().into_owned();
        let err = SqliteConnection::new(config_for(&path, &[]))
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("was not found"), "{err}");
        assert!(!std::path::Path::new(&path).exists());
    }

    /// WALモードのデータベースも、読み取り専用で読める（#528）。ほかの
    /// 接続が書いている間（`-wal`・`-shm`がある）も、閉じた後も
    #[tokio::test]
    async fn test_wal_database_can_be_read() {
        let (_dir, path) = database_file().await;
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let mut writer = sqlx::SqliteConnection::connect_with(&options)
            .await
            .unwrap();
        sqlx::query("INSERT INTO items (name) VALUES ('w')")
            .execute(&mut writer)
            .await
            .unwrap();
        let conn = SqliteConnection::new(config_for(&path, &[(OPTION_READ_ONLY, "true")]))
            .await
            .unwrap();
        let count = conn.query("SELECT count(*) FROM items", &[]).await.unwrap();
        assert_eq!(debug(&count.rows), debug(&[vec![Value::Int(4)]]));
        conn.close().await.unwrap();
        writer.close().await.unwrap();

        let conn = SqliteConnection::new(config_for(&path, &[(OPTION_READ_ONLY, "true")]))
            .await
            .unwrap();
        let count = conn.query("SELECT count(*) FROM items", &[]).await.unwrap();
        assert_eq!(debug(&count.rows), debug(&[vec![Value::Int(4)]]));
    }

    /// 値は、値の型で読む。パラメーターは`?`にバインドする（#528）
    #[tokio::test]
    async fn test_query_reads_each_value_by_its_type() {
        let (_dir, path) = database_file().await;
        let conn = SqliteConnection::new(config_for(&path, &[(OPTION_READ_ONLY, "true")]))
            .await
            .unwrap();
        let result = conn
            .query(
                "SELECT id, name, price, data FROM items WHERE id = ?",
                &[Value::Int(1)],
            )
            .await
            .unwrap();
        let names: Vec<&str> = result.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "price", "data"]);
        assert_eq!(
            debug(&result.rows),
            debug(&[vec![
                Value::Int(1),
                Value::String("a".to_string()),
                Value::Float(1.5),
                Value::Binary(vec![0x00, 0xff]),
            ]])
        );

        // 行が無くても、列を返す
        let empty = conn
            .query("SELECT name FROM items WHERE id = 99", &[])
            .await
            .unwrap();
        assert!(empty.rows.is_empty());
        assert_eq!(empty.columns.len(), 1);
    }

    /// 既定（`allow_write = false`）では、ファイルを読み取り専用で開く。
    /// 書き込みの接続は無く、`query()`で書こうとしてもSQLiteが断る（#528）
    #[tokio::test]
    async fn test_read_only_database_cannot_be_written() {
        let (_dir, path) = database_file().await;
        let conn = SqliteConnection::new(config_for(&path, &[(OPTION_READ_ONLY, "true")]))
            .await
            .unwrap();
        let err = conn
            .execute("DELETE FROM items", &[])
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("read-only"), "{err}");
        let err = conn
            .query("DELETE FROM items RETURNING id", &[])
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("readonly"), "{err}");
        let count = conn.query("SELECT count(*) FROM items", &[]).await.unwrap();
        assert_eq!(debug(&count.rows), debug(&[vec![Value::Int(3)]]));
    }

    /// `allow_write`なら書け、コミットする。`query()`は、書き込みを許しても
    /// 読み取り専用の接続で読む（#528）
    #[tokio::test]
    async fn test_writable_database_commits_and_query_stays_read_only() {
        let (_dir, path) = database_file().await;
        let conn = SqliteConnection::new(config_for(&path, &[])).await.unwrap();
        let done = conn
            .execute(
                "INSERT INTO items (name, price) VALUES (?, ?)",
                &[Value::String("d".to_string()), Value::Float(9.0)],
            )
            .await
            .unwrap();
        assert_eq!(done.rows_affected, 1);
        assert_eq!(debug(&done.last_insert_id), debug(&Some(Value::Int(4))));
        let err = conn
            .query("UPDATE items SET price = 0 RETURNING id", &[])
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("readonly"), "{err}");
        let count = conn.query("SELECT count(*) FROM items", &[]).await.unwrap();
        assert_eq!(debug(&count.rows), debug(&[vec![Value::Int(4)]]));
    }

    /// 行数・バイト数の上限で、残りの行は読まない（#528）
    #[tokio::test]
    async fn test_query_stops_at_the_row_and_byte_limits() {
        let (_dir, path) = database_file().await;
        let conn = SqliteConnection::new(config_for(
            &path,
            &[(OPTION_READ_ONLY, "true"), (OPTION_FETCH_LIMIT, "2")],
        ))
        .await
        .unwrap();
        let rows = conn.query("SELECT name FROM items", &[]).await.unwrap();
        assert_eq!(rows.rows.len(), 2);

        let conn = SqliteConnection::new(config_for(
            &path,
            &[(OPTION_READ_ONLY, "true"), (OPTION_BYTE_LIMIT, "1")],
        ))
        .await
        .unwrap();
        let rows = conn.query("SELECT name FROM items", &[]).await.unwrap();
        assert_eq!(rows.rows.len(), 1);
    }

    /// 時間の上限を過ぎたSQLは、止めて時間切れのエラーにする。接続は、その
    /// 後も使える（#528）
    #[tokio::test]
    async fn test_query_is_stopped_after_the_timeout() {
        let mut config = create_test_config();
        config.features.query_timeout = 1;
        let conn = SqliteConnection::new(config).await.unwrap();
        let started = Instant::now();
        let err = conn
            .query(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) \
                 SELECT count(*) FROM n",
                &[],
            )
            .await
            .unwrap_err();
        assert!(matches!(err, DatabaseError::TimeoutError(_)), "{err}");
        assert!(started.elapsed() < Duration::from_secs(10));
        let one = conn.query("SELECT 1", &[]).await.unwrap();
        assert_eq!(debug(&one.rows), debug(&[vec![Value::Int(1)]]));
    }

    /// スキーマ: 列・主キー・外部キー・インデックス・ビュー。式を含む
    /// インデックスは除く。表の名前はSQLに埋め込まない（#528）
    #[tokio::test]
    async fn test_schema_of_a_database_file() {
        let (_dir, path) = database_file().await;
        let conn = SqliteConnection::new(config_for(&path, &[(OPTION_READ_ONLY, "true")]))
            .await
            .unwrap();
        let schema = conn.get_schema().await.unwrap();
        let names: Vec<&str> = schema.tables.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["descending", "items", "owners", "pairs"]);
        // `INTEGER PRIMARY KEY DESC`は、rowidの別名ではないので、NULLを入れられる
        assert!(schema.tables[0].columns[0].nullable);
        let items = &schema.tables[1];
        assert_eq!(items.primary_keys, ["id"]);
        let id = &items.columns[0];
        assert_eq!(
            (id.name.as_str(), id.data_type.as_str(), id.nullable),
            ("id", "INTEGER", false)
        );
        let name = items.columns.iter().find(|c| c.name == "name").unwrap();
        assert!(!name.nullable);
        assert_eq!(items.foreign_keys.len(), 1);
        assert_eq!(items.foreign_keys[0].column, "owner_id");
        assert_eq!(items.foreign_keys[0].referenced_table, "owners");
        assert_eq!(items.foreign_keys[0].referenced_column, "id");
        let indexes: Vec<&str> = items.indexes.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(indexes, ["items_name"]);
        assert!(items.indexes[0].is_unique);
        assert_eq!(schema.views.len(), 1);
        assert_eq!(schema.views[0].name, "cheap");
        assert!(schema.views[0].definition.contains("price < 2"));

        // 生成列も返す。複合の主キーの`INTEGER`の列は、NULLを入れられる
        // （PR #798のレビュー）
        let pairs = &schema.tables[3];
        let columns: Vec<(&str, bool)> = pairs
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.nullable))
            .collect();
        assert_eq!(columns, [("a", true), ("b", true), ("total", true)]);
        assert_eq!(pairs.primary_keys, ["a", "b"]);

        let view = conn.get_table_schema("cheap").await.unwrap();
        // 名前は、大文字と小文字を区別しない
        assert_eq!(conn.get_table_schema("ITEMS").await.unwrap().name, "items");
        assert_eq!(view.columns.len(), 1);
        // 名前は埋め込まないので、SQLとして読まれない
        let err = conn
            .get_table_schema("items); DROP TABLE items; --")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("table not found"), "{err}");
    }

    /// UPDATEでは、前の挿入の`last_insert_id`を返さない（PR #798のレビュー）
    #[tokio::test]
    async fn test_last_insert_id_only_for_inserts() {
        let (_dir, path) = database_file().await;
        let mut config = config_for(&path, &[]);
        config.pool.max_connections = 1;
        let conn = SqliteConnection::new(config).await.unwrap();
        let inserted = conn
            .execute("INSERT INTO items (name) VALUES ('d')", &[])
            .await
            .unwrap();
        assert_eq!(debug(&inserted.last_insert_id), debug(&Some(Value::Int(4))));
        let updated = conn
            .execute("UPDATE items SET price = 1 WHERE id = 1", &[])
            .await
            .unwrap();
        assert_eq!(updated.rows_affected, 1);
        assert!(updated.last_insert_id.is_none());

        // `WITHOUT ROWID`の表への挿入と、既にある行を変えた`ON CONFLICT`も
        // 返さない（PR #798のレビュー）
        conn.execute(
            "CREATE TABLE tags (name TEXT PRIMARY KEY, n INTEGER) WITHOUT ROWID",
            &[],
        )
        .await
        .unwrap();
        let no_rowid = conn
            .execute("INSERT INTO tags (name, n) VALUES ('x', 1)", &[])
            .await
            .unwrap();
        assert_eq!(no_rowid.rows_affected, 1);
        assert!(no_rowid.last_insert_id.is_none());
        let upsert = conn
            .execute(
                "INSERT INTO items (id, name) VALUES (1, 'z') \
                 ON CONFLICT (id) DO UPDATE SET price = 5",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(upsert.rows_affected, 1);
        assert!(upsert.last_insert_id.is_none());
    }

    /// SQLiteのファイルでなければ、開くときに断る（PR #798のレビュー）
    #[tokio::test]
    async fn test_a_file_that_is_not_sqlite_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.txt");
        std::fs::write(
            &path,
            "this is not a database, just some text that is long enough\n".repeat(20),
        )
        .unwrap();
        let path = path.to_string_lossy().into_owned();
        let err = SqliteConnection::new(config_for(&path, &[(OPTION_READ_ONLY, "true")]))
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("cannot open the SQLite database"), "{err}");
    }

    /// 大きな時間の上限でも、sqlxの`busy_timeout`でパニックしない（PR #798の
    /// レビュー）
    #[tokio::test]
    async fn test_a_long_timeout_does_not_panic() {
        let (_dir, path) = database_file().await;
        let mut config = config_for(&path, &[(OPTION_READ_ONLY, "true")]);
        config.features.query_timeout = u32::MAX;
        let conn = SqliteConnection::new(config).await.unwrap();
        conn.query("SELECT 1", &[]).await.unwrap();
    }

    /// エンジンのプールを共有する接続は、`close`でプールを閉じない。別の
    /// ファイルの設定には、その接続だけのプールを開く（PR #798のレビュー）
    #[tokio::test]
    async fn test_engine_connections_share_pools_safely() {
        use crate::handlers::database::engine::DatabaseEngine;
        let (_dir, path) = database_file().await;
        let (_other_dir, other) = database_file().await;
        let config = config_for(&path, &[(OPTION_READ_ONLY, "true")]);
        let engine = super::super::SqliteEngine::new(config.clone())
            .await
            .unwrap();
        let a = engine.connect(&config).await.unwrap();
        let b = engine.connect(&config).await.unwrap();
        a.close().await.unwrap();
        b.query("SELECT 1", &[]).await.unwrap();

        // 待つ時間が違えば、その接続だけのプールを開く。閉じても、共有の
        // プールは閉じない
        let mut slower = config.clone();
        slower.connection.timeout_seconds = 1;
        let d = engine.connect(&slower).await.unwrap();
        d.close().await.unwrap();
        b.query("SELECT 1", &[]).await.unwrap();

        let writable = config_for(&other, &[]);
        let c = engine.connect(&writable).await.unwrap();
        c.execute("DELETE FROM items", &[]).await.unwrap();
        let left = c.query("SELECT count(*) FROM items", &[]).await.unwrap();
        assert_eq!(debug(&left.rows), debug(&[vec![Value::Int(0)]]));
        let kept = b.query("SELECT count(*) FROM items", &[]).await.unwrap();
        assert_eq!(debug(&kept.rows), debug(&[vec![Value::Int(3)]]));
    }

    /// 取り消した呼び出しの時間の上限は、接続に残らない。後の準備済みの文が、
    /// 前の上限で止まらない（PR #798のレビュー）
    #[tokio::test]
    async fn test_a_cancelled_call_leaves_no_deadline() {
        let mut config = create_test_config();
        config.features.query_timeout = 1;
        let conn = SqliteConnection::new(config).await.unwrap();
        let endless = "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n) \
                       SELECT count(*) FROM n";
        let cancelled =
            tokio::time::timeout(Duration::from_millis(200), conn.query(endless, &[])).await;
        assert!(cancelled.is_err());
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let prepared = conn
            .prepare(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 200000) \
                 SELECT count(*) FROM n",
            )
            .await
            .unwrap();
        // 止まらずに最後まで読める（準備済みの文は、値を宣言の型で読むので、
        // 式の値は`Null`になる。#528の前からの動き）
        let result = prepared.query(&[]).await.unwrap();
        assert_eq!(result.rows.len(), 1);
    }
}
