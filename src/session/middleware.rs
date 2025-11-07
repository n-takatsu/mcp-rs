use crate::session::{
    SessionSecurityIntegration, SessionSecurityConfig, SessionManager,
    SessionId, SessionError, SessionState, SecurityEventType, SecuritySeverity
};
use crate::security::{
    AuditLogger, InputValidator, RateLimiter, XssProtector, SqlInjectionProtector,
};
use crate::policy_validation::PolicyValidationEngine;
use crate::plugin_isolation::security_validation::SecurityValidationSystem;
use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, info, warn, instrument};
use serde_json::json;

/// セッションセキュリティミドルウェア
/// 
/// HTTP/WebSocketリクエストにセッションベースのセキュリティ監視を追加
#[derive(Debug, Clone)]
pub struct SessionSecurityMiddleware {
    /// セッションセキュリティ統合
    security_integration: Arc<SessionSecurityIntegration>,
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// ミドルウェア設定
    config: SessionSecurityMiddlewareConfig,
}

/// ミドルウェア設定
#[derive(Debug, Clone)]
pub struct SessionSecurityMiddlewareConfig {
    /// セッション抽出を必須とするか
    pub require_session: bool,
    /// 無効なセッションでのアクセスを許可するか
    pub allow_invalid_sessions: bool,
    /// セキュリティ検証をスキップするパス
    pub skip_validation_paths: Vec<String>,
    /// セッション作成を自動で行うか
    pub auto_create_sessions: bool,
    /// セキュリティ違反時のレスポンス設定
    pub violation_response: ViolationResponseConfig,
}

/// セキュリティ違反時のレスポンス設定
#[derive(Debug, Clone)]
pub struct ViolationResponseConfig {
    /// ブロック時のHTTPステータスコード
    pub block_status_code: u16,
    /// ブロック時のレスポンスボディ
    pub block_response_body: String,
    /// 詳細エラー情報を含めるか
    pub include_details: bool,
}

impl SessionSecurityMiddleware {
    /// 新しいミドルウェアインスタンスを作成
    pub fn new(
        session_manager: Arc<SessionManager>,
        audit_logger: Arc<AuditLogger>,
        input_validator: Arc<InputValidator>,
        rate_limiter: Arc<RateLimiter>,
        xss_protector: Arc<XssProtector>,
        sql_protector: Arc<SqlInjectionProtector>,
        policy_engine: Arc<PolicyValidationEngine>,
        security_validator: Arc<SecurityValidationSystem>,
        security_config: SessionSecurityConfig,
        middleware_config: SessionSecurityMiddlewareConfig,
    ) -> Self {
        let security_integration = Arc::new(SessionSecurityIntegration::new(
            session_manager.clone(),
            audit_logger,
            input_validator,
            rate_limiter,
            xss_protector,
            sql_protector,
            policy_engine,
            security_validator,
            security_config,
        ));
        
        Self {
            security_integration,
            session_manager,
            config: middleware_config,
        }
    }
    
    /// HTTPリクエスト処理ミドルウェア
    #[instrument(skip(self, request, next), fields(path = %request.uri().path()))]
    pub async fn process_http_request<B>(
        &self,
        mut request: Request<B>,
        next: Next<B>,
    ) -> Result<Response, axum::response::Response> {
        let path = request.uri().path().to_string();
        
        // パスがスキップ対象かチェック
        if self.should_skip_validation(&path) {
            debug!("セキュリティ検証をスキップ: {}", path);
            return Ok(next.run(request).await);
        }
        
        // セッション情報を抽出
        let session_info = self.extract_session_info(&request).await;
        
        match session_info {
            Ok(Some(session_id)) => {
                // セッション存在時の処理
                self.process_with_session(&mut request, &session_id, next).await
            }
            Ok(None) => {
                // セッション未存在時の処理
                self.process_without_session(&mut request, next).await
            }
            Err(e) => {
                // セッション抽出エラー時の処理
                error!("セッション抽出エラー: {}", e);
                self.create_error_response(
                    axum::http::StatusCode::BAD_REQUEST,
                    "Session extraction failed",
                ).await
            }
        }
    }
    
