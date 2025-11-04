use serde_json::Value;
use std::collections::HashSet;
use tracing::{debug, error, info, warn};

use crate::error::McpError;
use crate::policy_config::{
    AuthenticationPolicyConfig, EncryptionConfig, InputValidationConfig, MonitoringPolicyConfig,
    PolicyConfig, RateLimitingConfig, SecurityPolicyConfig, TlsConfig,
};

/// ポリシー検証エンジン
///
/// ポリシーファイルの構文チェック、論理整合性チェック、
/// セキュリティ要件チェックを包括的に実行する
pub struct PolicyValidationEngine {
    /// 検証ルール設定
    validation_rules: ValidationRules,
    /// 検証統計
    stats: ValidationStats,
}

/// 検証ルール設定
#[derive(Debug, Clone)]
pub struct ValidationRules {
    /// 必須フィールドの検証を有効化
    pub require_mandatory_fields: bool,
    /// セキュリティ要件の厳格な検証
    pub strict_security_validation: bool,
    /// カスタムフィールドの検証
    pub validate_custom_fields: bool,
    /// 値の範囲チェック
    pub validate_value_ranges: bool,
    /// 論理整合性チェック
    pub validate_logical_consistency: bool,
}

/// 検証結果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// 検証が成功したか
    pub is_valid: bool,
    /// 検証レベル
    pub validation_level: ValidationLevel,
    /// エラーリスト
    pub errors: Vec<ValidationError>,
    /// 警告リスト
    pub warnings: Vec<ValidationWarning>,
    /// 推奨事項リスト
    pub recommendations: Vec<ValidationRecommendation>,
    /// 検証実行時間（ミリ秒）
    pub validation_time_ms: u64,
}

/// 検証レベル
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationLevel {
    /// 基本構文チェックのみ
    Basic,
    /// 標準検証（構文 + 論理整合性）
    Standard,
    /// 厳格検証（標準 + セキュリティ要件）
    Strict,
    /// カスタム検証（厳格 + カスタムルール）
    Custom,
}

/// 検証エラー
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// エラーコード
    pub code: String,
    /// エラーメッセージ
    pub message: String,
    /// 発生箇所（パス）
    pub path: String,
    /// 重要度
    pub severity: ErrorSeverity,
}

/// 検証警告
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    /// 警告コード
    pub code: String,
    /// 警告メッセージ
    pub message: String,
    /// 対象箇所
    pub path: String,
}

/// 推奨事項
#[derive(Debug, Clone)]
pub struct ValidationRecommendation {
    /// 推奨コード
    pub code: String,
    /// 推奨内容
    pub message: String,
    /// 対象箇所
    pub path: String,
    /// 改善効果
    pub impact: RecommendationImpact,
}

/// エラーの重要度
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    /// 致命的（ポリシー適用不可）
    Critical,
    /// 高（セキュリティリスクあり）
    High,
    /// 中（機能制限あり）
    Medium,
    /// 低（軽微な問題）
    Low,
}

/// 推奨事項の改善効果
#[derive(Debug, Clone, PartialEq)]
pub enum RecommendationImpact {
    /// セキュリティ向上
    Security,
    /// パフォーマンス向上
    Performance,
    /// 可用性向上
    Availability,
    /// 保守性向上
    Maintainability,
}

