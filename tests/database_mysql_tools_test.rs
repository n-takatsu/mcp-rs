//! `[database]`のMySQL・MariaDB（`engine = "mysql"`・`"mariadb"`）を、MCPの
//! ツールとして使う結合テスト（#527）。PostgreSQLの
//! `database_postgres_tools_test.rs`と同じことを確かめる。同じテストを、
//! MySQL（`mysql::`）とMariaDB（`mariadb::`）の両方で動かす。
//!
//! 実際のデータベースが要る。`TEST_MYSQL_URL`（例:
//! `mysql://root:testpass@127.0.0.1:3307/mcptest`）・`TEST_MARIADB_URL`（例:
//! `mysql://root:testpass@127.0.0.1:3308/mcptest`）が無ければ、そのほうの
//! テストは何もせずに通る。CIでは、Run Integration Testsのジョブの
//! mysql・mariadbのサービスを使う。書き込む関数を作るため、`SUPER`か
//! `SYSTEM_VARIABLES_ADMIN`を持つユーザー（CIではroot）でつなぐ。
#![cfg(feature = "mysql-backend")]

use mcp_rs::config::DatabaseSettings;
use mcp_rs::handlers::database::DatabaseHandler;
use mcp_rs::mcp::{McpHandler, ToolCallParams};
use mysql_async::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;

/// テストするデータベースの種類
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    MySql,
    MariaDb,
}

impl Kind {
    /// 接続先を書く環境変数
    fn url_variable(self) -> &'static str {
        match self {
            Kind::MySql => "TEST_MYSQL_URL",
            Kind::MariaDb => "TEST_MARIADB_URL",
        }
    }

    /// `[database]`の`engine`
    fn engine(self) -> &'static str {
        match self {
            Kind::MySql => "mysql",
            Kind::MariaDb => "mariadb",
        }
    }
}

/// `kind`の接続先と、テストの準備に使うプール
async fn test_database(kind: Kind) -> Option<(String, mysql_async::Pool)> {
    let variable = kind.url_variable();
    let url = std::env::var(variable).unwrap_or_default();
    if url.is_empty() {
        println!("skipping: {variable} not set");
        return None;
    }
    let pool = mysql_async::Pool::new(url.as_str());
    pool.get_conn()
        .await
        .unwrap_or_else(|e| panic!("{variable} is set but the database is not reachable: {e}"));
    Some((url, pool))
}

/// 準備のSQLを実行する
async fn run(pool: &mysql_async::Pool, sql: &str) {
    let mut conn = pool.get_conn().await.unwrap();
    conn.query_drop(sql).await.unwrap();
}

