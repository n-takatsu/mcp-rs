//! Redis connection management and pooling

use super::command_restrict::CommandRestrictor;
use super::types::{RedisCommand, RedisConfig, RedisValue};
use crate::handlers::database::{
    engine::{ConnectionInfo, DatabaseConnection},
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use chrono::Utc;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, Client, RedisResult};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Redis connection pool manager
pub struct RedisConnection {
    config: RedisConfig,
    client: Client,
    connection: Arc<RwLock<MultiplexedConnection>>,
    command_restrictor: Arc<CommandRestrictor>,
}

impl RedisConnection {
    /// Create new Redis connection
    pub async fn connect(config: &RedisConfig) -> Result<Self, DatabaseError> {
        // Validate configuration
        if config.host.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "Redis host cannot be empty".to_string(),
            ));
        }

        if config.port == 0 {
            return Err(DatabaseError::ConfigurationError(
                "Redis port must be greater than 0".to_string(),
            ));
        }

        if config.database > 15 {
            return Err(DatabaseError::ConfigurationError(
                "Redis database number must be 0-15".to_string(),
            ));
        }

        // Build Redis connection URL
        let url = if let Some(ref password) = config.password {
            if config.use_tls {
                format!(
                    "rediss://:{}@{}:{}/{}",
                    password, config.host, config.port, config.database
                )
            } else {
                format!(
                    "redis://:{}@{}:{}/{}",
                    password, config.host, config.port, config.database
                )
            }
        } else if config.use_tls {
            format!(
                "rediss://{}:{}/{}",
                config.host, config.port, config.database
            )
        } else {
            format!(
                "redis://{}:{}/{}",
                config.host, config.port, config.database
            )
        };

        // Create Redis client
        let client = Client::open(url).map_err(|e| {
            DatabaseError::ConnectionFailed(format!("Failed to create Redis client: {}", e))
        })?;

        // Get multiplexed connection
        let connection = client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| {
                DatabaseError::ConnectionFailed(format!("Failed to connect to Redis: {}", e))
            })?;

        // Create command restrictor from config
        let command_restrictor = Arc::new(CommandRestrictor::from_config(&config.security));

        Ok(RedisConnection {
            config: config.clone(),
            client,
            connection: Arc::new(RwLock::new(connection)),
            command_restrictor,
        })
    }

    /// Get connection configuration
    pub fn config(&self) -> &RedisConfig {
        &self.config
    }

    /// Health check
    pub async fn health_check(&self) -> Result<(), DatabaseError> {
        let mut conn = self.connection.write().await;
        let _: String = redis::cmd("PING")
            .query_async(&mut *conn)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Redis PING failed: {}", e)))?;
        Ok(())
    }

    /// Get connection info
    pub fn get_info(&self) -> RedisConnectionInfo {
        RedisConnectionInfo {
            host: self.config.host.clone(),
            port: self.config.port,
            database: self.config.database,
            use_tls: self.config.use_tls,
        }
    }

    /// Execute a Redis command
    pub async fn execute_command(
        &self,
        command: &RedisCommand,
    ) -> Result<RedisValue, DatabaseError> {
        // Check command restriction
        self.command_restrictor.check_command(command)?;

        let mut conn = self.connection.write().await;

        match command {
            // String operations
            RedisCommand::Get(key) => {
                let value: Option<String> = conn.get(key).await.map_err(to_db_error)?;
                Ok(value.map(RedisValue::String).unwrap_or(RedisValue::Null))
            }
            RedisCommand::Set(key, value) => {
                let str_value = redis_value_to_string(value)?;
                let _: () = conn.set(key, str_value).await.map_err(to_db_error)?;
                Ok(RedisValue::String("OK".to_string()))
            }
            RedisCommand::Incr(key) => {
                let value: i64 = conn.incr(key, 1).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(value))
            }
            RedisCommand::Decr(key) => {
                let value: i64 = conn.decr(key, 1).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(value))
            }
            RedisCommand::Append(key, value) => {
                let len: i64 = conn.append(key, value).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(len))
            }

            // List operations
            RedisCommand::LPush(key, values) => {
                let str_values: Vec<String> = values
                    .iter()
                    .map(redis_value_to_string)
                    .collect::<Result<Vec<_>, _>>()?;
                let len: i64 = conn.lpush(key, str_values).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(len))
            }
            RedisCommand::RPush(key, values) => {
                let str_values: Vec<String> = values
                    .iter()
                    .map(redis_value_to_string)
                    .collect::<Result<Vec<_>, _>>()?;
                let len: i64 = conn.rpush(key, str_values).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(len))
            }
            RedisCommand::LPop(key, count) => {
                if let Some(_cnt) = count {
                    // 複数のLPOPは現在未サポート - 単一のLPOPのみ
                    let value: Option<String> = conn.lpop(key, None).await.map_err(to_db_error)?;
                    Ok(value.map(RedisValue::String).unwrap_or(RedisValue::Null))
                } else {
                    let value: Option<String> = conn.lpop(key, None).await.map_err(to_db_error)?;
                    Ok(value.map(RedisValue::String).unwrap_or(RedisValue::Null))
                }
            }
            RedisCommand::RPop(key, count) => {
                if let Some(_cnt) = count {
                    // 複数のRPOPは現在未サポート - 単一のRPOPのみ
                    let value: Option<String> = conn.rpop(key, None).await.map_err(to_db_error)?;
                    Ok(value.map(RedisValue::String).unwrap_or(RedisValue::Null))
                } else {
                    let value: Option<String> = conn.rpop(key, None).await.map_err(to_db_error)?;
                    Ok(value.map(RedisValue::String).unwrap_or(RedisValue::Null))
                }
            }
            RedisCommand::LLen(key) => {
                let len: i64 = conn.llen(key).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(len))
            }
            RedisCommand::LRange(key, start, stop) => {
                let values: Vec<String> = conn
                    .lrange(key, *start as isize, *stop as isize)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::List(
                    values.into_iter().map(RedisValue::String).collect(),
                ))
            }

            // Set operations
            RedisCommand::SAdd(key, members) => {
                let count: i64 = conn.sadd(key, members).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::SRem(key, members) => {
                let count: i64 = conn.srem(key, members).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::SMembers(key) => {
                let members: Vec<String> = conn.smembers(key).await.map_err(to_db_error)?;
                Ok(RedisValue::Set(members))
            }
            RedisCommand::SCard(key) => {
                let count: i64 = conn.scard(key).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }

            // Hash operations
            RedisCommand::HSet(key, field_values) => {
                for (field, value) in field_values {
                    let str_value = redis_value_to_string(value)?;
                    let _: () = conn
                        .hset(key, field, str_value)
                        .await
                        .map_err(to_db_error)?;
                }
                Ok(RedisValue::Integer(field_values.len() as i64))
            }
            RedisCommand::HGet(key, field) => {
                let value: Option<String> = conn.hget(key, field).await.map_err(to_db_error)?;
                Ok(value.map(RedisValue::String).unwrap_or(RedisValue::Null))
            }
            RedisCommand::HDel(key, fields) => {
                let count: i64 = conn.hdel(key, fields).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::HKeys(key) => {
                let keys: Vec<String> = conn.hkeys(key).await.map_err(to_db_error)?;
                Ok(RedisValue::List(
                    keys.into_iter().map(RedisValue::String).collect(),
                ))
            }
            RedisCommand::HVals(key) => {
                let values: Vec<String> = conn.hvals(key).await.map_err(to_db_error)?;
                Ok(RedisValue::List(
                    values.into_iter().map(RedisValue::String).collect(),
                ))
            }
            RedisCommand::HGetAll(key) => {
                let hash: HashMap<String, String> = conn.hgetall(key).await.map_err(to_db_error)?;
                let mut result = HashMap::new();
                for (k, v) in hash {
                    result.insert(k, RedisValue::String(v));
                }
                Ok(RedisValue::Hash(result))
            }

            // Sorted Set operations
            RedisCommand::ZAdd(key, score_members) => {
                let items: Vec<(f64, String)> = score_members
                    .iter()
                    .map(|(score, member)| (*score, member.clone()))
                    .collect();
                let count: i64 = conn.zadd_multiple(key, &items).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::ZRem(key, members) => {
                let count: i64 = conn.zrem(key, members).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::ZRange(key, start, stop) => {
                let members: Vec<String> = conn
                    .zrange(key, *start as isize, *stop as isize)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::List(
                    members.into_iter().map(RedisValue::String).collect(),
                ))
            }
            RedisCommand::ZRangeByScore(key, min, max) => {
                let members: Vec<String> = conn
                    .zrangebyscore(key, *min, *max)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::List(
                    members.into_iter().map(RedisValue::String).collect(),
                ))
            }
            RedisCommand::ZRank(key, member) => {
                let rank: Option<i64> = conn.zrank(key, member).await.map_err(to_db_error)?;
                Ok(rank.map(RedisValue::Integer).unwrap_or(RedisValue::Null))
            }
            RedisCommand::ZScore(key, member) => {
                let score: Option<f64> = conn.zscore(key, member).await.map_err(to_db_error)?;
                Ok(score.map(RedisValue::Float).unwrap_or(RedisValue::Null))
            }
            RedisCommand::ZCard(key) => {
                let count: i64 = conn.zcard(key).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::ZCount(key, min, max) => {
                let count: i64 = conn.zcount(key, *min, *max).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::ZIncrBy(key, increment, member) => {
                let score: f64 = conn
                    .zincr(key, member, *increment)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::Float(score))
            }
            RedisCommand::ZRemRangeByRank(key, start, stop) => {
                let count: i64 = conn
                    .zrembyscore(key, *start, *stop)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::ZRemRangeByScore(key, min, max) => {
                let count: i64 = conn
                    .zrembyscore(key, *min, *max)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }

            // Key operations
            RedisCommand::Del(keys) => {
                let count: i64 = conn.del(keys).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::Exists(keys) => {
                let count: i64 = conn.exists(keys).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(count))
            }
            RedisCommand::Expire(key, seconds) => {
                let result: bool = conn
                    .expire(key, *seconds as i64)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::Integer(if result { 1 } else { 0 }))
            }
            RedisCommand::Ttl(key) => {
                let ttl: i64 = conn.ttl(key).await.map_err(to_db_error)?;
                Ok(RedisValue::Integer(ttl))
            }
            RedisCommand::Keys(pattern) => {
                let keys: Vec<String> = conn.keys(pattern).await.map_err(to_db_error)?;
                Ok(RedisValue::List(
                    keys.into_iter().map(RedisValue::String).collect(),
                ))
            }

            // Transactions (未実装)
            RedisCommand::Multi => Err(DatabaseError::UnsupportedOperation(
                "MULTI transactions not yet implemented".to_string(),
            )),
            RedisCommand::Exec => Err(DatabaseError::UnsupportedOperation(
                "EXEC transactions not yet implemented".to_string(),
            )),
            RedisCommand::Discard => Err(DatabaseError::UnsupportedOperation(
                "DISCARD transactions not yet implemented".to_string(),
            )),

            // Connection commands
            RedisCommand::Ping => {
                let _: String = redis::cmd("PING")
                    .query_async(&mut *conn)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::String("PONG".to_string()))
            }
            RedisCommand::Echo(msg) => Ok(RedisValue::String(msg.clone())),
            RedisCommand::Select(db) => {
                let _: () = redis::cmd("SELECT")
                    .arg(*db)
                    .query_async(&mut *conn)
                    .await
                    .map_err(to_db_error)?;
                Ok(RedisValue::String("OK".to_string()))
            }
            RedisCommand::Auth(_password) => {
                // 認証は接続時に行われるため、ここでは実行しない
                Err(DatabaseError::SecurityViolation(
                    "AUTH command should not be called directly".to_string(),
                ))
            }
        }
    }
}

