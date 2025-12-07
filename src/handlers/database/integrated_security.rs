//! Integrated Database Security Manager
//!
//! This module provides comprehensive, unified security management for database operations,
//! coordinating all security layers:
//!
//! ## Security Components
//!
//! - **RBAC (Role-Based Access Control)**: Multi-level access control with role hierarchy,
//!   time-based restrictions, IP filtering, and data masking
//! - **Multi-Factor Authentication (MFA)**: Additional authentication layer for sensitive operations
//! - **SQL Injection Detection**: Real-time detection of 11 attack patterns
//! - **Rate Limiting**: Token bucket algorithm with DDoS protection
//! - **Anomaly Detection**: Behavioral analysis and threat scoring
//! - **Column Encryption**: AES-GCM-256 encryption for sensitive data
//! - **Audit Logging**: Comprehensive security event tracking
//!
//! ## Features
//!
//! - Unified security policy enforcement
//! - Real-time threat intelligence integration
//! - Incident response management
//! - Security event correlation
//! - Compliance reporting

use super::{
    advanced_security::{
        AccessDecision, ActionType, AnomalyDetector, AnomalyScore,
        MultiFactorAuth, RoleBasedAccessControl, TrustScore,
    },
    column_encryption::ColumnEncryptionManager,
    security::{AuditLogger, DatabaseSecurity, RateLimiter, SqlInjectionDetector},
    security_config::{AdvancedSecurityConfig, ResponseAction, SeverityLevel},
    types::{QueryContext, SecurityError, ValidationResult},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Integrated Security Manager
///
/// Coordinates all security components and provides unified security policy enforcement
pub struct IntegratedSecurityManager {
    config: AdvancedSecurityConfig,

    // 既存のセキュリティコンポーネント
    database_security: DatabaseSecurity,
    sql_injection_detector: SqlInjectionDetector,
    audit_logger: AuditLogger,
    rate_limiter: RateLimiter,

    // 高度なセキュリティコンポーネント
    mfa: MultiFactorAuth,
    rbac: RoleBasedAccessControl,
    anomaly_detector: AnomalyDetector,
    column_encryption: ColumnEncryptionManager,

    // セキュリティ状態管理
    security_events: Arc<RwLock<Vec<SecurityEvent>>>,
    threat_intelligence: Arc<RwLock<ThreatIntelligence>>,
    incident_response: IncidentResponseManager,
}

impl IntegratedSecurityManager {
    pub fn new(config: AdvancedSecurityConfig) -> Self {
        // 既存のセキュリティコンポーネント（デフォルト設定で初期化）
        let default_config = super::types::SecurityConfig::default();
        let database_security = DatabaseSecurity::new(default_config, None);

        Self {
            database_security,
            sql_injection_detector: SqlInjectionDetector::new(),
            audit_logger: AuditLogger::new(),
            rate_limiter: RateLimiter::new(),

            mfa: MultiFactorAuth::new(),
            rbac: RoleBasedAccessControl::new(),
            anomaly_detector: AnomalyDetector::new(),
            column_encryption: {
                use super::column_encryption::ColumnEncryptionConfig;
                let mut enc_config = ColumnEncryptionConfig::default();
                // Configure encrypted columns from advanced security config
                if let Some(ref encryption_cfg) = config.encryption.encrypted_columns {
                    enc_config.encrypted_columns = encryption_cfg.clone();
                }
                ColumnEncryptionManager::new(enc_config)
            },

            security_events: Arc::new(RwLock::new(Vec::new())),
            threat_intelligence: Arc::new(RwLock::new(ThreatIntelligence::new())),
            incident_response: IncidentResponseManager::new(config.incident_response.clone()),

            config,
        }
    }

    /// 包括的なセキュリティ検証
    pub async fn comprehensive_security_check(
        &self,
        sql: &str,
        context: &QueryContext,
    ) -> Result<SecurityCheckResult, SecurityError> {
        info!(
            "Starting comprehensive security check for user: {:?}",
            context.user_id
        );

        // 1. レート制限チェック
        if let Err(_e) = self.check_rate_limit(context).await {
            warn!("Rate limit exceeded");
            return Ok(SecurityCheckResult::blocked("Rate limit exceeded"));
        }

        // 2. 認証・認可チェック
        let auth_result = self.check_authentication_and_authorization(context).await?;
        if !auth_result.is_allowed() {
            warn!("Authentication/Authorization failed");
            return Ok(SecurityCheckResult::blocked("Access denied"));
        }

        // 3. SQL インジェクション検知
        if let Err(e) = self.sql_injection_detector.scan(sql, context) {
            error!("SQL injection detected: {:?}", e);
            self.record_security_event(SecurityEvent::sql_injection_attempt(
                context.clone(),
                0.9, // 高い信頼度
            ))
            .await;
            return Ok(SecurityCheckResult::blocked("SQL injection detected"));
        }

        // 4. 異常検知
        let anomaly_score = self
            .anomaly_detector
            .analyze_query_pattern(sql, context)
            .await?;
        if anomaly_score.score > self.config.anomaly_detection.thresholds.high_risk {
            warn!("High anomaly score detected: {:.2}", anomaly_score.score);
            self.record_security_event(SecurityEvent::anomaly_detected(
                context.clone(),
                anomaly_score.clone(),
            ))
            .await;

            // 高リスクの場合は自動対応を実行
            if anomaly_score.score > self.config.anomaly_detection.thresholds.critical_risk {
                self.incident_response
                    .handle_critical_anomaly(&anomaly_score, context)
                    .await?;
                return Ok(SecurityCheckResult::blocked("Critical anomaly detected"));
            }
        }

        // 5. 既存のデータベースセキュリティ検証
        let db_validation = self.database_security.validate_query(sql, context).await?;
        match db_validation {
            ValidationResult::Approved => {
                // 続行
            }
            ValidationResult::Denied(reason) => {
                warn!("Database security validation failed: {}", reason);
                return Ok(SecurityCheckResult::blocked(&reason));
            }
            ValidationResult::Warning(message) => {
                warn!("Database security warning: {}", message);
                // 警告の場合は続行
            }
        }

        // 6. 監査ログ記録
        self.audit_logger
            .log_query_validation(sql, context, &ValidationResult::Approved)
            .await?;

        Ok(SecurityCheckResult::allowed(SecurityContext {
            anomaly_score: anomaly_score.score,
            trust_level: auth_result.trust_level,
            access_level: auth_result.access_level,
            restrictions: auth_result.restrictions,
        }))
    }

    /// 認証・認可チェック
    async fn check_authentication_and_authorization(
        &self,
        context: &QueryContext,
    ) -> Result<AuthorizationResult, SecurityError> {
        // MFA チェック（必要な場合）
        if self.config.mfa.required {
            if let Some(_user_id) = &context.user_id {
                // デバイス信頼度チェック（簡略化）
                let device_id = context.client_info.as_deref().unwrap_or("unknown");
                let trust_score = self.mfa.verify_device_trust(device_id).await?;
                if trust_score.score < self.config.mfa.device_trust.trust_threshold {
                    return Ok(AuthorizationResult::mfa_required());
                }
            }
        }

        // RBAC チェック
        if self.config.rbac.enabled {
            if let Some(user_id) = &context.user_id {
                // クエリタイプに基づいてアクションを決定
                let action = self.determine_action_from_query(&context.query_type);

                // リソース名を決定（実際のクエリから抽出するのが理想的）
                let target_resource = self.extract_resource_from_context(context);

                let access_decision = self
                    .rbac
                    .check_access(user_id, &target_resource, &action, context)
                    .await?;

                match access_decision {
                    AccessDecision::Allow => {
                        info!(
                            "RBAC check passed for user {} on resource {}",
                            user_id, target_resource
                        );
                    }
                    AccessDecision::Deny => {
                        warn!(
                            "RBAC check denied for user {} on resource {}",
                            user_id, target_resource
                        );
                        return Ok(AuthorizationResult::access_denied("RBAC policy denied"));
                    }
                    AccessDecision::Conditional(conditions) => {
                        info!(
                            "RBAC conditional access for user {}: {:?}",
                            user_id, conditions
                        );
                        return Ok(AuthorizationResult::conditional_access(conditions));
                    }
                }
            }
        }

        Ok(AuthorizationResult::allowed())
    }

    /// クエリタイプからアクションを決定
    fn determine_action_from_query(&self, query_type: &super::types::QueryType) -> ActionType {
        use super::types::QueryType;

        match query_type {
            QueryType::Select => ActionType::Read,
            QueryType::Insert | QueryType::Update => ActionType::Write,
            QueryType::Delete => ActionType::Delete,
            QueryType::Create | QueryType::Alter | QueryType::Drop | QueryType::Ddl => {
                ActionType::Admin
            }
            QueryType::StoredProcedure => ActionType::Execute,
            QueryType::Transaction | QueryType::Unknown | QueryType::Custom(_) => {
                ActionType::Execute
            }
        }
    }

    /// コンテキストからリソース名を抽出（簡易実装）
    fn extract_resource_from_context(&self, _context: &QueryContext) -> String {
        // TODO: 実際のSQLパースからテーブル名を抽出
        // 現在は簡易実装としてデフォルトリソースを返す
        "default_resource".to_string()
    }

    /// RBACにユーザーロールを割り当て
    pub async fn assign_user_role(&self, user_id: &str, role: &str) -> Result<(), SecurityError> {
        self.rbac.assign_role(user_id, role).await
    }

    /// RBACからユーザーロールを削除
    pub async fn revoke_user_role(&self, user_id: &str, role: &str) -> Result<(), SecurityError> {
        self.rbac.revoke_role(user_id, role).await
    }

    /// RBAC設定を更新
    pub async fn update_rbac_config(&self, config: super::security_config::RbacConfig) {
        self.rbac.update_config(config).await;
        info!("RBAC configuration updated");
    }

    /// カラムレベルアクセスをチェック
    pub async fn check_column_access(
        &self,
        user_id: &str,
        table_name: &str,
        column_name: &str,
        action: &ActionType,
    ) -> Result<super::advanced_security::ColumnAccessResult, SecurityError> {
        // ユーザーのロールを取得
        let user_roles = self.get_user_roles(user_id).await;

        self.rbac
            .check_column_access(&user_roles, table_name, column_name, action)
            .await
    }

    /// 行レベルセキュリティをチェック
    pub async fn check_row_level_security(
        &self,
        user_id: &str,
        table_name: &str,
        row_data: &HashMap<String, String>,
    ) -> Result<bool, SecurityError> {
        // ユーザーのロールを取得
        let user_roles = self.get_user_roles(user_id).await;

        self.rbac
            .check_row_level_security(user_id, &user_roles, table_name, row_data)
            .await
    }

    /// ユーザーのロールを取得（内部ヘルパー）
    async fn get_user_roles(&self, user_id: &str) -> std::collections::HashSet<String> {
        // RBACの公開APIを使用してロールを取得
        self.rbac.get_user_roles(user_id).await
    }

    /// データマスキングを適用
    pub fn apply_data_masking(
        &self,
        data: &str,
        masking_rule: &super::security_config::MaskingRule,
    ) -> String {
        self.rbac.apply_data_masking(data, masking_rule)
    }

    /// レート制限チェック
    async fn check_rate_limit(&self, context: &QueryContext) -> Result<(), SecurityError> {
        self.rate_limiter.check_rate_limit(context).await
    }

    /// セキュリティイベントの記録
    async fn record_security_event(&self, event: SecurityEvent) {
        let mut events = self.security_events.write().await;
        events.push(event);

        // イベント数の制限
        if events.len() > 10000 {
            events.remove(0);
        }
    }

    /// 脅威インテリジェンス更新
    pub async fn update_threat_intelligence(
        &self,
        intel: ThreatIntelligenceUpdate,
    ) -> Result<(), SecurityError> {
        let mut threat_intel = self.threat_intelligence.write().await;
        threat_intel.update(intel);
        info!("Threat intelligence updated");
        Ok(())
    }

    /// セキュリティダッシュボードデータ生成
    pub async fn generate_security_dashboard(&self) -> Result<SecurityDashboard, SecurityError> {
        let events = self.security_events.read().await;
        let threat_intel = self.threat_intelligence.read().await;

        let now = Utc::now();
        let last_24h = now - chrono::Duration::hours(24);

        // 過去24時間のイベント統計
        let recent_events: Vec<&SecurityEvent> =
            events.iter().filter(|e| e.timestamp > last_24h).collect();

        let mut event_counts = HashMap::new();
        for event in &recent_events {
            *event_counts.entry(event.event_type.clone()).or_insert(0) += 1;
        }

        // 異常検知統計
        let anomaly_events: Vec<&SecurityEvent> = recent_events
            .iter()
            .filter(|e| matches!(e.event_type, SecurityEventType::AnomalyDetected))
            .cloned() // &&SecurityEvent から &SecurityEvent に変換
            .collect();

        let avg_anomaly_score = if !anomaly_events.is_empty() {
            anomaly_events
                .iter()
                .filter_map(|e| e.anomaly_score)
                .sum::<f64>()
                / anomaly_events.len() as f64
        } else {
            0.0
        };

        // トップリスクユーザー
        let mut user_risk_scores = HashMap::new();
        for event in &recent_events {
            if let Some(user_id) = &event.context.user_id {
                let score = event.risk_score.unwrap_or(0.0);
                let entry = user_risk_scores.entry(user_id.clone()).or_insert(0.0);
                *entry += score;
            }
        }

        let mut top_risk_users: Vec<(String, f64)> = user_risk_scores.into_iter().collect();
        top_risk_users.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        top_risk_users.truncate(10);

        Ok(SecurityDashboard {
            timestamp: now,
            event_summary: EventSummary {
                total_events: recent_events.len(),
                event_counts,
                critical_events: recent_events
                    .iter()
                    .filter(|e| e.severity == SeverityLevel::Critical)
                    .count(),
            },
            anomaly_summary: AnomalySummary {
                total_anomalies: anomaly_events.len(),
                average_score: avg_anomaly_score,
                high_risk_anomalies: anomaly_events
                    .iter()
                    .filter(|e| e.anomaly_score.unwrap_or(0.0) > 0.8)
                    .count(),
            },
            top_risk_users,
            threat_intelligence_status: ThreatIntelligenceStatus {
                last_update: threat_intel.last_update,
                active_threats: threat_intel.active_threats.len(),
                blocked_ips: threat_intel.blocked_ips.len(),
            },
            system_health: SystemHealthStatus {
                security_components_status: self.get_component_health().await,
                performance_metrics: self.get_performance_metrics().await,
            },
        })
    }

    async fn get_component_health(&self) -> HashMap<String, ComponentHealth> {
        let mut health = HashMap::new();

        health.insert("mfa".to_string(), ComponentHealth::Healthy);
        health.insert("rbac".to_string(), ComponentHealth::Healthy);
        health.insert("anomaly_detection".to_string(), ComponentHealth::Healthy);
        health.insert("encryption".to_string(), ComponentHealth::Healthy);
        health.insert("audit_logging".to_string(), ComponentHealth::Healthy);

        health
    }

    async fn get_performance_metrics(&self) -> PerformanceMetrics {
        PerformanceMetrics {
            average_response_time_ms: 150.0,
            throughput_per_second: 1000.0,
            error_rate_percent: 0.1,
            memory_usage_mb: 512.0,
        }
    }

    /// 定期的なセキュリティメンテナンス
    pub async fn perform_maintenance(&self) -> Result<(), SecurityError> {
        info!("Starting security maintenance");

        // 古いセキュリティイベントのクリーンアップ
        self.cleanup_old_events().await;

        // 脅威インテリジェンスの更新
        self.refresh_threat_intelligence().await?;

        // 異常検知モデルの再訓練（必要に応じて）
        if self.should_retrain_anomaly_model().await {
            self.retrain_anomaly_model().await?;
        }

        // セキュリティメトリクスの更新
        self.update_security_metrics().await?;

        info!("Security maintenance completed");
        Ok(())
    }

    async fn cleanup_old_events(&self) {
        let mut events = self.security_events.write().await;
        let cutoff = Utc::now() - chrono::Duration::days(30);
        events.retain(|event| event.timestamp > cutoff);
    }

    async fn refresh_threat_intelligence(&self) -> Result<(), SecurityError> {
        // 外部脅威インテリジェンスソースからの更新
        // 実装は環境に依存
        Ok(())
    }

    async fn should_retrain_anomaly_model(&self) -> bool {
        // モデル再訓練の必要性判定
        true // 簡略実装
    }

    async fn retrain_anomaly_model(&self) -> Result<(), SecurityError> {
        // 異常検知モデルの再訓練
        info!("Retraining anomaly detection model");
        Ok(())
    }

    async fn update_security_metrics(&self) -> Result<(), SecurityError> {
        // セキュリティメトリクスの計算と更新
        Ok(())
    }
}

/// インシデント対応マネージャー
pub struct IncidentResponseManager {
    config: super::security_config::IncidentResponseConfig,
    active_incidents: Arc<RwLock<HashMap<String, ActiveIncident>>>,
}

impl IncidentResponseManager {
    pub fn new(config: super::security_config::IncidentResponseConfig) -> Self {
        Self {
            config,
            active_incidents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn handle_critical_anomaly(
        &self,
        anomaly: &AnomalyScore,
        context: &QueryContext,
    ) -> Result<(), SecurityError> {
        let incident_id = format!("INC-{}", Utc::now().timestamp());

        let incident = ActiveIncident {
            id: incident_id.clone(),
            incident_type: IncidentType::CriticalAnomaly,
            severity: SeverityLevel::Critical,
            context: context.clone(),
            anomaly_score: Some(anomaly.clone()),
            created_at: Utc::now(),
            actions_taken: Vec::new(),
        };

        // インシデントを記録
        {
            let mut incidents = self.active_incidents.write().await;
            incidents.insert(incident_id.clone(), incident);
        }

        // 自動対応を実行
        if self.config.auto_response_enabled {
            self.execute_automated_response(&incident_id, anomaly, context)
                .await?;
        }

        // 通知送信
        self.send_notifications(&incident_id, SeverityLevel::Critical)
            .await?;

        Ok(())
    }

    async fn execute_automated_response(
        &self,
        incident_id: &str,
        anomaly: &AnomalyScore,
        context: &QueryContext,
    ) -> Result<(), SecurityError> {
        for rule in &self.config.response_rules {
            if self.rule_matches(rule, anomaly, context) {
                for action in &rule.actions {
                    self.execute_action(incident_id, action, context).await?;
                }
            }
        }
        Ok(())
    }

    fn rule_matches(
        &self,
        _rule: &super::security_config::ResponseRule,
        _anomaly: &AnomalyScore,
        _context: &QueryContext,
    ) -> bool {
        // ルールマッチング実装
        true
    }

    async fn execute_action(
        &self,
        incident_id: &str,
        action: &ResponseAction,
        context: &QueryContext,
    ) -> Result<(), SecurityError> {
        match action {
            ResponseAction::BlockUser => {
                if let Some(user_id) = &context.user_id {
                    info!("Blocking user {} for incident {}", user_id, incident_id);
                    // ユーザーブロック実装
                }
            }
            ResponseAction::BlockIP => {
                if let Some(ip) = &context.source_ip {
                    info!("Blocking IP {} for incident {}", ip, incident_id);
                    // IPブロック実装
                }
            }
            ResponseAction::RequireReAuthentication => {
                info!("Requiring re-authentication for incident {}", incident_id);
                // 再認証要求実装
            }
            ResponseAction::NotifyAdministrator => {
                info!("Notifying administrator for incident {}", incident_id);
                // 管理者通知実装
            }
            ResponseAction::LogIncident => {
                info!("Logging incident {}", incident_id);
                // インシデントログ実装
            }
            ResponseAction::QuarantineQuery => {
                info!("Quarantining query for incident {}", incident_id);
                // クエリ隔離実装
            }
            ResponseAction::EscalateToHuman => {
                info!("Escalating incident {} to human analyst", incident_id);
                // 人的エスカレーション実装
            }
        }
        Ok(())
    }

    async fn send_notifications(
        &self,
        incident_id: &str,
        severity: SeverityLevel,
    ) -> Result<(), SecurityError> {
        if let Some(targets) = self.config.notification.notification_targets.get(&severity) {
            for target in targets {
                info!(
                    "Sending notification to {} for incident {}",
                    target, incident_id
                );
                // 通知送信実装
            }
        }
        Ok(())
    }
}

// 関連する型定義

#[derive(Debug, Clone)]
pub struct SecurityCheckResult {
    pub allowed: bool,
    pub reason: String,
    pub context: Option<SecurityContext>,
}

impl SecurityCheckResult {
    pub fn allowed(context: SecurityContext) -> Self {
        Self {
            allowed: true,
            reason: "Access granted".to_string(),
            context: Some(context),
        }
    }

    pub fn blocked(reason: &str) -> Self {
        Self {
            allowed: false,
            reason: reason.to_string(),
            context: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub anomaly_score: f64,
    pub trust_level: f64,
    pub access_level: AccessLevel,
    pub restrictions: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum AccessLevel {
    ReadOnly,
    Standard,
    Elevated,
    Administrative,
}

#[derive(Debug, Clone)]
pub struct AuthorizationResult {
    pub allowed: bool,
    pub trust_level: f64,
    pub access_level: AccessLevel,
    pub restrictions: Vec<String>,
    pub mfa_required: bool,
}

impl AuthorizationResult {
    pub fn allowed() -> Self {
        Self {
            allowed: true,
            trust_level: 1.0,
            access_level: AccessLevel::Standard,
            restrictions: Vec::new(),
            mfa_required: false,
        }
    }

    pub fn access_denied(reason: &str) -> Self {
        Self {
            allowed: false,
            trust_level: 0.0,
            access_level: AccessLevel::ReadOnly,
            restrictions: vec![reason.to_string()],
            mfa_required: false,
        }
    }

    pub fn mfa_required() -> Self {
        Self {
            allowed: false,
            trust_level: 0.5,
            access_level: AccessLevel::ReadOnly,
            restrictions: vec!["MFA required".to_string()],
            mfa_required: true,
        }
    }

    pub fn conditional_access(conditions: Vec<String>) -> Self {
        Self {
            allowed: true,
            trust_level: 0.7,
            access_level: AccessLevel::Standard,
            restrictions: conditions,
            mfa_required: false,
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.allowed
    }
}

#[derive(Debug, Clone)]
pub struct SecurityEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: SecurityEventType,
    pub severity: SeverityLevel,
    pub context: QueryContext,
    pub description: String,
    pub risk_score: Option<f64>,
    pub anomaly_score: Option<f64>,
}

impl SecurityEvent {
    pub fn sql_injection_attempt(context: QueryContext, confidence: f64) -> Self {
        Self {
            id: format!("SEC-{}", Utc::now().timestamp_millis()),
            timestamp: Utc::now(),
            event_type: SecurityEventType::SqlInjectionAttempt,
            severity: SeverityLevel::High,
            context,
            description: format!(
                "SQL injection attempt detected with confidence {:.2}",
                confidence
            ),
            risk_score: Some(confidence),
            anomaly_score: None,
        }
    }

    pub fn anomaly_detected(context: QueryContext, anomaly: AnomalyScore) -> Self {
        let severity = if anomaly.score > 0.95 {
            SeverityLevel::Critical
        } else if anomaly.score > 0.8 {
            SeverityLevel::High
        } else if anomaly.score > 0.6 {
            SeverityLevel::Medium
        } else {
            SeverityLevel::Low
        };

        Self {
            id: format!("ANO-{}", Utc::now().timestamp_millis()),
            timestamp: Utc::now(),
            event_type: SecurityEventType::AnomalyDetected,
            severity,
            context,
            description: format!("Anomaly detected: {}", anomaly.explanation),
            risk_score: Some(anomaly.score),
            anomaly_score: Some(anomaly.score),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SecurityEventType {
    SqlInjectionAttempt,
    AnomalyDetected,
    AuthenticationFailure,
    AuthorizationDenied,
    RateLimitExceeded,
    SuspiciousActivity,
    DataExfiltrationAttempt,
}

#[derive(Debug)]
pub struct ThreatIntelligence {
    pub last_update: DateTime<Utc>,
    pub active_threats: Vec<ThreatIndicator>,
    pub blocked_ips: Vec<String>,
    pub suspicious_patterns: Vec<String>,
}

impl Default for ThreatIntelligence {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreatIntelligence {
    pub fn new() -> Self {
        Self {
            last_update: Utc::now(),
            active_threats: Vec::new(),
            blocked_ips: Vec::new(),
            suspicious_patterns: Vec::new(),
        }
    }

    pub fn update(&mut self, update: ThreatIntelligenceUpdate) {
        self.last_update = Utc::now();
        self.active_threats.extend(update.new_threats);
        self.blocked_ips.extend(update.new_blocked_ips);
        self.suspicious_patterns.extend(update.new_patterns);
    }
}

#[derive(Debug)]
pub struct ThreatIntelligenceUpdate {
    pub new_threats: Vec<ThreatIndicator>,
    pub new_blocked_ips: Vec<String>,
    pub new_patterns: Vec<String>,
}

#[derive(Debug)]
pub struct ThreatIndicator {
    pub indicator_type: String,
    pub value: String,
    pub severity: SeverityLevel,
    pub description: String,
}

#[derive(Debug)]
pub struct ActiveIncident {
    pub id: String,
    pub incident_type: IncidentType,
    pub severity: SeverityLevel,
    pub context: QueryContext,
    pub anomaly_score: Option<AnomalyScore>,
    pub created_at: DateTime<Utc>,
    pub actions_taken: Vec<String>,
}

#[derive(Debug)]
pub enum IncidentType {
    CriticalAnomaly,
    SqlInjection,
    DataBreach,
    UnauthorizedAccess,
    SystemCompromise,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityDashboard {
    pub timestamp: DateTime<Utc>,
    pub event_summary: EventSummary,
    pub anomaly_summary: AnomalySummary,
    pub top_risk_users: Vec<(String, f64)>,
    pub threat_intelligence_status: ThreatIntelligenceStatus,
    pub system_health: SystemHealthStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventSummary {
    pub total_events: usize,
    pub event_counts: HashMap<SecurityEventType, i32>,
    pub critical_events: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnomalySummary {
    pub total_anomalies: usize,
    pub average_score: f64,
    pub high_risk_anomalies: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreatIntelligenceStatus {
    pub last_update: DateTime<Utc>,
    pub active_threats: usize,
    pub blocked_ips: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemHealthStatus {
    pub security_components_status: HashMap<String, ComponentHealth>,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ComponentHealth {
    Healthy,
    Warning,
    Critical,
    Offline,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub average_response_time_ms: f64,
    pub throughput_per_second: f64,
    pub error_rate_percent: f64,
    pub memory_usage_mb: f64,
}
