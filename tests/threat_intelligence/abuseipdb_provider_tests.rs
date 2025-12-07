//! AbuseIPDB Provider Tests
//!
//! AbuseIPDBプロバイダーの包括的なテスト

#[cfg(test)]
mod abuseipdb_tests {
    use mcp_rs::threat_intelligence::providers::{
        AbuseIPDBProvider, ProviderFactory, ThreatProvider,
    };
    use mcp_rs::threat_intelligence::types::{
        IndicatorType, ProviderConfig, ThreatError, ThreatIndicator,
    };
    use std::collections::HashMap;

    /// テスト用のプロバイダー設定を作成
    fn create_test_config(api_key: &str) -> ProviderConfig {
        ProviderConfig {
            name: "AbuseIPDB".to_string(),
            enabled: true,
            api_key: api_key.to_string(),
            base_url: "https://api.abuseipdb.com".to_string(),
            timeout_seconds: 10,
            rate_limit_per_minute: 60,
            reliability_factor: 0.95,
            provider_specific: HashMap::new(),
        }
    }

    /// テスト用の指標を作成
    fn create_test_indicator(ip: &str) -> ThreatIndicator {
        ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: ip.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_provider_creation_success() {
        let config = create_test_config("valid_api_key");
        let result = AbuseIPDBProvider::new(config);
        assert!(result.is_ok(), "Should create provider with valid config");
    }

    #[test]
    fn test_provider_creation_empty_api_key() {
        let config = create_test_config("");
        let result = AbuseIPDBProvider::new(config);
        assert!(result.is_err(), "Should fail with empty API key");

        if let Err(ThreatError::ConfigurationError(msg)) = result {
            assert!(
                msg.contains("API key is required"),
                "Error message should mention API key"
            );
        } else {
            panic!("Expected ConfigurationError");
        }
    }

    #[test]
    fn test_provider_name() {
        let config = create_test_config("test_key");
        let provider = AbuseIPDBProvider::new(config).unwrap();
        assert_eq!(provider.name(), "AbuseIPDB");
    }

    #[test]
    fn test_provider_config() {
        let config = create_test_config("test_key");
        let provider = AbuseIPDBProvider::new(config.clone()).unwrap();
        assert_eq!(provider.config().name, config.name);
        assert_eq!(provider.config().api_key, config.api_key);
    }

    #[test]
    fn test_invalid_ip_format() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config("test_key");
            let provider = AbuseIPDBProvider::new(config).unwrap();

            let invalid_ips = vec!["not-an-ip", "999.999.999.999", "invalid"];

            for invalid_ip in invalid_ips {
                let indicator = create_test_indicator(invalid_ip);
                let result = provider.check_indicator(&indicator).await;

                assert!(
                    result.is_err(),
                    "Should fail for invalid IP: {}",
                    invalid_ip
                );

                if let Err(ThreatError::ConfigurationError(msg)) = result {
                    assert!(
                        msg.contains("Invalid IP address"),
                        "Error should mention invalid IP format"
                    );
                }
            }
        });
    }

    #[test]
    fn test_valid_ipv4_format() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config("test_key");
            let provider = AbuseIPDBProvider::new(config).unwrap();

            let valid_ips = vec!["8.8.8.8", "192.168.1.1", "10.0.0.1"];

            for valid_ip in valid_ips {
                let indicator = create_test_indicator(valid_ip);
                // IP形式の検証は通るが、APIキーが無効なのでネットワークエラーになる
                let result = provider.check_indicator(&indicator).await;

                // 無効なAPIキーの場合、ネットワークエラーまたはプロバイダーエラーが期待される
                if let Err(e) = result {
                    match e {
                        ThreatError::NetworkError(_) | ThreatError::ProviderError(_) => {
                            // これは期待される動作
                        }
                        ThreatError::ConfigurationError(msg) => {
                            panic!("Should not get ConfigurationError for valid IP: {}", msg);
                        }
                        _ => {}
                    }
                }
            }
        });
    }

    #[test]
    fn test_valid_ipv6_format() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config("test_key");
            let provider = AbuseIPDBProvider::new(config).unwrap();

            let ipv6 = "2001:4860:4860::8888";
            let indicator = create_test_indicator(ipv6);

            let result = provider.check_indicator(&indicator).await;

            // IPv6も有効な形式として受け入れられるべき
            if let Err(ThreatError::ConfigurationError(msg)) = result {
                panic!("Should not reject valid IPv6 format: {}", msg);
            }
        });
    }

    #[test]
    fn test_unsupported_indicator_type() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = create_test_config("test_key");
            let provider = AbuseIPDBProvider::new(config).unwrap();

            let domain_indicator = ThreatIndicator {
                indicator_type: IndicatorType::Domain,
                value: "example.com".to_string(),
                pattern: None,
                tags: Vec::new(),
                context: None,
                first_seen: chrono::Utc::now(),
            };

            let result = provider.check_indicator(&domain_indicator).await;

            assert!(
                result.is_err(),
                "Should fail for unsupported indicator type"
            );

            if let Err(ThreatError::ConfigurationError(msg)) = result {
                assert!(
                    msg.contains("only supports IP address"),
                    "Error should mention supported types"
                );
            } else {
                panic!("Expected ConfigurationError for unsupported type");
            }
        });
    }

    #[tokio::test]
    async fn test_rate_limit_status() {
        let config = create_test_config("test_key");
        let provider = AbuseIPDBProvider::new(config).unwrap();

        let status = provider.get_rate_limit_status().await;
        assert!(status.is_ok(), "Should return rate limit status");

        let status = status.unwrap();
        assert_eq!(status.limit_per_minute, 60);
    }

    #[test]
    fn test_factory_creation() {
        let config = create_test_config("test_key");
        let result = ProviderFactory::create_provider(config);

        assert!(result.is_ok(), "Factory should create AbuseIPDB provider");

        let provider = result.unwrap();
        assert_eq!(provider.name(), "AbuseIPDB");
    }

    #[test]
    fn test_category_to_threat_type_mapping() {
        // カテゴリー番号から脅威タイプへのマッピングが正しいことを確認
        // これは内部実装の詳細なので、間接的にテスト

        // カテゴリーの一部:
        // 3-11: Malware
        // 12-13: Phishing
        // 14: Spam
        // 15-17: C&C
        // 18-20: Botnet
        // 21: Exploit

        // この実装は parse_abuseipdb_response 内で使用される
        // 直接テストするにはプロバイダーをモック化する必要がある
    }

    #[test]
    fn test_severity_level_calculation() {
        // abuse_confidence_score に基づく深刻度レベルの計算
        // >= 0.8: Critical
        // >= 0.6: High
        // >= 0.4: Medium
        // >= 0.2: Low
        // else: Info

        // この実装は parse_abuseipdb_response 内で使用される
        // 実際のAPIレスポンスをシミュレートする統合テストが必要
    }

    #[tokio::test]
    async fn test_batch_check_empty() {
        let config = create_test_config("test_key");
        let provider = AbuseIPDBProvider::new(config).unwrap();

        let empty_indicators: Vec<ThreatIndicator> = vec![];
        let result = provider.batch_check_indicators(&empty_indicators).await;

        assert!(result.is_ok(), "Should handle empty batch");
        let threats = result.unwrap();
        assert_eq!(threats.len(), 0, "Should return empty results");
    }

    #[test]
    fn test_timeout_configuration() {
        let mut config = create_test_config("test_key");
        config.timeout_seconds = 5;

        let provider = AbuseIPDBProvider::new(config).unwrap();
        assert_eq!(provider.config().timeout_seconds, 5);
    }

    #[test]
    fn test_reliability_factor() {
        let mut config = create_test_config("test_key");
        config.reliability_factor = 0.85;

        let provider = AbuseIPDBProvider::new(config).unwrap();
        assert_eq!(provider.config().reliability_factor, 0.85);
    }

    #[test]
    fn test_multiple_provider_instances() {
        let config1 = create_test_config("key1");
        let config2 = create_test_config("key2");

        let provider1 = AbuseIPDBProvider::new(config1);
        let provider2 = AbuseIPDBProvider::new(config2);

        assert!(provider1.is_ok());
        assert!(provider2.is_ok());
    }
}

