//! セッションセキュリティ統合
//!
//! リアルタイム編集システム用の基本セキュリティ機能を提供

use crate::error::SessionError;
use crate::session::{SessionId, SessionManager, SessionState};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

/// セッションセキュリティミドルウェア
#[derive(Debug, Clone)]
pub struct SessionSecurityMiddleware {
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// セキュリティイベント記録
    security_events: Arc<RwLock<Vec<SecurityEvent>>>,
    /// セッション属性追跡（IP、User-Agent等）
    session_attributes: Arc<RwLock<HashMap<SessionId, SessionAttributes>>>,
    /// 設定
    config: SecurityConfig,
}

/// セッション属性（異常検知用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAttributes {
    /// IPアドレス
    pub ip_address: Option<IpAddr>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// 最終更新時刻
    pub last_updated: DateTime<Utc>,
    /// リクエスト回数
    pub request_count: u32,
}

/// セキュリティ設定
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// 最大セキュリティ違反回数
    pub max_violations: u32,
    /// 違反チェック間隔（秒）
    pub violation_check_interval: u64,
    /// セッション検証必須フラグ
    pub require_session_validation: bool,
}

/// セキュリティイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEvent {
    /// イベントID
    pub id: String,
    /// セッションID
    pub session_id: Option<SessionId>,
    /// イベントタイプ
    pub event_type: SecurityEventType,
    /// 深刻度
    pub severity: SecuritySeverity,
    /// メッセージ
    pub message: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// IPアドレス
    pub ip_address: Option<IpAddr>,
}

/// セキュリティイベントタイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityEventType {
    /// セッション認証
    SessionAuth,
    /// 不正アクセス
    UnauthorizedAccess,
    /// レート制限超過
    RateLimitExceeded,
    /// 不正な入力
    InvalidInput,
    /// セッション乗っ取り
    SessionHijacking,
}

/// セキュリティ深刻度
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum SecuritySeverity {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 緊急
    Critical,
}

