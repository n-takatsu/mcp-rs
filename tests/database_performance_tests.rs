use futures::future::join_all;
use mcp_rs::handlers::database::{
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig, QueryType,
        SecurityConfig,
    },
    DatabaseHandler,
};
use mcp_rs::mcp::{types::ToolCallParams, McpHandler};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// 基本パフォーマンステスト - 単一クエリの実行時間測定
#[tokio::test]
async fn test_basic_query_performance() {
    let config = create_test_database_config();
    let handler = create_test_database_handler(config).await;

    let start_time = Instant::now();

    let result = handler
        .call_tool(ToolCallParams {
            name: "execute_query".to_string(),
            arguments: Some(HashMap::from([
                ("sql".to_string(), json!("SELECT 1 as test")),
                ("params".to_string(), json!([])),
            ])),
        })
        .await;

    let duration = start_time.elapsed();

    println!("🚀 Basic Query Performance Test:");
    println!("  Single query execution time: {:?}", duration);
    println!("  Query successful: {}", result.is_ok());

    // 基本的なパフォーマンス要件
    assert!(result.is_ok(), "クエリが成功するべき");
    assert!(
        duration < Duration::from_millis(50),
        "クエリ実行時間は50ms未満であるべき"
    );
}

/// 並行接続テスト - 複数のクエリを同時実行
#[tokio::test]
async fn test_concurrent_query_performance() {
    let config = create_test_database_config();
    let handler = create_test_database_handler(config).await;

    let concurrent_queries = 10;
    let start_time = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..concurrent_queries {
        let handler_clone = handler.clone();

        let task = tokio::spawn(async move {
            let query_start = Instant::now();
            let result = handler_clone
                .call_tool(ToolCallParams {
                    name: "execute_query".to_string(),
                    arguments: Some(HashMap::from([
                        (
                            "sql".to_string(),
                            json!(format!("SELECT {} as concurrent_test", i)),
                        ),
                        ("params".to_string(), json!([])),
                    ])),
                })
                .await;
            let query_duration = query_start.elapsed();

            (result.is_ok(), query_duration)
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;
    let total_duration = start_time.elapsed();

    // 結果の分析
    let successful_queries = results.iter().filter(|r| r.as_ref().unwrap().0).count();
    let average_query_time: Duration = results
        .iter()
        .map(|r| r.as_ref().unwrap().1)
        .sum::<Duration>()
        / results.len() as u32;

    println!("⚡ Concurrent Query Performance Test:");
    println!("  Total concurrent queries: {}", concurrent_queries);
    println!(
        "  Successful queries: {}/{}",
        successful_queries, concurrent_queries
    );
    println!("  Total test duration: {:?}", total_duration);
    println!("  Average query time: {:?}", average_query_time);
    println!(
        "  Throughput: {:.2} queries/second",
        concurrent_queries as f64 / total_duration.as_secs_f64()
    );

    // パフォーマンス要件の検証
    assert!(
        successful_queries >= concurrent_queries * 90 / 100,
        "90%以上の成功率が必要"
    );
    assert!(
        average_query_time < Duration::from_millis(100),
        "平均クエリ時間は100ms未満であるべき"
    );
}

/// ツールリスト取得のパフォーマンステスト
#[tokio::test]
async fn test_list_tools_performance() {
    let config = create_test_database_config();
    let handler = create_test_database_handler(config).await;

    let iterations = 20;
    let start_time = Instant::now();
    let mut successful_calls = 0;

    for _ in 0..iterations {
        let call_start = Instant::now();
        let result = handler.list_tools().await;
        let call_duration = call_start.elapsed();

        if result.is_ok() {
            successful_calls += 1;
        }

        // ツールリストの基本的な内容検証
        if let Ok(tools) = &result {
            assert!(!tools.is_empty(), "少なくとも1つのツールが存在するべき");
            println!(
                "  Call {}: {} tools, duration: {:?}",
                successful_calls,
                tools.len(),
                call_duration
            );
        }
    }

    let total_duration = start_time.elapsed();
    let average_call_time = total_duration / iterations as u32;

    println!("📋 List Tools Performance Test:");
    println!("  Total iterations: {}", iterations);
    println!("  Successful calls: {}/{}", successful_calls, iterations);
    println!("  Average call time: {:?}", average_call_time);
    println!(
        "  Calls per second: {:.2}",
        iterations as f64 / total_duration.as_secs_f64()
    );

    // パフォーマンス要件
    assert!(
        successful_calls >= iterations * 95 / 100,
        "95%以上の成功率が必要"
    );
    assert!(
        average_call_time < Duration::from_millis(20),
        "平均呼び出し時間は20ms未満であるべき"
    );
}

/// 初期化パフォーマンステスト
#[tokio::test]
async fn test_initialization_performance() {
    let config = create_test_database_config();

    let initialization_start = Instant::now();
    let handler = create_test_database_handler(config).await;
    let initialization_duration = initialization_start.elapsed();

    // 初期化後の基本機能テスト
    let post_init_start = Instant::now();
    let tools = handler.list_tools().await;
    let post_init_duration = post_init_start.elapsed();

    println!("🏁 Initialization Performance Test:");
    println!("  Handler creation time: {:?}", initialization_duration);
    println!("  Post-init tool list time: {:?}", post_init_duration);
    println!(
        "  Tools available: {}",
        tools.as_ref().map_or(0, |t| t.len())
    );

    // 初期化性能の要件
    assert!(
        initialization_duration < Duration::from_secs(2),
        "初期化時間は2秒未満であるべき"
    );
    assert!(
        post_init_duration < Duration::from_millis(50),
        "初期化後の操作は50ms未満であるべき"
    );
    assert!(tools.is_ok(), "初期化後にツールリストが取得できるべき");
}

/// 長時間実行テスト（軽量版）
#[tokio::test]
async fn test_sustained_operation_performance() {
    let config = create_test_database_config();
    let handler = create_test_database_handler(config).await;

    let test_duration = Duration::from_secs(10); // 10秒間のテスト
    let query_interval = Duration::from_millis(100); // 100msごとにクエリ

    let start_time = Instant::now();
    let mut successful_queries = 0;
    let mut total_query_time = Duration::new(0, 0);
    let mut query_count = 0;

    while start_time.elapsed() < test_duration {
        let query_start = Instant::now();
        let result = handler
            .call_tool(ToolCallParams {
                name: "execute_query".to_string(),
                arguments: Some(HashMap::from([
                    (
                        "sql".to_string(),
                        json!(format!("SELECT {} as sustained_test", query_count)),
                    ),
                    ("params".to_string(), json!([])),
                ])),
            })
            .await;
        let query_time = query_start.elapsed();

        query_count += 1;
        total_query_time += query_time;

        if result.is_ok() {
            successful_queries += 1;
        }

        sleep(query_interval).await;
    }

    let total_duration = start_time.elapsed();
    let average_query_time = if query_count > 0 {
        total_query_time / query_count as u32
    } else {
        Duration::new(0, 0)
    };

    println!("⏰ Sustained Operation Performance Test:");
    println!("  Test duration: {:?}", total_duration);
    println!("  Total queries: {}", query_count);
    println!("  Successful queries: {}", successful_queries);
    println!(
        "  Success rate: {:.1}%",
        (successful_queries as f64 / query_count as f64) * 100.0
    );
    println!("  Average query time: {:?}", average_query_time);
    println!(
        "  Throughput: {:.2} queries/second",
        query_count as f64 / total_duration.as_secs_f64()
    );

    // 持続性能の要件
    assert!(
        successful_queries >= query_count * 95 / 100,
        "95%以上の成功率が必要"
    );
    assert!(
        average_query_time < Duration::from_millis(150),
        "平均クエリ時間は150ms未満であるべき"
    );
}

/// エラーハンドリングパフォーマンステスト
#[tokio::test]
async fn test_error_handling_performance() {
    let config = create_test_database_config();
    let handler = create_test_database_handler(config).await;

    let invalid_queries = vec![
        "INVALID SQL QUERY",
        "SELECT * FROM non_existent_table",
        "INSERT INTO",
        "",
    ];

    let iterations_per_query = 5;
    let start_time = Instant::now();
    let mut total_error_handling_time = Duration::new(0, 0);
    let mut error_count = 0;

    for query in &invalid_queries {
        for _ in 0..iterations_per_query {
            let error_start = Instant::now();
            let result = handler
                .call_tool(ToolCallParams {
                    name: "execute_query".to_string(),
                    arguments: Some(HashMap::from([
                        ("sql".to_string(), json!(query)),
                        ("params".to_string(), json!([])),
                    ])),
                })
                .await;
            let error_duration = error_start.elapsed();

            total_error_handling_time += error_duration;
            error_count += 1;

            // エラーが適切に処理されているかチェック
            assert!(result.is_err(), "無効なクエリはエラーになるべき");
        }
    }

    let total_duration = start_time.elapsed();
    let average_error_handling_time = total_error_handling_time / error_count as u32;

    println!("❌ Error Handling Performance Test:");
    println!("  Total error scenarios: {}", error_count);
    println!(
        "  Average error handling time: {:?}",
        average_error_handling_time
    );
    println!("  Total test duration: {:?}", total_duration);

    // エラーハンドリング性能の要件
    assert!(
        average_error_handling_time < Duration::from_millis(50),
        "エラーハンドリング時間は50ms未満であるべき"
    );
}

// ヘルパー関数群

fn create_test_database_config() -> DatabaseConfig {
    DatabaseConfig {
        database_type: DatabaseType::PostgreSQL,
        connection: ConnectionConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "performance_test_db".to_string(),
            username: "test_user".to_string(),
            password: "test_password".to_string(),
            ssl_mode: Some("prefer".to_string()),
            timeout_seconds: 30,
            retry_attempts: 3,
            options: HashMap::new(),
        },
        pool: PoolConfig {
            max_connections: 10,
            min_connections: 2,
            max_lifetime: 1800,
            idle_timeout: 600,
            connection_timeout: 30,
        },
        security: SecurityConfig {
            enable_sql_injection_detection: true,
            enable_query_whitelist: false,
            enable_audit_logging: false,
            threat_intelligence_enabled: false,
            max_query_length: 10000,
            allowed_operations: vec![QueryType::Select, QueryType::Insert],
        },
        features: FeatureConfig::default(),
    }
}

async fn create_test_database_handler(config: DatabaseConfig) -> Arc<DatabaseHandler> {
    let handler = DatabaseHandler::new(None)
        .await
        .expect("Failed to create database handler");

    // PostgreSQLエンジンを登録
    handler
        .add_database("test_postgres".to_string(), config)
        .await
        .expect("Failed to register PostgreSQL engine");

    Arc::new(handler)
}
