use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::mcp::{
    call_tool_result_for_version, negotiate_protocol_version, tools_list_for_version, Caller,
    InitializeParams, JsonRpcRequest, JsonRpcResponse, McpError, PromptGetParams, PromptSpec,
    Resource, ResourceReadParams, Tool, ToolCallParams, DEFAULT_PROTOCOL_VERSION,
};

#[async_trait]
pub trait McpHandler: Send + Sync {
    async fn initialize(&self, params: InitializeParams) -> Result<serde_json::Value, McpError>;
    async fn list_tools(&self) -> Result<Vec<Tool>, McpError>;
    async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError>;
    async fn list_resources(&self) -> Result<Vec<Resource>, McpError>;
    async fn read_resource(
        &self,
        params: ResourceReadParams,
    ) -> Result<serde_json::Value, McpError>;

    /// `mcp_rs_status`（#434）に出す、このハンドラーの状態の1行（例:
    /// WordPressのサイトURL、プラグインが動いているか）。パスワード・キー等の
    /// 秘密は含めない。既定は無し（名前だけを出す）
    async fn status(&self) -> Option<String> {
        None
    }

    /// `uri`のリソースと同じデータを返す、このハンドラーのツールの名前
    /// （#538）。APIキー・アクセストークンでツールを制限した呼び出し元は、
    /// このツールを使えるときだけ、そのリソースを一覧で見て読める。`None`
    /// （既定）のリソースは、制限のある呼び出し元には出さない。
    fn resource_tool(&self, _uri: &str) -> Option<String> {
        None
    }

    /// このハンドラーのツールの「読み取りだけ」の印（`readOnlyHint`）を、
    /// APIキー・トークンの`@read_only`の判定に使ってよいか（#595）。
    /// mcp-rs自身のハンドラーは`true`（既定）。外部のプラグインは、設定で
    /// 信じると書いたときだけ`true`
    fn trusts_read_only_hints(&self) -> bool {
        true
    }

    /// このハンドラーのリソースのテンプレート（#644）。既定は無し。
    /// テンプレートから作ったURIは、`resource_tool`が`Some`を返すものとして
    /// このハンドラーに振り分ける。`resource_tool`には、テンプレートの文字列
    /// （`wordpress://post/{id}`）も渡す
    async fn list_resource_templates(&self) -> Result<Vec<crate::mcp::ResourceTemplate>, McpError> {
        Ok(Vec::new())
    }

    /// このハンドラーのプロンプト（#643）。既定は無し
    fn prompts(&self) -> Vec<PromptSpec> {
        Vec::new()
    }

    /// プロンプトの中身（#643）。`name`は`prompts`のどれか。引数は、
    /// 必須のものがあることを確かめてから渡す
    async fn get_prompt(
        &self,
        name: &str,
        _arguments: &std::collections::HashMap<String, String>,
    ) -> Result<serde_json::Value, McpError> {
        Err(McpError::InvalidParams(format!("Unknown prompt: {name}")))
    }

    /// mcp-rsの終了の前に呼ぶ（#673）。外部のプラグインは、ここで子プロセス・
    /// コンテナを止める。HTTPモードでは、ハンドラーをHTTPの処理と共有して
    /// いて、破棄では止まらないため。既定は何もしない
    async fn shutdown(&self) {}
}

/// 呼び出し元の制限（`@read_only`等）を当てはめるときに見る、ツールの形
/// （#595）。印を信じないハンドラーのツールからは、「読み取りだけ」の印を
/// 外す。AIのクライアントに返す一覧は、この形ではなく元のまま
fn tool_for_access(tool: &Tool, trusted: bool) -> std::borrow::Cow<'_, Tool> {
    match &tool.annotations {
        Some(annotations) if !trusted && annotations.read_only_hint.is_some() => {
            let mut tool = tool.clone();
            if let Some(annotations) = tool.annotations.as_mut() {
                annotations.read_only_hint = None;
            }
            std::borrow::Cow::Owned(tool)
        }
        _ => std::borrow::Cow::Borrowed(tool),
    }
}

/// 登録されたハンドラーの一覧（登録順）。ツール名の重複時に、どちらが
/// 使われるかを登録順で決めるため、HashMapではなく順序のある一覧で持つ。
type Handlers = Vec<(String, Arc<dyn McpHandler>)>;

/// 呼び出し元の制限で拒否したもの（監査ログの書き分けに使う）
#[derive(Clone, Copy)]
enum Denied {
    Tool,
    Resource,
    Prompt,
}

/// JSON-RPCのリクエストとして読めないメッセージへの応答（#543、JSON-RPC
/// 2.0）。JSONとして読めなければ`-32700`（Parse error）、JSONだがリクエスト
/// でなければ`-32600`（Invalid Request）。バッチ（配列）は、MCPが使わない
/// （2025-06-18で削除）ので`-32600`。`id`が読み取れれば付け、読み取れな
/// ければ`null`。
///
/// クライアントからの応答（`method`が無く、`result`か`error`がある）には
/// 答えない（`None`、PR #555のレビュー）。JSON-RPCでは、応答には応答
/// しない。答えると、クライアントが、同じ`id`の自分の要求への答えと
/// 取り違えうる
fn invalid_message_response(message: &str) -> Option<JsonRpcResponse> {
    let (code, text, id) = match serde_json::from_str::<serde_json::Value>(message) {
        Err(_) => (-32700, "Parse error", None),
        Ok(serde_json::Value::Array(_)) => (
            -32600,
            "Invalid Request: batch requests are not supported",
            None,
        ),
        Ok(value)
            if value.get("method").is_none()
                && (value.get("result").is_some() || value.get("error").is_some()) =>
        {
            debug!("Ignored a JSON-RPC response from the client");
            return None;
        }
        Ok(value) => (
            -32600,
            "Invalid Request",
            value
                .get("id")
                .filter(|id| id.is_string() || id.is_number())
                .cloned(),
        ),
    };
    Some(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: None,
        error: Some(crate::mcp::JsonRpcError {
            code,
            message: text.to_string(),
            data: None,
        }),
        id,
    })
}

/// サーバーの能力。`initialize`と`server/discover`で同じものを返す
fn server_capabilities() -> serde_json::Value {
    serde_json::json!({
        "tools": { "listChanged": false },
        "resources": { "subscribe": false, "listChanged": false },
        "prompts": { "listChanged": false }
    })
}

fn invalid_params(message: String) -> crate::mcp::JsonRpcError {
    crate::mcp::JsonRpcError {
        code: -32602,
        message: format!("Invalid params: {}", message),
        data: None,
    }
}

/// リクエストが2026-07-28のもの（`params._meta`に版がある）なら、その版
/// （#570）。`initialize`と、`_meta`に版の無いリクエストは今までどおり
/// （legacy）で`None`。
///
/// 2026-07-28のリクエストで、版・クライアントの能力が無い・形が違えば
/// `-32602`、対応していない版なら`-32022`（`data`に`supported`・
/// `requested`）。`server/discover`は2026-07-28にしか無いので、版が無ければ
/// `-32602`。
fn modern_request_version(
    request: &JsonRpcRequest,
) -> Result<Option<&'static str>, crate::mcp::JsonRpcError> {
    use crate::mcp::{META_CLIENT_CAPABILITIES, META_PROTOCOL_VERSION};
    if request.method == "initialize" {
        return Ok(None);
    }
    let meta = request
        .params
        .as_ref()
        .and_then(|params| params.get("_meta"));
    let Some(requested) = meta.and_then(|meta| meta.get(META_PROTOCOL_VERSION)) else {
        if request.method == "server/discover" {
            return Err(invalid_params(format!(
                "server/discover requires '{}' in params._meta",
                META_PROTOCOL_VERSION
            )));
        }
        return Ok(None);
    };
    let Some(requested) = requested.as_str() else {
        return Err(invalid_params(format!(
            "'{}' in params._meta must be a string",
            META_PROTOCOL_VERSION
        )));
    };
    if !meta
        .and_then(|meta| meta.get(META_CLIENT_CAPABILITIES))
        .is_some_and(serde_json::Value::is_object)
    {
        return Err(invalid_params(format!(
            "params._meta requires '{}' (an object)",
            META_CLIENT_CAPABILITIES
        )));
    }
    match crate::mcp::MODERN_PROTOCOL_VERSIONS
        .iter()
        .copied()
        .find(|v| *v == requested)
    {
        Some(version) => Ok(Some(version)),
        None => Err(crate::mcp::JsonRpcError {
            code: crate::mcp::UNSUPPORTED_PROTOCOL_VERSION_CODE,
            message: "Unsupported protocol version".to_string(),
            data: Some(serde_json::json!({
                "supported": crate::mcp::all_supported_protocol_versions(),
                "requested": requested
            })),
        }),
    }
}

/// リソースが無いときのエラー（2025-11-25までの版）。2026-07-28では
/// 使わず、`-32602`にする
const RESOURCE_NOT_FOUND_CODE: i32 = -32002;

/// エラーを、リクエストの版の形のJSON-RPCのエラーにする（#570）。
/// リソースが無いときは、legacyなら`-32002`、2026-07-28なら`-32602`
/// （以前は、どちらも`-32603`だった）。`data`に、そのURIを入れる
fn error_for_era(error: McpError, modern: bool) -> crate::mcp::JsonRpcError {
    match error {
        McpError::ResourceNotFound(uri) => crate::mcp::JsonRpcError {
            code: if modern {
                -32602
            } else {
                RESOURCE_NOT_FOUND_CODE
            },
            message: format!("Resource not found: {}", uri),
            data: Some(serde_json::json!({ "uri": uri })),
        },
        error => error.into(),
    }
}

/// `server/discover`の結果（2026-07-28）。`supportedVersions`は、`_meta`で
/// 受ける版だけ（`initialize`で交渉する版は、`server/discover`では使えない
/// ため）。呼び出し元によらず同じなので、`cacheScope`は`public`。
/// `_meta`のサーバーの情報は、アイコン付きで先に入れる（`modern_result`は、
/// 既にあれば上書きしない）。`resultType`は`modern_result`で付ける
fn discover_result() -> serde_json::Value {
    serde_json::json!({
        "supportedVersions": crate::mcp::MODERN_PROTOCOL_VERSIONS,
        "capabilities": server_capabilities(),
        "ttlMs": DISCOVER_TTL_MS,
        "cacheScope": "public",
        // アイコンは、ここと`initialize`だけに付ける（ほかの結果の`_meta`
        // には付けない）
        "_meta": {
            crate::mcp::META_SERVER_INFO:
                crate::mcp::server_info_with_icons(crate::mcp::MODERN_PROTOCOL_VERSION)
        }
    })
}

/// `server/discover`の結果を、クライアントが使い回してよい時間。版・能力は
/// mcp-rsを入れ替えるまで変わらない
const DISCOVER_TTL_MS: u64 = 3_600_000;

/// ツール・リソースの一覧を、クライアントが使い回してよい時間。一覧は、
/// 起動中は変わらない（`listChanged`は`false`）
const LIST_TTL_MS: u64 = 300_000;

/// 2026-07-28のリクエストへの結果に、その版の項目を付ける（#570）。
///
/// - `resultType`: 必須。mcp-rsは途中で入力を求めない（MRTRを使わない）
///   ので、いつも`complete`
/// - `_meta`の`io.modelcontextprotocol/serverInfo`（SHOULD）
/// - 一覧・リソースの読み取りの`ttlMs`・`cacheScope`。一覧は、APIキー・
///   アクセストークンによって変わるので`private`。リソースの中身
///   （投稿等）は変わりうるので、使い回さない（`ttlMs`は0）
fn modern_result(method: &str, version: &str, mut result: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(fields) = &mut result else {
        return result;
    };
    fields.insert("resultType".to_string(), "complete".into());
    let cache = match method {
        "tools/list" | "resources/list" | "resources/templates/list" | "prompts/list" => {
            Some(LIST_TTL_MS)
        }
        "resources/read" | "prompts/get" => Some(0),
        _ => None,
    };
    if let Some(ttl_ms) = cache {
        fields.entry("ttlMs").or_insert(ttl_ms.into());
        fields.entry("cacheScope").or_insert("private".into());
    }
    let meta = fields
        .entry("_meta")
        .or_insert_with(|| serde_json::json!({}));
    if let serde_json::Value::Object(meta) = meta {
        meta.entry(crate::mcp::META_SERVER_INFO)
            .or_insert_with(|| crate::mcp::server_info_for_version(version));
    }
    result
}

pub struct McpServer {
    handlers: Handlers,
    /// 呼び出しを拒否したツールを記録する（#52）
    audit_logger: Option<Arc<crate::security::AuditLogger>>,
    /// `mcp_rs_status`に出す情報（#434）
    status: Arc<crate::mcp::StatusInfo>,
}

