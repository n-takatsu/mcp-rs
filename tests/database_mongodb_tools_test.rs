//! `[database]`のMongoDB（`engine = "mongodb"`）を、MCPのツールとして使う結合テスト
//! （#530）。
//!
//! 実際のMongoDBが要る。`TEST_MONGODB_URL`（例: `mongodb://localhost:27018`）が
//! 無ければ、何もせずに通る。CIでは、Run Integration Testsのジョブのmongodbの
//! サービスを使う。テストごとに別のデータベースを使い、初めに消す。
//!
//! 決まった名前（`mcp_rs_tools_read`等）のデータベースを、確かめずに消す。
//! `TEST_MONGODB_URL`は、消してよいテスト用のサーバーを指すこと（共有の
//! サーバーを指さない。#810。`docs/development/integration-tests.md`）
#![cfg(feature = "mongodb-backend")]

use mcp_rs::config::DatabaseSettings;
use mcp_rs::handlers::database::handler_from_settings;
use mcp_rs::mcp::{McpError, McpHandler, ToolCallParams};
use mongodb::bson::doc;
use serde_json::{json, Value};
use std::sync::Arc;

/// `TEST_MONGODB_URL`の接続先（ホストとポート）。無ければ`None`
fn test_server() -> Option<(String, u16)> {
    let url = std::env::var("TEST_MONGODB_URL").unwrap_or_default();
    if url.is_empty() {
        println!("skipping: TEST_MONGODB_URL not set");
        return None;
    }
    let parsed = url::Url::parse(&url).expect("TEST_MONGODB_URL is a URL");
    Some((
        parsed
            .host_str()
            .expect("TEST_MONGODB_URL has a host")
            .to_string(),
        parsed.port().unwrap_or(27017),
    ))
}

/// データベース`database`を消して、テストのデータを入れる
async fn prepare(host: &str, port: u16, database: &str) {
    let client = mongodb::Client::with_uri_str(format!("mongodb://{host}:{port}"))
        .await
        .unwrap();
    let db = client.database(database);
    db.drop().await.unwrap();
    let users = db.collection::<mongodb::bson::Document>("users");
    let mut docs = Vec::new();
    for i in 0..8 {
        docs.push(doc! { "name": format!("u{i}"), "age": i, "tags": ["a"] });
    }
    users.insert_many(docs).await.unwrap();
    db.collection::<mongodb::bson::Document>("notes")
        .insert_one(doc! { "body": "x".repeat(2000) })
        .await
        .unwrap();
}

fn settings(
    host: &str,
    port: u16,
    database: &str,
    allow_write: bool,
    max_result_bytes: u64,
) -> DatabaseSettings {
    DatabaseSettings {
        engine: "mongodb".to_string(),
        host: Some(host.to_string()),
        port: Some(port),
        database: database.to_string(),
        allow_write: Some(allow_write),
        max_rows: Some(5),
        max_result_bytes: Some(max_result_bytes),
        ..Default::default()
    }
}

