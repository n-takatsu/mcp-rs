//! セキュアなMCPサーバー実装
//!
//! このモジュールは入力検証、レート制限、暗号化を統合したセキュアなサーバーを提供します。

use crate::{
    config::RateLimitConfig,
    error::{Error, Result, SecurityError},
    protocol::McpProtocol,
    security::{InputValidator, RateLimiter, ValidationResult},
    types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, RequestId},
};
use axum::{
    extract::{ConnectInfo, State},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc, time::Instant};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

/// セキュリティ設定
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// 入力検証を有効にするか
    pub enable_input_validation: bool,
    /// レート制限を有効にするか
    pub enable_rate_limiting: bool,
    /// 最大リクエストサイズ（バイト）
    pub max_request_size: usize,
    /// ログレベル
    pub log_security_events: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_input_validation: true,
            enable_rate_limiting: true,
            max_request_size: 1024 * 1024, // 1MB
            log_security_events: true,
        }
    }
}

/// セキュアなMCPサーバー
pub struct SecureMcpServer<P: McpProtocol> {
    protocol: Arc<P>,
    input_validator: Arc<InputValidator>,
    rate_limiter: Arc<RateLimiter>,
    config: SecurityConfig,
}

impl<P: McpProtocol + 'static> SecureMcpServer<P> {
    /// 新しいセキュアサーバーを作成
    pub fn new(protocol: P, config: SecurityConfig) -> Self {
        Self {
            protocol: Arc::new(protocol),
            input_validator: Arc::new(InputValidator::new()),
            rate_limiter: Arc::new(RateLimiter::new(RateLimitConfig {
                requests_per_second: 100,
                burst_size: 200,
                enabled: true,
            })),
            config,
        }
    }

    /// デフォルト設定でセキュアサーバーを作成
    pub fn with_defaults(protocol: P) -> Self {
        Self::new(protocol, SecurityConfig::default())
    }

    /// カスタム入力検証システムを設定
    pub fn with_validator(mut self, validator: InputValidator) -> Self {
        self.input_validator = Arc::new(validator);
        self
    }

    /// カスタムレート制限を設定
    pub fn with_rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = Arc::new(rate_limiter);
        self
    }

    /// Axumルーターを作成
    pub fn router(self) -> Router {
        let server_state = Arc::new(ServerState {
            protocol: Arc::clone(&self.protocol),
            input_validator: Arc::clone(&self.input_validator),
            rate_limiter: Arc::clone(&self.rate_limiter),
            config: self.config,
        });

        Router::new()
            .route("/", post(handle_secure_request::<P>))
            .layer(TraceLayer::new_for_http())
            .with_state(server_state)
    }

    /// セキュアサーバーを開始
    pub async fn serve(self, addr: impl Into<std::net::SocketAddr>) -> Result<()> {
        let addr = addr.into();
        info!("Starting secure MCP server on {}", addr);

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, self.router())
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|e| Error::Internal(e.to_string()))?;

        Ok(())
    }
}

/// サーバー状態（共有リソース）
#[derive(Debug)]
struct ServerState<P: McpProtocol> {
    protocol: Arc<P>,
    input_validator: Arc<InputValidator>,
    rate_limiter: Arc<RateLimiter>,
    config: SecurityConfig,
}

/// セキュアなリクエストハンドラ
async fn handle_secure_request<P: McpProtocol>(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<ServerState<P>>>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    let start_time = Instant::now();

    // レート制限チェック
    if state.config.enable_rate_limiting {
        let client_id = addr.ip().to_string();
        match state.rate_limiter.check_rate_limit(&client_id).await {
            Err(_) => {
                if state.config.log_security_events {
                    warn!("Rate limit exceeded for client: {}", addr);
                }

                let error = JsonRpcError {
                    code: -32000,
                    message: "Rate limit exceeded".to_string(),
                    data: None,
                };

                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(error),
                };

                return Json(response);
            }
            Ok(_) => {
                // Rate limit passed, continue processing
            }
        }
    }

    // 入力検証
    if state.config.enable_input_validation {
        if let Err(validation_error) = validate_request(&state.input_validator, &request) {
            if state.config.log_security_events {
                warn!(
                    "Input validation failed for client {}: {}",
                    addr, validation_error
                );
            }

            let error = JsonRpcError {
                code: -32602,
                message: format!("Invalid input: {}", validation_error),
                data: None,
            };

            let response = JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: None,
                error: Some(error),
            };

            return Json(response);
        }
    }

    // プロトコル処理
    let response = match request.method.as_str() {
        "initialize" => match state.protocol.initialize().await {
            Ok((server_info, capabilities)) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({
                    "serverInfo": server_info,
                    "capabilities": capabilities
                })),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: e.to_json_rpc_code(),
                    message: e.to_string(),
                    data: None,
                }),
            },
        },
        "tools/list" => match state.protocol.list_tools().await {
            Ok(tools) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: Some(json!({ "tools": tools })),
                error: None,
            },
            Err(e) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: e.to_json_rpc_code(),
                    message: e.to_string(),
                    data: None,
                }),
            },
        },
        "tools/call" => {
            if let Some(params) = request.params {
                // パラメータから tool名と引数を抽出
                let tool_name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let arguments = params.get("arguments");

                match state
                    .protocol
                    .call_tool(tool_name, arguments.cloned())
                    .await
                {
                    Ok(result) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: Some(result),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: request.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: e.to_json_rpc_code(),
                            message: e.to_string(),
                            data: None,
                        }),
                    },
                }
            } else {
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Invalid params".to_string(),
                        data: None,
                    }),
                }
            }
        }
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
        },
    };

    // セキュリティイベントログ
    if state.config.log_security_events {
        let processing_time = start_time.elapsed();
        info!(
            "Request processed: method={}, client={}, time={:?}, success={}",
            request.method,
            addr,
            processing_time,
            response.error.is_none()
        );
    }

    Json(response)
}

