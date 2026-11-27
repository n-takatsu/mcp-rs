//! `[database]`のRedis（`engine = "redis"`）を、MCPのツールとして使う結合テスト
//! （#529）。
//!
//! 実際のRedisが要る。`TEST_REDIS_URL`（例: `redis://localhost:6380/0`）が無ければ、
//! 何もせずに通る。CIでは、Run Integration Testsのジョブのredisのサービスを使う。
//! ほかのテストと鍵がぶつからないよう、テストごとに別のデータベースの番号
//! （9・10・11）を使い、初めに空にする（並べて動かしても、互いの鍵を数えない）
#![cfg(feature = "redis-backend")]

use mcp_rs::config::DatabaseSettings;
use mcp_rs::handlers::database::handler_from_settings;
use mcp_rs::mcp::{McpError, McpHandler, ToolCallParams};
use serde_json::{json, Value};
use std::sync::Arc;

/// 読み取りのテストに使うデータベースの番号
const READ_DATABASE: u16 = 9;

/// 書き込みのテストに使うデータベースの番号
const WRITE_DATABASE: u16 = 10;

/// `TEST_REDIS_URL`の接続先（ホストとポート）。無ければ`None`
fn test_server() -> Option<(String, u16)> {
    let url = std::env::var("TEST_REDIS_URL").unwrap_or_default();
    if url.is_empty() {
        println!("skipping: TEST_REDIS_URL not set");
        return None;
    }
    let parsed = url::Url::parse(&url).expect("TEST_REDIS_URL is a URL");
    Some((
        parsed
            .host_str()
            .expect("TEST_REDIS_URL has a host")
            .to_string(),
        parsed.port().unwrap_or(6379),
    ))
}

/// データベース`database`を空にして、テストのデータを入れる
async fn prepare(host: &str, port: u16, database: u16) {
    let client = redis::Client::open(format!("redis://{host}:{port}/{database}")).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    let big = "x".repeat(2000);
    let mut pipe = redis::pipe();
    pipe.cmd("FLUSHDB")
        .cmd("SET")
        .arg("greeting")
        .arg("hello")
        .cmd("SET")
        .arg("big")
        .arg(&big)
        .cmd("HSET")
        .arg("user:1")
        .arg("name")
        .arg("a")
        .arg("age")
        .arg(3)
        .cmd("SADD")
        .arg("tags")
        .arg("red")
        .arg("blue")
        .cmd("ZADD")
        .arg("scores")
        .arg(1)
        .arg("a")
        .arg(2)
        .arg("b")
        .cmd("XADD")
        .arg("events")
        .arg("*")
        .arg("n")
        .arg(1)
        .cmd("XADD")
        .arg("events")
        .arg("*")
        .arg("n")
        .arg(2);
    for i in 0..20 {
        pipe.cmd("RPUSH").arg("list").arg(i);
    }
    let _: () = pipe.query_async(&mut conn).await.unwrap();
}

fn settings(host: &str, port: u16, database: u16, allow_write: bool) -> DatabaseSettings {
    DatabaseSettings {
        engine: "redis".to_string(),
        host: Some(host.to_string()),
        port: Some(port),
        database: database.to_string(),
        allow_write: Some(allow_write),
        max_rows: Some(5),
        max_result_bytes: Some(1000),
        ..Default::default()
    }
}

async fn handler(host: &str, port: u16, database: u16, allow_write: bool) -> Arc<dyn McpHandler> {
    let audit = Arc::new(mcp_rs::security::AuditLogger::with_defaults());
    handler_from_settings(&settings(host, port, database, allow_write), audit)
        .await
        .unwrap()
}

async fn call(handler: &Arc<dyn McpHandler>, tool: &str, args: Value) -> Result<Value, McpError> {
    let result = handler
        .call_tool(ToolCallParams {
            name: tool.to_string(),
            arguments: Some(serde_json::from_value(args).unwrap()),
        })
        .await?;
    Ok(serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap())
}

async fn command(
    handler: &Arc<dyn McpHandler>,
    name: &str,
    args: Value,
) -> Result<Value, McpError> {
    call(
        handler,
        "redis_command",
        json!({"command": name, "args": args}),
    )
    .await
}

