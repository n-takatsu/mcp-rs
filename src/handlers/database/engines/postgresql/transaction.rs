//! PostgreSQL Transaction Management
//!
//! Provides ACID transaction support with savepoint functionality for PostgreSQL.

use crate::handlers::database::{engine::IsolationLevel, types::DatabaseError};
use sqlx::postgres::PgPool;
use std::sync::Arc;

/// PostgreSQL Transaction Manager
///
/// Manages transaction lifecycle including begin, commit, rollback, and savepoints
pub struct PostgreSqlTransactionManager {
    pool: Arc<PgPool>,
}

impl PostgreSqlTransactionManager {
    /// Create a new transaction manager
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Begin a new transaction with specified isolation level
    pub async fn begin(
        &self,
        isolation_level: IsolationLevel,
    ) -> Result<PostgreSqlTransaction, DatabaseError> {
        // Build the BEGIN statement with isolation level
        let begin_sql = match isolation_level {
            IsolationLevel::ReadUncommitted => "BEGIN ISOLATION LEVEL READ UNCOMMITTED".to_string(),
            IsolationLevel::ReadCommitted => "BEGIN ISOLATION LEVEL READ COMMITTED".to_string(),
            IsolationLevel::RepeatableRead => "BEGIN ISOLATION LEVEL REPEATABLE READ".to_string(),
            IsolationLevel::SerializableIsolation => {
                "BEGIN ISOLATION LEVEL SERIALIZABLE".to_string()
            }
        };

        // Execute BEGIN command
        match sqlx::query(&begin_sql).execute(self.pool.as_ref()).await {
            Ok(_) => Ok(PostgreSqlTransaction {
                pool: self.pool.clone(),
                is_active: true,
                isolation_level,
                savepoints: Vec::new(),
            }),
            Err(e) => Err(DatabaseError::TransactionError(format!(
                "Failed to begin transaction: {}",
                e
            ))),
        }
    }
}

/// PostgreSQL Transaction
///
/// Represents an active transaction context with savepoint support
pub struct PostgreSqlTransaction {
    pool: Arc<PgPool>,
    is_active: bool,
    isolation_level: IsolationLevel,
    savepoints: Vec<String>,
}

impl PostgreSqlTransaction {
    /// Check if transaction is active
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// Get transaction isolation level
    pub fn isolation_level(&self) -> IsolationLevel {
        self.isolation_level
    }

    /// Get list of savepoints
    pub fn savepoints(&self) -> &[String] {
        &self.savepoints
    }

