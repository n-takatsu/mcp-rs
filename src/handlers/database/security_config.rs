//! Advanced Security Configuration and Policy Management
//!
//! This module defines the configuration structures for all security components:
//!
//! - **Multi-Factor Authentication (MFA)**: TOTP, backup codes, device trust
//! - **Role-Based Access Control (RBAC)**: Roles, permissions, hierarchy, conditions
//! - **Anomaly Detection**: Behavioral analysis, threat intelligence
//! - **Encryption**: AES-GCM-256, key derivation, column encryption
//! - **Audit & Compliance**: Event logging, retention, compliance reporting
//! - **Incident Response**: Automated threat mitigation, escalation policies

use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Advanced Security Configuration
///
/// Root configuration structure containing all security subsystem configurations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdvancedSecurityConfig {
    /// Multi-Factor Authentication configuration
    pub mfa: MfaConfig,

    /// Role-Based Access Control configuration
    pub rbac: RbacConfig,

    /// Anomaly Detection configuration
    pub anomaly_detection: AnomalyDetectionConfig,

    /// Encryption configuration
    pub encryption: EncryptionConfig,

    /// Audit and Compliance configuration
    pub audit: AuditConfig,

    /// Automated Incident Response configuration
    pub incident_response: IncidentResponseConfig,
}

