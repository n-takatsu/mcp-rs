//! JSON-RPC over WebSocket Implementation
//!
//! WebSocketメッセージとJSON-RPCメッセージの双方向変換

use crate::error::{Error, Result};
use crate::transport::websocket::types::WebSocketMessage;
use crate::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, RequestId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC通知メッセージ（IDなし）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPCメッセージ型（Request/Response/Notification）
#[derive(Debug, Clone)]
pub enum JsonRpcMessage {
    /// JSON-RPCリクエスト
    Request(JsonRpcRequest),
    /// JSON-RPCレスポンス
    Response(JsonRpcResponse),
    /// JSON-RPC通知（IDなし）
    Notification(JsonRpcNotification),
}

impl JsonRpcMessage {
    /// WebSocketMessageから変換
    pub fn from_websocket(message: WebSocketMessage) -> Result<Self> {
        match message {
            WebSocketMessage::Text(text) => Self::from_json_str(&text),
            WebSocketMessage::Binary(data) => {
                let text = String::from_utf8(data.to_vec())
                    .map_err(|e| Error::InvalidMessage(format!("Invalid UTF-8: {}", e)))?;
                Self::from_json_str(&text)
            }
            WebSocketMessage::Close(_) => {
                Err(Error::ConnectionError("Connection closed".to_string()))
            }
            WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => {
                Err(Error::InvalidMessage("Control frame received".to_string()))
            }
        }
    }

    /// JSON文字列から変換
    pub fn from_json_str(text: &str) -> Result<Self> {
        // まずJSON-RPCの共通フィールドを確認
        let value: Value = serde_json::from_str(text)?;

        // jsonrpcバージョンチェック
        if let Some(version) = value.get("jsonrpc") {
            if version != "2.0" {
                return Err(Error::InvalidMessage(format!(
                    "Unsupported JSON-RPC version: {}",
                    version
                )));
            }
        } else {
            return Err(Error::InvalidMessage(
                "Missing jsonrpc version field".to_string(),
            ));
        }

        // IDフィールドでRequest/Responseを判別
        if value.get("method").is_some() {
            // methodフィールドがある場合
            if value.get("id").is_some() {
                // IDがある場合はRequest
                let request: JsonRpcRequest = serde_json::from_value(value)?;
                Ok(JsonRpcMessage::Request(request))
            } else {
                // IDがない場合はNotification
                let notification: JsonRpcNotification = serde_json::from_value(value)?;
                Ok(JsonRpcMessage::Notification(notification))
            }
        } else if value.get("result").is_some() || value.get("error").is_some() {
            // resultまたはerrorフィールドがある場合はResponse
            let response: JsonRpcResponse = serde_json::from_value(value)?;
            Ok(JsonRpcMessage::Response(response))
        } else {
            Err(Error::InvalidMessage(
                "Invalid JSON-RPC message structure".to_string(),
            ))
        }
    }

    /// WebSocketMessageに変換
    pub fn to_websocket(&self) -> Result<WebSocketMessage> {
        let json = match self {
            JsonRpcMessage::Request(req) => serde_json::to_string(req)?,
            JsonRpcMessage::Response(res) => serde_json::to_string(res)?,
            JsonRpcMessage::Notification(notif) => serde_json::to_string(notif)?,
        };
        Ok(WebSocketMessage::Text(json))
    }

    /// バイナリWebSocketMessageに変換
    pub fn to_websocket_binary(&self) -> Result<WebSocketMessage> {
        let json = match self {
            JsonRpcMessage::Request(req) => serde_json::to_vec(req)?,
            JsonRpcMessage::Response(res) => serde_json::to_vec(res)?,
            JsonRpcMessage::Notification(notif) => serde_json::to_vec(notif)?,
        };
        Ok(WebSocketMessage::Binary(json))
    }
}

/// JSON-RPCエラーコード定数
pub mod error_codes {
    /// Parse error - JSONとして無効
    pub const PARSE_ERROR: i32 = -32700;
    /// Invalid Request - JSON-RPCリクエストとして無効
    pub const INVALID_REQUEST: i32 = -32600;
    /// Method not found - メソッドが存在しない
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid params - 無効なパラメータ
    pub const INVALID_PARAMS: i32 = -32602;
    /// Internal error - 内部エラー
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Server error - サーバー定義エラー（-32000 to -32099）
    pub const SERVER_ERROR_START: i32 = -32000;
    pub const SERVER_ERROR_END: i32 = -32099;
}

/// JSON-RPCエラーレスポンスヘルパー
impl JsonRpcResponse {
    /// Parse errorレスポンスを作成
    pub fn parse_error(id: RequestId, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: error_codes::PARSE_ERROR,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }

    /// Invalid requestレスポンスを作成
    pub fn invalid_request(id: RequestId, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: error_codes::INVALID_REQUEST,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }

