use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::mcp::{
    call_tool_result_for_version, negotiate_protocol_version, tools_list_for_version, Caller,
    InitializeParams, JsonRpcRequest, JsonRpcResponse, McpError, Resource, ResourceReadParams,
    Tool, ToolCallParams, DEFAULT_PROTOCOL_VERSION,
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
}

/// 登録されたハンドラーの一覧（登録順）。ツール名の重複時に、どちらが
/// 使われるかを登録順で決めるため、HashMapではなく順序のある一覧で持つ。
type Handlers = Vec<(String, Arc<dyn McpHandler>)>;

pub struct McpServer {
    handlers: Handlers,
    /// 呼び出しを拒否したツールを記録する（#52）
    audit_logger: Option<Arc<crate::security::AuditLogger>>,
    /// `mcp_rs_status`に出す情報（#434）
    status: Arc<crate::mcp::StatusInfo>,
    capabilities: ServerCapabilities,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ServerCapabilities {
    pub tools: Option<ToolsCapability>,
    pub resources: Option<ResourcesCapability>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolsCapability {
    pub list_changed: Option<bool>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResourcesCapability {
    pub subscribe: Option<bool>,
    pub list_changed: Option<bool>,
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
        McpServer::process_request_as(
            message,
            &self.handlers,
            &self.session,
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
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: Some(false),
                    list_changed: Some(false),
                }),
            },
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

    /// ハンドラーを登録する。同じ名前が既にあれば置き換える（登録順の
    /// 位置は保つ）。
    pub fn add_handler(&mut self, name: String, handler: Arc<dyn McpHandler>) {
        if let Some(entry) = self.handlers.iter_mut().find(|(n, _)| *n == name) {
            entry.1 = handler;
        } else {
            self.handlers.push((name, handler));
        }
    }

    pub async fn run(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        info!("MCP Server listening on {}", addr);

        loop {
            let (stream, _) = listener.accept().await?;
            let handlers = self.handlers.clone();
            let capabilities = self.capabilities.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(stream, handlers, capabilities).await {
                    error!("Error handling connection: {}", e);
                }
            });
        }
    }

    async fn handle_connection(
        mut stream: TcpStream,
        handlers: Handlers,
        _capabilities: ServerCapabilities,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (reader, mut writer) = stream.split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        let session = Session::default();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                break; // Connection closed
            }

            let response = match Self::process_request(&line, &handlers, &session).await {
                Ok(Some(resp)) => resp,
                // 通知（idなし）には応答しない（JSON-RPC 2.0）
                Ok(None) => continue,
                Err(e) => {
                    warn!("Error processing request: {}", e);
                    continue;
                }
            };

            let response_str = serde_json::to_string(&response)?;
            writer.write_all(response_str.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }

        Ok(())
    }

    /// 1行分のJSON-RPCメッセージを処理する。通知（`id`なし）の場合は
    /// `Ok(None)`を返し、呼び出し元は何も書き出さない。JSON-RPC 2.0では
    /// サーバーは通知に応答してはならない（#360）。
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
        let request: JsonRpcRequest = serde_json::from_str(line.trim())?;
        let version = protocol_version.unwrap_or_else(|| session.protocol_version());

        if request.id.is_none() {
            Self::handle_notification(&request.method);
            return Ok(None);
        }

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
            "tools/list" => {
                let mut tools = Self::list_all_tools(handlers).await;
                tools.retain(|tool| caller.access.allows(tool));
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
                        Self::audit_denied_tool(audit_logger, caller, &params.name).await;
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
                let resources = Self::list_all_resources(handlers).await;
                Ok(serde_json::json!({ "resources": resources }))
            }
            "resources/read" => {
                match serde_json::from_value::<ResourceReadParams>(
                    request.params.clone().unwrap_or_default(),
                ) {
                    Ok(params) => Self::read_resource_routed(handlers, params).await,
                    Err(e) => Err(McpError::InvalidParams(e.to_string())),
                }
            }
            _ => Err(McpError::InvalidMethod(request.method.clone())),
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
                error: Some(e.into()),
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
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false }
            },
            "serverInfo": crate::mcp::server_info_for_version(protocol_version)
        })
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
        Self::list_all_tools(handlers)
            .await
            .iter()
            .find(|tool| tool.name == name)
            .is_none_or(|tool| caller.access.allows(tool))
    }

    /// 呼び出しを拒否したツールを、監査ログとログに記録する。
    async fn audit_denied_tool(
        audit_logger: Option<&crate::security::AuditLogger>,
        caller: &Caller,
        name: &str,
    ) {
        warn!(
            "Denied tool call '{}' by '{}': the tool is not allowed for this caller",
            name,
            caller.display_name()
        );
        let Some(audit_logger) = audit_logger else {
            return;
        };
        let mut entry = crate::security::AuditLogEntry::new(
            crate::security::AuditLevel::Warning,
            crate::security::AuditCategory::Authorization,
            format!("Tool call denied: {} by {}", name, caller.display_name()),
        )
        .with_action("tool".to_string(), name.to_string())
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
        let mut tools: Vec<Tool> = vec![crate::mcp::status_tool()];
        for (name, handler) in handlers {
            match handler.list_tools().await {
                Ok(handler_tools) => {
                    for tool in handler_tools {
                        if tools.iter().any(|t| t.name == tool.name) {
                            warn!(
                                "Tool '{}' from handler '{}' is hidden: another handler registered earlier provides a tool with the same name",
                                tool.name, name
                            );
                        } else {
                            tools.push(tool);
                        }
                    }
                }
                Err(e) => warn!("Failed to list tools from handler '{}': {}", name, e),
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

    /// 全ハンドラーのリソースを登録順にまとめる。
    async fn list_all_resources(handlers: &[(String, Arc<dyn McpHandler>)]) -> Vec<Resource> {
        let mut resources = Vec::new();
        for (name, handler) in handlers {
            match handler.list_resources().await {
                Ok(handler_resources) => resources.extend(handler_resources),
                Err(e) => warn!("Failed to list resources from handler '{}': {}", name, e),
            }
        }
        resources
    }

    /// URIから読み出し先のハンドラーを決めて読む。
    async fn read_resource_routed(
        handlers: &[(String, Arc<dyn McpHandler>)],
        params: ResourceReadParams,
    ) -> Result<serde_json::Value, McpError> {
        for (name, handler) in handlers {
            match handler.list_resources().await {
                Ok(resources) if resources.iter().any(|r| r.uri == params.uri) => {
                    return handler.read_resource(params).await;
                }
                Ok(_) => {}
                Err(e) => warn!("Failed to list resources from handler '{}': {}", name, e),
            }
        }
        Err(McpError::ResourceNotFound(params.uri))
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
        struct Annotated;
        #[async_trait]
        impl McpHandler for Annotated {
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
        vec![("a".to_string(), Arc::new(Annotated))]
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

        // 無いツールは、これまでどおり「ツールが無い」
        let missing = dispatcher
            .handle_message_as(&call("no_such_tool"), &caller)
            .await
            .unwrap()
            .unwrap()
            .result
            .unwrap();
        assert!(missing["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no_such_tool"));
        assert!(!missing["content"][0]["text"]
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
            started_at: std::time::Instant::now(),
            plugin_failures: vec![("fs".to_string(), "npx was not found".to_string())],
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

        // JSON-RPCとして読めないものはエラー
        assert!(dispatcher
            .handle_jsonrpc("not json", &Caller::unrestricted())
            .await
            .is_err());
    }
}
