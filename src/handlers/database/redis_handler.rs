//! Redisのツール（`[database]`の`engine = "redis"`、#529）
//!
//! RedisはSQLではないので、SQLのツール（`execute_query`等）とは別のツールを
//! 出す。
//!
//! - `redis_command`: 読み取りのコマンドを1つ実行する（`GET`・`HGETALL`・
//!   `LRANGE`・`ZRANGE`・`TTL`等）
//! - `scan_keys`: `SCAN`で、鍵を少しずつ一覧する（`KEYS`は使わない）
//! - `get_schema`: サーバーの版・データベースの番号・鍵の数（`INFO`・`DBSIZE`）
//! - `redis_write_command`: 書き込みのコマンドを1つ実行する。`allow_write = true`の
//!   ときだけ出す
//!
//! 安全性:
//!
//! - 使えるのは、mcp-rsが決めた一覧のコマンドだけ。`KEYS`・`FLUSHALL`・
//!   `FLUSHDB`・`CONFIG`・`EVAL`・`SCRIPT`・`FUNCTION`・`MODULE`・`SHUTDOWN`・
//!   `DEBUG`・`MIGRATE`・`SELECT`・`CLIENT`・`ACL`・`MONITOR`・`SUBSCRIBE`・
//!   `MULTI`等は、`allow_write`でも使えない
//! - 返す量の上限。集合をまるごと返すコマンド（`HGETALL`・`SMEMBERS`等）と範囲の
//!   コマンド（`LRANGE`・`ZRANGE`・`XRANGE`）は、送る前に数を確かめ、
//!   `max_rows`を超えるなら断る（Redisが送る前に止める）。大きな値（`GET`等）は、
//!   `STRLEN`で確かめ、`max_result_bytes`を超えるなら断る
//! - 1つのコマンドの時間の上限（`query_timeout_seconds`。応答を待つ時間）
//! - 監査ログには、コマンドの名前と引数の数だけを書く（値は書かない）

use crate::config::DatabaseSettings;
use crate::handlers::database::engines::{is_loopback_host, ssl_mode_missing};
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolAnnotations,
    ToolCallParams,
};
use async_trait::async_trait;
use base64::prelude::*;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{ConnectionAddr, ConnectionInfo, IntoConnectionInfo, RedisConnectionInfo};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

/// 接続を作るときの待ち時間の上限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// `scan_keys`で、1回の呼び出しに送る`SCAN`の数の上限
const MAX_SCAN_ROUNDS: usize = 1000;

/// 送る前に確かめること
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    /// 結果は小さい（数・真偽・1つの短い値）
    None,
    /// 鍵（`args[0]`）の集合の要素の数を、このコマンドで確かめる
    Cardinality(&'static str),
    /// 鍵の文字列の長さを`STRLEN`で確かめる（`GET`・`GETRANGE`）
    StringLength,
    /// ハッシュの値の長さを`HSTRLEN`で確かめる（`HGET`）
    HashValueLength,
    /// `args[skip..]`の数（鍵・項目の数）が`max_rows`まで
    ArgumentCount(usize),
    /// `LRANGE`: `LLEN`と範囲から、返す数を計算する
    ListRange,
    /// `ZRANGE`: 番号の範囲なら`ZCARD`から計算し、ほかは`LIMIT`が要る
    SortedSetRange,
    /// `XRANGE`・`XREVRANGE`: `COUNT`が無ければ足す
    StreamRange,
    /// `HSCAN`・`SSCAN`・`ZSCAN`: `COUNT`は`max_rows`まで
    Scan,
    /// `LPOP`・`RPOP`: 数（`args[1]`）は`max_rows`まで
    PopCount,
}

/// 読み取りのコマンド（`redis_command`）
const READ_COMMANDS: &[(&str, Check)] = &[
    // 文字列
    ("GET", Check::StringLength),
    ("MGET", Check::ArgumentCount(0)),
    ("STRLEN", Check::None),
    ("GETRANGE", Check::StringLength),
    // 鍵
    ("EXISTS", Check::None),
    ("TYPE", Check::None),
    ("TTL", Check::None),
    ("PTTL", Check::None),
    // ハッシュ
    ("HGET", Check::HashValueLength),
    ("HMGET", Check::ArgumentCount(1)),
    ("HLEN", Check::None),
    ("HEXISTS", Check::None),
    ("HSTRLEN", Check::None),
    ("HGETALL", Check::Cardinality("HLEN")),
    ("HKEYS", Check::Cardinality("HLEN")),
    ("HVALS", Check::Cardinality("HLEN")),
    ("HSCAN", Check::Scan),
    // リスト
    ("LLEN", Check::None),
    ("LINDEX", Check::None),
    ("LRANGE", Check::ListRange),
    // 集合
    ("SCARD", Check::None),
    ("SISMEMBER", Check::None),
    ("SMISMEMBER", Check::ArgumentCount(1)),
    ("SMEMBERS", Check::Cardinality("SCARD")),
    ("SSCAN", Check::Scan),
    // ソート済み集合
    ("ZCARD", Check::None),
    ("ZSCORE", Check::None),
    ("ZMSCORE", Check::ArgumentCount(1)),
    ("ZRANK", Check::None),
    ("ZREVRANK", Check::None),
    ("ZCOUNT", Check::None),
    ("ZRANGE", Check::SortedSetRange),
    ("ZSCAN", Check::Scan),
    // ストリーム
    ("XLEN", Check::None),
    ("XRANGE", Check::StreamRange),
    ("XREVRANGE", Check::StreamRange),
];

