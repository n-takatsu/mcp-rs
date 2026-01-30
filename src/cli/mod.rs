//! MCP CLI module

pub mod commands;
pub mod config;

// Re-export CLI entry point for external use
pub mod main;

pub use config::CliConfig;
