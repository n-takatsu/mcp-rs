use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info, warn};

use crate::mcp::{
    InitializeParams, JsonRpcRequest, JsonRpcResponse, McpError, Resource, ResourceReadParams,
    Tool, ToolCallParams,
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
}

/// 登録されたハンドラーの一覧（登録順）。ツール名の重複時に、どちらが
/// 使われるかを登録順で決めるため、HashMapではなく順序のある一覧で持つ。
type Handlers = Vec<(String, Arc<dyn McpHandler>)>;

pub struct McpServer {
    handlers: Handlers,
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

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
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

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                break; // Connection closed
            }

            let response = match Self::process_request(&line, &handlers).await {
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
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        let request: JsonRpcRequest = serde_json::from_str(line.trim())?;

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
                    Ok(params) => Ok(Self::initialize_all(handlers, params).await),
                    Err(e) => Err(McpError::InvalidParams(e.to_string())),
                }
            }
            "tools/list" => {
                let tools = Self::list_all_tools(handlers).await;
                Ok(serde_json::json!({ "tools": tools }))
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
                    Ok(params) => match Self::call_tool_routed(handlers, params).await {
                        Ok(value) => Ok(value),
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
    ) -> serde_json::Value {
        for (name, handler) in handlers {
            if let Err(e) = handler.initialize(params.clone()).await {
                warn!("Handler '{}' failed to initialize: {}", name, e);
            }
        }
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false }
            },
            "serverInfo": {
                "name": "mcp-rs",
                "version": env!("CARGO_PKG_VERSION")
            }
        })
    }

    /// 全ハンドラーのツールを登録順にまとめる。同じ名前のツールは、先に
    /// 登録したハンドラーのものだけを残す（呼び出しもそちらに振り分ける）。
    /// 一覧を取得できないハンドラーは警告を出して飛ばす。
    async fn list_all_tools(handlers: &[(String, Arc<dyn McpHandler>)]) -> Vec<Tool> {
        let mut tools: Vec<Tool> = Vec::new();
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

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                break; // EOF
            }

            let response = match Self::process_request(&line, &self.handlers).await {
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

        let response = McpServer::process_request(&line, &handlers_with_mock())
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

        let response = McpServer::process_request(&line, &handlers_with_mock())
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

        let response = McpServer::process_request(&line, &handlers_with_mock())
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

        let response = McpServer::process_request(&line, &handlers_with_failing_tool())
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
            let response = McpServer::process_request(&line, &handlers_with_mock())
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

        let response = McpServer::process_request(&line, &handlers).await.unwrap();
        assert!(response.is_none());
    }

    fn tool(name: &str) -> Tool {
        Tool {
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({"type": "object"}),
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
        McpServer::process_request(&line, handlers)
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
        assert_eq!(names, vec!["get_post", "shared", "plugin__echo"]);
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

        // ハンドラーが無くても、ツール一覧は空で応答する
        let response = request(&Vec::new(), "tools/list", serde_json::json!({})).await;
        assert_eq!(response.result.unwrap()["tools"], serde_json::json!([]));
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
}