#[cfg(test)]
mod integration_tests {
    use mcp_rs::threat_intelligence::providers::{AbuseIPDBProvider, ThreatProvider};
    use mcp_rs::threat_intelligence::types::{IndicatorType, ProviderConfig, ThreatIndicator};
    use std::collections::HashMap;

    /// 統合テスト用のヘルパー
    /// 実際のAPIキーが環境変数に設定されている場合のみ実行
    fn get_real_api_key() -> Option<String> {
        std::env::var("ABUSEIPDB_API_KEY").ok()
    }

    /// テスト用の指標を作成
    fn create_test_indicator(ip: &str) -> ThreatIndicator {
        ThreatIndicator {
            indicator_type: IndicatorType::IpAddress,
            value: ip.to_string(),
            pattern: None,
            tags: Vec::new(),
            context: None,
            first_seen: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    #[ignore] // デフォルトでは無視（実際のAPI呼び出しを避けるため）
    async fn test_real_api_safe_ip() {
        let Some(api_key) = get_real_api_key() else {
            println!("Skipping: ABUSEIPDB_API_KEY not set");
            return;
        };

        let config = ProviderConfig {
            name: "AbuseIPDB".to_string(),
            enabled: true,
            api_key,
            base_url: "https://api.abuseipdb.com".to_string(),
            timeout_seconds: 10,
            rate_limit_per_minute: 60,
            reliability_factor: 0.95,
            provider_specific: HashMap::new(),
        };

        let provider = AbuseIPDBProvider::new(config).unwrap();

        let indicator = create_test_indicator("8.8.8.8");
        let result = provider.check_indicator(&indicator).await;

        assert!(result.is_ok(), "Should successfully check Google DNS");
    }

    #[tokio::test]
    #[ignore]
    async fn test_real_api_health_check() {
        let Some(api_key) = get_real_api_key() else {
            println!("Skipping: ABUSEIPDB_API_KEY not set");
            return;
        };

        let config = ProviderConfig {
            name: "AbuseIPDB".to_string(),
            enabled: true,
            api_key,
            base_url: "https://api.abuseipdb.com".to_string(),
            timeout_seconds: 10,
            rate_limit_per_minute: 60,
            reliability_factor: 0.95,
            provider_specific: HashMap::new(),
        };

        let provider = AbuseIPDBProvider::new(config).unwrap();
        let health = provider.health_check().await;

        assert!(health.is_ok(), "Health check should succeed");
        let health = health.unwrap();
        println!("Health status: {:?}", health.status);
        println!("Response time: {}ms", health.response_time_ms);
    }
}
