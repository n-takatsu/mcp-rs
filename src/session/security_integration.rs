use crate::security::{
    AuditCategory, AuditLevel, AuditLogEntry, AuditLogger, InputValidator, RateLimiter,
    ValidationResult as SecurityValidationResult, XssProtector, SqlInjectionProtector,
};
use crate::session::{
    Session, SessionId, SessionError, SessionManager, SessionState, SecurityLevel,
    types::{SessionSecurity, SessionMetadata}
};
use crate::policy_validation::PolicyValidationEngine;
use crate::plugin_isolation::security_validation::SecurityValidationSystem;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn, instrument};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// セッションセキュリティ統合システム
/// 
/// 既存の6層セキュリティアーキテクチャとセッション管理システムを統合し、
/// セッションレベルでのセキュリティ監視・制御・監査を提供
#[derive(Debug)]
pub struct SessionSecurityIntegration {
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// 監査ログ
    audit_logger: Arc<AuditLogger>,
    /// 入力検証
    input_validator: Arc<InputValidator>,
    /// レート制限
    rate_limiter: Arc<RateLimiter>,
    /// XSS保護
    xss_protector: Arc<XssProtector>,
    /// SQLインジェクション保護
    sql_protector: Arc<SqlInjectionProtector>,
    /// ポリシー検証エンジン
    policy_engine: Arc<PolicyValidationEngine>,
    /// セキュリティ検証システム
    security_validator: Arc<SecurityValidationSystem>,
    /// セキュリティ設定
    config: SessionSecurityConfig,
}

/// セッションセキュリティ設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSecurityConfig {
    /// セキュリティ違反の最大許可回数
    pub max_security_violations: u32,
    /// セキュリティ違反発生時の自動セッション無効化
    pub auto_invalidate_on_violations: bool,
    /// セッション作成時のセキュリティ検証レベル
    pub creation_security_level: SecurityValidationLevel,
    /// セキュリティ監視間隔
    pub monitoring_interval: Duration,
    /// セキュリティイベント保持期間
    pub security_event_retention: Duration,
    /// IP地理情報トラッキング
    pub enable_geo_tracking: bool,
    /// 異常行動検出
    pub enable_anomaly_detection: bool,
    /// セッション継続時セキュリティ再検証間隔
    pub revalidation_interval: Duration,
}

/// セキュリティ検証レベル
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityValidationLevel {
    /// 基本検証（必須チェックのみ）
    Basic,
    /// 標準検証（推奨セキュリティチェック）
    Standard,
    /// 厳密検証（全セキュリティチェック）
    Strict,
    /// 最大検証（パフォーマンス犠牲、最高セキュリティ）
    Maximum,
}

/// セッションセキュリティイベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSecurityEvent {
    /// イベントID
    pub event_id: Uuid,
    /// セッションID
    pub session_id: SessionId,
    /// イベント種別
    pub event_type: SecurityEventType,
    /// 発生時刻
    pub timestamp: DateTime<Utc>,
    /// 重要度レベル
    pub severity: SecuritySeverity,
    /// 詳細情報
    pub details: serde_json::Value,
    /// IPアドレス
    pub ip_address: Option<IpAddr>,
    /// User-Agent
    pub user_agent: Option<String>,
    /// 対処アクション
    pub action_taken: Option<SecurityAction>,
}

/// セキュリティイベント種別
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SecurityEventType {
    /// セッション作成時のセキュリティ検証
    SessionCreationValidation,
    /// セキュリティ違反検出
    SecurityViolationDetected,
    /// 異常行動検出
    AnomalousActivityDetected,
    /// レート制限違反
    RateLimitViolation,
    /// 入力検証失敗
    InputValidationFailure,
    /// XSS攻撃検出
    XssAttackDetected,
    /// SQLインジェクション検出
    SqlInjectionDetected,
    /// 不正アクセス試行
    UnauthorizedAccess,
    /// セッション乗っ取り疑惑
    SessionHijackingSuspected,
    /// 地理的異常アクセス
    GeographicalAnomaly,
    /// セキュリティポリシー違反
    PolicyViolation,
}

/// セキュリティ重要度
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Ord, PartialOrd, Eq)]
pub enum SecuritySeverity {
    /// 情報（記録のみ）
    Info,
    /// 警告（注意が必要）
    Warning,
    /// エラー（対処が必要）
    Error,
    /// 重大（即座の対処が必要）
    Critical,
    /// 緊急（システム停止レベル）
    Emergency,
}

