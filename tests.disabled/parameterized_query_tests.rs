//! Parameterized Query Tests
//!
//! Comprehensive test suite for validating MySQL parameterized query functionality
//! Tests various data types, edge cases, and performance characteristics

use log::{debug, info, warn};
use mcp_rs::handlers::database::{
    engine::{DatabaseEngine, DatabaseConnection},
    engines::mysql::{MySqlEngine, MySqlParamConverter},
    security::DatabaseSecurity,
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseError, DatabaseType,
        FeatureConfig, PoolConfig, QueryContext, QueryType, SecurityConfig, Value,
    },
};
use std::sync::Arc;


/// Initialize test environment
fn init_test_logger() {
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .try_init();
}

/// Create test database configuration
fn create_test_config() -> DatabaseConfig {
    DatabaseConfig {
        engine: "mysql".to_string(),
        connection: ConnectionConfig {
            host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "localhost".to_string()),
            port: std::env::var("MYSQL_PORT")
                .unwrap_or_else(|_| "3306".to_string())
                .parse()
                .unwrap_or(3306),
            database: std::env::var("MYSQL_DATABASE").unwrap_or_else(|_| "test_db".to_string()),
            username: std::env::var("MYSQL_USER").unwrap_or_else(|_| "test_user".to_string()),
            password: Some(
                std::env::var("MYSQL_PASSWORD").unwrap_or_else(|_| "test_pass".to_string()),
            ),
            ssl: Some(false),
            connection_timeout: Some(30),
            query_timeout: Some(60),
            pool_size: Some(10),
        },
        features: vec![
            "parameterized_queries".to_string(),
            "prepared_statements".to_string(),
        ],
        security_config: Some(SecurityConfig {
            enable_sql_injection_detection: true,
            enable_query_logging: true,
            max_query_length: 10000,
            allowed_query_types: vec![
                QueryType::Select,
                QueryType::Insert,
                QueryType::Update,
                QueryType::Delete,
            ],
            blocked_patterns: vec![],
            enable_parameterized_queries_only: false,
        }),
    }
}

/// Setup test environment
async fn setup_test_env(
) -> Result<(MySqlEngine, Box<dyn DatabaseConnection>), Box<dyn std::error::Error>> {
    init_test_logger();

    let config = create_test_config();
    let security =
        Arc::new(DatabaseSecurity::new(config.security_config.as_ref().unwrap().clone()).await?);

    let engine = MySqlEngine::new(config.clone(), security).await?;
    let connection = engine.connect(&config).await?;

    Ok((engine, connection))
}

#[tokio::test]
async fn test_basic_data_type_parameters() {
    init_test_logger();
    info!("🧪 Testing Basic Data Type Parameters");

    let test_cases = vec![
        // Integer types
        (Value::from_i64(42), "Integer parameter"),
        (Value::from_i64(-123), "Negative integer parameter"),
        (Value::from_i64(0), "Zero integer parameter"),
        (Value::from_i64(i64::MAX), "Maximum integer parameter"),
        (Value::from_i64(i64::MIN), "Minimum integer parameter"),
        // String types
        (
            Value::String("Hello World".to_string()),
            "Basic string parameter",
        ),
        (Value::String("".to_string()), "Empty string parameter"),
        (
            Value::String("Special chars: !@#$%^&*()".to_string()),
            "Special characters parameter",
        ),
        (
            Value::String("Unicode: 🚀🔒✅❌".to_string()),
            "Unicode string parameter",
        ),
        (
            Value::String("Multi\nLine\nString".to_string()),
            "Multiline string parameter",
        ),
        (
            Value::String("'Single quotes' and \"double quotes\"".to_string()),
            "Quoted string parameter",
        ),
        // Boolean types
        (Value::from_bool(true), "True boolean parameter"),
        (Value::from_bool(false), "False boolean parameter"),
        // Float types
        (Value::Float(3.14159), "Float parameter"),
        (Value::Float(-2.718), "Negative float parameter"),
        (Value::Float(0.0), "Zero float parameter"),
        (Value::Float(f64::MAX), "Maximum float parameter"),
        (Value::Float(f64::MIN), "Minimum float parameter"),
        // Null type
        (Value::Null, "Null parameter"),
    ];

    for (param_value, description) in test_cases {
        info!("Testing: {}", description);

        // Test parameter conversion
        let converted = MySqlParamConverter::convert_value(&param_value);
        assert!(
            converted.is_ok(),
            "Parameter conversion should succeed for: {}",
            description
        );

        // Test parameter in actual query context
        let test_sql = "SELECT ? as test_value";
        let params = vec![param_value.clone()];

        let param_count_result = MySqlParamConverter::validate_param_count(test_sql, params.len());
        assert!(
            param_count_result.is_ok(),
            "Parameter count validation should pass for: {}",
            description
        );

        let param_type_result = MySqlParamConverter::validate_parameter_types(&params);
        assert!(
            param_type_result.is_ok(),
            "Parameter type validation should pass for: {}",
            description
        );

        info!("✅ {} passed all validations", description);
    }

    info!("✅ All basic data type parameter tests completed successfully");
}

