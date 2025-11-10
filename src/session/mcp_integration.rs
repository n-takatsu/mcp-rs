use crate::session::{
    SessionManager, SessionSecurityMiddleware, SessionId, SessionError, Session
};
use crate::mcp::{McpHandler, Tool, ToolCallParams, InitializeParams, Resource, ResourceReadParams};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, error, info, warn, instrument};
use chrono::Utc;

/// セッション対応MCPハンドラーラッパー
/// 
/// 既存のMCPハンドラーをセッション管理システムと統合し、
/// セッションレベルでのセキュリティ・監査・状態管理を提供
#[derive(Debug)]
pub struct SessionAwareMcpHandler<H: McpHandler> {
    /// 内部ハンドラー
    inner_handler: H,
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// セキュリティミドルウェア
    security_middleware: Arc<SessionSecurityMiddleware>,
    /// ハンドラー設定
    config: SessionAwareMcpConfig,
}

/// セッション対応MCPハンドラー設定
#[derive(Debug, Clone)]
pub struct SessionAwareMcpConfig {
    /// セッション必須操作
    pub require_session_for_operations: bool,
    /// セッション自動作成
    pub auto_create_session: bool,
    /// 操作ログ詳細レベル
    pub operation_log_level: OperationLogLevel,
    /// セキュリティ検証レベル
    pub security_validation_level: crate::session::SecurityValidationLevel,
    /// セッション更新間隔
    pub session_update_interval_secs: u64,
    /// エラー時のセッション処理
    pub error_session_handling: ErrorSessionHandling,
}

/// 操作ログレベル
#[derive(Debug, Clone, PartialEq)]
pub enum OperationLogLevel {
    /// ログなし
    None,
    /// 基本情報のみ
    Basic,
    /// 詳細情報
    Detailed,
    /// 完全ログ（デバッグ用）
    Full,
}

/// エラー時のセッション処理
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSessionHandling {
    /// 何もしない
    None,
    /// 警告ログのみ
    LogWarning,
    /// セキュリティ違反として記録
    RecordViolation,
    /// セッション一時停止
    SuspendSession,
}

/// セッション対応操作コンテキスト
#[derive(Debug, Clone)]
pub struct SessionOperationContext {
    /// セッションID
    pub session_id: Option<SessionId>,
    /// 操作ID
    pub operation_id: uuid::Uuid,
    /// 操作開始時刻
    pub started_at: chrono::DateTime<Utc>,
    /// 操作タイプ
    pub operation_type: String,
    /// クライアント情報
    pub client_info: Option<ClientInfo>,
}

/// クライアント情報
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClientInfo {
    /// IPアドレス
    pub ip_address: Option<std::net::IpAddr>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// リクエストヘッダー
    pub request_headers: std::collections::HashMap<String, String>,
}

impl<H: McpHandler + Send + Sync> SessionAwareMcpHandler<H> {
    /// 新しいセッション対応MCPハンドラーを作成
    pub fn new(
        inner_handler: H,
        session_manager: Arc<SessionManager>,
        security_middleware: Arc<SessionSecurityMiddleware>,
        config: SessionAwareMcpConfig,
    ) -> Self {
        Self {
            inner_handler,
            session_manager,
            security_middleware,
            config,
        }
    }
    
    /// セッション情報を含むツール実行
    #[instrument(skip(self, params), fields(tool_name = %params.name))]
    async fn execute_tool_with_session(
        &self,
        params: ToolCallParams,
        session_context: Option<SessionOperationContext>,
    ) -> Result<Value, crate::mcp::McpError> {
        let operation_start = std::time::Instant::now();
        
        // セッション検証
        if self.config.require_session_for_operations && session_context.is_none() {
            return Err(crate::mcp::McpError::Unauthorized("Session required for this operation".to_string()));
        }
        
        // セキュリティ検証
        if let Some(ref context) = session_context {
            if let Some(ref session_id) = context.session_id {
                self.validate_session_operation(session_id, &params).await?;
            }
        }
        
        // 操作ログ記録（開始）
        if self.should_log_operation() {
            self.log_operation_start(&params, &session_context).await;
        }
        
        // 内部ハンドラー実行
        let result = self.inner_handler.call_tool(params.clone()).await;
        
        let operation_duration = operation_start.elapsed();
        
        // 結果処理
        match &result {
            Ok(response) => {
                // 成功時の処理
                self.handle_operation_success(
                    &params,
                    response,
                    &session_context,
                    operation_duration,
                ).await;
            }
            Err(error) => {
                // エラー時の処理
                self.handle_operation_error(
                    &params,
                    error,
                    &session_context,
                    operation_duration,
                ).await;
            }
        }
        
        result
    }
    
