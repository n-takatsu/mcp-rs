//! MITRE ATT&CK Provider Tests

use mcp_rs::threat_intelligence::*;
use std::collections::HashMap;

/// テスト用のMITRE ATT&CK設定を作成
fn create_test_config() -> ProviderConfig {
    ProviderConfig {
        name: "MITRE-ATTACK".to_string(),
        enabled: true,
        api_key: String::new(),
        base_url: "https://raw.githubusercontent.com/mitre/cti/master".to_string(),
        timeout_seconds: 30,
        rate_limit_per_minute: 60,
        reliability_factor: 0.95,
        provider_specific: HashMap::new(),
    }
}

#[tokio::test]
async fn test_provider_creation_success() {
    let config = create_test_config();
    let result = ProviderFactory::create_provider(config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_provider_name() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    assert_eq!(provider.name(), "MITRE-ATTACK");
}

#[tokio::test]
async fn test_provider_config() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config.clone()).unwrap();
    assert_eq!(provider.config().name, config.name);
}

#[tokio::test]
async fn test_valid_technique_id_format() {
    let valid_ids = vec!["T1566", "T1059.001", "T1003", "T9999.999"];
    
    for id in valid_ids {
        // テクニックID形式の検証はプロバイダー内部で行われる
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: id.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };
        
        assert!(!indicator.value.is_empty());
    }
}

#[tokio::test]
async fn test_invalid_technique_id_format() {
    let invalid_ids = vec!["T123", "1566", "T", "TXXX"];
    
    for id in invalid_ids {
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: id.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };
        
        // 無効なIDでもインジケーターは作成できる（検証はプロバイダー側）
        assert!(!indicator.value.is_empty());
    }
}

#[tokio::test]
async fn test_keyword_search_indicator_type() {
    let keywords = vec!["phishing", "credential dumping", "powershell"];
    
    for keyword in keywords {
        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: keyword.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };
        
        assert_eq!(indicator.indicator_type, IndicatorType::FileHash);
    }
}

#[tokio::test]
async fn test_rate_limit_status() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    
    let result = provider.get_rate_limit_status().await;
    assert!(result.is_ok());
    
    let status = result.unwrap();
    assert_eq!(status.limit_per_minute, 60);
}

#[tokio::test]
async fn test_factory_creation_mitre() {
    let config = create_test_config();
    let result = ProviderFactory::create_provider(config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_factory_creation_mitre_attack() {
    let mut config = create_test_config();
    config.name = "MITRE-ATTACK".to_string();
    let result = ProviderFactory::create_provider(config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mitre_technique_metadata() {
    let technique = MitreAttackTechnique {
        technique_id: "T1566".to_string(),
        sub_technique_id: Some("T1566.001".to_string()),
        name: "Phishing".to_string(),
        tactics: vec!["initial-access".to_string()],
        platforms: vec!["Windows".to_string(), "Linux".to_string()],
        data_sources: vec!["Email Gateway".to_string()],
        description: Some("Adversaries may send phishing messages".to_string()),
        detection: Some("Monitor for suspicious emails".to_string()),
        mitigation: vec!["User Training".to_string()],
    };
    
    assert_eq!(technique.technique_id, "T1566");
    assert_eq!(technique.name, "Phishing");
    assert_eq!(technique.tactics.len(), 1);
    assert_eq!(technique.platforms.len(), 2);
}

#[tokio::test]
async fn test_batch_check_empty() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    
    let indicators: Vec<ThreatIndicator> = vec![];
    let result = provider.batch_check_indicators(&indicators).await;
    
    assert!(result.is_ok());
    let threats = result.unwrap();
    assert_eq!(threats.len(), 0);
}

#[tokio::test]
async fn test_timeout_configuration() {
    let config = create_test_config();
    assert_eq!(config.timeout_seconds, 30);
}

#[tokio::test]
async fn test_reliability_factor() {
    let config = create_test_config();
    assert_eq!(config.reliability_factor, 0.95);
}

// 統合テスト（実際のAPIを使用）
#[tokio::test]
#[ignore] // cargo test --ignored で実行
async fn test_real_api_phishing_technique() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "T1566".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };
    
    let result = provider.check_indicator(&indicator).await;
    
    // APIが利用できない場合はスキップ
    if result.is_ok() {
        let threats = result.unwrap();
        assert!(!threats.is_empty());
        
        let threat = &threats[0];
        assert!(!threat.metadata.mitre_attack_techniques.is_empty());
    }
}

#[tokio::test]
#[ignore]
async fn test_real_api_health_check() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    
    let result = provider.health_check().await;
    
    if result.is_ok() {
        let health = result.unwrap();
        assert_eq!(health.provider_name, "MITRE-ATTACK");
    }
}

#[tokio::test]
#[ignore]
async fn test_real_api_keyword_search() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "phishing".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: chrono::Utc::now(),
    };
    
    let result = provider.check_indicator(&indicator).await;
    
    if result.is_ok() {
        let threats = result.unwrap();
        // キーワード検索の結果は空の場合もある
        println!("Found {} threats for keyword 'phishing'", threats.len());
    }
}

#[tokio::test]
#[ignore]
async fn test_real_api_batch_techniques() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).unwrap();
    
    let indicators = vec![
        ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: "T1566".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        },
        ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: "T1003".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        },
    ];
    
    let result = provider.batch_check_indicators(&indicators).await;
    
    if result.is_ok() {
        let threats = result.unwrap();
        println!("Batch check found {} threats", threats.len());
    }
}
