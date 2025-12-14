//! JSON-RPC over WebSocket Integration Tests

use mcp_rs::transport::websocket::jsonrpc::{error_codes, JsonRpcMessage, JsonRpcNotification};
use mcp_rs::transport::websocket::types::WebSocketMessage;
use mcp_rs::types::{JsonRpcRequest, JsonRpcResponse, RequestId};
use serde_json::json;

#[test]
fn test_request_roundtrip() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "initialize".to_string(),
        params: Some(json!({"version": "1.0"})),
        id: RequestId::Number(1),
    };

    let message = JsonRpcMessage::Request(request.clone());
    let ws_msg = message.to_websocket().unwrap();
    let parsed = JsonRpcMessage::from_websocket(ws_msg).unwrap();

    match parsed {
        JsonRpcMessage::Request(req) => {
            assert_eq!(req.jsonrpc, request.jsonrpc);
            assert_eq!(req.method, request.method);
            assert_eq!(req.id, request.id);
        }
        _ => panic!("Expected Request"),
    }
}

#[test]
fn test_response_roundtrip() {
    let response = JsonRpcResponse::success(RequestId::Number(1), json!({"status": "ok"}));

    let message = JsonRpcMessage::Response(response.clone());
    let ws_msg = message.to_websocket().unwrap();
    let parsed = JsonRpcMessage::from_websocket(ws_msg).unwrap();

    match parsed {
        JsonRpcMessage::Response(res) => {
            assert_eq!(res.jsonrpc, response.jsonrpc);
            assert_eq!(res.id, response.id);
            assert!(res.result.is_some());
        }
        _ => panic!("Expected Response"),
    }
}

#[test]
fn test_notification_roundtrip() {
    let notification = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "progress".to_string(),
        params: Some(json!({"percent": 50})),
    };

    let message = JsonRpcMessage::Notification(notification.clone());
    let ws_msg = message.to_websocket().unwrap();
    let parsed = JsonRpcMessage::from_websocket(ws_msg).unwrap();

    match parsed {
        JsonRpcMessage::Notification(notif) => {
            assert_eq!(notif.jsonrpc, notification.jsonrpc);
            assert_eq!(notif.method, notification.method);
        }
        _ => panic!("Expected Notification"),
    }
}

#[test]
fn test_error_response_creation() {
    let response = JsonRpcResponse::method_not_found(RequestId::String("abc".to_string()), "test");

    assert!(response.error.is_some());
    let error = response.error.unwrap();
    assert_eq!(error.code, error_codes::METHOD_NOT_FOUND);
    assert!(error.message.contains("test"));
}

#[test]
fn test_binary_message_conversion() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "test".to_string(),
        params: None,
        id: RequestId::Number(1),
    };

    let message = JsonRpcMessage::Request(request);
    let ws_msg = message.to_websocket_binary().unwrap();

    match ws_msg {
        WebSocketMessage::Binary(data) => {
            assert!(!data.is_empty());
            let text = String::from_utf8(data).unwrap();
            assert!(text.contains("\"method\":\"test\""));
        }
        _ => panic!("Expected Binary message"),
    }
}

#[test]
fn test_all_error_codes() {
    let id = RequestId::Number(1);

    let parse_err = JsonRpcResponse::parse_error(id.clone(), "Invalid JSON");
    assert_eq!(
        parse_err.error.as_ref().unwrap().code,
        error_codes::PARSE_ERROR
    );

    let invalid_req = JsonRpcResponse::invalid_request(id.clone(), "Missing field");
    assert_eq!(
        invalid_req.error.as_ref().unwrap().code,
        error_codes::INVALID_REQUEST
    );

    let method_not_found = JsonRpcResponse::method_not_found(id.clone(), "unknown");
    assert_eq!(
        method_not_found.error.as_ref().unwrap().code,
        error_codes::METHOD_NOT_FOUND
    );

    let invalid_params = JsonRpcResponse::invalid_params(id.clone(), "Wrong type");
    assert_eq!(
        invalid_params.error.as_ref().unwrap().code,
        error_codes::INVALID_PARAMS
    );

    let internal_err = JsonRpcResponse::internal_error(id, "Server error");
    assert_eq!(
        internal_err.error.as_ref().unwrap().code,
        error_codes::INTERNAL_ERROR
    );
}

#[test]
fn test_invalid_json_handling() {
    let ws_msg = WebSocketMessage::Text("not valid json".to_string());
    let result = JsonRpcMessage::from_websocket(ws_msg);
    assert!(result.is_err());
}

#[test]
fn test_malformed_jsonrpc_handling() {
    // JSON-RPCバージョンなし
    let ws_msg = WebSocketMessage::Text(r#"{"method":"test","id":1}"#.to_string());
    let result = JsonRpcMessage::from_websocket(ws_msg);
    assert!(result.is_err());

    // 不正なJSON-RPCバージョン
    let ws_msg = WebSocketMessage::Text(r#"{"jsonrpc":"3.0","method":"test","id":1}"#.to_string());
    let result = JsonRpcMessage::from_websocket(ws_msg);
    assert!(result.is_err());
}

#[test]
fn test_request_id_types() {
    // Number ID
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "test".to_string(),
        params: None,
        id: RequestId::Number(42),
    };
    let msg = JsonRpcMessage::Request(req);
    let ws_msg = msg.to_websocket().unwrap();
    assert!(matches!(ws_msg, WebSocketMessage::Text(_)));

    // String ID
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "test".to_string(),
        params: None,
        id: RequestId::String("test-id".to_string()),
    };
    let msg = JsonRpcMessage::Request(req);
    let ws_msg = msg.to_websocket().unwrap();
    assert!(matches!(ws_msg, WebSocketMessage::Text(_)));

    // Null ID
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        method: "test".to_string(),
        params: None,
        id: RequestId::Null,
    };
    let msg = JsonRpcMessage::Request(req);
    let ws_msg = msg.to_websocket().unwrap();
    assert!(matches!(ws_msg, WebSocketMessage::Text(_)));
}
