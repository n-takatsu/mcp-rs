//! Integration tests proving `DatabaseHandler` actually enforces column
//! encryption + RBAC on real query results, instead of the encryption and
//! RBAC building blocks sitting unreachable from the query execution path.
//!
//! Requires a real PostgreSQL instance reachable via `TEST_DATABASE_URL`,
//! with `migrations/20260201_column_encryption_rbac.sql` applied. Soft-skips
//! (prints a message and returns) when no such database is configured or
//! reachable, since CI has none.

#![cfg(feature = "database")]

use mcp_rs::handlers::database::column_encryption::{
    ColumnEncryptionConfig, ColumnEncryptionManager,
};
use mcp_rs::handlers::database::column_encryption_rbac::ColumnEncryptionRbac;
use mcp_rs::handlers::database::handler::DatabaseHandler;
use mcp_rs::handlers::database::types::{ConnectionConfig, DatabaseConfig, DatabaseType};
use mcp_rs::mcp::{McpHandler, ToolCallParams};
use mcp_rs::security::auth::types::{AuthUser, Role};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

async fn try_connect() -> Option<PgPool> {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    if database_url.is_empty() {
        println!("skipping: TEST_DATABASE_URL not set");
        return None;
    }
    match PgPool::connect(&database_url).await {
        Ok(pool) => match sqlx::query("SELECT 1").execute(&pool).await {
            Ok(_) => Some(pool),
            Err(e) => {
                println!("skipping: cannot query test Postgres ({e})");
                None
            }
        },
        Err(e) => {
            println!("skipping: cannot connect to test Postgres ({e})");
            None
        }
    }
}

fn connection_config_from_url(database_url: &str) -> ConnectionConfig {
    // Reuse whatever TEST_DATABASE_URL already points at (same convention as
    // tests/column_encryption_rbac_test.rs) rather than hardcoding
    // credentials, by letting sqlx parse it and reading the pieces back out.
    let url = url::Url::parse(database_url).expect("TEST_DATABASE_URL must be a valid URL");
    ConnectionConfig {
        host: url.host_str().unwrap_or("localhost").to_string(),
        port: url.port().unwrap_or(5432),
        database: url.path().trim_start_matches('/').to_string(),
        username: url.username().to_string(),
        password: url.password().unwrap_or("").to_string(),
        ssl_mode: None,
        timeout_seconds: 10,
        retry_attempts: 1,
        options: HashMap::new(),
    }
}

async fn cleanup(pool: &PgPool, table: &str) {
    let _ = sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
        .execute(pool)
        .await;
}

fn tool_call(sql: &str) -> ToolCallParams {
    let mut arguments = HashMap::new();
    arguments.insert("sql".to_string(), json!(sql));
    ToolCallParams {
        name: "execute_query".to_string(),
        arguments: Some(arguments),
    }
}

fn rows_of(response: &serde_json::Value) -> &Vec<serde_json::Value> {
    response
        .get("rows")
        .and_then(|v| v.as_array())
        .expect("response should have a rows array")
}

