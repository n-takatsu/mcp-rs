//! Integration tests proving `DatabaseHandler` actually enforces
//! `MicroSegmentation` (`src/zero_trust/micro_segmentation.rs`) table-level
//! access control on real queries, instead of that engine sitting
//! unreachable from the query execution path (Issue #226).
//!
//! Requires a real PostgreSQL instance reachable via `TEST_DATABASE_URL`.
//! Soft-skips (prints a message and returns) when no such database is
//! configured or reachable, since CI has none.

#![cfg(feature = "database")]

use mcp_rs::handlers::database::handler::DatabaseHandler;
use mcp_rs::handlers::database::types::{ConnectionConfig, DatabaseConfig, DatabaseType};
use mcp_rs::mcp::{McpHandler, ToolCallParams};
use mcp_rs::security::auth::types::{AuthUser, Role};
use mcp_rs::zero_trust::micro_segmentation::{AccessPolicy, MicroSegmentation};
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

/// The engine id used to register the database under - `handler.rs` builds
/// resource strings as `database:{engine_id}:{table}`, so tests must use the
/// same id to construct matching `AccessPolicy::resource_patterns`.
const ENGINE_ID: &str = "pg";

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

async fn handler_with_database(database_url: &str) -> DatabaseHandler {
    let handler = DatabaseHandler::new(None)
        .await
        .expect("failed to create handler");
    handler
        .add_database(
            ENGINE_ID.to_string(),
            DatabaseConfig {
                database_type: DatabaseType::PostgreSQL,
                connection: connection_config_from_url(database_url),
                ..Default::default()
            },
        )
        .await
        .expect("failed to add database");
    handler
}

#[tokio::test]
#[ignore] // requires a real Postgres, see TEST_DATABASE_URL above
async fn allowed_role_can_query_the_table() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "microseg_it_allowed";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ('hello')"))
        .execute(&pool)
        .await
        .expect("failed to insert row");

    let mut engine = MicroSegmentation::new();
    engine.add_global_policy(
        AccessPolicy::new("allow-analyst", format!("database:{ENGINE_ID}:{table}"), 0)
            .with_action("read")
            .with_role("analyst"),
    );

    let handler = handler_with_database(&database_url)
        .await
        .with_micro_segmentation(Arc::new(engine));

    let mut analyst = AuthUser::new("analyst-user".to_string(), "analyst-user".to_string());
    analyst.roles.insert(Role::Custom("analyst".to_string()));

    let response = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}") }),
            &analyst,
        )
        .await
        .expect("query should succeed for an allowed role");

    let rows = response
        .get("rows")
        .and_then(|v| v.as_array())
        .expect("response should have a rows array");
    assert_eq!(rows.len(), 1);

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn disallowed_role_is_denied_not_masked() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "microseg_it_denied";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ('secret')"))
        .execute(&pool)
        .await
        .expect("failed to insert row");

    let mut engine = MicroSegmentation::new();
    engine.add_global_policy(
        AccessPolicy::new("allow-analyst", format!("database:{ENGINE_ID}:{table}"), 0)
            .with_action("read")
            .with_role("analyst"),
    );

    let handler = handler_with_database(&database_url)
        .await
        .with_micro_segmentation(Arc::new(engine));

    // A "guest" role never matches the "analyst"-only policy above, and
    // there is no other policy that would match this resource, so
    // MicroSegmentation::evaluate_access must fail closed.
    let mut guest = AuthUser::new("guest-user".to_string(), "guest-user".to_string());
    guest.roles.insert(Role::Guest);

    let result = handler
        .execute_query_as(
            json!({ "sql": format!("SELECT value FROM {table}") }),
            &guest,
        )
        .await;

    assert!(
        result.is_err(),
        "expected the query to be denied outright (not partially masked), got: {result:?}"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn join_with_one_denied_table_denies_the_whole_query() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let left_table = "microseg_it_join_left";
    let right_table = "microseg_it_join_right";
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
    sqlx::query(&format!("INSERT INTO {left_table} (value) VALUES ('x')"))
        .execute(&pool)
        .await
        .expect("failed to insert into left table");
    sqlx::query(&format!("INSERT INTO {right_table} (left_id) VALUES (1)"))
        .execute(&pool)
        .await
        .expect("failed to insert into right table");

    // Only the left table has a matching policy - the right table has none,
    // so it must fail closed even though the caller is otherwise privileged
    // enough for the left table alone.
    let mut engine = MicroSegmentation::new();
    engine.add_global_policy(
        AccessPolicy::new(
            "allow-left-only",
            format!("database:{ENGINE_ID}:{left_table}"),
            0,
        )
        .with_action("read")
        .with_role("analyst"),
    );

    let handler = handler_with_database(&database_url)
        .await
        .with_micro_segmentation(Arc::new(engine));

    let mut analyst = AuthUser::new("analyst-user".to_string(), "analyst-user".to_string());
    analyst.roles.insert(Role::Custom("analyst".to_string()));

    let result = handler
        .execute_query_as(
            json!({
                "sql": format!(
                    "SELECT l.value FROM {left_table} l JOIN {right_table} r ON l.id = r.left_id"
                )
            }),
            &analyst,
        )
        .await;

    assert!(
        result.is_err(),
        "expected the whole query to be denied when any touched table lacks a matching policy, got: {result:?}"
    );

    cleanup(&pool, left_table).await;
    cleanup(&pool, right_table).await;
}

#[tokio::test]
#[ignore]
async fn no_identity_call_tool_path_is_denied_for_any_positive_trust_requirement() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "microseg_it_no_identity";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");

    // min_trust_score > 0 can never be satisfied by the identity-less
    // call_tool path, since execute_query_core assigns trust_score = 0 when
    // auth_user is None (device_verifier/network_analyzer aren't wired in
    // yet to provide a real score) - this is the fail-closed default.
    let mut engine = MicroSegmentation::new();
    engine.add_global_policy(AccessPolicy::new(
        "requires-some-trust",
        format!("database:{ENGINE_ID}:{table}"),
        1,
    ));

    let handler = handler_with_database(&database_url)
        .await
        .with_micro_segmentation(Arc::new(engine));

    let result = handler
        .call_tool(tool_call(&format!("SELECT value FROM {table}")))
        .await;

    assert!(
        result.is_err(),
        "expected the identity-less call_tool path to be denied by a positive trust requirement, got: {result:?}"
    );

    cleanup(&pool, table).await;
}

#[tokio::test]
#[ignore]
async fn unset_micro_segmentation_does_not_affect_existing_behavior() {
    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    let Some(pool) = try_connect().await else {
        return;
    };

    let table = "microseg_it_unset";
    cleanup(&pool, table).await;
    sqlx::query(&format!(
        "CREATE TABLE {table} (id SERIAL PRIMARY KEY, value TEXT)"
    ))
    .execute(&pool)
    .await
    .expect("failed to create test table");
    sqlx::query(&format!("INSERT INTO {table} (value) VALUES ('x')"))
        .execute(&pool)
        .await
        .expect("failed to insert row");

    // Deliberately never call .with_micro_segmentation(...).
    let handler = handler_with_database(&database_url).await;

    let response = handler
        .call_tool(tool_call(&format!("SELECT value FROM {table}")))
        .await
        .expect("query should succeed when micro-segmentation is not configured at all");
    let rows = response
        .get("rows")
        .and_then(|v| v.as_array())
        .expect("response should have a rows array");
    assert_eq!(rows.len(), 1);

    cleanup(&pool, table).await;
}