/// 検証統計
#[derive(Debug, Clone, Default)]
pub struct ValidationStats {
    /// 総検証回数
    pub total_validations: u64,
    /// 成功回数
    pub successful_validations: u64,
    /// 失敗回数
    pub failed_validations: u64,
    /// 平均検証時間（ミリ秒）
    pub average_validation_time_ms: f64,
    /// 最後の検証時刻
    pub last_validation_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl PolicyValidationEngine {
    /// 新しい検証エンジンを作成
    pub fn new() -> Self {
        Self {
            validation_rules: ValidationRules::default(),
            stats: ValidationStats::default(),
        }
    }

    /// カスタム検証ルールで作成
    pub fn with_rules(rules: ValidationRules) -> Self {
        Self {
            validation_rules: rules,
            stats: ValidationStats::default(),
        }
    }

    /// ポリシー設定を検証
    pub async fn validate_policy(
        &mut self,
        policy: &PolicyConfig,
        level: ValidationLevel,
    ) -> ValidationResult {
        let start_time = std::time::Instant::now();

        info!("ポリシー検証を開始: {} (レベル: {:?})", policy.name, level);

        let mut result = ValidationResult {
            is_valid: true,
            validation_level: level.clone(),
            errors: Vec::new(),
            warnings: Vec::new(),
            recommendations: Vec::new(),
            validation_time_ms: 0,
        };

        // 1. 基本構文チェック
        self.validate_basic_syntax(policy, &mut result).await;

        // 2. 論理整合性チェック（Standard以上）
        if matches!(
            level,
            ValidationLevel::Standard | ValidationLevel::Strict | ValidationLevel::Custom
        ) {
            self.validate_logical_consistency(policy, &mut result).await;
        }

        // 3. セキュリティ要件チェック（Strict以上）
        if matches!(level, ValidationLevel::Strict | ValidationLevel::Custom) {
            self.validate_security_requirements(policy, &mut result)
                .await;
        }

        // 4. カスタムルールチェック（Custom）
        if matches!(level, ValidationLevel::Custom) {
            self.validate_custom_rules(policy, &mut result).await;
        }

        // 5. 推奨事項の生成
        self.generate_recommendations(policy, &mut result).await;

        // 検証結果の最終判定
        result.is_valid = result
            .errors
            .iter()
            .all(|e| e.severity != ErrorSeverity::Critical);

        let duration = start_time.elapsed();
        result.validation_time_ms = duration.as_millis() as u64;

        // 統計更新
        self.update_stats(&result);

        if result.is_valid {
            info!(
                "✅ ポリシー検証成功: {} ({}ms)",
                policy.name, result.validation_time_ms
            );
        } else {
            error!(
                "❌ ポリシー検証失敗: {} - {} 個のエラー",
                policy.name,
                result.errors.len()
            );
        }

        result
    }

    /// 基本構文チェック
    async fn validate_basic_syntax(&self, policy: &PolicyConfig, result: &mut ValidationResult) {
        debug!("基本構文チェックを実行中...");

        // 必須フィールドのチェック
        if policy.id.is_empty() {
            result.errors.push(ValidationError {
                code: "SYNTAX001".to_string(),
                message: "ポリシーIDが空です".to_string(),
                path: "id".to_string(),
                severity: ErrorSeverity::Critical,
            });
        }

        if policy.name.is_empty() {
            result.errors.push(ValidationError {
                code: "SYNTAX002".to_string(),
                message: "ポリシー名が空です".to_string(),
                path: "name".to_string(),
                severity: ErrorSeverity::Critical,
            });
        }

        if policy.version.is_empty() {
            result.errors.push(ValidationError {
                code: "SYNTAX003".to_string(),
                message: "バージョンが空です".to_string(),
                path: "version".to_string(),
                severity: ErrorSeverity::High,
            });
        }

        // バージョン形式のチェック
        if !self.is_valid_version_format(&policy.version) {
            result.warnings.push(ValidationWarning {
                code: "SYNTAX004".to_string(),
                message: "バージョン形式が推奨形式（x.y.z）と異なります".to_string(),
                path: "version".to_string(),
            });
        }

        // 日付フィールドのチェック
        if policy.created_at > policy.updated_at {
            result.errors.push(ValidationError {
                code: "SYNTAX005".to_string(),
                message: "作成日時が更新日時より新しいです".to_string(),
                path: "created_at/updated_at".to_string(),
                severity: ErrorSeverity::Medium,
            });
        }
    }

    /// 論理整合性チェック
    async fn validate_logical_consistency(
        &self,
        policy: &PolicyConfig,
        result: &mut ValidationResult,
    ) {
        debug!("論理整合性チェックを実行中...");

        // セキュリティ設定の整合性
        self.validate_security_consistency(&policy.security, result);

        // 監視設定の整合性
        self.validate_monitoring_consistency(&policy.monitoring, result);

        // 認証設定の整合性
        self.validate_authentication_consistency(&policy.authentication, result);

        // 設定間の相互整合性
        self.validate_cross_section_consistency(policy, result);
    }

    /// セキュリティ要件チェック
    async fn validate_security_requirements(
        &self,
        policy: &PolicyConfig,
        result: &mut ValidationResult,
    ) {
        debug!("セキュリティ要件チェックを実行中...");

        let security = &policy.security;

        // 暗号化要件
        if security.enabled {
            if security.encryption.key_size < 256 {
                result.errors.push(ValidationError {
                    code: "SEC001".to_string(),
                    message: "暗号化キーサイズが最小要件（256ビット）を満たしていません"
                        .to_string(),
                    path: "security.encryption.key_size".to_string(),
                    severity: ErrorSeverity::High,
                });
            }

            if security.encryption.pbkdf2_iterations < 10000 {
                result.warnings.push(ValidationWarning {
                    code: "SEC002".to_string(),
                    message: "PBKDF2反復回数が推奨値（10,000回以上）より少ないです".to_string(),
                    path: "security.encryption.pbkdf2_iterations".to_string(),
                });
            }
        }

        // TLS要件
        if security.tls.enforce
            && security.tls.min_version != "TLSv1.2"
            && security.tls.min_version != "TLSv1.3"
        {
            result.errors.push(ValidationError {
                code: "SEC003".to_string(),
                message: "TLS最小バージョンが安全な値（TLSv1.2以上）ではありません".to_string(),
                path: "security.tls.min_version".to_string(),
                severity: ErrorSeverity::High,
            });
        }

        // レート制限要件
        if security.rate_limiting.enabled {
            if security.rate_limiting.requests_per_minute > 1000 {
                result.warnings.push(ValidationWarning {
                    code: "SEC004".to_string(),
                    message: "レート制限が非常に緩い設定です（1000 req/min超）".to_string(),
                    path: "security.rate_limiting.requests_per_minute".to_string(),
                });
            }

            if security.rate_limiting.burst_size > security.rate_limiting.requests_per_minute / 2 {
                result.warnings.push(ValidationWarning {
                    code: "SEC005".to_string(),
                    message: "バーストサイズがレート制限に対して大きすぎます".to_string(),
                    path: "security.rate_limiting.burst_size".to_string(),
                });
            }
        }

        // 入力検証要件
        if security.input_validation.enabled
            && security.input_validation.max_input_length > 10 * 1024 * 1024
        {
            result.warnings.push(ValidationWarning {
                code: "SEC006".to_string(),
                message: "最大入力長が非常に大きいです（10MB超）".to_string(),
                path: "security.input_validation.max_input_length".to_string(),
            });
        }
    }

    /// カスタムルールチェック
    async fn validate_custom_rules(&self, policy: &PolicyConfig, result: &mut ValidationResult) {
        debug!("カスタムルールチェックを実行中...");

        // カスタムフィールドの検証
        if self.validation_rules.validate_custom_fields {
            for (key, value) in &policy.custom {
                self.validate_custom_field(key, value, result);
            }
        }

        // 環境固有の検証
        if let Some(env_value) = policy.custom.get("environment") {
            if let Some(env_str) = env_value.as_str() {
                self.validate_environment_specific_rules(env_str, policy, result);
            }
        }
    }

    /// 推奨事項の生成
    async fn generate_recommendations(&self, policy: &PolicyConfig, result: &mut ValidationResult) {
        debug!("推奨事項を生成中...");

        // セキュリティ関連の推奨事項
        if policy.security.enabled {
            if policy.security.encryption.pbkdf2_iterations < 100000 {
                result.recommendations.push(ValidationRecommendation {
                    code: "REC001".to_string(),
                    message: "PBKDF2反復回数を100,000回以上に設定することを推奨します".to_string(),
                    path: "security.encryption.pbkdf2_iterations".to_string(),
                    impact: RecommendationImpact::Security,
                });
            }

            if policy.security.tls.min_version == "TLSv1.2" {
                result.recommendations.push(ValidationRecommendation {
                    code: "REC002".to_string(),
                    message: "TLSv1.3の使用を推奨します".to_string(),
                    path: "security.tls.min_version".to_string(),
                    impact: RecommendationImpact::Security,
                });
            }
        }

        // 監視関連の推奨事項
        if policy.monitoring.interval_seconds > 300 {
            result.recommendations.push(ValidationRecommendation {
                code: "REC003".to_string(),
                message: "監視間隔を300秒以下に設定することを推奨します".to_string(),
                path: "monitoring.interval_seconds".to_string(),
                impact: RecommendationImpact::Availability,
            });
        }

        // 認証関連の推奨事項
        if policy.authentication.enabled && !policy.authentication.require_mfa {
            result.recommendations.push(ValidationRecommendation {
                code: "REC004".to_string(),
                message: "多要素認証（MFA）の有効化を推奨します".to_string(),
                path: "authentication.require_mfa".to_string(),
                impact: RecommendationImpact::Security,
            });
        }
    }

    /// セキュリティ設定の整合性チェック
    fn validate_security_consistency(
        &self,
        security: &SecurityPolicyConfig,
        result: &mut ValidationResult,
    ) {
        if !security.enabled {
            result.warnings.push(ValidationWarning {
                code: "LOGIC001".to_string(),
                message: "セキュリティ機能が無効化されています".to_string(),
                path: "security.enabled".to_string(),
            });
            return;
        }

        // TLS強制とHTTPS要件の整合性
        if security.tls.enforce && security.tls.cipher_suites.is_empty() {
            result.errors.push(ValidationError {
                code: "LOGIC002".to_string(),
                message: "TLS強制が有効なのに暗号スイートが設定されていません".to_string(),
                path: "security.tls.cipher_suites".to_string(),
                severity: ErrorSeverity::High,
            });
        }

        // 入力検証とSQL/XSS保護の整合性
        if security.input_validation.enabled
            && !security.input_validation.sql_injection_protection
            && !security.input_validation.xss_protection
        {
            result.warnings.push(ValidationWarning {
                code: "LOGIC003".to_string(),
                message: "入力検証が有効なのにSQL/XSS保護が無効です".to_string(),
                path: "security.input_validation".to_string(),
            });
        }
    }

    /// 監視設定の整合性チェック
    fn validate_monitoring_consistency(
        &self,
        monitoring: &MonitoringPolicyConfig,
        result: &mut ValidationResult,
    ) {
        if monitoring.alerts_enabled && monitoring.interval_seconds > 600 {
            result.warnings.push(ValidationWarning {
                code: "LOGIC004".to_string(),
                message: "アラートが有効なのに監視間隔が長すぎます（10分超）".to_string(),
                path: "monitoring.interval_seconds".to_string(),
            });
        }

        if monitoring.metrics.enabled && monitoring.metrics.sampling_rate <= 0.0 {
            result.errors.push(ValidationError {
                code: "LOGIC005".to_string(),
                message: "メトリクス収集が有効なのにサンプリングレートが無効です".to_string(),
                path: "monitoring.metrics.sampling_rate".to_string(),
                severity: ErrorSeverity::Medium,
            });
        }
    }

    /// 認証設定の整合性チェック
    fn validate_authentication_consistency(
        &self,
        auth: &AuthenticationPolicyConfig,
        result: &mut ValidationResult,
    ) {
        if !auth.enabled {
            result.warnings.push(ValidationWarning {
                code: "LOGIC006".to_string(),
                message: "認証が無効化されています".to_string(),
                path: "authentication.enabled".to_string(),
            });
        }

        if auth.session_timeout_seconds < 300 {
            result.warnings.push(ValidationWarning {
                code: "LOGIC007".to_string(),
                message: "セッションタイムアウトが短すぎます（5分未満）".to_string(),
                path: "authentication.session_timeout_seconds".to_string(),
            });
        }

        if auth.session_timeout_seconds > 86400 {
            result.warnings.push(ValidationWarning {
                code: "LOGIC008".to_string(),
                message: "セッションタイムアウトが長すぎます（24時間超）".to_string(),
                path: "authentication.session_timeout_seconds".to_string(),
            });
        }
    }

    /// 設定間の相互整合性チェック
    fn validate_cross_section_consistency(
        &self,
        policy: &PolicyConfig,
        result: &mut ValidationResult,
    ) {
        // 認証とセキュリティの整合性
        if policy.authentication.require_mfa && !policy.security.enabled {
            result.warnings.push(ValidationWarning {
                code: "LOGIC009".to_string(),
                message: "MFA認証が必須なのにセキュリティ機能が無効です".to_string(),
                path: "authentication.require_mfa/security.enabled".to_string(),
            });
        }

        // 監視とセキュリティの整合性
        if policy.security.enabled && !policy.monitoring.alerts_enabled {
            result.recommendations.push(ValidationRecommendation {
                code: "LOGIC010".to_string(),
                message: "セキュリティが有効な場合、監視アラートの有効化を推奨します".to_string(),
                path: "monitoring.alerts_enabled".to_string(),
                impact: RecommendationImpact::Security,
            });
        }
    }

    /// カスタムフィールドの検証
    fn validate_custom_field(&self, key: &str, value: &Value, result: &mut ValidationResult) {
        // 環境設定の検証
        if key == "environment" {
            if let Some(env_str) = value.as_str() {
                let valid_environments = ["development", "staging", "production"];
                if !valid_environments.contains(&env_str) {
                    result.warnings.push(ValidationWarning {
                        code: "CUSTOM001".to_string(),
                        message: format!("未知の環境設定です: {}", env_str),
                        path: format!("custom.{}", key),
                    });
                }
            }
        }

        // コンプライアンスモードの検証
        if key == "compliance_mode" {
            if let Some(mode_str) = value.as_str() {
                let valid_modes = ["basic", "standard", "strict"];
                if !valid_modes.contains(&mode_str) {
                    result.warnings.push(ValidationWarning {
                        code: "CUSTOM002".to_string(),
                        message: format!("未知のコンプライアンスモードです: {}", mode_str),
                        path: format!("custom.{}", key),
                    });
                }
            }
        }
    }

    /// 環境固有のルール検証
    fn validate_environment_specific_rules(
        &self,
        environment: &str,
        policy: &PolicyConfig,
        result: &mut ValidationResult,
    ) {
        match environment {
            "production" => {
                // 本番環境固有の検証
                if !policy.security.enabled {
                    result.errors.push(ValidationError {
                        code: "ENV001".to_string(),
                        message: "本番環境ではセキュリティ機能が必須です".to_string(),
                        path: "security.enabled".to_string(),
                        severity: ErrorSeverity::Critical,
                    });
                }

                if !policy.authentication.require_mfa {
                    result.errors.push(ValidationError {
                        code: "ENV002".to_string(),
                        message: "本番環境ではMFA認証が必須です".to_string(),
                        path: "authentication.require_mfa".to_string(),
                        severity: ErrorSeverity::High,
                    });
                }

                if policy.monitoring.log_level == "debug" {
                    result.warnings.push(ValidationWarning {
                        code: "ENV003".to_string(),
                        message: "本番環境でデバッグレベルのログは推奨されません".to_string(),
                        path: "monitoring.log_level".to_string(),
                    });
                }
            }
            "development" => {
                // 開発環境固有の検証
                if policy.security.rate_limiting.requests_per_minute < 100 {
                    result.warnings.push(ValidationWarning {
                        code: "ENV004".to_string(),
                        message: "開発環境でレート制限が厳しすぎる可能性があります".to_string(),
                        path: "security.rate_limiting.requests_per_minute".to_string(),
                    });
                }
            }
            _ => {
                // 未知の環境
                result.warnings.push(ValidationWarning {
                    code: "ENV005".to_string(),
                    message: format!("未知の環境設定です: {}", environment),
                    path: "custom.environment".to_string(),
                });
            }
        }
    }

    /// バージョン形式の検証
    fn is_valid_version_format(&self, version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return false;
        }

        parts.iter().all(|part| part.parse::<u32>().is_ok())
    }

