//! 脅威インテリジェンス設定ローダー
//!
//! configs/threat-intelligence.toml を読み込み、
//! ThreatIntelligenceManager と AutoPolicyGenerator を初期化

use crate::policy::{
    AutoPolicyGenerator, DynamicPolicyUpdater, PolicyApplicationMode, ThreatIntelligenceManager,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// 脅威インテリジェンス設定全体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatIntelligenceConfig {
    #[serde(default)]
    pub abuseipdb: Option<AbuseIpDbConfig>,

    #[serde(default)]
    pub cve_database: Option<CveDatabaseConfig>,

    #[serde(default)]
    pub mitre_attack: Option<MitreAttackConfig>,

    #[serde(default)]
    pub auto_policy_generator: Option<AutoPolicyGeneratorConfig>,
}

/// AbuseIPDB設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbuseIpDbConfig {
    pub enabled: bool,

    /// 環境変数名 (例: "ABUSEIPDB_API_KEY")
    pub api_key_env: String,

    /// フェッチ間隔 (分)
    pub fetch_interval_minutes: u64,

    /// 信頼度閾値 (0-100)
    pub confidence_threshold: u8,

    /// 最大データ保持期間 (日)
    pub max_age_days: u64,
}

impl Default for AbuseIpDbConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key_env: "ABUSEIPDB_API_KEY".to_string(),
            fetch_interval_minutes: 60,
            confidence_threshold: 75,
            max_age_days: 90,
        }
    }
}

/// CVE Database設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CveDatabaseConfig {
    pub enabled: bool,

    /// NVD API Key (オプション)
    pub api_key_env: Option<String>,

    /// フェッチ間隔 (分)
    pub fetch_interval_minutes: u64,

    /// 最小深刻度
    pub severity_threshold: String, // "LOW" | "MEDIUM" | "HIGH" | "CRITICAL"

    /// 最小CVSSスコア
    pub cvss_min_score: f32,
}

impl Default for CveDatabaseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            api_key_env: Some("CVE_API_KEY".to_string()),
            fetch_interval_minutes: 120,
            severity_threshold: "MEDIUM".to_string(),
            cvss_min_score: 5.0,
        }
    }
}

/// MITRE ATT&CK設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MitreAttackConfig {
    pub enabled: bool,

    /// フレームワークバージョン
    pub framework_version: String,

    /// フェッチ間隔 (時間)
    pub fetch_interval_hours: u64,

    /// 自動更新
    pub auto_update: bool,

    /// 信頼度閾値
    pub confidence_threshold: f32,
}

impl Default for MitreAttackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            framework_version: "v13".to_string(),
            fetch_interval_hours: 168, // 週次
            auto_update: true,
            confidence_threshold: 0.8,
        }
    }
}

/// 自動ポリシー生成設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoPolicyGeneratorConfig {
    pub enabled: bool,

    /// "automatic" | "manual_review"
    pub application_mode: String,

    /// IPブロックリスト閾値
    pub ip_blocklist_threshold: u8,

    /// パターン信頼度最小値
    pub pattern_confidence_min: f32,

    /// 高信頼度ルールを自動適用
    pub auto_apply_high_confidence: bool,
}

impl Default for AutoPolicyGeneratorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            application_mode: "manual_review".to_string(),
            ip_blocklist_threshold: 80,
            pattern_confidence_min: 0.75,
            auto_apply_high_confidence: false,
        }
    }
}

impl ThreatIntelligenceConfig {
    /// TOMLファイルから設定を読み込み
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: ThreatIntelligenceConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// デフォルト設定ファイルから読み込み
    pub fn load_default() -> Result<Self, Box<dyn std::error::Error>> {
        Self::load_from_file("configs/threat-intelligence.toml")
    }

    /// ThreatIntelligenceManagerを初期化
    pub fn create_manager(
        &self,
        policy_updater: Arc<DynamicPolicyUpdater>,
    ) -> Result<ThreatIntelligenceManager, Box<dyn std::error::Error>> {
        let mut manager = ThreatIntelligenceManager::new(policy_updater, Some(0.7));

        // AbuseIPDB設定
        if let Some(ref config) = self.abuseipdb {
            if config.enabled {
                let api_key = std::env::var(&config.api_key_env)
                    .map_err(|_| format!("Environment variable {} not set", config.api_key_env))?;
                manager = manager.with_abuseipdb(api_key);
            }
        }

        // CVE Database設定
        if let Some(ref config) = self.cve_database {
            if config.enabled {
                let api_key = config
                    .api_key_env
                    .as_ref()
                    .and_then(|env_name| std::env::var(env_name).ok());
                manager = manager.with_cve_db(api_key);
            }
        }

        // MITRE ATT&CK設定
        if let Some(ref config) = self.mitre_attack {
            if config.enabled {
                manager = manager.with_mitre_attack(config.framework_version.clone());
            }
        }

        Ok(manager)
    }

    /// AutoPolicyGeneratorを初期化
    pub fn create_policy_generator(
        &self,
        policy_updater: Arc<DynamicPolicyUpdater>,
    ) -> Result<AutoPolicyGenerator, Box<dyn std::error::Error>> {
        let config = self.auto_policy_generator.clone().unwrap_or_default();

        if !config.enabled {
            return Err("Auto policy generator is disabled".into());
        }

        let mode = match config.application_mode.as_str() {
            "automatic" => PolicyApplicationMode::Automatic,
            "manual_review" => PolicyApplicationMode::ManualReview,
            _ => {
                return Err(format!("Invalid application mode: {}", config.application_mode).into())
            }
        };

        Ok(AutoPolicyGenerator::new(
            policy_updater,
            config.ip_blocklist_threshold,
            config.pattern_confidence_min as f64,
            mode,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ThreatIntelligenceConfig {
            abuseipdb: Some(AbuseIpDbConfig::default()),
            cve_database: Some(CveDatabaseConfig::default()),
            mitre_attack: Some(MitreAttackConfig::default()),
            auto_policy_generator: Some(AutoPolicyGeneratorConfig::default()),
        };

        assert!(config.abuseipdb.as_ref().unwrap().enabled);
        assert_eq!(config.abuseipdb.as_ref().unwrap().confidence_threshold, 75);

        assert!(config.cve_database.as_ref().unwrap().enabled);
        assert_eq!(config.cve_database.as_ref().unwrap().cvss_min_score, 5.0);

        assert!(config.mitre_attack.as_ref().unwrap().enabled);
        assert_eq!(
            config.mitre_attack.as_ref().unwrap().framework_version,
            "v13"
        );

        assert!(config.auto_policy_generator.as_ref().unwrap().enabled);
        assert_eq!(
            config
                .auto_policy_generator
                .as_ref()
                .unwrap()
                .application_mode,
            "manual_review"
        );
    }

    #[test]
    fn test_serialize_deserialize() {
        let config = ThreatIntelligenceConfig {
            abuseipdb: Some(AbuseIpDbConfig::default()),
            cve_database: None,
            mitre_attack: Some(MitreAttackConfig::default()),
            auto_policy_generator: Some(AutoPolicyGeneratorConfig::default()),
        };

        let toml_str = toml::to_string(&config).unwrap();
        println!("Serialized TOML:\n{}", toml_str);

        let deserialized: ThreatIntelligenceConfig = toml::from_str(&toml_str).unwrap();

        assert!(deserialized.abuseipdb.is_some());
        assert!(deserialized.cve_database.is_none());
        assert!(deserialized.mitre_attack.is_some());
    }
}
