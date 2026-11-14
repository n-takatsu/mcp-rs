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
        // 前置きは1つだけ（以前は`Operation failed: Query execution failed: …`）
        assert!(!err.contains("Operation failed"), "{column}: {err}");
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

/// 結果のバイト数の上限（#539）。上限を超えた行は返さず、1行だけで超える
/// なら大きな値を切り詰める
#[tokio::test]
async fn large_results_are_limited_by_bytes() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let mut settings = settings_from_url(&url);
    settings.max_result_bytes = Some(100_000);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    // 1行は約1KB。100KBに収まる行だけを返す
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT n, repeat('x', 1000) AS s FROM generate_series(1, 1000) AS n" }),
    )
    .await
    .unwrap();
    let rows = result["rows"].as_array().unwrap();
    assert_eq!(result["truncated"], json!(true));
    assert_eq!(result["max_result_bytes"], json!(100_000));
    assert!(rows.len() > 50 && rows.len() < 100, "{}", rows.len());
    assert_eq!(result["total_rows"], json!(rows.len()));
    assert!(result.to_string().len() < 150_000);

    // 上限に収まる結果は、そのまま
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT repeat('x', 1000) FROM generate_series(1, 10)" }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(false));
    assert_eq!(result["rows"].as_array().unwrap().len(), 10);

    // 1行だけで上限を超える: 大きな値を切り詰め、小さな値は残す
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT 7 AS n, repeat('y', 1000000) AS big, decode(repeat('ff', 500000), 'hex') AS raw" }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(true));
    let row = &result["rows"][0];
    assert_eq!(row[0], json!(7));
    let big = row[1].as_str().unwrap();
    assert!(big.starts_with("yyyy"), "{}", &big[..20]);
    assert!(
        big.contains("[cut: 1000000 bytes in total"),
        "{}",
        &big[big.len() - 100..]
    );
    assert!(big.len() < 100_000);
    assert!(row[2]
        .as_str()
        .unwrap()
        .contains("binary value of 500000 bytes"));
}

/// 大きな行が多い結果でも、最初の行で止める（#539）。データベースも残りの
/// 行を作らない（カーソルを閉じる）ので、すぐに返る
#[tokio::test]
async fn huge_rows_stop_at_the_first_row() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url))
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT repeat('x', 20000000) FROM generate_series(1, 1000)" }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(true));
    assert_eq!(result["rows"].as_array().unwrap().len(), 1);
    // 全部の行（20GB）を作らせたら、時間の上限（30秒）まで終わらない
    assert!(started.elapsed() < std::time::Duration::from_secs(20));

    // 末尾の`;`があっても動く（カーソルの中には書けないので除く）
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT 1 AS one;  " }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([[1]]));
}

/// 1行目だけ小さく、後の行が大きい結果でも、上限を超えた行を多く作らせない
/// （PR #545のレビュー）。以前は、2回目の`FETCH`が500行になり、sqlxが
/// 残りの行を受け取って捨てる間、PostgreSQLは行を作り続けた。1行ごとに
/// 0.05秒待たせ、多く作らせると時間の上限（3秒）を超えるようにする
#[tokio::test]
async fn rows_growing_after_a_small_first_row_are_not_overfetched() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let mut settings = settings_from_url(&url);
    settings.query_timeout_seconds = Some(3);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT CASE WHEN n = 1 THEN 'x' ELSE repeat('y', 4000000) END AS s, \
                        pg_sleep(CASE WHEN n = 1 THEN 0 ELSE 0.05 END) AS wait \
                        FROM generate_series(1, 1000) AS n" }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(true));
    // 10MBに収まるのは、1行目と4MBの行2つ
    assert_eq!(result["rows"].as_array().unwrap().len(), 3);
}

/// `execute_command`の中で変えたセッションの設定・ロールは、接続をプールに
/// 戻すときに元に戻る（#539）
#[tokio::test]
async fn session_settings_do_not_stay_on_pooled_connections() {
    let Some((url, pool)) = test_database().await else {
        return;
    };
    let table = unique_name("sess");
    let role = unique_name("other_role");
    sqlx::query(&format!("CREATE TABLE {table} (x int)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("INSERT INTO {table} VALUES (1)"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("CREATE ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("GRANT {role} TO CURRENT_USER"))
        .execute(&pool)
        .await
        .unwrap();

    let mut settings = settings_from_url(&url);
    settings.allow_write = Some(true);
    // 接続を1つにして、同じ接続を使い回させる
    settings.max_connections = Some(1);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();
    let before = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT current_user::text, current_setting('search_path')" }),
    )
    .await
    .unwrap()["rows"]
        .clone();

    let result = call(
        &handler,
        "execute_command",
        json!({ "sql": format!(
            "UPDATE {table} SET x = x WHERE set_config('role', '{role}', false) IS NOT NULL \
             AND set_config('search_path', 'pg_catalog', false) IS NOT NULL"
        ) }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows_affected"], json!(1));

    // 戻した接続の後始末は非同期なので、少し待つ
    let mut after = json!(null);
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        after = call(
            &handler,
            "execute_query",
            json!({ "sql": "SELECT current_user::text, current_setting('search_path')" }),
        )
        .await
        .unwrap()["rows"]
            .clone();
        if after == before {
            break;
        }
    }
    assert_eq!(after, before);

    drop(handler);
    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}

/// `ssl_root_cert`を書いたのに、`ssl_mode`が証明書を確かめない設定なら、
/// 起動しない（PR #545のレビュー）
#[tokio::test]
async fn ssl_root_cert_needs_a_verifying_ssl_mode() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let mut settings = settings_from_url(&url);
    settings.ssl_mode = Some("prefer".to_string());
    settings.ssl_root_cert = Some("ca.pem".to_string());
    let err = match DatabaseHandler::from_settings(&settings).await {
        Ok(_) => panic!("ssl_root_cert with ssl_mode = prefer must not start"),
        Err(err) => err,
    };
    assert!(
        err.contains("ssl_root_cert") && err.contains("verify-full"),
        "{err}"
    );
}

/// 監査ログには、パラメーターの値を書かない（#539）。クライアントへの
/// エラーには、PostgreSQLの文をそのまま返す
#[tokio::test]
async fn parameter_values_are_not_written_to_the_audit_log() {
    let Some((url, _pool)) = test_database().await else {
        return;
    };
    let audit = std::sync::Arc::new(mcp_rs::security::AuditLogger::with_defaults());
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url))
        .await
        .unwrap()
        .with_audit_logger(audit.clone());

    // mcp-rsの型の検査のエラー
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT $1::uuid", "params": ["secret-value-1"] }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("$1") && err.contains("uuid"), "{err}");
    assert!(!err.contains("secret-value-1"), "{err}");

    // PostgreSQLのキャストのエラー（値を含む）
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT $1::text::int", "params": ["secret-value-2"] }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("secret-value-2"), "{err}");

    let entries = audit.get_all_entries().await;
    let logged = serde_json::to_string(&entries).unwrap();
    assert_eq!(entries.len(), 2, "{logged}");
    assert!(!logged.contains("secret-value"), "{logged}");
    assert!(logged.contains("invalid input syntax"), "{logged}");
}
