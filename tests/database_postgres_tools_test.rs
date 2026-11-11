//! `[database]`のPostgreSQLを、MCPのツールとして使う結合テスト（#453）。
//!
//! 実際のPostgreSQLが要る。`TEST_DATABASE_URL`（例:
//! `postgres://testuser:testpass@localhost:5433/mcptest`）が無ければ、何も
//! せずに通る。CIでは、Run Integration Testsのジョブのpostgresのサービスを
//! 使う。
#![cfg(feature = "database")]

use mcp_rs::config::DatabaseSettings;
use mcp_rs::handlers::database::DatabaseHandler;
use mcp_rs::mcp::{McpHandler, ToolCallParams};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;

/// `TEST_DATABASE_URL`の接続先と、テストの準備に使うプール
async fn test_database() -> Option<(String, PgPool)> {
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_default();
    if url.is_empty() {
        println!("skipping: TEST_DATABASE_URL not set");
        return None;
    }
    let pool = PgPool::connect(&url)
        .await
        .expect("TEST_DATABASE_URL is set but PostgreSQL is not reachable");
    Some((url, pool))
}

fn settings_from_url(url: &str) -> DatabaseSettings {
    let url = url::Url::parse(url).expect("TEST_DATABASE_URL must be a URL");
    DatabaseSettings {
        engine: "postgresql".to_string(),
        host: url.host_str().map(str::to_string),
        port: url.port(),
        database: url.path().trim_start_matches('/').to_string(),
        username: url.username().to_string(),
        password: url.password().map(str::to_string),
        ssl_mode: Some("disable".to_string()),
        ..Default::default()
    }
}

/// 毎回別の名前の表を使う（テストを並べて動かすため）
fn unique_name(prefix: &str) -> String {
    format!("{}_{}", prefix, uuid::Uuid::new_v4().simple())
}

async fn call(handler: &DatabaseHandler, tool: &str, arguments: Value) -> Result<Value, String> {
    let arguments: HashMap<String, Value> = serde_json::from_value(arguments).unwrap();
    let result = handler
        .call_tool(ToolCallParams {
            name: tool.to_string(),
            arguments: Some(arguments),
        })
        .await
        .map_err(|e| e.to_string())?;
    assert_eq!(result["isError"], json!(false));
    let text = result["content"][0]["text"].as_str().expect("text content");
    Ok(serde_json::from_str(text).expect("JSON in the text content"))
}

async fn tool_names(handler: &DatabaseHandler) -> Vec<String> {
    handler
        .list_tools()
        .await
        .unwrap()
        .into_iter()
        .map(|tool| tool.name)
        .collect()
}