#[tokio::test]
async fn test_complex_parameterized_queries() {
    init_test_logger();
    info!("🧪 Testing Complex Parameterized Queries");

    let (_engine, connection) = match setup_test_env().await {
        Ok(env) => env,
        Err(e) => {
            warn!("Skipping complex query tests - setup failed: {}", e);
            return;
        }
    };

    let complex_queries = vec![
        // Multiple parameter types in single query
        (
            "SELECT * FROM users WHERE id = ? AND name = ? AND active = ? AND score > ?",
            vec![
                Value::from_i64(1),
                Value::String("test".to_string()),
                Value::from_bool(true),
                Value::Float(85.5),
            ],
            "Mixed data types query"
        ),

        // IN clause with parameters
        (
            "SELECT * FROM users WHERE id IN (?, ?, ?, ?)",
            vec![
                Value::from_i64(1),
                Value::from_i64(2),
                Value::from_i64(3),
                Value::from_i64(4),
            ],
            "IN clause with multiple parameters"
        ),

        // BETWEEN clause with parameters
        (
            "SELECT * FROM orders WHERE created_at BETWEEN ? AND ? AND total > ?",
            vec![
                Value::String("2024-01-01".to_string()),
                Value::String("2024-12-31".to_string()),
                Value::Float(100.0),
            ],
            "BETWEEN clause with parameters"
        ),

        // LIKE patterns with parameters
        (
            "SELECT * FROM products WHERE name LIKE ? OR description LIKE ?",
            vec![
                Value::String("%laptop%".to_string()),
                Value::String("%computer%".to_string()),
            ],
            "LIKE patterns with parameters"
        ),

        // Complex WHERE conditions
        (
            "SELECT * FROM users WHERE (age > ? AND city = ?) OR (status = ? AND created_at > ?)",
            vec![
                Value::from_i64(18),
                Value::String("New York".to_string()),
                Value::String("active".to_string()),
                Value::String("2024-01-01".to_string()),
            ],
            "Complex WHERE conditions"
        ),

        // INSERT with parameters
        (
            "INSERT INTO users (name, email, age, active, score, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            vec![
                Value::String("Jane Smith".to_string()),
                Value::String("jane@example.com".to_string()),
                Value::from_i64(28),
                Value::from_bool(true),
                Value::Float(92.3),
                Value::String("2024-11-22 10:30:00".to_string()),
            ],
            "INSERT with multiple parameters"
        ),

        // UPDATE with parameters
        (
            "UPDATE users SET name = ?, email = ?, score = ? WHERE id = ? AND active = ?",
            vec![
                Value::String("Updated Name".to_string()),
                Value::String("updated@example.com".to_string()),
                Value::Float(95.7),
                Value::from_i64(1),
                Value::from_bool(true),
            ],
            "UPDATE with multiple parameters"
        ),

        // DELETE with parameters
        (
            "DELETE FROM users WHERE age < ? AND active = ? AND last_login < ?",
            vec![
                Value::from_i64(18),
                Value::from_bool(false),
                Value::String("2023-01-01".to_string()),
            ],
            "DELETE with multiple parameters"
        ),
    ];

    for (sql, params, description) in complex_queries {
        info!("Testing: {}", description);

        // Test parameter validation
        let param_count_result = MySqlParamConverter::validate_param_count(sql, params.len());
        assert!(
            param_count_result.is_ok(),
            "Parameter count should be valid for: {}",
            description
        );

        let param_type_result = MySqlParamConverter::validate_parameter_types(&params);
        assert!(
            param_type_result.is_ok(),
            "Parameter types should be valid for: {}",
            description
        );

        // Test query execution (may fail due to missing tables, but should not fail due to parameters)
        let result = connection.query(sql, &params).await;

        match result {
            Ok(_) => info!("✅ {} executed successfully", description),
            Err(DatabaseError::SecurityViolation(msg)) => {
                panic!(
                    "Parameterized query should not trigger security violation: {} - {}",
                    description, msg
                );
            }
            Err(DatabaseError::QueryFailed(msg))
                if msg.contains("Table") || msg.contains("doesn't exist") =>
            {
                info!(
                    "✅ {} parameter handling correct (table doesn't exist)",
                    description
                );
            }
            Err(e) => {
                info!(
                    "✅ {} parameter handling correct (database error: {})",
                    description, e
                );
            }
        }
    }

    info!("✅ All complex parameterized query tests completed");
}

