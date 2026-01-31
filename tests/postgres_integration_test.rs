//! PostgreSQL統合テスト
//!
//! PostgreSQL最適化機能の統合テスト

#[cfg(feature = "postgresql-backend")]
mod postgres_tests {
    use mcp_rs::handlers::database::engines::postgres::{
        PostgresConfig, PostgresEngine, create_optimized_pool,
        config::SslMode,
    };

    #[tokio::test]
    #[ignore] // PostgreSQLサーバーが必要なため、デフォルトではスキップ
    async fn test_postgres_engine_creation() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .min_connections(5)
            .max_connections(20)
            .build()
            .expect("Failed to build config");

        let result = PostgresEngine::new(config).await;
        assert!(result.is_ok(), "Failed to create PostgreSQL engine");
    }

    #[tokio::test]
    #[ignore] // PostgreSQLサーバーが必要なため、デフォルトではスキップ
    async fn test_postgres_pool_creation() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .min_connections(5)
            .max_connections(20)
            .build()
            .expect("Failed to build config");

        let pool = create_optimized_pool(&config).await;
        assert!(pool.is_ok(), "Failed to create optimized pool");
    }

    #[test]
    fn test_postgres_config_builder() {
        let config = PostgresConfig::builder()
            .host("testhost")
            .port(5433)
            .database("testdb")
            .username("testuser")
            .password("testpass")
            .min_connections(10)
            .max_connections(50)
            .ssl_mode(SslMode::Require)
            .build();

        assert!(config.is_ok(), "Failed to build config");

        let config = config.unwrap();
        assert_eq!(config.host, "testhost");
        assert_eq!(config.port, 5433);
        assert_eq!(config.database, "testdb");
        assert_eq!(config.username, "testuser");
        assert_eq!(config.min_connections, 10);
        assert_eq!(config.max_connections, 50);
        assert_eq!(config.ssl_mode, SslMode::Require);
    }

    #[test]
    fn test_postgres_connection_string() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mydb")
            .username("myuser")
            .password("mypass")
            .ssl_mode(SslMode::Prefer)
            .application_name("test-app")
            .build()
            .unwrap();

        let conn_str = config.connection_string();
        
        assert!(conn_str.contains("postgresql://myuser:mypass@localhost:5432/mydb"));
        assert!(conn_str.contains("sslmode=prefer"));
        assert!(conn_str.contains("application_name=test-app"));
    }

    #[test]
    fn test_postgres_config_validation() {
        // 有効な設定
        let valid_config = PostgresConfig::builder()
            .host("localhost")
            .database("testdb")
            .username("testuser")
            .min_connections(5)
            .max_connections(20)
            .build();
        assert!(valid_config.is_ok());

        // min > maxの無効な設定
        let invalid_config = PostgresConfig::builder()
            .min_connections(100)
            .max_connections(50)
            .build();
        assert!(invalid_config.is_err());

        // 空のホスト名
        let invalid_host = PostgresConfig::builder()
            .host("")
            .build();
        assert!(invalid_host.is_err());

        // max_connections = 0
        let invalid_max = PostgresConfig::builder()
            .max_connections(0)
            .build();
        assert!(invalid_max.is_err());
    }
}