/// Helper function to convert RedisValue to String
fn redis_value_to_string(value: &RedisValue) -> Result<String, DatabaseError> {
    match value {
        RedisValue::String(s) => Ok(s.clone()),
        RedisValue::Integer(i) => Ok(i.to_string()),
        RedisValue::Float(f) => Ok(f.to_string()),
        RedisValue::Binary(b) => String::from_utf8(b.clone())
            .map_err(|e| DatabaseError::OperationFailed(format!("Invalid UTF-8: {}", e))),
        RedisValue::Null => Ok(String::new()),
        _ => Err(DatabaseError::OperationFailed(
            "Complex types cannot be converted to string".to_string(),
        )),
    }
}

/// Helper function to convert redis errors to DatabaseError
fn to_db_error(err: redis::RedisError) -> DatabaseError {
    DatabaseError::QueryFailed(format!("Redis error: {}", err))
}

/// Connection information
#[derive(Clone, Debug)]
pub struct RedisConnectionInfo {
    pub host: String,
    pub port: u16,
    pub database: u8,
    pub use_tls: bool,
}

#[async_trait]
impl DatabaseConnection for RedisConnection {
    async fn query(&self, sql: &str, _params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // Interpret simple SQL-like queries for Redis
        // For example: "SELECT * FROM key" -> GET key
        let sql_lower = sql.trim().to_lowercase();

        if sql_lower.starts_with("select") {
            // Extract key from query
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() >= 4 && parts[2] == "from" {
                let key = parts[3];
                let command = RedisCommand::Get(key.to_string());
                let value = self.execute_command(&command).await?;

                // Convert to QueryResult
                let row = vec![redis_value_to_db_value(&value)];
                Ok(QueryResult {
                    columns: vec![crate::handlers::database::types::ColumnInfo {
                        name: "value".to_string(),
                        data_type: "string".to_string(),
                        nullable: true,
                        max_length: None,
                    }],
                    rows: vec![row],
                    total_rows: Some(1),
                    execution_time_ms: 0,
                })
            } else {
                Err(DatabaseError::QueryFailed(
                    "Invalid SELECT syntax for Redis".to_string(),
                ))
            }
        } else {
            Err(DatabaseError::UnsupportedOperation(
                "Only basic SELECT queries supported for Redis".to_string(),
            ))
        }
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> Result<ExecuteResult, DatabaseError> {
        // Interpret simple SQL-like commands for Redis
        // For example: "INSERT INTO key VALUES ('value')" -> SET key value
        let sql_lower = sql.trim().to_lowercase();

        if sql_lower.starts_with("insert") || sql_lower.starts_with("update") {
            // Extract key and value from SQL
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() >= 4 && parts[1].to_lowercase() == "into" {
                let key = parts[2];
                let value = if params.is_empty() {
                    parts
                        .get(4)
                        .map(|s| s.trim_matches(|c| c == '\'' || c == '"'))
                        .unwrap_or("")
                } else if let Value::String(s) = &params[0] {
                    s.as_str()
                } else {
                    ""
                };

                let command =
                    RedisCommand::Set(key.to_string(), RedisValue::String(value.to_string()));
                self.execute_command(&command).await?;

                Ok(ExecuteResult {
                    rows_affected: 1,
                    last_insert_id: None,
                    execution_time_ms: 0,
                })
            } else {
                Err(DatabaseError::QueryFailed(
                    "Invalid INSERT/UPDATE syntax for Redis".to_string(),
                ))
            }
        } else if sql_lower.starts_with("delete") {
            // DELETE FROM key -> DEL key
            let parts: Vec<&str> = sql.split_whitespace().collect();
            if parts.len() >= 3 && parts[1].to_lowercase() == "from" {
                let key = parts[2];
                let command = RedisCommand::Del(vec![key.to_string()]);
                let result = self.execute_command(&command).await?;

                let count = match result {
                    RedisValue::Integer(n) => n as u64,
                    _ => 0,
                };

                Ok(ExecuteResult {
                    rows_affected: count,
                    last_insert_id: None,
                    execution_time_ms: 0,
                })
            } else {
                Err(DatabaseError::QueryFailed(
                    "Invalid DELETE syntax for Redis".to_string(),
                ))
            }
        } else {
            Err(DatabaseError::UnsupportedOperation(
                "Only basic INSERT/UPDATE/DELETE supported for Redis".to_string(),
            ))
        }
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::handlers::database::engine::DatabaseTransaction>, DatabaseError>
    {
        // Redis MULTI/EXEC transactions
        Err(DatabaseError::UnsupportedOperation(
            "MULTI/EXEC transactions not yet implemented".to_string(),
        ))
    }

    async fn get_schema(
        &self,
    ) -> Result<crate::handlers::database::types::DatabaseSchema, DatabaseError> {
        // Redis doesn't have schema, return INFO command results
        let mut conn = self.connection.write().await;
        let _info: String = redis::cmd("INFO")
            .query_async(&mut *conn)
            .await
            .map_err(to_db_error)?;

        Ok(crate::handlers::database::types::DatabaseSchema {
            database_name: format!("redis-db-{}", self.config.database),
            tables: vec![],
            views: vec![],
            procedures: vec![],
        })
    }

    async fn get_table_schema(
        &self,
        table_name: &str,
    ) -> Result<crate::handlers::database::types::TableInfo, DatabaseError> {
        // Use KEYS command to get keys matching pattern
        let command = RedisCommand::Keys(table_name.to_string());
        let result = self.execute_command(&command).await?;

        let _keys = match result {
            RedisValue::List(keys) => keys
                .iter()
                .map(|v| match v {
                    RedisValue::String(s) => s.clone(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        };

        Ok(crate::handlers::database::types::TableInfo {
            name: table_name.to_string(),
            schema: None,
            columns: vec![
                crate::handlers::database::types::ColumnInfo {
                    name: "key".to_string(),
                    data_type: "String".to_string(),
                    nullable: false,
                    max_length: None,
                },
                crate::handlers::database::types::ColumnInfo {
                    name: "value".to_string(),
                    data_type: "String".to_string(),
                    nullable: true,
                    max_length: None,
                },
            ],
            primary_keys: vec!["key".to_string()],
            foreign_keys: vec![],
            indexes: vec![],
        })
    }

    async fn prepare(
        &self,
        _sql: &str,
    ) -> Result<Box<dyn crate::handlers::database::engine::PreparedStatement>, DatabaseError> {
        // Redis doesn't use prepared statements in traditional sense
        Err(DatabaseError::UnsupportedOperation(
            "Prepared statements not supported for Redis".to_string(),
        ))
    }

    async fn ping(&self) -> Result<(), DatabaseError> {
        self.health_check().await
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo {
            connection_id: format!("redis-{}-{}", self.config.host, self.config.port),
            database_name: format!("redis-db-{}", self.config.database),
            user_name: "redis".to_string(),
            server_version: "7.0.0".to_string(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        }
    }
}

/// Helper function to convert RedisValue to database Value
fn redis_value_to_db_value(redis_val: &RedisValue) -> Value {
    match redis_val {
        RedisValue::String(s) => Value::String(s.clone()),
        RedisValue::Integer(i) => Value::Int(*i),
        RedisValue::Float(f) => Value::Float(*f),
        RedisValue::Binary(b) => Value::Binary(b.clone()),
        RedisValue::Null => Value::Null,
        RedisValue::List(list) => {
            let values: Vec<Value> = list.iter().map(redis_value_to_db_value).collect();
            Value::String(format!("{:?}", values))
        }
        RedisValue::Set(set) => Value::String(format!("{:?}", set)),
        RedisValue::Hash(hash) => Value::String(format!("{:?}", hash)),
        RedisValue::SortedSet(zset) => Value::String(format!("{:?}", zset)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires actual Redis server
    async fn test_redis_connection_creation() {
        let config = RedisConfig {
            host: "localhost".to_string(),
            port: 6379,
            database: 0,
            password: None,
            timeout_seconds: 30,
            use_tls: false,
            pool_settings: super::super::types::RedisPoolSettings {
                max_connections: 50,
                min_idle: 10,
                connection_timeout_ms: 5000,
                idle_timeout_seconds: 300,
            },
            security: Default::default(),
        };

        let connection = RedisConnection::connect(&config).await;
        assert!(connection.is_ok());
    }

    #[tokio::test]
    #[ignore] // Requires actual Redis server
    async fn test_redis_connection_validation() {
        let mut config = RedisConfig {
            host: "".to_string(), // Invalid
            port: 6379,
            database: 0,
            password: None,
            timeout_seconds: 30,
            use_tls: false,
            pool_settings: super::super::types::RedisPoolSettings {
                max_connections: 50,
                min_idle: 10,
                connection_timeout_ms: 5000,
                idle_timeout_seconds: 300,
            },
            security: Default::default(),
        };

        let result = RedisConnection::connect(&config).await;
        assert!(result.is_err());

        // Test invalid database
        config.host = "localhost".to_string();
        config.database = 20; // Invalid
        let result = RedisConnection::connect(&config).await;
        assert!(result.is_err());
    }
}
