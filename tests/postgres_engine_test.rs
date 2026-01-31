//! PostgreSQL Engine Integration Tests
//!
//! Tests for DatabaseEngine and DatabaseConnection implementations

#[cfg(feature = "postgresql-backend")]
mod postgres_engine_tests {
    use mcp_rs::handlers::database::{
        engines::postgres::{PostgresConfig, PostgresEngine},
        types::Value,
        DatabaseEngine,
    };

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_engine_connect() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config.clone()).await.expect("Failed to create engine");
        
        // Convert PostgresConfig to DatabaseConfig
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await;
        assert!(connection.is_ok(), "Failed to create connection");
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_connection_ping() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config).await.expect("Failed to create engine");
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await.expect("Failed to connect");
        
        let ping_result = connection.ping().await;
        assert!(ping_result.is_ok(), "Ping failed");
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_connection_query() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config).await.expect("Failed to create engine");
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await.expect("Failed to connect");
        
        let result = connection.query("SELECT 1 as num, 'test' as text", &[]).await;
        assert!(result.is_ok(), "Query failed");
        
        let query_result = result.unwrap();
        assert_eq!(query_result.columns.len(), 2);
        assert_eq!(query_result.rows.len(), 1);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_connection_execute() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config).await.expect("Failed to create engine");
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await.expect("Failed to connect");
        
        // Create temporary table
        let create_result = connection.execute(
            "CREATE TEMP TABLE test_table (id SERIAL PRIMARY KEY, name TEXT)",
            &[]
        ).await;
        assert!(create_result.is_ok(), "Failed to create table");

        // Insert data
        let insert_result = connection.execute(
            "INSERT INTO test_table (name) VALUES ($1)",
            &[Value::String("test".to_string())]
        ).await;
        assert!(insert_result.is_ok(), "Failed to insert data");
        
        let exec_result = insert_result.unwrap();
        assert_eq!(exec_result.rows_affected, 1);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_prepared_statement() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config).await.expect("Failed to create engine");
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await.expect("Failed to connect");
        
        // Prepare statement
        let stmt = connection.prepare("SELECT $1::int as num, $2::text as text").await;
        assert!(stmt.is_ok(), "Failed to prepare statement");
        
        let prepared = stmt.unwrap();
        assert_eq!(prepared.parameter_count(), 2);
        
        // Execute prepared statement
        let result = prepared.query(&[
            Value::Int(42),
            Value::String("hello".to_string())
        ]).await;
        assert!(result.is_ok(), "Failed to execute prepared statement");
        
        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_get_schema() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config).await.expect("Failed to create engine");
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await.expect("Failed to connect");
        
        let schema = connection.get_schema().await;
        assert!(schema.is_ok(), "Failed to get schema");
        
        let db_schema = schema.unwrap();
        assert_eq!(db_schema.database_name, "public");
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_begin_transaction_from_connection() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config).await.expect("Failed to create engine");
        let db_config = mcp_rs::handlers::database::types::DatabaseConfig::default();
        let connection = engine.connect(&db_config).await.expect("Failed to connect");
        
        let tx = connection.begin_transaction().await;
        assert!(tx.is_ok(), "Failed to begin transaction");
    }
}
