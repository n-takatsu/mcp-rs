//! ポリシー管理モジュール
//!
//! 動的ポリシー更新システムの中核機能を提供

pub mod auto_policy_generator;
pub mod dynamic_updater;
pub mod hot_reload;
pub mod rollback;
pub mod threat_intel_config;
pub mod threat_intelligence;
pub mod threat_providers;
pub mod version_control;

pub use auto_policy_generator::{
    AutoPolicyGenerator, GeneratedPolicyRule, PolicyApplicationMode, PolicyRuleType,
};
pub use dynamic_updater::{DynamicPolicyUpdater, PolicyUpdateEvent, UpdateConfig};
pub use hot_reload::{HotReloadManager, ReloadStrategy};
pub use rollback::{RollbackManager, RollbackPoint};
pub use threat_intel_config::{
    AbuseIpDbConfig, AutoPolicyGeneratorConfig, CveDatabaseConfig, MitreAttackConfig,
    ThreatIntelligenceConfig,
};
pub use threat_intelligence::{
    ThreatIntelligence, ThreatIntelligenceManager, ThreatLevel, ThreatType,
};
pub use threat_providers::{
    AbuseIpDbClient, AbuseIpDbReport, AttackPattern, CveDbClient, CveReport, CveSeverity,
    MitreAttackClient,
};
pub use version_control::{PolicyVersion, VersionManager};