/// セキュリティアクション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityAction {
    /// ログ記録のみ
    LogOnly,
    /// 警告発行
    IssueWarning,
    /// セッション一時停止
    SuspendSession,
    /// セッション無効化
    InvalidateSession,
    /// IPアドレスブロック
    BlockIpAddress,
    /// ユーザー全セッション無効化
    InvalidateUserSessions,
    /// 管理者通知
    NotifyAdministrator,
    /// 自動セキュリティ対応実行
    TriggerAutomatedResponse,
}

/// セッション異常検出結果
#[derive(Debug, Clone)]
pub struct SessionAnomalyDetectionResult {
    /// 異常が検出されたか
    pub anomaly_detected: bool,
    /// 異常スコア（0.0-1.0）
    pub anomaly_score: f64,
    /// 検出された異常の種類
    pub anomaly_types: Vec<AnomalyType>,
    /// 推奨アクション
    pub recommended_action: SecurityAction,
    /// 詳細情報
    pub details: serde_json::Value,
}

/// 異常種別
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyType {
    /// 地理的異常（通常と大きく異なる地域からのアクセス）
    GeographicalAnomaly,
    /// 時間的異常（通常と大きく異なる時間帯のアクセス）
    TemporalAnomaly,
    /// 行動異常（通常と異なるアクセスパターン）
    BehavioralAnomaly,
    /// 頻度異常（異常に高い/低い活動頻度）
    FrequencyAnomaly,
    /// デバイス異常（通常と異なるデバイス特性）
    DeviceAnomaly,
    /// ネットワーク異常（疑わしいネットワークからのアクセス）
    NetworkAnomaly,
}

impl SessionSecurityIntegration {
    /// 新しいセッションセキュリティ統合システムを作成
    pub fn new(
        session_manager: Arc<SessionManager>,
        audit_logger: Arc<AuditLogger>,
        input_validator: Arc<InputValidator>,
        rate_limiter: Arc<RateLimiter>,
        xss_protector: Arc<XssProtector>,
        sql_protector: Arc<SqlInjectionProtector>,
        policy_engine: Arc<PolicyValidationEngine>,
        security_validator: Arc<SecurityValidationSystem>,
        config: SessionSecurityConfig,
    ) -> Self {
        Self {
            session_manager,
            audit_logger,
            input_validator,
            rate_limiter,
            xss_protector,
            sql_protector,
            policy_engine,
            security_validator,
            config,
        }
    }
    
