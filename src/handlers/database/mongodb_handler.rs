//! MongoDBのツール（`[database]`の`engine = "mongodb"`、#530）
//!
//! MongoDBはSQLではないので、SQLのツール（`execute_query`等）とは別の、操作
//! ごとのツールを出す。
//!
//! - `find`: フィルター・射影・並べ替え・件数で、ドキュメントを読む
//! - `aggregate`: 集約のパイプラインで読む
//! - `count_documents`: フィルターに合うドキュメントの数
//! - `get_schema`: コレクションの一覧と、そのドキュメントの数、数件から読んだ
//!   フィールドの名前と型
//! - `insert_documents`・`update_documents`・`delete_documents`: `allow_write = true`の
//!   ときだけ出す
//!
//! 安全性:
//!
//! - 集約の書き込みの段（`$out`・`$merge`）と、JavaScriptを動かす演算子
//!   （`$where`・`$function`・`$accumulator`）は、どのツールでも断る（フィルター・
//!   射影・並べ替え・更新の中も見る）
//! - 返す量の上限。`find`の件数は`max_rows`まで、`aggregate`には`$limit`を
//!   足す。ドキュメントは少しずつ受け取り、`max_result_bytes`を超えたら残りを
//!   読まない（カーソルを閉じる）
//! - 1つの操作の時間の上限（`query_timeout_seconds`）。読み取りは、サーバーの
//!   `maxTimeMS`と、こちらで待つ時間。`find`・`aggregate`・`get_schema`は、初めの
//!   応答から読み終えるまでの全体で1つの上限。書き込みには`maxTimeMS`を
//!   送れない（ドライバーに無い）ので、こちらで待つ時間だけ。時間切れの
//!   書き込みは、当たったかどうか分からない
//! - ドライバーの操作は別のタスクで動かし、待つのをやめても最後まで動かす
//!   （ドライバーのfutureを途中で落とすと、内部の状態が壊れることがある）
//! - 書いたホストにだけつなぐ（`directConnection`）。レプリカセットの他の
//!   メンバーを探してつなぐと、`ssl_mode`の確認を通っていないホストに、認証の
//!   情報とデータを送ることになるため
//! - `update_documents`・`delete_documents`は、空のフィルター（全部のドキュメント）を
//!   断る。`insert_documents`の数は`max_rows`まで。コレクション・インデックスの
//!   作成・削除はしない。`insert_documents`は、あるコレクションにだけ挿入する
//!   （MongoDBは、無いコレクションへの挿入でコレクションを作るため）
//! - 監査ログには、ツールの名前とコレクションの名前だけを書く（値は書かない）

use crate::config::DatabaseSettings;
use crate::handlers::database::engines::{is_loopback_host, ssl_mode_missing};
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolAnnotations,
    ToolCallParams,
};
use async_trait::async_trait;
use futures::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::{ClientOptions, Credential, ServerAddress, Tls, TlsOptions};
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

/// サーバーを選ぶ・つなぐときの待ち時間の上限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// サーバーの時間の上限（`maxTimeMS`）より、こちらで待つ時間を少し長くする
const CLIENT_TIMEOUT_GRACE: Duration = Duration::from_secs(2);

/// `get_schema`で、コレクションごとにフィールドを読むドキュメントの数
const SCHEMA_SAMPLE: i64 = 20;

/// `get_schema`で、残りの時間がこれより短ければ、次のコレクションを読まない
/// （`maxTimeMS`は、0だと上限なしになる）
const SCHEMA_MIN_REMAINING: Duration = Duration::from_millis(10);

/// どのツールでも断る、集約の段・演算子（#530）
const REFUSED_OPERATORS: &[(&str, &str)] = &[
    ("$out", "writes the result to a collection"),
    ("$merge", "writes the result to a collection"),
    ("$where", "runs JavaScript on the server"),
    ("$function", "runs JavaScript on the server"),
    ("$accumulator", "runs JavaScript on the server"),
];

/// ツールの設定
#[derive(Clone, Debug)]
struct Limits {
    max_items: usize,
    max_result_bytes: usize,
    timeout: Duration,
}

/// MongoDBのハンドラー（#530）
pub struct MongoHandler {
    database: mongodb::Database,
    allow_write: bool,
    limits: Limits,
    /// ドライバーの操作を動かしているタスクの数の上限（接続のプールの大きさ。
    /// #808）。待つのをやめた後も、タスクは最後まで動き、接続を持ち続ける。
    /// その間に新しいタスクを積み上げないよう、空きが無ければ断る。
    ///
    /// 枠が空くのは、サーバーが答えるか、接続が切れたとき。TCPのkeepalive
    /// （ドライバーは120秒）が切るのは、相手が消えた接続だけで、サーバーが
    /// 生きていて答えない操作（ロックを待つ等）は切らない。そうした操作で枠が
    /// 埋まったままなら、サーバーで操作を止める（`killOp`）か、mcp-rsを
    /// 起動し直す（PR #817のレビュー）
    busy: Arc<tokio::sync::Semaphore>,
    status_line: String,
    audit_logger: Option<Arc<crate::security::AuditLogger>>,
}

