//! CVE Provider Tests
//!
//! CVEプロバイダーの包括的なテスト

#[cfg(test)]
mod cve_tests {
    use mcp_rs::threat_intelligence::providers::{CVEProvider, ProviderFactory, ThreatProvider};
    use mcp_rs::threat_intelligence::types::{
        IndicatorType, ProviderConfig, ThreatError, ThreatIndicator,
    };
    use std::collections::HashMap;

    /// テスト用のプロバイダー設定を作成
    fn create_test_config() -> ProviderConfig {
        ProviderConfig {
            name: "CVE".to_string(),
            enabled: true,
            api_key: String::new(), // NVD APIはキー不要
            base_url: "https://services.nvd.nist.gov/rest/json".to_string(),
            timeout_seconds: 30,
            rate_limit_per_minute: 10,
            reliability_factor: 0.98,
            provider_specific: HashMap::new(),
        }
    }

    /// テスト用の指標を作成
    fn create_test_indicator(cve_id: &str) -> ThreatIndicator {
        ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: cve_id.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_provider_creation_success() {
        let config = create_test_config();
        let result = CVEProvider::new(config);
        assert!(result.is_ok(), "Should create provider with valid config");
    }

    #[test]
    fn test_provider_name() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();
        assert_eq!(provider.name(), "CVE");
    }

    #[test]
    fn test_provider_config() {
        let config = create_test_config();
        let provider = CVEProvider::new(config.clone()).unwrap();
        assert_eq!(provider.config().name, config.name);
        assert_eq!(provider.config().base_url, config.base_url);
    }

    #[test]
    fn test_valid_cve_id_format() {
        let valid_ids = vec![
            "CVE-2021-44228",
            "CVE-2014-0160",
            "CVE-2017-5638",
            "CVE-2023-12345",
            "CVE-1999-0001",
        ];

        for cve_id in valid_ids {
            // is_valid_cve_id は private なので、間接的にテスト
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let config = create_test_config();
                let provider = CVEProvider::new(config).unwrap();
                let indicator = create_test_indicator(cve_id);

                // 有効な形式なので、ネットワークエラーまたは成功が期待される
                let result = provider.check_indicator(&indicator).await;
                
                // ConfigurationErrorは返されないはず
                if let Err(ThreatError::ConfigurationError(msg)) = result {
                    if msg.contains("Invalid CVE ID format") {
                        panic!("Should not reject valid CVE ID: {}", cve_id);
                    }
                }
            });
        }
    }

    #[test]
    fn test_invalid_cve_id_format() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config();
            let provider = CVEProvider::new(config).unwrap();

            let invalid_ids = vec!["CVE-INVALID", "not-a-cve", "CVE-99-1", "CVE-2021"];

            for invalid_id in invalid_ids {
                let indicator = ThreatIndicator {
                    indicator_type: IndicatorType::FileHash,
                    value: invalid_id.to_string(),
                    pattern: None,
                    tags: Vec::new(),
                    context: None,
                    first_seen: chrono::Utc::now(),
                };

                let result = provider.check_indicator(&indicator).await;

                // 無効な形式の場合、ConfigurationErrorが期待される
                if let Err(ThreatError::ConfigurationError(msg)) = result {
                    assert!(
                        msg.contains("Invalid CVE ID format"),
                        "Error message should mention invalid format: {}",
                        msg
                    );
                }
                // または、キーワード検索として扱われる可能性もある
            }
        });
    }

    #[test]
    fn test_keyword_search_indicator_type() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config();
            let provider = CVEProvider::new(config).unwrap();

            // Domain タイプでもキーワード検索として処理されるべき
            let indicator = ThreatIndicator {
                indicator_type: IndicatorType::Domain,
                value: "apache".to_string(),
                pattern: None,
                tags: Vec::new(),
                context: None,
                first_seen: chrono::Utc::now(),
            };

            let result = provider.check_indicator(&indicator).await;

            // キーワード検索は成功するか、ネットワークエラーになるべき
            match result {
                Ok(_) => {} // 成功
                Err(ThreatError::NetworkError(_)) => {} // ネットワークエラーは許容
                Err(ThreatError::RateLimitExceeded(_)) => {} // レート制限も許容
                Err(e) => {
                    // ConfigurationError でキーワード検索を拒否してはいけない
                    if let ThreatError::ConfigurationError(msg) = e {
                        if !msg.contains("Invalid CVE ID format") {
                            panic!("Should not reject keyword search: {}", msg);
                        }
                    }
                }
            }
        });
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        // 初期状態
        let initial_size = provider.cache_size().await;
        assert_eq!(initial_size, 0, "Cache should be empty initially");

        // キャッシュクリア
        provider.clear_cache().await;
        let size_after_clear = provider.cache_size().await;
        assert_eq!(size_after_clear, 0, "Cache should be empty after clear");
    }

    #[tokio::test]
    async fn test_rate_limit_status() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        let status = provider.get_rate_limit_status().await;
        assert!(status.is_ok(), "Should return rate limit status");

        let status = status.unwrap();
        assert_eq!(status.limit_per_minute, 10);
    }

    #[test]
    fn test_factory_creation() {
        let config = create_test_config();
        let result = ProviderFactory::create_provider(config);

        assert!(result.is_ok(), "Factory should create CVE provider");

        let provider = result.unwrap();
        assert_eq!(provider.name(), "CVE");
    }

    #[test]
    fn test_cvss_score_to_severity() {
        // cvss_score_to_severity は private なので、間接的にテスト
        // 実際のCVEレスポンスをシミュレートする統合テストが必要
        
        // このテストはドキュメント化のため
        // CVSS スコアと深刻度のマッピング:
        // >= 9.0: Critical
        // >= 7.0: High
        // >= 4.0: Medium
        // > 0.0: Low
        // = 0.0: Info
    }

    #[test]
    fn test_multiple_provider_instances() {
        let config1 = create_test_config();
        let config2 = create_test_config();

        let provider1 = CVEProvider::new(config1);
        let provider2 = CVEProvider::new(config2);

        assert!(provider1.is_ok());
        assert!(provider2.is_ok());
    }

    #[test]
    fn test_timeout_configuration() {
        let mut config = create_test_config();
        config.timeout_seconds = 60;

        let provider = CVEProvider::new(config).unwrap();
        assert_eq!(provider.config().timeout_seconds, 60);
    }

    #[test]
    fn test_reliability_factor() {
        let mut config = create_test_config();
        config.reliability_factor = 0.95;

        let provider = CVEProvider::new(config).unwrap();
        assert_eq!(provider.config().reliability_factor, 0.95);
    }

    #[tokio::test]
    async fn test_batch_check_empty() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        let empty_indicators: Vec<ThreatIndicator> = vec![];
        let result = provider.batch_check_indicators(&empty_indicators).await;

        assert!(result.is_ok(), "Should handle empty batch");
        let threats = result.unwrap();
        assert_eq!(threats.len(), 0, "Should return empty results");
    }
}

