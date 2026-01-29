//! Phase 1統合テスト - 脅威インテリジェンス統合

use mcp_rs::error::Result;
use mcp_rs::policy::threat_intel_config::{
    AbuseIpDbConfig, AutoPolicyGeneratorConfig, CveDatabaseConfig, MitreAttackConfig,
    ThreatIntelligenceConfig,
};

#[tokio::test]
async fn test_threat_intelligence_config_loading() -> Result<()> {
    // 設定構造体のデフォルト値をテスト
    let config = ThreatIntelligenceConfig {
        abuseipdb: Some(AbuseIpDbConfig::default()),
        cve_database: Some(CveDatabaseConfig::default()),
        mitre_attack: Some(MitreAttackConfig::default()),
        auto_policy_generator: Some(AutoPolicyGeneratorConfig::default()),
    };

    assert!(config.abuseipdb.as_ref().unwrap().enabled);
    assert!(config.cve_database.as_ref().unwrap().enabled);
    assert!(config.mitre_attack.as_ref().unwrap().enabled);

    Ok(())
}

#[tokio::test]
async fn test_auto_policy_generator_config() -> Result<()> {
    let config = AutoPolicyGeneratorConfig::default();

    assert!(config.enabled);
    assert_eq!(config.ip_blocklist_threshold, 80);
    assert!(config.pattern_confidence_min > 0.0);
    assert!(config.pattern_confidence_min <= 1.0);

    Ok(())
}

#[test]
fn test_threat_intelligence_module_exists() {
    // モジュールが正しくコンパイルされることを確認
    use mcp_rs::policy::auto_policy_generator::AutoPolicyGenerator;
    use mcp_rs::policy::threat_intelligence::ThreatIntelligenceManager;
    use mcp_rs::policy::threat_providers::{AbuseIpDbClient, CveDbClient, MitreAttackClient};

    // 型が存在することを確認
    let _: Option<ThreatIntelligenceManager> = None;
    let _: Option<AutoPolicyGenerator> = None;
    let _: Option<AbuseIpDbClient> = None;
    let _: Option<CveDbClient> = None;
    let _: Option<MitreAttackClient> = None;
}

#[test]
fn test_policy_application_modes() {
    use mcp_rs::policy::auto_policy_generator::PolicyApplicationMode;

    // すべてのモードが定義されていることを確認
    let _automatic = PolicyApplicationMode::Automatic;
    let _manual = PolicyApplicationMode::ManualReview;
}

#[tokio::test]
async fn test_policy_config_with_blocked_ips() -> Result<()> {
    use chrono::Utc;
    use mcp_rs::policy_config::{PolicyConfig, SecurityPolicyConfig};

    let mut policy = PolicyConfig {
        id: "test-policy".to_string(),
        name: "Test Policy".to_string(),
        version: "1.0.0".to_string(),
        description: Some("Test policy".to_string()),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        security: SecurityPolicyConfig::default(),
        monitoring: Default::default(),
        authentication: Default::default(),
        custom: Default::default(),
    };

    // blocked_ipsフィールドが使用できることを確認
    policy
        .security
        .blocked_ips
        .push("192.168.1.100".to_string());
    policy.security.blocked_ips.push("10.0.0.50".to_string());

    assert_eq!(policy.security.blocked_ips.len(), 2);
    assert!(policy
        .security
        .blocked_ips
        .contains(&"192.168.1.100".to_string()));

    Ok(())
}
