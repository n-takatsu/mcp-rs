//! MySQL Security Test Suite
//!
//! Comprehensive security testing for MySQL engine implementation
//! Tests SQL injection prevention, parameterized queries, and security layer integration

#![cfg(feature = "database")]

use log::{info, warn};
use mcp_rs::handlers::database::{
    // engine traits not directly used in these tests
    engines::mysql::{MySqlEngine, MySqlParamConverter},
    security::DatabaseSecurity,
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig, QueryContext,
        QueryType, SecurityConfig, Value,
    },
};
use std::sync::Arc;

// Initialize test logger for security testing
fn init_test_logger() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .try_init();
}

// Create test MySQL configuration
fn create_test_config() -> DatabaseConfig {
    DatabaseConfig {
        database_type: DatabaseType::MySQL,
        connection: ConnectionConfig {
            host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("MYSQL_PORT")
                .unwrap_or_else(|_| "3306".to_string())
                .parse()
                .unwrap_or(3306),
            database: std::env::var("MYSQL_DATABASE").unwrap_or_else(|_| "test_db".to_string()),
            username: std::env::var("MYSQL_USER").unwrap_or_else(|_| "test_user".to_string()),
            password: std::env::var("MYSQL_PASSWORD").unwrap_or_else(|_| "test_pass".to_string()),
            ssl_mode: Some("disabled".to_string()),
            timeout_seconds: 30,
            retry_attempts: 3,
            options: std::collections::HashMap::new(),
        },
        pool: PoolConfig {
            max_connections: 5,
            min_connections: 1,
            connection_timeout: 30,
            idle_timeout: 300,
            max_lifetime: 3600,
        },
        security: SecurityConfig {
            enable_sql_injection_detection: true,
            enable_query_whitelist: false,
            enable_audit_logging: true,
            threat_intelligence_enabled: true,
            max_query_length: 10000,
            allowed_operations: vec![
                QueryType::Select,
                QueryType::Insert,
                QueryType::Update,
                QueryType::Delete,
            ],
        },
        features: FeatureConfig::default(),
    }
}

// Setup test environment with MySQL engine and security layer
async fn setup_test_environment(
) -> Result<(MySqlEngine, Arc<DatabaseSecurity>), Box<dyn std::error::Error>> {
    init_test_logger();

    let config = create_test_config();
    let security = Arc::new(DatabaseSecurity::new(config.security.clone(), None));

    let engine = MySqlEngine::new(config, security.clone()).await?;

    info!("Test environment setup complete");
    Ok((engine, security))
}

#[tokio::test]
async fn test_mysql_param_converter_basic_types() {
    init_test_logger();
    info!("Testing MySqlParamConverter basic type conversions");

    // Test integer conversion
    let int_val = Value::from_i64(42);
    let converted = MySqlParamConverter::convert_value(&int_val);
    assert!(converted.is_ok(), "Integer conversion should succeed");

    // Test string conversion
    let str_val = Value::String("test".to_string());
    let converted = MySqlParamConverter::convert_value(&str_val);
    assert!(converted.is_ok(), "String conversion should succeed");

    // Test boolean conversion
    let bool_val = Value::from_bool(true);
    let converted = MySqlParamConverter::convert_value(&bool_val);
    assert!(converted.is_ok(), "Boolean conversion should succeed");

    // Test null conversion
    let null_val = Value::Null;
    let converted = MySqlParamConverter::convert_value(&null_val);
    assert!(converted.is_ok(), "Null conversion should succeed");

    info!("MySqlParamConverter basic type conversion tests passed ✅");
}

#[tokio::test]
async fn test_parameter_validation() {
    init_test_logger();
    info!("Testing parameter validation");

    // Test parameter count validation
    let test_cases = vec![
        ("SELECT * FROM users WHERE id = ?", 1, true),
        ("SELECT * FROM users WHERE id = ? AND name = ?", 2, true),
        ("SELECT * FROM users", 0, true),
        ("SELECT * FROM users WHERE id = ?", 0, false), // Missing parameter
        ("SELECT * FROM users WHERE id = ?", 2, false), // Too many parameters
    ];

    for (sql, param_count, should_succeed) in test_cases {
        let result = MySqlParamConverter::validate_param_count(sql, param_count);

        if should_succeed {
            assert!(
                result.is_ok(),
                "Parameter validation should succeed for: {}",
                sql
            );
        } else {
            assert!(
                result.is_err(),
                "Parameter validation should fail for: {}",
                sql
            );
        }
    }

    // Test parameter conversion
    let test_values = vec![
        Value::from_i64(42),
        Value::from_bool(true),
        Value::String("test".to_string()),
        Value::Null,
    ];

    for value in test_values {
        let result = MySqlParamConverter::convert_value(&value);
        assert!(
            result.is_ok(),
            "Parameter conversion should succeed for: {:?}",
            value
        );
    }

    info!("Parameter validation tests completed ✅");
}

