use mcp_rs::mcp::{JsonRpcRequest, JsonRpcResponse, ToolCallParams};
use std::collections::HashMap;

#[test]
fn test_json_rpc_request_parsing() {
    let json = r#"
    {
        "jsonrpc": "2.0",
        "method": "tools/list",
        "id": 1
    }"#;

    let request: JsonRpcRequest = serde_json::from_str(json).unwrap();
    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.method, "tools/list");
    assert_eq!(request.id, Some(serde_json::json!(1)));
}

#[test]
fn test_tool_call_params_serialization() {
    let mut args = HashMap::new();
    args.insert("title".to_string(), serde_json::json!("Test Title"));
    args.insert("content".to_string(), serde_json::json!("Test Content"));

    let params = ToolCallParams {
        name: "create_post".to_string(),
        arguments: Some(args),
    };

    let json = serde_json::to_string(&params).unwrap();
    assert!(json.contains("create_post"));
    assert!(json.contains("Test Title"));
}

#[test]
fn test_json_rpc_response_creation() {
    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: Some(serde_json::json!({"status": "success"})),
        error: None,
        id: Some(serde_json::json!(1)),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("2.0"));
    assert!(json.contains("success"));
}

#[test]
fn test_error_response_creation() {
    let error = mcp_rs::mcp::JsonRpcError {
        code: -32601,
        message: "Method not found".to_string(),
        data: None,
    };

    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: None,
        error: Some(error),
        id: Some(serde_json::json!(1)),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("-32601"));
    assert!(json.contains("Method not found"));
}
