//! MySQL Database Handler Integration Tests
//!
//! Comprehensive tests for MySQL database operations

#![cfg(feature = "database")]

#[cfg(test)]
mod tests {
    use mcp_rs::handlers::database::{
        engine::DatabaseEngine,
        engines::mysql::MySqlEngine,
        types::{
            ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig,
            SecurityConfig, Value,
        },
    };

    /// Helper function to create test database config
    fn create_test_config() -> DatabaseConfig {
        DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: ConnectionConfig {
                host: "localhost".to_string(),
                port: 3307, // Using port 3307 to avoid conflict with PostgreSQL
                database: "test_db".to_string(),
                username: "root".to_string(),
                password: "password".to_string(),
                ssl_mode: None,
                timeout_seconds: 30,
                retry_attempts: 3,
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_engine_creation() {
        let config = create_test_config();
        let result = MySqlEngine::new_without_security(config).await;
        assert!(result.is_ok());

        let engine = result.unwrap();
        assert_eq!(engine.engine_type(), DatabaseType::MySQL);
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_connection() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await;
        assert!(connection.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_simple_query() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        // Simple SELECT query
        let result = connection.query("SELECT 1 AS num", &[]).await;
        assert!(result.is_ok());

        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
        assert_eq!(query_result.rows[0].len(), 1);
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_parameterized_query() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        // Parameterized query
        let params = vec![Value::Int(42), Value::String("test".to_string())];
        let result = connection
            .query("SELECT ? AS num, ? AS text", &params)
            .await;

        assert!(result.is_ok());
        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_execute() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        // Create test table
        let create_table = "CREATE TABLE IF NOT EXISTS test_users (
            id INT AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(100),
            age INT
        )";

        let result = connection.execute(create_table, &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_insert_and_select() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        // Create table
        let _ = connection
            .execute(
                "CREATE TABLE IF NOT EXISTS test_insert (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    name VARCHAR(100)
                )",
                &[],
            )
            .await;

        // Insert data
        let params = vec![Value::String("Alice".to_string())];
        let insert_result = connection
            .execute("INSERT INTO test_insert (name) VALUES (?)", &params)
            .await;

        assert!(insert_result.is_ok());
        let execute_result = insert_result.unwrap();
        assert_eq!(execute_result.rows_affected, 1);
        assert!(execute_result.last_insert_id.is_some());

        // Select data
        let last_id = match execute_result.last_insert_id {
            Some(Value::Int(id)) => id,
            _ => panic!("Expected integer last_insert_id"),
        };
        let query_result = connection
            .query(
                "SELECT name FROM test_insert WHERE id = ?",
                &[Value::Int(last_id)],
            )
            .await;

        assert!(query_result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_transaction_commit() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        // Create table
        let _ = connection
            .execute(
                "CREATE TABLE IF NOT EXISTS test_transaction (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    value VARCHAR(100)
                )",
                &[],
            )
            .await;

        // Begin transaction
        let transaction = connection.begin_transaction().await.unwrap();

        // Insert within transaction
        let params = vec![Value::String("tx_value".to_string())];
        let result = transaction
            .execute("INSERT INTO test_transaction (value) VALUES (?)", &params)
            .await;

        assert!(result.is_ok());

        // Commit
        let commit_result = transaction.commit().await;
        assert!(commit_result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_transaction_rollback() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        // Create table
        let _ = connection
            .execute(
                "CREATE TABLE IF NOT EXISTS test_rollback (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    value VARCHAR(100)
                )",
                &[],
            )
            .await;

        // Begin transaction
        let transaction = connection.begin_transaction().await.unwrap();

        // Insert within transaction
        let params = vec![Value::String("rollback_value".to_string())];
        let _ = transaction
            .execute("INSERT INTO test_rollback (value) VALUES (?)", &params)
            .await;

        // Rollback
        let rollback_result = transaction.rollback().await;
        assert!(rollback_result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_ping() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        let ping_result = connection.ping().await;
        assert!(ping_result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_connection_info() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let connection = engine.connect(&config).await.unwrap();

        let info = connection.connection_info();
        assert_eq!(info.database_name, "test_db");
        // connected_at is a timestamp, just check it exists
        assert!(info.connected_at.timestamp() > 0);
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_health_check() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config).await.unwrap();

        let health = engine.health_check().await;
        assert!(health.is_ok());

        use mcp_rs::handlers::database::types::HealthStatusType;
        let status = health.unwrap();
        assert_eq!(status.status, HealthStatusType::Healthy);
    }

    #[tokio::test]
    #[ignore] // Requires running MySQL server
    async fn test_mysql_supported_features() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config).await.unwrap();

        use mcp_rs::handlers::database::types::DatabaseFeature;
        let features = engine.supported_features();

        assert!(features.contains(&DatabaseFeature::Transactions));
        assert!(features.contains(&DatabaseFeature::PreparedStatements));
    }

    #[tokio::test]
    async fn test_mysql_config_validation() {
        let config = create_test_config();
        let engine = MySqlEngine::new_without_security(config.clone())
            .await
            .unwrap();

        let validation = engine.validate_config(&config);
        assert!(validation.is_ok());
    }
}