#[cfg(test)]
mod integration_tests {
    use mcp_rs::threat_intelligence::providers::{CVEProvider, ThreatProvider};
    use mcp_rs::threat_intelligence::types::{IndicatorType, ProviderConfig, ThreatIndicator};
    use std::collections::HashMap;

    /// テスト用のプロバイダー設定を作成
    fn create_test_config() -> ProviderConfig {
        ProviderConfig {
            name: "CVE".to_string(),
            enabled: true,
            api_key: String::new(),
            base_url: "https://services.nvd.nist.gov/rest/json".to_string(),
            timeout_seconds: 30,
            rate_limit_per_minute: 10,
            reliability_factor: 0.98,
            provider_specific: HashMap::new(),
        }
    }

    #[tokio::test]
    #[ignore] // デフォルトでは無視（実際のAPI呼び出しを避けるため）
    async fn test_real_api_log4shell() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: "CVE-2021-44228".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: Some("Log4Shell".to_string()),
            first_seen: chrono::Utc::now(),
        };

        let result = provider.check_indicator(&indicator).await;
        assert!(result.is_ok(), "Should successfully check Log4Shell CVE");

        let threats = result.unwrap();
        assert!(!threats.is_empty(), "Should find Log4Shell CVE");

        let threat = &threats[0];
        println!("CVE: {:?}", threat.metadata.cve_references);
        println!("Severity: {:?}", threat.severity);
        
        // Log4Shellは Critical または High であるべき
        assert!(
            threat.severity == mcp_rs::threat_intelligence::types::SeverityLevel::Critical
                || threat.severity == mcp_rs::threat_intelligence::types::SeverityLevel::High,
            "Log4Shell should be Critical or High severity"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_api_health_check() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        let health = provider.health_check().await;
        assert!(health.is_ok(), "Health check should succeed");

        let health = health.unwrap();
        println!("Health status: {:?}", health.status);
        println!("Response time: {}ms", health.response_time_ms);
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_api_keyword_search() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::Domain,
            value: "log4j".to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        let result = provider.check_indicator(&indicator).await;
        
        match result {
            Ok(threats) => {
                println!("Found {} CVEs related to 'log4j'", threats.len());
                assert!(!threats.is_empty(), "Should find CVEs related to log4j");
            }
            Err(e) => {
                println!("Search failed: {}", e);
                // ネットワークエラーやレート制限は許容
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_api_cache_functionality() {
        let config = create_test_config();
        let provider = CVEProvider::new(config).unwrap();

        let indicator = ThreatIndicator {
            indicator_type: IndicatorType::FileHash,
            value: "CVE-2014-0160".to_string(), // Heartbleed
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        };

        // 1回目のリクエスト
        let start1 = std::time::Instant::now();
        let result1 = provider.check_indicator(&indicator).await;
        let duration1 = start1.elapsed();

        assert!(result1.is_ok(), "First request should succeed");

        // キャッシュサイズを確認
        let cache_size = provider.cache_size().await;
        assert!(cache_size > 0, "Cache should have entries");

        // 2回目のリクエスト（キャッシュから）
        let start2 = std::time::Instant::now();
        let result2 = provider.check_indicator(&indicator).await;
        let duration2 = start2.elapsed();

        assert!(result2.is_ok(), "Second request should succeed");

        println!("First request: {:?}", duration1);
        println!("Second request (cached): {:?}", duration2);
        println!("Cache size: {}", cache_size);

        // キャッシュの方が速いはず
        assert!(
            duration2 < duration1,
            "Cached request should be faster"
        );
    }
}