/// `[database]`を、つながずに確かめる（`mcp-rs check`、#589）。通信を守れない
/// 設定なら、その警告を返す
pub fn check_settings(settings: &DatabaseSettings) -> Result<Option<String>, String> {
    settings.validate()?;
    // ユーザー名だけでパスワードが無ければ、認証できない（Redisと同じく、
    // 黙って認証せずにつながないよう、先に断る）
    if !settings.username.trim().is_empty()
        && settings.password.as_deref().is_none_or(str::is_empty)
    {
        return Err(format!(
            "database.username = \"{}\" needs a password (password or keyring)",
            settings.username.trim()
        ));
    }
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

/// `[database]`から、MongoDBの接続の設定を作る
fn client_options(settings: &DatabaseSettings) -> ClientOptions {
    let mut options = ClientOptions::default();
    options.hosts = vec![ServerAddress::Tcp {
        host: settings.host().to_string(),
        port: Some(settings.port()),
    }];
    // 書いたホストにだけつなぐ。既定（探す）だと、レプリカセットが知らせる
    // 他のメンバーにもつなぐので、`check_settings`が見ていないホストに、TLS
    // なしで認証の情報とデータを送ることがある
    options.direct_connection = Some(true);
    options.app_name = Some(format!("mcp-rs {}", env!("CARGO_PKG_VERSION")));
    options.connect_timeout = Some(CONNECT_TIMEOUT);
    options.server_selection_timeout = Some(CONNECT_TIMEOUT);
    options.max_pool_size = Some(settings.max_connections());
    if !settings.username.trim().is_empty() {
        options.credential = Some(
            Credential::builder()
                .username(settings.username.trim().to_string())
                .password(settings.password.clone().unwrap_or_default())
                .source(
                    settings
                        .auth_source
                        .clone()
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or_else(|| "admin".to_string()),
                )
                .build(),
        );
    }
    options.tls = match settings.ssl_mode.as_deref() {
        Some("require") => Some(Tls::Enabled(
            TlsOptions::builder()
                .allow_invalid_certificates(true)
                .build(),
        )),
        Some("verify-full") => Some(Tls::Enabled(
            TlsOptions::builder()
                .ca_file_path(
                    settings
                        .ssl_root_cert
                        .as_deref()
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .map(std::path::PathBuf::from),
                )
                .build(),
        )),
        _ => Some(Tls::Disabled),
    };
    options
}

impl MongoHandler {
    /// `[database]`から作り、つながるかを確かめる。失敗したら、起動時のログと
    /// `mcp_rs_status`に出す理由を返す（パスワードは含めない）
    pub async fn from_settings(settings: &DatabaseSettings) -> Result<Self, String> {
        if let Some(warning) = check_settings(settings)? {
            warn!("{}", warning);
        }
        // rustlsの暗号の実装を決める（MySQL・Redisと同じ。既に決まっていれば
        // 何もしない）
        let _ = rustls::crypto::ring::default_provider().install_default();
        let describe = settings.describe();
        let client = mongodb::Client::with_options(client_options(settings))
            .map_err(|e| format!("cannot set up {describe}: {e}"))?;
        let database = client.database(settings.database.trim());
        // 認証とサーバーの選択は、最初の操作で行われる。応えるかを確かめる
        let ping = {
            let database = database.clone();
            async move { database.run_command(doc! { "ping": 1 }).await }
        };
        run_to_completion(CONNECT_TIMEOUT + CLIENT_TIMEOUT_GRACE, ping)
            .await
            .map_err(|_| {
                format!(
                    "cannot connect to {describe}: no answer within {} seconds",
                    CONNECT_TIMEOUT.as_secs()
                )
            })?
            .map_err(|e| format!("cannot connect to {describe}: {e}"))?;
        Ok(Self {
            database,
            allow_write: settings.allow_write(),
            busy: Arc::new(tokio::sync::Semaphore::new(
                usize::try_from(settings.max_connections())
                    .unwrap_or(usize::MAX)
                    .clamp(1, tokio::sync::Semaphore::MAX_PERMITS),
            )),
            limits: Limits {
                max_items: settings.max_rows() as usize,
                max_result_bytes: usize::try_from(settings.max_result_bytes())
                    .unwrap_or(usize::MAX),
                timeout: Duration::from_secs(u64::from(settings.query_timeout_seconds())),
            },
            status_line: describe,
            audit_logger: None,
        })
    }

    /// ツール呼び出しを監査ログに記録する（ビルダースタイル）
    pub fn with_audit_logger(mut self, audit_logger: Arc<crate::security::AuditLogger>) -> Self {
        self.audit_logger = Some(audit_logger);
        self
    }

    /// ドライバーの操作を別のタスクで動かし、こちらで待つ時間の上限をつけて待つ。
    /// 時間切れ・タスクの失敗は`Err`、ドライバーの結果は`Ok`の中
    ///
    /// ドライバーのfutureは、途中で落とすと内部の状態が壊れることがある
    /// （ドライバーのREADME）。だから、待つのをやめても、タスクは最後まで動かす。
    /// `write`のときは、時間切れでも書き込みが当たっているかもしれないと伝える
    async fn run<T: Send + 'static>(
        &self,
        what: &str,
        write: bool,
        future: impl std::future::Future<Output = mongodb::error::Result<T>> + Send + 'static,
    ) -> Result<mongodb::error::Result<T>, McpError> {
        let future = self.hold_slot(what, future)?;
        let seconds = self.limits.timeout.as_secs();
        match run_to_completion(self.limits.timeout + CLIENT_TIMEOUT_GRACE, future).await {
            Ok(result) => Ok(result),
            Err(Stopped::TimedOut) if write => Err(McpError::ExternalApi(format!(
                "MongoDB {what} did not answer within {seconds} seconds. The outcome is unknown: the write may already be applied, or the server may still apply it later, so a read that does not show it does not prove that it failed. Do not send it again unless you are sure that applying it twice leaves the data as you want it"
            ))),
            Err(Stopped::TimedOut) => Err(McpError::ExternalApi(format!(
                "MongoDB {what} did not finish within {seconds} seconds"
            ))),
            Err(Stopped::Failed) => Err(McpError::Internal(format!(
                "MongoDB {what}: the task stopped"
            ))),
        }
    }

    /// `future`を、タスクの枠（[`MongoHandler::busy`]）を1つ持ったまま動く
    /// ものにする。枠が空いていなければ、断る（#808）
    fn hold_slot<T>(
        &self,
        what: &str,
        future: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Result<impl std::future::Future<Output = T> + Send + 'static, McpError> {
        let slot = self.busy.clone().try_acquire_owned().map_err(|_| {
            McpError::ExternalApi(format!(
                "MongoDB {what} was not started: every connection of the pool is still used by earlier operations that mcp-rs stopped waiting for (the server has not answered them yet). Try again later. If this does not clear, stop those operations on the server (killOp) or restart mcp-rs"
            ))
        })?;
        Ok(async move {
            let result = future.await;
            drop(slot);
            result
        })
    }

    /// 読み取りの操作を待つ
    async fn wait<T: Send + 'static>(
        &self,
        what: &str,
        future: impl std::future::Future<Output = mongodb::error::Result<T>> + Send + 'static,
    ) -> Result<T, McpError> {
        self.run(what, false, future)
            .await?
            .map_err(|e| McpError::ExternalApi(format!("MongoDB {what}: {e}")))
    }

    /// 書き込みの操作を待つ。書き込みには`maxTimeMS`を送れない（ドライバーに
    /// 無い）ので、時間切れのあとも、サーバーは続けることがある
    ///
    /// ドライバーのエラーが、書かれていないと分かるもの（[`write_not_applied`]）
    /// でなければ、成否が分からないと伝える（#808。書いた後に接続が切れると、
    /// 送り直しで二重に書く）。`many`（`updateMany`・`deleteMany`）では、
    /// サーバーが途中で止めたエラーでも、それまでのドキュメントは変わって
    /// いる（PR #817のレビュー）
    async fn wait_write<T: Send + 'static>(
        &self,
        what: &str,
        many: bool,
        future: impl std::future::Future<Output = mongodb::error::Result<T>> + Send + 'static,
    ) -> Result<T, McpError> {
        self.run(what, true, future).await?.map_err(|e| {
            if write_not_applied(&e, many) {
                McpError::ExternalApi(format!("MongoDB {what}: {e}"))
            } else if many && write_rejected_by_server(&e) {
                McpError::ExternalApi(format!(
                    "MongoDB {what}: {e}. The server stopped part way: the documents handled before the error stay changed. Check them before sending it again (sending it again changes them again)"
                ))
            } else {
                McpError::ExternalApi(format!(
                    "MongoDB {what}: {e}. The outcome is unknown: the server may have applied the write before the answer was lost. Do not send it again unless you are sure that applying it twice leaves the data as you want it"
                ))
            }
        })
    }

    fn collection(&self, args: &JsonValue) -> Result<mongodb::Collection<Document>, McpError> {
        Ok(self.database.collection(&collection_name(args)?))
    }

    /// `find`
    async fn handle_find(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let collection = self.collection(args)?;
        let filter = document_arg(args, "filter")?.unwrap_or_default();
        let projection = document_arg(args, "projection")?;
        let sort = document_arg(args, "sort")?;
        let max = self.limits.max_items;
        let limit = match args.get("limit") {
            None | Some(JsonValue::Null) => max,
            Some(value) => value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .filter(|n| (1..=max).contains(n))
                .ok_or_else(|| {
                    McpError::InvalidParams(format!(
                        "limit must be a number from 1 to max_rows ({max})"
                    ))
                })?,
        };
        let skip = match args.get("skip") {
            None | Some(JsonValue::Null) => None,
            Some(value) => Some(value.as_u64().ok_or_else(|| {
                McpError::InvalidParams("skip must be a number of 0 or more".to_string())
            })?),
        };
        let timeout = self.limits.timeout;
        let max_bytes = self.limits.max_result_bytes;
        // 初めの応答とカーソルの読み取りを、1つの時間の上限の中で行う
        let (documents, truncated) = self
            .wait("find", async move {
                // 1つ多く読み、超えたかを知る
                let mut find = collection
                    .find(filter)
                    .limit(i64::try_from(limit + 1).unwrap_or(i64::MAX))
                    .max_time(timeout)
                    .batch_size(u32::try_from(limit.min(1000) + 1).unwrap_or(1000));
                if let Some(projection) = projection {
                    find = find.projection(projection);
                }
                if let Some(sort) = sort {
                    find = find.sort(sort);
                }
                if let Some(skip) = skip {
                    find = find.skip(skip);
                }
                let cursor = find.await?;
                read_documents(cursor, limit, max_bytes).await
            })
            .await?;
        Ok(json!({
            "documents": documents,
            "truncated": truncated,
            "max_rows": max,
            "max_result_bytes": self.limits.max_result_bytes,
        }))
    }

    /// `aggregate`
    async fn handle_aggregate(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let collection = self.collection(args)?;
        let stages = match args.get("pipeline") {
            Some(JsonValue::Array(stages)) => stages,
            _ => {
                return Err(McpError::InvalidParams(
                    "'pipeline' must be an array of stages".to_string(),
                ))
            }
        };
        let mut pipeline = Vec::new();
        for (i, stage) in stages.iter().enumerate() {
            let stage = to_document(stage, &format!("pipeline[{i}]"))?;
            if stage.len() != 1 {
                return Err(McpError::InvalidParams(format!(
                    "pipeline[{i}] must have exactly one stage name, such as {{\"$match\": {{...}}}}"
                )));
            }
            pipeline.push(stage);
        }
        let max = self.limits.max_items;
        // 1つ多く読み、超えたかを知る
        pipeline.push(doc! { "$limit": i64::try_from(max + 1).unwrap_or(i64::MAX) });
        let timeout = self.limits.timeout;
        let max_bytes = self.limits.max_result_bytes;
        // 初めの応答とカーソルの読み取りを、1つの時間の上限の中で行う
        let (documents, truncated) = self
            .wait("aggregate", async move {
                let cursor = collection
                    .aggregate(pipeline)
                    .max_time(timeout)
                    .batch_size(u32::try_from(max.min(1000) + 1).unwrap_or(1000))
                    .await?;
                read_documents(cursor, max, max_bytes).await
            })
            .await?;
        Ok(json!({
            "documents": documents,
            "truncated": truncated,
            "max_rows": max,
            "max_result_bytes": self.limits.max_result_bytes,
        }))
    }

    /// `count_documents`
    async fn handle_count(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let collection = self.collection(args)?;
        let filter = document_arg(args, "filter")?.unwrap_or_default();
        let timeout = self.limits.timeout;
        let count = self
            .wait("count_documents", async move {
                collection.count_documents(filter).max_time(timeout).await
            })
            .await?;
        Ok(json!({ "count": count }))
    }

    /// `get_schema`: コレクションの一覧と、そのドキュメントの数・フィールド。
    /// 全体で1つの時間の上限（`query_timeout_seconds`）を使う
    async fn handle_get_schema(&self) -> Result<JsonValue, McpError> {
        let limits = self.limits.clone();
        let progress: SchemaProgress = Arc::new(std::sync::Mutex::new(SchemaBuilder::new(
            self.database.name(),
            limits.max_result_bytes,
        )));
        let task = self.hold_slot(
            "get_schema",
            read_schema(self.database.clone(), limits.clone(), progress.clone()),
        )?;
        schema_within(
            limits.timeout + CLIENT_TIMEOUT_GRACE,
            limits.timeout.as_secs(),
            progress,
            task,
        )
        .await
    }

    /// `insert_documents`
    async fn handle_insert(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let collection = self.collection(args)?;
        let documents = match args.get("documents") {
            Some(JsonValue::Array(documents)) if !documents.is_empty() => documents,
            _ => {
                return Err(McpError::InvalidParams(
                    "'documents' must be a non-empty array of documents".to_string(),
                ))
            }
        };
        let max = self.limits.max_items;
        if documents.len() > max {
            return Err(McpError::InvalidParams(format!(
                "insert_documents takes at most max_rows ({max}) documents at a time"
            )));
        }
        let documents = documents
            .iter()
            .enumerate()
            .map(|(i, d)| to_document(d, &format!("documents[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let total = documents.len();
        let name = collection.name().to_string();
        let database = self.database.clone();
        // MongoDBは、無いコレクションへの挿入でコレクションを作る。名前の
        // 打ち間違いで作らないよう、あるコレクションにだけ挿入する（確かめた
        // あとに他の誰かが消した場合だけは、作られる）。`authorizedCollections`:
        // コレクションごとの権限だけのユーザーでも、確かめられる
        let outcome = self
            .run("insertMany", true, async move {
                let found = database
                    .list_collection_names()
                    .filter(doc! { "name": collection.name() })
                    .authorized_collections(true)
                    .await?;
                if found.is_empty() {
                    return Ok(None);
                }
                collection.insert_many(documents).await.map(Some)
            })
            .await?;
        let result = match outcome {
            Ok(Some(result)) => result,
            Ok(None) => {
                return Err(McpError::InvalidParams(format!(
                "collection '{name}' does not exist: insert_documents does not create collections"
            )))
            }
            Err(e) => return Err(McpError::ExternalApi(insert_error(&e, total))),
        };
        let mut ids: Vec<(usize, Bson)> = result.inserted_ids.into_iter().collect();
        ids.sort_by_key(|(i, _)| *i);
        Ok(json!({
            "inserted_count": ids.len(),
            "inserted_ids": ids.into_iter().map(|(_, id)| id.into_relaxed_extjson()).collect::<Vec<_>>(),
        }))
    }

    /// `update_documents`
    async fn handle_update(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let collection = self.collection(args)?;
        let filter = required_filter(args)?;
        let update = match args.get("update") {
            Some(JsonValue::Array(stages)) if !stages.is_empty() => {
                mongodb::options::UpdateModifications::Pipeline(
                    stages
                        .iter()
                        .enumerate()
                        .map(|(i, s)| to_document(s, &format!("update[{i}]")))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }
            Some(value @ JsonValue::Object(_)) => {
                let update = to_document(value, "update")?;
                if update.is_empty() || !update.keys().all(|k| k.starts_with('$')) {
                    return Err(McpError::InvalidParams(
                        "'update' must use update operators such as {\"$set\": {...}} (a document without them would replace the whole document)"
                            .to_string(),
                    ));
                }
                mongodb::options::UpdateModifications::Document(update)
            }
            _ => {
                return Err(McpError::InvalidParams(
                    "'update' must be a document of update operators or a pipeline".to_string(),
                ))
            }
        };
        let many = args
            .get("many")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        let result = if many {
            self.wait_write("updateMany", true, async move {
                collection.update_many(filter, update).await
            })
            .await?
        } else {
            self.wait_write("updateOne", false, async move {
                collection.update_one(filter, update).await
            })
            .await?
        };
        Ok(json!({
            "matched_count": result.matched_count,
            "modified_count": result.modified_count,
        }))
    }

    /// `delete_documents`
    async fn handle_delete(&self, args: &JsonValue) -> Result<JsonValue, McpError> {
        let collection = self.collection(args)?;
        let filter = required_filter(args)?;
        let many = args
            .get("many")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        let result = if many {
            self.wait_write("deleteMany", true, async move {
                collection.delete_many(filter).await
            })
            .await?
        } else {
            self.wait_write("deleteOne", false, async move {
                collection.delete_one(filter).await
            })
            .await?
        };
        Ok(json!({ "deleted_count": result.deleted_count }))
    }

    /// ツール呼び出しを監査ログに記録する。ツールとコレクションの名前だけを
    /// 書き、フィルター・値は書かない
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
        if let Ok(name) = collection_name(args) {
            entry = entry.add_metadata("collection".to_string(), name);
        }
        if let Err(e) = result {
            let kind = match e {
                McpError::InvalidParams(_) => "refused by mcp-rs",
                McpError::ExternalApi(_) => "MongoDB returned an error",
                _ => "failed",
            };
            entry = entry.add_metadata("error".to_string(), kind.to_string());
        }
        let entry = crate::mcp::audit_tool_call_context(entry, started);
        if let Err(e) = audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", tool_name, e);
        }
    }
}

/// 書き込みのエラーが、書かれていないと分かるものか（#808）。送る前に
/// 止まった（サーバーを選べない・引数・BSONにできない・認証）とき。1つの
/// ドキュメントの書き込み（`many`でない）では、サーバーが断ったとき
/// （[`write_rejected_by_server`]）も。複数のドキュメントの書き込みは、
/// サーバーが途中で止めても、それまでの分は変わっている（PR #817の
/// レビュー）。通信のエラー、`writeConcern`のエラー（主には書けている）等は、
/// 成否が分からない
fn write_not_applied(error: &mongodb::error::Error, many: bool) -> bool {
    use mongodb::error::ErrorKind;
    if error.contains_label("RetryableWriteError") {
        return false;
    }
    let not_sent = matches!(
        error.kind.as_ref(),
        ErrorKind::ServerSelection { .. }
            | ErrorKind::InvalidArgument { .. }
            | ErrorKind::BsonSerialization(_)
            | ErrorKind::Authentication { .. }
    );
    not_sent || (!many && write_rejected_by_server(error))
}

/// サーバーが、書き込みのエラー・コマンドのエラーで答えたか（再送してよいと
/// 付いたエラーは除く）
fn write_rejected_by_server(error: &mongodb::error::Error) -> bool {
    use mongodb::error::{ErrorKind, WriteFailure};
    !error.contains_label("RetryableWriteError")
        && matches!(
            error.kind.as_ref(),
            ErrorKind::Write(WriteFailure::WriteError(_)) | ErrorKind::Command(_)
        )
}

/// `run_to_completion`が、結果を得られなかった理由
#[derive(Debug, PartialEq)]
enum Stopped {
    /// 待つ時間の上限を超えた（タスクは続いている）
    TimedOut,
    /// タスクが止まった（パニック等）
    Failed,
}

/// ドライバーの操作を別のタスクで最後まで動かし、`limit`まで待つ。
///
/// `tokio::time::timeout`でドライバーのfutureを落とすと、ドライバーの内部の
/// 状態が壊れることがある（ドライバーのREADMEの注意）。待つのはタスクの
/// ハンドルにして、時間切れのあとも、タスクは最後まで動かす
async fn run_to_completion<T: Send + 'static>(
    limit: Duration,
    future: impl std::future::Future<Output = T> + Send + 'static,
) -> Result<T, Stopped> {
    match tokio::time::timeout(limit, tokio::spawn(future)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(Stopped::Failed),
        Err(_) => Err(Stopped::TimedOut),
    }
}

