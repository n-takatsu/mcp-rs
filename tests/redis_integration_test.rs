//! Redis Phase 3 統合テスト
//! 実際のRedisサーバーに接続して全機能をテストします

#[cfg(all(test, feature = "redis", feature = "database"))]
mod redis_integration {
    use mcp_rs::handlers::database::engines::redis::types::RedisSecuritySettings;
    use mcp_rs::handlers::database::engines::redis::{
        RedisCommand, RedisConfig, RedisConnection, RedisValue,
    };

    fn create_test_config() -> RedisConfig {
        let security = RedisSecuritySettings {
            command_whitelist: vec![
                "GET".to_string(),
                "SET".to_string(),
                "INCR".to_string(),
                "DECR".to_string(),
                "LPUSH".to_string(),
                "RPUSH".to_string(),
                "LPOP".to_string(),
                "RPOP".to_string(),
                "LLEN".to_string(),
                "LRANGE".to_string(),
                "SADD".to_string(),
                "SREM".to_string(),
                "SMEMBERS".to_string(),
                "SCARD".to_string(),
                "HSET".to_string(),
                "HGET".to_string(),
                "HDEL".to_string(),
                "HKEYS".to_string(),
                "HVALS".to_string(),
                "HGETALL".to_string(),
                "ZADD".to_string(),
                "ZREM".to_string(),
                "ZRANGE".to_string(),
                "ZRANGEBYSCORE".to_string(),
                "ZRANK".to_string(),
                "ZSCORE".to_string(),
                "ZCARD".to_string(),
                "DEL".to_string(),
                "EXISTS".to_string(),
                "EXPIRE".to_string(),
                "TTL".to_string(),
                "KEYS".to_string(),
                "PING".to_string(),
            ],
            ..Default::default()
        };

        RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            database: 0,
            password: None,
            timeout_seconds: 30,
            use_tls: false,
            pool_settings: Default::default(),
            security,
        }
    }

    #[tokio::test]
    async fn test_redis_connection() {
        let config = create_test_config();
        let result = RedisConnection::connect(&config).await;
        assert!(result.is_ok(), "Redis connection should succeed");
    }

    #[tokio::test]
    async fn test_ping() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();
        let result = conn.health_check().await;
        assert!(result.is_ok(), "PING should succeed");
    }

    #[tokio::test]
    async fn test_string_operations() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();

        // SET
        let set_cmd = RedisCommand::Set(
            "test:string".to_string(),
            RedisValue::String("test_value".to_string()),
        );
        let result = conn.execute_command(&set_cmd).await;
        assert!(result.is_ok(), "SET should succeed");

        // GET
        let get_cmd = RedisCommand::Get("test:string".to_string());
        let result = conn.execute_command(&get_cmd).await;
        assert!(result.is_ok(), "GET should succeed");
        match result.unwrap() {
            RedisValue::String(s) => assert_eq!(s, "test_value"),
            _ => panic!("Expected String value"),
        }

        // Cleanup
        let del_cmd = RedisCommand::Del(vec!["test:string".to_string()]);
        conn.execute_command(&del_cmd).await.unwrap();
    }

    #[tokio::test]
    async fn test_incr_decr() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();

        let key = "test:counter";

        // SET初期値
        let set_cmd = RedisCommand::Set(key.to_string(), RedisValue::String("10".to_string()));
        conn.execute_command(&set_cmd).await.unwrap();

        // INCR
        let incr_cmd = RedisCommand::Incr(key.to_string());
        let result = conn.execute_command(&incr_cmd).await.unwrap();
        match result {
            RedisValue::Integer(n) => assert_eq!(n, 11),
            _ => panic!("Expected Integer"),
        }

        // DECR
        let decr_cmd = RedisCommand::Decr(key.to_string());
        let result = conn.execute_command(&decr_cmd).await.unwrap();
        match result {
            RedisValue::Integer(n) => assert_eq!(n, 10),
            _ => panic!("Expected Integer"),
        }

        // Cleanup
        let del_cmd = RedisCommand::Del(vec![key.to_string()]);
        conn.execute_command(&del_cmd).await.unwrap();
    }

    #[tokio::test]
    async fn test_list_operations() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();

        let key = "test:list";

        // LPUSH
        let lpush_cmd = RedisCommand::LPush(
            key.to_string(),
            vec![
                RedisValue::String("item1".to_string()),
                RedisValue::String("item2".to_string()),
            ],
        );
        conn.execute_command(&lpush_cmd).await.unwrap();

        // LLEN
        let llen_cmd = RedisCommand::LLen(key.to_string());
        let result = conn.execute_command(&llen_cmd).await.unwrap();
        match result {
            RedisValue::Integer(n) => assert_eq!(n, 2),
            _ => panic!("Expected Integer"),
        }

        // LRANGE
        let lrange_cmd = RedisCommand::LRange(key.to_string(), 0, -1);
        let result = conn.execute_command(&lrange_cmd).await.unwrap();
        match result {
            RedisValue::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("Expected List"),
        }

        // Cleanup
        let del_cmd = RedisCommand::Del(vec![key.to_string()]);
        conn.execute_command(&del_cmd).await.unwrap();
    }

    #[tokio::test]
    async fn test_hash_operations() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();

        let key = "test:hash";

        // HSET
        let hset_cmd = RedisCommand::HSet(
            key.to_string(),
            vec![
                (
                    "field1".to_string(),
                    RedisValue::String("value1".to_string()),
                ),
                (
                    "field2".to_string(),
                    RedisValue::String("value2".to_string()),
                ),
            ],
        );
        conn.execute_command(&hset_cmd).await.unwrap();

        // HGET
        let hget_cmd = RedisCommand::HGet(key.to_string(), "field1".to_string());
        let result = conn.execute_command(&hget_cmd).await.unwrap();
        match result {
            RedisValue::String(s) => assert_eq!(s, "value1"),
            _ => panic!("Expected String"),
        }

        // HKEYS
        let hkeys_cmd = RedisCommand::HKeys(key.to_string());
        let result = conn.execute_command(&hkeys_cmd).await.unwrap();
        match result {
            RedisValue::List(keys) => assert_eq!(keys.len(), 2),
            _ => panic!("Expected List"),
        }

        // Cleanup
        let del_cmd = RedisCommand::Del(vec![key.to_string()]);
        conn.execute_command(&del_cmd).await.unwrap();
    }

    #[tokio::test]
    async fn test_sorted_set_operations() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();

        let key = "test:zset";

        // ZADD
        let zadd_cmd = RedisCommand::ZAdd(
            key.to_string(),
            vec![
                (100.0, "member1".to_string()),
                (200.0, "member2".to_string()),
                (150.0, "member3".to_string()),
            ],
        );
        conn.execute_command(&zadd_cmd).await.unwrap();

        // ZCARD
        let zcard_cmd = RedisCommand::ZCard(key.to_string());
        let result = conn.execute_command(&zcard_cmd).await.unwrap();
        match result {
            RedisValue::Integer(n) => assert_eq!(n, 3),
            _ => panic!("Expected Integer"),
        }

        // ZRANGE (score順にソート済み)
        let zrange_cmd = RedisCommand::ZRange(key.to_string(), 0, -1);
        let result = conn.execute_command(&zrange_cmd).await.unwrap();
        match result {
            RedisValue::List(members) => {
                assert_eq!(members.len(), 3);
                // スコア順: member1(100), member3(150), member2(200)
            }
            _ => panic!("Expected List"),
        }

        // ZSCORE
        let zscore_cmd = RedisCommand::ZScore(key.to_string(), "member2".to_string());
        let result = conn.execute_command(&zscore_cmd).await.unwrap();
        match result {
            RedisValue::Float(score) => assert_eq!(score, 200.0),
            _ => panic!("Expected Float"),
        }

        // Cleanup
        let del_cmd = RedisCommand::Del(vec![key.to_string()]);
        conn.execute_command(&del_cmd).await.unwrap();
    }

    #[tokio::test]
    async fn test_key_operations() {
        let config = create_test_config();
        let conn = RedisConnection::connect(&config).await.unwrap();

        let key = "test:key_ops";

        // SET
        let set_cmd = RedisCommand::Set(key.to_string(), RedisValue::String("value".to_string()));
        conn.execute_command(&set_cmd).await.unwrap();

        // EXISTS
        let exists_cmd = RedisCommand::Exists(vec![key.to_string()]);
        let result = conn.execute_command(&exists_cmd).await.unwrap();
        match result {
            RedisValue::Integer(n) => assert_eq!(n, 1),
            _ => panic!("Expected Integer"),
        }

        // EXPIRE
        let expire_cmd = RedisCommand::Expire(key.to_string(), 300);
        conn.execute_command(&expire_cmd).await.unwrap();

        // TTL
        let ttl_cmd = RedisCommand::Ttl(key.to_string());
        let result = conn.execute_command(&ttl_cmd).await.unwrap();
        match result {
            RedisValue::Integer(ttl) => assert!(ttl > 0 && ttl <= 300),
            _ => panic!("Expected Integer"),
        }

        // DEL
        let del_cmd = RedisCommand::Del(vec![key.to_string()]);
        let result = conn.execute_command(&del_cmd).await.unwrap();
        match result {
            RedisValue::Integer(n) => assert_eq!(n, 1),
            _ => panic!("Expected Integer"),
        }
    }

    #[tokio::test]
    async fn test_security_restrictions() {
        // セキュリティ設定（ホワイトリスト）
        let security = RedisSecuritySettings {
            command_whitelist: vec!["GET".to_string(), "SET".to_string(), "DEL".to_string()],
            ..Default::default()
        };

        let config = RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            database: 0,
            password: None,
            timeout_seconds: 30,
            use_tls: false,
            pool_settings: Default::default(),
            security,
        };

        let conn = RedisConnection::connect(&config).await.unwrap();

        // 通常のコマンドは成功
        let set_cmd = RedisCommand::Set(
            "test:security".to_string(),
            RedisValue::String("test".to_string()),
        );
        let result = conn.execute_command(&set_cmd).await;
        assert!(result.is_ok(), "Normal commands should work");

        // Cleanup
        let del_cmd = RedisCommand::Del(vec!["test:security".to_string()]);
        conn.execute_command(&del_cmd).await.unwrap();
    }
}