async fn handler(
    host: &str,
    port: u16,
    database: &str,
    allow_write: bool,
    max_result_bytes: u64,
) -> Arc<dyn McpHandler> {
    let audit = Arc::new(mcp_rs::security::AuditLogger::with_defaults());
    handler_from_settings(
        &settings(host, port, database, allow_write, max_result_bytes),
        audit,
    )
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

#[tokio::test]
async fn mongodb_read_tools() {
    let Some((host, port)) = test_server() else {
        return;
    };
    let database = "mcp_rs_tools_read";
    prepare(&host, port, database).await;
    // 返す量の上限（1000バイト）が小さいハンドラー
    let handler = handler(&host, port, database, false, 1000).await;

    let tools = handler.list_tools().await.unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        ["find", "aggregate", "count_documents", "get_schema"]
    );
    assert_eq!(
        handler.status().await.unwrap(),
        format!("mongodb {host}:{port}/{database} (read-only)")
    );

    // 件数は、max_rows（5）まで
    let found = call(
        &handler,
        "find",
        json!({"collection": "users", "sort": {"age": 1}, "projection": {"_id": 0, "name": 1}}),
    )
    .await
    .unwrap();
    assert_eq!(found["documents"].as_array().unwrap().len(), 5);
    assert_eq!(found["documents"][0], json!({"name": "u0"}));
    assert_eq!(found["truncated"], true);
    let filtered = call(
        &handler,
        "find",
        json!({"collection": "users", "filter": {"age": {"$gte": 6}}, "projection": {"_id": 0, "age": 1}, "sort": {"age": 1}}),
    )
    .await
    .unwrap();
    assert_eq!(filtered["documents"], json!([{"age": 6}, {"age": 7}]));
    assert_eq!(filtered["truncated"], false);
    // バイト数の上限（1000）を超える文書は、返さない
    let big = call(&handler, "find", json!({"collection": "notes"}))
        .await
        .unwrap();
    assert_eq!(big["documents"], json!([]));
    assert_eq!(big["truncated"], true);

    let grouped = call(
        &handler,
        "aggregate",
        json!({"collection": "users", "pipeline": [
            {"$match": {"age": {"$lt": 4}}},
            {"$group": {"_id": null, "total": {"$sum": "$age"}}},
            {"$project": {"_id": 0}}
        ]}),
    )
    .await
    .unwrap();
    assert_eq!(grouped["documents"], json!([{"total": 6}]));
    let count = call(
        &handler,
        "count_documents",
        json!({"collection": "users", "filter": {"tags": "a"}}),
    )
    .await
    .unwrap();
    assert_eq!(count["count"], 8);

    // 書き込みの段・JavaScript・system.は断る
    for (tool, args) in [
        (
            "aggregate",
            json!({"collection": "users", "pipeline": [{"$out": "copy"}]}),
        ),
        (
            "aggregate",
            json!({"collection": "users", "pipeline": [{"$merge": {"into": "copy"}}]}),
        ),
        (
            "find",
            json!({"collection": "users", "filter": {"$where": "true"}}),
        ),
        ("count_documents", json!({"collection": "system.users"})),
        ("find", json!({"collection": "users", "limit": 100})),
    ] {
        let err = call(&handler, tool, args.clone()).await.unwrap_err();
        assert!(matches!(err, McpError::InvalidParams(_)), "{args}: {err}");
    }
    // 書き込みのツールは無い
    assert!(call(
        &handler,
        "delete_documents",
        json!({"collection": "users", "filter": {"age": 1}})
    )
    .await
    .is_err());

    // get_schemaも、バイト数の上限（1000）に収める（フィールドを省く）
    let small = call(&handler, "get_schema", json!({})).await.unwrap();
    assert_eq!(small["truncated"], true);
    assert_eq!(small["timed_out"], false);
    assert!(serde_json::to_string_pretty(&small).unwrap().len() <= 1000);

    let wide = crate::handler(&host, port, database, false, 100_000).await;
    let schema = call(&wide, "get_schema", json!({})).await.unwrap();
    assert_eq!(schema["truncated"], false);
    let collections: Vec<&str> = schema["collections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(collections, ["notes", "users"]);
    let users = &schema["collections"][1];
    assert_eq!(users["estimated_documents"], 8);
    let fields: Vec<&str> = users["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["_id", "age", "name", "tags"]);
}

#[tokio::test]
async fn mongodb_write_tools() {
    let Some((host, port)) = test_server() else {
        return;
    };
    let database = "mcp_rs_tools_write";
    prepare(&host, port, database).await;
    let handler = handler(&host, port, database, true, 100_000).await;

    let inserted = call(
        &handler,
        "insert_documents",
        json!({"collection": "users", "documents": [{"name": "new", "age": 100}]}),
    )
    .await
    .unwrap();
    assert_eq!(inserted["inserted_count"], 1);
    let updated = call(
        &handler,
        "update_documents",
        json!({"collection": "users", "filter": {"age": {"$lt": 3}}, "update": {"$set": {"young": true}}, "many": true}),
    )
    .await
    .unwrap();
    assert_eq!(updated["modified_count"], 3);
    let deleted = call(
        &handler,
        "delete_documents",
        json!({"collection": "users", "filter": {"name": "new"}}),
    )
    .await
    .unwrap();
    assert_eq!(deleted["deleted_count"], 1);

    // 無いコレクションには挿入しない（コレクションを作らない）
    let err = call(
        &handler,
        "insert_documents",
        json!({"collection": "usres", "documents": [{"name": "typo"}]}),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, McpError::InvalidParams(_)), "{err}");
    assert!(err.to_string().contains("does not exist"), "{err}");
    let schema = call(&handler, "get_schema", json!({})).await.unwrap();
    let collections: Vec<&str> = schema["collections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(collections, ["notes", "users"]);
    assert_eq!(schema["truncated"], false);
    assert_eq!(schema["timed_out"], false);

    // 途中で失敗したら、何件が入ったかをエラーに書く
    let err = call(
        &handler,
        "insert_documents",
        json!({"collection": "users", "documents": [{"_id": 1}, {"_id": 1}, {"_id": 2}]}),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("documents[1] failed"), "{err}");
    assert!(err.contains("1 of 3 documents were inserted"), "{err}");

    // 空のフィルター・置き換えの更新・多すぎる挿入は断る
    for (tool, args) in [
        (
            "delete_documents",
            json!({"collection": "users", "filter": {}, "many": true}),
        ),
        (
            "update_documents",
            json!({"collection": "users", "filter": {}, "update": {"$set": {"a": 1}}}),
        ),
        (
            "update_documents",
            json!({"collection": "users", "filter": {"age": 1}, "update": {"name": "replaced"}}),
        ),
        (
            "insert_documents",
            json!({"collection": "users", "documents": [{}, {}, {}, {}, {}, {}]}),
        ),
    ] {
        let err = call(&handler, tool, args.clone()).await.unwrap_err();
        assert!(matches!(err, McpError::InvalidParams(_)), "{args}: {err}");
    }
    // 初めの8件と、途中まで入った1件（_id = 1）
    let count = call(&handler, "count_documents", json!({"collection": "users"}))
        .await
        .unwrap();
    assert_eq!(count["count"], 9);
}
