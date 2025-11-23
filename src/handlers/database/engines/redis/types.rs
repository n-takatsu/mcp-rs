//! Redis data types and configuration structures

use std::collections::{HashMap, BTreeMap};
use serde::{Deserialize, Serialize};

/// Redis configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub database: u8,
    pub password: Option<String>,
    pub timeout_seconds: u32,
    pub use_tls: bool,
    pub pool_settings: RedisPoolSettings,
    pub security: RedisSecuritySettings,
}

/// Connection pool configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedisPoolSettings {
    pub max_connections: u32,
    pub min_idle: u32,
    pub connection_timeout_ms: u64,
    pub idle_timeout_seconds: u64,
}

/// Security settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedisSecuritySettings {
    pub enable_audit_logging: bool,
    pub enable_anomaly_detection: bool,
    pub command_whitelist: Vec<String>,
    pub command_blacklist: Vec<String>,
}

impl Default for RedisSecuritySettings {
    fn default() -> Self {
        RedisSecuritySettings {
            enable_audit_logging: true,
            enable_anomaly_detection: true,
            command_whitelist: vec![
                "GET".to_string(),
                "SET".to_string(),
                "LPUSH".to_string(),
                "RPUSH".to_string(),
                "LPOP".to_string(),
                "RPOP".to_string(),
                "SADD".to_string(),
                "SREM".to_string(),
                "SMEMBERS".to_string(),
                "HSET".to_string(),
                "HGET".to_string(),
                "HDEL".to_string(),
                "ZADD".to_string(),
                "ZREM".to_string(),
                "ZRANGE".to_string(),
                "ZRANK".to_string(),
            ],
            command_blacklist: vec![
                "FLUSHDB".to_string(),
                "FLUSHALL".to_string(),
                "SHUTDOWN".to_string(),
                "CONFIG".to_string(),
            ],
        }
    }
}

/// Redis value types
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RedisValue {
    String(String),
    Integer(i64),
    Float(f64),
    Binary(Vec<u8>),
    List(Vec<RedisValue>),
    Set(Vec<String>),
    Hash(HashMap<String, RedisValue>),
    SortedSet(BTreeMap<String, f64>), // member -> score mapping
    Null,
}

/// Redis commands
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RedisCommand {
    // String operations
    Get(String),
    Set(String, RedisValue),
    Incr(String),
    Decr(String),
    Append(String, String),

    // List operations
    LPush(String, Vec<RedisValue>),
    RPush(String, Vec<RedisValue>),
    LPop(String, Option<u32>),
    RPop(String, Option<u32>),
    LLen(String),
    LRange(String, i32, i32),

    // Set operations
    SAdd(String, Vec<String>),
    SRem(String, Vec<String>),
    SMembers(String),
    SCard(String),

    // Hash operations
    HSet(String, Vec<(String, RedisValue)>),
    HGet(String, String),
    HDel(String, Vec<String>),
    HKeys(String),
    HVals(String),
    HGetAll(String),

    // Sorted Set operations
    ZAdd(String, Vec<(f64, String)>), // score, member pairs
    ZRem(String, Vec<String>),
    ZRange(String, i32, i32),
    ZRangeByScore(String, f64, f64),
    ZRank(String, String),
    ZScore(String, String),
    ZCard(String),
    ZCount(String, f64, f64),

    // Key operations
    Del(Vec<String>),
    Exists(Vec<String>),
    Expire(String, u64),
    TTL(String),
    Keys(String),

    // Transactions
    Multi,
    Exec,
    Discard,

    // Connection
    Ping,
    Echo(String),
    Select(u8),
    Auth(String),
}

/// Sorted Set member with score
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SortedSetMember {
    pub member: String,
    pub score: f64,
}

impl PartialOrd for SortedSetMember {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.score.partial_cmp(&other.score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_config_default_security() {
        let security = RedisSecuritySettings::default();
        assert!(security.enable_audit_logging);
        assert!(security.enable_anomaly_detection);
        assert!(!security.command_whitelist.is_empty());
        assert!(!security.command_blacklist.is_empty());
    }

    #[test]
    fn test_sorted_set_member_ordering() {
        let m1 = SortedSetMember {
            member: "a".to_string(),
            score: 1.0,
        };
        let m2 = SortedSetMember {
            member: "b".to_string(),
            score: 2.0,
        };
        assert!(m1 < m2);
    }
}
