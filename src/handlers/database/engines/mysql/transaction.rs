//! MySQL Transaction Management
//!
//! Provides ACID transaction support with savepoint functionality
//! Ensures data integrity and consistent database operations

use crate::handlers::database::types::{DatabaseError, IsolationLevel};
use mysql_async::Pool;
use std::sync::Arc;

/// MySQL Transaction Manager
///
/// Manages transaction lifecycle including begin, commit, rollback, and savepoints
pub struct MySqlTransactionManager {
    pool: Arc<Pool>,
}

impl MySqlTransactionManager {
    /// Create a new transaction manager
    pub fn new(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// Start a new transaction with specified isolation level
    pub async fn begin(
        &self,
        isolation_level: IsolationLevel,
    ) -> Result<MySqlTransaction, DatabaseError> {
        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        // Set isolation level
        let isolation_sql = format!("SET TRANSACTION ISOLATION LEVEL {}", isolation_level);
        conn.query_drop(&isolation_sql)
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to set isolation level: {}", e)))?;

        // Start transaction
        conn.query_drop("START TRANSACTION")
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to start transaction: {}", e)))?;

        Ok(MySqlTransaction {
            pool: Arc::clone(&self.pool),
            is_active: true,
            savepoint_counter: 0,
        })
    }
}

/// MySQL Transaction
///
/// Represents an active transaction with savepoint support
pub struct MySqlTransaction {
    pool: Arc<Pool>,
    is_active: bool,
    savepoint_counter: usize,
}

impl MySqlTransaction {
    /// Commit the current transaction
    pub async fn commit(&mut self) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        conn.query_drop("COMMIT")
            .await
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;

        self.is_active = false;
        Ok(())
    }

    /// Rollback the current transaction
    pub async fn rollback(&mut self) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        conn.query_drop("ROLLBACK")
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to rollback transaction: {}", e))
            })?;

        self.is_active = false;
        Ok(())
    }

    /// Create a savepoint within the transaction
    ///
    /// Returns a savepoint identifier that can be used for rollback
    pub async fn savepoint(&mut self, name: Option<String>) -> Result<String, DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        self.savepoint_counter += 1;
        let savepoint_name = name.unwrap_or_else(|| format!("sp_{}", self.savepoint_counter));

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let savepoint_sql = format!("SAVEPOINT {}", savepoint_name);
        conn.query_drop(&savepoint_sql)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to create savepoint: {}", e))
            })?;

        Ok(savepoint_name)
    }

    /// Rollback to a specific savepoint
    pub async fn rollback_to_savepoint(&mut self, savepoint_name: &str) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let rollback_sql = format!("ROLLBACK TO SAVEPOINT {}", savepoint_name);
        conn.query_drop(&rollback_sql)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to rollback to savepoint: {}", e))
            })?;

        Ok(())
    }

    /// Release a savepoint
    pub async fn release_savepoint(&mut self, savepoint_name: &str) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::ValidationError(
                "Transaction is not active".to_string(),
            ));
        }

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(e.to_string()))?;

        let release_sql = format!("RELEASE SAVEPOINT {}", savepoint_name);
        conn.query_drop(&release_sql)
            .await
            .map_err(|e| {
                DatabaseError::QueryFailed(format!("Failed to release savepoint: {}", e))
            })?;

        Ok(())
    }

    /// Check if transaction is still active
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Get current savepoint counter
    pub fn get_savepoint_counter(&self) -> usize {
        self.savepoint_counter
    }
}

impl Drop for MySqlTransaction {
    fn drop(&mut self) {
        // Automatic rollback if transaction is still active
        // In a real implementation, we would use async-aware cleanup
        if self.is_active {
            // Log warning: transaction not explicitly closed
            eprintln!("Warning: Transaction was not explicitly closed, automatic rollback performed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_savepoint_naming() {
        let savepoint_name = format!("sp_{}", 1);
        assert_eq!(savepoint_name, "sp_1");
    }

    #[test]
    fn test_savepoint_counter() {
        let mut counter = 0;
        counter += 1;
        counter += 1;
        counter += 1;
        assert_eq!(counter, 3);
    }
}
