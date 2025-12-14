//! Operator Types
//!
//! Kubernetes Operator用の型定義

use kube::Client;
use thiserror::Error;

/// Operator errors
#[derive(Error, Debug)]
pub enum OperatorError {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Invalid resource spec: {0}")]
    InvalidSpec(String),

    #[error("Reconciliation failed: {0}")]
    ReconcileError(String),
}

/// Result type for operator operations
pub type Result<T, E = OperatorError> = std::result::Result<T, E>;

/// Context data for the controller
pub struct Context {
    pub client: Client,
}

impl Context {
    pub fn new(client: Client) -> Self {
        Self { client }
    }
}

/// Finalizer name for MCP resources
pub const FINALIZER_NAME: &str = "mcp.n-takatsu.dev/finalizer";
