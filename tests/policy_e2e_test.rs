//! 動的ポリシー更新システム E2Eテスト
//!
//! Issue #211の統合テストスイート

use mcp_rs::policy::{
    AbuseIpDbClient, AutoPolicyGenerator, CveDbClient, DynamicPolicyUpdater, MitreAttackClient,
    PolicyApplicationMode, ThreatIntelligenceManager,
};
use std::sync::Arc;

#[tokio::test]
#[ignore] // 外部API呼び出しが必要なため、デフォルトではスキップ
async fn test_abuseipdb_integration() {
    // 環境変数からAPIキーを取得
    let api_key = std::env::var("ABUSEIPDB_API_KEY").expect("ABUSEIPDB_API_KEY not set");

    let client = AbuseIpDbClient::new(api_key);

    // 既知の悪意あるIP (テスト用)
    let result = client.check_ip("118.25.6.39", 30).await;

    assert!(result.is_ok());
    let report = result.unwrap();
    assert_eq!(report.ip_address, "118.25.6.39");
    println!("Abuse confidence score: {}", report.abuse_confidence_score);
}

#[tokio::test]
#[ignore]
async fn test_cve_database_integration() {
    let client = CveDbClient::new(None);

    // 既知のCVE (テスト用)
    let result = client.fetch_cve("CVE-2024-21413").await;

    assert!(result.is_ok());
    let cve = result.unwrap();
    assert_eq!(cve.cve_id, "CVE-2024-21413");
    println!("CVE CVSS Score: {}", cve.cvss_score);
    println!("Severity: {:?}", cve.severity);
}

#[tokio::test]
#[ignore]
async fn test_mitre_attack_integration() {
    let client = MitreAttackClient::new("v13".to_string());

    // Command and Scripting Interpreter
    let result = client.fetch_technique("T1059").await;

    assert!(result.is_ok());
    let pattern = result.unwrap();
    assert_eq!(pattern.id, "T1059");
    println!("Technique: {}", pattern.name);
    println!("Description: {}", pattern.description);
}

#[tokio::test]
async fn test_threat_intelligence_manager() {
    use mcp_rs::policy::UpdateConfig;
    // ポリシー更新マネージャーのモック作成
    let config = mcp_rs::policy_config::PolicyConfig::default();
    let policy_updater = Arc::new(DynamicPolicyUpdater::new(
        config.clone(),
        UpdateConfig::default(),
    ));

    // 脅威インテリジェンスマネージャー作成
    let manager = ThreatIntelligenceManager::new(policy_updater.clone(), Some(0.7));

    // 自動更新を有効化
    manager.enable_auto_update().await;
    assert!(manager.is_auto_update_enabled().await);

    // 期限切れ脅威のクリーンアップテスト
    let cleaned = manager.cleanup_expired_threats().await;
    assert_eq!(cleaned, 0); // 初期状態では0件
}

#[tokio::test]
async fn test_auto_policy_generator() {
    use mcp_rs::policy::UpdateConfig;
    let config = mcp_rs::policy_config::PolicyConfig::default();
    let policy_updater = Arc::new(DynamicPolicyUpdater::new(
        config.clone(),
        UpdateConfig::default(),
    ));

    let generator = AutoPolicyGenerator::new(
        policy_updater.clone(),
        75,   // IP blocklist threshold
        0.75, // Pattern confidence min
        PolicyApplicationMode::ManualReview,
    );

    // モックデータで自動ポリシー生成テスト
    use mcp_rs::policy::AbuseIpDbReport;
    use std::time::SystemTime;

    let reports = vec![
        AbuseIpDbReport {
            ip_address: "192.0.2.1".to_string(),
            abuse_confidence_score: 90,
            country_code: Some("CN".to_string()),
            usage_type: None,
            isp: None,
            domain: None,
            total_reports: 50,
            last_reported_at: Some(SystemTime::now()),
            is_whitelisted: false,
            is_tor: false,
        },
        AbuseIpDbReport {
            ip_address: "192.0.2.2".to_string(),
            abuse_confidence_score: 60, // 閾値以下
            country_code: Some("US".to_string()),
            usage_type: None,
            isp: None,
            domain: None,
            total_reports: 10,
            last_reported_at: Some(SystemTime::now()),
            is_whitelisted: false,
            is_tor: false,
        },
    ];

    let rules = generator
        .generate_ip_blocklist_from_abuseipdb(&reports)
        .await
        .unwrap();

    // 閾値以上のIP(192.0.2.1)のみがブロックされる
    assert_eq!(rules.len(), 1);

    let rule = &rules[0];
    if let mcp_rs::policy::PolicyRuleType::IpBlock { ips } = &rule.rule_type {
        assert_eq!(ips.len(), 1);
        assert_eq!(ips[0], "192.0.2.1");
    } else {
        panic!("Expected IpBlock rule type");
    }

    assert!(!rule.auto_apply); // ManualReviewモード
}

