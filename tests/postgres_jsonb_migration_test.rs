//! PostgreSQL JSONB and Migration Integration Tests

#[cfg(feature = "postgresql-backend")]
mod postgres_jsonb_tests {
    use mcp_rs::handlers::database::engines::postgres::{
        JsonbHandler, JsonbQueryBuilder, MigrationManager, PostgresConfig, PostgresEngine,
    };
    use serde_json::json;

    async fn setup_test_engine() -> PostgresEngine {
        let config = PostgresConfig::builder()
            .host("localhost")
            .port(5432)
            .database("mcp_rs_test")
            .username("postgres")
            .password("postgres")
            .build()
            .expect("Failed to build config");

        PostgresEngine::new(config)
            .await
            .expect("Failed to create engine")
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_query_builder() {
        let builder = JsonbQueryBuilder::new("users", "profile")
            .has_key("email")
            .contains(json!({"active": true}));

        let sql = builder.build_select();
        assert!(sql.contains("profile ? 'email'"));
        assert!(sql.contains("profile @> $1"));
        assert_eq!(builder.params().len(), 1);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_insert() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create temporary table
        let create_sql = r#"
            CREATE TEMP TABLE test_jsonb (
                id SERIAL PRIMARY KEY,
                data JSONB
            )
        "#;
        sqlx::query(create_sql)
            .execute(engine.pool())
            .await
            .expect("Failed to create temp table");

        // Insert JSONB data
        let data = json!({
            "name": "Test User",
            "age": 25,
            "tags": ["test", "jsonb"]
        });

        let result = handler.insert_jsonb("test_jsonb", "data", &data).await;
        assert!(result.is_ok());

        let exec_result = result.unwrap();
        assert_eq!(exec_result.rows_affected, 1);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_update_field() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create and populate temp table
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_update (
                id SERIAL PRIMARY KEY,
                data JSONB DEFAULT '{}'
            );
            INSERT INTO test_update (data) VALUES ('{"name": "Old Name", "age": 30}');
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to setup");

        // Update JSONB field
        let result = handler
            .update_jsonb_field("test_update", "data", "{name}", &json!("New Name"), None)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().rows_affected, 1);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_delete_path() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create and populate temp table
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_delete (
                id SERIAL PRIMARY KEY,
                data JSONB DEFAULT '{}'
            );
            INSERT INTO test_delete (data) VALUES ('{"name": "Test", "age": 30, "city": "Tokyo"}');
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to setup");

        // Delete path from JSONB
        let result = handler
            .delete_jsonb_path("test_delete", "data", "{city}", None)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_query_path() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create and populate temp table
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_query (
                id SERIAL PRIMARY KEY,
                data JSONB DEFAULT '{}'
            );
            INSERT INTO test_query (data) VALUES 
                ('{"user": {"name": "Alice", "age": 30}}'),
                ('{"user": {"name": "Bob", "age": 25}}');
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to setup");

        // Query JSONB path
        let result = handler
            .query_jsonb_path("test_query", "data", "user.name", None)
            .await;

        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 2);
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_aggregate() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create and populate temp table
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_agg (
                id SERIAL PRIMARY KEY,
                data JSONB DEFAULT '{}'
            );
            INSERT INTO test_agg (data) VALUES 
                ('{"value": 1}'),
                ('{"value": 2}'),
                ('{"value": 3}');
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to setup");

        // Aggregate JSONB
        let result = handler.aggregate_jsonb("test_agg", "data", None).await;

        assert!(result.is_ok());
        let agg = result.unwrap();
        assert!(agg.is_array());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_gin_index_creation() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create temp table
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_gin (
                id SERIAL PRIMARY KEY,
                data JSONB DEFAULT '{}'
            )
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to create table");

        // Create GIN index
        let result = handler.create_gin_index("test_gin", "data", None).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_has_gin_index() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create temp table with GIN index
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_check_gin (
                id SERIAL PRIMARY KEY,
                data JSONB DEFAULT '{}'
            );
            CREATE INDEX test_gin_idx ON test_check_gin USING GIN (data);
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to setup");

        // Check for GIN index
        let result = handler.has_gin_index("test_check_gin", "data").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_migration_history() {
        let engine = setup_test_engine().await;
        let manager = engine.migration_manager("./migrations/postgres");

        // Get migration history
        let result = manager.get_migration_history().await;

        // Should succeed even if no migrations run
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_is_up_to_date() {
        let engine = setup_test_engine().await;
        let manager = engine.migration_manager("./migrations/postgres");

        let result = manager.is_up_to_date().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_pending_migrations_count() {
        let engine = setup_test_engine().await;
        let manager = engine.migration_manager("./migrations/postgres");

        let result = manager.pending_migrations_count().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_operators_contains() {
        let builder =
            JsonbQueryBuilder::new("events", "metadata").contains(json!({"status": "active"}));

        let where_clause = builder.build_where();
        assert!(where_clause.contains("@>"));
        assert!(where_clause.contains("$1"));
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_operators_has_any_key() {
        let builder =
            JsonbQueryBuilder::new("events", "metadata").has_any_key(&["user_id", "session_id"]);

        let where_clause = builder.build_where();
        assert!(where_clause.contains("?|"));
        assert!(where_clause.contains("ARRAY"));
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_jsonb_operators_has_all_keys() {
        let builder =
            JsonbQueryBuilder::new("events", "metadata").has_all_keys(&["timestamp", "event_type"]);

        let where_clause = builder.build_where();
        assert!(where_clause.contains("?&"));
        assert!(where_clause.contains("ARRAY"));
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_build_jsonb_object() {
        let engine = setup_test_engine().await;
        let handler = engine.jsonb_handler();

        // Create temp table
        sqlx::query(
            r#"
            CREATE TEMP TABLE test_build (
                id SERIAL PRIMARY KEY,
                name VARCHAR(100),
                age INTEGER
            );
            INSERT INTO test_build (name, age) VALUES ('Alice', 30), ('Bob', 25);
            "#,
        )
        .execute(engine.pool())
        .await
        .expect("Failed to setup");

        // Build JSONB objects
        let result = handler
            .build_jsonb_object("test_build", &[("name", "name"), ("age", "age")], None)
            .await;

        assert!(result.is_ok());
        let objects = result.unwrap();
        assert_eq!(objects.len(), 2);
    }
}
