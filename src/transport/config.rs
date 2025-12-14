//! Transport Configuration
//!
//! Configuration types for transport setup

use serde::{Deserialize, Serialize};

use super::http;
use super::stdio;
use super::types::TransportType;

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub transport_type: TransportType,
    pub stdio: stdio::StdioConfig,
    pub http: http::HttpConfig,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Stdio,
            stdio: stdio::StdioConfig::default(),
            http: http::HttpConfig::default(),
        }
    }
}