fn settings_from_url(url: &str, kind: Kind) -> DatabaseSettings {
    let url = url::Url::parse(url).expect("the test database URL must be a URL");
    DatabaseSettings {
        engine: kind.engine().to_string(),
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

async fn read_only_by_default(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("ro");
    run(
        &pool,
        &format!("CREATE TABLE {table} (id int PRIMARY KEY, name varchar(20))"),
    )
    .await;
    run(&pool, &format!("INSERT INTO {table} VALUES (1, 'one')")).await;

    let handler = DatabaseHandler::from_settings(&settings_from_url(&url, kind))
        .await
        .unwrap();
    assert_eq!(
        tool_names(&handler).await,
        vec!["execute_query", "get_schema"]
    );
    let tools = handler.list_tools().await.unwrap();
    // MySQLの書き方（`?`）を説明する
    assert!(tools[0].input_schema["properties"]["params"]["description"]
        .as_str()
        .unwrap()
        .contains("? placeholders"));

    let err = call(
        &handler,
        "execute_command",
        json!({ "sql": format!("DELETE FROM {table}") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("read-only"), "{err}");

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT id, name FROM {table} WHERE id = ?"), "params": [1] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([[1, "one"]]));
    assert_eq!(result["columns"][0]["name"], json!("id"));
    assert_eq!(result["columns"][0]["type"], json!("int"));

    // 文字列のパラメーターは、SQLに埋め込まないので、引用符もそのまま値になる
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT ? AS v", "params": ["it's'; DROP TABLE x; --"] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([["it's'; DROP TABLE x; --"]]));

    let schema = call(&handler, "get_schema", json!({})).await.unwrap();
    let found = schema["tables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == json!(table))
        .expect("the test table is in the schema");
    assert_eq!(found["primary_keys"], json!(["id"]));

    run(&pool, &format!("DROP TABLE {table}")).await;
}

async fn get_schema_returns_foreign_keys_and_indexes(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let parent = unique_name("parent");
    let child = unique_name("child");
    let view = unique_name("v");
    run(
        &pool,
        &format!("CREATE TABLE {parent} (id int PRIMARY KEY, code varchar(10) UNIQUE)"),
    )
    .await;
    run(
        &pool,
        &format!(
            "CREATE TABLE {child} (id int PRIMARY KEY, parent_id int NOT NULL, \
             note text, INDEX by_parent (parent_id, id), \
             CONSTRAINT fk_parent FOREIGN KEY (parent_id) REFERENCES {parent} (id))"
        ),
    )
    .await;
    run(
        &pool,
        &format!("CREATE VIEW {view} AS SELECT id FROM {parent}"),
    )
    .await;

    let handler = DatabaseHandler::from_settings(&settings_from_url(&url, kind))
        .await
        .unwrap();
    let schema = call(&handler, "get_schema", json!({})).await.unwrap();
    let table = |name: &str| {
        schema["tables"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == json!(name))
            .cloned()
            .unwrap_or_else(|| panic!("{name} is in the schema"))
    };
    let child_info = table(&child);
    assert_eq!(
        child_info["foreign_keys"],
        json!([{ "name": "fk_parent", "column": "parent_id",
                 "referenced_table": parent, "referenced_column": "id" }])
    );
    let by_parent = child_info["indexes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == json!("by_parent"))
        .expect("the by_parent index");
    assert_eq!(by_parent["columns"], json!(["parent_id", "id"]));
    assert_eq!(by_parent["is_unique"], json!(false));
    let note = child_info["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == json!("note"))
        .unwrap();
    assert_eq!(note["type"], json!("text"));
    assert_eq!(note["nullable"], json!(true));
    let code_index = table(&parent)["indexes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["columns"] == json!(["code"]))
        .cloned()
        .expect("the unique index on code");
    assert_eq!(code_index["is_unique"], json!(true));
    // ビューは表ではなく、ビューとして返す
    assert!(schema["views"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["name"] == json!(view)));

    run(&pool, &format!("DROP VIEW {view}")).await;
    run(&pool, &format!("DROP TABLE {child}")).await;
    run(&pool, &format!("DROP TABLE {parent}")).await;
}

async fn execute_query_cannot_write(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("guard");
    let function = unique_name("write_fn");
    run(&pool, &format!("CREATE TABLE {table} (id int)")).await;
    run(&pool, &format!("INSERT INTO {table} VALUES (1), (2)")).await;
    // バイナリログが有効なMySQLで、書き込む関数を作れるようにする
    run(&pool, "SET GLOBAL log_bin_trust_function_creators = 1").await;
    run(
        &pool,
        &format!(
            "CREATE FUNCTION {function}() RETURNS int MODIFIES SQL DATA \
             BEGIN DELETE FROM {table}; RETURN 0; END"
        ),
    )
    .await;

    let mut settings = settings_from_url(&url, kind);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    for sql in [
        format!("SELECT * INTO @copied FROM {table}"),
        format!("DELETE FROM {table}"),
        format!("SELECT 1; DELETE FROM {table}"),
        // MariaDBだけが実行するコメント（PR #573のレビュー）
        format!("SELECT id FROM {table} /*M! INTO @copied */"),
        "SELECT 1 /*M!100100 INTO OUTFILE '/tmp/mcp_rs_x' */".to_string(),
    ] {
        let err = call(&handler, "execute_query", json!({ "sql": sql }))
            .await
            .unwrap_err();
        assert!(
            err.contains("SELECT")
                || err.contains("statement")
                || err.contains("parse")
                || err.contains("/*M!"),
            "{sql}: {err}"
        );
    }
    // そのコメントの中身は、MariaDBだけが実行する（だから拒否する）
    let mut conn = pool.get_conn().await.unwrap();
    let executed: Option<i64> = conn.query_first("SELECT 1 /*M! + 1 */").await.unwrap();
    let expected = match kind {
        Kind::MySql => 1,
        Kind::MariaDb => 2,
    };
    assert_eq!(executed, Some(expected));
    drop(conn);
    // 構文はSELECTでも、書き込む関数はデータベースが拒否する
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT {function}()") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("READ ONLY"), "{err}");

    let mut conn = pool.get_conn().await.unwrap();
    let count: Option<i64> = conn
        .query_first(format!("SELECT count(*) FROM {table}"))
        .await
        .unwrap();
    assert_eq!(count, Some(2), "no rows were deleted");
    drop(conn);

    run(&pool, &format!("DROP FUNCTION {function}")).await;
    run(&pool, &format!("DROP TABLE {table}")).await;
}

async fn execute_command_when_writes_are_allowed(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("rw");
    run(
        &pool,
        &format!("CREATE TABLE {table} (id int AUTO_INCREMENT PRIMARY KEY, name varchar(20))"),
    )
    .await;

    let mut settings = settings_from_url(&url, kind);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();
    assert_eq!(
        tool_names(&handler).await,
        vec!["execute_query", "get_schema", "execute_command"]
    );

    let result = call(
        &handler,
        "execute_command",
        json!({ "sql": format!("INSERT INTO {table} (name) VALUES (?), (?)"), "params": ["a", "b"] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows_affected"], json!(2));
    assert_eq!(result["last_insert_id"], json!(1));

    // DDLは受け付けない
    let err = call(
        &handler,
        "execute_command",
        json!({ "sql": format!("DROP TABLE {table}") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("INSERT, UPDATE, and DELETE"), "{err}");

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT name FROM {table} ORDER BY id") }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([["a"], ["b"]]));

    run(&pool, &format!("DROP TABLE {table}")).await;
}

async fn values_are_decoded_by_type(kind: Kind) {
    let Some((url, _pool)) = test_database(kind).await else {
        return;
    };
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url, kind))
        .await
        .unwrap();
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT CAST(18446744073709551615 AS UNSIGNED) AS big, \
                 CAST('12345678901234567890.123' AS DECIMAL(30,3)) AS dec_value, \
                 DATE '2026-11-16' AS d, TIMESTAMP '2026-11-16 09:05:07' AS dt, \
                 TIME '-26:03:04' AS t, JSON_OBJECT('a', 1) AS j, \
                 CAST('abc' AS BINARY) AS bin, NULL AS n, 1.5e0 AS f, 'テキスト' AS s" }),
    )
    .await
    .unwrap();
    // MariaDBのJSONは、文字列の型（LONGTEXT）なので、文字列のまま返す
    let json_value = match kind {
        Kind::MySql => json!({ "a": 1 }),
        Kind::MariaDb => json!("{\"a\": 1}"),
    };
    assert_eq!(
        result["rows"],
        json!([[
            "18446744073709551615",
            "12345678901234567890.123",
            "2026-11-16",
            "2026-11-16 09:05:07",
            "-26:03:04",
            json_value,
            "YWJj",
            null,
            1.5,
            "テキスト"
        ]])
    );
    // 返した値を、そのままパラメーターに渡せる
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT CAST(? AS DATETIME) = TIMESTAMP '2026-11-16 09:05:07' AS same",
                "params": ["2026-11-16 09:05:07"] }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([[1]]));
}

async fn rows_are_limited_and_empty_results_keep_columns(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("many");
    run(&pool, &format!("CREATE TABLE {table} (n int)")).await;
    run(
        &pool,
        &format!("INSERT INTO {table} VALUES (1),(2),(3),(4),(5),(6),(7),(8),(9),(10)"),
    )
    .await;
    let mut settings = settings_from_url(&url, kind);
    settings.max_rows = Some(3);
    // 上限で止めた接続は閉じるので、1つでも続けて使えることを確かめる
    settings.max_connections = Some(1);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    for _ in 0..3 {
        let result = call(
            &handler,
            "execute_query",
            json!({ "sql": format!("SELECT n FROM {table} ORDER BY n") }),
        )
        .await
        .unwrap();
        assert_eq!(result["rows"], json!([[1], [2], [3]]));
        assert_eq!(result["truncated"], json!(true));
    }

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT n FROM {table} WHERE n <= 3") }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(false));

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT 1 AS one, 'x' AS two FROM DUAL WHERE false" }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([]));
    assert_eq!(result["columns"][0]["name"], json!("one"));
    assert_eq!(result["columns"][1]["name"], json!("two"));

    run(&pool, &format!("DROP TABLE {table}")).await;
}

async fn large_results_are_limited_by_bytes(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("big");
    run(&pool, &format!("CREATE TABLE {table} (n int)")).await;
    let values: Vec<String> = (1..=50).map(|n| format!("({n})")).collect();
    run(
        &pool,
        &format!("INSERT INTO {table} VALUES {}", values.join(",")),
    )
    .await;
    let mut settings = settings_from_url(&url, kind);
    settings.max_result_bytes = Some(100_000);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": format!("SELECT n, REPEAT('x', 10000) AS filler FROM {table} ORDER BY n") }),
    )
    .await
    .unwrap();
    assert_eq!(result["truncated"], json!(true));
    let rows = result["rows"].as_array().unwrap().len();
    assert!(rows > 0 && rows < 50, "{rows} rows");

    run(&pool, &format!("DROP TABLE {table}")).await;
}

async fn slow_statements_time_out(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("slow");
    run(&pool, &format!("CREATE TABLE {table} (x int)")).await;
    let values: Vec<String> = (1..=1000).map(|n| format!("({n})")).collect();
    run(
        &pool,
        &format!("INSERT INTO {table} VALUES {}", values.join(",")),
    )
    .await;
    let mut settings = settings_from_url(&url, kind);
    settings.query_timeout_seconds = Some(1);
    settings.allow_write = Some(true);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    // SELECTは、サーバーのMAX_EXECUTION_TIME（MariaDBはmax_statement_time）
    // で止まる。`count(*)`だけの結合は、MySQLが行を作らずに数えるので、行
    // ごとに計算させる
    let err = call(
        &handler,
        "execute_query",
        json!({ "sql": format!(
            "SELECT count(*) FROM {table} a, {table} b, {table} c WHERE a.x + b.x + c.x = -1"
        ) }),
    )
    .await
    .unwrap_err();
    let expected = match kind {
        Kind::MySql => "execution time",
        Kind::MariaDb => "max_statement_time",
    };
    assert!(err.contains(expected), "{err}");

    // 書き込みは、別の接続からKILL QUERYで止める
    let started = std::time::Instant::now();
    let err = call(
        &handler,
        "execute_command",
        json!({ "sql": format!("UPDATE {table} SET x = x WHERE x = 1 AND SLEEP(10) = 0") }),
    )
    .await
    .unwrap_err();
    assert!(err.contains("did not finish within 1 seconds"), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(8));

    // 止めた後も使える
    let result = call(&handler, "execute_query", json!({ "sql": "SELECT 1" }))
        .await
        .unwrap();
    assert_eq!(result["rows"], json!([[1]]));

    run(&pool, &format!("DROP TABLE {table}")).await;
}

async fn connection_errors_do_not_show_the_password(kind: Kind) {
    let Some((url, _pool)) = test_database(kind).await else {
        return;
    };
    let mut settings = settings_from_url(&url, kind);
    settings.password = Some("wrong-secret-password".to_string());
    let err = match DatabaseHandler::from_settings(&settings).await {
        Ok(_) => panic!("connecting with a wrong password must fail"),
        Err(err) => err,
    };
    assert!(err.contains("cannot"), "{err}");
    assert!(!err.contains("wrong-secret-password"), "{err}");
}

/// 接続をプールに戻すとき、セッションの変数・ロックを元に戻す
async fn session_state_does_not_stay_on_pooled_connections(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let table = unique_name("sess");
    let lock = unique_name("lock");
    run(&pool, &format!("CREATE TABLE {table} (x int)")).await;
    run(&pool, &format!("INSERT INTO {table} VALUES (1)")).await;

    let mut settings = settings_from_url(&url, kind);
    settings.allow_write = Some(true);
    // 接続を1つにして、同じ接続を使い回させる
    settings.max_connections = Some(1);
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();

    let result = call(
        &handler,
        "execute_command",
        json!({ "sql": format!(
            "UPDATE {table} SET x = (@mcp_rs_var := 42) WHERE GET_LOCK('{lock}', 0) = 1"
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
            json!({ "sql": format!("SELECT @mcp_rs_var, IS_FREE_LOCK('{lock}')") }),
        )
        .await
        .unwrap()["rows"]
            .clone();
        if after == json!([[null, 1]]) {
            break;
        }
    }
    assert_eq!(after, json!([[null, 1]]));

    drop(handler);
    run(&pool, &format!("DROP TABLE {table}")).await;
}

/// TLS（#527）。CIのMySQL・MariaDBは、自己署名の証明書でTLSを使える
async fn tls_modes(kind: Kind) {
    let Some((url, _pool)) = test_database(kind).await else {
        return;
    };
    // MariaDBは、performance_schemaが既定で無効なので、information_schemaを見る
    let cipher_sql = match kind {
        Kind::MySql => {
            "SELECT VARIABLE_VALUE FROM performance_schema.session_status \
             WHERE VARIABLE_NAME = 'Ssl_cipher'"
        }
        Kind::MariaDb => {
            "SELECT VARIABLE_VALUE FROM information_schema.SESSION_STATUS \
             WHERE VARIABLE_NAME = 'SSL_CIPHER'"
        }
    };

    // requireは暗号化する（証明書は確かめない）
    let mut settings = settings_from_url(&url, kind);
    settings.ssl_mode = Some("require".to_string());
    let handler = DatabaseHandler::from_settings(&settings).await.unwrap();
    let result = call(&handler, "execute_query", json!({ "sql": cipher_sql }))
        .await
        .unwrap();
    let cipher = result["rows"][0][0]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!cipher.is_empty(), "{result}");

    // disableは暗号化しない
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url, kind))
        .await
        .unwrap();
    let result = call(&handler, "execute_query", json!({ "sql": cipher_sql }))
        .await
        .unwrap();
    assert_eq!(result["rows"][0][0], json!(""), "{result}");

    // verify-fullは、自己署名の証明書を受け入れない
    settings.ssl_mode = Some("verify-full".to_string());
    assert!(DatabaseHandler::from_settings(&settings).await.is_err());
}

/// `ssl_root_cert`を書いたのに、`ssl_mode`が証明書を確かめない設定なら、
/// 起動しない
async fn ssl_root_cert_needs_a_verifying_ssl_mode(kind: Kind) {
    let Some((url, _pool)) = test_database(kind).await else {
        return;
    };
    let mut settings = settings_from_url(&url, kind);
    settings.ssl_mode = Some("require".to_string());
    settings.ssl_root_cert = Some("ca.pem".to_string());
    let err = match DatabaseHandler::from_settings(&settings).await {
        Ok(_) => panic!("ssl_root_cert with ssl_mode = require must not start"),
        Err(err) => err,
    };
    assert!(
        err.contains("ssl_root_cert") && err.contains("verify-full"),
        "{err}"
    );
}

/// `engine = "mysql"`のままMariaDBにつないでも、サーバーの版を見て、
/// MariaDBの時間の上限の変数を使う
async fn mysql_engine_setting_works_with_mariadb(kind: Kind) {
    if kind != Kind::MariaDb {
        return;
    }
    let Some((url, _pool)) = test_database(kind).await else {
        return;
    };
    let handler = DatabaseHandler::from_settings(&settings_from_url(&url, Kind::MySql))
        .await
        .unwrap();
    let result = call(
        &handler,
        "execute_query",
        json!({ "sql": "SELECT @@SESSION.max_statement_time AS limit_seconds" }),
    )
    .await
    .unwrap();
    assert_eq!(result["rows"], json!([[30.0]]), "{result}");
}

/// サーバーの`sql_mode`に`NO_BACKSLASH_ESCAPES`があっても、mcp-rsの接続では
/// 外し、文字列の読み方をSQLの検査（sqlparser）とそろえる（#574）
async fn backslash_escapes_match_the_sql_check(kind: Kind) {
    let Some((url, pool)) = test_database(kind).await else {
        return;
    };
    let mut conn = pool.get_conn().await.unwrap();
    let before: String = conn
        .query_first("SELECT @@GLOBAL.sql_mode")
        .await
        .unwrap()
        .unwrap();
    // サーバーの既定に足す（このテストの終わりに戻す。ほかのテストは、
    // 文字列に\を使わないので、影響しない）
    conn.query_drop(
        "SET GLOBAL sql_mode = CONCAT_WS(',', NULLIF(@@GLOBAL.sql_mode, ''), 'NO_BACKSLASH_ESCAPES')",
    )
    .await
    .unwrap();
    let during: String = conn
        .query_first("SELECT @@GLOBAL.sql_mode")
        .await
        .unwrap()
        .unwrap();

    // 新しい接続は、サーバーの既定（NO_BACKSLASH_ESCAPES）で始まる
    let result = match DatabaseHandler::from_settings(&settings_from_url(&url, kind)).await {
        Ok(handler) => {
            call(
                &handler,
                "execute_query",
                json!({ "sql": "SELECT @@SESSION.sql_mode AS mode, 'a\\'b' AS s" }),
            )
            .await
        }
        Err(e) => Err(e),
    };
    // 確かめる前に戻す
    conn.query_drop(format!("SET GLOBAL sql_mode = '{before}'"))
        .await
        .unwrap();

    assert!(during.contains("NO_BACKSLASH_ESCAPES"), "{during}");
    let result = result.unwrap();
    let mode = result["rows"][0][0]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!mode.contains("NO_BACKSLASH_ESCAPES"), "{mode}");
    // ほかのsql_modeは残す
    for other in during.split(',').filter(|m| *m != "NO_BACKSLASH_ESCAPES") {
        assert!(
            mode.split(',').any(|m| m == other),
            "{other} was removed: {mode}"
        );
    }
    // \' は、引用符のエスケープとして読まれる（検査と同じ）
    assert_eq!(result["rows"][0][1], json!("a'b"), "{result}");
}

/// 同じテストを、MySQLとMariaDBの両方で動かす
macro_rules! for_each_server {
    ($($name:ident),* $(,)?) => {
        mod mysql {
            $(
                #[tokio::test]
                async fn $name() {
                    super::$name(super::Kind::MySql).await
                }
            )*
        }
        mod mariadb {
            $(
                #[tokio::test]
                async fn $name() {
                    super::$name(super::Kind::MariaDb).await
                }
            )*
        }
    };
}

for_each_server!(
    read_only_by_default,
    get_schema_returns_foreign_keys_and_indexes,
    execute_query_cannot_write,
    execute_command_when_writes_are_allowed,
    values_are_decoded_by_type,
    rows_are_limited_and_empty_results_keep_columns,
    large_results_are_limited_by_bytes,
    slow_statements_time_out,
    connection_errors_do_not_show_the_password,
    session_state_does_not_stay_on_pooled_connections,
    tls_modes,
    ssl_root_cert_needs_a_verifying_ssl_mode,
    mysql_engine_setting_works_with_mariadb,
    backslash_escapes_match_the_sql_check,
);