/// 書き込みのコマンド（`redis_write_command`）
const WRITE_COMMANDS: &[(&str, Check)] = &[
    ("SET", Check::None),
    ("DEL", Check::ArgumentCount(0)),
    ("UNLINK", Check::ArgumentCount(0)),
    ("EXPIRE", Check::None),
    ("PEXPIRE", Check::None),
    ("PERSIST", Check::None),
    ("INCR", Check::None),
    ("INCRBY", Check::None),
    ("DECR", Check::None),
    ("DECRBY", Check::None),
    ("HSET", Check::None),
    ("HDEL", Check::None),
    ("HINCRBY", Check::None),
    ("LPUSH", Check::None),
    ("RPUSH", Check::None),
    ("LPOP", Check::PopCount),
    ("RPOP", Check::PopCount),
    ("LSET", Check::None),
    ("LREM", Check::None),
    ("SADD", Check::None),
    ("SREM", Check::None),
    ("ZADD", Check::None),
    ("ZREM", Check::None),
    ("ZINCRBY", Check::None),
    ("XADD", Check::None),
];

/// ツールの設定
#[derive(Clone, Debug)]
struct Limits {
    max_items: usize,
    max_result_bytes: usize,
}

/// Redisのハンドラー（#529）
pub struct RedisHandler {
    conn: ConnectionManager,
    allow_write: bool,
    limits: Limits,
    database: u16,
    status_line: String,
    audit_logger: Option<Arc<crate::security::AuditLogger>>,
}

/// `[database]`を、つながずに確かめる（`mcp-rs check`、#589）。通信を守れない
/// 設定なら、その警告を返す
pub fn check_settings(settings: &DatabaseSettings) -> Result<Option<String>, String> {
    settings.validate()?;
    let host = settings.host();
    match settings.ssl_mode.as_deref() {
        None if !is_loopback_host(host) => Err(ssl_mode_missing(host)),
        Some("disable") if !is_loopback_host(host) => Ok(Some(format!(
            "ssl_mode = \"disable\": the connection to {host} is not encrypted (the password and the data are sent as they are)"
        ))),
        Some("require") => Ok(Some(format!(
            "ssl_mode = \"require\": the server certificate of {host} is not checked, so another server could pretend to be it. Use ssl_mode = \"verify-full\""
        ))),
        _ => Ok(None),
    }
}

/// `[database]`から、Redisの接続の情報を作る
fn connection_info(settings: &DatabaseSettings) -> ConnectionInfo {
    let host = settings.host().to_string();
    let port = settings.port();
    let addr = match settings.ssl_mode.as_deref() {
        Some("require") => ConnectionAddr::TcpTls {
            host,
            port,
            insecure: true,
            tls_params: None,
        },
        Some("verify-full") => ConnectionAddr::TcpTls {
            host,
            port,
            insecure: false,
            tls_params: None,
        },
        _ => ConnectionAddr::Tcp(host, port),
    };
    let mut redis = RedisConnectionInfo::default()
        .set_db(i64::from(settings.redis_database().unwrap_or(0)))
        .set_lib_name("mcp-rs", env!("CARGO_PKG_VERSION"));
    if !settings.username.trim().is_empty() {
        redis = redis.set_username(settings.username.trim());
    }
    if let Some(password) = settings.password.as_deref().filter(|p| !p.is_empty()) {
        redis = redis.set_password(password);
    }
    // `ConnectionAddr`からの変換は失敗しない
    addr.into_connection_info()
        .expect("a ConnectionAddr is a connection info")
        .set_redis_settings(redis)
}

