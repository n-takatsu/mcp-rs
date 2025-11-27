//! Command restriction and authorization system for Redis
//! Provides whitelist/blacklist-based command control with audit logging

use super::types::{RedisCommand, RedisSecuritySettings};
use crate::handlers::database::types::DatabaseError;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};

/// Command execution audit entry
#[derive(Clone, Debug)]
pub struct CommandAuditEntry {
    pub timestamp: DateTime<Utc>,
    pub command: String,
    pub user_id: Option<String>,
    pub status: CommandExecutionStatus,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandExecutionStatus {
    Allowed,
    Blocked,
    Error,
}

/// Command restrictor for enforcing security policies
pub struct CommandRestrictor {
    whitelist: HashSet<String>,
    blacklist: HashSet<String>,
    audit_log: Vec<CommandAuditEntry>,
    allow_unlisted: bool,
}

impl CommandRestrictor {
    /// Create new command restrictor with default settings
    pub fn new() -> Self {
        let settings = RedisSecuritySettings::default();
        let whitelist = settings.command_whitelist.into_iter().collect();
        let blacklist = settings.command_blacklist.into_iter().collect();

        CommandRestrictor {
            whitelist,
            blacklist,
            audit_log: Vec::new(),
            allow_unlisted: false, // Whitelist mode
        }
    }

    /// Create from Redis security settings
    pub fn from_config(settings: &RedisSecuritySettings) -> Self {
        let whitelist = settings.command_whitelist.iter().cloned().collect();
        let blacklist = settings.command_blacklist.iter().cloned().collect();

        CommandRestrictor {
            whitelist,
            blacklist,
            audit_log: Vec::new(),
            allow_unlisted: false,
        }
    }

    /// Create with custom settings
    pub fn with_settings(settings: RedisSecuritySettings) -> Self {
        let whitelist = settings.command_whitelist.into_iter().collect();
        let blacklist = settings.command_blacklist.into_iter().collect();

        CommandRestrictor {
            whitelist,
            blacklist,
            audit_log: Vec::new(),
            allow_unlisted: false,
        }
    }

    /// Check if command is allowed (without logging)
    pub fn check_command(&self, cmd: &RedisCommand) -> Result<(), DatabaseError> {
        if !self.is_allowed(cmd) {
            let cmd_name = self.command_name(cmd);
            return Err(DatabaseError::SecurityViolation(format!(
                "Command '{}' is not allowed",
                cmd_name
            )));
        }
        Ok(())
    }

    /// Check if command is allowed
    pub fn is_allowed(&self, cmd: &RedisCommand) -> bool {
        let cmd_name = self.command_name(cmd);

        // Check blacklist first
        if self.blacklist.contains(cmd_name) {
            return false;
        }

        // If whitelist mode, check if command is in whitelist
        if !self.allow_unlisted {
            return self.whitelist.contains(cmd_name);
        }

        true
    }

    /// Check command and log execution
    pub fn check_and_log(
        &mut self,
        cmd: &RedisCommand,
        user_id: Option<String>,
    ) -> Result<(), DatabaseError> {
        let is_allowed = self.is_allowed(cmd);
        let cmd_name = self.command_name(cmd).to_string();

        let status = if is_allowed {
            CommandExecutionStatus::Allowed
        } else {
            CommandExecutionStatus::Blocked
        };

        let reason = if !is_allowed {
            Some(format!(
                "Command '{}' not in whitelist or in blacklist",
                cmd_name
            ))
        } else {
            None
        };

        self.audit_log.push(CommandAuditEntry {
            timestamp: Utc::now(),
            command: cmd_name,
            user_id,
            status: status.clone(),
            reason,
        });

        if !is_allowed {
            return Err(DatabaseError::SecurityViolation(
                "Command execution not allowed".to_string(),
            ));
        }

        Ok(())
    }

    /// Get command name from RedisCommand enum
    fn command_name(&self, cmd: &RedisCommand) -> &'static str {
        match cmd {
            RedisCommand::Get(_) => "GET",
            RedisCommand::Set(_, _) => "SET",
            RedisCommand::Incr(_) => "INCR",
            RedisCommand::Decr(_) => "DECR",
            RedisCommand::Append(_, _) => "APPEND",

            RedisCommand::LPush(_, _) => "LPUSH",
            RedisCommand::RPush(_, _) => "RPUSH",
            RedisCommand::LPop(_, _) => "LPOP",
            RedisCommand::RPop(_, _) => "RPOP",
            RedisCommand::LLen(_) => "LLEN",
            RedisCommand::LRange(_, _, _) => "LRANGE",

            RedisCommand::SAdd(_, _) => "SADD",
            RedisCommand::SRem(_, _) => "SREM",
            RedisCommand::SMembers(_) => "SMEMBERS",
            RedisCommand::SCard(_) => "SCARD",

            RedisCommand::HSet(_, _) => "HSET",
            RedisCommand::HGet(_, _) => "HGET",
            RedisCommand::HDel(_, _) => "HDEL",
            RedisCommand::HKeys(_) => "HKEYS",
            RedisCommand::HVals(_) => "HVALS",
            RedisCommand::HGetAll(_) => "HGETALL",

            RedisCommand::ZAdd(_, _) => "ZADD",
            RedisCommand::ZRem(_, _) => "ZREM",
            RedisCommand::ZRange(_, _, _) => "ZRANGE",
            RedisCommand::ZRangeByScore(_, _, _) => "ZRANGEBYSCORE",
            RedisCommand::ZRank(_, _) => "ZRANK",
            RedisCommand::ZScore(_, _) => "ZSCORE",
            RedisCommand::ZCard(_) => "ZCARD",
            RedisCommand::ZCount(_, _, _) => "ZCOUNT",
            RedisCommand::ZIncrBy(_, _, _) => "ZINCRBY",
            RedisCommand::ZRemRangeByRank(_, _, _) => "ZREMRANGEBYRANK",
            RedisCommand::ZRemRangeByScore(_, _, _) => "ZREMRANGEBYSCORE",

            RedisCommand::Del(_) => "DEL",
            RedisCommand::Exists(_) => "EXISTS",
            RedisCommand::Expire(_, _) => "EXPIRE",
            RedisCommand::Ttl(_) => "TTL",
            RedisCommand::Keys(_) => "KEYS",

            RedisCommand::Multi => "MULTI",
            RedisCommand::Exec => "EXEC",
            RedisCommand::Discard => "DISCARD",

            RedisCommand::Ping => "PING",
            RedisCommand::Echo(_) => "ECHO",
            RedisCommand::Select(_) => "SELECT",
            RedisCommand::Auth(_) => "AUTH",
        }
    }