impl SessionSecurityMiddleware {
    /// 新しいセキュリティミドルウェアを作成
    pub fn new(session_manager: Arc<SessionManager>, config: SecurityConfig) -> Self {
        Self {
            session_manager,
            security_events: Arc::new(RwLock::new(Vec::new())),
            session_attributes: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// セッション検証（SessionManagerと統合）
    #[instrument(skip(self))]
    pub async fn validate_session(
        &self,
        session_id: &SessionId,
        ip_address: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<bool, SessionError> {
        debug!("セッション検証: session_id={}", session_id.as_str());

        // 1. SessionManagerでセッション存在確認
        let session = match self.session_manager.get_session(session_id).await? {
            Some(s) => s,
            None => {
                warn!("セッションが存在しません: {}", session_id.as_str());
                self.log_security_event(SecurityEvent {
                    id: uuid::Uuid::new_v4().to_string(),
                    session_id: Some(session_id.clone()),
                    event_type: SecurityEventType::UnauthorizedAccess,
                    severity: SecuritySeverity::High,
                    message: "セッションが存在しません".to_string(),
                    timestamp: Utc::now(),
                    ip_address,
                })
                .await?;
                return Ok(false);
            }
        };

        // 2. セッション有効性チェック（期限切れ確認）
        if session.is_expired() {
            warn!("セッションが期限切れです: {}", session_id.as_str());
            self.log_security_event(SecurityEvent {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: Some(session_id.clone()),
                event_type: SecurityEventType::SessionAuth,
                severity: SecuritySeverity::Medium,
                message: "セッションが期限切れです".to_string(),
                timestamp: Utc::now(),
                ip_address,
            })
            .await?;
            return Ok(false);
        }

        // 3. セッション状態確認
        if session.state != SessionState::Active {
            warn!(
                "セッションがアクティブではありません: {} (state: {:?})",
                session_id.as_str(),
                session.state
            );
            return Ok(false);
        }

        // 4. 異常検知（IP/User-Agent変更検出）
        if let Err(e) = self
            .detect_session_anomaly(session_id, ip_address, user_agent.clone())
            .await
        {
            error!("セッション異常検知エラー: {:?}", e);
            self.log_security_event(SecurityEvent {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: Some(session_id.clone()),
                event_type: SecurityEventType::SessionHijacking,
                severity: SecuritySeverity::Critical,
                message: format!("セッション乗っ取りの疑い: {:?}", e),
                timestamp: Utc::now(),
                ip_address,
            })
            .await?;
            return Ok(false);
        }

        // 5. セッション活性更新
        self.session_manager.touch_session(session_id).await?;

        // 6. セッション属性更新
        self.update_session_attributes(session_id, ip_address, user_agent)
            .await?;

        Ok(true)
    }

    /// セッション異常検知
    #[instrument(skip(self))]
    async fn detect_session_anomaly(
        &self,
        session_id: &SessionId,
        ip_address: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<(), SessionError> {
        let attributes = self.session_attributes.read().await;

        if let Some(stored_attrs) = attributes.get(session_id) {
            // IP変更検出
            if let (Some(stored_ip), Some(current_ip)) = (&stored_attrs.ip_address, &ip_address) {
                if stored_ip != current_ip {
                    warn!(
                        "IP変更検出: session={}, old={}, new={}",
                        session_id.as_str(),
                        stored_ip,
                        current_ip
                    );
                    return Err(SessionError::SecurityViolation(format!(
                        "IP変更検出: {} -> {}",
                        stored_ip, current_ip
                    )));
                }
            }

            // User-Agent変更検出
            if let (Some(stored_ua), Some(current_ua)) = (&stored_attrs.user_agent, &user_agent) {
                if stored_ua != current_ua {
                    warn!("User-Agent変更検出: session={}", session_id.as_str());
                    return Err(SessionError::SecurityViolation(
                        "User-Agent変更検出".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// セッション属性更新
    #[instrument(skip(self))]
    async fn update_session_attributes(
        &self,
        session_id: &SessionId,
        ip_address: Option<IpAddr>,
        user_agent: Option<String>,
    ) -> Result<(), SessionError> {
        let mut attributes = self.session_attributes.write().await;

        let attrs = attributes
            .entry(session_id.clone())
            .or_insert_with(|| SessionAttributes {
                ip_address,
                user_agent: user_agent.clone(),
                last_updated: Utc::now(),
                request_count: 0,
            });

        attrs.last_updated = Utc::now();
        attrs.request_count += 1;

        // 初回アクセス時のIP/User-Agent記録
        if attrs.ip_address.is_none() {
            attrs.ip_address = ip_address;
        }
        if attrs.user_agent.is_none() {
            attrs.user_agent = user_agent;
        }

        Ok(())
    }

    /// セキュリティイベント記録
    #[instrument(skip(self))]
    pub async fn log_security_event(&self, event: SecurityEvent) -> Result<(), SessionError> {
        info!("セキュリティイベント記録: {:?}", event.event_type);

        let mut events = self.security_events.write().await;
        events.push(event);

        // イベント数制限（メモリ使用量制御）
        if events.len() > 10000 {
            events.drain(0..1000);
        }

        Ok(())
    }

    /// セキュリティ違反チェック
    #[instrument(skip(self))]
    pub async fn check_violations(&self, session_id: &SessionId) -> Result<u32, SessionError> {
        let events = self.security_events.read().await;

        let violation_count = events
            .iter()
            .filter(|e| {
                e.session_id.as_ref() == Some(session_id) && e.severity >= SecuritySeverity::Medium
            })
            .count() as u32;

        debug!(
            "セキュリティ違反チェック: session_id={}, violations={}",
            session_id.as_str(),
            violation_count
        );

        Ok(violation_count)
    }

    /// 入力検証
    #[instrument(skip(self, input))]
    pub async fn validate_input(&self, input: &str, context: &str) -> Result<bool, SessionError> {
        debug!("入力検証: context={}, length={}", context, input.len());

        // 基本的なサニタイゼーション
        if input.len() > 10000 {
            return Ok(false);
        }

        // 危険な文字列のチェック
        let dangerous_patterns = ["<script", "javascript:", "data:", "vbscript:", "on"];
        for pattern in &dangerous_patterns {
            if input.to_lowercase().contains(pattern) {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_violations: 5,
            violation_check_interval: 300, // 5分
            require_session_validation: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_security_middleware_creation() {
        let session_manager = Arc::new(SessionManager::new());
        let config = SecurityConfig::default();
        let middleware = SessionSecurityMiddleware::new(session_manager, config);

        assert_eq!(middleware.config.max_violations, 5);
    }

    #[tokio::test]
    async fn test_session_validation() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let config = SecurityConfig::default();
        let middleware = SessionSecurityMiddleware::new(session_manager.clone(), config);

        // セッション作成
        let session = session_manager
            .create_session("test_user".to_string())
            .await?;
        session_manager.activate_session(&session.id).await?;

        // セッション検証（成功）
        let ip = Some("127.0.0.1".parse().unwrap());
        let user_agent = Some("Test Agent".to_string());
        let result = middleware
            .validate_session(&session.id, ip, user_agent.clone())
            .await?;
        assert!(result);

        // 存在しないセッション検証（失敗）
        let invalid_id = SessionId::new();
        let result = middleware
            .validate_session(&invalid_id, ip, user_agent)
            .await?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_session_anomaly_detection() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let config = SecurityConfig::default();
        let middleware = SessionSecurityMiddleware::new(session_manager.clone(), config);

        // セッション作成とアクティベート
        let session = session_manager
            .create_session("test_user".to_string())
            .await?;
        session_manager.activate_session(&session.id).await?;

        let ip1 = Some("127.0.0.1".parse().unwrap());
        let ip2 = Some("192.168.1.1".parse().unwrap());
        let user_agent = Some("Test Agent".to_string());

        // 初回アクセス
        let result = middleware
            .validate_session(&session.id, ip1, user_agent.clone())
            .await?;
        assert!(result);

        // IP変更検出（異常）
        let result = middleware
            .validate_session(&session.id, ip2, user_agent)
            .await?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_input_validation() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let middleware = SessionSecurityMiddleware::new(session_manager, SecurityConfig::default());

        // 正常な入力
        let result = middleware.validate_input("Hello world", "test").await?;
        assert!(result);

        // 危険な入力
        let result = middleware
            .validate_input("<script>alert('xss')</script>", "test")
            .await?;
        assert!(!result);

        Ok(())
    }

    #[tokio::test]
    async fn test_security_violations() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let config = SecurityConfig::default();
        let middleware = SessionSecurityMiddleware::new(session_manager.clone(), config);

        let session = session_manager
            .create_session("test_user".to_string())
            .await?;

        // セキュリティイベント記録
        middleware
            .log_security_event(SecurityEvent {
                id: uuid::Uuid::new_v4().to_string(),
                session_id: Some(session.id.clone()),
                event_type: SecurityEventType::UnauthorizedAccess,
                severity: SecuritySeverity::High,
                message: "テスト違反".to_string(),
                timestamp: Utc::now(),
                ip_address: None,
            })
            .await?;

        // 違反チェック
        let violations = middleware.check_violations(&session.id).await?;
        assert_eq!(violations, 1);

        Ok(())
    }
}
