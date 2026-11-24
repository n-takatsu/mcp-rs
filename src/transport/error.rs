//! Transport Errors
//!
//! Error types for transport operations

use thiserror::Error;

/// Transport-specific error types
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Message parsing error: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Transport not supported: {0}")]
    NotSupported(String),

    #[error("Buffer overflow: message too large")]
    BufferOverflow,

    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl From<crate::error::Error> for TransportError {
    fn from(err: crate::error::Error) -> Self {
        match err {
            crate::error::Error::Io(e) => TransportError::Io(e),
            crate::error::Error::Parse(e) => TransportError::Internal(e),
            crate::error::Error::Config(e) => TransportError::Configuration(e),
            crate::error::Error::Internal(e) => TransportError::Internal(e),
            crate::error::Error::NotSupported(e) => TransportError::NotSupported(e),
            // 包んでいた種類（Unauthorized等）を、そのまま返す（#760）
            crate::error::Error::TransportError(e) => e,
            _ => TransportError::Internal(format!("Converted error: {}", err)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 包んでいた`TransportError`は、`Internal`にせず、そのまま返す（#760）
    #[test]
    fn test_wrapped_transport_error_keeps_its_kind() {
        let err: TransportError = crate::error::Error::TransportError(
            TransportError::Unauthorized("missing API key".to_string()),
        )
        .into();
        assert!(
            matches!(&err, TransportError::Unauthorized(m) if m == "missing API key"),
            "{err:?}"
        );

        let err: TransportError = crate::error::Error::TransportError(
            TransportError::Configuration("bad bind address".to_string()),
        )
        .into();
        assert!(matches!(err, TransportError::Configuration(_)), "{err:?}");
    }
}