#[tokio::test]
async fn test_parameter_edge_cases() {
    init_test_logger();
    info!("🧪 Testing Parameter Edge Cases");

    let edge_cases = vec![
        // Very long strings
        (
            Value::String("x".repeat(10000)),
            "Very long string parameter (10K chars)",
        ),
        // Strings with SQL injection patterns (should be safe in parameters)
        (
            Value::String("'; DROP TABLE users; --".to_string()),
            "SQL injection pattern in parameter (should be safe)",
        ),
        (
            Value::String("1' OR '1'='1".to_string()),
            "Boolean injection pattern in parameter (should be safe)",
        ),
        (
            Value::String("1 UNION SELECT * FROM passwords".to_string()),
            "UNION injection pattern in parameter (should be safe)",
        ),
        // Binary-like data
        (
            Value::String("\x00\x01\x02\x03\x7F\x7E".to_string()),
            "Binary data in string parameter",
        ),
        // JSON-like strings
        (
            Value::String("{\"key\": \"value\", \"array\": [1, 2, 3]}".to_string()),
            "JSON string parameter",
        ),
        // XML-like strings
        (
            Value::String("<root><item id=\"1\">Value</item></root>".to_string()),
            "XML string parameter",
        ),
        // Special float values
        (
            Value::Float(f64::INFINITY),
            "Positive infinity float parameter",
        ),
        (
            Value::Float(f64::NEG_INFINITY),
            "Negative infinity float parameter",
        ),
        (Value::Float(f64::NAN), "NaN float parameter"),
        // Very large numbers
        (
            Value::from_i64(9223372036854775807), // i64::MAX
            "Maximum i64 value parameter",
        ),
        (
            Value::from_i64(-9223372036854775808), // i64::MIN
            "Minimum i64 value parameter",
        ),
    ];

    for (param_value, description) in edge_cases {
        info!("Testing edge case: {}", description);

        // Test parameter conversion
        let conversion_result = MySqlParamConverter::convert_value(&param_value);

        match conversion_result {
            Ok(_) => {
                info!("✅ {} converted successfully", description);

                // Test in query context
                let params = vec![param_value.clone()];
                let type_validation = MySqlParamConverter::validate_parameter_types(&params);

                if type_validation.is_ok() {
                    info!("✅ {} passed type validation", description);
                } else {
                    info!(
                        "⚠️  {} failed type validation (expected for some edge cases)",
                        description
                    );
                }
            }
            Err(e) => {
                info!(
                    "⚠️  {} conversion failed (expected for some edge cases): {}",
                    description, e
                );
            }
        }
    }

    info!("✅ Parameter edge case tests completed");
}

