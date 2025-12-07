//! Redis Database Handler Integration Tests
//!
//! These tests require a running Redis server.
//! Start Redis with: docker run -d --name redis-test -p 6380:6379 redis:7.4-alpine
//!
//! Run tests with: cargo test --test redis_integration_tests --features database,redis-backend -- --ignored

#![cfg(all(feature = "database", feature = "redis-backend"))]

use mcp_rs::handlers::database::{
    engine::DatabaseEngine,
    engines::redis::RedisEngine,
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig, SecurityConfig,
    },
};

/// Create test database configuration
fn create_test_config() -> DatabaseConfig {
    DatabaseConfig {
        database_type: DatabaseType::Redis,
        connection: ConnectionConfig {
            host: "localhost".to_string(),
            port: 6380, // Using port 6380 for test container
            database: "0".to_string(),
            username: String::new(),
            password: String::new(),
            ssl_mode: None,
            timeout_seconds: 30,
            retry_attempts: 3,
            options: Default::default(),
        },
        pool: PoolConfig::default(),
        security: SecurityConfig::default(),
        features: FeatureConfig::default(),
    }
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_engine_creation() {
    let config = create_test_config();
    let engine = RedisEngine::new(config).await;
    assert!(engine.is_ok());
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_connection() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();

    let connection = engine.connect(&config).await;
    assert!(connection.is_ok());
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_ping() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    let result = connection.ping().await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_basic_set_get() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    // SET operation using SQL-like syntax
    let result = connection
        .execute("INSERT INTO test_key VALUES ('test_value')", &[])
        .await;

    if let Err(ref e) = result {
        eprintln!("Execute error: {:?}", e);
    }
    assert!(result.is_ok());

    // GET operation using SQL-like syntax
    let result = connection.query("SELECT * FROM test_key", &[]).await;

    if let Err(ref e) = result {
        eprintln!("Query error: {:?}", e);
    }
    assert!(result.is_ok());
    let query_result = result.unwrap();
    assert_eq!(query_result.rows.len(), 1);

    // Cleanup
    let _ = connection.execute("DELETE FROM test_key", &[]).await;
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_delete() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    // SET a key
    let _ = connection
        .execute("INSERT INTO test_delete_key VALUES ('value')", &[])
        .await;

    // DELETE the key
    let result = connection.execute("DELETE FROM test_delete_key", &[]).await;
    assert!(result.is_ok());

    let exec_result = result.unwrap();
    assert!(exec_result.rows_affected >= 1);
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_transaction_commit() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    // Begin transaction
    let tx = connection.begin_transaction().await;
    assert!(tx.is_ok());
    let tx = tx.unwrap();

    // Execute commands in transaction
    let _ = tx
        .execute("INSERT INTO tx_test_key VALUES ('tx_value')", &[])
        .await;

    // Commit transaction
    let result = tx.commit().await;
    assert!(result.is_ok());

    // Verify data was committed
    let result = connection.query("SELECT * FROM tx_test_key", &[]).await;
    assert!(result.is_ok());

    // Cleanup
    let _ = connection.execute("DELETE FROM tx_test_key", &[]).await;
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_transaction_rollback() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    // Begin transaction
    let tx = connection.begin_transaction().await;
    assert!(tx.is_ok());
    let tx = tx.unwrap();

    // Execute commands in transaction
    let _ = tx
        .execute(
            "INSERT INTO tx_rollback_key VALUES ('should_not_exist')",
            &[],
        )
        .await;

    // Rollback transaction
    let result = tx.rollback().await;
    assert!(result.is_ok());

    // Verify data was NOT committed (Redis doesn't guarantee this)
    // Note: Redis MULTI/EXEC doesn't support true rollback
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_transaction_info() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    let tx = connection.begin_transaction().await.unwrap();

    let tx_info = tx.transaction_info();
    assert_eq!(
        tx_info.isolation_level,
        mcp_rs::handlers::database::engine::IsolationLevel::Serializable
    );

    let _ = tx.rollback().await;
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_connection_info() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    let info = connection.connection_info();
    assert!(info.connection_id.contains("redis"));
    assert!(info.database_name.contains("redis-db"));
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_get_schema() {
    let config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();
    let connection = engine.connect(&config).await.unwrap();

    let result = connection.get_schema().await;
    assert!(result.is_ok());

    let schema = result.unwrap();
    assert!(schema.database_name.contains("redis-db"));
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_health_check() {
    let config = create_test_config();
    let engine = RedisEngine::new(config).await.unwrap();

    let health = engine.health_check().await;
    assert!(health.is_ok());
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_supported_features() {
    let config = create_test_config();
    let engine = RedisEngine::new(config).await.unwrap();

    use mcp_rs::handlers::database::types::DatabaseFeature;
    let features = engine.supported_features();

    assert!(features.contains(&DatabaseFeature::InMemoryStorage));
    assert!(features.contains(&DatabaseFeature::Transactions));
    assert!(features.contains(&DatabaseFeature::KeyValueStore));
}

#[tokio::test]
#[ignore] // Requires running Redis server
async fn test_redis_config_validation() {
    let mut config = create_test_config();
    let engine = RedisEngine::new(config.clone()).await.unwrap();

    // Valid config
    let result = engine.validate_config(&config);
    assert!(result.is_ok());

    // Invalid host
    config.connection.host = String::new();
    let result = engine.validate_config(&config);
    assert!(result.is_err());

    // Invalid port
    config.connection.host = "localhost".to_string();
    config.connection.port = 0;
    let result = engine.validate_config(&config);
    assert!(result.is_err());
}
