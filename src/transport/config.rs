//! Transport Configuration
//!
//! Configuration types for transport setup

use serde::{Deserialize, Serialize};

use super::http;
use super::types::TransportType;

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub transport_type: TransportType,
    pub http: http::HttpConfig,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            transport_type: TransportType::Stdio,
            http: http::HttpConfig::default(),
        }
    }
}