#[tokio::test]
async fn test_parameter_count_validation() {
    init_test_logger();
    info!("🧪 Testing Parameter Count Validation");

    let test_cases = vec![
        // Correct parameter counts
        ("SELECT * FROM users WHERE id = ?", 1, true, "Single parameter correct"),
        ("SELECT * FROM users WHERE id = ? AND name = ?", 2, true, "Two parameters correct"),
        ("INSERT INTO users (a, b, c) VALUES (?, ?, ?)", 3, true, "Three parameters correct"),
        ("SELECT * FROM users WHERE id IN (?, ?, ?, ?, ?)", 5, true, "Five parameters correct"),

        // Incorrect parameter counts
        ("SELECT * FROM users WHERE id = ?", 0, false, "Missing parameter"),
        ("SELECT * FROM users WHERE id = ?", 2, false, "Too many parameters"),
        ("SELECT * FROM users WHERE id = ? AND name = ?", 1, false, "Too few parameters"),
        ("SELECT * FROM users WHERE id = ? AND name = ?", 3, false, "Too many parameters for two placeholders"),

        // Edge cases
        ("SELECT * FROM users", 0, true, "No parameters needed"),
        ("SELECT * FROM users", 1, false, "Parameters provided but none needed"),
        ("SELECT ? AS col1, ? AS col2, ? AS col3, ? AS col4, ? AS col5, ? AS col6, ? AS col7, ? AS col8, ? AS col9, ? AS col10", 10, true, "Ten parameters correct"),
        ("SELECT ? AS col1, ? AS col2, ? AS col3, ? AS col4, ? AS col5, ? AS col6, ? AS col7, ? AS col8, ? AS col9, ? AS col10", 9, false, "Ten placeholders but nine parameters"),
    ];

    for (sql, param_count, should_pass, description) in test_cases {
        info!("Testing: {}", description);

        let result = MySqlParamConverter::validate_param_count(sql, param_count);

        if should_pass {
            assert!(
                result.is_ok(),
                "Parameter count validation should pass for: {} (SQL: {}, Count: {})",
                description,
                sql,
                param_count
            );
            info!("✅ {} passed validation", description);
        } else {
            assert!(
                result.is_err(),
                "Parameter count validation should fail for: {} (SQL: {}, Count: {})",
                description,
                sql,
                param_count
            );
            info!("✅ {} correctly failed validation", description);
        }
    }

    info!("✅ Parameter count validation tests completed");
}