#[tokio::test]
#[ignore]
async fn test_end_to_end_threat_intelligence_workflow() {
    // 完全な E2E ワークフロー
    let api_key = std::env::var("ABUSEIPDB_API_KEY").ok();

    if api_key.is_none() {
        println!("Skipping E2E test: ABUSEIPDB_API_KEY not set");
        return;
    }

    use mcp_rs::policy::UpdateConfig;
    // 1. ポリシー更新システム初期化
    let config = mcp_rs::policy_config::PolicyConfig::default();
    let policy_updater = Arc::new(DynamicPolicyUpdater::new(
        config.clone(),
        UpdateConfig::default(),
    ));

    // 2. 脅威インテリジェンスマネージャー初期化
    let manager = ThreatIntelligenceManager::new(policy_updater.clone(), Some(0.7))
        .with_abuseipdb(api_key.unwrap())
        .with_cve_db(None)
        .with_mitre_attack("v13".to_string());

    manager.enable_auto_update().await;

    // 3. 外部脅威情報取得
    let ips = vec!["118.25.6.39".to_string()];
    let abuseipdb_reports = manager.fetch_bulk_from_abuseipdb(&ips, 30).await.unwrap();

    println!("Fetched {} AbuseIPDB reports", abuseipdb_reports.len());

    // 4. 自動ポリシー生成
    let generator = AutoPolicyGenerator::new(
        policy_updater.clone(),
        75,
        0.75,
        PolicyApplicationMode::Automatic,
    );

    let rules = generator
        .generate_ip_blocklist_from_abuseipdb(&abuseipdb_reports)
        .await
        .unwrap();

    println!("Generated {} policy rules", rules.len());

    // 5. ルール適用
    let applied_count = generator.apply_all_rules(&rules).await.unwrap();

    println!("Applied {} rules", applied_count);

    // 6. ポリシー検証
    let current_policy = policy_updater.get_active_policy().await;
    assert!(!current_policy.security.blocked_ips.is_empty());

    println!("✅ E2E workflow completed successfully");
}

#[tokio::test]
async fn test_policy_application_modes() {
    use mcp_rs::policy::UpdateConfig;
    let config = mcp_rs::policy_config::PolicyConfig::default();
    let policy_updater = Arc::new(DynamicPolicyUpdater::new(
        config.clone(),
        UpdateConfig::default(),
    ));

    // Automaticモードのテスト
    let auto_generator = AutoPolicyGenerator::new(
        policy_updater.clone(),
        75,
        0.75,
        PolicyApplicationMode::Automatic,
    );

    // ManualReviewモードのテスト
    let manual_generator = AutoPolicyGenerator::new(
        policy_updater.clone(),
        75,
        0.75,
        PolicyApplicationMode::ManualReview,
    );

    // モードの違いを確認
    use mcp_rs::policy::AbuseIpDbReport;
    use std::time::SystemTime;

    let report = AbuseIpDbReport {
        ip_address: "192.0.2.100".to_string(),
        abuse_confidence_score: 95,
        country_code: Some("XX".to_string()),
        usage_type: None,
        isp: None,
        domain: None,
        total_reports: 100,
        last_reported_at: Some(SystemTime::now()),
        is_whitelisted: false,
        is_tor: false,
    };

    let auto_rules = auto_generator
        .generate_ip_blocklist_from_abuseipdb(std::slice::from_ref(&report))
        .await
        .unwrap();
    let manual_rules = manual_generator
        .generate_ip_blocklist_from_abuseipdb(std::slice::from_ref(&report))
        .await
        .unwrap();

    assert_eq!(auto_rules.len(), 1);
    assert_eq!(manual_rules.len(), 1);
    assert!(auto_rules[0].auto_apply);
    assert!(!manual_rules[0].auto_apply);
}