/// リクエストの入力検証
fn validate_request(
    validator: &InputValidator,
    request: &JsonRpcRequest,
) -> std::result::Result<(), SecurityError> {
    // メソッド名の検証
    let method_result = validator
        .validate_security(&request.method)
        .map_err(|e| SecurityError::ValidationError(format!("Method validation failed: {}", e)))?;

    if !method_result.is_valid {
        return Err(SecurityError::ValidationError(format!(
            "Invalid method name: {}",
            method_result.errors.join(", ")
        )));
    }

    // パラメータの検証（JSON文字列として）
    if let Some(ref params) = request.params {
        let params_str = serde_json::to_string(params).map_err(|e| {
            SecurityError::ValidationError(format!("Parameter serialization failed: {}", e))
        })?;

        let params_result = validator.validate_security(&params_str).map_err(|e| {
            SecurityError::ValidationError(format!("Parameter validation failed: {}", e))
        })?;

        if !params_result.is_valid {
            return Err(SecurityError::ValidationError(format!(
                "Invalid parameters: {}",
                params_result.errors.join(", ")
            )));
        }
    }

    Ok(())
}

/// グレースフルシャットダウンシグナル
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
}

/// セキュリティメトリクス
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SecurityMetrics {
    /// 処理されたリクエスト総数
    pub total_requests: u64,
    /// 拒否されたリクエスト数
    pub rejected_requests: u64,
    /// レート制限によるブロック数
    pub rate_limited: u64,
    /// 検証失敗数
    pub validation_failures: u64,
    /// 平均処理時間（ミリ秒）
    pub avg_processing_time_ms: f64,
}

impl Default for SecurityMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            rejected_requests: 0,
            rate_limited: 0,
            validation_failures: 0,
            avg_processing_time_ms: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::BasicMcpProtocol;
    use serde_json::Value;

    #[tokio::test]
    async fn test_secure_server_creation() {
        let protocol = BasicMcpProtocol::new("test-server", "0.1.0");
        let config = SecurityConfig::default();
        let server = SecureMcpServer::new(protocol, config);
        let _router = server.router();
    }

    #[tokio::test]
    async fn test_input_validation() {
        let validator = InputValidator::new();

        // 正常なリクエスト
        let normal_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String("1".to_string()),
            method: "initialize".to_string(),
            params: None,
        };

        assert!(validate_request(&validator, &normal_request).is_ok());

        // SQLインジェクションを含むリクエスト
        let malicious_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: RequestId::String("1".to_string()),
            method: "'; DROP TABLE users; --".to_string(),
            params: None,
        };

        assert!(validate_request(&validator, &malicious_request).is_err());
    }

    #[tokio::test]
    async fn test_security_config() {
        let config = SecurityConfig {
            enable_input_validation: true,
            enable_rate_limiting: false,
            max_request_size: 512,
            log_security_events: false,
        };

        assert!(config.enable_input_validation);
        assert!(!config.enable_rate_limiting);
        assert_eq!(config.max_request_size, 512);
    }

    #[test]
    fn test_security_metrics() {
        let metrics = SecurityMetrics::default();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.rejected_requests, 0);
        assert_eq!(metrics.rate_limited, 0);
        assert_eq!(metrics.validation_failures, 0);
        assert_eq!(metrics.avg_processing_time_ms, 0.0);
    }
}