/// カーソルから、件数・バイト数の上限まで読む。上限で止めたら`true`
async fn read_documents(
    mut cursor: mongodb::Cursor<Document>,
    max_items: usize,
    max_bytes: usize,
) -> mongodb::error::Result<(Vec<JsonValue>, bool)> {
    let mut documents = Vec::new();
    let mut bytes = 0usize;
    while let Some(document) = cursor.try_next().await? {
        let value = Bson::Document(document).into_relaxed_extjson();
        bytes += serde_json::to_string(&value).map(|s| s.len()).unwrap_or(0);
        if documents.len() >= max_items || bytes > max_bytes {
            // 1つ多く読めたら、上限を超えている。残りは読まない
            // （カーソルは、落とすと閉じる）
            return Ok((documents, true));
        }
        documents.push(value);
    }
    Ok((documents, false))
}

/// `get_schema`の結果を、`max_result_bytes`に収めながら作る。
///
/// 読むタスクと、待つ側とで分け合う（`Mutex`の中）。待つ側は、時間切れのとき、
/// そこまでに入ったものを返す。バイト数は、返す形（字下げつきのJSON）で数える
struct SchemaBuilder {
    database: String,
    collections: Vec<JsonValue>,
    /// 残りのバイト数
    left: usize,
    /// コレクションの名前の一覧を読み終えたか
    listed: bool,
    truncated: bool,
}

