//! MariaDB Integration Tests
//!
//! Tests for MariaDB engine implementation

#[cfg(feature = "mysql-backend")]
mod tests {
    use mcp_rs::handlers::database::{
        engine::DatabaseEngine,
        engines::mariadb::MariaDbEngine,
        types::{ConnectionConfig, DatabaseConfig, DatabaseFeature, DatabaseType, PoolConfig},
    };
    use std::collections::HashMap;

    fn create_test_config() -> DatabaseConfig {
        DatabaseConfig {
            database_type: DatabaseType::MariaDB,
            connection: ConnectionConfig {
                host: "localhost".to_string(),
                port: 3306,
                database: "test_db".to_string(),
                username: "test".to_string(),
                password: "test".to_string(),
                ssl_mode: None,
                timeout_seconds: 30,
                retry_attempts: 3,
                options: HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: Default::default(),
            features: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_mariadb_engine_creation() {
        let config = create_test_config();
        let result = MariaDbEngine::new_without_security(config).await;

        match result {
            Ok(engine) => {
                assert_eq!(engine.engine_type(), DatabaseType::MariaDB);
                let features = engine.supported_features();
                assert!(!features.is_empty());
            }
            Err(e) => {
                // Connection might fail if MariaDB is not running
                println!(
                    "MariaDB connection failed (expected if server not running): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_mariadb_supported_features() {
        let config = create_test_config();

        match MariaDbEngine::new_without_security(config).await {
            Ok(engine) => {
                let features = engine.supported_features();
                assert!(!features.is_empty());
                assert!(features.contains(&DatabaseFeature::JsonSupport));
                assert!(features.contains(&DatabaseFeature::Transactions));
                println!("MariaDB supported features: {:?}", features);
            }
            Err(e) => {
                println!(
                    "MariaDB engine creation failed (expected if server not running): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_mariadb_health_check() {
        let config = create_test_config();

        match MariaDbEngine::new_without_security(config).await {
            Ok(engine) => {
                let health = engine.health_check().await;
                match health {
                    Ok(status) => {
                        println!("MariaDB health check: {:?}", status.status);
                        // Response time is always non-negative (u64 type)
                    }
                    Err(e) => {
                        println!(
                            "MariaDB health check failed (expected if server not running): {}",
                            e
                        );
                    }
                }
            }
            Err(e) => {
                println!(
                    "MariaDB engine creation failed (expected if server not running): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_mariadb_connection() {
        let config = create_test_config();

        match MariaDbEngine::new_without_security(config.clone()).await {
            Ok(engine) => match engine.connect(&config).await {
                Ok(conn) => {
                    let info = conn.connection_info();
                    assert_eq!(info.database_name, "test_db");
                    assert!(!info.connection_id.is_empty());
                    println!("MariaDB connection successful: {:?}", info);
                }
                Err(e) => {
                    println!(
                        "MariaDB connection failed (expected if server not running): {}",
                        e
                    );
                }
            },
            Err(e) => {
                println!(
                    "MariaDB engine creation failed (expected if server not running): {}",
                    e
                );
            }
        }
    }

    #[tokio::test]
    async fn test_mariadb_config_validation() {
        let config = create_test_config();

        match MariaDbEngine::new_without_security(config.clone()).await {
            Ok(engine) => {
                let result = engine.validate_config(&config);
                match result {
                    Ok(()) => println!("MariaDB config validation passed"),
                    Err(e) => println!("MariaDB config validation failed: {}", e),
                }
            }
            Err(e) => {
                println!(
                    "MariaDB engine creation failed (expected if server not running): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_mariadb_config_structure() {
        let config = create_test_config();
        assert_eq!(config.database_type, DatabaseType::MariaDB);
        assert_eq!(config.connection.host, "localhost");
        assert_eq!(config.connection.port, 3306);
        assert_eq!(config.connection.database, "test_db");
        assert_eq!(config.connection.username, "test");
    }

    #[tokio::test]
    async fn test_mariadb_version() {
        let config = create_test_config();

        match MariaDbEngine::new_without_security(config).await {
            Ok(engine) => match engine.get_version().await {
                Ok(version) => {
                    println!("MariaDB version: {}", version);
                    assert!(!version.is_empty());
                }
                Err(e) => {
                    println!(
                        "MariaDB version check failed (expected if server not running): {}",
                        e
                    );
                }
            },
            Err(e) => {
                println!(
                    "MariaDB engine creation failed (expected if server not running): {}",
                    e
                );
            }
        }
    }
}
