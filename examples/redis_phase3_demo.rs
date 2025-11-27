//! Redis Phase 3 実装デモ
//! 実際のRedis接続とコマンド実行を示します

use mcp_rs::handlers::database::engines::redis::{
    RedisCommand, RedisConfig, RedisConnection, RedisValue,
};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 Redis Phase 3 実装デモ\n");

    // Redis設定
    let config = RedisConfig {
        host: "localhost".to_string(),
        port: 6379,
        database: 0,
        password: None,
        timeout_seconds: 30,
        use_tls: false,
        pool_settings: Default::default(),
        security: Default::default(),
    };

    println!("📡 Redisサーバーへの接続を試みています...");
    println!("   ホスト: {}:{}", config.host, config.port);
    println!("   データベース: {}\n", config.database);

    // 接続を試みる（Redisサーバーが起動していない場合は失敗）
    match RedisConnection::connect(&config).await {
        Ok(conn) => {
            println!("✅ 接続成功！\n");

            // PING テスト
            println!("🔍 テスト 1: PING コマンド");
            match conn.health_check().await {
                Ok(_) => println!("   ✅ PING成功\n"),
                Err(e) => println!("   ❌ PING失敗: {}\n", e),
            }

            // String操作テスト
            println!("🔍 テスト 2: String操作 (SET/GET)");
            let set_cmd = RedisCommand::Set(
                "test_key".to_string(),
                RedisValue::String("Hello, Redis Phase 3!".to_string()),
            );
            match conn.execute_command(&set_cmd).await {
                Ok(_) => println!("   ✅ SET成功"),
                Err(e) => println!("   ❌ SET失敗: {}", e),
            }

            let get_cmd = RedisCommand::Get("test_key".to_string());
            match conn.execute_command(&get_cmd).await {
                Ok(value) => println!("   ✅ GET成功: {:?}\n", value),
                Err(e) => println!("   ❌ GET失敗: {}\n", e),
            }

            // List操作テスト
            println!("🔍 テスト 3: List操作 (LPUSH/LRANGE)");
            let lpush_cmd = RedisCommand::LPush(
                "test_list".to_string(),
                vec![
                    RedisValue::String("item1".to_string()),
                    RedisValue::String("item2".to_string()),
                    RedisValue::String("item3".to_string()),
                ],
            );
            match conn.execute_command(&lpush_cmd).await {
                Ok(len) => println!("   ✅ LPUSH成功: {:?} items", len),
                Err(e) => println!("   ❌ LPUSH失敗: {}", e),
            }

            let lrange_cmd = RedisCommand::LRange("test_list".to_string(), 0, -1);
            match conn.execute_command(&lrange_cmd).await {
                Ok(items) => println!("   ✅ LRANGE成功: {:?}\n", items),
                Err(e) => println!("   ❌ LRANGE失敗: {}\n", e),
            }

            // Sorted Set操作テスト
            println!("🔍 テスト 4: Sorted Set操作 (ZADD/ZRANGE)");
            let zadd_cmd = RedisCommand::ZAdd(
                "leaderboard".to_string(),
                vec![
                    (100.0, "player1".to_string()),
                    (200.0, "player2".to_string()),
                    (150.0, "player3".to_string()),
                ],
            );
            match conn.execute_command(&zadd_cmd).await {
                Ok(count) => println!("   ✅ ZADD成功: {:?} members", count),
                Err(e) => println!("   ❌ ZADD失敗: {}", e),
            }

            let zrange_cmd = RedisCommand::ZRange("leaderboard".to_string(), 0, -1);
            match conn.execute_command(&zrange_cmd).await {
                Ok(members) => println!("   ✅ ZRANGE成功: {:?}\n", members),
                Err(e) => println!("   ❌ ZRANGE失敗: {}\n", e),
            }

            // クリーンアップ
            println!("🧹 クリーンアップ");
            let del_cmd = RedisCommand::Del(vec![
                "test_key".to_string(),
                "test_list".to_string(),
                "leaderboard".to_string(),
            ]);
            match conn.execute_command(&del_cmd).await {
                Ok(count) => println!("   ✅ {:?}個のキーを削除\n", count),
                Err(e) => println!("   ❌ 削除失敗: {}\n", e),
            }

            println!("✨ すべてのテストが完了しました！");
        }
        Err(e) => {
            println!("❌ 接続失敗: {}\n", e);
            println!("💡 ヒント:");
            println!("   1. Redisサーバーが起動しているか確認してください");
            println!(
                "   2. Windows: `redis-server` または Docker: `docker run -p 6379:6379 redis`"
            );
            println!("   3. 接続設定を確認してください（ホスト、ポート）\n");
            println!("ℹ️  Redisサーバーなしでも、コードは正常にコンパイルされます。");
        }
    }

    Ok(())
}