    /// Get whitelist
    pub fn get_whitelist(&self) -> Vec<String> {
        self.whitelist.iter().cloned().collect()
    }

    /// Set whitelist
    pub fn set_whitelist(&mut self, whitelist: Vec<String>) -> Result<(), DatabaseError> {
        if whitelist.is_empty() {
            return Err(DatabaseError::ConfigurationError(
                "Whitelist cannot be empty".to_string(),
            ));
        }
        self.whitelist = whitelist.into_iter().collect();
        Ok(())
    }

    /// Get blacklist
    pub fn get_blacklist(&self) -> Vec<String> {
        self.blacklist.iter().cloned().collect()
    }

    /// Set blacklist
    pub fn set_blacklist(&mut self, blacklist: Vec<String>) -> Result<(), DatabaseError> {
        self.blacklist = blacklist.into_iter().collect();
        Ok(())
    }

    /// Add command to whitelist
    pub fn allow_command(&mut self, command: String) {
        self.whitelist.insert(command);
    }

    /// Remove command from whitelist
    pub fn disallow_command(&mut self, command: String) {
        self.whitelist.remove(&command);
    }

    /// Block command
    pub fn block_command(&mut self, command: String) {
        self.blacklist.insert(command);
    }

    /// Unblock command
    pub fn unblock_command(&mut self, command: String) {
        self.blacklist.remove(&command);
    }

    /// Get audit log
    pub fn get_audit_log(&self) -> &[CommandAuditEntry] {
        &self.audit_log
    }

    /// Clear audit log
    pub fn clear_audit_log(&mut self) {
        self.audit_log.clear();
    }

    /// Get audit statistics
    pub fn get_audit_stats(&self) -> CommandAuditStats {
        let mut stats = CommandAuditStats::default();

        for entry in &self.audit_log {
            stats.total_commands += 1;
            match entry.status {
                CommandExecutionStatus::Allowed => stats.allowed_commands += 1,
                CommandExecutionStatus::Blocked => stats.blocked_commands += 1,
                CommandExecutionStatus::Error => stats.error_commands += 1,
            }

            *stats
                .commands_by_name
                .entry(entry.command.clone())
                .or_insert(0) += 1;
        }

        stats
    }
}

impl Default for CommandRestrictor {
    fn default() -> Self {
        Self::new()
    }
}

/// Audit statistics
#[derive(Clone, Debug, Default)]
pub struct CommandAuditStats {
    pub total_commands: u32,
    pub allowed_commands: u32,
    pub blocked_commands: u32,
    pub error_commands: u32,
    pub commands_by_name: HashMap<String, u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_allowed() {
        let restrictor = CommandRestrictor::new();
        assert!(restrictor.is_allowed(&RedisCommand::Get("key".to_string())));
    }

    #[test]
    fn test_command_blocked() {
        let restrictor = CommandRestrictor::new();
        // FLUSHDB is in default blacklist
        assert!(!restrictor.is_allowed(&RedisCommand::Del(vec!["*".to_string()])));
    }

    #[test]
    fn test_whitelist_modification() {
        let mut restrictor = CommandRestrictor::new();
        restrictor.allow_command("CUSTOM".to_string());
        assert!(restrictor.get_whitelist().contains(&"CUSTOM".to_string()));
    }

    #[test]
    fn test_audit_log() {
        let mut restrictor = CommandRestrictor::new();
        let _ = restrictor.check_and_log(&RedisCommand::Get("key".to_string()), None);

        let log = restrictor.get_audit_log();
        assert!(!log.is_empty());
        assert_eq!(log[0].status, CommandExecutionStatus::Allowed);
    }

    #[test]
    fn test_audit_stats() {
        let mut restrictor = CommandRestrictor::new();
        let _ = restrictor.check_and_log(&RedisCommand::Get("key".to_string()), None);
        let _ = restrictor.check_and_log(&RedisCommand::Get("key2".to_string()), None);

        let stats = restrictor.get_audit_stats();
        assert_eq!(stats.total_commands, 2);
        assert_eq!(stats.allowed_commands, 2);
    }
}
