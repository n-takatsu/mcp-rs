use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

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

pub struct McpServer {
    handlers: HashMap<String, Arc<dyn McpHandler>>,
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
            handlers: HashMap::new(),
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

    pub fn add_handler(&mut self, name: String, handler: Arc<dyn McpHandler>) {
        self.handlers.insert(name, handler);
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
        handlers: HashMap<String, Arc<dyn McpHandler>>,
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
                Ok(resp) => resp,
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

    async fn process_request(
        line: &str,
        handlers: &HashMap<String, Arc<dyn McpHandler>>,
    ) -> Result<JsonRpcResponse, McpError> {
        let request: JsonRpcRequest = serde_json::from_str(line.trim())?;

        // 注意: ここで `?` を使って早期returnしてはいけない。そうすると
        // request.id を失い、クライアントには何の応答も返らないまま接続が
        // 無言でハングする（JSON-RPC 2.0違反）。失敗はすべて`result`に
        // 畳み込み、関数末尾でrequest.id付きのエラーレスポンスとして返す。
        let result = match request.method.as_str() {
            "initialize" => {
                if let Some(handler) = handlers.values().next() {
                    match serde_json::from_value::<InitializeParams>(
                        request.params.clone().unwrap_or_default(),
                    ) {
                        Ok(params) => handler.initialize(params).await,
                        Err(e) => Err(McpError::InvalidParams(e.to_string())),
                    }
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "tools/list" => {
                if let Some(handler) = handlers.values().next() {
                    match handler.list_tools().await {
                        Ok(tools) => Ok(serde_json::json!({ "tools": tools })),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "tools/call" => {
                if let Some(handler) = handlers.values().next() {
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
                        Ok(params) => match handler.call_tool(params).await {
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
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "resources/list" => {
                if let Some(handler) = handlers.values().next() {
                    match handler.list_resources().await {
                        Ok(resources) => Ok(serde_json::json!({ "resources": resources })),
                        Err(e) => Err(e),
                    }
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            "resources/read" => {
                if let Some(handler) = handlers.values().next() {
                    match serde_json::from_value::<ResourceReadParams>(
                        request.params.clone().unwrap_or_default(),
                    ) {
                        Ok(params) => handler.read_resource(params).await,
                        Err(e) => Err(McpError::InvalidParams(e.to_string())),
                    }
                } else {
                    Err(McpError::InvalidMethod("No handlers available".to_string()))
                }
            }
            _ => Err(McpError::InvalidMethod(request.method.clone())),
        };

        match result {
            Ok(result) => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(result),
                error: None,
                id: request.id,
            }),
            Err(e) => Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: Some(e.into()),
                id: request.id,
            }),
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
                Ok(resp) => resp,
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

    fn handlers_with_mock() -> HashMap<String, Arc<dyn McpHandler>> {
        let mut handlers: HashMap<String, Arc<dyn McpHandler>> = HashMap::new();
        handlers.insert("mock".to_string(), Arc::new(MockHandler));
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
            Ok(vec![])
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

    fn handlers_with_failing_tool() -> HashMap<String, Arc<dyn McpHandler>> {
        let mut handlers: HashMap<String, Arc<dyn McpHandler>> = HashMap::new();
        handlers.insert("mock".to_string(), Arc::new(FailingToolMockHandler));
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
            .unwrap();

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
            .unwrap();

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
            .unwrap();

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
            .unwrap();

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
}