/// 字下げつきのJSONの中で、`value`が`indent`個の空白の位置の要素として使う
/// バイト数（前の改行と、後ろのコンマを含む）
fn nested_len(value: &JsonValue, indent: usize) -> usize {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    text.len() + text.lines().count() * indent + 2
}

impl SchemaBuilder {
    fn new(database: &str, max_bytes: usize) -> Self {
        let mut builder = Self {
            database: database.to_string(),
            collections: Vec::new(),
            left: 0,
            listed: false,
            truncated: false,
        };
        // 外側（コレクション以外）の分を、先に引く。3は、`collections`が空で
        // ないときの、閉じ括弧の前の改行と字下げ
        let envelope =
            serde_json::to_string_pretty(&builder.result(false)).map_or(0, |text| text.len() + 3);
        builder.left = max_bytes.saturating_sub(envelope);
        builder
    }

    /// 1つのコレクションを足す。`fields_unread`は、ドキュメントを読み切れなかった
    /// （権限が無い・途中で失敗した）とき。フィールドが入りきらなければ、入った分と
    /// `fields_truncated`を出す。名前も入らなければ、何も足さない。全部が
    /// 入ったら`true`（`false`なら、もう足せない）
    fn push(
        &mut self,
        name: &str,
        count: Option<u64>,
        fields: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
        fields_unread: bool,
    ) -> bool {
        // `fields_truncated`がある形で数える（無ければ、その分だけ余る）
        let mut entry = json!({
            "name": name,
            "estimated_documents": count,
            "fields": [],
            "fields_truncated": true,
        });
        if fields_unread {
            entry["fields_unread"] = JsonValue::Bool(true);
        }
        // 7は、`fields`が空でないときの、閉じ括弧の前の改行と字下げ
        let mut size = nested_len(&entry, 4) + 7;
        if size > self.left {
            self.truncated = true;
            return false;
        }
        let mut listed = Vec::new();
        let mut complete = true;
        for (field, types) in fields {
            let value = json!({ "name": field, "types": types });
            let len = nested_len(&value, 8);
            if size + len > self.left {
                complete = false;
                break;
            }
            size += len;
            listed.push(value);
        }
        entry["fields"] = JsonValue::Array(listed);
        if complete {
            if let Some(map) = entry.as_object_mut() {
                map.remove("fields_truncated");
            }
        } else {
            self.truncated = true;
        }
        self.left -= size;
        self.collections.push(entry);
        complete
    }

