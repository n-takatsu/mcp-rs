//! MongoDB Database Engine
//!
//! Provides MongoDB-specific implementations for database operations
//! including document operations, aggregation pipelines, and GridFS support.

pub mod config;
pub mod connection;
pub mod document;
pub mod engine;
pub mod transaction;

pub use config::{MongoConfig, MongoConnectionOptions};
pub use connection::MongoConnection;
pub use document::{
    AggregationPipeline, AggregationStage, MongoDocument, MongoIndex, MongoIndexOptions,
    MongoResult, MongoStats,
};
pub use engine::MongoEngine;
pub use transaction::MongoTransaction;

use crate::handlers::database::types::DatabaseError;

/// MongoDB specific error conversion
pub(crate) fn convert_mongodb_error(err: mongodb::error::Error) -> DatabaseError {
    match err.kind.as_ref() {
        mongodb::error::ErrorKind::Authentication { .. } => {
            DatabaseError::ConnectionFailed(format!("Authentication failed: {}", err))
        }
        mongodb::error::ErrorKind::ConnectionPoolCleared { .. } => {
            DatabaseError::ConnectionFailed(format!("Connection pool cleared: {}", err))
        }
        mongodb::error::ErrorKind::InvalidArgument { .. } => {
            DatabaseError::InvalidQuery(format!("Invalid argument: {}", err))
        }
        _ => DatabaseError::QueryFailed(err.to_string()),
    }
}
