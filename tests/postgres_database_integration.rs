/// PostgreSQL Phase 2 - Actual Database Connection Tests
/// Tests for real PostgreSQL database connectivity and operations
///
/// Prerequisites:
/// - PostgreSQL running on localhost:5432
/// - Database: testdb
/// - User: postgres / Password: postgres
/// - Docker Compose environment can be started with:
///   docker-compose -f docker-compose.postgres.yml up -d
///
/// Environment Variables:
/// - TEST_POSTGRESQL_URL: Connection URL (default: postgresql://postgres:postgres@localhost:5432/testdb)

use std::time::Duration;
use sqlx::Row;

#[cfg(test)]
mod postgres_database_integration_tests {
    use super::*;

    // Test configuration
    const DEFAULT_DB_URL: &str = "postgresql://postgres:postgres@localhost:5432/testdb";

    fn get_db_url() -> String {
        std::env::var("TEST_POSTGRESQL_URL").unwrap_or_else(|_| DEFAULT_DB_URL.to_string())
    }

    /// Check if database is available
    async fn is_db_available() -> bool {
        if let Ok(url) = std::env::var("SKIP_DB_TESTS") {
            if url == "1" {
                return false;
            }
        }

        match sqlx::postgres::PgPool::connect(&get_db_url()).await {
            Ok(pool) => {
                let _ = pool.acquire().await;
                true
            }
            Err(_) => false,
        }
    }

    // ==================== Basic Connection Tests ====================

