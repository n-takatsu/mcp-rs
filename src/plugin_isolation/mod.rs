//! 隔離プラグインマネージャー
//!
//! セキュアコアサーバーから完全に分離されたプラグインサーバー群の管理システム
//! 各プラグインは独立したコンテナ環境で実行され、厳格なセキュリティ制約下で動作する

pub mod communication_broker;
mod config;
pub mod error_handler;
mod health;
pub mod inter_plugin_comm;
pub mod isolation_engine;
pub mod lifecycle_manager;
mod manager;
pub mod monitoring;
pub mod sandbox;
pub mod security_validation;
mod types;

pub use communication_broker::{
    AuthenticationInfo, BrokerConfig, BrokerMessage, ChannelStats, ChannelType,
    CommunicationBroker, CommunicationChannel, EncryptionAlgorithm, FilterAction, FilterType,
    MessageFilter, MessageType, RateLimitConfig,
};
pub use config::{
    AlertThresholds, IsolationConfig, MonitoringConfig, PluginManagerConfig, SecurityPolicy,
};
pub use error_handler::{
    ErrorCategory, ErrorHandlingConfig, ErrorSeverity, ErrorStats, PluginError,
    PluginErrorHandler, RecoveryAction,
};
pub use health::PluginManagerHealth;
pub use inter_plugin_comm::{
    CommunicationEvent, CommunicationRule, CommEventType, CommResult, InterPluginCommConfig,
    InterPluginCommStats, InterPluginCommunicationController, QueuedMessage, RuleStatus,
};
pub use manager::IsolatedPluginManager;
pub use monitoring::{
    Alert, AlertSeverity, AlertStatus, DetailedMetrics, EventSeverity, LogEntry, LogLevel,
    MetricValue, MonitoringEvent, MonitoringEventType, MonitoringSystem, PerformanceStats,
    ProcessStats, SecurityStats, SystemMetrics,
};
pub use security_validation::{
    DynamicAnalysisResult, FindingType, IssueSeverity, PermissionValidationResult, SecurityFinding,
    SecurityIssue, SecurityIssueType, SecurityLevel, SecurityValidationConfig,
    SecurityValidationSystem, StaticAnalysisResult, ValidationResult, ValidationStatus,
    ValidationType, VulnerabilityResult,
};
pub use types::{PluginInstance, PluginMetadata, PluginMetrics, PluginState, ResourceLimits};
