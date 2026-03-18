//! PostgreSQL Transaction Tests
//!
//! Tests for PostgreSQL transaction management including savepoints and isolation levels

#[cfg(feature = "postgresql-backend")]
mod postgres_transaction_tests {
    use mcp_rs::handlers::database::{
        engine::IsolationLevel,
        engines::postgres::{PostgresConfig, PostgresEngine},
    };

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_transaction_creation() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config)
            .await
            .expect("Failed to create engine");
        let tx = engine.begin_transaction().await;
        assert!(tx.is_ok(), "Failed to begin transaction");
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_transaction_with_isolation_level() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config)
            .await
            .expect("Failed to create engine");

        for level in &[
            IsolationLevel::ReadUncommitted,
            IsolationLevel::ReadCommitted,
            IsolationLevel::RepeatableRead,
            IsolationLevel::Serializable,
        ] {
            let tx = engine.begin_transaction_with_isolation(*level).await;
            assert!(
                tx.is_ok(),
                "Failed to create transaction with isolation level: {:?}",
                level
            );

            if let Ok(tx) = tx {
                assert_eq!(tx.isolation_level(), Some(*level));
            }
        }
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_savepoint_operations() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config)
            .await
            .expect("Failed to create engine");
        let mut tx = engine
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Create savepoint
        let result = tx.create_savepoint("sp1").await;
        assert!(result.is_ok(), "Failed to create savepoint");
        assert_eq!(tx.savepoint_count(), 1);

        // Create second savepoint
        let result = tx.create_savepoint("sp2").await;
        assert!(result.is_ok(), "Failed to create second savepoint");
        assert_eq!(tx.savepoint_count(), 2);

        // Rollback to first savepoint
        let result = tx.rollback_to_savepoint("sp1").await;
        assert!(result.is_ok(), "Failed to rollback to savepoint");
        assert_eq!(tx.savepoint_count(), 1);

        // Release savepoint
        let result = tx.release_savepoint("sp1").await;
        assert!(result.is_ok(), "Failed to release savepoint");
        assert_eq!(tx.savepoint_count(), 0);
    }

    #[test]
    fn test_isolation_level_string_representation() {
        assert_eq!(
            IsolationLevel::ReadUncommitted.to_string(),
            "READ UNCOMMITTED"
        );
        assert_eq!(IsolationLevel::ReadCommitted.to_string(), "READ COMMITTED");
        assert_eq!(
            IsolationLevel::RepeatableRead.to_string(),
            "REPEATABLE READ"
        );
        assert_eq!(IsolationLevel::Serializable.to_string(), "SERIALIZABLE");
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_transaction_duration_tracking() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config)
            .await
            .expect("Failed to create engine");
        let tx = engine
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let duration = tx.duration_ms();
        assert!(
            duration >= 100,
            "Transaction duration should be at least 100ms, got {}ms",
            duration
        );
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_transaction_is_active() {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        let engine = PostgresEngine::new(config)
            .await
            .expect("Failed to create engine");
        let tx = engine
            .begin_transaction()
            .await
            .expect("Failed to begin transaction");

        assert!(tx.is_active().await, "Transaction should be active");
    }
}
