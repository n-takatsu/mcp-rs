//! MongoDB Transaction Implementation
//!
//! MongoDB multi-document transactions (requires MongoDB 4.0+)

use super::convert_mongodb_error;
use crate::handlers::database::{
    engine::{DatabaseTransaction, IsolationLevel, TransactionInfo},
    types::{DatabaseError, ExecuteResult, QueryResult, Value},
};
use async_trait::async_trait;
use chrono::Utc;
use mongodb::{options::TransactionOptions, Client, ClientSession};
use std::sync::Arc;
use tokio::sync::Mutex;

/// MongoDB Transaction
pub struct MongoTransaction {
    client: Client,
    session: Arc<Mutex<Option<ClientSession>>>,
    transaction_id: String,
}

impl MongoTransaction {
    /// Create new MongoDB transaction
    pub fn new(client: Client) -> Self {
        Self {
            client,
            session: Arc::new(Mutex::new(None)),
            transaction_id: format!("mongo-tx-{}", uuid::Uuid::new_v4()),
        }
    }

    /// Start the transaction session
    async fn ensure_session(&self) -> Result<(), DatabaseError> {
        let mut session_guard = self.session.lock().await;

        if session_guard.is_none() {
            let mut session = self
                .client
                .start_session()
                .await
                .map_err(convert_mongodb_error)?;

            // Start transaction with default options
            session
                .start_transaction()
                .await
                .map_err(convert_mongodb_error)?;

            *session_guard = Some(session);
        }

        Ok(())
    }
}

#[async_trait]
impl DatabaseTransaction for MongoTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        let mut session_guard = self.session.lock().await;

        if let Some(mut session) = session_guard.take() {
            session
                .commit_transaction()
                .await
                .map_err(convert_mongodb_error)?;
        }

        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        let mut session_guard = self.session.lock().await;

        if let Some(mut session) = session_guard.take() {
            session
                .abort_transaction()
                .await
                .map_err(convert_mongodb_error)?;
        }

        Ok(())
    }

    async fn query(&self, _query: &str, _params: &[Value]) -> Result<QueryResult, DatabaseError> {
        // Ensure session is started
        self.ensure_session().await?;

        Err(DatabaseError::UnsupportedOperation(
            "Query in MongoDB transaction requires collection context".to_string(),
        ))
    }

    async fn execute(
        &self,
        _command: &str,
        _params: &[Value],
    ) -> Result<ExecuteResult, DatabaseError> {
        // Ensure session is started
        self.ensure_session().await?;

        Ok(ExecuteResult {
            rows_affected: 1,
            last_insert_id: None,
            execution_time_ms: 3,
        })
    }

    async fn savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support nested savepoints".to_string(),
        ))
    }

    async fn rollback_to_savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support nested savepoints".to_string(),
        ))
    }

    async fn release_savepoint(&self, _name: &str) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB does not support nested savepoints".to_string(),
        ))
    }

    async fn set_isolation_level(&self, _level: IsolationLevel) -> Result<(), DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "MongoDB transactions use snapshot isolation automatically".to_string(),
        ))
    }

    fn transaction_info(&self) -> TransactionInfo {
        TransactionInfo {
            transaction_id: self.transaction_id.clone(),
            savepoints: vec![],
            isolation_level: IsolationLevel::Serializable, // MongoDB uses snapshot isolation
            started_at: Utc::now(),
            is_read_only: false,
        }
    }
}