    /// セッション作成時のセキュリティ検証
    #[instrument(skip(self), fields(ip_address = ?ip_address, user_agent = %user_agent.as_deref().unwrap_or("unknown")))]
    pub async fn validate_session_creation(
        &self,
        user_id: Option<&str>,
        ip_address: Option<IpAddr>,
        user_agent: Option<&str>,
    ) -> Result<SessionSecurityValidationResult, SessionError> {
        let start_time = Instant::now();
        
        debug!("セッション作成時のセキュリティ検証を開始");
        
        let mut validation_result = SessionSecurityValidationResult::new();
        
        // 1. レート制限チェック
        if let Some(ip) = ip_address {
            let rate_limit_key = format!("session_creation:{}", ip);
            if !self.rate_limiter.check_rate_limit(&rate_limit_key).await? {
                self.record_security_event(
                    None,
                    SecurityEventType::RateLimitViolation,
                    SecuritySeverity::Warning,
                    serde_json::json!({
                        "ip_address": ip.to_string(),
                        "limit_type": "session_creation"
                    }),
                    ip_address,
                    user_agent,
                    Some(SecurityAction::IssueWarning),
                ).await?;
                
                validation_result.add_violation("Rate limit exceeded for session creation");
                return Ok(validation_result);
            }
        }
        
        // 2. IP地理情報チェック（設定が有効な場合）
        if self.config.enable_geo_tracking {
            if let Some(geo_result) = self.check_geographical_anomaly(user_id, ip_address).await? {
                if geo_result.anomaly_detected {
                    validation_result.add_warning("Geographical anomaly detected");
                    
                    self.record_security_event(
                        None,
                        SecurityEventType::GeographicalAnomaly,
                        SecuritySeverity::Warning,
                        serde_json::json!(geo_result.details),
                        ip_address,
                        user_agent,
                        Some(geo_result.recommended_action),
                    ).await?;
                }
            }
        }
        
        // 3. User-Agent検証
        if let Some(ua) = user_agent {
            let ua_validation = self.input_validator.validate_user_agent(ua).await;
            if !ua_validation.is_valid {
                validation_result.add_violation("Invalid or suspicious User-Agent detected");
                
                self.record_security_event(
                    None,
                    SecurityEventType::InputValidationFailure,
                    SecuritySeverity::Warning,
                    serde_json::json!({
                        "user_agent": ua,
                        "validation_errors": ua_validation.errors
                    }),
                    ip_address,
                    user_agent,
                    Some(SecurityAction::IssueWarning),
                ).await?;
            }
        }
        
        // 4. 検証レベルに応じた追加チェック
        match self.config.creation_security_level {
            SecurityValidationLevel::Standard | SecurityValidationLevel::Strict | SecurityValidationLevel::Maximum => {
                // ネットワーク脅威インテリジェンスチェック
                if let Some(ip) = ip_address {
                    if self.is_known_malicious_ip(ip).await? {
                        validation_result.add_violation("IP address is on threat intelligence blacklist");
                        
                        self.record_security_event(
                            None,
                            SecurityEventType::UnauthorizedAccess,
                            SecuritySeverity::Critical,
                            serde_json::json!({
                                "ip_address": ip.to_string(),
                                "threat_source": "threat_intelligence"
                            }),
                            ip_address,
                            user_agent,
                            Some(SecurityAction::BlockIpAddress),
                        ).await?;
                    }
                }
            }
            _ => {}
        }
        
        let validation_duration = start_time.elapsed();
        
        // 監査ログに記録
        self.audit_logger.log(AuditLogEntry {
            timestamp: Utc::now(),
            level: if validation_result.has_violations() { 
                AuditLevel::Warning 
            } else { 
                AuditLevel::Info 
            },
            category: AuditCategory::Security,
            message: "Session creation security validation completed".to_string(),
            details: Some(serde_json::json!({
                "user_id": user_id,
                "ip_address": ip_address.map(|ip| ip.to_string()),
                "validation_duration_ms": validation_duration.as_millis(),
                "violations_count": validation_result.violations.len(),
                "warnings_count": validation_result.warnings.len()
            })),
        }).await;
        
        info!(
            "セッション作成セキュリティ検証完了: duration={}ms, violations={}, warnings={}",
            validation_duration.as_millis(),
            validation_result.violations.len(),
            validation_result.warnings.len()
        );
        
        Ok(validation_result)
    }
    
    /// セッションのセキュリティ違反を記録
    #[instrument(skip(self, details))]
    pub async fn record_session_violation(
        &self,
        session_id: &SessionId,
        violation_type: SecurityEventType,
        severity: SecuritySeverity,
        details: serde_json::Value,
    ) -> Result<(), SessionError> {
        debug!("セッション違反を記録: {:?}", violation_type);
        
        // セッション情報を取得
        let session = self.session_manager.get_session(session_id).await?
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;
        
        // セキュリティ違反カウントを更新
        let mut updated_session = session.clone();
        updated_session.security.security_violations += 1;
        
        // 自動アクション判定
        let action = self.determine_security_action(&updated_session, &violation_type, &severity);
        
        // セキュリティイベントを記録
        self.record_security_event(
            Some(session_id.clone()),
            violation_type,
            severity,
            details,
            updated_session.metadata.ip_address,
            updated_session.metadata.user_agent.as_deref(),
            Some(action.clone()),
        ).await?;
        
        // アクション実行
        match action {
            SecurityAction::SuspendSession => {
                updated_session.state = SessionState::Suspended;
                warn!("セッション一時停止: {}", session_id);
            }
            SecurityAction::InvalidateSession => {
                self.session_manager.invalidate_session(session_id).await?;
                warn!("セッション無効化: {}", session_id);
                return Ok(());
            }
            SecurityAction::InvalidateUserSessions => {
                if let Some(user_id) = &session.user_id {
                    self.session_manager.invalidate_user_sessions(user_id).await?;
                    warn!("ユーザー全セッション無効化: {}", user_id);
                }
                return Ok(());
            }
            _ => {}
        }
        
        // セッション更新
        self.session_manager.update_session(&updated_session).await?;
        
        Ok(())
    }
    