    /// セッション付きリクエスト処理
    async fn process_with_session<B>(
        &self,
        request: &mut Request<B>,
        session_id: &SessionId,
        next: Next<B>,
    ) -> Result<Response, axum::response::Response> {
        debug!("セッション付きリクエスト処理: {}", session_id);
        
        // セッション状態確認
        match self.session_manager.get_session(session_id).await {
            Ok(Some(session)) => {
                match session.state {
                    SessionState::Active => {
                        // アクティブセッションの処理
                        self.process_active_session(request, &session, next).await
                    }
                    SessionState::Suspended => {
                        warn!("停止中のセッションからのアクセス: {}", session_id);
                        self.create_error_response(
                            axum::http::StatusCode::FORBIDDEN,
                            "Session is suspended",
                        ).await
                    }
                    SessionState::Expired | SessionState::Invalidated => {
                        warn!("無効なセッションからのアクセス: {}", session_id);
                        if self.config.allow_invalid_sessions {
                            Ok(next.run(*request).await)
                        } else {
                            self.create_error_response(
                                axum::http::StatusCode::UNAUTHORIZED,
                                "Session is invalid",
                            ).await
                        }
                    }
                    SessionState::Pending => {
                        debug!("保留中のセッション: {}", session_id);
                        self.create_error_response(
                            axum::http::StatusCode::ACCEPTED,
                            "Session is pending validation",
                        ).await
                    }
                }
            }
            Ok(None) => {
                warn!("存在しないセッション: {}", session_id);
                if self.config.allow_invalid_sessions {
                    Ok(next.run(*request).await)
                } else {
                    self.create_error_response(
                        axum::http::StatusCode::UNAUTHORIZED,
                        "Session not found",
                    ).await
                }
            }
            Err(e) => {
                error!("セッション取得エラー: {}", e);
                self.create_error_response(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Session retrieval failed",
                ).await
            }
        }
    }
    
    /// アクティブセッションの処理
    async fn process_active_session<B>(
        &self,
        request: &mut Request<B>,
        session: &crate::session::Session,
        next: Next<B>,
    ) -> Result<Response, axum::response::Response> {
        // 異常検出実行
        if let Ok(Some(anomaly_result)) = self.security_integration
            .detect_session_anomalies(&session.id).await {
            
            warn!(
                "セッション異常検出: {} (score: {:.2})",
                session.id,
                anomaly_result.anomaly_score
            );
            
            // 重大な異常の場合はブロック
            if anomaly_result.anomaly_score >= 0.8 {
                let _ = self.security_integration.record_session_violation(
                    &session.id,
                    SecurityEventType::AnomalousActivityDetected,
                    SecuritySeverity::Critical,
                    json!({
                        "anomaly_types": anomaly_result.anomaly_types,
                        "anomaly_score": anomaly_result.anomaly_score
                    }),
                ).await;
                
                return self.create_error_response(
                    axum::http::StatusCode::FORBIDDEN,
                    "Anomalous activity detected",
                ).await;
            }
        }
        
        // セッション情報をリクエストヘッダーに追加
        request.headers_mut().insert(
            "X-Session-ID",
            session.id.as_str().parse().unwrap(),
        );
        
        if let Some(ref user_id) = session.user_id {
            request.headers_mut().insert(
                "X-User-ID",
                user_id.parse().unwrap(),
            );
        }
        
        // セッション最終アクセス時刻更新
        if let Err(e) = self.session_manager.touch_session(&session.id).await {
            warn!("セッション最終アクセス時刻更新失敗: {}", e);
        }
        
        Ok(next.run(*request).await)
    }
    
    /// セッション未存在時の処理
    async fn process_without_session<B>(
        &self,
        request: &mut Request<B>,
        next: Next<B>,
    ) -> Result<Response, axum::response::Response> {
        if self.config.require_session {
            return self.create_error_response(
                axum::http::StatusCode::UNAUTHORIZED,
                "Session required",
            ).await;
        }
        
        if self.config.auto_create_sessions {
            // 自動セッション作成
            match self.auto_create_session(request).await {
                Ok(session_id) => {
                    info!("自動セッション作成: {}", session_id);
                    request.headers_mut().insert(
                        "X-Session-ID",
                        session_id.as_str().parse().unwrap(),
                    );
                }
                Err(e) => {
                    error!("自動セッション作成失敗: {}", e);
                    // エラーでもリクエストは継続
                }
            }
        }
        
        Ok(next.run(*request).await)
    }
    