#[tokio::test]
#[ignore] // requires a real Postgres, see TEST_DATABASE_URL above
async fn execute_query_as_admin_decrypts_column_values() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_secrets_admin";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    rbac.grant_permission("Admin", table, "value", true, true, false)
        .await
        .expect("failed to grant admin permission");

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac.clone()));

    let context = mcp_rs::handlers::database::types::QueryContext::new(
        mcp_rs::handlers::database::types::QueryType::Insert,
    );
    let ciphertext = manager
        .encrypt(table, "value", "top secret plaintext", &context)
        .await
        .expect("encrypt should succeed");
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ($1)"))
        .bind(&ciphertext)
        .execute(&pool)
        .await
        .expect("failed to insert encrypted row");

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    let response = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}") }),
            &admin,
        )
        .await
        .expect("execute_query_as should succeed");

    let rows = rows_of(&response);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0].as_str(), Some("top secret plaintext"));

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_as_unprivileged_role_gets_masked_and_audited() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_secrets_denied";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    // Deliberately grant nothing to the "User" role.

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac.clone()));

    let context = mcp_rs::handlers::database::types::QueryContext::new(
        mcp_rs::handlers::database::types::QueryType::Insert,
    );
    let ciphertext = manager
        .encrypt(table, "value", "top secret plaintext", &context)
        .await
        .expect("encrypt should succeed");
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ($1)"))
        .bind(&ciphertext)
        .execute(&pool)
        .await
        .expect("failed to insert encrypted row");

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let mut limited = AuthUser::new("limited-user".to_string(), "limited-user".to_string());
    limited.roles.insert(Role::User);

    let response = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}") }),
            &limited,
        )
        .await
        .expect("execute_query_as should succeed (masked, not an error)");

    let rows = rows_of(&response);
    assert_eq!(rows[0][0].as_str(), Some("***ENCRYPTED***"));

    let logs = rbac
        .get_audit_logs(Some("limited-user"), None, 10)
        .await
        .expect("failed to read audit logs");
    assert!(
        logs.iter().any(|l| !l.success),
        "expected a denied-decrypt audit log entry for limited-user"
    );

    // The existing MCP-facing call_tool path has no identity available at
    // all, so it must default to masking the same way (fail-closed).
    let no_identity_response = handler
        .call_tool(tool_call(&format!("SELECT value FROM {table}")))
        .await
        .expect("call_tool should succeed (masked, not an error)");
    let no_identity_rows = rows_of(&no_identity_response);
    assert_eq!(no_identity_rows[0][0].as_str(), Some("***ENCRYPTED***"));

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_join_with_ambiguous_encrypted_column_is_masked() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let left_table = "encryption_it_join_left";
    let right_table = "encryption_it_join_right";
    cleanup(&pool, left_table).await;
    cleanup(&pool, right_table).await;
    sqlx::query(&format!(
        "CREATE TABLE {left_table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create left table");
    sqlx::query(&format!(
        "CREATE TABLE {right_table} (id SERIAL PRIMARY KEY, left_id INT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create right table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    rbac.grant_permission("Admin", left_table, "value", true, true, false)
        .await
        .expect("failed to grant admin permission");

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{left_table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac));

    let context = mcp_rs::handlers::database::types::QueryContext::new(
        mcp_rs::handlers::database::types::QueryType::Insert,
    );
    let ciphertext = manager
        .encrypt(left_table, "value", "top secret plaintext", &context)
        .await
        .expect("encrypt should succeed");
    sqlx::query(&format!("INSERT INTO {left_table} (value) VALUES ($1)"))
        .bind(&ciphertext)
        .execute(&pool)
        .await
        .expect("failed to insert into left table");
    // A NULL row alongside the ciphertext one: NULL carries no ciphertext to
    // hide, so it must survive Unknown-provenance masking unchanged, the
    // same way it already survives the encrypted-column decrypt path.
    sqlx::query(&format!("INSERT INTO {left_table} (value) VALUES (NULL)"))
        .execute(&pool)
        .await
        .expect("failed to insert null row into left table");
    sqlx::query(&format!("INSERT INTO {right_table} (left_id) VALUES (1)"))
        .execute(&pool)
        .await
        .expect("failed to insert into right table");
    sqlx::query(&format!("INSERT INTO {right_table} (left_id) VALUES (2)"))
        .execute(&pool)
        .await
        .expect("failed to insert second row into right table");

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    // `value` is unqualified across a two-table join, so column_provenance
    // can't attribute it to either table even though the admin has full
    // permission on encryption_it_join_left.value specifically. Since its
    // provenance can't be proven, it must be masked rather than risk
    // exposing ciphertext for what might be an encrypted column - even to
    // an otherwise-fully-privileged caller.
    let response = handler
        .execute_query_as(
            json!({
                "sql": format!(
                    "SELECT value FROM {left_table} l JOIN {right_table} r ON l.id = r.left_id ORDER BY l.id"
                )
            }),
            &admin,
        )
        .await
        .expect("query should succeed (masked, not an error)");

    let rows = rows_of(&response);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0].as_str(), Some("***UNKNOWN_PROVENANCE***"));
    assert!(
        rows[1][0].is_null(),
        "NULL should survive Unknown-provenance masking, got: {:?}",
        rows[1][0]
    );

    cleanup(&pool, left_table).await;
    cleanup(&pool, right_table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_with_unsupported_statement_shape_is_refused() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_secrets_cte";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    rbac.grant_permission("Admin", table, "value", true, true, false)
        .await
        .expect("failed to grant admin permission");

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac));

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    // column_provenance refuses to reason about CTEs at all (not even a
    // per-column Unknown), so with column encryption configured for this
    // database the whole query must be refused rather than returned
    // unmasked.
    let result = handler
        .execute_query_as(
            json!({
                "sql": format!(
                    "WITH t AS (SELECT value FROM {table}) SELECT value FROM t"
                )
            }),
            &admin,
        )
        .await;

    assert!(
        result.is_err(),
        "expected the unsupported statement shape to be refused, got: {result:?}"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_rejects_multi_statement_sql_before_it_ever_runs() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_multi_statement";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    rbac.grant_permission("Admin", table, "value", true, true, false)
        .await
        .expect("failed to grant admin permission");

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac));

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    // A stacked, multi-statement payload: if the provenance check ran only
    // *after* execution (as it did before this fix), the DROP TABLE here
    // would already have run against the real database by the time the
    // "unsupported statement shape" error was raised. It must be rejected
    // before the SQL ever reaches the connection at all.
    let result = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}; DROP TABLE {table};") }),
            &admin,
        )
        .await;

    assert!(
        result.is_err(),
        "expected the multi-statement query to be refused, got: {result:?}"
    );

    let table_still_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
    )
    .bind(table)
    .fetch_one(&pool)
    .await
    .expect("failed to check table existence");
    assert!(
        table_still_exists,
        "the DROP TABLE statement must never have executed"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_denial_writes_exactly_one_audit_row_for_many_result_rows() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_audit_batching";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    // Deliberately grant nothing to the "User" role.

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac.clone()));

    let context = mcp_rs::handlers::database::types::QueryContext::new(
        mcp_rs::handlers::database::types::QueryType::Insert,
    );
    for i in 0..5 {
        let ciphertext = manager
            .encrypt(table, "value", &format!("secret-{i}"), &context)
            .await
            .expect("encrypt should succeed");
        sqlx::query(&format!("INSERT INTO {table} (value) VALUES ($1)"))
            .bind(&ciphertext)
            .execute(&pool)
            .await
            .expect("failed to insert encrypted row");
    }

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    // encryption_audit_log is never cleared between test runs, so a
    // hardcoded user_id would accumulate stale rows across repeated runs of
    // this same test - use a fresh one each time instead.
    let user_id = format!("batching-test-user-{}", uuid::Uuid::new_v4());
    let mut limited = AuthUser::new(user_id.clone(), user_id.clone());
    limited.roles.insert(Role::User);

    let response = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}") }),
            &limited,
        )
        .await
        .expect("query should succeed (masked, not an error)");

    let rows = rows_of(&response);
    assert_eq!(rows.len(), 5);
    for row in rows {
        assert_eq!(row[0].as_str(), Some("***ENCRYPTED***"));
    }

    // The permission check and denial are the same fact for all 5 rows of
    // this one column - decrypt_batch() must check and audit it once, not
    // once per row.
    let logs = rbac
        .get_audit_logs(Some(&user_id), None, 100)
        .await
        .expect("failed to read audit logs");
    let denied_count = logs.iter().filter(|l| !l.success).count();
    assert_eq!(
        denied_count, 1,
        "expected exactly one denied-decrypt audit row for a 5-row result, got {denied_count}"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_success_writes_exactly_one_audit_row_for_many_result_rows() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_audit_batching_success";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    rbac.grant_permission("Admin", table, "value", true, true, false)
        .await
        .expect("failed to grant admin permission");

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac.clone()));

    let context = mcp_rs::handlers::database::types::QueryContext::new(
        mcp_rs::handlers::database::types::QueryType::Insert,
    );
    for i in 0..5 {
        let ciphertext = manager
            .encrypt(table, "value", &format!("secret-{i}"), &context)
            .await
            .expect("encrypt should succeed");
        sqlx::query(&format!("INSERT INTO {table} (value) VALUES ($1)"))
            .bind(&ciphertext)
            .execute(&pool)
            .await
            .expect("failed to insert encrypted row");
    }

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    // encryption_audit_log is never cleared between test runs, so a
    // hardcoded user_id would accumulate stale rows across repeated runs of
    // this same test - use a fresh one each time instead.
    let user_id = format!("success-batching-test-user-{}", uuid::Uuid::new_v4());
    let mut admin = AuthUser::new(user_id.clone(), user_id.clone());
    admin.roles.insert(Role::Admin);

    let response = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}") }),
            &admin,
        )
        .await
        .expect("query should succeed");

    let rows = rows_of(&response);
    assert_eq!(rows.len(), 5);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row[0].as_str(), Some(format!("secret-{i}")).as_deref());
    }

    // Five successful decrypts of the same column in one query are one
    // fact, not five - decrypt_batch() must audit it once, not once per row.
    let logs = rbac
        .get_audit_logs(Some(&user_id), None, 100)
        .await
        .expect("failed to read audit logs");
    let success_count = logs.iter().filter(|l| l.success).count();
    assert_eq!(
        success_count, 1,
        "expected exactly one successful-decrypt audit row for a 5-row result, got {success_count}"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_rejects_non_select_even_without_column_encryption_configured() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_no_encryption_configured";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    // No .with_column_encryption(...) at all - execute_query's own "SELECT
    // statements only" contract must still be enforced.
    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler");
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let stacked = handler
        .call_tool(tool_call(&format!(
            "SELECT id FROM {table}; DROP TABLE {table};"
        )))
        .await;
    assert!(
        stacked.is_err(),
        "expected a stacked multi-statement payload to be refused, got: {stacked:?}"
    );

    let non_select = handler
        .call_tool(tool_call(&format!("DELETE FROM {table}")))
        .await;
    assert!(
        non_select.is_err(),
        "expected a non-SELECT statement to be refused, got: {non_select:?}"
    );

    let table_still_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
    )
    .bind(table)
    .fetch_one(&pool)
    .await
    .expect("failed to check table existence");
    assert!(
        table_still_exists,
        "the DROP TABLE statement must never have executed"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_masks_malformed_ciphertext_instead_of_failing_the_whole_query() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "encryption_it_malformed_ciphertext";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    let rbac = Arc::new(ColumnEncryptionRbac::new(pool.clone()));
    rbac.grant_permission("Admin", table, "value", true, true, false)
        .await
        .expect("failed to grant admin permission");

    let mut config = ColumnEncryptionConfig::default();
    config.encrypted_columns.push(format!("{table}.value"));
    let manager = Arc::new(ColumnEncryptionManager::with_rbac(config, rbac));

    let context = mcp_rs::handlers::database::types::QueryContext::new(
        mcp_rs::handlers::database::types::QueryType::Insert,
    );
    let good_ciphertext = manager
        .encrypt(table, "value", "top secret plaintext", &context)
        .await
        .expect("encrypt should succeed");
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ($1)"))
        .bind(&good_ciphertext)
        .execute(&pool)
        .await
        .expect("failed to insert well-formed encrypted row");
    // Not valid base64 / not an EncryptedData payload at all - simulates
    // corrupt data, a manual edit, or a bug elsewhere that wrote something
    // other than real ciphertext into this encrypted column.
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ($1)"))
        .bind("not valid ciphertext at all")
        .execute(&pool)
        .await
        .expect("failed to insert malformed row");

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler")
        .with_column_encryption(manager);
    handler
        .add_database(
            "pg".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    // One malformed value must not fail the whole query: the good row still
    // decrypts, and only the malformed row gets masked.
    let response = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table} ORDER BY id") }),
            &admin,
        )
        .await
        .expect("query should succeed despite one malformed ciphertext value");

    let rows = rows_of(&response);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0][0].as_str(), Some("top secret plaintext"));
    assert_eq!(rows[1][0].as_str(), Some("***DECRYPTION_FAILED***"));

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn execute_query_honors_explicit_engine_argument_over_the_active_one() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(_pool) = try_connect().await else {
        return;
    };

    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler");
    // The first engine added becomes active automatically.
    handler
        .add_database(
            "real".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(&database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add the real database");
    // Registered but never made active - unreachable, so any query actually
    // dispatched to it (rather than silently falling back to "real") fails.
    handler
        .add_database(
            "unreachable".to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: ConnectionConfig {
                    host: "localhost".to_string(),
                    port: 1,
                    ..connection_config_from_url(&database_url)
                },
                ..Default::default()
            },
        )
        .await
        .expect("failed to add the unreachable database");

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    // No `engine` argument - uses the active one ("real") and succeeds.
    handler
        .execute_query_as(json!({ "sql": "SELECT 1" }), &admin)
        .await
        .expect("query against the active engine should succeed");

    // `execute_query`'s own input schema documents an `engine` argument that
    // selects a specific engine instead of the active one - it must actually
    // be honored, not silently ignored in favor of whichever engine happens
    // to be active. Explicitly targeting "unreachable" must reach that
    // engine (and fail), not quietly fall back to "real".
    let result = handler
        .execute_query_as(
            json!({ "sql": "SELECT 1", "engine": "unreachable" }),
            &admin,
        )
        .await;
    assert!(
        result.is_err(),
        "expected the query to be dispatched to the unreachable engine and fail, got: {result:?}"
    );
}

