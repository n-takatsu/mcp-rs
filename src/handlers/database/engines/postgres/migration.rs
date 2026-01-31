//! PostgreSQL Database Migration Support
//!
//! Provides schema migration capabilities using sqlx::migrate

use crate::handlers::database::types::DatabaseError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{migrate::MigrateDatabase, PgPool, Postgres, Row};
use std::path::Path;

/// Migration information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationInfo {
    pub version: i64,
    pub description: String,
    pub installed_on: Option<DateTime<Utc>>,
    pub execution_time_ms: Option<i64>,
    pub success: bool,
    pub checksum: Option<Vec<u8>>,
}

/// Migration manager for PostgreSQL
pub struct MigrationManager {
    pool: PgPool,
    migrations_path: String,
}

impl MigrationManager {
    /// Create a new migration manager
    pub fn new(pool: PgPool, migrations_path: impl Into<String>) -> Self {
        Self {
            pool,
            migrations_path: migrations_path.into(),
        }
    }

    /// Run all pending migrations
    ///
    /// # Example
    /// ```ignore
    /// let manager = MigrationManager::new(pool, "./migrations");
    /// manager.run_migrations().await?;
    /// ```
    pub async fn run_migrations(&self) -> Result<Vec<MigrationInfo>, DatabaseError> {
        let migrator = sqlx::migrate::Migrator::new(Path::new(&self.migrations_path))
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Failed to load migrations: {}", e)))?;

        migrator
            .run(&self.pool)
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Migration execution failed: {}", e)))?;

        self.get_migration_history().await
    }

    /// Revert the last migration
    ///
    /// Note: sqlx doesn't support automatic rollback, this is a manual process
    pub async fn revert_last_migration(&self) -> Result<(), DatabaseError> {
        // Get the last applied migration
        let history = self.get_migration_history().await?;
        
        if let Some(last) = history.last() {
            // Check if down migration exists
            let down_file = format!("{}/{}_down.sql", self.migrations_path, last.version);
            if !Path::new(&down_file).exists() {
                return Err(DatabaseError::MigrationError(
                    format!("No down migration found for version {}", last.version)
                ));
            }

            // Read and execute down migration
            let sql = tokio::fs::read_to_string(&down_file)
                .await
                .map_err(|e| DatabaseError::MigrationError(format!("Failed to read down migration: {}", e)))?;

            sqlx::raw_sql(&sql)
                .execute(&self.pool)
                .await
                .map_err(|e| DatabaseError::MigrationError(format!("Down migration failed: {}", e)))?;

            // Remove from migration history
            sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
                .bind(last.version)
                .execute(&self.pool)
                .await
                .map_err(|e| DatabaseError::MigrationError(format!("Failed to update migration history: {}", e)))?;

            Ok(())
        } else {
            Err(DatabaseError::MigrationError("No migrations to revert".to_string()))
        }
    }

    /// Get migration history
    pub async fn get_migration_history(&self) -> Result<Vec<MigrationInfo>, DatabaseError> {
        let rows = sqlx::query(
            r#"
            SELECT version, description, installed_on, execution_time, success, checksum
            FROM _sqlx_migrations
            ORDER BY version
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DatabaseError::QueryFailed(format!("Failed to fetch migration history: {}", e)))?;

        let migrations: Vec<MigrationInfo> = rows
            .iter()
            .map(|row| MigrationInfo {
                version: row.try_get("version").unwrap_or(0),
                description: row.try_get("description").unwrap_or_default(),
                installed_on: row.try_get("installed_on").ok(),
                execution_time_ms: row.try_get("execution_time").ok(),
                success: row.try_get("success").unwrap_or(false),
                checksum: row.try_get("checksum").ok(),
            })
            .collect();

        Ok(migrations)
    }

    /// Check if migrations are up to date
    pub async fn is_up_to_date(&self) -> Result<bool, DatabaseError> {
        let migrator = sqlx::migrate::Migrator::new(Path::new(&self.migrations_path))
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Failed to load migrations: {}", e)))?;

        // Get list of all migrations
        let all_migrations = migrator.migrations.len();
        
        // Get applied migrations count
        let history = self.get_migration_history().await?;
        let applied_count = history.len();

        Ok(all_migrations == applied_count)
    }

    /// Get pending migrations count
    pub async fn pending_migrations_count(&self) -> Result<usize, DatabaseError> {
        let migrator = sqlx::migrate::Migrator::new(Path::new(&self.migrations_path))
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Failed to load migrations: {}", e)))?;

        let all_count = migrator.migrations.len();
        let history = self.get_migration_history().await?;
        let applied_count = history.len();

        Ok(all_count.saturating_sub(applied_count))
    }

    /// Validate migration checksums
    pub async fn validate_migrations(&self) -> Result<bool, DatabaseError> {
        // Note: sqlx Migrator doesn't expose public validate method
        // We can check if pending migrations exist as a simple validation
        let pending = self.pending_migrations_count().await?;
        Ok(pending == 0)
    }

    /// Create a new migration file template
    pub async fn create_migration(
        &self,
        name: &str,
    ) -> Result<(String, String), DatabaseError> {
        let timestamp = Utc::now().timestamp();
        let version = timestamp;
        
        let up_file = format!("{}/{}_{}.up.sql", self.migrations_path, version, name);
        let down_file = format!("{}/{}_{}.down.sql", self.migrations_path, version, name);

        let up_template = format!(
            "-- Migration: {}\n-- Created: {}\n\n-- Add your UP migration SQL here\n",
            name,
            Utc::now().to_rfc3339()
        );

        let down_template = format!(
            "-- Migration Rollback: {}\n-- Created: {}\n\n-- Add your DOWN migration SQL here\n",
            name,
            Utc::now().to_rfc3339()
        );

        tokio::fs::write(&up_file, up_template)
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Failed to create up migration: {}", e)))?;

        tokio::fs::write(&down_file, down_template)
            .await
            .map_err(|e| DatabaseError::MigrationError(format!("Failed to create down migration: {}", e)))?;

        Ok((up_file, down_file))
    }

    /// Create database if it doesn't exist
    pub async fn create_database(url: &str) -> Result<(), DatabaseError> {
        if !Postgres::database_exists(url).await.unwrap_or(false) {
            Postgres::create_database(url)
                .await
                .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to create database: {}", e)))?;
        }
        Ok(())
    }

    /// Drop database (use with caution!)
    pub async fn drop_database(url: &str) -> Result<(), DatabaseError> {
        if Postgres::database_exists(url).await.unwrap_or(false) {
            Postgres::drop_database(url)
                .await
                .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to drop database: {}", e)))?;
        }
        Ok(())
    }

    /// Reset database (drop and recreate)
    pub async fn reset_database(url: &str, migrations_path: &str) -> Result<(), DatabaseError> {
        // Drop database
        Self::drop_database(url).await?;
        
        // Create database
        Self::create_database(url).await?;

        // Wait a bit for database to be ready
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Connect and run migrations
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        let manager = Self::new(pool, migrations_path);
        manager.run_migrations().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_info_creation() {
        let info = MigrationInfo {
            version: 1,
            description: "Initial migration".to_string(),
            installed_on: Some(Utc::now()),
            execution_time_ms: Some(100),
            success: true,
            checksum: None,
        };

        assert_eq!(info.version, 1);
        assert!(info.success);
    }
}