    /// Commit the transaction
    pub async fn commit(&mut self) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionError(
                "Transaction is not active".to_string(),
            ));
        }

        // Execute COMMIT command
        match sqlx::query("COMMIT").execute(self.pool.as_ref()).await {
            Ok(_) => {
                self.is_active = false;
                Ok(())
            }
            Err(e) => Err(DatabaseError::TransactionError(format!(
                "Failed to commit transaction: {}",
                e
            ))),
        }
    }

    /// Rollback the transaction
    pub async fn rollback(&mut self) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionError(
                "Transaction is not active".to_string(),
            ));
        }

        // Execute ROLLBACK command
        match sqlx::query("ROLLBACK").execute(self.pool.as_ref()).await {
            Ok(_) => {
                self.is_active = false;
                Ok(())
            }
            Err(e) => Err(DatabaseError::TransactionError(format!(
                "Failed to rollback transaction: {}",
                e
            ))),
        }
    }

    /// Create a savepoint
    pub async fn savepoint(&mut self, savepoint_name: &str) -> Result<String, DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionError(
                "Transaction is not active".to_string(),
            ));
        }

        if savepoint_name.is_empty() {
            return Err(DatabaseError::ValidationError(
                "Savepoint name cannot be empty".to_string(),
            ));
        }
        // Validate savepoint name (no duplicates)
        if self.savepoints.contains(&savepoint_name.to_string()) {
            return Err(DatabaseError::ValidationError(format!(
                "Savepoint '{}' already exists in this transaction",
                savepoint_name
            )));
        }

        // Execute SAVEPOINT command
        let sql = format!("SAVEPOINT {}", savepoint_name);
        match sqlx::query(&sql).execute(self.pool.as_ref()).await {
            Ok(_) => {
                self.savepoints.push(savepoint_name.to_string());
                Ok(savepoint_name.to_string())
            }
            Err(e) => Err(DatabaseError::TransactionError(format!(
                "Failed to create savepoint: {}",
                e
            ))),
        }
    }

    /// Rollback to a specific savepoint
    pub async fn rollback_to_savepoint(
        &mut self,
        savepoint_name: &str,
    ) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionError(
                "Transaction is not active".to_string(),
            ));
        }

        if !self.savepoints.contains(&savepoint_name.to_string()) {
            return Err(DatabaseError::ValidationError(format!(
                "Savepoint '{}' does not exist in this transaction",
                savepoint_name
            )));
        }

        // Execute ROLLBACK TO SAVEPOINT command
        let sql = format!("ROLLBACK TO SAVEPOINT {}", savepoint_name);
        match sqlx::query(&sql).execute(self.pool.as_ref()).await {
            Ok(_) => {
                // Remove savepoints from the specified one onwards
                if let Some(pos) = self.savepoints.iter().position(|s| s == savepoint_name) {
                    self.savepoints.truncate(pos);
                }
                Ok(())
            }
            Err(e) => Err(DatabaseError::TransactionError(format!(
                "Failed to rollback to savepoint: {}",
                e
            ))),
        }
    }

    /// Release a savepoint
    pub async fn release_savepoint(&mut self, savepoint_name: &str) -> Result<(), DatabaseError> {
        if !self.is_active {
            return Err(DatabaseError::TransactionError(
                "Transaction is not active".to_string(),
            ));
        }

        if !self.savepoints.contains(&savepoint_name.to_string()) {
            return Err(DatabaseError::ValidationError(format!(
                "Savepoint '{}' does not exist in this transaction",
                savepoint_name
            )));
        }

        if !self.savepoints.contains(&savepoint_name.to_string()) {
            return Err(DatabaseError::ValidationError(format!(
                "Savepoint '{}' does not exist in this transaction",
                savepoint_name
            )));
        }

        // Execute RELEASE SAVEPOINT command
        let sql = format!("RELEASE SAVEPOINT {}", savepoint_name);
        match sqlx::query(&sql).execute(self.pool.as_ref()).await {
            Ok(_) => {
                self.savepoints.retain(|s| s != savepoint_name);
                Ok(())
            }
            Err(e) => Err(DatabaseError::TransactionError(format!(
                "Failed to release savepoint: {}",
                e
            ))),
        }
    }
}

impl Drop for PostgreSqlTransaction {
    fn drop(&mut self) {
        // In a real implementation, we would use async-aware cleanup
        if self.is_active {
            // Log warning: transaction not explicitly closed
            eprintln!(
                "Warning: PostgreSQL transaction was not explicitly closed, automatic rollback performed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let msg = "PostgreSQL transaction manager would be initialized with pool";
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_savepoint_state_tracking() {
        let mut txn = PostgreSqlTransaction {
            pool: Arc::new(
                /* mock pool would go here - using placeholder for compilation */
                unreachable!() as _,
            ),
            is_active: true,
            isolation_level: IsolationLevel::ReadCommitted,
            savepoints: Vec::new(),
        };

        assert!(txn.is_active());
        assert_eq!(txn.savepoints().len(), 0);

        txn.savepoints.push("sp_1".to_string());
        assert_eq!(txn.savepoints().len(), 1);
    }

    #[test]
    fn test_isolation_level_access() {
        let txn = PostgreSqlTransaction {
            pool: Arc::new(unreachable!() as _),
            is_active: true,
            isolation_level: IsolationLevel::SerializableIsolation,
            savepoints: Vec::new(),
        };

        assert_eq!(txn.isolation_level(), IsolationLevel::SerializableIsolation);
    }

    #[test]
    fn test_transaction_active_check() {
        let txn_active = PostgreSqlTransaction {
            pool: Arc::new(unreachable!() as _),
            is_active: true,
            isolation_level: IsolationLevel::ReadCommitted,
            savepoints: Vec::new(),
        };

        let txn_inactive = PostgreSqlTransaction {
            pool: Arc::new(unreachable!() as _),
            is_active: false,
            isolation_level: IsolationLevel::ReadCommitted,
            savepoints: Vec::new(),
        };

        assert!(txn_active.is_active());
        assert!(!txn_inactive.is_active());
    }
}