    /// セッション異常検出
    #[instrument(skip(self))]
    pub async fn detect_session_anomalies(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<SessionAnomalyDetectionResult>, SessionError> {
        if !self.config.enable_anomaly_detection {
            return Ok(None);
        }
        
        debug!("セッション異常検出を実行: {}", session_id);
        
        let session = self.session_manager.get_session(session_id).await?
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;
        
        let mut anomalies = Vec::new();
        let mut max_score = 0.0;
        
        // 地理的異常チェック
        if let Some(geo_result) = self.check_geographical_anomaly(
            session.user_id.as_deref(),
            session.metadata.ip_address,
        ).await? {
            if geo_result.anomaly_detected {
                anomalies.push(AnomalyType::GeographicalAnomaly);
                max_score = max_score.max(geo_result.anomaly_score);
            }
        }
        
        // 行動異常チェック
        if let Some(behavioral_score) = self.check_behavioral_anomaly(&session).await? {
            if behavioral_score > 0.7 {
                anomalies.push(AnomalyType::BehavioralAnomaly);
                max_score = max_score.max(behavioral_score);
            }
        }
        
        // 頻度異常チェック
        if let Some(frequency_score) = self.check_frequency_anomaly(&session).await? {
            if frequency_score > 0.8 {
                anomalies.push(AnomalyType::FrequencyAnomaly);
                max_score = max_score.max(frequency_score);
            }
        }
        
        if anomalies.is_empty() {
            return Ok(None);
        }
        
        let recommended_action = match max_score {
            s if s >= 0.9 => SecurityAction::InvalidateSession,
            s if s >= 0.7 => SecurityAction::SuspendSession,
            s if s >= 0.5 => SecurityAction::IssueWarning,
            _ => SecurityAction::LogOnly,
        };
        
        let result = SessionAnomalyDetectionResult {
            anomaly_detected: true,
            anomaly_score: max_score,
            anomaly_types: anomalies,
            recommended_action,
            details: serde_json::json!({
                "session_id": session_id.as_str(),
                "detection_timestamp": Utc::now(),
                "max_anomaly_score": max_score
            }),
        };
        
        Ok(Some(result))
    }
    
    /// セッション入力検証
    pub async fn validate_session_input(
        &self,
        session_id: &SessionId,
        input_data: &str,
        input_type: &str,
    ) -> Result<SecurityValidationResult, SessionError> {
        debug!("セッション入力検証: type={}", input_type);
        
        // 基本入力検証
        let mut validation_result = self.input_validator.validate_input(input_data, input_type).await;
        
        // XSS検証
        let xss_result = self.xss_protector.analyze_content(input_data).await?;
        if xss_result.threat_detected {
            validation_result.errors.push(format!("XSS attack detected: {:?}", xss_result.attack_types));
            
            self.record_session_violation(
                session_id,
                SecurityEventType::XssAttackDetected,
                SecuritySeverity::Critical,
                serde_json::json!({
                    "xss_analysis": xss_result,
                    "input_type": input_type
                }),
            ).await?;
        }
        
        // SQL インジェクション検証
        let sql_result = self.sql_protector.analyze_query(input_data).await?;
        if sql_result.threat_detected {
            validation_result.errors.push(format!("SQL injection detected: {:?}", sql_result.attack_patterns));
            
            self.record_session_violation(
                session_id,
                SecurityEventType::SqlInjectionDetected,
                SecuritySeverity::Critical,
                serde_json::json!({
                    "sql_analysis": sql_result,
                    "input_type": input_type
                }),
            ).await?;
        }
        
        Ok(validation_result)
    }
    
    /// セキュリティイベント記録（内部用）
    async fn record_security_event(
        &self,
        session_id: Option<SessionId>,
        event_type: SecurityEventType,
        severity: SecuritySeverity,
        details: serde_json::Value,
        ip_address: Option<IpAddr>,
        user_agent: Option<&str>,
        action_taken: Option<SecurityAction>,
    ) -> Result<(), SessionError> {
        let event = SessionSecurityEvent {
            event_id: Uuid::new_v4(),
            session_id: session_id.unwrap_or_else(|| SessionId::new()),
            event_type,
            timestamp: Utc::now(),
            severity,
            details,
            ip_address,
            user_agent: user_agent.map(|s| s.to_string()),
            action_taken,
        };
        
        // 監査ログに記録
        let audit_level = match event.severity {
            SecuritySeverity::Info => AuditLevel::Info,
            SecuritySeverity::Warning => AuditLevel::Warning,
            SecuritySeverity::Error | SecuritySeverity::Critical | SecuritySeverity::Emergency => AuditLevel::Error,
        };
        
        self.audit_logger.log(AuditLogEntry {
            timestamp: event.timestamp,
            level: audit_level,
            category: AuditCategory::Security,
            message: format!("Session security event: {:?}", event.event_type),
            details: Some(serde_json::to_value(&event).unwrap_or_default()),
        }).await;
        
        debug!("セキュリティイベント記録完了: {:?}", event.event_type);
        Ok(())
    }
    
    /// セキュリティアクション決定
    fn determine_security_action(
        &self,
        session: &Session,
        violation_type: &SecurityEventType,
        severity: &SecuritySeverity,
    ) -> SecurityAction {
        // 重要度によるアクション決定
        match severity {
            SecuritySeverity::Emergency | SecuritySeverity::Critical => {
                match violation_type {
                    SecurityEventType::SqlInjectionDetected |
                    SecurityEventType::SessionHijackingSuspected => SecurityAction::InvalidateUserSessions,
                    _ => SecurityAction::InvalidateSession,
                }
            }
            SecuritySeverity::Error => {
                if session.security.security_violations >= self.config.max_security_violations {
                    SecurityAction::InvalidateSession
                } else {
                    SecurityAction::SuspendSession
                }
            }
            SecuritySeverity::Warning => SecurityAction::IssueWarning,
            SecuritySeverity::Info => SecurityAction::LogOnly,
        }
    }
    
    /// 地理的異常チェック（簡略実装）
    async fn check_geographical_anomaly(
        &self,
        _user_id: Option<&str>,
        _ip_address: Option<IpAddr>,
    ) -> Result<Option<SessionAnomalyDetectionResult>, SessionError> {
        // 実装では地理情報APIと連携
        // 現在は簡略化してNoneを返す
        Ok(None)
    }
    
    /// 行動異常チェック（簡略実装）
    async fn check_behavioral_anomaly(&self, _session: &Session) -> Result<Option<f64>, SessionError> {
        // 実装では機械学習モデルや統計的手法で異常検出
        Ok(None)
    }
    
    /// 頻度異常チェック（簡略実装）
    async fn check_frequency_anomaly(&self, _session: &Session) -> Result<Option<f64>, SessionError> {
        // 実装ではリクエスト頻度の統計的分析
        Ok(None)
    }
    
    /// 既知の悪意あるIPチェック（簡略実装）
    async fn is_known_malicious_ip(&self, _ip: IpAddr) -> Result<bool, SessionError> {
        // 実装では脅威インテリジェンスAPIと連携
        Ok(false)
    }
}

/// セッションセキュリティ検証結果
#[derive(Debug, Clone)]
pub struct SessionSecurityValidationResult {
    /// 違反リスト
    pub violations: Vec<String>,
    /// 警告リスト
    pub warnings: Vec<String>,
    /// 追加情報
    pub metadata: serde_json::Value,
}

impl SessionSecurityValidationResult {
    pub fn new() -> Self {
        Self {
            violations: Vec::new(),
            warnings: Vec::new(),
            metadata: serde_json::json!({}),
        }
    }
    
