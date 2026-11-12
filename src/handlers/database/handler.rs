//! Database MCP Handler Implementation
//!
//! データベース操作のためのMCPハンドラー実装

use crate::handlers::database::{
    column_encryption::ColumnEncryptionManager,
    column_provenance::{
        dialect_for, resolve_column_provenance, resolve_query_tables,
        validate_single_query_statement, validate_single_write_statement, ColumnProvenance,
        ProjectionResolution,
    },
    engine::{DatabaseEngineBuilder, EngineRegistry},
    pool::{ConnectionPool, PoolManager},
    types::{
        DatabaseConfig, DatabaseError, DatabaseType, QueryContext, QueryResult, QueryType, Value,
    },
};
use crate::mcp::{
    InitializeParams, McpError, McpHandler, Resource, ResourceReadParams, Tool, ToolAnnotations,
    ToolCallParams,
};
use crate::security::auth::types::{AuthUser, Role};
use crate::zero_trust::{micro_segmentation::MicroSegmentation, AccessRequest, TrustScore};
use async_trait::async_trait;
use base64::prelude::*;
use serde_json::{json, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

/// Placeholder returned in place of an encrypted column's raw value when the
/// caller isn't authorized to see it decrypted.
const ENCRYPTED_PLACEHOLDER: &str = "***ENCRYPTED***";

/// Placeholder returned when a result column's source table/column can't be
/// confidently determined, so it can't be checked against the encrypted-
/// column configuration at all - see `column_provenance`.
const UNKNOWN_PROVENANCE_PLACEHOLDER: &str = "***UNKNOWN_PROVENANCE***";

/// Canonical string form of a `Role`, matching the lowercase role-name
/// convention `MicroSegmentation::setup_default_policies()` uses (`"user"`,
/// `"admin"`, etc). `Role` itself has no `Display` impl since its variants
/// aren't otherwise rendered as user-facing text elsewhere in the codebase.
fn role_name(role: &Role) -> String {
    match role {
        Role::Admin => "admin".to_string(),
        Role::User => "user".to_string(),
        Role::Guest => "guest".to_string(),
        Role::Custom(name) => name.clone(),
    }
}

/// データベースMCPハンドラー
pub struct DatabaseHandler {
    /// エンジンレジストリ
    engines: Arc<RwLock<EngineRegistry>>,
    /// 接続プールマネージャー
    pool_manager: Arc<PoolManager>,
    /// アクティブなエンジンID
    active_engine: Arc<RwLock<Option<String>>>,
    /// データベース設定
    configs: Arc<RwLock<HashMap<String, DatabaseConfig>>>,
    /// カラムレベル暗号化マネージャー（未設定ならこのハンドラでは暗号化列の
    /// 強制を一切行わない - 既存の呼び出し元には影響しないオプトイン機能）
    column_encryption: Option<Arc<ColumnEncryptionManager>>,
    /// マイクロセグメンテーションエンジン（未設定ならこのハンドラでは
    /// テーブル単位のアクセス制御を一切行わない - 既存の呼び出し元には
    /// 影響しないオプトイン機能）。`src/zero_trust/micro_segmentation.rs`
    /// は元々コードベースのどこからも参照されていなかった実装済みロジック
    /// で、Issue #226でこの経路に配線した。
    micro_segmentation: Option<Arc<MicroSegmentation>>,
    /// ツールの設定（書き込みを許すか、返す行数の上限、#453）
    tool_options: DatabaseToolOptions,
    /// ツール呼び出しを記録する監査ログ（#453。WordPressと同じもの）
    audit_logger: Option<Arc<crate::security::AuditLogger>>,
    /// `mcp_rs_status`に出す1行（接続先。パスワードは含めない）
    status_line: Option<String>,
}

/// データベースのツールの設定（#453）
#[derive(Debug, Clone)]
pub struct DatabaseToolOptions {
    /// 書き込みのツール（`execute_command`）を出すか。既定は`false`
    pub allow_write: bool,
    /// `execute_query`が返す行数の上限。超えた分は返さず、`truncated`を
    /// `true`にする
    pub max_rows: usize,
}

impl Default for DatabaseToolOptions {
    fn default() -> Self {
        Self {
            allow_write: false,
            max_rows: 1000,
        }
    }
}

/// 監査ログに記録するSQLの長さの上限（文字数）
const MAX_AUDITED_SQL_CHARS: usize = 2000;

impl DatabaseHandler {
    /// 新しいデータベースハンドラーを作成
    ///
    /// 以前は`new(threat_intelligence)`で、`DatabaseSecurity`（正規表現での
    /// SQLインジェクション検知等）・`SafetyManager`を作っていたが、クエリの
    /// 実行ではどれも使っていなかったため削除した（#499）。このハンドラーの
    /// 安全性は、読み取り専用のロール・文の種類の検査（SELECTだけ、
    /// `allow_write`のときだけ書き込み）・カラム暗号化・マイクロ
    /// セグメンテーションによる。
    pub async fn new() -> Result<Self, DatabaseError> {
        Ok(Self {
            engines: Arc::new(RwLock::new(EngineRegistry::new())),
            pool_manager: Arc::new(PoolManager::new()),
            active_engine: Arc::new(RwLock::new(None)),
            configs: Arc::new(RwLock::new(HashMap::new())),
            column_encryption: None,
            micro_segmentation: None,
            tool_options: DatabaseToolOptions::default(),
            audit_logger: None,
            status_line: None,
        })
    }

    /// 設定ファイルの`[database]`から作り、つながるかを確かめる（#453）。
    /// 失敗したら、起動時のログと`mcp_rs_status`に出す理由を返す（パスワード
    /// は含めない）
    pub async fn from_settings(settings: &crate::config::DatabaseSettings) -> Result<Self, String> {
        use crate::handlers::database::types::{ConnectionConfig, FeatureConfig, PoolConfig};

        settings.validate()?;
        let max_rows = settings.max_rows() as usize;
        let mut options = HashMap::new();
        // 上限より1行多く読み、超えたかを知る
        options.insert(
            crate::handlers::database::engines::postgresql::OPTION_FETCH_LIMIT.to_string(),
            (max_rows + 1).to_string(),
        );
        let config = DatabaseConfig {
            database_type: DatabaseType::PostgreSQL,
            connection: ConnectionConfig {
                host: settings.host().to_string(),
                port: settings.port(),
                database: settings.database.clone(),
                username: settings.username.clone(),
                password: settings.password.clone().unwrap_or_default(),
                ssl_mode: settings.ssl_mode.clone(),
                timeout_seconds: 10,
                retry_attempts: 0,
                options,
            },
            pool: PoolConfig {
                max_connections: settings.max_connections(),
                min_connections: 0,
                ..PoolConfig::default()
            },
            features: FeatureConfig {
                query_timeout: settings.query_timeout_seconds(),
                ..FeatureConfig::default()
            },
            ..DatabaseConfig::default()
        };

        let handler = Self::new()
            .await
            .map_err(|e| e.to_string())?
            .with_tool_options(DatabaseToolOptions {
                allow_write: settings.allow_write(),
                max_rows,
            })
            .with_status_line(settings.describe());
        handler
            .add_database("postgresql".to_string(), config)
            .await
            .map_err(|e| format!("cannot set up {}: {}", settings.describe(), e))?;
        handler
            .check_connection()
            .await
            .map_err(|e| format!("cannot connect to {}: {}", settings.describe(), e))?;
        Ok(handler)
    }

    /// ツールの設定（ビルダースタイル、#453）
    pub fn with_tool_options(mut self, options: DatabaseToolOptions) -> Self {
        self.tool_options = options;
        self
    }

    /// ツール呼び出しを監査ログに記録する（ビルダースタイル、#453）
    pub fn with_audit_logger(mut self, audit_logger: Arc<crate::security::AuditLogger>) -> Self {
        self.audit_logger = Some(audit_logger);
        self
    }

    /// `mcp_rs_status`に出す1行（ビルダースタイル、#453）
    pub fn with_status_line(mut self, line: impl Into<String>) -> Self {
        self.status_line = Some(line.into());
        self
    }

    /// 登録したデータベースにつながるかを確かめる（起動時、#453）
    pub async fn check_connection(&self) -> Result<(), DatabaseError> {
        let (_, config, pool) = self.resolve_engine(None).await?;
        let connection = pool.get_connection(&config).await?;
        connection.inner().ping().await
    }

    /// 使うエンジンのID・設定・プールを決める。`engine`が無ければ、
    /// アクティブなエンジン（#453。以前は`unwrap`で、エンジンが無いと
    /// パニックした）
    async fn resolve_engine(
        &self,
        engine: Option<&str>,
    ) -> Result<(String, DatabaseConfig, Arc<ConnectionPool>), DatabaseError> {
        let engine_id = match engine {
            Some(id) => id.to_string(),
            None => self.active_engine.read().await.clone().ok_or_else(|| {
                DatabaseError::ConfigurationError("No active database engine".to_string())
            })?,
        };
        let config = self
            .configs
            .read()
            .await
            .get(&engine_id)
            .cloned()
            .ok_or_else(|| {
                DatabaseError::ConfigurationError(format!(
                    "Configuration not found for engine: {engine_id}"
                ))
            })?;
        let pool = self
            .pool_manager
            .get_pool(&engine_id)
            .await
            .ok_or_else(|| DatabaseError::PoolError(format!("Pool not found: {engine_id}")))?;
        Ok((engine_id, config, pool))
    }

    /// ツール呼び出しを監査ログに記録する（#453）。成功・失敗とも記録する。
    /// SQLは長さを切って記録し、パラメーターの値は記録しない
    async fn audit_tool_call(
        &self,
        tool_name: &str,
        sql: Option<&str>,
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
        .with_action(
            self.status_line
                .clone()
                .unwrap_or_else(|| "database".to_string()),
            tool_name.to_string(),
        )
        .with_result(outcome.to_string());
        if let Some(sql) = sql {
            let mut text: String = sql.chars().take(MAX_AUDITED_SQL_CHARS).collect();
            if sql.chars().count() > MAX_AUDITED_SQL_CHARS {
                text.push('…');
            }
            entry = entry.add_metadata("sql".to_string(), text);
        }
        if let Err(e) = result {
            entry = entry.add_metadata("error".to_string(), e.to_string());
        }
        let entry = crate::mcp::audit_tool_call_context(entry, started);
        if let Err(e) = audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", tool_name, e);
        }
    }

    /// カラムレベル暗号化＋RBACを有効化する（ビルダースタイル）
    pub fn with_column_encryption(mut self, manager: Arc<ColumnEncryptionManager>) -> Self {
        self.column_encryption = Some(manager);
        self
    }

    /// マイクロセグメンテーション（テーブル単位のアクセス制御）を
    /// 有効化する（ビルダースタイル）
    pub fn with_micro_segmentation(mut self, engine: Arc<MicroSegmentation>) -> Self {
        self.micro_segmentation = Some(engine);
        self
    }

    /// データベースエンジンを追加
    pub async fn add_database(
        &self,
        id: String,
        config: DatabaseConfig,
    ) -> Result<(), DatabaseError> {
        // エンジンを作成
        let engine = DatabaseEngineBuilder::build(&config).await?;

        // 接続プールを作成
        let _pool = self
            .pool_manager
            .create_pool(id.clone(), engine.clone(), &config)
            .await?;

        // エンジンを登録
        {
            let mut engines = self.engines.write().await;
            engines.register(engine);
        }

        // 設定を保存
        {
            let mut configs = self.configs.write().await;
            configs.insert(id.clone(), config);
        }

        // 最初のエンジンをアクティブに設定
        {
            let mut active = self.active_engine.write().await;
            if active.is_none() {
                *active = Some(id);
            }
        }

        Ok(())
    }

    /// アクティブエンジンを切り替え
    pub async fn switch_engine(&self, engine_id: &str) -> Result<(), DatabaseError> {
        let configs = self.configs.read().await;
        if !configs.contains_key(engine_id) {
            return Err(DatabaseError::ConfigurationError(format!(
                "Engine not found: {}",
                engine_id
            )));
        }

        let mut active = self.active_engine.write().await;
        *active = Some(engine_id.to_string());

        Ok(())
    }

    /// クエリ実行処理（`McpHandler::call_tool`経由 - 呼び出し元の識別子は
    /// 得られないため、暗号化列は常にfail-closedで扱われる）
    async fn handle_execute_query(&self, args: JsonValue) -> Result<JsonValue, McpError> {
        self.execute_query_core(&args, None).await
    }

    /// 検証済みの呼び出し元識別子付きで`execute_query`を実行する。
    ///
    /// このコードベースのどのMCPトランスポートも、認証済みの呼び出し元を
    /// `McpHandler::call_tool`まで届けていない（トランスポート層の認証配線は
    /// 別問題として本Issueのスコープ外としている）。本メソッドは、何らかの
    /// 方法で既に検証済みの`AuthUser`を持っている呼び出し元
    /// （テスト、または将来的にトレイトオブジェクトではなく具体的な
    /// `DatabaseHandler`を保持する形で認証済みの呼び出し元が実装された場合）
    /// のためのものである。
    pub async fn execute_query_as(
        &self,
        args: JsonValue,
        auth_user: &AuthUser,
    ) -> Result<JsonValue, McpError> {
        self.execute_query_core(&args, Some(auth_user)).await
    }

    async fn execute_query_core(
        &self,
        args: &JsonValue,
        auth_user: Option<&AuthUser>,
    ) -> Result<JsonValue, McpError> {
        let sql: String = args
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidRequest("Missing 'sql' parameter".to_string()))?
            .to_string();

        let params: Vec<Value> = if let Some(params_json) = args.get("params") {
            self.parse_parameters(params_json)?
        } else {
            Vec::new()
        };

        // ツールのinput schemaが公開している`engine`引数を尊重する - 指定が
        // あればそのエンジンID、なければactive_engineを使う。dialect/config/
        // poolのすべてをこの同じ`engine_id`から解決することで、明示指定時も
        // 未指定時も三者が食い違わないようにする。active_engineを読む場合は
        // ロックを先に解放してからconfigsを読み、両方のロックを同時に保持
        // する時間を最小化する（将来ロック取得順が変わった場合のデッドロック
        // 要因を避けるため）。
        let engine_id = match args.get("engine") {
            None | Some(JsonValue::Null) => {
                let active_id = self.active_engine.read().await;
                active_id.clone().ok_or_else(|| {
                    McpError::InvalidRequest("No active database engine".to_string())
                })?
            }
            Some(JsonValue::String(id)) => id.clone(),
            // A present-but-non-string `engine` (a number, object, etc) must
            // be rejected outright, not silently treated as "not specified"
            // - that would dispatch the query to whichever engine happens
            // to be active instead of the one the caller actually asked for.
            Some(_) => {
                return Err(McpError::InvalidRequest(
                    "'engine' parameter must be a string".to_string(),
                ))
            }
        };
        let config = {
            let configs = self.configs.read().await;
            configs.get(&engine_id).cloned().ok_or_else(|| {
                McpError::InvalidRequest(format!("Configuration not found for engine: {engine_id}"))
            })?
        };
        let dialect = dialect_for(config.database_type.clone());

        // execute_queryは「SELECT文のみ」を契約として公開しているツールで
        // あり（list_toolsのdescription参照）、暗号化設定の有無に関わらず
        // この契約自体を常に強制する。列暗号化が設定されている場合のみ
        // 有効になる、より厳密な由来解決チェックとは別に、ここで無条件に
        // 検証する。SQLの方言はアクティブなエンジンの`DatabaseType`に
        // 合わせる - Postgres方言で固定すると、MySQL/SQLiteなど他エンジン
        // 向けの正当な構文を誤って拒否しかねない。
        validate_single_query_statement(&sql, dialect.as_ref())
            .map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        // マイクロセグメンテーションが設定されている場合、クエリが触れる
        // すべてのテーブルについて事前にアクセスを検証する。列暗号化と同様、
        // クエリ実行前に検証する - 実行後にチェックすると、未対応のSQL形が
        // 拒否される前にクエリ（および副作用を持つ別ステートメント）が
        // 既にデータベースに対して実行されてしまう。1つでも拒否された
        // テーブルがあればクエリ全体を拒否する（列レベルの部分マスクとは
        // 異なり、テーブルへのアクセス可否は分割できないため）。
        if let Some(engine) = &self.micro_segmentation {
            let tables = resolve_query_tables(&sql, dialect.as_ref()).map_err(|e| {
                McpError::InvalidRequest(format!(
                    "cannot safely determine which tables this query touches, \
                     and micro-segmentation is configured for this database: {e}"
                ))
            })?;

            let user_roles: HashSet<String> = auth_user
                .map(|u| u.roles.iter().map(role_name).collect())
                .unwrap_or_default();
            // device_verifier/network_analyzerが未配線のため、trust_scoreは
            // 「識別子ありか」だけを表す暫定値。これらのエンジンが実配線
            // された将来、device/network/behaviorの実測値に置き換えられる
            // 前提の、正直な簡略化。
            let trust_score: TrustScore = if auth_user.is_some() { 100 } else { 0 };

            for table in &tables {
                let resource = format!("database:{engine_id}:{table}");
                let request = AccessRequest::new(
                    auth_user.map(|u| u.id.clone()).unwrap_or_default(),
                    "unknown", // device_id: device_verifier未配線のため不明値
                    None,      // source_ip: トランスポート層のIPをDB層は知らない
                    resource,
                    "read", // execute_queryはSELECTのみ（既存契約）
                );
                let result = engine.evaluate_access(&request, &user_roles, trust_score);
                if !result.success {
                    warn!(
                        "Micro-segmentation denied access to table '{}' for user {:?}: {}",
                        table,
                        auth_user.map(|u| &u.id),
                        result.reason
                    );
                    return Err(McpError::InvalidRequest(format!(
                        "access denied by micro-segmentation policy for table '{table}'"
                    )));
                }
            }
        }

        // 接続プールから接続を取得。上で解決した`engine_id`をそのまま使う
        // (`get_active_pool()`は使わない) - `get_active_pool()`は
        // `active_engine`を独自に読み直すため、`engine`引数で明示指定された
        // 場合にそれと食い違ったり、指定が無い場合でもその間に
        // `switch_engine()`が割り込むと、上のdialect/configとは別のエンジン
        // のプールを取得してしまい、検証と実行が異なるエンジンに対して
        // 行われかねない。
        let pool = self
            .pool_manager
            .get_pool(&engine_id)
            .await
            .ok_or_else(|| McpError::InvalidRequest(format!("Pool not found: {engine_id}")))?;

        // 暗号化列が設定されている場合、クエリを実行する前にSQLの形が
        // 安全に解析できるか確認する。実行後にチェックすると、CTE/UNION/
        // 複数ステートメントのような未対応の形が拒否される前に、その
        // クエリ（および仕込まれた副作用を持つ別ステートメント）が
        // 既にデータベースに対して実行されてしまっているため、
        // ここで事前に確認する必要がある。
        let provenance = match &self.column_encryption {
            Some(manager) if manager.has_encrypted_columns() => Some(
                resolve_column_provenance(&sql, dialect.as_ref()).map_err(|e| {
                    McpError::InvalidRequest(format!(
                        "cannot safely determine which table this query's columns came from, \
                             and column encryption is configured for this database: {e}"
                    ))
                })?,
            ),
            _ => None,
        };

        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        // クエリ実行
        let mut result = connection
            .inner()
            .query(&sql, &params)
            .await
            .map_err(|e| McpError::InvalidRequest(e.to_string()))?;

        if let Some(resolution) = provenance {
            let manager = self
                .column_encryption
                .clone()
                .expect("provenance is only Some when self.column_encryption was Some above");
            Self::apply_column_encryption(&manager, resolution, &mut result, auth_user).await?;
        }

        // 返す行数の上限（#453）。接続は上限より1行多く読むので、超えたかが
        // 分かる
        let max_rows = self.tool_options.max_rows;
        let truncated = result.rows.len() > max_rows;
        if truncated {
            result.rows.truncate(max_rows);
            result.total_rows = Some(max_rows as u64);
        }

        // 結果をJSON形式で返す
        Ok(json!({
            "truncated": truncated,
            "max_rows": max_rows,
            "columns": result.columns.iter().map(|col| json!({
                "name": col.name,
                "type": col.data_type,
                "nullable": col.nullable,
                "max_length": col.max_length
            })).collect::<Vec<_>>(),
            "rows": result.rows.iter().map(|row| {
                row.iter().map(|value| self.value_to_json(value)).collect::<Vec<_>>()
            }).collect::<Vec<_>>(),
            "total_rows": result.total_rows,
            "execution_time_ms": result.execution_time_ms
        }))
    }

    /// 事前に解決済みの列由来（`resolution`）を使って、`result`内の
    /// 暗号化対象と設定された列を復号（許可時）またはマスク（拒否時・
    /// 由来不明時）に置き換える。SQLの解析自体は`execute_query_core`側で
    /// クエリ実行前に完了しているため、ここでは失敗しない
    /// （`decrypt()`呼び出し自体の失敗を除く）。
    async fn apply_column_encryption(
        manager: &Arc<ColumnEncryptionManager>,
        resolution: ProjectionResolution,
        result: &mut QueryResult,
        auth_user: Option<&AuthUser>,
    ) -> Result<(), McpError> {
        let context = QueryContext {
            query_type: QueryType::Select,
            user_id: auth_user.map(|u| u.id.clone()),
            session_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            source_ip: None,
            client_info: None,
        };

        // Normalize both resolution shapes into one per-column-index list,
        // so the rest of this function has a single code path.
        let column_provenances: Vec<Option<ColumnProvenance>> = match resolution {
            ProjectionResolution::WildcardTable(table) => result
                .columns
                .iter()
                .map(|c| {
                    Some(ColumnProvenance::Resolved {
                        table: table.clone(),
                        column: c.name.clone(),
                    })
                })
                .collect(),
            ProjectionResolution::PerColumn(provenances) => {
                provenances.into_iter().map(Some).collect()
            }
        };

        for (idx, provenance) in column_provenances.iter().enumerate() {
            match provenance {
                Some(ColumnProvenance::Resolved { table, column })
                    if manager.is_encrypted_column(table, column) =>
                {
                    Self::apply_encrypted_column(
                        manager, table, column, idx, result, &context, auth_user,
                    )
                    .await?;
                }
                Some(ColumnProvenance::Unknown) => {
                    // NULL carries no ciphertext to hide - preserve it, same
                    // as the encrypted-column path below does, instead of
                    // changing result semantics for consumers expecting null.
                    for row in result.rows.iter_mut() {
                        if let Some(value) = row.get_mut(idx) {
                            if !matches!(value, Value::Null) {
                                *value = Value::String(UNKNOWN_PROVENANCE_PLACEHOLDER.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Decrypts or masks one encrypted column's worth of values across
    /// every row of `result` at once, via [`ColumnEncryptionManager::decrypt_batch`]
    /// so the permission check (and, on denial, the audit log entry) happens
    /// once per query for this column rather than once per row.
    async fn apply_encrypted_column(
        manager: &Arc<ColumnEncryptionManager>,
        table: &str,
        column: &str,
        idx: usize,
        result: &mut QueryResult,
        context: &QueryContext,
        auth_user: Option<&AuthUser>,
    ) -> Result<(), McpError> {
        let mut row_indices = Vec::new();
        let mut ciphertexts = Vec::new();
        for (row_idx, row) in result.rows.iter().enumerate() {
            if let Some(Value::String(s)) = row.get(idx) {
                row_indices.push(row_idx);
                ciphertexts.push(s.as_str());
            }
        }

        if !ciphertexts.is_empty() {
            let decrypted = manager
                .decrypt_batch(table, column, &ciphertexts, context, auth_user)
                .await
                .map_err(|e| McpError::InvalidRequest(format!("decryption failed: {e}")))?;

            for (row_idx, value) in row_indices.into_iter().zip(decrypted) {
                result.rows[row_idx][idx] = Value::String(value);
            }
        }

        // An encrypted column should only ever contain NULL or the base64
        // ciphertext string produced by `encrypt()` - anything else is
        // unexpected, so mask it defensively rather than assume it's safe.
        for row in result.rows.iter_mut() {
            if let Some(value) = row.get_mut(idx) {
                if !matches!(value, Value::Null | Value::String(_)) {
                    *value = Value::String(ENCRYPTED_PLACEHOLDER.to_string());
                }
            }
        }

        Ok(())
    }

    /// コマンド実行処理（INSERT・UPDATE・DELETE）。`allow_write`のときだけ
    /// ツールとして出す（#453）
    async fn handle_execute_command(&self, args: JsonValue) -> Result<JsonValue, McpError> {
        if !self.tool_options.allow_write {
            return Err(McpError::InvalidRequest(
                "execute_command is disabled: the database is read-only (set allow_write = true in [database] to allow INSERT, UPDATE, and DELETE)".to_string(),
            ));
        }
        let sql: String = args
            .get("sql")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::InvalidParams("Missing 'sql' parameter".to_string()))?
            .to_string();

        let params: Vec<Value> = if let Some(params_json) = args.get("params") {
            self.parse_parameters(params_json)?
        } else {
            Vec::new()
        };

        let (_, config, pool) = self
            .resolve_engine(None)
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;
        // 1つのINSERT・UPDATE・DELETEだけ（DDL・権限の変更等は受け付けない）
        let dialect = dialect_for(config.database_type.clone());
        validate_single_write_statement(&sql, dialect.as_ref())
            .map_err(|e| McpError::InvalidParams(e.to_string()))?;

        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::ExternalApi(e.to_string()))?;
        let result = connection
            .inner()
            .execute(&sql, &params)
            .await
            .map_err(|e| McpError::ExternalApi(e.to_string()))?;

        // 結果をJSON形式で返す
        Ok(json!({
            "rows_affected": result.rows_affected,
            "last_insert_id": result.last_insert_id.map(|v| self.value_to_json(&v)),
            "execution_time_ms": result.execution_time_ms
        }))
    }

    /// スキーマ取得処理
    async fn handle_get_schema(&self, _args: JsonValue) -> Result<JsonValue, McpError> {
        let (_, config, pool) = self
            .resolve_engine(None)
            .await
            .map_err(|e| McpError::Internal(e.to_string()))?;
        let connection = pool
            .get_connection(&config)
            .await
            .map_err(|e| McpError::ExternalApi(e.to_string()))?;

        // スキーマ取得
        let schema = connection
            .inner()
            .get_schema()
            .await
            .map_err(|e| McpError::ExternalApi(e.to_string()))?;

        // 結果をJSON形式で返す
        Ok(json!({
            "database_name": schema.database_name,
            "tables": schema.tables.iter().map(|table| json!({
                "name": table.name,
                "schema": table.schema,
                "columns": table.columns.iter().map(|col| json!({
                    "name": col.name,
                    "type": col.data_type,
                    "nullable": col.nullable,
                    "max_length": col.max_length
                })).collect::<Vec<_>>(),
                "primary_keys": table.primary_keys,
                "foreign_keys": table.foreign_keys.iter().map(|fk| json!({
                    "name": fk.name,
                    "column": fk.column,
                    "referenced_table": fk.referenced_table,
                    "referenced_column": fk.referenced_column
                })).collect::<Vec<_>>(),
                "indexes": table.indexes.iter().map(|idx| json!({
                    "name": idx.name,
                    "columns": idx.columns,
                    "is_unique": idx.is_unique,
                    "is_primary": idx.is_primary
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "views": schema.views.iter().map(|view| json!({
                "name": view.name,
                "schema": view.schema,
                "definition": view.definition,
                "columns": view.columns.iter().map(|col| json!({
                    "name": col.name,
                    "type": col.data_type,
                    "nullable": col.nullable
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>(),
            "procedures": schema.procedures.iter().map(|proc| json!({
                "name": proc.name,
                "schema": proc.schema,
                "parameters": proc.parameters.iter().map(|param| json!({
                    "name": param.name,
                    "type": param.data_type,
                    "direction": format!("{:?}", param.direction)
                })).collect::<Vec<_>>(),
                "return_type": proc.return_type
            })).collect::<Vec<_>>()
        }))
    }
    /// パラメータをパース
    fn parse_parameters(&self, params_json: &JsonValue) -> Result<Vec<Value>, McpError> {
        if let Some(array) = params_json.as_array() {
            array.iter().map(|v| self.json_to_value(v)).collect()
        } else {
            Err(McpError::InvalidRequest(
                "Parameters must be an array".to_string(),
            ))
        }
    }

    /// JSONValueをValueに変換
    fn json_to_value(&self, json_val: &JsonValue) -> Result<Value, McpError> {
        match json_val {
            JsonValue::Null => Ok(Value::Null),
            JsonValue::Bool(b) => Ok(Value::Bool(*b)),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Value::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Value::Float(f))
                } else {
                    Err(McpError::InvalidRequest(
                        "Invalid number format".to_string(),
                    ))
                }
            }
            JsonValue::String(s) => Ok(Value::String(s.clone())),
            JsonValue::Array(_) | JsonValue::Object(_) => Ok(Value::Json(json_val.clone())),
        }
    }

    /// ValueをJSONValueに変換
    fn value_to_json(&self, value: &Value) -> JsonValue {
        match value {
            Value::Null => JsonValue::Null,
            Value::Bool(b) => JsonValue::Bool(*b),
            Value::Int(i) => JsonValue::Number((*i).into()),
            Value::Float(f) => {
                JsonValue::Number(serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()))
            }
            Value::String(s) => JsonValue::String(s.clone()),
            Value::Binary(bytes) => {
                JsonValue::String(base64::prelude::BASE64_STANDARD.encode(bytes))
            }
            Value::Json(j) => j.clone(),
            Value::DateTime(dt) => JsonValue::String(dt.to_rfc3339()),
        }
    }
}

#[async_trait]
impl McpHandler for DatabaseHandler {
    async fn initialize(&self, params: InitializeParams) -> Result<JsonValue, McpError> {
        Ok(json!({
            "protocolVersion": crate::mcp::negotiate_protocol_version(&params.protocol_version),
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "database-handler",
                "version": "1.0.0"
            }
        }))
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        let params_schema = json!({
            "type": "array",
            "description": "Values for $1, $2, ... in the SQL (bound as parameters, never inserted into the SQL text). Strings are converted to the type PostgreSQL expects for that parameter (uuid, date, timestamp, numeric, json, bytea as base64, ...), so values returned by execute_query can be passed back as they are. Strings also work for text and enum columns. For other types (interval, money, inet, arrays, ...), pass a string and cast it through text in the SQL, e.g. $1::text::interval",
            "items": {
                "type": ["null", "boolean", "number", "string"]
            }
        });
        let max_rows = self.tool_options.max_rows;
        let mut tools = vec![
            Tool {
                name: "execute_query".to_string(),
                title: Some("Run a read-only SQL query".to_string()),
                description: format!(
                    "Run one SELECT statement on the configured database and return the rows as JSON. \
                     It runs in a read-only transaction that is rolled back, so it cannot change data. \
                     At most {max_rows} rows are returned (truncated is true when there were more): \
                     use LIMIT, WHERE, or aggregates for large tables. Values of types that cannot be \
                     shown are replaced by a note; cast such columns to text."
                ),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "One SELECT statement (WITH ... SELECT is allowed; INSERT/UPDATE/DELETE, also inside WITH, are not)"
                        },
                        "params": params_schema.clone()
                    },
                    "required": ["sql"]
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
                title: Some("Show the database schema".to_string()),
                description: "List the tables of the configured database (schema public) with their columns, types, and primary keys.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {}
                }),
                annotations: Some(ToolAnnotations {
                    read_only_hint: Some(true),
                    open_world_hint: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ];
        if self.tool_options.allow_write {
            tools.push(Tool {
                name: "execute_command".to_string(),
                title: Some("Change data with SQL".to_string()),
                description: "Run one INSERT, UPDATE, or DELETE statement on the configured database and commit it. Returns the number of affected rows. Other statements (CREATE, DROP, ALTER, TRUNCATE, GRANT, ...) are refused.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "sql": {
                            "type": "string",
                            "description": "One INSERT, UPDATE, or DELETE statement"
                        },
                        "params": params_schema
                    },
                    "required": ["sql"]
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
        let args = params.arguments.clone().unwrap_or_default();
        let args_json =
            serde_json::to_value(&args).map_err(|e| McpError::InvalidParams(e.to_string()))?;
        let sql = args_json
            .get("sql")
            .and_then(JsonValue::as_str)
            .map(str::to_string);

        let result = match params.name.as_str() {
            // ツールとしては`engine`を出さない（設定のデータベースだけを使う）
            "execute_query" => {
                let mut args = args_json.clone();
                if let Some(object) = args.as_object_mut() {
                    object.remove("engine");
                }
                self.handle_execute_query(args).await
            }
            "execute_command" => self.handle_execute_command(args_json).await,
            "get_schema" => self.handle_get_schema(args_json).await,
            _ => {
                return Err(McpError::ToolNotFound(format!(
                    "Unknown tool: {}",
                    params.name
                )))
            }
        };
        self.audit_tool_call(&params.name, sql.as_deref(), &result, started)
            .await;
        // MCPのツールの結果（CallToolResult）にする。JSONを本文に入れる
        let value = result?;
        let text =
            serde_json::to_string_pretty(&value).map_err(|e| McpError::Internal(e.to_string()))?;
        Ok(json!({
            "content": [{ "type": "text", "text": text }],
            "isError": false
        }))
    }

    async fn status(&self) -> Option<String> {
        self.status_line.clone()
    }
    async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
        // データベーススキーマをリソースとして公開
        Ok(vec![Resource {
            uri: "db://schema".to_string(),
            name: "Database Schema".to_string(),
            description: Some("Complete database schema information".to_string()),
            mime_type: Some("application/json".to_string()),
        }])
    }

    async fn read_resource(&self, params: ResourceReadParams) -> Result<JsonValue, McpError> {
        match params.uri.as_str() {
            // MCPのリソースの結果（ReadResourceResult）にする（#453）
            "db://schema" => {
                let schema = self.handle_get_schema(json!({})).await?;
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
            _ => Err(McpError::InvalidRequest(format!(
                "Unknown resource: {}",
                params.uri
            ))),
        }
    }
}