    /// 統計情報の更新
    fn update_stats(&mut self, result: &ValidationResult) {
        self.stats.total_validations += 1;

        if result.is_valid {
            self.stats.successful_validations += 1;
        } else {
            self.stats.failed_validations += 1;
        }

        // 平均検証時間の更新
        let total_time =
            self.stats.average_validation_time_ms * (self.stats.total_validations - 1) as f64;
        self.stats.average_validation_time_ms =
            (total_time + result.validation_time_ms as f64) / self.stats.total_validations as f64;

        self.stats.last_validation_time = Some(chrono::Utc::now());
    }

    /// 検証統計を取得
    pub fn get_stats(&self) -> &ValidationStats {
        &self.stats
    }

    /// 検証ルールを更新
    pub fn update_rules(&mut self, rules: ValidationRules) {
        self.validation_rules = rules;
    }
}

impl Default for ValidationRules {
    fn default() -> Self {
        Self {
            require_mandatory_fields: true,
            strict_security_validation: true,
            validate_custom_fields: true,
            validate_value_ranges: true,
            validate_logical_consistency: true,
        }
    }
}

impl Default for PolicyValidationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_policy_validation_engine_creation() {
        let engine = PolicyValidationEngine::new();
        assert_eq!(engine.stats.total_validations, 0);
    }

    #[tokio::test]
    async fn test_basic_policy_validation() {
        let mut engine = PolicyValidationEngine::new();
        let policy = PolicyConfig::default();

        let result = engine
            .validate_policy(&policy, ValidationLevel::Basic)
            .await;
        assert!(result.is_valid);
        assert_eq!(result.validation_level, ValidationLevel::Basic);
    }

    #[tokio::test]
    async fn test_invalid_policy_validation() {
        let mut engine = PolicyValidationEngine::new();
        let policy = PolicyConfig {
            id: "".to_string(), // Invalid empty ID
            ..Default::default()
        };

        let result = engine
            .validate_policy(&policy, ValidationLevel::Standard)
            .await;
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_security_validation() {
        let mut engine = PolicyValidationEngine::new();
        let mut policy = PolicyConfig::default();
        policy.security.encryption.key_size = 128; // Below minimum

        let result = engine
            .validate_policy(&policy, ValidationLevel::Strict)
            .await;
        assert!(!result.is_valid || !result.errors.is_empty());
    }

    #[tokio::test]
    async fn test_version_format_validation() {
        let engine = PolicyValidationEngine::new();

        assert!(engine.is_valid_version_format("1.0.0"));
        assert!(engine.is_valid_version_format("2.5.1"));
        assert!(!engine.is_valid_version_format("1.0"));
        assert!(!engine.is_valid_version_format("v1.0.0"));
        assert!(!engine.is_valid_version_format("1.0.0-beta"));
    }

    #[tokio::test]
    async fn test_environment_specific_validation() {
        let mut engine = PolicyValidationEngine::new();
        let mut policy = PolicyConfig::default();

        // Production environment without MFA should trigger error
        policy.custom.insert(
            "environment".to_string(),
            serde_json::Value::String("production".to_string()),
        );
        policy.authentication.require_mfa = false;

        let result = engine
            .validate_policy(&policy, ValidationLevel::Custom)
            .await;
        let has_mfa_error = result.errors.iter().any(|e| e.code == "ENV002");
        assert!(has_mfa_error);
    }
}