#[tokio::test]
async fn read_only_by_default() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let table = unique_name("ro");
    sqlx::query(&format!(
        "CREATE TABLE {table} (id int PRIMARY KEY, name text)"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!("INSERT INTO {table} VALUES (1, 'one')"))
        .execute(&pool)
        .await
        .unwrap();

    let handler = DatabaseHandler::from_settings(&settings_from_url(&url))
        .await
        .unwrap();
    let names = tool_names(&handler).await;
    assert_eq!(names, vec!["execute_query", "get_schema"]);
    let tools = handler.list_tools().await.unwrap();
    assert!(tools
        .iter()
        .all(|t| t.annotations.as_ref().unwrap().read_only_hint == Some(true)));

    // 書き込みのツールは使えない
    let err = call(
        &handler,
        "execute_command",
        json!({ "sql": format!("DELETE FROM {table}") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("read-only"), "{err}");

    // 読める。パラメーターはバインドする
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT id, name FROM {table} WHERE id = $1"), "params": [1] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([[1, "one"]]));
    assert_eq!(result["columns"][0]["name"], json!("id"));
    assert_eq!(result["columns"][0]["type"], json!("INT4"));

    // 文字列のパラメーターは、SQLに埋め込まないので、引用符もそのまま値になる
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT $1::text AS v", "params": ["it's'; DROP TABLE x; --"] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([["it's'; DROP TABLE x; --"]]));

    // 表の一覧
    let schema = call(&handler, "get_schema", json!({})).await.unwrap();
    let found = schema["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == json!(table))
        .expect("the test table is in the schema");
    assert_eq!(found["primary_keys"], json!(["id"]));

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
}

/// SELECTに見えて書き込むSQLは、`allow_write = true`でも`execute_query`では
/// 書き込めない（構文の確認と、読み取り専用のトランザクションの両方）
#[tokio::test]
async fn execute_query_cannot_write() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let table = unique_name("guard");
    let function = unique_name("write_fn");
    sqlx::query(&format!("CREATE TABLE {table} (id int)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {table} VALUES (1), (2)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE FUNCTION {function}() RETURNS int LANGUAGE sql AS $$ DELETE FROM {table}; SELECT 0 $$"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut settings = settings_from_url(&url);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    for sql in [
        format!("WITH d AS (DELETE FROM {table} RETURNING *) SELECT * FROM d"),
        format!("SELECT * INTO {table}_copy FROM {table}"),
        format!("DELETE FROM {table}"),
    ] {
        let err = call(&handler, "execute_query", json!({ "sql": sql }))
            .await
            .unwrap_err();
        assert!(
            err.contains("SELECT") || err.contains("statement"),
            "{sql}: {err}"
        );
    }
    // 構文はSELECTでも、書き込む関数はデータベースが拒否する
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT {function}()") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("read-only"), "{err}");
    // 既定の読み取り専用を外しても、毎回のトランザクションは読み取り専用
    call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT set_config('default_transaction_read_only', 'off', false)" }),
    )
    .await
    .unwrap();
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT {function}()") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("read-only"), "{err}");

    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "no rows were deleted");

    sqlx::query(&format!("DROP FUNCTION {function}()"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn execute_command_when_writes_are_allowed() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let table = unique_name("rw");
    sqlx::query(&format!("CREATE TABLE {table} (id int, name text)"))
        .execute(&pool)
        .await
        .unwrap();

    let mut settings = settings_from_url(&url);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();
    let tools = handler.list_tools().await.unwrap();
    let command = tools
        .iter()
        .find(|t| t.name == "execute_command")
        .expect("execute_command is listed when allow_write = true");
    assert_eq!(
        command.annotations.as_ref().unwrap().destructive_hint,
        Some(true)
    );

    let result = call(
        &handler,
        "execute_command",
        json!({ "sql": format!("INSERT INTO {table} VALUES ($1, $2), ($3, $4)"), "params": [1, "a", 2, "b"] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows_affected"], json!(2));
    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "the insert was committed");

    // INSERT・UPDATE・DELETE以外は受け付けない
    for sql in [
        format!("DROP TABLE {table}"),
        format!("TRUNCATE {table}"),
        "CREATE TABLE x (id int)".to_string(),
    ] {
        assert!(
            call(&handler, "execute_command", json!({ "sql": sql }))
                .await
                .is_err(),
            "{sql}"
        );
    }

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
}

/// 文字列とNULLのパラメーターは、列の型に合わせて送る。結果で返した値を、
/// そのままパラメーターに戻せる（#453のレビュー）
#[tokio::test]
async fn string_and_null_parameters_follow_the_column_type() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let table = unique_name("typed");
    sqlx::query(&format!(
        "CREATE TABLE {table} (id uuid, d date, amount numeric(10,2), n int, at timestamptz, doc jsonb, raw bytea)"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let mut settings = settings_from_url(&url);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    // INSERT: uuid・日付・numeric・NULL・時刻・JSON・base64のbytea
    let result = call(
        &handler,
        "execute_command",
        json!({
            "sql": format!("INSERT INTO {table} VALUES ($1, $2, $3, $4, $5, $6, $7)"),
            "params": [
                "6f1c2a3e-9b2d-4c1a-8f00-123456789abc",
                "2026-11-11",
                "1.50",
                null,
                "2026-11-11T03:34:56+00:00",
                "{\"a\": 1}",
                "AQI="
            ]
        }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows_affected"], json!(1));

    // 読んだ値を、そのまま条件に使う
    let row = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT id, d, amount, at, raw FROM {table}") }),
    )
    .await
    .unwrap()["rows"][0]
        .clone();
    assert_eq!(row[2], json!("1.50"));
    let found = call(
        &handler,
        "execute_query",
        json!({
            "sql": format!(
                "SELECT count(*)::int AS c FROM {table} WHERE id = $1 AND d >= $2 AND amount = $3 AND at = $4 AND raw = $5 AND n IS NOT DISTINCT FROM $6"
            ),
            "params": [row[0], row[1], row[2], row[3], row[4], null]
        }),
    )
    .await
    .unwrap();
    assert_eq!(found["rows"], json!([[1]]));

    // 同じSQLを、文字列と数値で続けて呼んでも動く（準備済みの文は、推論
    // された型で使い回される。int4のパラメーターに、文字列も数値も合わせる）
    let sql = format!("SELECT count(*)::int AS c FROM {table} WHERE n IS DISTINCT FROM $1");
    for value in [json!("5"), json!(5), json!(5.0), json!(null)] {
        let result = call(
            &handler,
            "execute_query",
            json!({ "sql": sql, "params": [value] }),
        )
        .await
        .unwrap_or_else(|e| panic!("{value}: {e}"));
        assert!(result["rows"][0][0].is_number(), "{value}");
    }

    // 変換できない値は、どのパラメーターかを示すエラー
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT * FROM {table} WHERE id = $1"), "params": ["nope"] }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("$1") && err.contains("uuid"), "{err}");

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
}

/// 変換しない型（money・interval等）には、形式の違う値を送らない。送ると、
/// バイト数が偶然合ったときに別の値として保存される（#453の再レビュー）。
/// textを経由するキャストなら通る
#[tokio::test]
async fn values_are_not_sent_in_the_wrong_binary_format() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let table = unique_name("formats");
    let mood = unique_name("mood");
    let small = unique_name("small");
    for sql in [
        format!("CREATE TYPE {mood} AS ENUM ('happy', 'sad')"),
        format!("CREATE DOMAIN {small} AS int"),
        format!(
            "CREATE TABLE {table} (m money, i interval, addr inet, tags text[], feeling {mood}, s {small})"
        ),
    ] {
        sqlx::query(&sql).execute(&pool).await.unwrap();
    }

    let mut settings = settings_from_url(&url);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    // 8バイトの"$1000.00"・16バイトの"1 day 2 hours 3m"は、以前は化けて
    // 保存された。数値もmoneyの形式とは違う
    for (column, value, type_name) in [
        ("m", json!("$1000.00"), "money"),
        ("m", json!(1000), "money"),
        ("i", json!("1 day 2 hours 3m"), "interval"),
        ("addr", json!("10.0.0.1"), "inet"),
        ("tags", json!("{a}"), "text[]"),
    ] {
        let err = call(
            &handler,
            "execute_command",
            json!({
                "sql": format!("INSERT INTO {table} ({column}) VALUES ($1)"),
                "params": [value]
            }),
        )
        .await
        .unwrap_err();
        assert!(
            err.contains(&format!("$1::text::{type_name}")),
            "{column}: {err}"
        );
    }
    let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "nothing may be stored from a refused parameter");

    // textを経由すれば通る。列挙型の文字列と、intのドメインの数値はそのまま
    call(
        &handler,
        "execute_command",
        json!({
            "sql": format!(
                "INSERT INTO {table} VALUES ($1::text::money, $2::text::interval, $3::text::inet, $4::text::text[], $5, $6)"
            ),
            "params": ["1000.00", "1 day 2 hours 3m", "10.0.0.1", "{a,b}", "happy", 5]
        }),
    )
    .await
    .unwrap();
    let row = call(
        &handler,
        "execute_query",
        json!({
            "sql": format!(
                "SELECT m::numeric::text, i::text, addr::text, array_to_string(tags, ','), feeling::text, s::int FROM {table}"
            )
        }),
    )
    .await
    .unwrap()["rows"][0]
        .clone();
    assert_eq!(
        row,
        json!([
            "1000.00",
            "1 day 02:03:00",
            "10.0.0.1/32",
            "a,b",
            "happy",
            5
        ])
    );

    for sql in [
        format!("DROP TABLE {table}"),
        format!("DROP TYPE {mood}"),
        format!("DROP DOMAIN {small}"),
    ] {
        sqlx::query(&sql).execute(&pool).await.unwrap();
    }
}

/// `execute_query`で取ったセッションの勧告ロックは、接続をプールに戻すときに
/// 外す（#453のレビュー）
#[tokio::test]
async fn advisory_locks_do_not_stay_on_pooled_connections() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let key: i64 = 453_000 + i64::from(std::process::id() % 1000);
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url))
        .await
        .unwrap();
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT pg_advisory_lock($1)", "params": [key] }),
    )
    .await
    .unwrap();
    // VOIDは「表示できない値」ではなくnull
    assert_eq!(result["rows"], json!([[null]]));

    // 戻した接続の後始末は非同期なので、少し待つ
    let mut held = 1;
    for _ in 0..50 {
        held = sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND objid = $1::bigint::oid",
        )
        .bind(key)
        .fetch_one(&pool)
        .await
        .unwrap();
        if held == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(held, 0, "the advisory lock was released");
}

#[tokio::test]
async fn values_are_decoded_by_type() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url))
        .await
        .unwrap();
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT
            12345678901234.5678::numeric(20,4) AS big,
            (-0.0001)::numeric AS small,
            'NaN'::numeric AS nan,
            '6f1c2a3e-9b2d-4c1a-8f00-123456789abc'::uuid AS id,
            '{\"a\": [1, 2]}'::jsonb AS doc,
            '2026-11-11 12:34:56+09'::timestamptz AS at,
            '2026-11-11 12:34:56.5'::timestamp AS local_at,
            '2026-11-11'::date AS day,
            '\\x0102'::bytea AS bytes,
            ARRAY['x', 'y']::text[] AS tags,
            NULL::int AS nothing,
            true AS flag,
            1.5::float8 AS ratio,
            '1 day'::interval AS span" }),
    )
    .await
    .unwrap();
    let row = &result["rows"][0];
    assert_eq!(row[0], json!("12345678901234.5678"));
    assert_eq!(row[1], json!("-0.0001"));
    assert_eq!(row[2], json!("NaN"));
    assert_eq!(row[3], json!("6f1c2a3e-9b2d-4c1a-8f00-123456789abc"));
    assert_eq!(row[4], json!({ "a": [1, 2] }));
    assert_eq!(row[5], json!("2026-11-11T03:34:56+00:00"));
    assert_eq!(row[6], json!("2026-11-11T12:34:56.500"));
    assert_eq!(row[7], json!("2026-11-11"));
    assert_eq!(row[8], json!("AQI="));
    assert_eq!(row[9], json!(["x", "y"]));
    assert_eq!(row[10], Value::Null);
    assert_eq!(row[11], json!(true));
    assert_eq!(row[12], json!(1.5));
    // 読めない型はNULLにせず、そう分かる文字列にする
    assert!(
        row[13].as_str().unwrap().contains("INTERVAL"),
        "{}",
        row[13]
    );
}

#[tokio::test]
async fn rows_are_limited_and_empty_results_keep_columns() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let mut settings = settings_from_url(&url);
    settings.max_rows = Some(3);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT n FROM generate_series(1, 10) AS n" }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([[1], [2], [3]]));
    assert_eq!(result["truncated"], json!(true));

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT n FROM generate_series(1, 3) AS n" }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(false));

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT 1 AS one, 'x'::text AS two WHERE false" }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([]));
    assert_eq!(result["columns"][0]["name"], json!("one"));
    assert_eq!(result["columns"][1]["name"], json!("two"));
}

#[tokio::test]
async fn slow_queries_time_out() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let mut settings = settings_from_url(&url);
    settings.query_timeout_seconds = Some(1);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT pg_sleep(5)" }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("statement timeout"), "{err}");
}

#[tokio::test]
async fn connection_errors_do_not_show_the_password() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let mut settings = settings_from_url(&url);
    settings.password = Some("wrong-secret-password".to_string());
    let err = match DatabaseHandler::from_settings(&settings).await {
        Ok(_) => panic!("connecting with a wrong password must fail"),
        Err(err) => err,
    };
    assert!(err.contains("cannot connect"), "{err}");
    assert!(!err.contains("wrong-secret-password"), "{err}");
}