    /// セッション操作検証
    async fn validate_session_operation(
        &self,
        session_id: &SessionId,
        params: &ToolCallParams,
    ) -> Result<(), crate::mcp::McpError> {
        // セッション状態確認
        match self.session_manager.get_session(session_id).await {
            Ok(Some(session)) => {
                match session.state {
                    crate::session::SessionState::Active => {
                        // アクティブセッションの追加検証
                        self.validate_active_session_operation(&session, params).await?;
                    }
                    crate::session::SessionState::Suspended => {
                        return Err(crate::mcp::McpError::Forbidden("Session is suspended".to_string()));
                    }
                    crate::session::SessionState::Expired | crate::session::SessionState::Invalidated => {
                        return Err(crate::mcp::McpError::Unauthorized("Session is invalid".to_string()));
                    }
                    crate::session::SessionState::Pending => {
                        return Err(crate::mcp::McpError::Forbidden("Session is pending validation".to_string()));
                    }
                }
            }
            Ok(None) => {
                return Err(crate::mcp::McpError::NotFound("Session not found".to_string()));
            }
            Err(e) => {
                error!("セッション取得エラー: {}", e);
                return Err(crate::mcp::McpError::Internal(format!("Session retrieval failed: {}", e)));
            }
        }
        
        Ok(())
    }
    
    /// アクティブセッションの操作検証
    async fn validate_active_session_operation(
        &self,
        session: &Session,
        params: &ToolCallParams,
    ) -> Result<(), crate::mcp::McpError> {
        // セキュリティレベルによる制限チェック
        match session.security.security_level {
            crate::session::SecurityLevel::Maximum => {
                // 最高セキュリティレベル：追加検証
                self.validate_maximum_security_operation(session, params).await?;
            }
            crate::session::SecurityLevel::High => {
                // 高セキュリティレベル：標準検証
                self.validate_high_security_operation(session, params).await?;
            }
            _ => {
                // 標準・低セキュリティレベル：基本検証のみ
            }
        }
        
        // セッション最終アクセス時刻更新
        if let Err(e) = self.session_manager.touch_session(&session.id).await {
            warn!("セッション最終アクセス時刻更新失敗: {}", e);
        }
        
        Ok(())
    }
    
    /// 最高セキュリティ操作検証
    async fn validate_maximum_security_operation(
        &self,
        session: &Session,
        params: &ToolCallParams,
    ) -> Result<(), crate::mcp::McpError> {
        // 危険な操作の制限
        let restricted_operations = vec![
            "execute_command",
            "file_write",
            "database_write",
            "system_modify",
        ];
        
        if restricted_operations.contains(&params.name.as_str()) {
            return Err(crate::mcp::McpError::Forbidden(
                "Operation not allowed in maximum security mode".to_string()
            ));
        }
        
        // 入力パラメータのセキュリティ検証
        if let Some(ref arguments) = params.arguments {
            let input_str = serde_json::to_string(arguments)
                .map_err(|e| crate::mcp::McpError::InvalidRequest(format!("Invalid arguments: {}", e)))?;
            
            let validation_result = self.security_middleware
                .validate_session_input(&session.id, &input_str, "mcp_tool_arguments")
                .await
                .map_err(|e| crate::mcp::McpError::Internal(format!("Security validation failed: {}", e)))?;
            
            if !validation_result.is_valid {
                return Err(crate::mcp::McpError::Forbidden(
                    format!("Security validation failed: {:?}", validation_result.errors)
                ));
            }
        }
        
        Ok(())
    }
    
