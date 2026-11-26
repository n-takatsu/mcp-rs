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
//! - 1つの操作の時間の上限（`query_timeout_seconds`。サーバーの`maxTimeMS`と、
//!   こちらで待つ時間）
//! - `update_documents`・`delete_documents`は、空のフィルター（全部のドキュメント）を
//!   断る。`insert_documents`の数は`max_rows`まで。コレクション・インデックスの
//!   作成・削除はしない
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
use std::future::IntoFuture;
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

/// サーバーを選ぶ・つなぐときの待ち時間の上限
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// サーバーの時間の上限（`maxTimeMS`）より、こちらで待つ時間を少し長くする
const CLIENT_TIMEOUT_GRACE: Duration = Duration::from_secs(2);

/// `get_schema`で、コレクションごとにフィールドを読むドキュメントの数
const SCHEMA_SAMPLE: i64 = 20;

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
        tokio::time::timeout(
            CONNECT_TIMEOUT + CLIENT_TIMEOUT_GRACE,
            database.run_command(doc! { "ping": 1 }),
        )
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

    /// こちらで待つ時間の上限をつけて、操作を待つ
    async fn wait<T>(
        &self,
        what: &str,
        future: impl std::future::Future<Output = mongodb::error::Result<T>>,
    ) -> Result<T, McpError> {
        tokio::time::timeout(self.limits.timeout + CLIENT_TIMEOUT_GRACE, future)
            .await
            .map_err(|_| {
                McpError::ExternalApi(format!(
                    "MongoDB {what} did not finish within {} seconds",
                    self.limits.timeout.as_secs()
                ))
            })?
            .map_err(|e| McpError::ExternalApi(format!("MongoDB {what}: {e}")))
    }

    fn collection(&self, args: &JsonValue) -> Result<mongodb::Collection<Document>, McpError> {
        Ok(self.database.collection(&collection_name(args)?))
    }

    /// カーソルから、件数・バイト数の上限まで読む。上限で止めたら`true`
    async fn read_cursor(
        &self,
        mut cursor: mongodb::Cursor<Document>,
        max_items: usize,
    ) -> Result<(Vec<JsonValue>, bool), McpError> {
        let mut documents = Vec::new();
        let mut bytes = 0usize;
        let read = async {
            while let Some(document) = cursor.try_next().await? {
                let value = Bson::Document(document).into_relaxed_extjson();
                bytes += serde_json::to_string(&value).map(|s| s.len()).unwrap_or(0);
                if documents.len() >= max_items || bytes > self.limits.max_result_bytes {
                    // 1つ多く読めたら、上限を超えている。残りは読まない
                    // （カーソルは、落とすと閉じる）
                    return Ok::<_, mongodb::error::Error>(true);
                }
                documents.push(value);
            }
            Ok(false)
        };
        let truncated = self.wait("read", read).await?;
        Ok((documents, truncated))
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
        // 1つ多く読み、超えたかを知る
        let mut find = collection
            .find(filter)
            .limit(i64::try_from(limit + 1).unwrap_or(i64::MAX))
            .max_time(self.limits.timeout)
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
        let cursor = self.wait("find", find.into_future()).await?;
        let (documents, truncated) = self.read_cursor(cursor, limit).await?;
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
        let aggregate = collection
            .aggregate(pipeline)
            .max_time(self.limits.timeout)
            .batch_size(u32::try_from(max.min(1000) + 1).unwrap_or(1000));
        let cursor = self.wait("aggregate", aggregate.into_future()).await?;
        let (documents, truncated) = self.read_cursor(cursor, max).await?;
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
        let count = self
            .wait(
                "count_documents",
                collection
                    .count_documents(filter)
                    .max_time(self.limits.timeout)
                    .into_future(),
            )
            .await?;
        Ok(json!({ "count": count }))
    }

    /// `get_schema`: コレクションの一覧と、そのドキュメントの数・フィールド
    async fn handle_get_schema(&self) -> Result<JsonValue, McpError> {
        let mut names = self
            .wait(
                "listCollections",
                self.database.list_collection_names().into_future(),
            )
            .await?;
        names.retain(|name| !name.starts_with("system."));
        names.sort();
        let max = self.limits.max_items;
        let truncated = names.len() > max;
        names.truncate(max);
        let mut collections = Vec::new();
        for name in names {
            let collection = self.database.collection::<Document>(&name);
            let count = self
                .wait(
                    "estimatedDocumentCount",
                    collection
                        .estimated_document_count()
                        .max_time(self.limits.timeout)
                        .into_future(),
                )
                .await
                .ok();
            // 数件のドキュメントから、トップレベルのフィールドの名前と型を集める
            // （値は返さない）
            let mut fields: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
                std::collections::BTreeMap::new();
            let sample = collection
                .find(doc! {})
                .limit(SCHEMA_SAMPLE)
                .max_time(self.limits.timeout);
            if let Ok(cursor) = self.wait("find", sample.into_future()).await {
                let read = async {
                    let mut cursor = cursor;
                    while let Some(document) = cursor.try_next().await? {
                        for (field, value) in &document {
                            fields
                                .entry(field.clone())
                                .or_default()
                                .insert(bson_type(value).to_string());
                        }
                    }
                    Ok::<_, mongodb::error::Error>(())
                };
                let _ = self.wait("find", read).await;
            }
            collections.push(json!({
                "name": name,
                "estimated_documents": count,
                "fields": fields
                    .into_iter()
                    .map(|(name, types)| json!({ "name": name, "types": types }))
                    .collect::<Vec<_>>(),
            }));
        }
        Ok(json!({
            "engine": "mongodb",
            "database": self.database.name(),
            "collections": collections,
            "truncated": truncated,
            "note": format!("fields are read from up to {SCHEMA_SAMPLE} documents of each collection, so rare fields may be missing"),
        }))
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
        let result = self
            .wait(
                "insertMany",
                collection.insert_many(documents).into_future(),
            )
            .await?;
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
            self.wait(
                "updateMany",
                collection.update_many(filter, update).into_future(),
            )
            .await?
        } else {
            self.wait(
                "updateOne",
                collection.update_one(filter, update).into_future(),
            )
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
            self.wait("deleteMany", collection.delete_many(filter).into_future())
                .await?
        } else {
            self.wait("deleteOne", collection.delete_one(filter).into_future())
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

/// 引数`collection`。空・`system.`で始まる・`$`やNULを含む名前は断る
fn collection_name(args: &JsonValue) -> Result<String, McpError> {
    let name = args
        .get("collection")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
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
                description: format!("List the collections of the configured MongoDB database (at most {max_rows}) with their estimated document counts, and the top-level field names and types read from up to {SCHEMA_SAMPLE} documents of each (no values)."),
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
                description: format!("Insert documents into a collection (at most {max_rows} at a time). Returns the inserted ids."),
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
                description: format!("Update the documents of a collection that match a filter: the first one, or all of them with many = true. The filter must not be empty. update uses update operators ({{\"$set\": {{...}}}}) or a pipeline; a plain document (which would replace the document) is refused. {refused}."),
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
                description: "Delete the documents of a collection that match a filter: the first one, or all of them with many = true. The filter must not be empty. Collections and indexes are not dropped.".to_string(),
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
        for bad in [
            json!({}),
            json!({"collection": ""}),
            json!({"collection": "system.users"}),
            json!({"collection": "a$b"}),
        ] {
            assert!(collection_name(&bad).is_err(), "{bad}");
        }
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
    }
}