#[tokio::test]
async fn redis_read_tools() {
    let Some((host, port)) = test_server() else {
        return;
    };
    prepare(&host, port, READ_DATABASE).await;
    let handler = handler(&host, port, READ_DATABASE, false).await;

    let tools = handler.list_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["redis_command", "scan_keys", "get_schema"]);
    assert_eq!(
        handler.status().await.unwrap(),
        format!("redis {host}:{port}/{READ_DATABASE} (read-only)")
    );

    let got = command(&handler, "get", json!(["greeting"])).await.unwrap();
    assert_eq!(got["result"], "hello");
    let user = command(&handler, "HGETALL", json!(["user:1"]))
        .await
        .unwrap();
    assert_eq!(user["result"], json!({"name": "a", "age": "3"}));
    // HSCANは切らずに返す。カーソルを最後までたどると、全部の項目が返る
    let mut fields = Vec::new();
    let mut cursor = "0".to_string();
    for _ in 0..100 {
        let page = command(&handler, "HSCAN", json!(["user:1", cursor, "COUNT", 1]))
            .await
            .unwrap();
        assert_eq!(page["truncated"], false);
        let parts = page["result"].as_array().unwrap();
        cursor = parts[0].as_str().unwrap().to_string();
        fields.extend(parts[1].as_array().unwrap().iter().step_by(2).cloned());
        if cursor == "0" {
            break;
        }
    }
    fields.sort_by_key(|f| f.as_str().unwrap().to_string());
    assert_eq!(fields, [json!("age"), json!("name")]);
    // 文字列でない鍵は、MGETではnull（長さの確かめで止めない）
    let many = command(&handler, "MGET", json!(["greeting", "user:1", "missing"]))
        .await
        .unwrap();
    assert_eq!(many["result"], json!(["hello", null, null]));
    let tags = command(&handler, "SMEMBERS", json!(["tags"]))
        .await
        .unwrap();
    assert_eq!(tags["result"].as_array().unwrap().len(), 2);
    let scores = command(&handler, "ZRANGE", json!(["scores", 0, -1, "WITHSCORES"]))
        .await
        .unwrap();
    assert_eq!(scores["result"], json!(["a", "1", "b", "2"]));
    let events = command(&handler, "XRANGE", json!(["events", "-", "+"]))
        .await
        .unwrap();
    assert_eq!(events["result"].as_array().unwrap().len(), 2);
    assert_eq!(events["truncated"], false);
    let ttl = command(&handler, "TTL", json!(["greeting"])).await.unwrap();
    assert_eq!(ttl["result"], -1);

    // 範囲は、max_rowsまで
    let five = command(&handler, "LRANGE", json!(["list", 0, 4]))
        .await
        .unwrap();
    assert_eq!(five["result"].as_array().unwrap().len(), 5);
    for (name, args) in [
        ("LRANGE", json!(["list", 0, -1])),
        ("ZRANGE", json!(["scores", 0, 10, "BYSCORE"])),
        ("XRANGE", json!(["events", "-", "+", "COUNT", 100])),
        (
            "XRANGE",
            json!(["events", "-", "+", "COUNT", 5, "COUNT", 100]),
        ),
        // 大きな値は、max_result_bytesまで
        ("GET", json!(["big"])),
        // 一覧に無いコマンド
        ("KEYS", json!(["*"])),
        ("FLUSHDB", json!([])),
        ("CONFIG", json!(["GET", "*"])),
        ("SET", json!(["greeting", "bye"])),
    ] {
        let err = command(&handler, name, args).await.unwrap_err();
        assert!(matches!(err, McpError::InvalidParams(_)), "{name}: {err}");
    }
    let part = command(&handler, "GETRANGE", json!(["big", 0, 9]))
        .await
        .unwrap();
    assert_eq!(part["result"], "xxxxxxxxxx");
    // 書き込みのツールは無い
    assert!(call(
        &handler,
        "redis_write_command",
        json!({"command": "SET", "args": ["a", "b"]})
    )
    .await
    .is_err());
    let unchanged = command(&handler, "GET", json!(["greeting"])).await.unwrap();
    assert_eq!(unchanged["result"], "hello");

    let keys = call(&handler, "scan_keys", json!({"pattern": "user:*"}))
        .await
        .unwrap();
    assert_eq!(keys["keys"], json!(["user:1"]));
    assert_eq!(keys["done"], true);
    // カーソルを最後までたどると、全部の鍵が1回ずつ返る（1回のSCANは、
    // max_rowsより多く返すことがある）
    let mut all = Vec::new();
    let mut cursor = "0".to_string();
    for _ in 0..100 {
        let page = call(&handler, "scan_keys", json!({"cursor": cursor}))
            .await
            .unwrap();
        all.extend(page["keys"].as_array().unwrap().iter().cloned());
        cursor = page["cursor"].as_str().unwrap().to_string();
        if page["done"] == true {
            break;
        }
    }
    let mut names: Vec<String> = all
        .iter()
        .map(|k| k.as_str().unwrap().to_string())
        .collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 7, "{all:?}");

    let schema = call(&handler, "get_schema", json!({})).await.unwrap();
    assert_eq!(schema["database"], READ_DATABASE);
    assert_eq!(schema["keys"], 7);
    assert!(schema["version"].is_string(), "{schema}");
}