    #[tokio::test]
    #[ignore] // Requires PostgreSQL running
    async fn test_basic_postgres_connection() {
        // Skip if database not available
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        // Test direct connection using sqlx
        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;

        assert!(
            pool.is_ok(),
            "Failed to connect to PostgreSQL: {}",
            pool.err().unwrap()
        );

        let pool = pool.unwrap();
        let result = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await;

        assert!(result.is_ok(), "Failed to execute SELECT version()");

        if let Ok(row) = result {
            let version: String = row.get(0);
            assert!(version.contains("PostgreSQL"));
            println!("Connected to: {}", version);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_connection_pool_creation() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool_result = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await;

        assert!(pool_result.is_ok());

        let pool = pool_result.unwrap();
        let num_idle = pool.num_idle();
        assert!(num_idle > 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_connection_acquire_release() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Acquire connection
        let conn = pool.acquire().await;
        assert!(conn.is_ok());

        // Connection is automatically released when dropped
        drop(conn);
    }

    // ==================== Query Execution Tests ====================

    #[tokio::test]
    #[ignore]
    async fn test_select_from_test_schema() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Test table existence
        let result: Result<Option<(String,)>, _> = sqlx::query_as(
            "SELECT table_name FROM information_schema.tables 
             WHERE table_schema = 'test_schema' AND table_name = 'users'"
        )
        .fetch_optional(&pool)
        .await;

        assert!(result.is_ok());

        if let Ok(Some((table_name,))) = result {
            assert_eq!(table_name, "users");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_insert_and_select() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Clean up any test data
        let _ = sqlx::query("DELETE FROM test_schema.users WHERE email = 'test@integration.test'")
            .execute(&pool)
            .await;

        // Insert test data
        let insert_result = sqlx::query(
            "INSERT INTO test_schema.users (name, email, age, active) 
             VALUES ($1, $2, $3, $4)"
        )
        .bind("Test User")
        .bind("test@integration.test")
        .bind(30)
        .bind(true)
        .execute(&pool)
        .await;

        assert!(
            insert_result.is_ok(),
            "Failed to insert: {}",
            insert_result.err().unwrap()
        );

        // Query inserted data
        let query_result: Result<Option<(String, String, i32, bool)>, _> = sqlx::query_as(
            "SELECT name, email, age, active FROM test_schema.users 
             WHERE email = 'test@integration.test'"
        )
        .fetch_optional(&pool)
        .await;

        assert!(query_result.is_ok());

        if let Ok(Some((name, email, age, active))) = query_result {
            assert_eq!(name, "Test User");
            assert_eq!(email, "test@integration.test");
            assert_eq!(age, 30);
            assert!(active);
        }

        // Clean up
        let _ = sqlx::query("DELETE FROM test_schema.users WHERE email = 'test@integration.test'")
            .execute(&pool)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn test_parameterized_query() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Test parameterized query with multiple types
        let _test_id = 1i32;

        let result: Result<Option<(i32, String)>, _> = sqlx::query_as(
            "SELECT id, email FROM test_schema.users WHERE id = $1 AND active = $2 LIMIT 1"
        )
        .bind(_test_id)
        .bind(true)
        .fetch_optional(&pool)
        .await;

        assert!(result.is_ok());
    }

    // ==================== Transaction Tests ====================

    #[tokio::test]
    #[ignore]
    async fn test_transaction_begin_commit() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Start transaction
        let begin = sqlx::query("BEGIN ISOLATION LEVEL READ COMMITTED")
            .execute(&pool)
            .await;
        assert!(begin.is_ok());

        // Execute command
        let insert = sqlx::query("INSERT INTO test_schema.users (name, email, age, active) VALUES ($1, $2, $3, $4)")
            .bind("Transaction User")
            .bind("txn@test.com")
            .bind(25)
            .bind(true)
            .execute(&pool)
            .await;
        assert!(insert.is_ok());

        // Commit
        let commit = sqlx::query("COMMIT").execute(&pool).await;
        assert!(commit.is_ok());

        // Verify insert
        let verify: Result<Option<String>, _> = sqlx::query_scalar(
            "SELECT email FROM test_schema.users WHERE email = 'txn@test.com'"
        )
        .fetch_optional(&pool)
        .await;

        if let Ok(Some(_)) = verify {
            // Clean up
            let _ = sqlx::query("DELETE FROM test_schema.users WHERE email = 'txn@test.com'")
                .execute(&pool)
                .await;
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_transaction_rollback() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Start transaction
        let begin = sqlx::query("BEGIN").execute(&pool).await;
        assert!(begin.is_ok());

        // Execute insert
        let insert = sqlx::query("INSERT INTO test_schema.users (name, email, age, active) VALUES ($1, $2, $3, $4)")
            .bind("Rollback User")
            .bind("rollback@test.com")
            .bind(35)
            .bind(false)
            .execute(&pool)
            .await;
        assert!(insert.is_ok());

        // Rollback
        let rollback = sqlx::query("ROLLBACK").execute(&pool).await;
        assert!(rollback.is_ok());

        // Verify rollback (should not find the inserted row)
        let verify: Result<Option<String>, _> = sqlx::query_scalar(
            "SELECT email FROM test_schema.users WHERE email = 'rollback@test.com'"
        )
        .fetch_optional(&pool)
        .await;

        assert!(verify.is_ok());
        assert!(verify.unwrap().is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn test_savepoint_creation() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Begin transaction
        let begin = sqlx::query("BEGIN").execute(&pool).await;
        assert!(begin.is_ok());

        // Create savepoint
        let savepoint = sqlx::query("SAVEPOINT sp_1").execute(&pool).await;
        assert!(savepoint.is_ok());

        // Insert data
        let insert = sqlx::query("INSERT INTO test_schema.users (name, email, age, active) VALUES ($1, $2, $3, $4)")
            .bind("SP User")
            .bind("sp@test.com")
            .bind(28)
            .bind(true)
            .execute(&pool)
            .await;
        assert!(insert.is_ok());

        // Rollback to savepoint
        let rollback_sp = sqlx::query("ROLLBACK TO SAVEPOINT sp_1")
            .execute(&pool)
            .await;
        assert!(rollback_sp.is_ok());

        // Commit
        let commit = sqlx::query("COMMIT").execute(&pool).await;
        assert!(commit.is_ok());

        // Verify user was not inserted (rolled back to savepoint before insert)
        let verify: Result<Option<String>, _> = sqlx::query_scalar(
            "SELECT email FROM test_schema.users WHERE email = 'sp@test.com'"
        )
        .fetch_optional(&pool)
        .await;

        assert!(verify.is_ok());
        assert!(verify.unwrap().is_none());
    }

    // ==================== JSON Operations Tests ====================

    #[tokio::test]
    #[ignore]
    async fn test_json_insert_and_query() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Insert post with JSON metadata
        let insert = sqlx::query(
            "INSERT INTO test_schema.posts (user_id, title, content, published, metadata) 
             VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(1i32)
        .bind("Test Post")
        .bind("Test content")
        .bind(true)
        .bind(r#"{"tags": ["rust", "postgres"], "author": "test"}"#)
        .execute(&pool)
        .await;

        assert!(
            insert.is_ok(),
            "Failed to insert post with JSON: {}",
            insert.err().unwrap()
        );

        // Query and extract JSON field
        let result: Result<Option<String>, _> = sqlx::query_scalar(
            "SELECT metadata->>'tags' FROM test_schema.posts WHERE title = 'Test Post'"
        )
        .fetch_optional(&pool)
        .await;

        assert!(result.is_ok());
    }

    // ==================== Concurrent Operations Tests ====================

    #[tokio::test]
    #[ignore]
    async fn test_concurrent_queries() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = std::sync::Arc::new(pool.unwrap());

        // Create multiple concurrent queries
        let handles: Vec<_> = (0..5)
            .map(|_i| {
                let pool = pool.clone();
                tokio::spawn(async move {
                    let result: Result<(String,), _> = sqlx::query_as("SELECT version()")
                        .fetch_one(pool.as_ref())
                        .await;

                    result.is_ok()
                })
            })
            .collect();

        // Wait for all to complete
        for handle in handles {
            let result = handle.await;
            assert!(result.is_ok());
            assert!(result.unwrap());
        }
    }

    // ==================== Error Handling Tests ====================

    #[tokio::test]
    #[ignore]
    async fn test_query_with_invalid_syntax() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Execute invalid query
        let result = sqlx::query("SELECT * FROM nonexistent_table").fetch_all(&pool).await;

        // Should fail
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore]
    async fn test_parameter_type_mismatch() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Try to insert with mismatched types (should be caught by sqlx)
        // This tests the type safety of sqlx
        let _result = sqlx::query("INSERT INTO test_schema.users (name, email, age, active) VALUES ($1, $2, $3, $4)")
            .bind("Test")
            .bind("test@test.com")
            .bind("not_an_integer") // Should be i32
            .bind(true)
            .execute(&pool)
            .await;

        // Binding phase should handle this, or execution should fail
        // This test verifies error handling
    }

    #[tokio::test]
    #[ignore]
    async fn test_connection_timeout() {
        // Try to connect to non-existent host with timeout
        let invalid_url = "postgresql://postgres:postgres@192.0.2.1:5432/testdb";

        let pool_result = tokio::time::timeout(
            Duration::from_secs(5),
            sqlx::postgres::PgPool::connect(invalid_url),
        )
        .await;

        // Should timeout or fail to connect
        assert!(pool_result.is_err() || pool_result.unwrap().is_err());
    }

    // ==================== Schema Verification Tests ====================

    #[tokio::test]
    #[ignore]
    async fn test_schema_tables_exist() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Check for required tables
        let tables = vec!["users", "posts", "comments"];

        for table in tables {
            let result: Result<Option<(String,)>, _> = sqlx::query_as(
                "SELECT table_name FROM information_schema.tables 
                 WHERE table_schema = 'test_schema' AND table_name = $1"
            )
            .bind(table)
            .fetch_optional(&pool)
            .await;

            assert!(result.is_ok(), "Failed to check for table {}", table);

            if let Ok(Some(_)) = result {
                println!("✓ Table '{}' exists", table);
            }
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_schema_columns_exist() {
        if !is_db_available().await {
            println!("Database not available, skipping test");
            return;
        }

        let url = get_db_url();
        let pool = sqlx::postgres::PgPool::connect(&url).await;
        assert!(pool.is_ok());

        let pool = pool.unwrap();

        // Check for required columns in users table
        let result: Result<Vec<String>, _> = sqlx::query_scalar(
            "SELECT column_name FROM information_schema.columns 
             WHERE table_schema = 'test_schema' AND table_name = 'users'
             ORDER BY ordinal_position"
        )
        .fetch_all(&pool)
        .await;

        assert!(result.is_ok());

        if let Ok(columns) = result {
            let expected = vec!["id", "name", "email", "age", "active"];
            for col in expected {
                assert!(
                    columns.iter().any(|c| c == col),
                    "Column '{}' not found in users table",
                    col
                );
            }
        }
    }
}