/// No `TEST_DATABASE_URL`/real Postgres needed for this one either: a
/// non-string `engine` must be rejected before any engine/pool lookup at
/// all, so there's nothing here that needs a registered database.
#[tokio::test]
async fn execute_query_rejects_non_string_engine_argument() {
    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler");
    // Deliberately never call add_database().

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    // A present-but-non-string `engine` (a JSON number here) must be
    // rejected outright, not silently treated as "not specified" and
    // dispatched to whichever engine happens to be active.
    let result = handler
        .execute_query_as(json!({ "sql": "SELECT 1", "engine": 123 }), &admin)
        .await;

    assert!(
        result.is_err(),
        "expected a non-string 'engine' argument to be rejected, got: {result:?}"
    );
}

/// No `TEST_DATABASE_URL`/real Postgres needed for this one: it's exercising
/// the "no active engine registered yet" precondition, which by definition
/// never gets as far as a real connection.
#[tokio::test]
async fn execute_query_with_no_active_engine_returns_an_error_not_a_panic() {
    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler");
    // Deliberately never call add_database().

    let mut admin = AuthUser::new("admin-user".to_string(), "admin-user".to_string());
    admin.roles.insert(Role::Admin);

    let result = handler
        .execute_query_as(json!({ "sql": "SELECT 1" }), &admin)
        .await;

    assert!(
        result.is_err(),
        "expected a controlled error, got: {result:?}"
    );
}
