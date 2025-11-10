//! Connection management utilities for MCP transports

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, SystemTime};

/// Connection information for transport layers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Connection type
    pub connection_type: String,
    /// Remote address (for network connections)
    pub remote_addr: Option<SocketAddr>,
    /// Local address (for network connections)
    pub local_addr: Option<SocketAddr>,
    /// Connection established time
    pub connected_at: SystemTime,
    /// Whether the connection is active
    pub is_active: bool,
}

impl ConnectionInfo {
    /// Create new connection info for STDIO
    pub fn stdio() -> Self {
        Self {
            connection_type: "stdio".to_string(),
            remote_addr: None,
            local_addr: None,
            connected_at: SystemTime::now(),
            is_active: true,
        }
    }

    /// Create new connection info for HTTP
    pub fn http(local_addr: SocketAddr) -> Self {
        Self {
            connection_type: "http".to_string(),
            remote_addr: None,
            local_addr: Some(local_addr),
            connected_at: SystemTime::now(),
            is_active: true,
        }
    }

    /// Create new connection info for WebSocket
    pub fn websocket(local_addr: SocketAddr, remote_addr: Option<SocketAddr>) -> Self {
        Self {
            connection_type: "websocket".to_string(),
            remote_addr,
            local_addr: Some(local_addr),
            connected_at: SystemTime::now(),
            is_active: true,
        }
    }
}

/// Connection statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionStats {
    /// Number of messages sent
    pub messages_sent: u64,
    /// Number of messages received
    pub messages_received: u64,
    /// Number of bytes sent
    pub bytes_sent: u64,
    /// Number of bytes received
    pub bytes_received: u64,
    /// Connection uptime
    pub uptime: Duration,
    /// Last activity time
    pub last_activity: Option<SystemTime>,
}

impl ConnectionStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a sent message
    pub fn record_sent_message(&mut self, bytes: usize) {
        self.messages_sent += 1;
        self.bytes_sent += bytes as u64;
        self.last_activity = Some(SystemTime::now());
    }

    /// Record a received message
    pub fn record_received_message(&mut self, bytes: usize) {
        self.messages_received += 1;
        self.bytes_received += bytes as u64;
        self.last_activity = Some(SystemTime::now());
    }

    /// Update uptime
    pub fn update_uptime(&mut self, start_time: SystemTime) {
        if let Ok(duration) = SystemTime::now().duration_since(start_time) {
            self.uptime = duration;
        }
    }
}