    /// 高セキュリティ操作検証
    async fn validate_high_security_operation(
        &self,
        session: &Session,
        params: &ToolCallParams,
    ) -> Result<(), crate::mcp::McpError> {
        // 基本的なパラメータ検証
        if let Some(ref arguments) = params.arguments {
            // SQLインジェクション・XSS基本チェック
            let input_str = serde_json::to_string(arguments)
                .map_err(|e| crate::mcp::McpError::InvalidRequest(format!("Invalid arguments: {}", e)))?;
            
            // 簡易セキュリティチェック（実装は security_middleware に依存）
            if input_str.to_lowercase().contains("script>") || 
               input_str.to_lowercase().contains("union select") {
                return Err(crate::mcp::McpError::Forbidden("Potentially malicious input detected".to_string()));
            }
        }
        
        Ok(())
    }
    
    /// 操作成功処理
    async fn handle_operation_success(
        &self,
        params: &ToolCallParams,
        response: &Value,
        session_context: &Option<SessionOperationContext>,
        duration: std::time::Duration,
    ) {
        // セッション統計更新
        if let Some(context) = session_context {
            if let Some(ref session_id) = context.session_id {
                let _ = self.update_session_statistics(session_id, &params.name, duration, true).await;
            }
        }
        
        // 操作ログ記録（成功）
        if self.should_log_operation() {
            self.log_operation_success(params, response, session_context, duration).await;
        }
    }
    
    /// 操作エラー処理
    async fn handle_operation_error(
        &self,
        params: &ToolCallParams,
        error: &crate::mcp::McpError,
        session_context: &Option<SessionOperationContext>,
        duration: std::time::Duration,
    ) {
        // エラー時のセッション処理
        if let Some(context) = session_context {
            if let Some(ref session_id) = context.session_id {
                match self.config.error_session_handling {
                    ErrorSessionHandling::RecordViolation => {
                        let _ = self.security_middleware.record_session_violation(
                            session_id,
                            crate::session::SecurityEventType::PolicyViolation,
                            crate::session::SecuritySeverity::Warning,
                            json!({
                                "operation": params.name,
                                "error": format!("{:?}", error),
                                "duration_ms": duration.as_millis()
                            }),
                        ).await;
                    }
                    ErrorSessionHandling::SuspendSession => {
                        if let Ok(Some(mut session)) = self.session_manager.get_session(session_id).await {
                            session.state = crate::session::SessionState::Suspended;
                            let _ = self.session_manager.update_session(&session).await;
                        }
                    }
                    _ => {}
                }
                
                let _ = self.update_session_statistics(session_id, &params.name, duration, false).await;
            }
        }
        
        // 操作ログ記録（エラー）
        if self.should_log_operation() {
            self.log_operation_error(params, error, session_context, duration).await;
        }
    }
    
    /// セッション統計更新
    async fn update_session_statistics(
        &self,
        session_id: &SessionId,
        operation_name: &str,
        duration: std::time::Duration,
        success: bool,
    ) -> Result<(), SessionError> {
        // セッションの要求カウントと転送バイト数を更新
        if let Some(mut session) = self.session_manager.get_session(session_id).await? {
            session.metadata.request_count += 1;
            session.metadata.last_accessed = Utc::now();
            
            // 操作タイプに基づく統計更新
            let estimated_bytes = match operation_name {
                "list_files" => 1024,
                "read_file" => 8192,
                "write_file" => 16384,
                _ => 512,
            };
            
            session.metadata.bytes_transferred += estimated_bytes;
            
            self.session_manager.update_session(&session).await?;
        }
        
        Ok(())
    }
    
    /// 操作ログ判定
    fn should_log_operation(&self) -> bool {
        self.config.operation_log_level != OperationLogLevel::None
    }
    
    /// 操作開始ログ
    async fn log_operation_start(
        &self,
        params: &ToolCallParams,
        session_context: &Option<SessionOperationContext>,
    ) {
        match self.config.operation_log_level {
            OperationLogLevel::Full => {
                info!(
                    "MCP操作開始: tool={}, session={:?}, params={:?}",
                    params.name,
                    session_context.as_ref().and_then(|c| c.session_id.as_ref()),
                    params.arguments
                );
            }
            OperationLogLevel::Detailed => {
                info!(
                    "MCP操作開始: tool={}, session={:?}",
                    params.name,
                    session_context.as_ref().and_then(|c| c.session_id.as_ref())
                );
            }
            OperationLogLevel::Basic => {
                debug!("MCP操作開始: {}", params.name);
            }
            _ => {}
        }
    }
    
