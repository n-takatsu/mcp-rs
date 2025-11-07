//! Database Handler Test Cases
//!
//! データベースハンドラーの包括的なテストスイート

#[cfg(test)]
mod database_tests {
    use super::*;
    use crate::handlers::database::{
        engine::DatabaseEngineBuilder,
        handler::DatabaseHandler,
        types::{
            ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig,
            SecurityConfig,
        },
    };
    use crate::mcp::{
        server::McpHandler,
        types::{ClientCapabilities, InitializeParams, ToolCallParams},
    };
    use serde_json::json;
    use std::collections::HashMap;
    use tokio_test;

    /// テスト用の基本設定を作成
    fn create_test_database_config() -> DatabaseConfig {
        DatabaseConfig {
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
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_database_handler_creation() {
        // データベースハンドラーが正常に作成できることをテスト
        let handler = DatabaseHandler::new(None).await;
        assert!(
            handler.is_ok(),
            "DatabaseHandler should be created successfully"
        );
    }

    #[tokio::test]
    async fn test_database_config_validation() {
        let config = create_test_database_config();

        // PostgreSQLエンジンビルダーでの設定検証をテスト
        // 実際のデータベース接続は不要なので、設定の構造のみテスト
        assert_eq!(config.database_type, DatabaseType::PostgreSQL);
        assert_eq!(config.connection.host, "localhost");
        assert_eq!(config.connection.port, 5432);
        assert_eq!(config.connection.database, "test_db");
    }

    #[tokio::test]
    async fn test_mcp_handler_interface() {
        let handler = DatabaseHandler::new(None).await.unwrap();

        // 初期化テスト
        let init_params = InitializeParams {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ClientCapabilities {
                experimental: None,
                sampling: None,
            },
            client_info: crate::mcp::types::ClientInfo {
                name: "test-client".to_string(),
                version: "1.0.0".to_string(),
            },
        };

        let init_result = handler.initialize(init_params).await;
        assert!(init_result.is_ok(), "Handler initialization should succeed");

        // ツール一覧取得テスト
        let tools = handler.list_tools().await;
        assert!(tools.is_ok(), "list_tools should succeed");

        let tools = tools.unwrap();
        assert!(!tools.is_empty(), "Should have at least one tool");

        // 期待されるツール名をチェック
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains(&"execute_query"));
        assert!(tool_names.contains(&"execute_command"));
        assert!(tool_names.contains(&"get_schema"));
        assert!(tool_names.contains(&"begin_transaction"));
    }

    #[tokio::test]
    async fn test_database_types() {
        // データベースタイプの表示形式をテスト
        assert_eq!(format!("{}", DatabaseType::PostgreSQL), "postgresql");
        assert_eq!(format!("{}", DatabaseType::MySQL), "mysql");
        assert_eq!(format!("{}", DatabaseType::SQLite), "sqlite");
        assert_eq!(format!("{}", DatabaseType::MongoDB), "mongodb");
    }

    #[tokio::test]
    async fn test_value_conversions() {
        use crate::handlers::database::types::Value;

        // Value型の基本的な変換テスト
        let int_val = Value::from_i64(42);
        let string_val = Value::from_string("test".to_string());
        let bool_val = Value::from_bool(true);
        let float_val = Value::from_f64(std::f64::consts::PI);

        match int_val {
            Value::Int(42) => {}
            _ => panic!("Expected Int(42)"),
        }

        match string_val {
            Value::String(s) if s == "test" => {}
            _ => panic!("Expected String(test)"),
        }

        match bool_val {
            Value::Bool(true) => {}
            _ => panic!("Expected Bool(true)"),
        }

        match float_val {
            Value::Float(f) if (f - std::f64::consts::PI).abs() < f64::EPSILON => {}
            _ => panic!("Expected Float(PI)"),
        }
    }

    #[tokio::test]
    async fn test_security_config_defaults() {
        let security_config = SecurityConfig::default();

        assert!(security_config.enable_sql_injection_detection);
        assert!(!security_config.enable_query_whitelist);
        assert!(security_config.enable_audit_logging);
        assert!(security_config.threat_intelligence_enabled);
        assert_eq!(security_config.max_query_length, 10000);
    }

    #[tokio::test]
    async fn test_pool_config_defaults() {
        let pool_config = PoolConfig::default();

        assert_eq!(pool_config.max_connections, 20);
        assert_eq!(pool_config.min_connections, 5);
        assert_eq!(pool_config.connection_timeout, 30);
        assert_eq!(pool_config.idle_timeout, 300);
        assert_eq!(pool_config.max_lifetime, 3600);
    }

    #[tokio::test]
    async fn test_feature_config_defaults() {
        let feature_config = FeatureConfig::default();

        assert!(feature_config.enable_transactions);
        assert!(feature_config.enable_prepared_statements);
        assert!(feature_config.enable_stored_procedures);
        assert_eq!(feature_config.query_timeout, 30);
        assert!(!feature_config.enable_query_caching);
    }

    #[tokio::test]
    async fn test_query_context_creation() {
        use crate::handlers::database::types::{QueryContext, QueryType};

        let context = QueryContext::new(QueryType::Select);

        assert!(matches!(context.query_type, QueryType::Select));
        assert!(context.user_id.is_none());
        assert!(!context.session_id.is_empty());
        assert!(context.source_ip.is_none());
        assert!(context.client_info.is_none());
    }

    #[tokio::test]
    async fn test_tool_call_params_structure() {
        use crate::mcp::types::ToolCallParams;

        // ツール呼び出しパラメータの構造をテスト
        let mut arguments = HashMap::new();
        arguments.insert("sql".to_string(), json!("SELECT * FROM users"));
        arguments.insert("params".to_string(), json!([]));

        let params = ToolCallParams {
            name: "execute_query".to_string(),
            arguments: Some(arguments),
        };

        assert_eq!(params.name, "execute_query");
        assert!(params.arguments.as_ref().unwrap().get("sql").is_some());
        assert!(params.arguments.as_ref().unwrap().get("params").is_some());
    }

    #[tokio::test]
    async fn test_error_handling() {
        use crate::handlers::database::types::DatabaseError;

        // エラータイプのテスト
        let conn_error = DatabaseError::ConnectionFailed("Connection refused".to_string());
        let query_error = DatabaseError::QueryFailed("Syntax error".to_string());
        let security_error = DatabaseError::SecurityViolation("SQL injection detected".to_string());

        assert!(matches!(conn_error, DatabaseError::ConnectionFailed(_)));
        assert!(matches!(query_error, DatabaseError::QueryFailed(_)));
        assert!(matches!(
            security_error,
            DatabaseError::SecurityViolation(_)
        ));

        // エラーメッセージの表示テスト
        let error_string = format!("{}", conn_error);
        assert!(error_string.contains("Connection failed"));
    }

    #[tokio::test]
    async fn test_isolation_level_display() {
        use crate::handlers::database::engine::IsolationLevel;

        assert_eq!(
            format!("{}", IsolationLevel::ReadCommitted),
            "READ COMMITTED"
        );
        assert_eq!(
            format!("{}", IsolationLevel::ReadUncommitted),
            "READ UNCOMMITTED"
        );
        assert_eq!(
            format!("{}", IsolationLevel::RepeatableRead),
            "REPEATABLE READ"
        );
        assert_eq!(format!("{}", IsolationLevel::Serializable), "SERIALIZABLE");
    }

    // セキュリティ機能のテスト
    mod security_tests {
        use super::*;
        use crate::handlers::database::security::{QueryWhitelist, SqlInjectionDetector};
        use crate::handlers::database::types::{QueryContext, QueryType, SecurityError};

        #[tokio::test]
        async fn test_sql_injection_detection() {
            let detector = SqlInjectionDetector::new();
            let context = QueryContext::new(QueryType::Select);

            // 安全なクエリ
            let safe_query = "SELECT * FROM users WHERE id = $1";
            let result = detector.scan(safe_query, &context);
            assert!(result.is_ok(), "Safe query should pass");

            // 危険なクエリ（UNION攻撃）
            let dangerous_query = "SELECT * FROM users UNION SELECT password FROM admin";
            let result = detector.scan(dangerous_query, &context);
            assert!(result.is_err(), "Dangerous query should be detected");
            assert!(matches!(
                result.unwrap_err(),
                SecurityError::SqlInjectionDetected(_)
            ));
        }

        #[tokio::test]
        async fn test_query_whitelist() {
            let mut whitelist = QueryWhitelist::new();
            let context = QueryContext::new(QueryType::Select);

            // パターンを追加
            whitelist
                .add_pattern(r"^SELECT \* FROM users WHERE id = \$1$")
                .unwrap();

            // 許可されたクエリ
            let allowed_query = "SELECT * FROM users WHERE id = $1";
            let result = whitelist.validate(allowed_query, &context);
            assert!(result.is_ok(), "Whitelisted query should pass");

            // 許可されていないクエリ
            let blocked_query = "DELETE FROM users";
            let result = whitelist.validate(blocked_query, &context);
            assert!(result.is_err(), "Non-whitelisted query should be blocked");
        }

        #[tokio::test]
        async fn test_quote_balance_detection() {
            let detector = SqlInjectionDetector::new();
            let context = QueryContext::new(QueryType::Select);

            // バランスの取れた引用符
            let balanced_query = "SELECT * FROM users WHERE name = 'John'";
            let result = detector.scan(balanced_query, &context);
            assert!(result.is_ok(), "Balanced quotes should pass");

            // バランスの取れていない引用符
            let unbalanced_query = "SELECT * FROM users WHERE name = 'John";
            let result = detector.scan(unbalanced_query, &context);
            assert!(result.is_err(), "Unbalanced quotes should be detected");
        }
    }

    // PostgreSQL固有のテスト
    #[cfg(feature = "postgresql")]
    mod postgresql_tests {
        use super::*;
        use crate::handlers::database::engines::postgresql::PostgreSqlEngine;

        #[tokio::test]
        async fn test_postgresql_engine_creation() {
            let config = create_test_database_config();
            let result = PostgreSqlEngine::new(config).await;

            // 実際のDB接続は不要なので、エンジン作成のみテスト
            assert!(result.is_ok(), "PostgreSqlEngine should be created");
        }

        #[tokio::test]
        async fn test_postgresql_config_validation() {
            let mut config = create_test_database_config();

            // 正常な設定
            let result = PostgreSqlEngine::validate_postgresql_config(&config);
            assert!(result.is_ok(), "Valid config should pass");

            // 無効な設定（ホストが空）
            config.connection.host = String::new();
            let result = PostgreSqlEngine::validate_postgresql_config(&config);
            assert!(result.is_err(), "Invalid config should fail");
        }
    }

    // クエリビルダーのテスト
    mod query_builder_tests {
        use super::*;
        use crate::handlers::database::engine::query_builder::SelectBuilder;

        #[test]
        fn test_select_builder() {
            let query = SelectBuilder::new("users")
                .columns(&["id", "name", "email"])
                .where_clause("id = $1")
                .order_by("name ASC")
                .limit(10)
                .offset(0)
                .build();

            let expected = "SELECT id, name, email FROM users WHERE id = $1 ORDER BY name ASC LIMIT 10 OFFSET 0";
            assert_eq!(query, expected);
        }

        #[test]
        fn test_select_builder_minimal() {
            let query = SelectBuilder::new("products").build();

            assert_eq!(query, "SELECT * FROM products");
        }
    }
}

// 統合テスト用のヘルパー関数
#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use crate::handlers::database::types::{
        ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig, SecurityConfig,
    };

    /// テスト用のモックデータベース設定を作成
    pub fn create_mock_database_config(db_type: DatabaseType) -> DatabaseConfig {
        let db_type_clone = db_type.clone();
        DatabaseConfig {
            database_type: db_type,
            connection: ConnectionConfig {
                host: "localhost".to_string(),
                port: match db_type_clone {
                    DatabaseType::PostgreSQL => 5432,
                    DatabaseType::MySQL => 3306,   // mysql_async復活済み
                    DatabaseType::MariaDB => 3306, // mysql_async対応済み
                    DatabaseType::SQLite => 0,     // SQLiteはファイルベース
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
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        }
    }

    /// テスト用のMCPツール呼び出しパラメータを作成
    pub fn create_test_tool_params(
        tool_name: &str,
        sql: &str,
    ) -> crate::mcp::types::ToolCallParams {
        let mut arguments = std::collections::HashMap::new();
        arguments.insert("sql".to_string(), serde_json::json!(sql));
        arguments.insert("params".to_string(), serde_json::json!([]));

        crate::mcp::types::ToolCallParams {
            name: tool_name.to_string(),
            arguments: Some(arguments),
        }
    }
}