#[tokio::test]
async fn redis_write_tool() {
    let Some((host, port)) = test_server() else {
        return;
    };
    prepare(&host, port, WRITE_DATABASE).await;
    let handler = handler(&host, port, WRITE_DATABASE, true).await;
    let write = |name: &str, args: Value| {
        let handler = handler.clone();
        let name = name.to_string();
        async move {
            call(
                &handler,
                "redis_write_command",
                json!({"command": name, "args": args}),
            )
            .await
        }
    };
    let set = write("SET", json!(["note", "hi"])).await.unwrap();
    assert_eq!(set["result"], "OK");
    let got = command(&handler, "GET", json!(["note"])).await.unwrap();
    assert_eq!(got["result"], "hi");
    let deleted = write("DEL", json!(["note"])).await.unwrap();
    assert_eq!(deleted["result"], 1);
    // 書き込みのツールでも、一覧に無いコマンドは断る
    for name in ["FLUSHDB", "FLUSHALL", "RENAME", "EVAL", "SELECT", "GET"] {
        let err = write(name, json!(["a", "b"])).await.unwrap_err();
        assert!(matches!(err, McpError::InvalidParams(_)), "{name}: {err}");
    }
    let schema = call(&handler, "get_schema", json!({})).await.unwrap();
    assert_eq!(schema["keys"], 7);
}

/// `max_rows = 1`・`COUNT 1`でも、`HSCAN`・`SSCAN`・`ZSCAN`の結果は切らない。
/// カーソルを最後までたどると、全部の項目が返る（切ると、切った項目は次の
/// カーソルで返らない）。ほかのテストと鍵の数がぶつからないよう、番号11を使う
#[tokio::test]
async fn redis_scans_are_not_cut() {
    let Some((host, port)) = test_server() else {
        return;
    };
    const DATABASE: u16 = 11;
    let client = redis::Client::open(format!("redis://{host}:{port}/{DATABASE}")).unwrap();
    let mut conn = client.get_multiplexed_async_connection().await.unwrap();
    let _: () = redis::pipe()
        .cmd("FLUSHDB")
        .cmd("HSET")
        .arg("h")
        .arg("a")
        .arg(1)
        .arg("b")
        .arg(2)
        .arg("c")
        .arg(3)
        .cmd("SADD")
        .arg("s")
        .arg("a")
        .arg("b")
        .arg("c")
        .cmd("ZADD")
        .arg("z")
        .arg(1)
        .arg("a")
        .arg(2)
        .arg("b")
        .arg(3)
        .arg("c")
        // フィールドの名前がUTF-8でないハッシュ（#808）
        .cmd("HSET")
        .arg("hb")
        .arg(&[0xff_u8][..])
        .arg("v")
        .query_async(&mut conn)
        .await
        .unwrap();
    let audit = Arc::new(mcp_rs::security::AuditLogger::with_defaults());
    let handler = handler_from_settings(
        &DatabaseSettings {
            max_rows: Some(1),
            ..settings(&host, port, DATABASE, false)
        },
        audit,
    )
    .await
    .unwrap();
    for (name, key, per) in [("HSCAN", "h", 2), ("SSCAN", "s", 1), ("ZSCAN", "z", 2)] {
        let mut members = Vec::new();
        let mut largest = 0;
        let mut cursor = "0".to_string();
        for _ in 0..100 {
            let page = command(&handler, name, json!([key, cursor, "COUNT", 1]))
                .await
                .unwrap();
            assert_eq!(page["truncated"], false, "{name}");
            let parts = page["result"].as_array().unwrap();
            cursor = parts[0].as_str().unwrap().to_string();
            let items = parts[1].as_array().unwrap();
            largest = largest.max(items.len() / per);
            members.extend(items.iter().step_by(per).cloned());
            if cursor == "0" {
                break;
            }
        }
        assert_eq!(cursor, "0", "{name}: the cursor did not reach 0");
        members.sort_by_key(|m| m.as_str().unwrap().to_string());
        members.dedup();
        assert_eq!(members, [json!("a"), json!("b"), json!("c")], "{name}");
        // 小さい集合は一度に全部返るので、max_rows（1）を超えるページがある
        assert!(largest > 1, "{name}: {largest}");
    }
    // オブジェクトにできない`HGETALL`は、フィールドと値の組で数える。
    // `max_rows = 1`でも、値は落ちない（#808）
    let hash = command(&handler, "HGETALL", json!(["hb"])).await.unwrap();
    assert_eq!(hash["truncated"], false, "{hash}");
    assert_eq!(hash["result"].as_array().map(Vec::len), Some(2), "{hash}");
}