impl RedisHandler {
    /// `[database]`から作り、つながるかを確かめる。失敗したら、起動時のログと
    /// `mcp_rs_status`に出す理由を返す（パスワードは含めない）
    pub async fn from_settings(settings: &DatabaseSettings) -> Result<Self, String> {
        if let Some(warning) = check_settings(settings)? {
            warn!("{}", warning);
        }
        // rustlsの暗号の実装を決める（MySQLと同じ。既に決まっていれば何もしない）
        let _ = rustls::crypto::ring::default_provider().install_default();
        let info = connection_info(settings);
        let describe = settings.describe();
        let client = match settings
            .ssl_root_cert
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        {
            Some(cert) => {
                let root_cert = std::fs::read(cert)
                    .map_err(|e| format!("cannot read database.ssl_root_cert {cert}: {e}"))?;
                redis::Client::build_with_tls(
                    info,
                    redis::TlsCertificates {
                        client_tls: None,
                        root_cert: Some(root_cert),
                    },
                )
            }
            None => redis::Client::open(info),
        }
        .map_err(|e| format!("cannot set up {describe}: {e}"))?;
        let timeout = Duration::from_secs(u64::from(settings.query_timeout_seconds()));
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(CONNECT_TIMEOUT))
            .set_response_timeout(Some(timeout))
            .set_number_of_retries(2);
        let mut conn = tokio::time::timeout(
            CONNECT_TIMEOUT + Duration::from_secs(2),
            client.get_connection_manager_with_config(config),
        )
        .await
        .map_err(|_| {
            format!(
                "cannot connect to {describe}: no answer within {} seconds",
                CONNECT_TIMEOUT.as_secs()
            )
        })?
        .map_err(|e| format!("cannot connect to {describe}: {e}"))?;
        // 認証は、つないだときに済んでいる。応えるかを確かめる
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| format!("cannot connect to {describe}: {e}"))?;
        Ok(Self {
            conn,
            allow_write: settings.allow_write(),
            limits: Limits {
                max_items: settings.max_rows() as usize,
                max_result_bytes: usize::try_from(settings.max_result_bytes())
                    .unwrap_or(usize::MAX),
            },
            database: settings.redis_database().unwrap_or(0),
            status_line: describe,
            audit_logger: None,
        })
    }

    /// ツール呼び出しを監査ログに記録する（ビルダースタイル）
    pub fn with_audit_logger(mut self, audit_logger: Arc<crate::security::AuditLogger>) -> Self {
        self.audit_logger = Some(audit_logger);
        self
    }

    /// コマンドを送り、結果を受け取る
    async fn send(&self, name: &str, args: &[String]) -> Result<redis::Value, McpError> {
        let mut cmd = redis::cmd(name);
        for arg in args {
            cmd.arg(arg);
        }
        let mut conn = self.conn.clone();
        cmd.query_async::<redis::Value>(&mut conn)
            .await
            .map_err(|e| McpError::ExternalApi(format!("Redis {name}: {e}")))
    }

    /// 数を返すコマンドを送る
    async fn count(&self, name: &str, args: &[String]) -> Result<i64, McpError> {
        match self.send(name, args).await? {
            redis::Value::Int(n) => Ok(n),
            other => Err(McpError::ExternalApi(format!(
                "Redis {name} returned {other:?} instead of a number"
            ))),
        }
    }

    /// `check`を行い、送る引数を返す（`XRANGE`は`COUNT`を足す）
    async fn prepare(
        &self,
        name: &str,
        check: Check,
        mut args: Vec<String>,
    ) -> Result<Vec<String>, McpError> {
        let max = self.limits.max_items;
        let too_many = |what: String| {
            McpError::InvalidParams(format!(
                "{name} would return {what}, more than max_rows ({max}) allows"
            ))
        };
        let key = || {
            args.first().cloned().ok_or_else(|| {
                McpError::InvalidParams(format!("{name} needs a key as its first argument"))
            })
        };
        match check {
            Check::None => {}
            Check::ArgumentCount(skip) => {
                let n = args.len().saturating_sub(skip);
                if n > max {
                    return Err(too_many(format!("{n} values")));
                }
            }
            Check::Cardinality(len_command) => {
                let n = self.count(len_command, &[key()?]).await?;
                if n > max as i64 {
                    let scan = if len_command == "HLEN" {
                        "HSCAN"
                    } else {
                        "SSCAN"
                    };
                    return Err(McpError::InvalidParams(format!(
                        "{name} would return {n} items, more than max_rows ({max}) allows. Use {scan} with COUNT to read them in parts"
                    )));
                }
            }
            Check::StringLength | Check::HashValueLength => {
                let length = if check == Check::StringLength {
                    self.count("STRLEN", &[key()?]).await?
                } else {
                    let field = args.get(1).cloned().ok_or_else(|| {
                        McpError::InvalidParams("HGET needs a key and a field".to_string())
                    })?;
                    self.count("HSTRLEN", &[key()?, field]).await?
                };
                let length = usize::try_from(length).unwrap_or(0);
                let requested = if name == "GETRANGE" {
                    range_size(length, args.get(1), args.get(2))?
                } else {
                    length
                };
                if requested > self.limits.max_result_bytes {
                    return Err(McpError::InvalidParams(format!(
                        "{name} would return {requested} bytes, more than max_result_bytes ({}) allows. Use GETRANGE to read a part of the value",
                        self.limits.max_result_bytes
                    )));
                }
            }
            Check::ListRange => {
                let length = self.count("LLEN", &[key()?]).await?;
                let n = range_size(
                    usize::try_from(length).unwrap_or(0),
                    args.get(1),
                    args.get(2),
                )?;
                if n > max {
                    return Err(too_many(format!("{n} items")));
                }
            }
            Check::SortedSetRange => {
                let upper: Vec<String> = args.iter().map(|a| a.to_ascii_uppercase()).collect();
                if let Some(at) = upper.iter().position(|a| a == "LIMIT") {
                    let count = upper
                        .get(at + 2)
                        .and_then(|c| c.parse::<i64>().ok())
                        .ok_or_else(|| {
                            McpError::InvalidParams(
                                "ZRANGE LIMIT needs an offset and a count".to_string(),
                            )
                        })?;
                    if count < 0 || count as usize > max {
                        return Err(McpError::InvalidParams(format!(
                            "ZRANGE LIMIT count must be from 0 to max_rows ({max})"
                        )));
                    }
                } else if upper.iter().any(|a| a == "BYSCORE" || a == "BYLEX") {
                    return Err(McpError::InvalidParams(format!(
                        "ZRANGE with BYSCORE or BYLEX needs LIMIT offset count (count up to max_rows, {max})"
                    )));
                } else {
                    let length = self.count("ZCARD", &[key()?]).await?;
                    let n = range_size(
                        usize::try_from(length).unwrap_or(0),
                        args.get(1),
                        args.get(2),
                    )?;
                    if n > max {
                        return Err(too_many(format!("{n} items")));
                    }
                }
            }
            Check::StreamRange => {
                let upper: Vec<String> = args.iter().map(|a| a.to_ascii_uppercase()).collect();
                match upper.iter().position(|a| a == "COUNT") {
                    Some(at) => {
                        let count = upper.get(at + 1).and_then(|c| c.parse::<usize>().ok());
                        if !count.is_some_and(|c| c <= max) {
                            return Err(McpError::InvalidParams(format!(
                                "{name} COUNT must be from 0 to max_rows ({max})"
                            )));
                        }
                    }
                    // 1つ多く読み、超えたかを知る
                    None => args.extend(["COUNT".to_string(), (max + 1).to_string()]),
                }
            }
            Check::Scan => {
                let upper: Vec<String> = args.iter().map(|a| a.to_ascii_uppercase()).collect();
                if let Some(at) = upper.iter().position(|a| a == "COUNT") {
                    let count = upper.get(at + 1).and_then(|c| c.parse::<usize>().ok());
                    if !count.is_some_and(|c| c <= max) {
                        return Err(McpError::InvalidParams(format!(
                            "{name} COUNT must be from 0 to max_rows ({max})"
                        )));
                    }
                }
            }
            Check::PopCount => {
                if let Some(count) = args.get(1) {
                    if !count.parse::<usize>().is_ok_and(|c| c <= max) {
                        return Err(McpError::InvalidParams(format!(
                            "{name} count must be from 0 to max_rows ({max})"
                        )));
                    }
                }
            }
        }
        Ok(args)
    }

    /// `redis_command`・`redis_write_command`
    async fn handle_command(&self, args: &JsonValue, write: bool) -> Result<JsonValue, McpError> {
        let (name, command_args) = parse_command(args)?;
        let list = if write { WRITE_COMMANDS } else { READ_COMMANDS };
        let check = list
            .iter()
            .find(|(command, _)| *command == name)
            .map(|(_, check)| *check)
            .ok_or_else(|| {
                let tool = if write {
                    "redis_write_command"
                } else {
                    "redis_command"
                };
                let commands: Vec<&str> = list.iter().map(|(c, _)| *c).collect();
                McpError::InvalidParams(format!(
                    "{tool} does not run {name}. Allowed: {}",
                    commands.join(", ")
                ))
            })?;
        let command_args = self.prepare(&name, check, command_args).await?;
        let value = self.send(&name, &command_args).await?;
        let mut result = if name == "HGETALL" {
            pairs_to_object(value)
        } else {
            value_to_json(value)
        };
        let mut truncated = false;
        if check == Check::StreamRange {
            truncated |= keep_items(&mut result, self.limits.max_items);
        }
        truncated |= keep_within_bytes(&mut result, self.limits.max_result_bytes);
        Ok(json!({
            "command": name,
            "result": result,
            "truncated": truncated,
            "max_rows": self.limits.max_items,
            "max_result_bytes": self.limits.max_result_bytes,
        }))
    }

    /// `scan_keys`
    async fn handle_scan_keys(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let pattern = optional_string(args, "pattern")?.unwrap_or_else(|| "*".to_string());
        let kind = optional_string(args, "type")?;
        let mut cursor = optional_string(args, "cursor")?.unwrap_or_else(|| "0".to_string());
        if cursor.parse::<u64>().is_err() {
            return Err(McpError::InvalidParams(
                "cursor must be the number returned by the previous scan_keys (or \"0\")"
                    .to_string(),
            ));
        }
        let max = self.limits.max_items;
        let batch = max.clamp(1, 1000).to_string();
        let mut keys: Vec<JsonValue> = Vec::new();
        for _ in 0..MAX_SCAN_ROUNDS {
            let mut scan_args = vec![
                cursor.clone(),
                "MATCH".to_string(),
                pattern.clone(),
                "COUNT".to_string(),
                batch.clone(),
            ];
            if let Some(kind) = &kind {
                scan_args.extend(["TYPE".to_string(), kind.clone()]);
            }
            let value = self.send("SCAN", &scan_args).await?;
            let redis::Value::Array(mut parts) = value else {
                return Err(McpError::ExternalApi(
                    "Redis SCAN returned an unexpected reply".to_string(),
                ));
            };
            if parts.len() != 2 {
                return Err(McpError::ExternalApi(
                    "Redis SCAN returned an unexpected reply".to_string(),
                ));
            }
            let found = parts.pop();
            cursor = match parts.pop() {
                Some(redis::Value::BulkString(bytes)) => {
                    String::from_utf8_lossy(&bytes).into_owned()
                }
                Some(redis::Value::SimpleString(text)) => text,
                _ => {
                    return Err(McpError::ExternalApi(
                        "Redis SCAN returned an unexpected cursor".to_string(),
                    ))
                }
            };
            if let Some(JsonValue::Array(found)) = found.map(value_to_json) {
                keys.extend(found);
            }
            if cursor == "0" || keys.len() >= max {
                break;
            }
        }
        // 1回の`SCAN`の結果は、`COUNT`より多いことがある
        let truncated = keys.len() > max;
        keys.truncate(max);
        Ok(json!({
            "keys": keys,
            "cursor": cursor,
            "done": cursor == "0",
            "truncated": truncated,
            "max_rows": max,
        }))
    }

    /// `get_schema`: サーバーの版・データベースの番号・鍵の数
    async fn handle_get_schema(&self) -> Result<JsonValue, McpError> {
        let keys = self.count("DBSIZE", &[]).await?;
        // `INFO`は、ACLで使えないことがある。使えなければ省く
        let server = self.info("server").await.map(|text| parse_info(&text));
        let keyspace = self.info("keyspace").await.map(|text| parse_info(&text));
        let field = |section: &Option<serde_json::Map<String, JsonValue>>, name: &str| {
            section
                .as_ref()
                .and_then(|s| s.get(name).cloned())
                .unwrap_or(JsonValue::Null)
        };
        Ok(json!({
            "engine": "redis",
            "version": field(&server, "redis_version"),
            "mode": field(&server, "redis_mode"),
            "database": self.database,
            "keys": keys,
            "keyspace": keyspace.map(JsonValue::Object).unwrap_or(JsonValue::Null),
            "note": "Redis has no tables: use scan_keys to list keys, and TYPE to see the type of a key",
        }))
    }

    /// `INFO <section>`の文。使えなければ`None`
    async fn info(&self, section: &str) -> Option<String> {
        match self.send("INFO", &[section.to_string()]).await {
            Ok(redis::Value::BulkString(bytes)) => {
                Some(String::from_utf8_lossy(&bytes).into_owned())
            }
            Ok(redis::Value::VerbatimString { text, .. }) => Some(text),
            _ => None,
        }
    }

    /// ツール呼び出しを監査ログに記録する。コマンドの名前と引数の数だけを
    /// 書き、値は書かない
    async fn audit_tool_call(
        &self,
        tool_name: &str,
        args: &JsonValue,
        result: &Result<JsonValue, McpError>,
        started: std::time::Instant,
    ) {
        let Some(audit_logger) = &self.audit_logger else {
            return;
        };
        use crate::security::{AuditCategory, AuditLevel, AuditLogEntry};
        let success = result.is_ok();
        let outcome = if success { "SUCCESS" } else { "FAILURE" };
        let mut entry = AuditLogEntry::new(
            if success {
                AuditLevel::Info
            } else {
                AuditLevel::Error
            },
            AuditCategory::DataAccess,
            format!("Database tool call: {} - {}", tool_name, outcome),
        )
        .with_action(self.status_line.clone(), tool_name.to_string())
        .with_result(outcome.to_string());
        if let Some(command) = args.get("command").and_then(JsonValue::as_str) {
            let command: String = command.chars().take(32).collect();
            entry = entry.add_metadata("command".to_string(), command.to_ascii_uppercase());
        }
        if let Some(count) = args.get("args").and_then(JsonValue::as_array).map(Vec::len) {
            entry = entry.add_metadata("args".to_string(), count.to_string());
        }
        if let Err(e) = result {
            let text: String = e.to_string().chars().take(500).collect();
            entry = entry.add_metadata("error".to_string(), text);
        }
        let entry = crate::mcp::audit_tool_call_context(entry, started);
        if let Err(e) = audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", tool_name, e);
        }
    }
}