    /// そこまでの結果。`timed_out`は、時間が尽きて途中でやめたとき
    fn result(&self, timed_out: bool) -> JsonValue {
        json!({
            "engine": "mongodb",
            "database": self.database,
            "collections": self.collections,
            "truncated": self.truncated || timed_out,
            "timed_out": timed_out,
            "note": format!("fields are read from up to {SCHEMA_SAMPLE} documents of each collection, so rare fields may be missing. fields_unread is true when the documents of a collection could not all be read (for example no permission), so its fields may be missing. truncated is true when collections or fields were left out (max_rows, max_result_bytes, or the time limit when timed_out is true)"),
        })
    }
}

type SchemaProgress = Arc<std::sync::Mutex<SchemaBuilder>>;

fn lock_schema(progress: &SchemaProgress) -> std::sync::MutexGuard<'_, SchemaBuilder> {
    progress
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `get_schema`のコレクションを読み、`progress`に足していく。全体で
/// `limits.timeout`まで。時間が尽きてやめたら`true`
async fn read_schema(
    database: mongodb::Database,
    limits: Limits,
    progress: SchemaProgress,
) -> mongodb::error::Result<bool> {
    let deadline = std::time::Instant::now() + limits.timeout;
    let remaining = || deadline.saturating_duration_since(std::time::Instant::now());
    // `authorizedCollections`: コレクションごとの権限だけのユーザーでも読める
    let mut names = database
        .list_collection_names()
        .authorized_collections(true)
        .await?;
    names.retain(|name| !name.starts_with("system."));
    names.sort();
    {
        let mut progress = lock_schema(&progress);
        progress.listed = true;
        progress.truncated = names.len() > limits.max_items;
    }
    names.truncate(limits.max_items);
    for name in names {
        if remaining() < SCHEMA_MIN_REMAINING {
            return Ok(true);
        }
        let collection = database.collection::<Document>(&name);
        let count = collection
            .estimated_document_count()
            .max_time(remaining().max(SCHEMA_MIN_REMAINING))
            .await
            .ok();
        if remaining() < SCHEMA_MIN_REMAINING {
            return Ok(true);
        }
        // 数件のドキュメントから、トップレベルのフィールドの名前と型を集める
        // （値は返さない）。読み切れなければ（権限が無い・途中で失敗した）、
        // 読めた分と`fields_unread`を出す
        let mut fields: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let mut fields_unread = false;
        match collection
            .find(doc! {})
            .limit(SCHEMA_SAMPLE)
            .max_time(remaining().max(SCHEMA_MIN_REMAINING))
            .await
        {
            Ok(mut cursor) => loop {
                match cursor.try_next().await {
                    Ok(Some(document)) => {
                        for (field, value) in &document {
                            fields
                                .entry(field.clone())
                                .or_default()
                                .insert(bson_type(value).to_string());
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        fields_unread = true;
                        break;
                    }
                }
            },
            Err(_) => fields_unread = true,
        }
        if remaining().is_zero() {
            // 時間切れで読み終えていないかもしれないコレクションは、出さない
            return Ok(true);
        }
        if !lock_schema(&progress).push(&name, count, fields, fields_unread) {
            break;
        }
    }
    Ok(false)
}

/// `task`（`read_schema`）を`limit`まで待ち、`progress`の結果を返す。
///
/// こちらで待つ時間が尽きたら（サーバーが応えない等）、そこまでに入った
/// ものを`timed_out`で返す。名前の一覧も読めていなければ、返すものが無いので
/// エラー。タスクは、待つのをやめても最後まで動く
async fn schema_within(
    limit: Duration,
    seconds: u64,
    progress: SchemaProgress,
    task: impl std::future::Future<Output = mongodb::error::Result<bool>> + Send + 'static,
) -> Result<JsonValue, McpError> {
    match run_to_completion(limit, task).await {
        Ok(Ok(timed_out)) => Ok(lock_schema(&progress).result(timed_out)),
        Ok(Err(e)) => Err(McpError::ExternalApi(format!("MongoDB get_schema: {e}"))),
        Err(Stopped::TimedOut) => {
            let progress = lock_schema(&progress);
            if progress.listed {
                Ok(progress.result(true))
            } else {
                Err(McpError::ExternalApi(format!(
                    "MongoDB get_schema did not list the collections within {seconds} seconds"
                )))
            }
        }
        Err(Stopped::Failed) => Err(McpError::Internal(
            "MongoDB get_schema: the task stopped".to_string(),
        )),
    }
}

/// `insert_many`のエラーの文。順に挿入するので、途中で失敗したら、その前の
/// ドキュメントは挿入されている。何件が入ったかを伝える（同じものを送り直すと
/// 二重に入るため）
fn insert_error(error: &mongodb::error::Error, total: usize) -> String {
    use mongodb::error::{ErrorKind, InsertManyError};
    if let ErrorKind::InsertMany(InsertManyError {
        write_errors: Some(errors),
        ..
    }) = error.kind.as_ref()
    {
        if let Some(first) = errors.iter().min_by_key(|e| e.index) {
            return partial_insert_message(first.index, total, &first.message);
        }
    }
    format!("MongoDB insertMany: {error}. If the connection was lost, some documents may have been inserted, or may still be inserted. Do not send them again unless you are sure that the collection refuses a second copy (a unique index on a key of the documents)")
}

/// `failed`番目（0から）のドキュメントで挿入が止まったときの文
fn partial_insert_message(failed: usize, total: usize, message: &str) -> String {
    let before = if failed == 0 {
        "No document was inserted".to_string()
    } else {
        format!(
            "{failed} of {total} documents were inserted (documents[0] to documents[{}]): do not send those again",
            failed - 1
        )
    };
    format!("MongoDB insertMany: documents[{failed}] failed: {message}. {before}. The documents after documents[{failed}] were not tried")
}

/// 引数`collection`。空（空白だけも）・`system.`で始まる・`$`やNULを含む
/// 名前は断る。前後の空白は削らない（#808。`users `と`users`は別の
/// コレクション）
fn collection_name(args: &JsonValue) -> Result<String, McpError> {
    let name = args
        .get("collection")
        .and_then(JsonValue::as_str)
        .filter(|n| !n.trim().is_empty())
        .ok_or_else(|| McpError::InvalidParams("Missing 'collection' parameter".to_string()))?;
    if name.starts_with("system.") || name.contains('$') || name.contains('\0') || name.len() > 255
    {
        return Err(McpError::InvalidParams(format!(
            "'{name}' is not a collection mcp-rs reads (system collections and names with $ are not available)"
        )));
    }
    Ok(name.to_string())
}

/// JSON（Extended JSON。`{"$oid": "…"}`・`{"$date": "…"}`も書ける）のオブジェクトを、
/// BSONのドキュメントにする。断る演算子があれば断る
fn to_document(value: &JsonValue, field: &str) -> Result<Document, McpError> {
    if !value.is_object() {
        return Err(McpError::InvalidParams(format!(
            "{field} must be a document (a JSON object)"
        )));
    }
    refuse_operators(value, field)?;
    match Bson::try_from(value.clone()) {
        Ok(Bson::Document(document)) => Ok(document),
        Ok(_) => Err(McpError::InvalidParams(format!(
            "{field} must be a document"
        ))),
        Err(e) => Err(McpError::InvalidParams(format!(
            "{field} is not valid: {e}"
        ))),
    }
}

/// 引数`key`（省略可）のドキュメント
fn document_arg(args: &JsonValue, key: &str) -> Result<Option<Document>, McpError> {
    match args.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(value) => to_document(value, key).map(Some),
    }
}

