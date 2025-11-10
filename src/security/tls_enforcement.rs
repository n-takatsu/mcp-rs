#[cfg(test)]
mod security_tests {
    use crate::config::{RateLimitConfig, WordPressConfig};
    use crate::handlers::WordPressHandler;

    #[test]
    fn test_https_enforcement() {
        // HTTP URLは拒否される
        let insecure_config = WordPressConfig {
            url: "http://example.com".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
        };

        let result = WordPressHandler::try_new(insecure_config);
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Insecure URL detected"));
        assert!(error_msg.contains("Only HTTPS connections are allowed"));
    }

    #[test]
    fn test_https_allowed() {
        // HTTPS URLは許可される
        let secure_config = WordPressConfig {
            url: "https://secure.example.com".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
        };

        let result = WordPressHandler::try_new(secure_config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_mixed_case_https() {
        // 大文字小文字を問わずHTTPS
        let config = WordPressConfig {
            url: "HTTPS://EXAMPLE.COM".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
        };

        // この場合は失敗する（小文字のhttps://のみを受け入れる）
        let result = WordPressHandler::try_new(config);
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_url() {
        // 不正なURL形式
        let malformed_configs = vec![
            "ftp://example.com",
            "tcp://example.com",
            "ws://example.com",
            "wss://example.com", // WebSocketも拒否
            "",
            "not-a-url",
        ];

        for url in malformed_configs {
            let config = WordPressConfig {
                url: url.to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                enabled: Some(true),
                timeout_seconds: Some(30),
                rate_limit: Some(RateLimitConfig::default()),
                encrypted_credentials: None,
            };

            let result = WordPressHandler::try_new(config);
            assert!(result.is_err(), "URL {} should be rejected", url);
        }
    }

    #[test]
    fn test_security_message_content() {
        let config = WordPressConfig {
            url: "http://insecure.site.com".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            enabled: Some(true),
            timeout_seconds: Some(30),
            rate_limit: Some(RateLimitConfig::default()),
            encrypted_credentials: None,
        };

        let result = WordPressHandler::try_new(config);
        let error_msg = result.unwrap_err();

        // セキュリティメッセージが適切に含まれているか確認
        assert!(error_msg.contains("Insecure URL detected"));
        assert!(error_msg.contains("http://insecure.site.com"));
        assert!(error_msg.contains("Only HTTPS connections are allowed"));
        assert!(error_msg.contains("security reasons"));
    }
}