    /// 操作成功ログ
    async fn log_operation_success(
        &self,
        params: &ToolCallParams,
        response: &Value,
        session_context: &Option<SessionOperationContext>,
        duration: std::time::Duration,
    ) {
        match self.config.operation_log_level {
            OperationLogLevel::Full => {
                info!(
                    "MCP操作成功: tool={}, duration={}ms, session={:?}, response_size={}",
                    params.name,
                    duration.as_millis(),
                    session_context.as_ref().and_then(|c| c.session_id.as_ref()),
                    response.to_string().len()
                );
            }
            OperationLogLevel::Detailed => {
                info!(
                    "MCP操作成功: tool={}, duration={}ms",
                    params.name,
                    duration.as_millis()
                );
            }
            OperationLogLevel::Basic => {
                debug!("MCP操作成功: {}", params.name);
            }
            _ => {}
        }
    }
    
    /// 操作エラーログ
    async fn log_operation_error(
        &self,
        params: &ToolCallParams,
        error: &crate::mcp::McpError,
        session_context: &Option<SessionOperationContext>,
        duration: std::time::Duration,
    ) {
        error!(
            "MCP操作エラー: tool={}, error={:?}, duration={}ms, session={:?}",
            params.name,
            error,
            duration.as_millis(),
            session_context.as_ref().and_then(|c| c.session_id.as_ref())
        );
    }
    
    /// セッションコンテキスト抽出（リクエストから）
    pub async fn extract_session_context(
        &self,
        headers: Option<&std::collections::HashMap<String, String>>,
    ) -> Option<SessionOperationContext> {
        let session_id = if let Some(headers) = headers {
            headers.get("x-session-id")
                .map(|s| SessionId::from_string(s.clone()))
        } else {
            None
        };
        
        let client_info = if let Some(headers) = headers {
            Some(ClientInfo {
                ip_address: headers.get("x-forwarded-for")
                    .or_else(|| headers.get("x-real-ip"))
                    .and_then(|s| s.parse().ok()),
                user_agent: headers.get("user-agent").cloned(),
                request_headers: headers.clone(),
            })
        } else {
            None
        };
        
        Some(SessionOperationContext {
            session_id,
            operation_id: uuid::Uuid::new_v4(),
            started_at: Utc::now(),
            operation_type: "mcp_tool".to_string(),
            client_info,
        })
    }
}

#[async_trait]
impl<H: McpHandler + Send + Sync> McpHandler for SessionAwareMcpHandler<H> {
    async fn initialize(&self, params: InitializeParams) -> Result<Value, crate::mcp::McpError> {
        debug!("セッション対応MCPハンドラー初期化");
        self.inner_handler.initialize(params).await
    }
    
    async fn list_tools(&self) -> Result<Vec<Tool>, crate::mcp::McpError> {
        self.inner_handler.list_tools().await
    }
    
    async fn call_tool(&self, params: ToolCallParams) -> Result<Value, crate::mcp::McpError> {
        // セッションコンテキスト抽出は実際のHTTPリクエストコンテキストから行う
        // ここでは簡略化してNoneを使用
        let session_context = None;
        
        self.execute_tool_with_session(params, session_context).await
    }
    
    async fn list_resources(&self) -> Result<Vec<Resource>, crate::mcp::McpError> {
        self.inner_handler.list_resources().await
    }
    
    async fn read_resource(&self, params: ResourceReadParams) -> Result<Value, crate::mcp::McpError> {
        self.inner_handler.read_resource(params).await
    }
}

impl Default for SessionAwareMcpConfig {
    fn default() -> Self {
        Self {
            require_session_for_operations: false,
            auto_create_session: true,
            operation_log_level: OperationLogLevel::Basic,
            security_validation_level: crate::session::SecurityValidationLevel::Standard,
            session_update_interval_secs: 300,
            error_session_handling: ErrorSessionHandling::LogWarning,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_aware_mcp_config_default() {
        let config = SessionAwareMcpConfig::default();
        assert!(!config.require_session_for_operations);
        assert!(config.auto_create_session);
        assert_eq!(config.operation_log_level, OperationLogLevel::Basic);
    }
    
    #[test]
    fn test_operation_log_levels() {
        assert_eq!(OperationLogLevel::None, OperationLogLevel::None);
        assert_ne!(OperationLogLevel::Basic, OperationLogLevel::Full);
    }
}