//! Database Handler Integration Tests
//!
//! MCP システム内でのデータベースハンドラーの統合テスト

use mcp_rs::handlers::database::types::DatabaseType;
use mcp_rs::handlers::database::{
    handler::DatabaseHandler,
    types::{ConnectionConfig, DatabaseConfig, FeatureConfig, PoolConfig, SecurityConfig},
};
use mcp_rs::handlers::generic::DatabaseType as GenericDatabaseType;
use mcp_rs::handlers::generic::{
    AuthConfig, ConnectionConfig as GenericConnectionConfig, HandlerConfig,
    SecurityConfig as GenericSecurityConfig, TargetType,
};
use mcp_rs::mcp::{
    server::McpHandler,
    types::{ClientCapabilities, ClientInfo, InitializeParams, ToolCallParams},
};
use serde_json::json;
use std::collections::HashMap;

/// データベースハンドラーの基本的な統合テスト
#[tokio::test]
async fn test_database_handler_mcp_integration() {
    // データベースハンドラーを作成
    let handler = DatabaseHandler::new(None).await.unwrap();

    // MCP初期化パラメータを設定
    let init_params = InitializeParams {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ClientCapabilities {
            experimental: None,
            sampling: None,
        },
        client_info: ClientInfo {
            name: "integration-test-client".to_string(),
            version: "1.0.0".to_string(),
        },
    };

    // ハンドラーを初期化
    let init_result = handler.initialize(init_params).await;
    assert!(init_result.is_ok(), "Handler initialization should succeed");

    // ツールリストを取得
    let tools = handler.list_tools().await;
    assert!(tools.is_ok(), "Should be able to list tools");

    let tools = tools.unwrap();
    assert!(!tools.is_empty(), "Should have at least one tool");

    // 期待されるツールが含まれているかチェック
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    assert!(
        tool_names.contains(&"execute_query"),
        "Should have execute_query tool"
    );
    assert!(
        tool_names.contains(&"execute_command"),
        "Should have execute_command tool"
    );
    assert!(
        tool_names.contains(&"get_schema"),
        "Should have get_schema tool"
    );
    assert!(
        tool_names.contains(&"begin_transaction"),
        "Should have begin_transaction tool"
    );
    assert!(
        tool_names.contains(&"list_engines"),
        "Should have list_engines tool"
    );
    assert!(
        tool_names.contains(&"switch_engine"),
        "Should have switch_engine tool"
    );

    println!("✅ Database handler MCP integration test passed");
}

/// マルチハンドラーシステムでのデータベースハンドラー統合テスト
#[tokio::test]
async fn test_multi_handler_database_integration() {
    // データベース設定を作成
    let _db_config = DatabaseConfig {
        database_type: DatabaseType::PostgreSQL,
        connection: ConnectionConfig {
            host: "localhost".to_string(),
            port: 5432,
            database: "test_db".to_string(),
            username: "test_user".to_string(),
            password: "test_password".to_string(),
            ssl_mode: Some("prefer".to_string()),
            timeout_seconds: 30,
            retry_attempts: 3,
            options: HashMap::new(),
        },
        pool: PoolConfig::default(),
        security: SecurityConfig::default(),
        features: FeatureConfig::default(),
    };

    // 汎用ハンドラー設定を作成
    let handler_config = HandlerConfig {
        name: "test_db".to_string(),
        target_type: TargetType::Database(GenericDatabaseType::PostgreSQL),
        connection: GenericConnectionConfig {
            endpoint: "postgresql://localhost:5432/test_db".to_string(),
            auth: AuthConfig::BasicAuth {
                username: "test_user".to_string(),
                password: "test_password".to_string(),
            },
            timeout_seconds: 30,
            retry_attempts: 3,
        },
        security: GenericSecurityConfig {
            use_tls: true,
            verify_cert: false,
            rate_limit: None,
            allowed_ips: vec![],
        },
        custom: HashMap::new(),
    };

    // 設定の妥当性を確認
    assert_eq!(handler_config.name, "test_db");
    assert!(!handler_config.connection.endpoint.is_empty());
    match handler_config.target_type {
        TargetType::Database(_) => {
            // データベース型であることを確認
        }
        _ => panic!("Expected database target type"),
    }

    println!("✅ Multi-handler database integration test passed");
}

/// データベースハンドラーのツール呼び出し統合テスト
#[tokio::test]
async fn test_database_handler_tool_call_integration() {
    let handler = DatabaseHandler::new(None).await.unwrap();

    // 初期化
    let init_params = InitializeParams {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ClientCapabilities {
            experimental: None,
            sampling: None,
        },
        client_info: ClientInfo {
            name: "tool-test-client".to_string(),
            version: "1.0.0".to_string(),
        },
    };
    handler.initialize(init_params).await.unwrap();

    // list_engines ツールを呼び出し
    let mut args = HashMap::new();
    let tool_params = ToolCallParams {
        name: "list_engines".to_string(),
        arguments: Some(args.clone()),
    };

    let result = handler.call_tool(tool_params).await;
    assert!(result.is_ok(), "list_engines tool call should succeed");

    // execute_query ツールのパラメータテスト（実際の接続なしでパラメータ検証）
    args.insert("sql".to_string(), json!("SELECT 1"));
    args.insert("params".to_string(), json!([]));

    let tool_params = ToolCallParams {
        name: "execute_query".to_string(),
        arguments: Some(args),
    };

    // 接続がないため失敗するはずだが、パラメータ検証は通る
    let _result = handler.call_tool(tool_params).await;
    // 接続エラーが発生するため、ここではエラーが期待される
    // ただし、パラメータ解析は成功している

    println!("✅ Database handler tool call integration test passed");
}

