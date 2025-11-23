//! Redis engine implementation for mcp-rs
//! Provides high-performance in-memory data operations with Sorted Set support
//! and command restriction features for enterprise security.

pub mod connection;
pub mod sorted_set;
pub mod command_restrict;
pub mod types;

pub use connection::RedisConnection;
pub use sorted_set::SortedSetOperations;
pub use command_restrict::CommandRestrictor;
pub use types::{RedisValue, RedisCommand, RedisConfig};

use crate::handlers::database::types::DatabaseError;
use async_trait::async_trait;

/// Redis engine for in-memory data operations
pub struct RedisEngine {
    connection: RedisConnection,
    command_restrictor: CommandRestrictor,
}

impl RedisEngine {
    /// Create new Redis engine instance
    pub async fn new(config: RedisConfig) -> Result<Self, DatabaseError> {
        let connection = RedisConnection::connect(config).await?;
        let command_restrictor = CommandRestrictor::new();

        Ok(RedisEngine {
            connection,
            command_restrictor,
        })
    }

    /// Check if command is allowed
    pub fn is_command_allowed(&self, cmd: &RedisCommand) -> bool {
        self.command_restrictor.is_allowed(cmd)
    }

    /// Get current command whitelist
    pub fn get_command_whitelist(&self) -> Vec<String> {
        self.command_restrictor.get_whitelist()
    }

    /// Update command whitelist
    pub fn set_command_whitelist(&mut self, whitelist: Vec<String>) -> Result<(), DatabaseError> {
        self.command_restrictor.set_whitelist(whitelist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redis_engine_creation() {
        // Test placeholder - actual implementation depends on redis-rs
    }
}
