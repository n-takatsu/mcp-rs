use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mcp_rs::mcp::{JsonRpcRequest, ToolCallParams};
use std::collections::HashMap;

fn benchmark_json_rpc_parsing(c: &mut Criterion) {
    let json_request = r#"
    {
        "jsonrpc": "2.0",
        "method": "tools/call",
        "params": {
            "name": "get_posts",
            "arguments": {
                "limit": 10
            }
        },
        "id": 1
    }"#;

    c.bench_function("json_rpc_parsing", |b| {
        b.iter(|| {
            let _: JsonRpcRequest = serde_json::from_str(black_box(json_request)).unwrap();
        })
    });
}

fn benchmark_tool_call_params(c: &mut Criterion) {
    let mut args = HashMap::new();
    args.insert("title".to_string(), serde_json::json!("Test Post"));
    args.insert("content".to_string(), serde_json::json!("This is test content"));

    c.bench_function("tool_call_params_creation", |b| {
        b.iter(|| {
            let _params = ToolCallParams {
                name: black_box("create_post".to_string()),
                arguments: black_box(Some(args.clone())),
            };
        })
    });
}

fn benchmark_json_serialization(c: &mut Criterion) {
    let mut args = HashMap::new();
    args.insert("title".to_string(), serde_json::json!("Test Post"));
    
    let params = ToolCallParams {
        name: "create_post".to_string(),
        arguments: Some(args),
    };

    c.bench_function("json_serialization", |b| {
        b.iter(|| {
            let _json = serde_json::to_string(black_box(&params)).unwrap();
        })
    });
}

criterion_group!(
    benches,
    benchmark_json_rpc_parsing,
    benchmark_tool_call_params,
    benchmark_json_serialization
);
criterion_main!(benches);