/// クライアントとの接続1つ分の状態。`initialize`で決まったプロトコルの版を
/// 持つ（#417）。`initialize`の前は`DEFAULT_PROTOCOL_VERSION`。
#[derive(Debug)]
pub struct Session {
    protocol_version: std::sync::Mutex<&'static str>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            protocol_version: std::sync::Mutex::new(DEFAULT_PROTOCOL_VERSION),
        }
    }
}

impl Session {
    /// この接続で使うプロトコルの版
    pub fn protocol_version(&self) -> &'static str {
        *self
            .protocol_version
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn set_protocol_version(&self, version: &'static str) {
        *self
            .protocol_version
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = version;
    }
}

/// 登録済みのハンドラーでJSON-RPCメッセージを処理する。`McpServer`の
/// STDIOと同じ処理を、HTTP/WebSocket（`HttpTransport`）からも使うため
/// （#400）。複製しても同じハンドラーを共有する。
///
/// HTTP/WebSocketでは、どのクライアントからのリクエストかを区別しないため、
/// プロトコルの版は複製元も含めて1つを共有する（最後の`initialize`で決まる）。
#[derive(Clone)]
pub struct RequestDispatcher {
    handlers: Arc<Handlers>,
    session: Arc<Session>,
    audit_logger: Option<Arc<crate::security::AuditLogger>>,
    status: Arc<crate::mcp::StatusInfo>,
}

impl RequestDispatcher {
    /// 1件のJSON-RPCメッセージを処理する。通知には`Ok(None)`。ツールは
    /// 制限しない（`handle_message_as`で、呼び出し元ごとに制限する）。
    pub async fn handle_message(&self, message: &str) -> Result<Option<JsonRpcResponse>, McpError> {
        self.handle_message_as(message, &Caller::unrestricted())
            .await
    }

    /// `handle_message`と同じ。`caller`が使えるツールだけを一覧に出し、
    /// 呼び出せるようにする（#52）。
    pub async fn handle_message_as(
        &self,
        message: &str,
        caller: &Caller,
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        self.handle_message_with_protocol_version(message, caller, None)
            .await
    }

    /// `handle_message_as`と同じ。`protocol_version`があれば、共有の版
    /// （最後の`initialize`で決まった版）の代わりに、このリクエストだけ
    /// その版で応答の項目を決める。HTTPの`MCP-Protocol-Version`ヘッダー
    /// （#484）。`initialize`には使わない（版は本文で交渉する）。
    pub async fn handle_message_with_protocol_version(
        &self,
        message: &str,
        caller: &Caller,
        protocol_version: Option<&'static str>,
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        self.handle_message_in_session(message, caller, protocol_version, &self.session)
            .await
    }

    /// `handle_message_with_protocol_version`と同じ。共有の接続の状態の
    /// 代わりに、`session`を使う（#570・#596）。HTTPの`/mcp`はリクエスト
    /// ごとに新しい状態を、`/ws`は接続ごとの状態を渡す。ある呼び出し元の
    /// `initialize`が、ほかの呼び出し元の版を変えないように
    pub async fn handle_message_in_session(
        &self,
        message: &str,
        caller: &Caller,
        protocol_version: Option<&'static str>,
        session: &Session,
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        McpServer::process_request_as(
            message,
            &self.handlers,
            session,
            caller,
            self.audit_logger.as_deref(),
            &self.status,
            protocol_version,
        )
        .await
    }
}

#[async_trait]
impl crate::transport::http::JsonRpcRequestHandler for RequestDispatcher {
    async fn handle_jsonrpc(
        &self,
        message: &str,
        caller: &Caller,
    ) -> Result<Option<serde_json::Value>, String> {
        self.handle_jsonrpc_with_protocol_version(message, caller, None)
            .await
    }

    async fn handle_jsonrpc_with_protocol_version(
        &self,
        message: &str,
        caller: &Caller,
        protocol_version: Option<&'static str>,
    ) -> Result<Option<serde_json::Value>, String> {
        match self
            .handle_message_with_protocol_version(message, caller, protocol_version)
            .await
        {
            Ok(Some(response)) => serde_json::to_value(&response)
                .map(Some)
                .map_err(|e| e.to_string()),
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    async fn handle_jsonrpc_in_session(
        &self,
        message: &str,
        caller: &Caller,
        protocol_version: Option<&'static str>,
        session: &Session,
    ) -> Result<Option<serde_json::Value>, String> {
        match self
            .handle_message_in_session(message, caller, protocol_version, session)
            .await
        {
            Ok(Some(response)) => serde_json::to_value(&response)
                .map(Some)
                .map_err(|e| e.to_string()),
            Ok(None) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
            audit_logger: None,
            status: Arc::new(crate::mcp::StatusInfo::default()),
        }
    }

    /// 呼び出しを拒否したツールを記録する監査ログ（#52）。`dispatcher`より
    /// 前に設定する。
    pub fn set_audit_logger(&mut self, audit_logger: Arc<crate::security::AuditLogger>) {
        self.audit_logger = Some(audit_logger);
    }

    /// `mcp_rs_status`（#434）に出す情報（動作モード・設定ファイル・起動
    /// できなかったプラグイン等）。`dispatcher`より前に設定する。
    pub fn set_status_info(&mut self, status: crate::mcp::StatusInfo) {
        self.status = Arc::new(status);
    }

    /// `set_audit_logger`で設定した監査ログ
    pub fn audit_logger(&self) -> Option<Arc<crate::security::AuditLogger>> {
        self.audit_logger.clone()
    }

    /// 登録済みのハンドラーで処理する`RequestDispatcher`を作る。以後に
    /// `add_handler`したハンドラーは含まれない。
    pub fn dispatcher(&self) -> RequestDispatcher {
        RequestDispatcher {
            handlers: Arc::new(self.handlers.clone()),
            session: Arc::new(Session::default()),
            audit_logger: self.audit_logger.clone(),
            status: self.status.clone(),
        }
    }

    /// 登録済みのハンドラーを止める（`McpHandler::shutdown`、#673）。
    /// `dispatcher`で作った`RequestDispatcher`も同じハンドラーを共有するので、
    /// HTTPモードでも、プラグインの子プロセス・コンテナがここで止まる
    pub async fn shutdown(&self) {
        for (_, handler) in &self.handlers {
            handler.shutdown().await;
        }
    }

    /// ハンドラーを登録する。同じ名前が既にあれば置き換える（登録順の
    /// 位置は保つ）。
    pub fn add_handler(&mut self, name: String, handler: Arc<dyn McpHandler>) {
        if let Some(entry) = self.handlers.iter_mut().find(|(n, _)| *n == name) {
            entry.1 = handler;
        } else {
            self.handlers.push((name, handler));
        }
    }

    /// 1行分のJSON-RPCメッセージを処理する。通知（`id`なし）の場合は
    /// `Ok(None)`を返し、呼び出し元は何も書き出さない。JSON-RPC 2.0では
    /// サーバーは通知に応答してはならない（#360）。テスト用（#543で、
    /// これを使っていたTCPのサーバー`McpServer::run`を削除した）
    #[cfg(test)]
    async fn process_request(
        line: &str,
        handlers: &[(String, Arc<dyn McpHandler>)],
        session: &Session,
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        Self::process_request_as(
            line,
            handlers,
            session,
            &Caller::unrestricted(),
            None,
            &crate::mcp::StatusInfo::default(),
            None,
        )
        .await
    }

    /// `process_request`と同じ。`caller`が使えないツールは、`tools/list`に
    /// 出さず、`tools/call`も拒否する（#52）。拒否は`audit_logger`に記録する。
    /// `protocol_version`があれば、`session`の版の代わりにその版で応答の
    /// 項目を決める（HTTPの`MCP-Protocol-Version`ヘッダー、#484）。
    async fn process_request_as(
        line: &str,
        handlers: &[(String, Arc<dyn McpHandler>)],
        session: &Session,
        caller: &Caller,
        audit_logger: Option<&crate::security::AuditLogger>,
        status: &crate::mcp::StatusInfo,
        protocol_version: Option<&'static str>,
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None); // 空の行（STDIOの改行だけ等）は無視する
        }
        let request: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(request) => request,
            // 以前は何も返さず、クライアントは応答を待ち続けた（#543）
            Err(_) => return Ok(invalid_message_response(line)),
        };
        if request.id.is_none() {
            Self::handle_notification(&request.method);
            return Ok(None);
        }