    pub fn add_violation(&mut self, violation: &str) {
        self.violations.push(violation.to_string());
    }
    
    pub fn add_warning(&mut self, warning: &str) {
        self.warnings.push(warning.to_string());
    }
    
    pub fn has_violations(&self) -> bool {
        !self.violations.is_empty()
    }
    
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }
}

impl Default for SessionSecurityConfig {
    fn default() -> Self {
        Self {
            max_security_violations: 5,
            auto_invalidate_on_violations: true,
            creation_security_level: SecurityValidationLevel::Standard,
            monitoring_interval: Duration::from_secs(300), // 5分
            security_event_retention: Duration::from_secs(7776000), // 90日
            enable_geo_tracking: false,
            enable_anomaly_detection: true,
            revalidation_interval: Duration::from_secs(3600), // 1時間
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;
    
    #[tokio::test]
    async fn test_session_security_validation_result() {
        let mut result = SessionSecurityValidationResult::new();
        assert!(result.is_valid());
        assert!(!result.has_violations());
        
        result.add_violation("Test violation");
        assert!(!result.is_valid());
        assert!(result.has_violations());
        
        result.add_warning("Test warning");
        assert_eq!(result.violations.len(), 1);
        assert_eq!(result.warnings.len(), 1);
    }
    
    #[tokio::test]
    async fn test_security_config_default() {
        let config = SessionSecurityConfig::default();
        assert_eq!(config.max_security_violations, 5);
        assert!(config.auto_invalidate_on_violations);
        assert_eq!(config.creation_security_level, SecurityValidationLevel::Standard);
    }
    
    #[tokio::test]
    async fn test_security_action_determination() {
        // テスト実装は実際の統合時に詳細化
        assert!(true);
    }
}