    /// セッション情報抽出
    async fn extract_session_info<B>(&self, request: &Request<B>) -> Result<Option<SessionId>, SessionError> {
        // Cookie からセッションIDを抽出
        if let Some(cookie_header) = request.headers().get("cookie") {
            if let Ok(cookie_str) = cookie_header.to_str() {
                for cookie in cookie_str.split(';') {
                    let cookie = cookie.trim();
                    if let Some(session_value) = cookie.strip_prefix("session_id=") {
                        return Ok(Some(SessionId::from_string(session_value.to_string())));
                    }
                }
            }
        }
        
        // Authorization ヘッダーからセッショントークンを抽出
        if let Some(auth_header) = request.headers().get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    // トークンからセッションIDを抽出（実装依存）
                    return Ok(Some(SessionId::from_string(token.to_string())));
                }
            }
        }
        
        // カスタムヘッダーからセッションIDを抽出
        if let Some(session_header) = request.headers().get("x-session-id") {
            if let Ok(session_str) = session_header.to_str() {
                return Ok(Some(SessionId::from_string(session_str.to_string())));
            }
        }
        
        Ok(None)
    }
    
    /// 自動セッション作成
    async fn auto_create_session<B>(&self, request: &Request<B>) -> Result<SessionId, SessionError> {
        // リクエストからクライアント情報を抽出
        let ip_address = self.extract_client_ip(request);
        let user_agent = request.headers()
            .get("user-agent")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        
        // セッション作成前のセキュリティ検証
        let validation_result = self.security_integration
            .validate_session_creation(None, ip_address, user_agent.as_deref())
            .await?;
        
        if validation_result.has_violations() {
            return Err(SessionError::SecurityViolation(format!(
                "Session creation security validation failed: {:?}",
                validation_result.violations
            )));
        }
        
        // セッション作成
        let session_request = crate::session::CreateSessionRequest {
            user_id: None,
            ttl: None,
            ip_address,
            user_agent,
            security_level: None,
            initial_data: None,
        };
        
        self.session_manager.create_session(session_request).await
    }
    
    /// クライアントIP抽出
    fn extract_client_ip<B>(&self, request: &Request<B>) -> Option<std::net::IpAddr> {
        // X-Forwarded-For ヘッダーをチェック
        if let Some(forwarded) = request.headers().get("x-forwarded-for") {
            if let Ok(forwarded_str) = forwarded.to_str() {
                if let Some(first_ip) = forwarded_str.split(',').next() {
                    if let Ok(ip) = first_ip.trim().parse() {
                        return Some(ip);
                    }
                }
            }
        }
        
        // X-Real-IP ヘッダーをチェック
        if let Some(real_ip) = request.headers().get("x-real-ip") {
            if let Ok(ip_str) = real_ip.to_str() {
                if let Ok(ip) = ip_str.parse() {
                    return Some(ip);
                }
            }
        }
        
        // ConnectInfo から抽出（Axum拡張機能）
        // 実際の実装ではExtractorを使用
        None
    }
    
    /// 検証スキップ判定
    fn should_skip_validation(&self, path: &str) -> bool {
        self.config.skip_validation_paths.iter()
            .any(|skip_path| path.starts_with(skip_path))
    }
    
    /// エラーレスポンス作成
    async fn create_error_response(
        &self,
        status: axum::http::StatusCode,
        message: &str,
    ) -> Result<Response, axum::response::Response> {
        let body = if self.config.violation_response.include_details {
            json!({
                "error": message,
                "timestamp": chrono::Utc::now(),
                "status": status.as_u16()
            }).to_string()
        } else {
            self.config.violation_response.block_response_body.clone()
        };
        
        Err(axum::response::Response::builder()
            .status(status)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(body))
            .unwrap()
            .into_response())
    }
}

impl Default for SessionSecurityMiddlewareConfig {
    fn default() -> Self {
        Self {
            require_session: false,
            allow_invalid_sessions: false,
            skip_validation_paths: vec![
                "/health".to_string(),
                "/metrics".to_string(),
                "/static/".to_string(),
            ],
            auto_create_sessions: true,
            violation_response: ViolationResponseConfig::default(),
        }
    }
}

impl Default for ViolationResponseConfig {
    fn default() -> Self {
        Self {
            block_status_code: 403,
            block_response_body: r#"{"error":"Access denied due to security violation"}"#.to_string(),
            include_details: false,
        }
    }
}

/// Axumミドルウェア統合用のヘルパー関数
pub async fn session_security_middleware<B>(
    State(middleware): State<Arc<SessionSecurityMiddleware>>,
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, axum::response::Response> {
    middleware.process_http_request(request, next).await
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_middleware_config_default() {
        let config = SessionSecurityMiddlewareConfig::default();
        assert!(!config.require_session);
        assert!(!config.allow_invalid_sessions);
        assert!(config.auto_create_sessions);
        assert_eq!(config.skip_validation_paths.len(), 3);
    }
    
    #[test]
    fn test_violation_response_config_default() {
        let config = ViolationResponseConfig::default();
        assert_eq!(config.block_status_code, 403);
        assert!(!config.include_details);
    }
}