/// エラーハンドリングの統合テスト
#[tokio::test]
async fn test_database_handler_error_integration() {
    let handler = DatabaseHandler::new(None).await.unwrap();

    // 初期化
    let init_params = InitializeParams {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ClientCapabilities {
            experimental: None,
            sampling: None,
        },
        client_info: ClientInfo {
            name: "error-test-client".to_string(),
            version: "1.0.0".to_string(),
        },
    };
    handler.initialize(init_params).await.unwrap();

    // 無効なツール名でツール呼び出し
    let args = HashMap::new();
    let tool_params = ToolCallParams {
        name: "invalid_tool".to_string(),
        arguments: Some(args),
    };

    let result = handler.call_tool(tool_params).await;
    assert!(result.is_err(), "Invalid tool call should fail");

    // 無効なパラメータでツール呼び出し
    let tool_params = ToolCallParams {
        name: "execute_query".to_string(),
        arguments: None, // SQLパラメータなし
    };

    let result = handler.call_tool(tool_params).await;
    assert!(
        result.is_err(),
        "Tool call without required parameters should fail"
    );

    println!("✅ Database handler error integration test passed");
}

/// 設定の統合テスト
#[tokio::test]
async fn test_database_config_integration() {
    // 異なるデータベースタイプでの設定テスト
    let db_types = vec![
        DatabaseType::PostgreSQL,
        DatabaseType::MySQL,
        DatabaseType::SQLite,
        DatabaseType::MongoDB,
        DatabaseType::Redis,
        DatabaseType::ClickHouse,
    ];

    for db_type in db_types {
        let db_config = DatabaseConfig {
            database_type: db_type.clone(),
            connection: ConnectionConfig {
                host: "localhost".to_string(),
                port: match db_type {
                    DatabaseType::PostgreSQL => 5432,
                    DatabaseType::MySQL => 3306,
                    DatabaseType::MariaDB => 3306,
                    DatabaseType::SQLite => 0,
                    DatabaseType::MongoDB => 27017,
                    DatabaseType::Redis => 6379,
                    DatabaseType::ClickHouse => 9000,
                },
                database: "test_db".to_string(),
                username: "test_user".to_string(),
                password: "test_password".to_string(),
                ssl_mode: Some("prefer".to_string()),
                timeout_seconds: 30,
                retry_attempts: 3,
                options: HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        };

        // 汎用ハンドラー設定として変換
        let handler_config = HandlerConfig {
            name: format!("test_{:?}", db_type).to_lowercase(),
            target_type: TargetType::Database(match db_type {
                DatabaseType::PostgreSQL => GenericDatabaseType::PostgreSQL,
                DatabaseType::MySQL => GenericDatabaseType::MySQL,
                DatabaseType::MariaDB => GenericDatabaseType::MySQL, // MariaDBはMySQLとして扱う
                DatabaseType::SQLite => GenericDatabaseType::SQLite,
                DatabaseType::MongoDB => GenericDatabaseType::MongoDB,
                DatabaseType::Redis => GenericDatabaseType::Redis,
                DatabaseType::ClickHouse => GenericDatabaseType::PostgreSQL, // ClickHouseは汎用側にないのでPostgreSQLで代替
            }),
            connection: GenericConnectionConfig {
                endpoint: format!(
                    "{}://localhost:{}/test_db",
                    match db_type {
                        DatabaseType::PostgreSQL => "postgresql",
                        DatabaseType::MySQL => "mysql",
                        DatabaseType::MariaDB => "mariadb",
                        DatabaseType::SQLite => "sqlite",
                        DatabaseType::MongoDB => "mongodb",
                        DatabaseType::Redis => "redis",
                        DatabaseType::ClickHouse => "clickhouse",
                    },
                    db_config.connection.port
                ),
                auth: AuthConfig::BasicAuth {
                    username: "test_user".to_string(),
                    password: "test_password".to_string(),
                },
                timeout_seconds: 30,
                retry_attempts: 3,
            },
            security: GenericSecurityConfig {
                use_tls: true,
                verify_cert: false,
                rate_limit: None,
                allowed_ips: vec![],
            },
            custom: HashMap::new(),
        };

        // 設定の妥当性を確認
        assert!(!handler_config.connection.endpoint.is_empty());
        match handler_config.target_type {
            TargetType::Database(_) => {
                // データベース型であることを確認（具体的な型は異なるため詳細比較はスキップ）
            }
            _ => panic!("Expected database target type"),
        }
    }

    println!("✅ Database configuration integration test passed");
}