        // `_meta`に版があれば2026-07-28のリクエストとして、`session`を使わず
        // その版で答える（#570）。版は、このリクエストだけのもの
        let modern = match modern_request_version(&request) {
            Ok(modern) => modern,
            Err(error) => {
                return Ok(Some(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(error),
                    id: request.id,
                }))
            }
        };
        let version = modern
            .or(protocol_version)
            .unwrap_or_else(|| session.protocol_version());

        // 注意: ここで `?` を使って早期returnしてはいけない。そうすると
        // request.id を失い、クライアントには何の応答も返らないまま接続が
        // 無言でハングする（JSON-RPC 2.0違反）。失敗はすべて`result`に
        // 畳み込み、関数末尾でrequest.id付きのエラーレスポンスとして返す。
        let result = match request.method.as_str() {
            "initialize" => {
                match serde_json::from_value::<InitializeParams>(
                    request.params.clone().unwrap_or_default(),
                ) {
                    Ok(params) => {
                        let version = negotiate_protocol_version(&params.protocol_version);
                        if version != params.protocol_version {
                            info!(
                                "Client requested MCP protocol version '{}', which is not supported; answering with '{}'",
                                params.protocol_version, version
                            );
                        }
                        session.set_protocol_version(version);
                        Ok(Self::initialize_all(handlers, params, version).await)
                    }
                    Err(e) => Err(McpError::InvalidParams(e.to_string())),
                }
            }
            // `modern_request_version`が、`_meta`に版の無い`server/discover`を
            // 拒否しているので、ここに来るのは2026-07-28のリクエストだけ
            "server/discover" => Ok(discover_result()),
            // legacyの版では、pingに空の結果で答える（MUST）。以前は
            // `-32601`だった。2026-07-28には無いので、`-32601`のまま（#570）
            "ping" if modern.is_none() => Ok(serde_json::json!({})),
            "tools/list" => {
                let mut tools = Self::list_all_tools(handlers).await;
                if caller.access != crate::mcp::ToolAccess::All {
                    // 制限は、印を信じないハンドラーの印を外した形で判定する
                    // （#595）。返す一覧は元のまま
                    let allowed: std::collections::HashSet<String> =
                        Self::list_access_tools(handlers)
                            .await
                            .into_iter()
                            .filter(|tool| caller.access.allows(tool))
                            .map(|tool| tool.name)
                            .collect();
                    tools.retain(|tool| allowed.contains(&tool.name));
                }
                Ok(tools_list_for_version(
                    version,
                    serde_json::json!({ "tools": tools }),
                ))
            }
            "tools/call" => {
                match serde_json::from_value::<ToolCallParams>(
                    request.params.clone().unwrap_or_default(),
                ) {
                    // MCP仕様上、ツール呼び出し自体の失敗（外部APIの
                    // 4xx/5xx等）はJSON-RPCレベルのerrorではなく、
                    // 成功レスポンス内のCallToolResult
                    // {isError: true, content: [...]}として返す。
                    // これによりクライアントは常にtools/callを
                    // "JSON-RPCとしては成功した呼び出し"として扱い、
                    // ツール自体の成否はisError/contentで判断できる。
                    // 一方、tools/call自体のparams形式が不正な場合は
                    // プロトコルレベルの問題なのでJSON-RPCエラーのまま。
                    Ok(params) if !Self::tool_allowed(handlers, caller, &params.name).await => {
                        Self::audit_denied(audit_logger, caller, Denied::Tool, &params.name).await;
                        Ok(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!(
                                    "Tool '{}' is not allowed for this API key or access token",
                                    params.name
                                )
                            }],
                            "isError": true
                        }))
                    }
                    Ok(params) if params.name == crate::mcp::STATUS_TOOL_NAME => Ok(
                        Self::call_status_tool(handlers, version, caller, audit_logger, status)
                            .await,
                    ),
                    Ok(params) => match crate::mcp::with_caller(
                        caller.clone(),
                        Self::call_tool_routed(handlers, params),
                    )
                    .await
                    {
                        Ok(value) => Ok(call_tool_result_for_version(version, value)),
                        Err(e) => Ok(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": e.to_string()
                            }],
                            "isError": true
                        })),
                    },
                    Err(e) => Err(McpError::InvalidParams(e.to_string())),
                }
            }
            "resources/list" => {
                let resources = Self::list_all_resources(handlers, caller).await;
                Ok(serde_json::json!({ "resources": resources }))
            }
            "resources/read" => {
                match serde_json::from_value::<ResourceReadParams>(
                    request.params.clone().unwrap_or_default(),
                ) {
                    Ok(params) => {
                        Self::read_resource_routed(handlers, params, caller, audit_logger).await
                    }
                    Err(e) => Err(McpError::InvalidParams(e.to_string())),
                }
            }
            "resources/templates/list" => {
                let mut templates = Vec::new();
                for (name, handler) in handlers {
                    match handler.list_resource_templates().await {
                        Ok(list) => {
                            for template in list {
                                if Self::resource_allowed(
                                    handler.as_ref(),
                                    caller,
                                    &template.uri_template,
                                )
                                .await
                                {
                                    templates.push(template);
                                }
                            }
                        }
                        Err(e) => warn!(
                            "Failed to list resource templates from handler '{}': {}",
                            name, e
                        ),
                    }
                }
                Ok(serde_json::json!({ "resourceTemplates": templates }))
            }
            "prompts/list" => {
                let mut prompts = Vec::new();
                for (_, spec) in Self::list_all_prompts(handlers).await {
                    if Self::prompt_allowed(handlers, caller, &spec).await {
                        prompts.push(spec.prompt);
                    }
                }
                Ok(serde_json::json!({ "prompts": prompts }))
            }
            "prompts/get" => {
                match serde_json::from_value::<PromptGetParams>(
                    request.params.clone().unwrap_or_default(),
                ) {
                    Ok(params) => {
                        Self::get_prompt_routed(handlers, params, caller, audit_logger).await
                    }
                    Err(e) => Err(McpError::InvalidParams(e.to_string())),
                }
            }
            _ => Err(McpError::InvalidMethod(request.method.clone())),
        };
        let result = match modern {
            Some(version) => result.map(|result| modern_result(&request.method, version, result)),
            None => result,
        };

        Ok(Some(match result {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(result),
                error: None,
                id: request.id,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(error_for_era(e, modern.is_some())),
                id: request.id,
            },
        }))
    }

    /// `initialize`に応答する。応答はサーバー全体（mcp-rs）として返す。
    /// 以前は最初のハンドラーの応答をそのまま返しており、ハンドラーが
    /// 無いとエラーになっていた。各ハンドラーの`initialize`も呼ぶが、
    /// 失敗しても応答は返す（そのハンドラーのツールは後で個別にエラーになる）。
    async fn initialize_all(
        handlers: &[(String, Arc<dyn McpHandler>)],
        params: InitializeParams,
        protocol_version: &str,
    ) -> serde_json::Value {
        for (name, handler) in handlers {
            if let Err(e) = handler.initialize(params.clone()).await {
                warn!("Handler '{}' failed to initialize: {}", name, e);
            }
        }
        serde_json::json!({
            "protocolVersion": protocol_version,
            "capabilities": server_capabilities(),
            "serverInfo": crate::mcp::server_info_with_icons(protocol_version)
        })
    }

    /// 全ハンドラーのプロンプト（#643）。同じ名前は、先に登録したハンドラーのもの
    async fn list_all_prompts(
        handlers: &[(String, Arc<dyn McpHandler>)],
    ) -> Vec<(Arc<dyn McpHandler>, PromptSpec)> {
        let mut prompts: Vec<(Arc<dyn McpHandler>, PromptSpec)> = Vec::new();
        for (_, handler) in handlers {
            for spec in handler.prompts() {
                if prompts
                    .iter()
                    .any(|(_, p)| p.prompt.name == spec.prompt.name)
                {
                    continue;
                }
                prompts.push((handler.clone(), spec));
            }
        }
        prompts
    }

    /// `caller`が、プロンプトの使うツールをすべて使えるか（#643）
    async fn prompt_allowed(
        handlers: &[(String, Arc<dyn McpHandler>)],
        caller: &Caller,
        spec: &PromptSpec,
    ) -> bool {
        for tool in &spec.tools {
            if !Self::tool_allowed(handlers, caller, tool).await {
                return false;
            }
        }
        true
    }

    /// `prompts/get`（#643）。無いプロンプト・必須の引数が無いときは`-32602`。
    /// 呼び出し元が使えないプロンプトは、断って監査ログに残す
    async fn get_prompt_routed(
        handlers: &[(String, Arc<dyn McpHandler>)],
        params: PromptGetParams,
        caller: &Caller,
        audit_logger: Option<&crate::security::AuditLogger>,
    ) -> Result<serde_json::Value, McpError> {
        let Some((handler, spec)) = Self::list_all_prompts(handlers)
            .await
            .into_iter()
            .find(|(_, spec)| spec.prompt.name == params.name)
        else {
            return Err(McpError::InvalidParams(format!(
                "Unknown prompt: {}",
                params.name
            )));
        };
        if !Self::prompt_allowed(handlers, caller, &spec).await {
            Self::audit_denied(audit_logger, caller, Denied::Prompt, &params.name).await;
            return Err(McpError::InvalidParams(format!(
                "Prompt '{}' is not allowed for this API key or access token",
                params.name
            )));
        }
        for argument in spec.prompt.arguments.iter().filter(|a| a.required) {
            let given = params
                .arguments
                .get(&argument.name)
                .is_some_and(|value| !value.trim().is_empty());
            if !given {
                return Err(McpError::InvalidParams(format!(
                    "Prompt '{}' needs the argument '{}'",
                    params.name, argument.name
                )));
            }
        }
        handler.get_prompt(&params.name, &params.arguments).await
    }

    /// `caller`が`name`のツールを使えるか。そのツールが無い場合は`true`
    /// （呼び出しは、これまでどおり「ツールが無い」で失敗する）。
    async fn tool_allowed(
        handlers: &[(String, Arc<dyn McpHandler>)],
        caller: &Caller,
        name: &str,
    ) -> bool {
        if caller.access == crate::mcp::ToolAccess::All {
            return true;
        }
        // 一覧に無いツールは、許さない（#596）。以前は許していたので、
        // 一覧を返せなかったハンドラーのツールを、制限を越えて呼べる
        // 余地があった
        Self::list_access_tools(handlers)
            .await
            .iter()
            .find(|tool| tool.name == name)
            .is_some_and(|tool| caller.access.allows(tool))
    }

    /// 拒否したツールの呼び出し・リソースの読み取りを、監査ログとログに
    /// 記録する。`name`はツール名かリソースのURI。
    async fn audit_denied(
        audit_logger: Option<&crate::security::AuditLogger>,
        caller: &Caller,
        kind: Denied,
        name: &str,
    ) {
        let (what, resource) = match kind {
            Denied::Tool => ("Tool call", "tool"),
            Denied::Resource => ("Resource read", "resource"),
            Denied::Prompt => ("Prompt", "prompt"),
        };
        warn!(
            "Denied {} '{}' by '{}': it is not allowed for this caller",
            what.to_lowercase(),
            name,
            caller.display_name()
        );
        let Some(audit_logger) = audit_logger else {
            return;
        };
        let mut entry = crate::security::AuditLogEntry::new(
            crate::security::AuditLevel::Warning,
            crate::security::AuditCategory::Authorization,
            format!("{} denied: {} by {}", what, name, caller.display_name()),
        )
        .with_action(resource.to_string(), name.to_string())
        .with_result("DENIED".to_string())
        .add_metadata("caller".to_string(), caller.display_name().to_string());
        entry.user_id = Some(caller.display_name().to_string());
        entry.ip_address = caller.ip_address.clone();
        if let Some(subject) = &caller.subject {
            entry = entry.add_metadata("subject".to_string(), subject.clone());
        }
        if let Err(e) = audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", name, e);
        }
    }

    /// `mcp_rs_status`（#434）に答える。呼び出しは監査ログに記録する
    async fn call_status_tool(
        handlers: &[(String, Arc<dyn McpHandler>)],
        protocol_version: &str,
        caller: &Caller,
        audit_logger: Option<&crate::security::AuditLogger>,
        status: &crate::mcp::StatusInfo,
    ) -> serde_json::Value {
        let started = std::time::Instant::now();
        let text =
            crate::mcp::render_status(status, protocol_version, caller, audit_logger, handlers)
                .await;
        if let Some(audit_logger) = audit_logger {
            let name = crate::mcp::STATUS_TOOL_NAME;
            let entry = crate::security::AuditLogEntry::new(
                crate::security::AuditLevel::Info,
                crate::security::AuditCategory::DataAccess,
                format!("Tool call: {} - SUCCESS", name),
            )
            .with_action("mcp-rs".to_string(), name.to_string())
            .with_result("SUCCESS".to_string());
            let entry = crate::mcp::with_caller(caller.clone(), async {
                crate::mcp::audit_tool_call_context(entry, started)
            })
            .await;
            if let Err(e) = audit_logger.log(entry).await {
                warn!("Failed to write audit log entry for {}: {}", name, e);
            }
        }
        serde_json::json!({ "content": [{ "type": "text", "text": text }] })
    }

    /// 全ハンドラーのツールを登録順にまとめる。同じ名前のツールは、先に
    /// 登録したハンドラーのものだけを残す（呼び出しもそちらに振り分ける）。
    /// 一覧を取得できないハンドラーは警告を出して飛ばす。
    /// 先頭は、本体の`mcp_rs_status`（#434）。
    async fn list_all_tools(handlers: &[(String, Arc<dyn McpHandler>)]) -> Vec<Tool> {
        Self::collect_tools(handlers, false).await
    }

    /// `list_all_tools`と同じ一覧を、呼び出し元の制限を当てはめる形
    /// （`tool_for_access`）で返す（#595）
    async fn list_access_tools(handlers: &[(String, Arc<dyn McpHandler>)]) -> Vec<Tool> {
        Self::collect_tools(handlers, true).await
    }

    async fn collect_tools(
        handlers: &[(String, Arc<dyn McpHandler>)],
        for_access: bool,
    ) -> Vec<Tool> {
        let mut tools: Vec<Tool> = vec![crate::mcp::status_tool()];
        for (name, handler) in handlers {
            let trusted = !for_access || handler.trusts_read_only_hints();
            match handler.list_tools().await {
                Ok(handler_tools) => {
                    for tool in handler_tools {
                        if tools.iter().any(|t| t.name == tool.name) {
                            if !for_access {
                                warn!(
                                    "Tool '{}' from handler '{}' is hidden: another handler registered earlier provides a tool with the same name",
                                    tool.name, name
                                );
                            }
                        } else {
                            tools.push(tool_for_access(&tool, trusted).into_owned());
                        }
                    }
                }
                Err(e) => {
                    if !for_access {
                        warn!("Failed to list tools from handler '{}': {}", name, e)
                    }
                }
            }
        }
        tools
    }

    /// ツール名から呼び出し先のハンドラーを決めて呼ぶ。`list_all_tools`と
    /// 同じく、登録順で最初にそのツールを持つハンドラーを使う。
    async fn call_tool_routed(
        handlers: &[(String, Arc<dyn McpHandler>)],
        params: ToolCallParams,
    ) -> Result<serde_json::Value, McpError> {
        for (name, handler) in handlers {
            match handler.list_tools().await {
                Ok(tools) if tools.iter().any(|t| t.name == params.name) => {
                    return handler.call_tool(params).await;
                }
                Ok(_) => {}
                Err(e) => warn!("Failed to list tools from handler '{}': {}", name, e),
            }
        }
        Err(McpError::ToolNotFound(params.name))
    }

    /// `caller`が、`handler`の`uri`のリソースを読めるか（#538）。制限のある
    /// 呼び出し元は、同じデータを返すツール（`resource_tool`）を使えるとき
    /// だけ読める。対応するツールが無いリソースは読めない。
    async fn resource_allowed(handler: &dyn McpHandler, caller: &Caller, uri: &str) -> bool {
        if caller.access == crate::mcp::ToolAccess::All {
            return true;
        }
        let Some(tool_name) = handler.resource_tool(uri) else {
            return false;
        };
        let trusted = handler.trusts_read_only_hints();
        match handler.list_tools().await {
            Ok(tools) => tools
                .iter()
                .find(|tool| tool.name == tool_name)
                .is_some_and(|tool| caller.access.allows(&tool_for_access(tool, trusted))),
            Err(_) => false,
        }
    }

    /// 全ハンドラーのリソースのうち、`caller`が読めるものを登録順にまとめる。
    async fn list_all_resources(
        handlers: &[(String, Arc<dyn McpHandler>)],
        caller: &Caller,
    ) -> Vec<Resource> {
        let mut resources = Vec::new();
        for (name, handler) in handlers {
            match handler.list_resources().await {
                Ok(handler_resources) => {
                    for resource in handler_resources {
                        if Self::resource_allowed(handler.as_ref(), caller, &resource.uri).await {
                            resources.push(resource);
                        }
                    }
                }
                Err(e) => warn!("Failed to list resources from handler '{}': {}", name, e),
            }
        }
        resources
    }

    /// URIから読み出し先のハンドラーを決めて読む。`caller`が読めない
    /// リソースは拒否する（#538）。読み取り・拒否は監査ログに記録する。
    async fn read_resource_routed(
        handlers: &[(String, Arc<dyn McpHandler>)],
        params: ResourceReadParams,
        caller: &Caller,
        audit_logger: Option<&crate::security::AuditLogger>,
    ) -> Result<serde_json::Value, McpError> {
        // 一覧にあるURIを持つハンドラー、無ければ、テンプレートから作ったURIを
        // 引き受けるハンドラー（#644）
        let mut owner = None;
        for (name, handler) in handlers {
            match handler.list_resources().await {
                Ok(resources) if resources.iter().any(|r| r.uri == params.uri) => {
                    owner = Some((name, handler));
                    break;
                }
                Ok(_) => {}
                Err(e) => warn!("Failed to list resources from handler '{}': {}", name, e),
            }
        }
        if owner.is_none() {
            for (name, handler) in handlers {
                let has_templates = handler
                    .list_resource_templates()
                    .await
                    .is_ok_and(|templates| !templates.is_empty());
                if has_templates && handler.resource_tool(&params.uri).is_some() {
                    owner = Some((name, handler));
                    break;
                }
            }
        }
        let Some((name, handler)) = owner else {
            return Err(McpError::ResourceNotFound(params.uri));
        };
        let uri = params.uri.clone();
        if !Self::resource_allowed(handler.as_ref(), caller, &uri).await {
            Self::audit_denied(audit_logger, caller, Denied::Resource, &uri).await;
            return Err(McpError::InvalidRequest(format!(
                "Resource '{}' is not allowed for this API key or access token",
                uri
            )));
        }
        let started = std::time::Instant::now();
        let result = crate::mcp::with_caller(caller.clone(), handler.read_resource(params)).await;
        Self::audit_resource_read(audit_logger, caller, name, &uri, &result, started).await;
        result
    }

    /// リソースの読み取りを、監査ログに記録する（#538）。
    async fn audit_resource_read(
        audit_logger: Option<&crate::security::AuditLogger>,
        caller: &Caller,
        handler_name: &str,
        uri: &str,
        result: &Result<serde_json::Value, McpError>,
        started: std::time::Instant,
    ) {
        let Some(audit_logger) = audit_logger else {
            return;
        };
        let outcome = if result.is_ok() { "SUCCESS" } else { "FAILURE" };
        let mut entry = crate::security::AuditLogEntry::new(
            if result.is_ok() {
                crate::security::AuditLevel::Info
            } else {
                crate::security::AuditLevel::Error
            },
            crate::security::AuditCategory::DataAccess,
            format!("Resource read: {} - {}", uri, outcome),
        )
        .with_action(handler_name.to_string(), uri.to_string())
        .with_result(outcome.to_string());
        if let Err(e) = result {
            entry = entry.add_metadata("error".to_string(), e.to_string());
        }
        let entry = crate::mcp::with_caller(caller.clone(), async {
            crate::mcp::audit_tool_call_context(entry, started)
        })
        .await;
        if let Err(e) = audit_logger.log(entry).await {
            warn!("Failed to write audit log entry for {}: {}", uri, e);
        }
    }

    /// 通知を受け取る。現状、サーバー側で対応が必要な通知はないため
    /// 記録のみ行う。
    ///
    /// `tools/call`等のリクエスト用のmethodが`id`なしで届いた場合も実行
    /// しない。MCPではリクエストに`id`が必須で、結果を返せないまま
    /// 投稿の削除等の副作用だけが起きるのを避けるため。
    fn handle_notification(method: &str) {
        if method.starts_with("notifications/") {
            debug!("Received notification: {}", method);
        } else {
            warn!(
                "Ignored '{}' without an id: MCP requests require an id, and notifications get no response",
                method
            );
        }
    }

    pub async fn run_stdio(&self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncWriteExt, BufReader};

        info!("MCP Server running on stdio");

        let stdin = stdin();
        let mut stdout = stdout();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();
        let session = Session::default();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                break; // EOF
            }

            let response = match Self::process_request_as(
                &line,
                &self.handlers,
                &session,
                &Caller::unrestricted(),
                self.audit_logger.as_deref(),
                &self.status,
                None,
            )
            .await
            {
                Ok(Some(resp)) => resp,
                // 通知（idなし）には応答しない（JSON-RPC 2.0）
                Ok(None) => continue,
                Err(e) => {
                    error!("Error processing request: {}", e);
                    continue;
                }
            };

            let response_str = serde_json::to_string(&response)?;
            stdout.write_all(response_str.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHandler;

    #[async_trait]
    impl McpHandler for MockHandler {
        async fn initialize(
            &self,
            params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({ "protocolVersion": params.protocol_version }))
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![])
        }

        async fn call_tool(&self, _params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(vec![])
        }

        async fn read_resource(
            &self,
            _params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
    }

    fn handlers_with_mock() -> Handlers {
        let handlers: Handlers = vec![("mock".to_string(), Arc::new(MockHandler))];
        handlers
    }

    /// call_toolが常に失敗するモック。外部API（WordPress等）の4xx/5xx
    /// エラーをシミュレートする。
    struct FailingToolMockHandler;

    #[async_trait]
    impl McpHandler for FailingToolMockHandler {
        async fn initialize(
            &self,
            _params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![tool("delete_post")])
        }

        async fn call_tool(&self, _params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Err(McpError::ExternalApi(
                "WordPress API error 404 Not Found: Invalid post ID. (rest_post_invalid_id)"
                    .to_string(),
            ))
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(vec![])
        }

        async fn read_resource(
            &self,
            _params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
    }

    fn handlers_with_failing_tool() -> Handlers {
        let handlers: Handlers = vec![("mock".to_string(), Arc::new(FailingToolMockHandler))];
        handlers
    }

    #[tokio::test]
    async fn test_initialize_with_real_client_wire_format_succeeds() {
        // Claude Desktopなど実クライアントが実際に送る形式（camelCase）。
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "claude-desktop", "version": "1.0.0" }
            }
        })
        .to_string();

        let response =
            McpServer::process_request(&line, &handlers_with_mock(), &Session::default())
                .await
                .unwrap()
                .expect("a request with an id must get a response");

        assert!(response.error.is_none(), "expected success, got error");
        assert_eq!(response.id, Some(serde_json::json!(1)));
    }

    #[tokio::test]
    async fn test_malformed_params_return_error_response_not_silent_drop() {
        // paramsが完全に欠落 -> InitializeParamsへのデシリアライズが失敗する
        // ケース。以前はここで`?`が関数全体を早期returnし、呼び出し元の
        // run_stdio/handle_connectionは応答を送らず無言でループを継続して
        // いた。修正後はrequest.id付きの正式なJSON-RPCエラー応答になる。
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "initialize"
        })
        .to_string();

        let response =
            McpServer::process_request(&line, &handlers_with_mock(), &Session::default())
                .await
                .unwrap()
                .expect("a request with an id must get a response");

        assert!(response.error.is_some());
        assert_eq!(response.id, Some(serde_json::json!(42)));
        assert_eq!(response.error.unwrap().code, -32602); // Invalid params
    }

    #[tokio::test]
    async fn test_unknown_method_returns_error_response() {
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "not/a/real/method"
        })
        .to_string();

        let response =
            McpServer::process_request(&line, &handlers_with_mock(), &Session::default())
                .await
                .unwrap()
                .expect("a request with an id must get a response");

        assert!(response.error.is_some());
        assert_eq!(response.id, Some(serde_json::json!(7)));
    }

    #[tokio::test]
    async fn test_tool_call_failure_returns_call_tool_result_not_jsonrpc_error() {
        // MCP仕様: ツール実行自体の失敗（外部APIの4xx/5xx等）は
        // JSON-RPCレベルのerrorではなく、成功レスポンス内の
        // CallToolResult{isError: true, content: [...]}として返すべき。
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": "delete_post",
                "arguments": { "post_id": 999999 }
            }
        })
        .to_string();

        let response =
            McpServer::process_request(&line, &handlers_with_failing_tool(), &Session::default())
                .await
                .unwrap()
                .expect("a request with an id must get a response");

        // JSON-RPCレベルでは成功（errorフィールドが無い）
        assert!(
            response.error.is_none(),
            "tool failure must not be a JSON-RPC error"
        );
        assert_eq!(response.id, Some(serde_json::json!(99)));

        let result = response.result.expect("expected a result payload");
        assert_eq!(result["isError"], serde_json::json!(true));
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("rest_post_invalid_id"));
        assert!(text.contains("404"));
    }

    #[tokio::test]
    async fn test_notifications_get_no_response() {
        // #360: クライアントが初期化後に送るnotifications/initialized等に、
        // 以前は-32601のエラー応答（"id":null）を返していた。
        for method in [
            "notifications/initialized",
            "notifications/cancelled",
            "notifications/progress",
            "notifications/unknown-future-notification",
        ] {
            let line = serde_json::json!({ "jsonrpc": "2.0", "method": method }).to_string();
            let response =
                McpServer::process_request(&line, &handlers_with_mock(), &Session::default())
                    .await
                    .unwrap();
            assert!(response.is_none(), "{method} must not get a response");
        }
    }

    /// `tools/call`等のmethodが`id`なしで届いても、ツールを実行せず、
    /// 応答も返さない。
    struct PanickingToolMockHandler;

    #[async_trait]
    impl McpHandler for PanickingToolMockHandler {
        async fn initialize(
            &self,
            _params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            panic!("initialize must not run for a notification")
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            panic!("list_tools must not run for a notification")
        }

        async fn call_tool(&self, _params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            panic!("call_tool must not run for a notification")
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            panic!("list_resources must not run for a notification")
        }

        async fn read_resource(
            &self,
            _params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            panic!("read_resource must not run for a notification")
        }
    }

    #[tokio::test]
    async fn test_request_method_without_id_is_not_executed() {
        let handlers: Handlers = vec![("mock".to_string(), Arc::new(PanickingToolMockHandler))];

        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": "delete_post", "arguments": { "post_id": 1 } }
        })
        .to_string();

        let response = McpServer::process_request(&line, &handlers, &Session::default())
            .await
            .unwrap();
        assert!(response.is_none());
    }

    fn tool(name: &str) -> Tool {
        Tool {
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object"}),
            ..Default::default()
        }
    }

    /// 決まったツール・リソースを持ち、呼ばれたツール名を自分の名前付きで
    /// 返すモック。複数のハンドラーへの振り分けを確かめるために使う。
    struct NamedHandler {
        label: &'static str,
        tools: Vec<&'static str>,
        resources: Vec<&'static str>,
    }

    #[async_trait]
    impl McpHandler for NamedHandler {
        async fn initialize(
            &self,
            _params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(self.tools.iter().map(|name| tool(name)).collect())
        }

        async fn call_tool(&self, params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({
                "content": [{"type": "text", "text": format!("{}:{}", self.label, params.name)}],
                "isError": false
            }))
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(self
                .resources
                .iter()
                .map(|uri| Resource {
                    uri: uri.to_string(),
                    name: uri.to_string(),
                    description: None,
                    mime_type: None,
                })
                .collect())
        }

        async fn read_resource(
            &self,
            params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({ "from": self.label, "uri": params.uri }))
        }
    }

    /// list_toolsが失敗するモック（起動に失敗したプラグイン等）
    struct BrokenHandler;

    #[async_trait]
    impl McpHandler for BrokenHandler {
        async fn initialize(
            &self,
            _params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            Err(McpError::Internal("not running".to_string()))
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Err(McpError::Internal("not running".to_string()))
        }

        async fn call_tool(&self, _params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Err(McpError::Internal("not running".to_string()))
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Err(McpError::Internal("not running".to_string()))
        }

        async fn read_resource(
            &self,
            _params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Err(McpError::Internal("not running".to_string()))
        }
    }

    fn two_handlers() -> Handlers {
        vec![
            (
                "first".to_string(),
                Arc::new(NamedHandler {
                    label: "first",
                    tools: vec!["get_post", "shared"],
                    resources: vec!["first://items"],
                }) as Arc<dyn McpHandler>,
            ),
            ("broken".to_string(), Arc::new(BrokenHandler)),
            (
                "second".to_string(),
                Arc::new(NamedHandler {
                    label: "second",
                    tools: vec!["plugin__echo", "shared"],
                    resources: vec!["second://items"],
                }),
            ),
        ]
    }

    async fn request(
        handlers: &Handlers,
        method: &str,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        })
        .to_string();
        McpServer::process_request(&line, handlers, &Session::default())
            .await
            .unwrap()
            .expect("a request with an id must get a response")
    }

    #[tokio::test]
    async fn test_tools_list_merges_all_handlers_in_registration_order() {
        let response = request(&two_handlers(), "tools/list", serde_json::json!({})).await;
        let names: Vec<String> = response.result.unwrap()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        // 一覧を取得できないハンドラーは飛ばし、重複した名前は先の
        // ハンドラーのものだけを残す
        // 先頭は本体のmcp_rs_status（#434）
        assert_eq!(
            names,
            vec!["mcp_rs_status", "get_post", "shared", "plugin__echo"]
        );
    }

    #[tokio::test]
    async fn test_tools_call_is_routed_by_tool_name() {
        let handlers = two_handlers();
        for (name, expected) in [
            ("get_post", "first:get_post"),
            ("plugin__echo", "second:plugin__echo"),
            // 重複した名前は、一覧と同じく先に登録したハンドラーへ
            ("shared", "first:shared"),
        ] {
            let response = request(
                &handlers,
                "tools/call",
                serde_json::json!({ "name": name, "arguments": {} }),
            )
            .await;
            let result = response.result.unwrap();
            assert_eq!(result["content"][0]["text"], serde_json::json!(expected));
        }
    }

    #[tokio::test]
    async fn test_unknown_tool_returns_tool_error_result() {
        let response = request(
            &two_handlers(),
            "tools/call",
            serde_json::json!({ "name": "no_such_tool", "arguments": {} }),
        )
        .await;
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result["isError"], serde_json::json!(true));
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no_such_tool"));
    }

    #[tokio::test]
    async fn test_resources_are_merged_and_read_is_routed_by_uri() {
        let handlers = two_handlers();
        let response = request(&handlers, "resources/list", serde_json::json!({})).await;
        let uris: Vec<String> = response.result.unwrap()["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["uri"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(uris, vec!["first://items", "second://items"]);

        let response = request(
            &handlers,
            "resources/read",
            serde_json::json!({ "uri": "second://items" }),
        )
        .await;
        assert_eq!(
            response.result.unwrap()["from"],
            serde_json::json!("second")
        );

        let response = request(
            &handlers,
            "resources/read",
            serde_json::json!({ "uri": "unknown://x" }),
        )
        .await;
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn test_initialize_answers_as_mcp_rs_even_without_handlers() {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "claude-desktop", "version": "1.0.0" }
        });
        for handlers in [Vec::new(), two_handlers()] {
            let response = request(&handlers, "initialize", params.clone()).await;
            assert!(response.error.is_none());
            let result = response.result.unwrap();
            assert_eq!(result["serverInfo"]["name"], serde_json::json!("mcp-rs"));
            assert_eq!(
                result["serverInfo"]["version"],
                serde_json::json!(env!("CARGO_PKG_VERSION"))
            );
        }

        // ハンドラーが無くても、ツール一覧に応答する（本体のmcp_rs_statusだけ）
        let response = request(&Vec::new(), "tools/list", serde_json::json!({})).await;
        let tools = response.result.unwrap()["tools"].clone();
        assert_eq!(tools.as_array().unwrap().len(), 1);
        assert_eq!(tools[0]["name"], serde_json::json!("mcp_rs_status"));
    }

    /// 新しい版の項目（outputSchema・annotations・structuredContent）を
    /// 返すモック
    struct StructuredHandler;

    #[async_trait]
    impl McpHandler for StructuredHandler {
        async fn initialize(
            &self,
            _params: InitializeParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }

        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![Tool {
                output_schema: Some(serde_json::json!({ "type": "object" })),
                annotations: Some(crate::mcp::ToolAnnotations {
                    read_only_hint: Some(true),
                    ..Default::default()
                }),
                ..tool("structured")
            }])
        }

        async fn call_tool(&self, _params: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({
                "content": [{ "type": "text", "text": "{\"n\":1}" }],
                "structuredContent": { "n": 1 },
                "isError": false
            }))
        }

        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(vec![])
        }

        async fn read_resource(
            &self,
            _params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
    }

    /// 同じ接続（`Session`）で、`initialize`→`tools/list`→`tools/call`を送る。
    /// 戻り値は（initializeの結果の版、ツール定義、ツールの結果）。
    async fn exchange_with_version(
        requested: &str,
    ) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
        let handlers: Handlers = vec![("s".to_string(), Arc::new(StructuredHandler))];
        let session = Session::default();
        let send = |method: &str, params: serde_json::Value| {
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                .to_string()
        };
        let mut results = Vec::new();
        for line in [
            send(
                "initialize",
                serde_json::json!({
                    "protocolVersion": requested,
                    "capabilities": {},
                    "clientInfo": { "name": "c", "version": "1" }
                }),
            ),
            send("tools/list", serde_json::json!({})),
            send(
                "tools/call",
                serde_json::json!({ "name": "structured", "arguments": {} }),
            ),
        ] {
            let response = McpServer::process_request(&line, &handlers, &session)
                .await
                .unwrap()
                .unwrap();
            results.push(response.result.unwrap());
        }
        (
            results[0]["protocolVersion"].clone(),
            results[1]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["name"] == "structured")
                .unwrap()
                .clone(),
            results[2].clone(),
        )
    }

    #[tokio::test]
    async fn test_protocol_version_is_negotiated_per_session() {
        // 新しい版: 新しい項目をそのまま渡す
        for requested in ["2025-11-25", "2025-06-18"] {
            let (version, tool, result) = exchange_with_version(requested).await;
            assert_eq!(version, serde_json::json!(requested));
            assert_eq!(
                tool["outputSchema"],
                serde_json::json!({ "type": "object" })
            );
            assert_eq!(tool["annotations"]["readOnlyHint"], serde_json::json!(true));
            assert_eq!(result["structuredContent"], serde_json::json!({ "n": 1 }));
        }

        // 古い版: その版に無い項目を除く
        let (version, tool, result) = exchange_with_version("2024-11-05").await;
        assert_eq!(version, serde_json::json!("2024-11-05"));
        assert!(tool.get("outputSchema").is_none());
        assert!(tool.get("annotations").is_none());
        assert!(result.get("structuredContent").is_none());
        assert_eq!(result["content"][0]["text"], serde_json::json!("{\"n\":1}"));

        // 対応していない版: 対応する最新の版で答える
        let (version, tool, _) = exchange_with_version("2025-03-26").await;
        assert_eq!(version, serde_json::json!("2025-11-25"));
        assert!(tool.get("outputSchema").is_some());
    }

    /// HTTPの`MCP-Protocol-Version`ヘッダーの版（#484）は、共有の版（最後の
    /// `initialize`）より優先する
    #[tokio::test]
    async fn test_dispatcher_uses_the_per_request_protocol_version() {
        let mut server = McpServer::new();
        server.add_handler("s".to_string(), Arc::new(StructuredHandler));
        let dispatcher = server.dispatcher();
        let send = |method: &str, params: serde_json::Value| {
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                .to_string()
        };
        // 別のクライアントが古い版でinitializeした
        dispatcher
            .handle_message(&send(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "old", "version": "1" }
                }),
            ))
            .await
            .unwrap();

        let structured_tool = |version: Option<&'static str>| {
            let dispatcher = dispatcher.clone();
            let line = send("tools/list", serde_json::json!({}));
            async move {
                let response = dispatcher
                    .handle_message_with_protocol_version(&line, &Caller::unrestricted(), version)
                    .await
                    .unwrap()
                    .unwrap();
                response.result.unwrap()["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["name"] == "structured")
                    .unwrap()
                    .clone()
            }
        };
        // ヘッダーが無ければ、共有の版（2024-11-05）
        assert!(structured_tool(None).await.get("outputSchema").is_none());
        // ヘッダーの版（2025-11-25）が優先する
        assert!(structured_tool(Some("2025-11-25"))
            .await
            .get("outputSchema")
            .is_some());
    }

    #[tokio::test]
    async fn test_server_info_follows_the_negotiated_version() {
        let server_info = |requested: &str| {
            let params = serde_json::json!({
                "protocolVersion": requested,
                "capabilities": {},
                "clientInfo": { "name": "c", "version": "1" }
            });
            async move {
                request(&Vec::new(), "initialize", params)
                    .await
                    .result
                    .unwrap()["serverInfo"]
                    .clone()
            }
        };

        // 2025-11-25: description・websiteUrlを付ける
        let latest = server_info("2025-11-25").await;
        assert_eq!(latest["name"], serde_json::json!("mcp-rs"));
        assert!(latest["description"].is_string());
        assert!(latest["websiteUrl"].is_string());

        // それより前の版: その版に無い項目は付けない
        for old in ["2025-06-18", "2024-11-05"] {
            let info = server_info(old).await;
            assert!(info.get("description").is_none(), "version {}", old);
            assert!(info.get("websiteUrl").is_none(), "version {}", old);
        }
    }

    #[tokio::test]
    async fn test_responses_before_initialize_use_the_oldest_version() {
        let handlers: Handlers = vec![("s".to_string(), Arc::new(StructuredHandler))];
        let response = request(&handlers, "tools/list", serde_json::json!({})).await;
        let tools = response.result.unwrap()["tools"].clone();
        let structured = tools
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "structured")
            .unwrap()
            .clone();
        assert!(structured.get("outputSchema").is_none());
        // mcp_rs_statusのtitle・annotations（新しい版の項目）も出さない
        assert!(tools[0].get("annotations").is_none());
    }

    /// `get_post`（読み取り専用）と`delete_post`（読み取り専用でない）を持つ
    /// ハンドラー（#52）
    fn handlers_with_annotated_tools() -> Handlers {
        handlers_with_annotated_tools_trusting(true)
    }

    /// `handlers_with_annotated_tools`と同じ。`trusted`が`false`のときは、
    /// 外部のプラグインのように、印を信じないハンドラーにする（#595）
    fn handlers_with_annotated_tools_trusting(trusted: bool) -> Handlers {
        struct Annotated {
            trusted: bool,
        }
        #[async_trait]
        impl McpHandler for Annotated {
            fn trusts_read_only_hints(&self) -> bool {
                self.trusted
            }
            async fn initialize(
                &self,
                _params: InitializeParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({}))
            }
            async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
                let annotated = |name: &str, read_only: bool| Tool {
                    annotations: Some(crate::mcp::ToolAnnotations {
                        read_only_hint: Some(read_only),
                        ..Default::default()
                    }),
                    ..tool(name)
                };
                Ok(vec![
                    annotated("get_post", true),
                    annotated("delete_post", false),
                ])
            }
            async fn call_tool(
                &self,
                params: ToolCallParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({
                    "content": [{ "type": "text", "text": format!("called {}", params.name) }]
                }))
            }
            async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
                Ok(vec![])
            }
            async fn read_resource(
                &self,
                _params: ResourceReadParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({}))
            }
        }
        vec![("a".to_string(), Arc::new(Annotated { trusted }))]
    }

    /// 印を信じないハンドラー（外部のプラグイン）のツールは、`@read_only`に
    /// 含めない。一覧に返す印は元のまま（#595）
    #[tokio::test]
    async fn test_read_only_preset_ignores_untrusted_hints() {
        let mut server = McpServer::new();
        for (name, handler) in handlers_with_annotated_tools_trusting(false) {
            server.add_handler(name, handler);
        }
        let dispatcher = server.dispatcher();
        let send = |method: &str, params: serde_json::Value| {
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                .to_string()
        };
        // annotationsは2025-03-26以降の版の項目なので、新しい版で始める
        dispatcher
            .handle_message_as(
                &send(
                    "initialize",
                    serde_json::json!({
                        "protocolVersion": "2025-11-25",
                        "capabilities": {},
                        "clientInfo": { "name": "c", "version": "1" }
                    }),
                ),
                &Caller::default(),
            )
            .await
            .unwrap();
        let reader = Caller::new(
            "api_key:reader",
            crate::mcp::ToolAccess::from_patterns(Some(&["@read_only".to_string()])).unwrap(),
        );
        let list = |caller: Caller| {
            let dispatcher = &dispatcher;
            let message = send("tools/list", serde_json::json!({}));
            async move {
                dispatcher
                    .handle_message_as(&message, &caller)
                    .await
                    .unwrap()
                    .unwrap()
                    .result
                    .unwrap()["tools"]
                    .clone()
            }
        };
        let names = |tools: &serde_json::Value| -> Vec<String> {
            tools
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect()
        };
        // 印を信じないので、get_postは@read_onlyに含まれない
        assert_eq!(names(&list(reader.clone()).await), vec!["mcp_rs_status"]);
        let denied = dispatcher
            .handle_message_as(
                &send(
                    "tools/call",
                    serde_json::json!({ "name": "get_post", "arguments": {} }),
                ),
                &reader,
            )
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert_eq!(denied["isError"], serde_json::json!(true));

        // 名前で許せば使える。返す一覧の印は、元のまま（readOnlyHint: true）
        let by_name = Caller::new(
            "api_key:named",
            crate::mcp::ToolAccess::from_patterns(Some(&["get_post".to_string()])).unwrap(),
        );
        let tools = list(by_name).await;
        assert_eq!(names(&tools), vec!["get_post"]);
        assert_eq!(
            tools[0]["annotations"]["readOnlyHint"],
            serde_json::json!(true)
        );

        // 制限の無い呼び出し元（STDIO等）には、すべてを元の印のまま返す
        let all = list(Caller::default()).await;
        assert_eq!(
            names(&all),
            vec!["mcp_rs_status", "get_post", "delete_post"]
        );
    }

    #[tokio::test]
    async fn test_restricted_caller_sees_and_calls_only_allowed_tools() {
        let audit_logger = Arc::new(crate::security::AuditLogger::with_defaults());
        let mut server = McpServer::new();
        server.set_audit_logger(audit_logger.clone());
        for (name, handler) in handlers_with_annotated_tools() {
            server.add_handler(name, handler);
        }
        let dispatcher = server.dispatcher();
        let caller = Caller::new(
            "api_key:reader",
            crate::mcp::ToolAccess::from_patterns(Some(&["@read_only".to_string()])).unwrap(),
        );
        let send = |method: &str, params: serde_json::Value| {
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                .to_string()
        };

        let response = dispatcher
            .handle_message_as(&send("tools/list", serde_json::json!({})), &caller)
            .await
            .unwrap()
            .unwrap();
        let names: Vec<String> = response.result.unwrap()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        // mcp_rs_statusも読み取り専用なので見える
        assert_eq!(names, vec!["mcp_rs_status", "get_post"]);

        let call = |name: &str| {
            send(
                "tools/call",
                serde_json::json!({ "name": name, "arguments": {} }),
            )
        };
        let allowed = dispatcher
            .handle_message_as(&call("get_post"), &caller)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert_eq!(
            allowed["content"][0]["text"],
            serde_json::json!("called get_post")
        );

        let denied = dispatcher
            .handle_message_as(&call("delete_post"), &caller)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert_eq!(denied["isError"], serde_json::json!(true));
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not allowed"));

        // 拒否は監査ログに残る
        let entries = audit_logger.get_all_entries().await;
        let entry = entries
            .iter()
            .find(|e| e.message.contains("delete_post"))
            .expect("denied call is audited");
        assert_eq!(entry.user_id.as_deref(), Some("api_key:reader"));
        assert_eq!(entry.result.as_deref(), Some("DENIED"));

        // 一覧に無いツールは、制限のある呼び出し元には「許可されていない」
        // （#596）。一覧を返せなかったハンドラーのツールを、制限を越えて
        // 呼ばせない。どのツールがあるかも知らせない
        let missing = dispatcher
            .handle_message_as(&call("no_such_tool"), &caller)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert_eq!(missing["isError"], serde_json::json!(true));
        assert!(missing["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not allowed"));

        // 制限の無い呼び出し元は、すべて使える
        let response = dispatcher
            .handle_message(&send("tools/list", serde_json::json!({})))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            response.result.unwrap()["tools"].as_array().unwrap().len(),
            3
        );
    }

    /// 制限のある呼び出し元は、対応するツールを使えるリソースだけを見て
    /// 読める。対応するツールの無いリソースは読めない。読み取り・拒否は
    /// 監査ログに残る（#538）
    #[tokio::test]
    async fn test_restricted_caller_reads_only_resources_of_allowed_tools() {
        struct WithResources;
        #[async_trait]
        impl McpHandler for WithResources {
            async fn initialize(
                &self,
                _params: InitializeParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({}))
            }
            async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
                Ok(vec![tool("get_items"), tool("get_secrets")])
            }
            async fn call_tool(
                &self,
                _params: ToolCallParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({ "content": [] }))
            }
            async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
                Ok(["x://items", "x://secrets", "x://unmapped"]
                    .iter()
                    .map(|uri| Resource {
                        uri: uri.to_string(),
                        name: uri.to_string(),
                        description: None,
                        mime_type: None,
                    })
                    .collect())
            }
            async fn read_resource(
                &self,
                params: ResourceReadParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({ "read": params.uri }))
            }
            fn resource_tool(&self, uri: &str) -> Option<String> {
                match uri {
                    "x://items" => Some("get_items".to_string()),
                    "x://secrets" => Some("get_secrets".to_string()),
                    _ => None,
                }
            }
        }

        let audit_logger = Arc::new(crate::security::AuditLogger::with_defaults());
        let mut server = McpServer::new();
        server.set_audit_logger(audit_logger.clone());
        server.add_handler("x".to_string(), Arc::new(WithResources));
        let dispatcher = server.dispatcher();
        let send = |method: &str, params: serde_json::Value| {
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                .to_string()
        };
        let list = |caller: Caller| {
            let dispatcher = &dispatcher;
            let message = send("resources/list", serde_json::json!({}));
            async move {
                dispatcher
                    .handle_message_as(&message, &caller)
                    .await
                    .unwrap()
                    .unwrap()
                    .result
                    .unwrap()["resources"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["uri"].as_str().unwrap().to_string())
                    .collect::<Vec<_>>()
            }
        };
        let read = |caller: Caller, uri: &str| {
            let dispatcher = &dispatcher;
            let message = send("resources/read", serde_json::json!({ "uri": uri }));
            async move {
                dispatcher
                    .handle_message_as(&message, &caller)
                    .await
                    .unwrap()
                    .unwrap()
            }
        };
        let restricted = |tools: &[&str]| {
            let tools: Vec<String> = tools.iter().map(|t| t.to_string()).collect();
            Caller::new(
                "api_key:limited",
                crate::mcp::ToolAccess::from_patterns(Some(&tools)).unwrap(),
            )
        };

        // get_itemsだけを使えるキーは、x://itemsだけを見て読める
        let items_only = restricted(&["get_items"]);
        assert_eq!(list(items_only.clone()).await, vec!["x://items"]);
        let response = read(items_only.clone(), "x://items").await;
        assert_eq!(response.result.unwrap()["read"], "x://items");
        for uri in ["x://secrets", "x://unmapped"] {
            let response = read(items_only.clone(), uri).await;
            assert!(response.result.is_none());
            assert!(response.error.unwrap().message.contains("not allowed"));
        }

        // どのツールも使えないキーは、どのリソースも見えず読めない
        let nothing = restricted(&[]);
        assert!(list(nothing.clone()).await.is_empty());
        let response = read(nothing.clone(), "x://items").await;
        assert!(response.error.unwrap().message.contains("not allowed"));

        // 無いリソースは、これまでどおり「リソースが無い」
        let response = read(nothing, "x://missing").await;
        let message = response.error.unwrap().message;
        assert!(message.contains("x://missing"));
        assert!(!message.contains("not allowed"));

        // 制限の無い呼び出し元は、対応するツールの無いリソースも読める
        assert_eq!(list(Caller::unrestricted()).await.len(), 3);
        let response = read(Caller::unrestricted(), "x://unmapped").await;
        assert_eq!(response.result.unwrap()["read"], "x://unmapped");

        // 読み取りと拒否は、呼び出し元付きで監査ログに残る
        let entries = audit_logger.get_all_entries().await;
        let read_entry = entries
            .iter()
            .find(|e| e.message == "Resource read: x://items - SUCCESS")
            .expect("resource read is audited");
        assert_eq!(read_entry.user_id.as_deref(), Some("api_key:limited"));
        let denied = entries
            .iter()
            .find(|e| e.message.contains("Resource read denied: x://secrets"))
            .expect("denied resource read is audited");
        assert_eq!(denied.user_id.as_deref(), Some("api_key:limited"));
        assert_eq!(denied.result.as_deref(), Some("DENIED"));
    }

    /// `mcp_rs_status`（#434）: ハンドラーの状態・起動できなかったプラグインを
    /// 出し、呼び出しは監査ログに残る。権限（#52）で拒否できる
    #[tokio::test]
    async fn test_status_tool() {
        struct WithStatus;
        #[async_trait]
        impl McpHandler for WithStatus {
            async fn initialize(
                &self,
                _params: InitializeParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({}))
            }
            async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
                Ok(vec![tool("delete_post")])
            }
            async fn call_tool(
                &self,
                _params: ToolCallParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({}))
            }
            async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
                Ok(vec![])
            }
            async fn read_resource(
                &self,
                _params: ResourceReadParams,
            ) -> Result<serde_json::Value, McpError> {
                Ok(serde_json::json!({}))
            }
            async fn status(&self) -> Option<String> {
                Some("WordPress site https://example.com".to_string())
            }
        }

        let audit_logger = Arc::new(crate::security::AuditLogger::with_defaults());
        let mut server = McpServer::new();
        server.set_audit_logger(audit_logger.clone());
        server.add_handler("wordpress".to_string(), Arc::new(WithStatus));
        server.add_handler("broken".to_string(), Arc::new(BrokenHandler));
        server.set_status_info(crate::mcp::StatusInfo {
            mode: "stdio".to_string(),
            config_file: Some(std::path::PathBuf::from("C:/cfg/mcp-config.toml")),
            missing_config_file: None,
            started_at: std::time::Instant::now(),
            plugin_failures: vec![("fs".to_string(), "npx was not found".to_string())],
            // テストでGitHubに問い合わせない
            check_updates: false,
        });
        let dispatcher = server.dispatcher();
        let call = serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "mcp_rs_status", "arguments": {} }
        })
        .to_string();

        let result = dispatcher
            .handle_message(&call)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.starts_with(&format!("mcp-rs {}", env!("CARGO_PKG_VERSION"))),
            "{}",
            text
        );
        for expected in [
            "Mode: stdio",
            "Protocol version: 2024-11-05",
            "Config file: C:/cfg/mcp-config.toml",
            "- wordpress: WordPress site https://example.com",
            "- broken",
            "- fs: npx was not found",
            "Update check: off ([server] check_updates = false)",
        ] {
            assert!(
                text.contains(expected),
                "missing {:?} in:\n{}",
                expected,
                text
            );
        }
        let entries = audit_logger.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action.as_deref(), Some("mcp_rs_status"));
        assert_eq!(entries[0].user_id.as_deref(), Some("local"));

        // 権限で外せる
        let caller = Caller::new(
            "api_key:limited",
            crate::mcp::ToolAccess::from_patterns(Some(&["delete_post".to_string()])).unwrap(),
        );
        let denied = dispatcher
            .handle_message_as(&call, &caller)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert_eq!(denied["isError"], serde_json::json!(true));
    }

    #[test]
    fn test_add_handler_replaces_same_name_in_place() {
        let mut server = McpServer::new();
        server.add_handler("a".to_string(), Arc::new(BrokenHandler));
        server.add_handler("b".to_string(), Arc::new(BrokenHandler));
        server.add_handler(
            "a".to_string(),
            Arc::new(NamedHandler {
                label: "a2",
                tools: vec![],
                resources: vec![],
            }),
        );
        let names: Vec<&str> = server.handlers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn test_dispatcher_serves_registered_handlers_over_the_transport_trait() {
        use crate::transport::http::JsonRpcRequestHandler;

        let mut server = McpServer::new();
        for (name, handler) in two_handlers() {
            server.add_handler(name, handler);
        }
        let dispatcher = server.dispatcher();

        let response = dispatcher
            .handle_jsonrpc(
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "tools/call",
                    "params": { "name": "plugin__echo", "arguments": {} }
                })
                .to_string(),
                &Caller::unrestricted(),
            )
            .await
            .unwrap()
            .expect("a request with an id must get a response");
        assert_eq!(response["id"], serde_json::json!(5));
        assert_eq!(
            response["result"]["content"][0]["text"],
            serde_json::json!("second:plugin__echo")
        );

        // 通知には応答しない
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        assert!(dispatcher
            .handle_jsonrpc(&notification.to_string(), &Caller::unrestricted())
            .await
            .unwrap()
            .is_none());

        // JSON-RPCとして読めないものには、JSON-RPCのエラーで答える（#543）
        let response = dispatcher
            .handle_jsonrpc("not json", &Caller::unrestricted())
            .await
            .unwrap()
            .expect("an unreadable message must get an error response");
        assert_eq!(response["error"]["code"], serde_json::json!(-32700));
        assert_eq!(response["id"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn test_messages_that_are_not_requests_get_an_error_response() {
        // #543: 以前は何も返さず、クライアントは応答を待ち続けた
        let server = McpServer::new();
        let dispatcher = server.dispatcher();
        let caller = Caller::unrestricted();
        let ask = |message: &'static str| {
            let dispatcher = &dispatcher;
            let caller = &caller;
            async move {
                dispatcher
                    .handle_message_as(message, caller)
                    .await
                    .unwrap()
                    .map(|r| serde_json::to_value(r).unwrap())
            }
        };

        let parse = ask("{\"jsonrpc\": \"2.0\", \"id\": 1,").await.unwrap();
        assert_eq!(parse["error"]["code"], serde_json::json!(-32700));
        assert_eq!(parse["id"], serde_json::Value::Null);
        assert!(parse.get("result").is_none());

        // バッチは使えない
        let batch = ask(r#"[{"jsonrpc":"2.0","id":1,"method":"tools/list"}]"#)
            .await
            .unwrap();
        assert_eq!(batch["error"]["code"], serde_json::json!(-32600));
        assert_eq!(batch["id"], serde_json::Value::Null);

        // `method`の無いリクエスト: `id`は読めるので付ける
        let invalid = ask(r#"{"jsonrpc":"2.0","id":7}"#).await.unwrap();
        assert_eq!(invalid["error"]["code"], serde_json::json!(-32600));
        assert_eq!(invalid["id"], serde_json::json!(7));

        // 空の行は無視する
        assert!(ask("  \n").await.is_none());

        // クライアントからの応答には答えない（PR #555のレビュー）
        assert!(ask(r#"{"jsonrpc":"2.0","id":1,"result":{}}"#)
            .await
            .is_none());
        assert!(
            ask(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"x"}}"#)
                .await
                .is_none()
        );
    }

    /// 2026-07-28のリクエストの`_meta`（#570）
    fn modern_meta(version: &str) -> serde_json::Value {
        serde_json::json!({
            "io.modelcontextprotocol/protocolVersion": version,
            "io.modelcontextprotocol/clientInfo": { "name": "test", "version": "1.0.0" },
            "io.modelcontextprotocol/clientCapabilities": {}
        })
    }

    async fn request_with_session(
        handlers: &Handlers,
        session: &Session,
        method: &str,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let line = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        })
        .to_string();
        McpServer::process_request(&line, handlers, session)
            .await
            .unwrap()
            .expect("a request with an id must get a response")
    }

    fn assert_server_info(result: &serde_json::Value) {
        let info = &result["_meta"]["io.modelcontextprotocol/serverInfo"];
        assert_eq!(info["name"], serde_json::json!("mcp-rs"));
        assert_eq!(
            info["version"],
            serde_json::json!(env!("CARGO_PKG_VERSION"))
        );
    }

    #[tokio::test]
    async fn test_server_discover_returns_versions_and_capabilities() {
        let response = request(
            &two_handlers(),
            "server/discover",
            serde_json::json!({ "_meta": modern_meta("2026-07-28") }),
        )
        .await;
        assert!(response.error.is_none(), "{:?}", response.error);
        let result = response.result.unwrap();
        assert_eq!(result["resultType"], serde_json::json!("complete"));
        assert_eq!(
            result["supportedVersions"],
            serde_json::json!(["2026-07-28"])
        );
        assert_eq!(result["capabilities"], server_capabilities());
        assert_eq!(result["cacheScope"], serde_json::json!("public"));
        assert!(result["ttlMs"].as_u64().is_some());
        assert_server_info(&result);
        // initializeの応答の項目は無い
        assert!(result.get("protocolVersion").is_none());
        assert!(result.get("serverInfo").is_none());
    }

    #[tokio::test]
    async fn test_server_discover_without_version_is_invalid_params() {
        for params in [
            serde_json::json!({}),
            serde_json::json!({ "_meta": {} }),
            serde_json::Value::Null,
        ] {
            let response = request(&two_handlers(), "server/discover", params.clone()).await;
            let error = response.error.expect("must be an error");
            assert_eq!(error.code, -32602, "params {}", params);
            assert!(
                error.message.contains("protocolVersion"),
                "{}",
                error.message
            );
        }
    }

    #[tokio::test]
    async fn test_modern_request_with_bad_meta_is_invalid_params() {
        let cases = [
            // クライアントの能力が無い
            serde_json::json!({ "io.modelcontextprotocol/protocolVersion": "2026-07-28" }),
            // クライアントの能力がオブジェクトでない
            serde_json::json!({
                "io.modelcontextprotocol/protocolVersion": "2026-07-28",
                "io.modelcontextprotocol/clientCapabilities": "all"
            }),
            // 版が文字列でない
            serde_json::json!({
                "io.modelcontextprotocol/protocolVersion": 20260728,
                "io.modelcontextprotocol/clientCapabilities": {}
            }),
        ];
        for meta in cases {
            for method in ["server/discover", "tools/list"] {
                let response = request(
                    &two_handlers(),
                    method,
                    serde_json::json!({ "_meta": meta }),
                )
                .await;
                let error = response.error.expect("must be an error");
                assert_eq!(error.code, -32602, "{} with {}", method, meta);
                assert!(response.result.is_none());
            }
        }
    }

    #[tokio::test]
    async fn test_modern_request_with_unsupported_version() {
        // 知らない版も、initializeで交渉する版も、_metaでは受けない
        for requested in ["1900-01-01", "2025-11-25"] {
            let response = request(
                &two_handlers(),
                "tools/list",
                serde_json::json!({ "_meta": modern_meta(requested) }),
            )
            .await;
            let error = response.error.expect("must be an error");
            assert_eq!(error.code, -32022);
            assert_eq!(error.message, "Unsupported protocol version");
            let data = error.data.unwrap();
            assert_eq!(data["requested"], serde_json::json!(requested));
            // initializeに戻れるよう、legacyの版も知らせる
            assert_eq!(
                data["supported"],
                serde_json::json!(["2026-07-28", "2025-11-25", "2025-06-18", "2024-11-05"])
            );
        }
    }

    #[tokio::test]
    async fn test_modern_requests_without_initialize() {
        let handlers = two_handlers();
        let session = Session::default();
        let meta = modern_meta("2026-07-28");

        let tools = request_with_session(
            &handlers,
            &session,
            "tools/list",
            serde_json::json!({ "_meta": meta }),
        )
        .await
        .result
        .unwrap();
        assert_eq!(tools["resultType"], serde_json::json!("complete"));
        assert_eq!(tools["ttlMs"], serde_json::json!(LIST_TTL_MS));
        assert_eq!(tools["cacheScope"], serde_json::json!("private"));
        assert_server_info(&tools);
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["mcp_rs_status", "get_post", "shared", "plugin__echo"]
        );
        // 新しい版なので、2025-06-18以降の項目（title等）も付く
        assert!(tools["tools"][0].get("title").is_some());

        let call = request_with_session(
            &handlers,
            &session,
            "tools/call",
            serde_json::json!({ "name": "plugin__echo", "arguments": {}, "_meta": meta }),
        )
        .await
        .result
        .unwrap();
        assert_eq!(call["resultType"], serde_json::json!("complete"));
        assert_eq!(
            call["content"][0]["text"],
            serde_json::json!("second:plugin__echo")
        );
        assert!(call.get("ttlMs").is_none());
        assert_server_info(&call);

        let resources = request_with_session(
            &handlers,
            &session,
            "resources/list",
            serde_json::json!({ "_meta": meta }),
        )
        .await
        .result
        .unwrap();
        assert_eq!(resources["resultType"], serde_json::json!("complete"));
        assert_eq!(resources["cacheScope"], serde_json::json!("private"));
        assert_eq!(resources["resources"].as_array().unwrap().len(), 2);

        let read = request_with_session(
            &handlers,
            &session,
            "resources/read",
            serde_json::json!({ "uri": "second://items", "_meta": meta }),
        )
        .await
        .result
        .unwrap();
        assert_eq!(read["from"], serde_json::json!("second"));
        assert_eq!(read["resultType"], serde_json::json!("complete"));
        assert_eq!(read["ttlMs"], serde_json::json!(0));
        assert_eq!(read["cacheScope"], serde_json::json!("private"));

        // 2026-07-28のリクエストは、接続の版を変えない
        assert_eq!(session.protocol_version(), DEFAULT_PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn test_modern_errors_and_removed_methods() {
        let meta = modern_meta("2026-07-28");
        // pingは2026-07-28に無い
        let response = request(
            &two_handlers(),
            "ping",
            serde_json::json!({ "_meta": meta }),
        )
        .await;
        assert_eq!(response.error.unwrap().code, -32601);
        // エラーには、結果の項目を付けない
        let response = request(
            &two_handlers(),
            "tools/call",
            serde_json::json!({ "_meta": meta }),
        )
        .await;
        assert_eq!(response.error.unwrap().code, -32602);
        assert!(response.result.is_none());
    }

    #[tokio::test]
    async fn test_legacy_and_modern_requests_on_the_same_connection() {
        let handlers = two_handlers();
        let session = Session::default();
        let init = request_with_session(
            &handlers,
            &session,
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "legacy", "version": "1.0.0" }
            }),
        )
        .await
        .result
        .unwrap();
        assert_eq!(init["protocolVersion"], serde_json::json!("2024-11-05"));
        assert_eq!(session.protocol_version(), "2024-11-05");

        // legacy: 今までどおり（2026-07-28の項目は付けない）
        let legacy = request_with_session(&handlers, &session, "tools/list", serde_json::json!({}))
            .await
            .result
            .unwrap();
        assert!(legacy.get("resultType").is_none());
        assert!(legacy.get("_meta").is_none());
        assert!(legacy["tools"][0].get("title").is_none());

        // 同じ接続でも、_metaに版があれば、その版で答える
        let modern = request_with_session(
            &handlers,
            &session,
            "tools/list",
            serde_json::json!({ "_meta": modern_meta("2026-07-28") }),
        )
        .await
        .result
        .unwrap();
        assert_eq!(modern["resultType"], serde_json::json!("complete"));
        assert!(modern["tools"][0].get("title").is_some());
        assert_eq!(session.protocol_version(), "2024-11-05");

        // legacyのリクエストの_meta（progressToken等）は、今までどおり
        let with_token = request_with_session(
            &handlers,
            &session,
            "tools/list",
            serde_json::json!({ "_meta": { "progressToken": 1 } }),
        )
        .await
        .result
        .unwrap();
        assert!(with_token.get("resultType").is_none());
    }

    #[tokio::test]
    async fn test_status_tool_shows_modern_protocol_version() {
        let response = request(
            &two_handlers(),
            "tools/call",
            serde_json::json!({
                "name": "mcp_rs_status",
                "_meta": modern_meta("2026-07-28")
            }),
        )
        .await;
        let text = response.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            text.contains("Protocol version: 2026-07-28 (stateless, per-request _meta)"),
            "{}",
            text
        );
        assert!(
            text.contains("Supported protocol versions: 2026-07-28, 2025-11-25"),
            "{}",
            text
        );
    }

    /// 呼び出し元の制限は、2026-07-28のリクエストでも効く（#570）
    #[tokio::test]
    async fn test_restricted_caller_in_modern_requests() {
        let mut server = McpServer::new();
        for (name, handler) in two_handlers() {
            server.add_handler(name, handler);
        }
        let dispatcher = server.dispatcher();
        let caller = Caller::new(
            "api_key:named",
            crate::mcp::ToolAccess::from_patterns(Some(&["get_post".to_string()])).unwrap(),
        );
        let send = |method: &str, mut params: serde_json::Value| {
            params["_meta"] = modern_meta("2026-07-28");
            serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })
                .to_string()
        };
        let tools = dispatcher
            .handle_message_as(&send("tools/list", serde_json::json!({})), &caller)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        let names: Vec<&str> = tools["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["get_post"]);
        assert_eq!(tools["resultType"], serde_json::json!("complete"));

        let denied = dispatcher
            .handle_message_as(
                &send("tools/call", serde_json::json!({ "name": "plugin__echo" })),
                &caller,
            )
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert_eq!(denied["isError"], serde_json::json!(true));
        assert!(denied["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("not allowed"));
    }

    /// legacyの版では、pingに空の結果で答える。2026-07-28では-32601（#570）
    #[tokio::test]
    async fn test_ping_is_answered_only_in_legacy_requests() {
        let response = request(&two_handlers(), "ping", serde_json::json!({})).await;
        assert!(response.error.is_none(), "{:?}", response.error);
        assert_eq!(response.result, Some(serde_json::json!({})));
        let response = request(&two_handlers(), "ping", serde_json::Value::Null).await;
        assert_eq!(response.result, Some(serde_json::json!({})));

        let response = request(
            &two_handlers(),
            "ping",
            serde_json::json!({ "_meta": modern_meta("2026-07-28") }),
        )
        .await;
        assert_eq!(response.error.unwrap().code, -32601);
    }

    /// リソースが無いときは、legacyなら-32002、2026-07-28なら-32602（#570）
    #[tokio::test]
    async fn test_resource_not_found_code_depends_on_the_era() {
        let response = request(
            &two_handlers(),
            "resources/read",
            serde_json::json!({ "uri": "nothing://here" }),
        )
        .await;
        let error = response.error.expect("must be an error");
        assert_eq!(error.code, -32002);
        assert_eq!(
            error.data,
            Some(serde_json::json!({ "uri": "nothing://here" }))
        );
        assert!(error.message.contains("nothing://here"));

        let response = request(
            &two_handlers(),
            "resources/read",
            serde_json::json!({ "uri": "nothing://here", "_meta": modern_meta("2026-07-28") }),
        )
        .await;
        let error = response.error.expect("must be an error");
        assert_eq!(error.code, -32602);
        assert_eq!(
            error.data,
            Some(serde_json::json!({ "uri": "nothing://here" }))
        );
        assert!(response.result.is_none());
    }

    /// アイコンは、initialize（2025-11-25以降）とserver/discoverだけに付け、
    /// ほかの結果の_metaには付けない（#570）
    #[tokio::test]
    async fn test_server_icons_only_in_initialize_and_discover() {
        let initialize = |version: &str| {
            serde_json::json!({
                "protocolVersion": version,
                "capabilities": {},
                "clientInfo": { "name": "c", "version": "1" }
            })
        };
        let result = request(&two_handlers(), "initialize", initialize("2025-11-25"))
            .await
            .result
            .unwrap();
        assert_eq!(result["serverInfo"]["icons"].as_array().unwrap().len(), 3);
        let result = request(&two_handlers(), "initialize", initialize("2024-11-05"))
            .await
            .result
            .unwrap();
        assert!(result["serverInfo"].get("icons").is_none());

        let meta = serde_json::json!({ "_meta": modern_meta("2026-07-28") });
        let discover = request(&two_handlers(), "server/discover", meta.clone())
            .await
            .result
            .unwrap();
        let info = &discover["_meta"]["io.modelcontextprotocol/serverInfo"];
        assert_eq!(info["icons"].as_array().unwrap().len(), 3);
        assert_eq!(info["name"], serde_json::json!("mcp-rs"));

        let tools = request(&two_handlers(), "tools/list", meta)
            .await
            .result
            .unwrap();
        let info = &tools["_meta"]["io.modelcontextprotocol/serverInfo"];
        assert_eq!(info["name"], serde_json::json!("mcp-rs"));
        assert!(info.get("icons").is_none());
    }

    // ---- #643: プロンプト ----

    /// ツール`get_post`・`delete_post`と、それぞれを使うプロンプトを持つ
    struct WithPrompts;

    #[async_trait]
    impl McpHandler for WithPrompts {
        async fn initialize(&self, _: InitializeParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![tool("get_post"), tool("delete_post")])
        }
        async fn call_tool(&self, _: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(Vec::new())
        }
        async fn read_resource(
            &self,
            _: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
        fn prompts(&self) -> Vec<PromptSpec> {
            let prompt = |name: &str, tool: &str| PromptSpec {
                prompt: crate::mcp::Prompt {
                    name: name.to_string(),
                    title: None,
                    description: Some(format!("uses {tool}")),
                    arguments: vec![crate::mcp::PromptArgument {
                        name: "post_id".to_string(),
                        description: None,
                        required: true,
                    }],
                },
                tools: vec![tool.to_string()],
            };
            vec![
                prompt("read_it", "get_post"),
                prompt("remove_it", "delete_post"),
            ]
        }
        async fn get_prompt(
            &self,
            name: &str,
            arguments: &std::collections::HashMap<String, String>,
        ) -> Result<serde_json::Value, McpError> {
            Ok(crate::mcp::prompt_result(
                name,
                format!("{name} {}", arguments["post_id"]),
            ))
        }
    }

    async fn prompt_request(
        method: &str,
        params: serde_json::Value,
        caller: &Caller,
        audit_logger: Option<&crate::security::AuditLogger>,
    ) -> JsonRpcResponse {
        let handlers: Handlers = vec![("p".to_string(), Arc::new(WithPrompts))];
        let line =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
                .to_string();
        McpServer::process_request_as(
            &line,
            &handlers,
            &Session::default(),
            caller,
            audit_logger,
            &crate::mcp::StatusInfo::default(),
            None,
        )
        .await
        .unwrap()
        .unwrap()
    }

    #[tokio::test]
    async fn test_prompts_list_and_get() {
        let all = Caller::unrestricted();
        let list = prompt_request("prompts/list", serde_json::json!({}), &all, None)
            .await
            .result
            .unwrap();
        let names: Vec<&str> = list["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["read_it", "remove_it"]);
        assert_eq!(
            list["prompts"][0]["arguments"][0],
            serde_json::json!({"name": "post_id", "required": true})
        );

        let got = prompt_request(
            "prompts/get",
            serde_json::json!({"name": "read_it", "arguments": {"post_id": "7"}}),
            &all,
            None,
        )
        .await
        .result
        .unwrap();
        assert_eq!(got["messages"][0]["role"], "user");
        assert_eq!(got["messages"][0]["content"]["text"], "read_it 7");
    }

    /// 無いプロンプト・必須の引数が無いときは-32602
    #[tokio::test]
    async fn test_prompts_get_errors() {
        let all = Caller::unrestricted();
        for (params, message) in [
            (serde_json::json!({"name": "nope"}), "Unknown prompt: nope"),
            (
                serde_json::json!({"name": "read_it"}),
                "Prompt 'read_it' needs the argument 'post_id'",
            ),
            (
                serde_json::json!({"name": "read_it", "arguments": {"post_id": " "}}),
                "needs the argument 'post_id'",
            ),
        ] {
            let error = prompt_request("prompts/get", params.clone(), &all, None)
                .await
                .error
                .unwrap();
            assert_eq!(error.code, -32602, "{params}");
            assert!(
                error.message.contains(message),
                "{params}: {}",
                error.message
            );
        }
    }

    /// 使えないツールを使うプロンプトは、一覧に出さず、取り出しも断って記録する
    #[tokio::test]
    async fn test_prompts_follow_tool_limits() {
        let reader = Caller::new(
            "api_key:reader",
            crate::mcp::ToolAccess::from_patterns(Some(&["get_post".to_string()])).unwrap(),
        );
        let list = prompt_request("prompts/list", serde_json::json!({}), &reader, None)
            .await
            .result
            .unwrap();
        assert_eq!(list["prompts"].as_array().unwrap().len(), 1);
        assert_eq!(list["prompts"][0]["name"], "read_it");

        let audit = crate::security::AuditLogger::with_defaults();
        let error = prompt_request(
            "prompts/get",
            serde_json::json!({"name": "remove_it", "arguments": {"post_id": "7"}}),
            &reader,
            Some(&audit),
        )
        .await
        .error
        .unwrap();
        assert!(
            error.message.contains("is not allowed"),
            "{}",
            error.message
        );
        let entries = audit.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, Some("remove_it".to_string()));
        assert_eq!(entries[0].resource, Some("prompt".to_string()));
    }

    /// 能力にプロンプトを出し、2026-07-28では一覧に使い回しの時間を付ける
    #[test]
    fn test_prompts_capability_and_modern_fields() {
        assert_eq!(
            server_capabilities()["prompts"],
            serde_json::json!({"listChanged": false})
        );
        let list = modern_result(
            "prompts/list",
            "2026-07-28",
            serde_json::json!({"prompts": []}),
        );
        assert_eq!(list["ttlMs"], serde_json::json!(LIST_TTL_MS));
        assert_eq!(list["cacheScope"], "private");
        let got = modern_result("prompts/get", "2026-07-28", serde_json::json!({}));
        assert_eq!(got["ttlMs"], serde_json::json!(0));
        assert_eq!(got["resultType"], "complete");
    }

    // ---- #644: リソースのテンプレート ----

    /// `t://item/{id}`のテンプレートと、読み取りのツール`get_item`を持つ
    struct WithTemplates;

    #[async_trait]
    impl McpHandler for WithTemplates {
        async fn initialize(&self, _: InitializeParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
        async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
            Ok(vec![Tool {
                annotations: Some(crate::mcp::ToolAnnotations {
                    read_only_hint: Some(true),
                    ..Default::default()
                }),
                ..tool("get_item")
            }])
        }
        async fn call_tool(&self, _: ToolCallParams) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({}))
        }
        async fn list_resources(&self) -> Result<Vec<Resource>, McpError> {
            Ok(Vec::new())
        }
        async fn read_resource(
            &self,
            params: ResourceReadParams,
        ) -> Result<serde_json::Value, McpError> {
            Ok(serde_json::json!({"contents": [{"uri": params.uri, "text": "item"}]}))
        }
        fn resource_tool(&self, uri: &str) -> Option<String> {
            let id = uri.strip_prefix("t://item/")?;
            (id == "{id}" || id.chars().all(|c| c.is_ascii_digit())).then(|| "get_item".to_string())
        }
        async fn list_resource_templates(
            &self,
        ) -> Result<Vec<crate::mcp::ResourceTemplate>, McpError> {
            Ok(vec![crate::mcp::ResourceTemplate {
                uri_template: "t://item/{id}".to_string(),
                name: "Item".to_string(),
                description: None,
                mime_type: Some("application/json".to_string()),
            }])
        }
    }

    async fn template_request(
        method: &str,
        params: serde_json::Value,
        caller: &Caller,
        audit_logger: Option<&crate::security::AuditLogger>,
    ) -> JsonRpcResponse {
        let handlers: Handlers = vec![("t".to_string(), Arc::new(WithTemplates))];
        let line =
            serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
                .to_string();
        McpServer::process_request_as(
            &line,
            &handlers,
            &Session::default(),
            caller,
            audit_logger,
            &crate::mcp::StatusInfo::default(),
            None,
        )
        .await
        .unwrap()
        .unwrap()
    }

    #[tokio::test]
    async fn test_resource_templates_list_and_read() {
        let all = Caller::unrestricted();
        let list = template_request(
            "resources/templates/list",
            serde_json::json!({}),
            &all,
            None,
        )
        .await
        .result
        .unwrap();
        assert_eq!(
            list["resourceTemplates"],
            serde_json::json!([{"uriTemplate": "t://item/{id}", "name": "Item", "mimeType": "application/json"}])
        );
        let read = template_request(
            "resources/read",
            serde_json::json!({"uri": "t://item/5"}),
            &all,
            None,
        )
        .await
        .result
        .unwrap();
        assert_eq!(read["contents"][0]["uri"], "t://item/5");
        // テンプレートに合わないURIは、無いリソース
        let missing = template_request(
            "resources/read",
            serde_json::json!({"uri": "t://item/x"}),
            &all,
            None,
        )
        .await
        .error
        .unwrap();
        assert_eq!(missing.code, -32002);
    }

    /// 読み取りのツールを使えない呼び出し元には、テンプレートを出さず、読ませない
    #[tokio::test]
    async fn test_resource_templates_follow_tool_limits() {
        let reader = Caller::new(
            "api_key:reader",
            crate::mcp::ToolAccess::from_patterns(Some(&["@read_only".to_string()])).unwrap(),
        );
        let list = template_request(
            "resources/templates/list",
            serde_json::json!({}),
            &reader,
            None,
        )
        .await
        .result
        .unwrap();
        assert_eq!(list["resourceTemplates"].as_array().unwrap().len(), 1);

        let other = Caller::new(
            "api_key:other",
            crate::mcp::ToolAccess::from_patterns(Some(&["other_tool".to_string()])).unwrap(),
        );
        let list = template_request(
            "resources/templates/list",
            serde_json::json!({}),
            &other,
            None,
        )
        .await
        .result
        .unwrap();
        assert!(list["resourceTemplates"].as_array().unwrap().is_empty());
        let audit = crate::security::AuditLogger::with_defaults();
        let denied = template_request(
            "resources/read",
            serde_json::json!({"uri": "t://item/5"}),
            &other,
            Some(&audit),
        )
        .await
        .error
        .unwrap();
        assert!(
            denied.message.contains("is not allowed"),
            "{}",
            denied.message
        );
        let entries = audit.get_all_entries().await;
        assert_eq!(entries[0].action, Some("t://item/5".to_string()));
    }

    #[test]
    fn test_resource_templates_list_modern_fields() {
        let list = modern_result(
            "resources/templates/list",
            "2026-07-28",
            serde_json::json!({"resourceTemplates": []}),
        );
        assert_eq!(list["ttlMs"], serde_json::json!(LIST_TTL_MS));
        assert_eq!(list["cacheScope"], "private");
    }
}
