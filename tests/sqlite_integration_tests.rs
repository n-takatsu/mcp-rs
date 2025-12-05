//! SQLite Integration Tests
//!
//! Comprehensive tests for SQLite database handler implementation

#[cfg(all(test, feature = "database", feature = "sqlite"))]
mod sqlite_tests {
    use mcp_rs::handlers::database::{
        engine::{DatabaseEngine, DatabaseTransaction},
        engines::sqlite::SqliteEngine,
        types::{
            ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig,
            SecurityConfig, Value,
        },
    };

    fn create_test_config() -> DatabaseConfig {
        DatabaseConfig {
            database_type: DatabaseType::SQLite,
            connection: ConnectionConfig {
                host: ":memory:".to_string(),
                port: 0,
                username: String::new(),
                password: String::new(),
                database: "test".to_string(),
                ssl_mode: None,
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
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_engine_creation() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await;
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_engine_type() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        assert_eq!(engine.engine_type(), DatabaseType::SQLite);
    }

    #[tokio::test]
    async fn test_connection() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await;
        assert!(conn.is_ok());
    }

    #[tokio::test]
    async fn test_health_check() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        let health = engine.health_check().await;
        assert!(health.is_ok());

        let health_status = health.unwrap();
        assert_eq!(
            health_status.status,
            mcp_rs::handlers::database::types::HealthStatusType::Healthy
        );
    }

    #[tokio::test]
    async fn test_query_execution() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        let result = conn.query("SELECT 1 as num, 'test' as text", &[]).await;
        assert!(result.is_ok());

        let query_result = result.unwrap();
        assert_eq!(query_result.rows.len(), 1);
        assert_eq!(query_result.columns.len(), 2);
    }

    #[tokio::test]
    async fn test_execute_statement() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute(
            "CREATE TABLE test_users (id INTEGER PRIMARY KEY, name TEXT)",
            &[],
        )
        .await
        .unwrap();

        let result = conn
            .execute(
                "INSERT INTO test_users (name) VALUES (?)",
                &[Value::String("Alice".to_string())],
            )
            .await;

        assert!(result.is_ok());
        let exec_result = result.unwrap();
        assert_eq!(exec_result.rows_affected, 1);
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute(
            "CREATE TABLE test_data (id INTEGER PRIMARY KEY, value TEXT)",
            &[],
        )
        .await
        .unwrap();

        let tx = conn.begin_transaction().await.unwrap();

        tx.execute(
            "INSERT INTO test_data (value) VALUES (?)",
            &[Value::String("committed".to_string())],
        )
        .await
        .unwrap();

        tx.commit().await.unwrap();

        let result = conn
            .query("SELECT COUNT(*) FROM test_data", &[])
            .await
            .unwrap();
        assert_eq!(result.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_transaction_rollback() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute(
            "CREATE TABLE test_rollback (id INTEGER PRIMARY KEY, value TEXT)",
            &[],
        )
        .await
        .unwrap();

        let tx = conn.begin_transaction().await.unwrap();

        tx.execute(
            "INSERT INTO test_rollback (value) VALUES (?)",
            &[Value::String("should_rollback".to_string())],
        )
        .await
        .unwrap();

        tx.rollback().await.unwrap();

        let result = conn
            .query("SELECT COUNT(*) as count FROM test_rollback", &[])
            .await
            .unwrap();

        if let Some(Value::Int(count)) = result.rows[0].first() {
            assert_eq!(*count, 0);
        } else {
            panic!("Expected BigInt value");
        }
    }

    #[tokio::test]
    async fn test_prepared_statement() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute(
            "CREATE TABLE test_prepared (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)",
            &[],
        )
        .await
        .unwrap();

        let stmt = conn
            .prepare("INSERT INTO test_prepared (name, age) VALUES (?, ?)")
            .await
            .unwrap();

        let result = stmt
            .execute(&[Value::String("Bob".to_string()), Value::Int(25)])
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().rows_affected, 1);
    }

    #[tokio::test]
    async fn test_savepoint() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute(
            "CREATE TABLE test_savepoint (id INTEGER PRIMARY KEY, value TEXT)",
            &[],
        )
        .await
        .unwrap();

        let tx = conn.begin_transaction().await.unwrap();

        tx.execute(
            "INSERT INTO test_savepoint (value) VALUES (?)",
            &[Value::String("first".to_string())],
        )
        .await
        .unwrap();

        tx.savepoint("sp1").await.unwrap();

        tx.execute(
            "INSERT INTO test_savepoint (value) VALUES (?)",
            &[Value::String("second".to_string())],
        )
        .await
        .unwrap();

        tx.rollback_to_savepoint("sp1").await.unwrap();
        tx.commit().await.unwrap();

        let result = conn
            .query("SELECT COUNT(*) as count FROM test_savepoint", &[])
            .await
            .unwrap();

        if let Some(Value::Int(count)) = result.rows[0].first() {
            assert_eq!(*count, 1);
        }
    }

    #[tokio::test]
    async fn test_get_schema() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute("CREATE TABLE schema_test (id INTEGER PRIMARY KEY)", &[])
            .await
            .unwrap();

        let schema = conn.get_schema().await.unwrap();
        assert!(schema.tables.iter().any(|t| t.name == "schema_test"));
    }

    #[tokio::test]
    async fn test_get_table_schema() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config.clone()).await.unwrap();
        let conn = engine.connect(&config).await.unwrap();

        conn.execute(
            "CREATE TABLE table_schema_test (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)",
            &[],
        )
        .await
        .unwrap();

        let table_info = conn.get_table_schema("table_schema_test").await.unwrap();
        assert_eq!(table_info.name, "table_schema_test");
        assert_eq!(table_info.columns.len(), 3);
    }

    #[tokio::test]
    async fn test_version() {
        let config = create_test_config();
        let engine = SqliteEngine::new(config).await.unwrap();
        let version = engine.get_version().await;
        assert!(version.is_ok());
        assert!(!version.unwrap().is_empty());
    }
}
