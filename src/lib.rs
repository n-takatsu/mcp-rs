//! # MCP-RS: Model Context Protocol Implementation in Rust
//!
//! This library provides a complete implementation of the Model Context Protocol (MCP)
//! in Rust, enabling secure and efficient communication between AI systems and external
//! resources like WordPress sites, databases, and APIs.

// Allow unused imports during development
#![allow(unused_imports)]
// async_traitのマクロ展開が、既に#[must_use]なResultを返すメソッドに
// メッセージ無しの#[must_use]を付けるため、新しいclippyが誤検出する。
// CIでは-A clippy::double_must_useで無効化しているが、エディタ
// （rust-analyzer）にも同じ扱いをさせるためクレート単位で許可する。
#![allow(clippy::double_must_use)]

pub mod cli_args;
pub mod config;
pub mod error;
pub mod handlers;
pub mod logging;
pub mod mcp;
pub mod plugin_proxy;
pub mod security;
pub mod transport;
pub mod types;

// Zero Trust（データベースのハンドラーのマイクロセグメンテーションが使う）
pub mod zero_trust;

pub use error::{Error, Result, SessionError};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::InvalidRequest("test".to_string());
        assert!(err.to_string().contains("test"));
    }
}