/// `command`・`args`を読む。コマンドの名前は大文字にする。引数は、文字列か数
fn parse_command(args: &JsonValue) -> Result<(String, Vec<String>), McpError> {
    let name = args
        .get("command")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| McpError::InvalidParams("Missing 'command' parameter".to_string()))?;
    if !name.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(McpError::InvalidParams(
            "'command' is the name of one Redis command (for example GET); put its arguments in 'args'"
                .to_string(),
        ));
    }
    let values = match args.get("args") {
        None | Some(JsonValue::Null) => Vec::new(),
        Some(JsonValue::Array(values)) => values.clone(),
        Some(_) => {
            return Err(McpError::InvalidParams(
                "'args' must be an array of strings and numbers".to_string(),
            ))
        }
    };
    let values = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| match value {
            JsonValue::String(s) => Ok(s),
            JsonValue::Number(n) => Ok(n.to_string()),
            _ => Err(McpError::InvalidParams(format!(
                "args[{i}] must be a string or a number"
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((name.to_ascii_uppercase(), values))
}

/// 文字列の引数（無ければ`None`）
fn optional_string(args: &JsonValue, key: &str) -> Result<Option<String>, McpError> {
    match args.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(s)) => Ok(Some(s.clone())),
        Some(JsonValue::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(McpError::InvalidParams(format!("'{key}' must be a string"))),
    }
}

/// 長さ`length`のものの、`start`から`stop`まで（Redisの番号。負の数は後ろから）の
/// 数
fn range_size(
    length: usize,
    start: Option<&String>,
    stop: Option<&String>,
) -> Result<usize, McpError> {
    let parse = |value: Option<&String>| {
        value.and_then(|v| v.parse::<i64>().ok()).ok_or_else(|| {
            McpError::InvalidParams("the range needs a start and a stop number".to_string())
        })
    };
    let (start, stop) = (parse(start)?, parse(stop)?);
    let length = i64::try_from(length).unwrap_or(i64::MAX);
    let normalize = |i: i64| if i < 0 { length.saturating_add(i) } else { i };
    let start = normalize(start).max(0);
    let stop = normalize(stop).min(length - 1);
    Ok(usize::try_from(stop.saturating_sub(start).saturating_add(1)).unwrap_or(0))
}

/// Redisの値をJSONにする。文字列はUTF-8なら文字列、そうでなければ
/// `{"base64": "…"}`
fn value_to_json(value: redis::Value) -> JsonValue {
    use redis::Value;
    match value {
        Value::Nil => JsonValue::Null,
        Value::Int(i) => json!(i),
        Value::BulkString(bytes) => match String::from_utf8(bytes) {
            Ok(text) => JsonValue::String(text),
            Err(e) => json!({ "base64": BASE64_STANDARD.encode(e.into_bytes()) }),
        },
        Value::Array(items) | Value::Set(items) => {
            JsonValue::Array(items.into_iter().map(value_to_json).collect())
        }
        Value::SimpleString(text) => JsonValue::String(text),
        Value::Okay => JsonValue::String("OK".to_string()),
        Value::Map(pairs) => JsonValue::Array(
            pairs
                .into_iter()
                .map(|(k, v)| JsonValue::Array(vec![value_to_json(k), value_to_json(v)]))
                .collect(),
        ),
        Value::Attribute { data, .. } => value_to_json(*data),
        Value::Double(f) => serde_json::Number::from_f64(f)
            .map(JsonValue::Number)
            .unwrap_or_else(|| JsonValue::String(f.to_string())),
        Value::Boolean(b) => JsonValue::Bool(b),
        Value::VerbatimString { text, .. } => JsonValue::String(text),
        other => JsonValue::String(format!("{other:?}")),
    }
}

/// `HGETALL`の結果（項目と値の並び）を、オブジェクトにする。項目がUTF-8で
/// なければ、並びのまま
fn pairs_to_object(value: redis::Value) -> JsonValue {
    let json = value_to_json(value);
    let JsonValue::Array(items) = &json else {
        return json;
    };
    if items.len() % 2 != 0 || !items.iter().step_by(2).all(JsonValue::is_string) {
        return json;
    }
    let mut object = serde_json::Map::new();
    for pair in items.chunks(2) {
        if let [JsonValue::String(field), value] = pair {
            object.insert(field.clone(), value.clone());
        }
    }
    JsonValue::Object(object)
}

/// 並びを`max`までにする。切ったら`true`
fn keep_items(value: &mut JsonValue, max: usize) -> bool {
    match value {
        JsonValue::Array(items) if items.len() > max => {
            items.truncate(max);
            true
        }
        _ => false,
    }
}

/// 結果のだいたいの大きさを`max`までにする。並びは後ろから外す。切ったら
/// `true`
fn keep_within_bytes(value: &mut JsonValue, max: usize) -> bool {
    let size = |v: &JsonValue| serde_json::to_string(v).map(|s| s.len()).unwrap_or(0);
    if size(value) <= max {
        return false;
    }
    match value {
        JsonValue::Array(items) => {
            while !items.is_empty() && size(&JsonValue::Array(items.clone())) > max {
                let keep = items.len() / 2;
                items.truncate(keep);
            }
        }
        JsonValue::Object(object) => {
            while !object.is_empty() && size(&JsonValue::Object(object.clone())) > max {
                let keep = object.len() / 2;
                let drop: Vec<String> = object.keys().skip(keep).cloned().collect();
                for key in drop {
                    object.remove(&key);
                }
            }
        }
        _ => *value = JsonValue::Null,
    }
    true
}

/// `INFO`の文（`name:value`の行）を、オブジェクトにする。`keyspace`の
/// `db0:keys=1,expires=0,avg_ttl=0`は、その中もオブジェクトにする
fn parse_info(text: &str) -> serde_json::Map<String, JsonValue> {
    let mut map = serde_json::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = if name.starts_with("db") && value.contains('=') {
            let mut inner = serde_json::Map::new();
            for part in value.split(',') {
                if let Some((k, v)) = part.split_once('=') {
                    let v = v
                        .parse::<i64>()
                        .map(JsonValue::from)
                        .unwrap_or_else(|_| JsonValue::String(v.to_string()));
                    inner.insert(k.to_string(), v);
                }
            }
            JsonValue::Object(inner)
        } else {
            JsonValue::String(value.to_string())
        };
        map.insert(name.to_string(), value);
    }
    map
}

#[async_trait]
impl McpHandler for RedisHandler {
    async fn initialize(&self, params: InitializeParams) -> Result<JsonValue, McpError> {
        Ok(json!({
            "protocolVersion": crate::mcp::negotiate_protocol_version(&params.protocol_version),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "redis-handler", "version": "1.0.0" }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        let max_rows = self.limits.max_items;
        let max_bytes = self.limits.max_result_bytes;
        let read: Vec<&str> = READ_COMMANDS.iter().map(|(c, _)| *c).collect();
        let write: Vec<&str> = WRITE_COMMANDS.iter().map(|(c, _)| *c).collect();
        let args_schema = json!({
            "type": "array",
            "description": "The arguments of the command, in order (key first), as strings or numbers. Binary values are not supported",
            "items": { "type": ["string", "number"] }
        });
        let mut tools = vec![
            Tool {
                name: "redis_command".to_string(),
                title: Some("Run a read-only Redis command".to_string()),
                description: format!(
                    "Run one read-only Redis command on the configured database and return its reply as JSON. Allowed commands: {}. \
                     Commands that return a whole hash or set (HGETALL, HKEYS, HVALS, SMEMBERS) and ranges (LRANGE, ZRANGE) are refused when they would return more than {max_rows} items: use HSCAN, SSCAN, or a narrower range. \
                     ZRANGE with BYSCORE or BYLEX needs LIMIT offset count. XRANGE and XREVRANGE return at most {max_rows} entries (truncated is true when there were more). \
                     Values longer than {max_bytes} bytes are refused (use GETRANGE). Strings that are not UTF-8 are returned as {{\"base64\": \"...\"}}. HGETALL is returned as an object. \
                     To find keys, use scan_keys (KEYS is not available).",
                    read.join(", ")
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The command name, for example GET or HGETALL" },
                        "args": args_schema.clone()
                    },
                    "required": ["command"]
                }),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Tool {
                name: "scan_keys".to_string(),
                title: Some("List Redis keys".to_string()),
                description: format!(
                    "List keys of the configured Redis database with SCAN, without blocking the server. Returns at most {max_rows} keys and a cursor: when done is false, call again with that cursor for more. \
                     pattern is a glob such as user:* (default *), and type limits the keys to one type (string, list, set, zset, hash, stream)."
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob pattern of the keys (default *)" },
                        "type": { "type": "string", "description": "Only keys of this type: string, list, set, zset, hash, or stream" },
                        "cursor": { "type": "string", "description": "The cursor returned by the previous call (default 0, the start)" }
                    }
                }),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
            Tool {
                name: "get_schema".to_string(),
                title: Some("Show the Redis database".to_string()),
                description: "Show the Redis server version, the configured database number, how many keys it has, and the keys of each database (INFO keyspace). Redis has no tables: use scan_keys to list keys.".to_string(),
                input_schema: json!({ "type": "object", "properties": {} }),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ];
        if self.allow_write {
            tools.push(Tool {
                name: "redis_write_command".to_string(),
                title: Some("Change data with a Redis command".to_string()),
                description: format!(
                    "Run one Redis command that changes data on the configured database. Allowed commands: {}. \
                     Commands that remove or change many keys at once (FLUSHDB, FLUSHALL, KEYS, RENAME, EVAL, ...) are not available.",
                    write.join(", ")
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The command name, for example SET or HSET" },
                        "args": args_schema
                    },
                    "required": ["command"]
                }),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            });
        }
        Ok(tools)
    }

    async fn call_tool(&self, params: ToolCallParams) -> Result<JsonValue, McpError> {
        let started = std::time::Instant::now();
        let args = serde_json::to_value(params.arguments.clone().unwrap_or_default())
            .map_err(|e| McpError::InvalidParams(e.to_string()))?;
        let result = match params.name.as_str() {
            "redis_command" => self.handle_command(&args, false).await,
            "redis_write_command" if self.allow_write => self.handle_command(&args, true).await,
            "scan_keys" => self.handle_scan_keys(&args).await,
            "get_schema" => self.handle_get_schema().await,
            _ => {
                return Err(McpError::ToolNotFound(format!(
                    "Unknown tool: {}",
                    params.name
                )))
            }
        };
        self.audit_tool_call(&params.name, &args, &result, started)
            .await;
        let value = result?;
        let text =
            serde_json::to_string_pretty(&value).map_err(|e| McpError::Internal(e.to_string()))?;
        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false
        }))
    }

    async fn status(&self) -> Option<String> {
        Some(self.status_line.clone())
    }

    fn resource_tool(&self, uri: &str) -> Option<String> {
        (uri == "db://schema").then(|| "get_schema".to_string())
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        Ok(vec![Resource {
            uri: "db://schema".to_string(),
            name: "Redis database".to_string(),
            description: Some(
                "The Redis server version and how many keys the database has".to_string(),
            ),
            mime_type: Some("application/json".to_string()),
        }])
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<JsonValue, McpError> {
        match params.uri.as_str() {
            "db://schema" => {
                let schema = self.handle_get_schema().await?;
                let text = serde_json::to_string_pretty(&schema)
                    .map_err(|e| McpError::Internal(e.to_string()))?;
                Ok(json!({
                    "contents": [{
                        "uri": "db://schema",
                        "mimeType": "application/json",
                        "text": text
                    }]
                }))
            }
            _ => Err(McpError::ResourceNotFound(params.uri)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn test_parse_command() {
        let (name, args) =
            parse_command(&json!({"command": "hget", "args": ["user:1", 2]})).unwrap();
        assert_eq!(name, "HGET");
        assert_eq!(args, ["user:1", "2"]);
        for bad in [
            json!({}),
            json!({"command": "GET key"}),
            json!({"command": "GET", "args": "key"}),
            json!({"command": "GET", "args": [{"a": 1}]}),
            json!({"command": "GET", "args": [null]}),
        ] {
            assert!(parse_command(&bad).is_err(), "{bad}");
        }
    }

    /// 危ないコマンドは、どちらの一覧にも無い
    #[test]
    fn test_dangerous_commands_are_not_listed() {
        for name in [
            "KEYS",
            "FLUSHALL",
            "FLUSHDB",
            "CONFIG",
            "EVAL",
            "EVALSHA",
            "SCRIPT",
            "FUNCTION",
            "FCALL",
            "MODULE",
            "SHUTDOWN",
            "DEBUG",
            "MIGRATE",
            "SELECT",
            "CLIENT",
            "ACL",
            "MONITOR",
            "SUBSCRIBE",
            "PSUBSCRIBE",
            "MULTI",
            "EXEC",
            "SAVE",
            "BGSAVE",
            "REPLICAOF",
            "SLAVEOF",
            "SWAPDB",
            "MOVE",
            "RENAME",
            "SORT",
            "BLPOP",
            "WAIT",
            "OBJECT",
            "RESTORE",
            "DUMP",
            "COPY",
        ] {
            assert!(
                READ_COMMANDS
                    .iter()
                    .chain(WRITE_COMMANDS)
                    .all(|(c, _)| *c != name),
                "{name}"
            );
        }
    }

    #[test]
    fn test_range_size() {
        let r = |len, a: &str, b: &str| {
            range_size(len, Some(&a.to_string()), Some(&b.to_string())).unwrap()
        };
        assert_eq!(r(10, "0", "-1"), 10);
        assert_eq!(r(10, "0", "4"), 5);
        assert_eq!(r(10, "-3", "-1"), 3);
        assert_eq!(r(10, "5", "100"), 5);
        assert_eq!(r(10, "8", "2"), 0);
        assert_eq!(r(0, "0", "-1"), 0);
        assert!(range_size(10, Some(&"a".to_string()), None).is_err());
    }

    #[test]
    fn test_value_to_json() {
        use redis::Value;
        assert_eq!(value_to_json(Value::Nil), JsonValue::Null);
        assert_eq!(value_to_json(Value::Int(3)), json!(3));
        assert_eq!(
            value_to_json(Value::BulkString(b"hi".to_vec())),
            json!("hi")
        );
        assert_eq!(
            value_to_json(Value::BulkString(vec![0xff, 0x00])),
            json!({"base64": "/wA="})
        );
        assert_eq!(value_to_json(Value::Okay), json!("OK"));
        assert_eq!(
            pairs_to_object(Value::Array(vec![
                Value::BulkString(b"name".to_vec()),
                Value::BulkString(b"a".to_vec()),
            ])),
            json!({"name": "a"})
        );
    }

    #[test]
    fn test_keep_within_bytes() {
        let mut items = json!(["aaaa", "bbbb", "cccc", "dddd"]);
        assert!(keep_within_bytes(&mut items, 16));
        assert!(serde_json::to_string(&items).unwrap().len() <= 16);
        let mut small = json!(["a"]);
        assert!(!keep_within_bytes(&mut small, 16));
        assert_eq!(small, json!(["a"]));
    }

    #[test]
    fn test_parse_info() {
        let info = parse_info("# Keyspace\r\ndb0:keys=3,expires=1,avg_ttl=0\r\n");
        assert_eq!(info["db0"], json!({"keys": 3, "expires": 1, "avg_ttl": 0}));
        let server = parse_info("# Server\r\nredis_version:7.2.4\r\nredis_mode:standalone\r\n");
        assert_eq!(server["redis_version"], json!("7.2.4"));
    }

    #[test]
    fn test_check_settings() {
        let settings = |host: &str, mode: Option<&str>| DatabaseSettings {
            engine: "redis".to_string(),
            host: Some(host.to_string()),
            ssl_mode: mode.map(str::to_string),
            ..Default::default()
        };
        assert_eq!(check_settings(&settings("localhost", None)).unwrap(), None);
        let err = check_settings(&settings("cache.example.com", None)).unwrap_err();
        assert!(err.contains("ssl_mode"), "{err}");
        assert!(
            check_settings(&settings("cache.example.com", Some("verify-full")))
                .unwrap()
                .is_none()
        );
        assert!(
            check_settings(&settings("cache.example.com", Some("require")))
                .unwrap()
                .unwrap()
                .contains("not checked")
        );
        assert!(
            check_settings(&settings("cache.example.com", Some("disable")))
                .unwrap()
                .unwrap()
                .contains("not encrypted")
        );
        assert!(check_settings(&settings("localhost", Some("verify-ca"))).is_err());
        let _ = s(&[]);
    }
}
