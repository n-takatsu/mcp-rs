//! JSON-RPC message types used by the `Transport` trait.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    /// 要求の`id`。通知（`id`なし）は`None`（PR #771）。以前は必須で、
    /// 通知を読めなかった。`"id": null`も`None`に読む（`HttpTransport`は、
    /// `id`が文字列・数値でない要求を、渡す前に断る）。
    ///
    /// `HttpTransport`のチャンネルでは、要求の`id`は内部の相関IDで、
    /// クライアントの`id`は`ReceivedRequest::original_id`にある。
    /// `notifications/cancelled`の`requestId`は、同じ呼び出し元の要求の
    /// `original_id`と照らし合わせる（PR #771のレビュー）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
}

/// JSON-RPC response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: RequestId,
}

/// JSON-RPC error object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Request ID that can be string, number or null
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    String(String),
    Number(i64),
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: Some(serde_json::json!({"key": "value"})),
            id: Some(RequestId::Number(1)),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"test\""));
        assert!(json.contains("\"id\":1"));
    }

    /// 通知（`id`なし）も読め、書くときも`id`を付けない（PR #771）
    #[test]
    fn test_json_rpc_notification_has_no_id() {
        let notification: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .unwrap();
        assert!(notification.id.is_none());
        let json = serde_json::to_value(&notification).unwrap();
        assert!(json.get("id").is_none(), "{json}");

        let request: JsonRpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#).unwrap();
        assert_eq!(request.id, Some(RequestId::String("a".to_string())));
    }

    #[test]
    fn test_request_id_variants() {
        let id_str = RequestId::String("test".to_string());
        let id_num = RequestId::Number(42);
        let id_null = RequestId::Null;

        assert_eq!(serde_json::to_string(&id_str).unwrap(), "\"test\"");
        assert_eq!(serde_json::to_string(&id_num).unwrap(), "42");
        assert_eq!(serde_json::to_string(&id_null).unwrap(), "null");
    }
}
