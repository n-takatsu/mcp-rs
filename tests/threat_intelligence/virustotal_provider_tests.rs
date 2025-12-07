//! VirusTotal Provider Tests
//!
//! VirusTotalプロバイダーの単体テストと統合テスト

use chrono::Utc;
use mcp_rs::threat_intelligence::{
    HealthStatus, IndicatorType, ProviderConfig, ProviderFactory, SeverityLevel, ThreatIndicator,
    ThreatProvider, VirusTotalProvider,
};
use std::collections::HashMap;

// テスト用のダミー設定
fn create_test_config() -> ProviderConfig {
    ProviderConfig {
        name: "VirusTotal".to_string(),
        enabled: true,
        api_key: "test_api_key_12345".to_string(),
        base_url: "https://www.virustotal.com/api/v3".to_string(),
        timeout_seconds: 15,
        rate_limit_per_minute: 4,
        reliability_factor: 0.98,
        provider_specific: HashMap::new(),
    }
}

#[test]
fn test_virustotal_provider_creation() {
    let config = create_test_config();
    let provider = VirusTotalProvider::new(config).expect("Failed to create provider");

    assert_eq!(provider.name(), "VirusTotal");
}

#[test]
fn test_virustotal_provider_creation_without_api_key() {
    let mut config = create_test_config();
    config.api_key = String::new(); // 空のAPIキー

    let result = VirusTotalProvider::new(config);
    assert!(result.is_err());

    if let Err(e) = result {
        let error_msg = format!("{}", e);
        assert!(error_msg.contains("API key is required"));
    }
}

#[test]
fn test_virustotal_provider_from_factory() {
    let config = create_test_config();
    let provider = ProviderFactory::create_provider(config).expect("Failed to create from factory");

    assert_eq!(provider.name(), "VirusTotal");
}

#[test]
fn test_virustotal_provider_config_validation() {
    let config = create_test_config();
    let provider = VirusTotalProvider::new(config.clone()).unwrap();

    assert_eq!(provider.config().name, "VirusTotal");
    assert!(provider.config().enabled);
    assert_eq!(provider.config().reliability_factor, 0.98);
}

#[test]
fn test_threat_indicator_domain_creation() {
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: "malicious.example.com".to_string(),
        pattern: None,
        tags: vec!["phishing".to_string()],
        context: Some("Known phishing domain".to_string()),
        first_seen: Utc::now(),
    };

    assert_eq!(indicator.indicator_type, IndicatorType::Domain);
    assert_eq!(indicator.value, "malicious.example.com");
    assert_eq!(indicator.tags.len(), 1);
}

#[test]
fn test_threat_indicator_ip_creation() {
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: "192.0.2.1".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: Utc::now(),
    };

    assert_eq!(indicator.indicator_type, IndicatorType::IpAddress);
    assert_eq!(indicator.value, "192.0.2.1");
}

#[test]
fn test_threat_indicator_url_creation() {
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Url,
        value: "https://malicious.example.com/malware.exe".to_string(),
        pattern: None,
        tags: vec!["malware".to_string()],
        context: Some("Malware distribution URL".to_string()),
        first_seen: Utc::now(),
    };

    assert_eq!(indicator.indicator_type, IndicatorType::Url);
    assert!(indicator.value.starts_with("https://"));
}

#[test]
fn test_threat_indicator_file_hash_creation() {
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "44d88612fea8a8f36de82e1278abb02f".to_string(),
        pattern: None,
        tags: vec!["md5".to_string(), "eicar".to_string()],
        context: Some("EICAR test file".to_string()),
        first_seen: Utc::now(),
    };

    assert_eq!(indicator.indicator_type, IndicatorType::FileHash);
    assert_eq!(indicator.value.len(), 32); // MD5 hash length
}

// ===== 統合テスト (実際のAPIキーが必要) =====

#[tokio::test]
#[ignore] // 実APIキーが必要なため通常は無視
async fn test_virustotal_health_check_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let health = provider.health_check().await.expect("Health check failed");

    assert_eq!(health.status, HealthStatus::Healthy);
    assert!(health.response_time_ms > 0);
}

#[tokio::test]
#[ignore]
async fn test_virustotal_check_malicious_domain_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: "027.ru".to_string(), // 既知の悪意のあるドメイン
        pattern: None,
        tags: Vec::new(),
        context: Some("Known malicious domain".to_string()),
        first_seen: Utc::now(),
    };

    let threats = provider
        .check_indicator(&indicator)
        .await
        .expect("Failed to check domain");

    // 悪意のあるドメインなので脅威が検出されるべき
    assert!(!threats.is_empty(), "Expected threats to be detected");

    for threat in &threats {
        assert!(threat.confidence_score > 0.0);
        assert!(matches!(
            threat.severity,
            SeverityLevel::Critical
                | SeverityLevel::High
                | SeverityLevel::Medium
                | SeverityLevel::Low
        ));
    }
}

#[tokio::test]
#[ignore]
async fn test_virustotal_check_safe_domain_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Domain,
        value: "google.com".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: Utc::now(),
    };

    let threats = provider
        .check_indicator(&indicator)
        .await
        .expect("Failed to check domain");

    // 正規のドメインなので脅威は少ないはず（または無し）
    if !threats.is_empty() {
        for threat in &threats {
            // 信頼度が低いか、深刻度が低いはず
            assert!(
                threat.confidence_score < 0.5 || threat.severity == SeverityLevel::Info,
                "Expected low threat for legitimate domain"
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_virustotal_check_ip_address_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::IpAddress,
        value: "1.1.1.1".to_string(), // Cloudflare DNS
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: Utc::now(),
    };

    let result = provider.check_indicator(&indicator).await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore]
async fn test_virustotal_check_url_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::Url,
        value: "https://example.com/".to_string(),
        pattern: None,
        tags: Vec::new(),
        context: None,
        first_seen: Utc::now(),
    };

    let result = provider.check_indicator(&indicator).await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore]
async fn test_virustotal_check_file_hash_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    // EICAR test file MD5
    let indicator = ThreatIndicator {
        indicator_type: IndicatorType::FileHash,
        value: "44d88612fea8a8f36de82e1278abb02f".to_string(),
        pattern: None,
        tags: vec!["md5".to_string()],
        context: Some("EICAR test file".to_string()),
        first_seen: Utc::now(),
    };

    let threats = provider
        .check_indicator(&indicator)
        .await
        .expect("Failed to check file hash");

    // EICAR test fileなので脅威が検出されるべき
    assert!(!threats.is_empty(), "Expected threats for EICAR test file");
}

#[tokio::test]
#[ignore]
async fn test_virustotal_rate_limit_status_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let status = provider
        .get_rate_limit_status()
        .await
        .expect("Failed to get rate limit status");

    assert!(status.limit_per_minute > 0);
}

#[tokio::test]
#[ignore]
async fn test_virustotal_batch_check_integration() {
    let api_key = std::env::var("VIRUSTOTAL_API_KEY").expect("VIRUSTOTAL_API_KEY not set");

    let mut config = create_test_config();
    config.api_key = api_key;

    let provider = VirusTotalProvider::new(config).unwrap();

    let indicators = vec![
        ThreatIndicator {
            indicator_type: IndicatorType::Domain,
            value: "google.com".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: Utc::now(),
        },
        ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: "1.1.1.1".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: Utc::now(),
        },
    ];

    let result = provider.batch_check_indicators(&indicators).await;
    assert!(result.is_ok());
}