/// 更新・削除のフィルター。空（全部のドキュメント）は断る
fn required_filter(args: &JsonValue) -> Result<Document, McpError> {
    let filter = document_arg(args, "filter")?.unwrap_or_default();
    if filter.is_empty() {
        return Err(McpError::InvalidParams(
            "'filter' must not be empty: changing or deleting every document is not available"
                .to_string(),
        ));
    }
    Ok(filter)
}

/// `value`の中（入れ子も）に、断る演算子があれば断る
fn refuse_operators(value: &JsonValue, field: &str) -> Result<(), McpError> {
    match value {
        JsonValue::Object(map) => {
            for (key, inner) in map {
                if let Some((name, why)) = REFUSED_OPERATORS.iter().find(|(name, _)| key == name) {
                    return Err(McpError::InvalidParams(format!(
                        "{field}: {name} is not available ({why})"
                    )));
                }
                refuse_operators(inner, field)?;
            }
            Ok(())
        }
        JsonValue::Array(items) => items
            .iter()
            .try_for_each(|item| refuse_operators(item, field)),
        _ => Ok(()),
    }
}

/// BSONの型の名前
fn bson_type(value: &Bson) -> &'static str {
    match value {
        Bson::Double(_) => "double",
        Bson::String(_) => "string",
        Bson::Array(_) => "array",
        Bson::Document(_) => "object",
        Bson::Boolean(_) => "bool",
        Bson::Null => "null",
        Bson::RegularExpression(_) => "regex",
        Bson::JavaScriptCode(_) | Bson::JavaScriptCodeWithScope(_) => "javascript",
        Bson::Int32(_) => "int",
        Bson::Int64(_) => "long",
        Bson::Timestamp(_) => "timestamp",
        Bson::Binary(_) => "binData",
        Bson::ObjectId(_) => "objectId",
        Bson::DateTime(_) => "date",
        Bson::Symbol(_) => "symbol",
        Bson::Decimal128(_) => "decimal",
        Bson::Undefined => "undefined",
        Bson::MaxKey => "maxKey",
        Bson::MinKey => "minKey",
        Bson::DbPointer(_) => "dbPointer",
    }
}

