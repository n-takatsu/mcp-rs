//! Transport Trait
//!
//! Core trait for all transport implementations

use crate::types::{JsonRpcRequest, JsonRpcResponse};
use async_trait::async_trait;
use std::fmt;

use super::connection::ConnectionStats;
use super::types::TransportInfo;

/// Transport layer abstraction for MCP communication
#[async_trait]
pub trait Transport: Send + Sync + fmt::Debug {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Start the transport and begin listening for connections
    async fn start(&mut self) -> std::result::Result<(), Self::Error>;

    /// Stop the transport and close all connections
    async fn stop(&mut self) -> std::result::Result<(), Self::Error>;

    /// Send a JSON-RPC response message
    async fn send_message(
        &mut self,
        message: JsonRpcResponse,
    ) -> std::result::Result<(), Self::Error>;

    /// Receive a JSON-RPC request message (non-blocking)
    async fn receive_message(&mut self)
        -> std::result::Result<Option<JsonRpcRequest>, Self::Error>;

    /// Check if the transport is currently connected/active
    fn is_connected(&self) -> bool;

    /// Get transport information and capabilities
    fn transport_info(&self) -> TransportInfo;

    /// Get current connection statistics
    fn connection_stats(&self) -> ConnectionStats;
}
