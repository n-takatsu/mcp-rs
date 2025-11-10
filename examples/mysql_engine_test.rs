//! MySQL Engine Test Example
//!
//! mysql_asyncライブラリを使用したMySQLエンジンの動作確認
//! セキュリティ監査済み: RSA脆弱性フリー

use mcp_rs::handlers::database::engines::mysql::MySqlEngine;
use mcp_rs::handlers::database::types::{
    ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, HealthStatusType, PoolConfig,
    SecurityConfig,
};
use mcp_rs::handlers::database::DatabaseEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 環境変数からMySQL接続情報を取得
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "mysql://root:password@localhost:3306/test".to_string());

    println!("🔐 MySQL接続テスト開始");
    println!(
        "📍 URL: {} (パスワードは表示されません)",
        mask_password(&database_url)
    );

    // データベース設定を構築
    let config = create_mysql_config(&database_url);

    // MySQLエンジンを作成
    match MySqlEngine::new(config.clone()).await {
        Ok(engine) => {
            println!("✅ MySQLエンジン作成成功");

            // バージョン情報を取得
            match engine.get_version().await {
                Ok(version) => println!("📊 MySQL Version: {}", version),
                Err(e) => println!("⚠️  バージョン取得失敗: {}", e),
            }

            // ヘルスチェック実行
            match engine.health_check().await {
                Ok(status) => println!(
                    "💚 Health Check: {} - {}",
                    match status.status {
                        HealthStatusType::Healthy => "OK",
                        _ => "NG",
                    },
                    status.error_message.as_deref().unwrap_or("正常")
                ),
                Err(e) => println!("❤️  Health Check失敗: {}", e),
            }

            // 接続テスト
            match engine.connect(&config).await {
                Ok(conn) => {
                    println!("🔌 データベース接続成功");

                    // 簡単なクエリテスト
                    let test_query = "SELECT 1 as test_value, 'Hello MySQL' as message";
                    match conn.query(test_query, &[]).await {
                        Ok(result) => {
                            println!("🎯 クエリ実行成功:");
                            println!("   カラム数: {}", result.columns.len());
                            println!("   行数: {}", result.rows.len());
                            println!("   実行時間: {}ms", result.execution_time_ms);

                            if let Some(first_row) = result.rows.first() {
                                println!("   結果: {:?}", first_row);
                            }
                        }
                        Err(e) => println!("❌ クエリ実行失敗: {}", e),
                    }
                }
                Err(e) => println!("❌ 接続失敗: {}", e),
            }

            // サポート機能一覧
            let features = engine.supported_features();
            println!("🚀 サポート機能: {:?}", features);
        }
        Err(e) => {
            println!("❌ MySQLエンジン作成失敗: {}", e);
            println!("💡 ヒント: MySQL サーバーが起動していることを確認してください");
            println!("💡 接続情報: DATABASE_URL環境変数で設定可能");
        }
    }

    println!("🏁 MySQL接続テスト完了");
    Ok(())
}

fn create_mysql_config(database_url: &str) -> DatabaseConfig {
    // 簡単なURL解析 (実際の実装ではより堅牢な解析が必要)
    let url_parts: Vec<&str> = database_url.split("://").collect();
    let connection_part = url_parts
        .get(1)
        .unwrap_or(&"root:password@localhost:3306/test");

    let parts: Vec<&str> = connection_part.split("@").collect();
    let auth_part = parts.first().unwrap_or(&"root:password");
    let host_part = parts.get(1).unwrap_or(&"localhost:3306/test");

    let auth_parts: Vec<&str> = auth_part.split(":").collect();
    let username = auth_parts.first().unwrap_or(&"root").to_string();
    let password = auth_parts.get(1).unwrap_or(&"password").to_string();

    let host_parts: Vec<&str> = host_part.split(":").collect();
    let host = host_parts.first().unwrap_or(&"localhost").to_string();
    let port_and_db = host_parts.get(1).unwrap_or(&"3306/test");

    let port_db_parts: Vec<&str> = port_and_db.split("/").collect();
    let port = port_db_parts
        .first()
        .unwrap_or(&"3306")
        .parse()
        .unwrap_or(3306);
    let database = port_db_parts.get(1).unwrap_or(&"test").to_string();

    DatabaseConfig {
        database_type: DatabaseType::MySQL,
        connection: ConnectionConfig {
            host,
            port,
            database,
            username,
            password,
            ssl_mode: None,
            timeout_seconds: 30,
            retry_attempts: 3,
            options: Default::default(),
        },
        pool: PoolConfig {
            min_connections: 1,
            max_connections: 10,
            connection_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 3600,
        },
        security: SecurityConfig {
            enable_sql_injection_detection: true,
            enable_query_whitelist: false,
            enable_audit_logging: true,
            threat_intelligence_enabled: true,
            max_query_length: 10000,
            allowed_operations: vec![],
        },
        features: FeatureConfig {
            enable_transactions: true,
            enable_prepared_statements: true,
            enable_stored_procedures: true,
            query_timeout: 30,
            enable_query_caching: false,
        },
    }
}

fn mask_password(url: &str) -> String {
    // パスワード部分をマスク
    let parts: Vec<&str> = url.split("://").collect();
    if parts.len() != 2 {
        return url.to_string();
    }

    let protocol = parts[0];
    let connection_part = parts[1];

    if let Some(at_pos) = connection_part.find('@') {
        let auth_part = &connection_part[..at_pos];
        let host_part = &connection_part[at_pos..];

        if let Some(colon_pos) = auth_part.find(':') {
            let username = &auth_part[..colon_pos];
            format!("{}://{}:****{}", protocol, username, host_part)
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}