#[async_trait]
impl McpHandler for MongoHandler {
    async fn initialize(&self, params: InitializeParams) -> Result<JsonValue, McpError> {
        Ok(json!({
            "protocolVersion": crate::mcp::negotiate_protocol_version(&params.protocol_version),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "mongodb-handler", "version": "1.0.0" }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        let max_rows = self.limits.max_items;
        let max_bytes = self.limits.max_result_bytes;
        let read_only = || {
            Some(ToolAnnotations {
                read_only_hint: Some(true),
                open_world_hint: Some(false),
                ..Default::default()
            })
        };
        let collection = json!({ "type": "string", "description": "The collection name" });
        let filter = json!({
            "type": "object",
            "description": "A query filter, such as {\"status\": \"active\", \"age\": {\"$gte\": 18}}. Extended JSON is accepted for special types: {\"$oid\": \"...\"}, {\"$date\": \"2026-01-01T00:00:00Z\"}"
        });
        let refused = "$where, $function and $accumulator (JavaScript) are not available";
        let mut tools = vec![
            Tool {
                name: "find".to_string(),
                title: Some("Find MongoDB documents".to_string()),
                description: format!(
                    "Read documents of a collection in the configured MongoDB database. At most {max_rows} documents and about {max_bytes} bytes are returned (truncated is true when there were more): use filter, projection and limit for large collections. Documents are returned as relaxed Extended JSON ({{\"$oid\": ...}}, {{\"$date\": ...}}). {refused}."
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection": collection.clone(),
                        "filter": filter.clone(),
                        "projection": { "type": "object", "description": "Fields to return, such as {\"name\": 1, \"_id\": 0}" },
                        "sort": { "type": "object", "description": "Sort order, such as {\"created_at\": -1}" },
                        "limit": { "type": "integer", "description": format!("Documents to return at most, from 1 to {max_rows} (default {max_rows})") },
                        "skip": { "type": "integer", "description": "Documents to skip first" }
                    },
                    "required": ["collection"]
                }),
                annotations: read_only(),
                ..Default::default()
            },
            Tool {
                name: "aggregate".to_string(),
                title: Some("Run a MongoDB aggregation".to_string()),
                description: format!(
                    "Run an aggregation pipeline on a collection of the configured MongoDB database and return the resulting documents. At most {max_rows} documents and about {max_bytes} bytes are returned (mcp-rs adds a $limit stage). $out and $merge (which write) and {refused}."
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection": collection.clone(),
                        "pipeline": {
                            "type": "array",
                            "description": "The stages, such as [{\"$match\": {...}}, {\"$group\": {...}}]",
                            "items": { "type": "object" }
                        }
                    },
                    "required": ["collection", "pipeline"]
                }),
                annotations: read_only(),
                ..Default::default()
            },
            Tool {
                name: "count_documents".to_string(),
                title: Some("Count MongoDB documents".to_string()),
                description: format!("Count the documents of a collection that match a filter. {refused}."),
                input_schema: json!({
                    "type": "object",
                    "properties": { "collection": collection.clone(), "filter": filter.clone() },
                    "required": ["collection"]
                }),
                annotations: read_only(),
                ..Default::default()
            },
            Tool {
                name: "get_schema".to_string(),
                title: Some("Show the MongoDB collections".to_string()),
                description: format!("List the collections of the configured MongoDB database (at most {max_rows}, and about {max_bytes} bytes) with their estimated document counts, and the top-level field names and types read from up to {SCHEMA_SAMPLE} documents of each (no values). truncated is true when collections or fields were left out."),
                input_schema: json!({ "type": "object", "properties": {} }),
                annotations: read_only(),
                ..Default::default()
            },
        ];
        if self.allow_write {
            let write = || {
                Some(ToolAnnotations {
                    read_only_hint: Some(false),
                    destructive_hint: Some(true),
                    idempotent_hint: Some(false),
                    open_world_hint: Some(false),
                    ..Default::default()
                })
            };
            tools.push(Tool {
                name: "insert_documents".to_string(),
                title: Some("Insert MongoDB documents".to_string()),
                description: format!("Insert documents into an existing collection (at most {max_rows} at a time). A collection name that does not exist is refused, so that a typing mistake does not create a collection. Returns the inserted ids. Documents are inserted in order: when one fails, the error says how many before it were inserted, and those must not be sent again. After a timeout the outcome is unknown (the server may still apply it later): do not send again unless the collection is sure to refuse a second copy."),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection": collection.clone(),
                        "documents": { "type": "array", "items": { "type": "object" }, "description": "The documents to insert" }
                    },
                    "required": ["collection", "documents"]
                }),
                annotations: write(),
                ..Default::default()
            });
            tools.push(Tool {
                name: "update_documents".to_string(),
                title: Some("Update MongoDB documents".to_string()),
                description: format!("Update the documents of a collection that match a filter: the first one, or all of them with many = true. The filter must not be empty. update uses update operators ({{\"$set\": {{...}}}}) or a pipeline; a plain document (which would replace the document) is refused. After a timeout the outcome is unknown (the server may still apply it later): do not send again unless applying it twice is sure to leave the data as intended. {refused}."),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection": collection.clone(),
                        "filter": filter.clone(),
                        "update": { "description": "Update operators such as {\"$set\": {\"status\": \"done\"}}, or a pipeline (an array of stages)" },
                        "many": { "type": "boolean", "description": "true updates every matching document (default false: the first one)" }
                    },
                    "required": ["collection", "filter", "update"]
                }),
                annotations: write(),
                ..Default::default()
            });
            tools.push(Tool {
                name: "delete_documents".to_string(),
                title: Some("Delete MongoDB documents".to_string()),
                description: "Delete the documents of a collection that match a filter: the first one, or all of them with many = true. The filter must not be empty. Collections and indexes are not dropped. After a timeout the outcome is unknown (the server may still apply it later): do not send again unless deleting twice is sure to delete nothing else.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "collection": collection,
                        "filter": filter,
                        "many": { "type": "boolean", "description": "true deletes every matching document (default false: the first one)" }
                    },
                    "required": ["collection", "filter"]
                }),
                annotations: write(),
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
            "find" => self.handle_find(&args).await,
            "aggregate" => self.handle_aggregate(&args).await,
            "count_documents" => self.handle_count(&args).await,
            "get_schema" => self.handle_get_schema().await,
            "insert_documents" if self.allow_write => self.handle_insert(&args).await,
            "update_documents" if self.allow_write => self.handle_update(&args).await,
            "delete_documents" if self.allow_write => self.handle_delete(&args).await,
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
            name: "MongoDB collections".to_string(),
            description: Some("The collections, their document counts and field names".to_string()),
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

    #[test]
    fn test_refused_operators_are_found_anywhere() {
        for (value, name) in [
            (json!({"$where": "this.a > 1"}), "$where"),
            (json!({"a": {"$elemMatch": {"$where": "1"}}}), "$where"),
            (
                json!({"$expr": {"$function": {"body": "x", "args": [], "lang": "js"}}}),
                "$function",
            ),
            (
                json!({"$group": {"_id": 1, "x": {"$accumulator": {}}}}),
                "$accumulator",
            ),
            (json!({"$out": "copy"}), "$out"),
            (json!({"$merge": {"into": "copy"}}), "$merge"),
            (json!({"$and": [{"a": 1}, {"$where": "1"}]}), "$where"),
        ] {
            let err = to_document(&value, "filter").unwrap_err().to_string();
            assert!(err.contains(name), "{value}: {err}");
        }
        assert!(to_document(&json!({"a": {"$gte": 1}, "b": "$where"}), "filter").is_ok());
    }

    #[test]
    fn test_extended_json_is_read() {
        let document = to_document(
            &json!({"_id": {"$oid": "65a1b2c3d4e5f60718293a4b"}, "at": {"$date": "2026-01-01T00:00:00Z"}}),
            "filter",
        )
        .unwrap();
        assert!(matches!(document.get("_id"), Some(Bson::ObjectId(_))));
        assert!(matches!(document.get("at"), Some(Bson::DateTime(_))));
        assert!(to_document(&json!([1]), "filter").is_err());
    }

    #[test]
    fn test_collection_name() {
        assert_eq!(
            collection_name(&json!({"collection": "users"})).unwrap(),
            "users"
        );
        // 前後の空白は削らない（#808）
        assert_eq!(
            collection_name(&json!({"collection": "users "})).unwrap(),
            "users "
        );
        for bad in [
            json!({}),
            json!({"collection": ""}),
            json!({"collection": "  "}),
            json!({"collection": "system.users"}),
            json!({"collection": "a$b"}),
        ] {
            assert!(collection_name(&bad).is_err(), "{bad}");
        }
    }

    /// 通信のエラーは、成否が分からない（#808。サーバーが断ったエラーの
    /// 型は、ドライバーの外では作れない）
    #[test]
    fn test_write_not_applied() {
        use mongodb::error::{Error, ErrorKind};
        let lost = Error::from(ErrorKind::Io(Arc::new(std::io::Error::from(
            std::io::ErrorKind::ConnectionReset,
        ))));
        assert!(!write_not_applied(&lost, false));
        assert!(!write_not_applied(&lost, true));
        assert!(!write_rejected_by_server(&lost));
    }

    #[test]
    fn test_required_filter() {
        assert!(required_filter(&json!({})).is_err());
        assert!(required_filter(&json!({"filter": {}})).is_err());
        assert!(required_filter(&json!({"filter": {"a": 1}})).is_ok());
    }

    #[test]
    fn test_check_settings() {
        let settings = |host: &str, mode: Option<&str>| DatabaseSettings {
            engine: "mongodb".to_string(),
            host: Some(host.to_string()),
            database: "app".to_string(),
            ssl_mode: mode.map(str::to_string),
            ..Default::default()
        };
        assert_eq!(check_settings(&settings("localhost", None)).unwrap(), None);
        assert!(check_settings(&settings("db.example.com", None))
            .unwrap_err()
            .contains("ssl_mode"));
        assert!(
            check_settings(&settings("db.example.com", Some("verify-full")))
                .unwrap()
                .is_none()
        );
        assert!(check_settings(&settings("db.example.com", Some("require")))
            .unwrap()
            .unwrap()
            .contains("not checked"));
        let user_only = DatabaseSettings {
            username: "reader".to_string(),
            ..settings("localhost", None)
        };
        assert!(check_settings(&user_only)
            .unwrap_err()
            .contains("needs a password"));
        // 書いたホストにだけつなぐ（他のメンバーを探さない）
        assert_eq!(
            client_options(&settings("localhost", None)).direct_connection,
            Some(true)
        );
    }

    /// 時間切れのあとも、タスクは最後まで動く（ドライバーのfutureを落とさない）
    #[tokio::test]
    async fn test_run_to_completion_keeps_the_task() {
        let (done, finished) = tokio::sync::oneshot::channel();
        let slow = async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = done.send(());
        };
        assert_eq!(
            run_to_completion(Duration::from_millis(10), slow).await,
            Err(Stopped::TimedOut)
        );
        assert!(finished.await.is_ok());
        assert_eq!(
            run_to_completion(Duration::from_secs(5), async { 1 }).await,
            Ok(1)
        );
    }

    fn schema_fields(
        count: usize,
    ) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
        (0..count)
            .map(|i| {
                (
                    format!("field_with_a_long_name_{i:02}"),
                    std::collections::BTreeSet::from(["null".to_string(), "string".to_string()]),
                )
            })
            .collect()
    }

    /// `get_schema`の結果は、返す形（字下げつきのJSON）で`max_result_bytes`に収まる
    #[test]
    fn test_schema_fits_max_result_bytes() {
        let build = |max_bytes: usize| {
            let mut builder = SchemaBuilder::new("app", max_bytes);
            for i in 0..12 {
                // 1つおきに、読み切れなかったコレクション
                if !builder.push(
                    &format!("collection_{i:02}"),
                    Some(3),
                    schema_fields(15),
                    i % 2 == 1,
                ) {
                    break;
                }
            }
            builder.result(false)
        };
        let all = build(1_000_000);
        assert_eq!(all["collections"].as_array().unwrap().len(), 12);
        assert_eq!(all["truncated"], false);
        assert!(all["collections"][0].get("fields_truncated").is_none());
        assert!(all["collections"][0].get("fields_unread").is_none());
        assert_eq!(all["collections"][1]["fields_unread"], true);
        assert_eq!(
            all["collections"][0]["fields"].as_array().unwrap().len(),
            15
        );
        let full = serde_json::to_string_pretty(&all).unwrap().len();

        // 外側（コレクションが空のとき）より大きい上限なら、いつも収まる
        let envelope = serde_json::to_string_pretty(&build(0)).unwrap().len();
        let mut cut_fields = 0;
        // 数え方は安全側（fields_truncatedの分などを多めに数える）なので、
        // ちょうどの上限では、少し早く切ることがある
        let slack = 12 * 48;
        for max_bytes in (envelope + 3..full + slack + 200).step_by(37) {
            let schema = build(max_bytes);
            let len = serde_json::to_string_pretty(&schema).unwrap().len();
            assert!(len <= max_bytes, "{len} > {max_bytes}");
            let collections = schema["collections"].as_array().unwrap();
            if max_bytes >= full + slack {
                assert_eq!(collections.len(), 12);
                assert_eq!(schema["truncated"], false);
            }
            if collections.len() < 12 {
                assert_eq!(schema["truncated"], true, "{max_bytes}");
            }
            if let Some(last) = collections.last() {
                if last.get("fields_truncated").is_some() {
                    cut_fields += 1;
                    assert_eq!(schema["truncated"], true);
                    assert!(last["fields"].as_array().unwrap().len() < 15);
                }
            }
        }
        assert!(cut_fields > 0);
        // 上限が外側より小さくても、コレクションは足さない
        let none = build(10);
        assert_eq!(none["collections"], json!([]));
        assert_eq!(none["truncated"], true);
    }

    /// こちらで待つ時間が尽きたら、そこまでに入ったコレクションを返す。名前の
    /// 一覧も読めていなければ、エラー
    #[tokio::test]
    async fn test_schema_returns_what_was_read_on_timeout() {
        let stalled = |progress: SchemaProgress, list: bool| async move {
            if list {
                let mut progress = lock_schema(&progress);
                progress.listed = true;
                progress.push("users", Some(8), schema_fields(2), false);
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(false)
        };
        let progress: SchemaProgress =
            Arc::new(std::sync::Mutex::new(SchemaBuilder::new("app", 100_000)));
        let schema = schema_within(
            Duration::from_millis(50),
            30,
            progress.clone(),
            stalled(progress.clone(), true),
        )
        .await
        .unwrap();
        assert_eq!(schema["timed_out"], true);
        assert_eq!(schema["truncated"], true);
        assert_eq!(schema["collections"].as_array().unwrap().len(), 1);
        assert_eq!(schema["collections"][0]["name"], "users");

        let progress: SchemaProgress =
            Arc::new(std::sync::Mutex::new(SchemaBuilder::new("app", 100_000)));
        let err = schema_within(
            Duration::from_millis(50),
            30,
            progress.clone(),
            stalled(progress.clone(), false),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("did not list the collections"),
            "{err}"
        );

        // 時間の中で終われば、そのままの結果
        let progress: SchemaProgress =
            Arc::new(std::sync::Mutex::new(SchemaBuilder::new("app", 100_000)));
        let done = {
            let progress = progress.clone();
            async move {
                let mut progress = lock_schema(&progress);
                progress.listed = true;
                progress.push("users", Some(8), schema_fields(2), false);
                Ok(false)
            }
        };
        let schema = schema_within(Duration::from_secs(5), 30, progress, done)
            .await
            .unwrap();
        assert_eq!(schema["timed_out"], false);
        assert_eq!(schema["truncated"], false);
    }

    #[test]
    fn test_partial_insert_message() {
        let first = partial_insert_message(0, 3, "duplicate key");
        assert!(
            first.contains("documents[0] failed: duplicate key"),
            "{first}"
        );
        assert!(first.contains("No document was inserted"), "{first}");
        let second = partial_insert_message(2, 3, "duplicate key");
        assert!(second.contains("documents[2] failed"), "{second}");
        assert!(
            second.contains("2 of 3 documents were inserted (documents[0] to documents[1])"),
            "{second}"
        );
    }
}