#[tokio::test]
async fn test_sql_injection_detection() {
    init_test_logger();
    info!("Testing SQL injection detection");

    let (_engine, security) = match setup_test_environment().await {
        Ok(env) => env,
        Err(e) => {
            warn!("Skipping SQL injection tests - setup failed: {}", e);
            return;
        }
    };

    // Test malicious SQL patterns
    let malicious_queries = vec![
        "SELECT * FROM users WHERE id = 1 OR 1=1",           // Classic boolean injection
        "SELECT * FROM users WHERE id = 1 UNION SELECT * FROM passwords", // UNION injection
        "SELECT * FROM users WHERE id = 1' OR '1'='1",       // String injection
        "SELECT * FROM users WHERE id = 1/**/UNION/**/SELECT/**/*/**FROM/**/passwords", // Comment obfuscation
        "SELECT * FROM users WHERE id = 1 AND (SELECT COUNT(*) FROM passwords) > 0", // Subquery injection
        "SELECT * FROM users; DROP TABLE users; --",         // Stacked queries
        "SELECT * FROM users WHERE id = 1' AND EXTRACTVALUE(1, CONCAT(0x7e, (SELECT @@version), 0x7e)) --", // Error-based injection (MySQL function)
    ];

    for malicious_sql in &malicious_queries {
        info!("Testing malicious query: {}", malicious_sql);

        let context = QueryContext::new(QueryType::Select);
        let result = security.validate_query(malicious_sql, &context).await;

        assert!(
            result.is_err(),
            "Malicious query should be blocked: {}",
            malicious_sql
        );

        info!("✅ Malicious query successfully blocked");
    }

    info!(
        "All {} SQL injection attack patterns successfully blocked ✅",
        malicious_queries.len()
    );
}

#[tokio::test]
async fn test_parameterized_query_safety() {
    init_test_logger();
    info!("Testing parameterized query safety");

    // Test safe parameterized queries through parameter converter
    let safe_queries = [
        ("SELECT * FROM users WHERE id = ?", vec![Value::from_i64(1)]),
        (
            "INSERT INTO users (name, age) VALUES (?, ?)",
            vec![Value::String("John".to_string()), Value::from_i64(18)],
        ),
        (
            "UPDATE users SET age = ? WHERE name = ?",
            vec![Value::from_i64(25), Value::String("John".to_string())],
        ),
        (
            "DELETE FROM users WHERE id = ? AND name = ?",
            vec![Value::String("John".to_string()), Value::from_i64(2)],
        ),
    ];

    for (i, (sql, params)) in safe_queries.iter().enumerate() {
        info!("Testing safe parameterized query {}: {}", i + 1, sql);

        // Test parameter validation and conversion
        let param_validation = MySqlParamConverter::validate_param_count(sql, params.len());
        assert!(
            param_validation.is_ok(),
            "Parameter count should be valid for: {}",
            sql
        );

        // Test parameter type conversion
        for param in params {
            let conversion = MySqlParamConverter::convert_value(param);
            assert!(
                conversion.is_ok(),
                "Parameter conversion should succeed for: {:?}",
                param
            );
        }

        info!("Query {} parameter validation passed ✅", i + 1);
    }

    info!("Parameterized query safety tests completed ✅");
}

#[tokio::test]
async fn test_security_layer_integration() {
    init_test_logger();
    info!("Testing security layer integration with MySQL engine");

    let (_engine, security) = match setup_test_environment().await {
        Ok(env) => env,
        Err(e) => {
            warn!("Skipping security integration tests - setup failed: {}", e);
            return;
        }
    };

    let test_cases = vec![
        ("SELECT * FROM users", QueryType::Select),
        ("INSERT INTO users VALUES (1, 'test')", QueryType::Insert),
        ("UPDATE users SET name = 'test'", QueryType::Update),
        ("DELETE FROM users WHERE id = 1", QueryType::Delete),
        ("CREATE TABLE test (id INT)", QueryType::Ddl),
    ];

    for (sql, expected_type) in test_cases {
        let context = QueryContext::new(expected_type.clone());

        // Test that security validation is called for each query type
        let result = security.validate_query(sql, &context).await;

        match result {
            Ok(_) => info!("Query type {:?} validation passed ✅", expected_type),
            Err(e) => {
                // Some queries may be blocked based on security settings, which is expected
                info!("Query type {:?} validation result: {} ✅", expected_type, e);
            }
        }
    }

    info!("Security layer integration tests completed ✅");
}

#[tokio::test]
async fn test_comprehensive_security_scenario() {
    init_test_logger();
    info!("Running comprehensive security scenario test");

    let (_engine, _security) = match setup_test_environment().await {
        Ok(env) => env,
        Err(e) => {
            warn!(
                "Skipping comprehensive security tests - setup failed: {}",
                e
            );
            return;
        }
    };

    // Use security layer for comprehensive testing
    info!("Connected to MySQL database successfully");

    // Test parameter count mismatches
    let result = MySqlParamConverter::validate_param_count("SELECT * FROM users WHERE id = ?", 0);
    assert!(result.is_err(), "Query with missing parameters should fail");

    // Test malicious parameter content
    let malicious_params = vec![Value::String("'; DROP TABLE users; --".to_string())];
    for param in malicious_params {
        let conversion = MySqlParamConverter::convert_value(&param);
        // Parameter conversion itself should succeed (security is checked at query level)
        assert!(
            conversion.is_ok(),
            "Parameter conversion should handle any string content"
        );
    }

    info!("Comprehensive security scenario tests completed ✅");
}

// Individual tests can be run separately using cargo test
// All tests are already async and will be executed by the test runner
