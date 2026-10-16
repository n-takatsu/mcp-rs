use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Response
///
/// `result`と`error`はJSON-RPC 2.0仕様上「互いに排他的で、該当しない方は
/// キー自体が存在してはならない」(単なる`null`では不可)。
/// `skip_serializing_if`無しで`Option<T>`をそのままシリアライズすると
/// `None`が明示的な`null`として出力されてしまい、実クライアント
/// （Claude Desktop等の厳格なスキーマ検証）に拒否される。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// MCP Protocol Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
#[allow(dead_code)]
pub enum McpMessage {
    #[serde(rename = "initialize")]
    Initialize { params: InitializeParams },
    #[serde(rename = "tools/list")]
    ToolsList { params: Option<serde_json::Value> },
    #[serde(rename = "tools/call")]
    ToolsCall { params: ToolCallParams },
    #[serde(rename = "resources/list")]
    ResourcesList { params: Option<serde_json::Value> },
    #[serde(rename = "resources/read")]
    ResourcesRead { params: ResourceReadParams },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    pub experimental: Option<HashMap<String, serde_json::Value>>,
    pub sampling: Option<SamplingCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingCapability {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    pub name: String,
    pub arguments: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadParams {
    pub uri: String,
}

/// MCP Tool Definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// MCP Resource Definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実クライアント（Claude Desktop等）はresult/errorを厳格なunion
    /// スキーマで検証し、該当しない方のキーが`null`であっても存在する
    /// こと自体を拒否する。successレスポンスには"error"キーが一切
    /// 出力されてはならない。
    #[test]
    fn test_success_response_omits_error_key_entirely() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({"ok": true})),
            error: None,
            id: Some(serde_json::json!(1)),
        };

        let value = serde_json::to_value(&response).unwrap();
        assert!(value.get("result").is_some());
        assert!(
            value.get("error").is_none(),
            "error key must be absent, not null, on success"
        );
    }

    /// 逆に、errorレスポンスには"result"キーが一切出力されてはならない。
    #[test]
    fn test_error_response_omits_result_key_entirely() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32602,
                message: "Invalid params".to_string(),
                data: None,
            }),
            id: Some(serde_json::json!(1)),
        };

        let value = serde_json::to_value(&response).unwrap();
        assert!(value.get("error").is_some());
        assert!(
            value.get("result").is_none(),
            "result key must be absent, not null, on error"
        );
        // dataがNoneの場合も同様にキー自体が省略されるべき
        assert!(value["error"].get("data").is_none());
    }

    /// 実際のMCPクライアント（Claude Desktop等）はワイヤー上でcamelCaseを
    /// 使う（例: protocolVersion, clientInfo）。snake_caseのままでは
    /// デシリアライズに失敗する。
    #[test]
    fn test_initialize_params_accepts_camel_case_wire_format() {
        let json = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "experimental": null, "sampling": null },
            "clientInfo": { "name": "claude-desktop", "version": "1.0.0" }
        });

        let params: InitializeParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.protocol_version, "2024-11-05");
        assert_eq!(params.client_info.name, "claude-desktop");
    }

    #[test]
    fn test_tool_serializes_to_camel_case_wire_format() {
        let tool = Tool {
            name: "example".to_string(),
            description: "An example tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };

        let value = serde_json::to_value(&tool).unwrap();
        assert!(value.get("inputSchema").is_some());
        assert!(value.get("input_schema").is_none());
    }

    #[test]
    fn test_resource_serializes_to_camel_case_wire_format() {
        let resource = Resource {
            uri: "file:///example.txt".to_string(),
            name: "example".to_string(),
            description: None,
            mime_type: Some("text/plain".to_string()),
        };

        let value = serde_json::to_value(&resource).unwrap();
        assert!(value.get("mimeType").is_some());
        assert!(value.get("mime_type").is_none());
    }
}