/// Multi-Factor Authentication Configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MfaConfig {
    /// Whether MFA is required for authentication
    pub required: bool,

    /// Time-based One-Time Password (TOTP) settings
    pub totp: TotpConfig,

    /// Backup code settings for MFA recovery
    pub backup_codes: BackupCodeConfig,

    /// Device trust level configuration
    pub device_trust: DeviceTrustConfig,

    /// List of user IDs exempt from MFA requirement
    pub exceptions: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpConfig {
    pub enabled: bool,
    pub secret_length: usize,
    pub time_window: u64,  // 秒
    pub algorithm: String, // "SHA1", "SHA256", "SHA512"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCodeConfig {
    pub enabled: bool,
    pub code_count: usize,
    pub code_length: usize,
    pub single_use: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTrustConfig {
    pub enabled: bool,
    pub trust_threshold: f64,
    pub learning_period_days: u32,
    pub auto_trust_known_devices: bool,
}

/// RBAC設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RbacConfig {
    /// RBAC有効フラグ
    pub enabled: bool,

    /// デフォルト役割
    pub default_role: String,

    /// 役割階層
    pub role_hierarchy: HashMap<String, Vec<String>>,

    /// リソースポリシー
    pub resource_policies: HashMap<String, ResourcePolicyConfig>,

    /// 時間ベースアクセス制御
    pub time_based_access: TimeBasedAccessConfig,

    /// IP制限設定
    pub ip_restrictions: IpRestrictionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePolicyConfig {
    pub table_name: String,
    pub access_rules: Vec<AccessRule>,
    pub column_level_permissions: HashMap<String, ColumnPermission>,
    pub row_level_security: Option<RowLevelSecurityConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRule {
    pub role: String,
    pub actions: HashSet<String>, // READ, WRITE, DELETE, etc.
    pub conditions: Vec<AccessCondition>,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessCondition {
    pub condition_type: ConditionType,
    pub value: String,
    pub operator: ConditionOperator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionType {
    TimeOfDay,
    DayOfWeek,
    IpAddress,
    UserAttribute,
    DataSensitivity,
    QueryComplexity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConditionOperator {
    Equals,
    NotEquals,
    Contains,
    NotContains,
    GreaterThan,
    LessThan,
    Between,
    In,
    NotIn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnPermission {
    pub read_roles: HashSet<String>,
    pub write_roles: HashSet<String>,
    pub encryption_required: bool,
    pub masking_rules: Option<MaskingRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskingRule {
    pub mask_type: MaskType,
    pub preserve_length: bool,
    pub mask_character: String,
    pub partial_mask: Option<PartialMaskConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaskType {
    Full,     // 完全マスク
    Partial,  // 部分マスク
    Hash,     // ハッシュ化
    Tokenize, // トークン化
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialMaskConfig {
    pub reveal_start: usize,
    pub reveal_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowLevelSecurityConfig {
    pub enabled: bool,
    pub policy_column: String,
    pub user_attribute: String,
    pub allow_admin_bypass: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBasedAccessConfig {
    pub enabled: bool,
    pub business_hours: BusinessHours,
    pub timezone: String,
    pub emergency_access: EmergencyAccessConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BusinessHours {
    pub monday: Option<DaySchedule>,
    pub tuesday: Option<DaySchedule>,
    pub wednesday: Option<DaySchedule>,
    pub thursday: Option<DaySchedule>,
    pub friday: Option<DaySchedule>,
    pub saturday: Option<DaySchedule>,
    pub sunday: Option<DaySchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaySchedule {
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub break_periods: Vec<BreakPeriod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakPeriod {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyAccessConfig {
    pub enabled: bool,
    pub emergency_roles: HashSet<String>,
    pub notification_required: bool,
    pub auto_revoke_hours: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpRestrictionConfig {
    pub enabled: bool,
    pub default_policy: IpPolicy,
    pub role_based_restrictions: HashMap<String, IpRoleRestriction>,
    pub geo_blocking: GeoBlockingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpPolicy {
    Allow,
    Deny,
    Conditional,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpRoleRestriction {
    pub allowed_ranges: Vec<String>,
    pub denied_ranges: Vec<String>,
    pub require_vpn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoBlockingConfig {
    pub enabled: bool,
    pub allowed_countries: HashSet<String>,
    pub blocked_countries: HashSet<String>,
    pub suspicious_countries: HashSet<String>,
}

/// 異常検知設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetectionConfig {
    /// 異常検知有効フラグ
    pub enabled: bool,

    /// 機械学習モデル設定
    pub ml_config: MachineLearningConfig,

    /// ベースライン学習設定
    pub baseline_learning: BaselineLearningConfig,

    /// 異常スコア閾値
    pub thresholds: AnomalyThresholds,

    /// リアルタイム監視設定
    pub real_time_monitoring: RealTimeMonitoringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineLearningConfig {
    pub model_type: String, // "isolation_forest", "one_class_svm", "neural_network"
    pub training_data_retention_days: u32,
    pub retrain_interval_hours: u32,
    pub feature_selection: FeatureSelectionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureSelectionConfig {
    pub query_timing: bool,
    pub query_complexity: bool,
    pub data_volume: bool,
    pub access_patterns: bool,
    pub user_behavior: bool,
    pub network_patterns: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineLearningConfig {
    pub learning_period_days: u32,
    pub minimum_samples: u32,
    pub update_frequency_hours: u32,
    pub seasonal_adjustment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyThresholds {
    pub low_risk: f64,      // 0.3
    pub medium_risk: f64,   // 0.6
    pub high_risk: f64,     // 0.8
    pub critical_risk: f64, // 0.95
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealTimeMonitoringConfig {
    pub enabled: bool,
    pub monitoring_interval_seconds: u32,
    pub alert_delay_seconds: u32,
    pub batch_processing: bool,
}

/// 暗号化設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EncryptionConfig {
    /// 一般復号化許可フラグ
    pub allow_general_decryption: bool,

    /// 暗号化対象カラムのリスト (table.column形式)
    pub encrypted_columns: Option<Vec<String>>,

    /// カラムレベル暗号化
    pub column_encryption: ColumnEncryptionConfig,

    /// 転送時暗号化
    pub transport_encryption: TransportEncryptionConfig,

    /// キー管理設定
    pub key_management: KeyManagementConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnEncryptionConfig {
    pub enabled: bool,
    pub default_algorithm: String, // "AES-256-GCM", "ChaCha20-Poly1305"
    pub encrypted_columns: HashMap<String, ColumnEncryptionRule>,
    pub key_rotation_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnEncryptionRule {
    pub table: String,
    pub column: String,
    pub algorithm: String,
    pub key_derivation: KeyDerivationConfig,
    pub authorized_roles: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyDerivationConfig {
    pub method: String, // "PBKDF2", "Argon2", "scrypt"
    pub iterations: u32,
    pub salt_length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportEncryptionConfig {
    pub tls_version: String, // "1.2", "1.3"
    pub cipher_suites: Vec<String>,
    pub certificate_validation: bool,
    pub mutual_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyManagementConfig {
    pub provider: String, // "internal", "aws_kms", "azure_keyvault", "hashicorp_vault"
    pub key_rotation_enabled: bool,
    pub key_rotation_interval_days: u32,
    pub backup_key_count: u32,
    pub hsm_integration: Option<HsmConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HsmConfig {
    pub enabled: bool,
    pub provider: String,
    pub partition_label: String,
    pub authentication_method: String,
}

/// 監査設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuditConfig {
    /// 詳細監査ログ
    pub detailed_logging: DetailedLoggingConfig,

    /// コンプライアンス設定
    pub compliance: ComplianceConfig,

    /// ログ保持設定
    pub retention: LogRetentionConfig,

    /// ログ分析設定
    pub analysis: LogAnalysisConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedLoggingConfig {
    pub log_all_queries: bool,
    pub log_query_results: bool,
    pub log_authentication_events: bool,
    pub log_authorization_decisions: bool,
    pub log_schema_changes: bool,
    pub log_admin_actions: bool,
    pub sensitive_data_logging: SensitiveDataLoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveDataLoggingConfig {
    pub enabled: bool,
    pub mask_sensitive_values: bool,
    pub log_access_attempts: bool,
    pub log_access_denials: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ComplianceConfig {
    pub gdpr_enabled: bool,
    pub ccpa_enabled: bool,
    pub sox_enabled: bool,
    pub hipaa_enabled: bool,
    pub pci_dss_enabled: bool,
    pub custom_regulations: Vec<CustomRegulationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRegulationConfig {
    pub name: String,
    pub requirements: Vec<ComplianceRequirement>,
    pub audit_frequency: AuditFrequency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRequirement {
    pub requirement_id: String,
    pub description: String,
    pub validation_rule: String,
    pub severity: ComplianceSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplianceSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditFrequency {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Annually,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRetentionConfig {
    pub default_retention_days: u32,
    pub compliance_retention_days: u32,
    pub high_risk_retention_days: u32,
    pub archive_storage: ArchiveStorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveStorageConfig {
    pub enabled: bool,
    pub storage_type: String, // "s3", "azure_blob", "gcs", "local"
    pub compression_enabled: bool,
    pub encryption_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogAnalysisConfig {
    pub real_time_analysis: bool,
    pub pattern_detection: bool,
    pub anomaly_correlation: bool,
    pub automated_reporting: AutomatedReportingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomatedReportingConfig {
    pub enabled: bool,
    pub report_types: Vec<ReportType>,
    pub delivery_schedule: DeliverySchedule,
    pub recipients: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportType {
    SecuritySummary,
    ComplianceStatus,
    AnomalyReport,
    AccessReport,
    PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverySchedule {
    pub frequency: ReportFrequency,
    pub time_of_day: NaiveTime,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportFrequency {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
}

/// インシデント対応設定
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IncidentResponseConfig {
    /// 自動対応有効フラグ
    pub auto_response_enabled: bool,

    /// 対応ルール
    pub response_rules: Vec<ResponseRule>,

    /// 通知設定
    pub notification: NotificationConfig,

    /// エスカレーション設定
    pub escalation: EscalationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseRule {
    pub rule_id: String,
    pub trigger_conditions: Vec<TriggerCondition>,
    pub actions: Vec<ResponseAction>,
    pub severity_level: SeverityLevel,
    pub auto_execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerCondition {
    pub condition_type: TriggerType,
    pub threshold: f64,
    pub time_window_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerType {
    AnomalyScore,
    FailedLoginAttempts,
    UnauthorizedAccess,
    DataExfiltration,
    SystemResourceUsage,
    NetworkTrafficAnomaly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseAction {
    BlockUser,
    BlockIP,
    RequireReAuthentication,
    NotifyAdministrator,
    LogIncident,
    QuarantineQuery,
    EscalateToHuman,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SeverityLevel {
    #[default]
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationConfig {
    pub email_enabled: bool,
    pub sms_enabled: bool,
    pub slack_enabled: bool,
    pub webhook_enabled: bool,
    pub notification_targets: HashMap<SeverityLevel, Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EscalationConfig {
    pub enabled: bool,
    pub escalation_rules: Vec<EscalationRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRule {
    pub severity: SeverityLevel,
    pub escalation_delay_minutes: u32,
    pub escalation_targets: Vec<String>,
    pub max_escalation_levels: u32,
}

impl Default for TotpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            secret_length: 32,
            time_window: 30,
            algorithm: "SHA1".to_string(),
        }
    }
}

impl Default for BackupCodeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            code_count: 10,
            code_length: 8,
            single_use: true,
        }
    }
}

impl Default for DeviceTrustConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trust_threshold: 0.7,
            learning_period_days: 30,
            auto_trust_known_devices: false,
        }
    }
}

impl Default for RbacConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_role: "user".to_string(),
            role_hierarchy: HashMap::new(),
            resource_policies: HashMap::new(),
            time_based_access: TimeBasedAccessConfig::default(),
            ip_restrictions: IpRestrictionConfig::default(),
        }
    }
}

impl Default for TimeBasedAccessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            business_hours: BusinessHours::default(),
            timezone: "UTC".to_string(),
            emergency_access: EmergencyAccessConfig::default(),
        }
    }
}

impl Default for EmergencyAccessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            emergency_roles: HashSet::new(),
            notification_required: true,
            auto_revoke_hours: 24,
        }
    }
}

impl Default for IpRestrictionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_policy: IpPolicy::Allow,
            role_based_restrictions: HashMap::new(),
            geo_blocking: GeoBlockingConfig::default(),
        }
    }
}

impl Default for AnomalyDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ml_config: MachineLearningConfig::default(),
            baseline_learning: BaselineLearningConfig::default(),
            thresholds: AnomalyThresholds::default(),
            real_time_monitoring: RealTimeMonitoringConfig::default(),
        }
    }
}

impl Default for MachineLearningConfig {
    fn default() -> Self {
        Self {
            model_type: "isolation_forest".to_string(),
            training_data_retention_days: 90,
            retrain_interval_hours: 24,
            feature_selection: FeatureSelectionConfig::default(),
        }
    }
}

impl Default for FeatureSelectionConfig {
    fn default() -> Self {
        Self {
            query_timing: true,
            query_complexity: true,
            data_volume: true,
            access_patterns: true,
            user_behavior: true,
            network_patterns: false,
        }
    }
}

impl Default for BaselineLearningConfig {
    fn default() -> Self {
        Self {
            learning_period_days: 30,
            minimum_samples: 100,
            update_frequency_hours: 6,
            seasonal_adjustment: true,
        }
    }
}

impl Default for AnomalyThresholds {
    fn default() -> Self {
        Self {
            low_risk: 0.3,
            medium_risk: 0.6,
            high_risk: 0.8,
            critical_risk: 0.95,
        }
    }
}

impl Default for RealTimeMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitoring_interval_seconds: 60,
            alert_delay_seconds: 30,
            batch_processing: false,
        }
    }
}

impl Default for ColumnEncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_algorithm: "AES-256-GCM".to_string(),
            encrypted_columns: HashMap::new(),
            key_rotation_days: 365,
        }
    }
}

impl Default for TransportEncryptionConfig {
    fn default() -> Self {
        Self {
            tls_version: "1.3".to_string(),
            cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
            ],
            certificate_validation: true,
            mutual_tls: false,
        }
    }
}

impl Default for KeyManagementConfig {
    fn default() -> Self {
        Self {
            provider: "internal".to_string(),
            key_rotation_enabled: true,
            key_rotation_interval_days: 90,
            backup_key_count: 3,
            hsm_integration: None,
        }
    }
}

impl Default for DetailedLoggingConfig {
    fn default() -> Self {
        Self {
            log_all_queries: true,
            log_query_results: false,
            log_authentication_events: true,
            log_authorization_decisions: true,
            log_schema_changes: true,
            log_admin_actions: true,
            sensitive_data_logging: SensitiveDataLoggingConfig::default(),
        }
    }
}

impl Default for SensitiveDataLoggingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            mask_sensitive_values: true,
            log_access_attempts: true,
            log_access_denials: true,
        }
    }
}

impl Default for LogRetentionConfig {
    fn default() -> Self {
        Self {
            default_retention_days: 365,
            compliance_retention_days: 2557, // 7 years
            high_risk_retention_days: 1095,  // 3 years
            archive_storage: ArchiveStorageConfig::default(),
        }
    }
}

impl Default for ArchiveStorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            storage_type: "local".to_string(),
            compression_enabled: true,
            encryption_enabled: true,
        }
    }
}

impl Default for LogAnalysisConfig {
    fn default() -> Self {
        Self {
            real_time_analysis: true,
            pattern_detection: true,
            anomaly_correlation: true,
            automated_reporting: AutomatedReportingConfig::default(),
        }
    }
}

impl Default for AutomatedReportingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            report_types: vec![ReportType::SecuritySummary],
            delivery_schedule: DeliverySchedule {
                frequency: ReportFrequency::Weekly,
                time_of_day: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                timezone: "UTC".to_string(),
            },
            recipients: Vec::new(),
        }
    }
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            email_enabled: true,
            sms_enabled: false,
            slack_enabled: false,
            webhook_enabled: false,
            notification_targets: HashMap::new(),
        }
    }
}