    /// Method not foundレスポンスを作成
    pub fn method_not_found(id: RequestId, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: error_codes::METHOD_NOT_FOUND,
                message: format!("Method not found: {}", method.into()),
                data: None,
            }),
            id,
        }
    }

    /// Invalid paramsレスポンスを作成
    pub fn invalid_params(id: RequestId, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: error_codes::INVALID_PARAMS,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }

    /// Internal errorレスポンスを作成
    pub fn internal_error(id: RequestId, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: error_codes::INTERNAL_ERROR,
                message: message.into(),
                data: None,
            }),
            id,
        }
    }

    /// Successレスポンスを作成
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_jsonrpc_request() {
        let json = r#"{"jsonrpc":"2.0","method":"test","params":{"key":"value"},"id":1}"#;
        let msg = JsonRpcMessage::from_json_str(json).unwrap();

        match msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.jsonrpc, "2.0");
                assert_eq!(req.method, "test");
                assert_eq!(req.id, RequestId::Number(1));
            }
            _ => panic!("Expected Request"),
        }
    }

    #[test]
    fn test_parse_jsonrpc_response() {
        let json = r#"{"jsonrpc":"2.0","result":"success","id":1}"#;
        let msg = JsonRpcMessage::from_json_str(json).unwrap();

        match msg {
            JsonRpcMessage::Response(res) => {
                assert_eq!(res.jsonrpc, "2.0");
                assert!(res.result.is_some());
                assert!(res.error.is_none());
            }
            _ => panic!("Expected Response"),
        }
    }

    #[test]
    fn test_parse_jsonrpc_notification() {
        let json = r#"{"jsonrpc":"2.0","method":"notify","params":{"message":"hello"}}"#;
        let msg = JsonRpcMessage::from_json_str(json).unwrap();

        match msg {
            JsonRpcMessage::Notification(notif) => {
                assert_eq!(notif.jsonrpc, "2.0");
                assert_eq!(notif.method, "notify");
            }
            _ => panic!("Expected Notification"),
        }
    }

    #[test]
    fn test_parse_jsonrpc_error() {
        let json =
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#;
        let msg = JsonRpcMessage::from_json_str(json).unwrap();

        match msg {
            JsonRpcMessage::Response(res) => {
                assert!(res.error.is_some());
                let err = res.error.unwrap();
                assert_eq!(err.code, -32601);
                assert_eq!(err.message, "Method not found");
            }
            _ => panic!("Expected Response with error"),
        }
    }

    #[test]
    fn test_websocket_text_conversion() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "test".to_string(),
            params: Some(json!({"key": "value"})),
            id: RequestId::Number(1),
        };

        let msg = JsonRpcMessage::Request(request);
        let ws_msg = msg.to_websocket().unwrap();

        match ws_msg {
            WebSocketMessage::Text(text) => {
                assert!(text.contains("\"method\":\"test\""));
            }
            _ => panic!("Expected Text message"),
        }
    }

    #[test]
    fn test_websocket_binary_conversion() {
        let response = JsonRpcResponse::success(RequestId::Number(1), json!("result"));

        let msg = JsonRpcMessage::Response(response);
        let ws_msg = msg.to_websocket_binary().unwrap();

        match ws_msg {
            WebSocketMessage::Binary(_) => {
                // Success
            }
            _ => panic!("Expected Binary message"),
        }
    }

    #[test]
    fn test_error_response_helpers() {
        let res = JsonRpcResponse::parse_error(RequestId::Number(1), "Invalid JSON");
        assert_eq!(res.error.as_ref().unwrap().code, error_codes::PARSE_ERROR);

        let res = JsonRpcResponse::method_not_found(RequestId::Number(1), "unknown");
        assert_eq!(
            res.error.as_ref().unwrap().code,
            error_codes::METHOD_NOT_FOUND
        );

        let res = JsonRpcResponse::invalid_params(RequestId::Number(1), "Missing param");
        assert_eq!(
            res.error.as_ref().unwrap().code,
            error_codes::INVALID_PARAMS
        );
    }

    #[test]
    fn test_invalid_jsonrpc_version() {
        let json = r#"{"jsonrpc":"1.0","method":"test","id":1}"#;
        let result = JsonRpcMessage::from_json_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_jsonrpc_field() {
        let json = r#"{"method":"test","id":1}"#;
        let result = JsonRpcMessage::from_json_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_websocket_text() {
        let ws_msg =
            WebSocketMessage::Text(r#"{"jsonrpc":"2.0","method":"test","id":1}"#.to_string());
        let msg = JsonRpcMessage::from_websocket(ws_msg).unwrap();

        match msg {
            JsonRpcMessage::Request(req) => {
                assert_eq!(req.method, "test");
            }
            _ => panic!("Expected Request"),
        }
    }

    #[test]
    fn test_from_websocket_close() {
        let ws_msg = WebSocketMessage::Close(None);
        let result = JsonRpcMessage::from_websocket(ws_msg);
        assert!(result.is_err());
    }
}