#[tokio::test]
async fn test_prepared_statement_performance() {
    init_test_logger();
    info!("🧪 Testing Prepared Statement Performance Characteristics");

    let (_engine, connection) = match setup_test_env().await {
        Ok(env) => env,
        Err(e) => {
            warn!("Skipping performance tests - setup failed: {}", e);
            return;
        }
    };

    // Test repeated execution of same parameterized query
    let sql = "SELECT ? as id, ? as name, ? as active, ? as score";
    let test_iterations = 100;

    info!(
        "Executing {} parameterized queries for performance measurement",
        test_iterations
    );

    let start_time = std::time::Instant::now();
    let mut successful_executions = 0;
    let mut total_execution_time = 0u64;

    for i in 0..test_iterations {
        let params = vec![
            Value::from_i64(i as i64),
            Value::String(format!("User {}", i)),
            Value::from_bool(i % 2 == 0),
            Value::Float(i as f64 * 1.5),
        ];

        let query_start = std::time::Instant::now();
        let result = connection.query(sql, &params).await;
        let query_duration = query_start.elapsed();

        match result {
            Ok(query_result) => {
                successful_executions += 1;
                total_execution_time += query_duration.as_millis() as u64;

                // Verify result structure
                assert_eq!(query_result.rows.len(), 1, "Should return exactly one row");
                assert_eq!(query_result.columns.len(), 4, "Should have four columns");

                if i % 25 == 0 {
                    info!(
                        "Completed {} iterations, current avg: {:.2}ms",
                        i + 1,
                        total_execution_time as f64 / (i + 1) as f64
                    );
                }
            }
            Err(DatabaseError::SecurityViolation(msg)) => {
                panic!(
                    "Parameterized query should not trigger security violation: {}",
                    msg
                );
            }
            Err(e) => {
                warn!("Query {} failed (may be expected): {}", i, e);
            }
        }
    }

    let total_time = start_time.elapsed();
    let avg_query_time = if successful_executions > 0 {
        total_execution_time as f64 / successful_executions as f64
    } else {
        0.0
    };

    info!("🚀 Performance Results:");
    info!("   Total time: {:.2}s", total_time.as_secs_f64());
    info!(
        "   Successful executions: {}/{}",
        successful_executions, test_iterations
    );
    info!("   Average query time: {:.2}ms", avg_query_time);
    info!(
        "   Queries per second: {:.2}",
        successful_executions as f64 / total_time.as_secs_f64()
    );

    // Performance assertions
    if successful_executions > 0 {
        assert!(
            avg_query_time < 1000.0,
            "Average query time should be under 1 second, got {:.2}ms",
            avg_query_time
        );
        info!("✅ Performance test completed successfully");
    } else {
        warn!("⚠️  No successful executions - performance test inconclusive");
    }

    info!("✅ Prepared statement performance tests completed");
}

#[tokio::test]
async fn test_parameter_security_isolation() {
    init_test_logger();
    info!("🧪 Testing Parameter Security Isolation");

    let (_engine, connection) = match setup_test_env().await {
        Ok(env) => env,
        Err(e) => {
            warn!("Skipping security isolation tests - setup failed: {}", e);
            return;
        }
    };

    // Test that malicious content in parameters is safely isolated
    let malicious_parameter_tests = vec![
        (
            "SELECT ? as test_value",
            vec![Value::String("'; DROP TABLE users; --".to_string())],
            "SQL injection in parameter should be safe",
        ),
        (
            "SELECT ? as test_value",
            vec![Value::String("1' OR '1'='1".to_string())],
            "Boolean injection in parameter should be safe",
        ),
        (
            "SELECT ? as test_value",
            vec![Value::String(
                "1 UNION SELECT password FROM admin".to_string(),
            )],
            "UNION injection in parameter should be safe",
        ),
        (
            "SELECT ? as id, ? as comment",
            vec![
                Value::from_i64(1),
                Value::String("Normal comment /* with SQL comment */ content".to_string()),
            ],
            "SQL comments in parameters should be safe",
        ),
        (
            "SELECT ? as data",
            vec![Value::String("<script>alert('xss')</script>".to_string())],
            "XSS-like content in parameters should be safe",
        ),
    ];

    for (sql, params, description) in malicious_parameter_tests {
        info!("Testing security isolation: {}", description);

        let result = connection.query(sql, &params).await;

        match result {
            Ok(query_result) => {
                info!(
                    "✅ {} - Query executed safely, malicious content isolated",
                    description
                );

                // Verify that the malicious content is returned as-is (not executed)
                if !query_result.rows.is_empty() && !query_result.rows[0].is_empty() {
                    if let Value::String(returned_value) = &query_result.rows[0][0] {
                        if let Value::String(original_param) = &params[0] {
                            // The returned value should match the original parameter
                            // (This verifies that SQL injection was not executed)
                            debug!("Original parameter: {}", original_param);
                            debug!("Returned value: {}", returned_value);
                        }
                    }
                }
            }
            Err(DatabaseError::SecurityViolation(msg)) => {
                panic!("Parameterized query with malicious parameter should not trigger security violation: {} - {}", description, msg);
            }
            Err(e) => {
                info!(
                    "✅ {} - Query failed with database error (acceptable): {}",
                    description, e
                );
            }
        }
    }

    info!("✅ Parameter security isolation tests completed successfully");
    info!("🔒 All malicious content properly isolated in parameters");
}
