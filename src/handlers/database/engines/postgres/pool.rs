//! Optimized PostgreSQL connection pool implementation

use crate::handlers::database::engines::postgres::{
    config::PostgresConfig,
    error::{PostgresError, Result},
};
use serde::{Deserialize, Serialize};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::str::FromStr;

/// Optimized connection pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizedPoolConfig {
    /// Minimum number of connections to maintain
    pub min_connections: u32,
    /// Maximum number of connections allowed
    pub max_connections: u32,
    /// Connection acquisition timeout in seconds
    pub acquire_timeout_secs: u64,
    /// Idle connection timeout in seconds
    pub idle_timeout_secs: u64,
    /// Maximum connection lifetime in seconds
    pub max_lifetime_secs: u64,
    /// Whether to test connections on acquire
    pub test_on_acquire: bool,
    /// Prepared statement cache size per connection
    pub statement_cache_capacity: usize,
}

impl Default for OptimizedPoolConfig {
    fn default() -> Self {
        Self {
            min_connections: 5,
            max_connections: 100,
            acquire_timeout_secs: 30,
            idle_timeout_secs: 600,
            max_lifetime_secs: 1800,
            test_on_acquire: true,
            statement_cache_capacity: 1000,
        }
    }
}

impl From<&PostgresConfig> for OptimizedPoolConfig {
    fn from(config: &PostgresConfig) -> Self {
        Self {
            min_connections: config.min_connections,
            max_connections: config.max_connections,
            acquire_timeout_secs: config.connection_timeout_secs,
            idle_timeout_secs: config.idle_timeout_secs,
            max_lifetime_secs: config.max_lifetime_secs,
            test_on_acquire: true,
            statement_cache_capacity: config.statement_cache_capacity,
        }
    }
}

/// Pool metrics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolMetrics {
    /// Current number of connections
    pub connections: u32,
    /// Number of idle connections
    pub idle_connections: u32,
    /// Connection pool efficiency (0.0 - 1.0)
    pub efficiency: f64,
    /// Total number of connection requests
    pub total_requests: u64,
    /// Number of failed connection attempts
    pub failed_attempts: u64,
    /// Average connection acquisition time in milliseconds
    pub avg_acquire_time_ms: f64,
}

impl PoolMetrics {
    /// Update metrics from the current pool state
    pub fn update_from_pool(&mut self, pool: &PgPool) {
        self.connections = pool.size();
        self.idle_connections = pool.num_idle() as u32;

        // Calculate efficiency as ratio of active connections to total
        if self.connections > 0 {
            let active = self.connections.saturating_sub(self.idle_connections);
            self.efficiency = active as f64 / self.connections as f64;
        } else {
            self.efficiency = 0.0;
        }
    }

    /// Check if pool efficiency meets the target (90%+)
    pub fn meets_target_efficiency(&self) -> bool {
        self.efficiency >= 0.9
    }
}

/// Create an optimized PostgreSQL connection pool
///
/// # Arguments
///
/// * `config` - PostgreSQL configuration
///
/// # Returns
///
/// An optimized connection pool with the specified configuration
///
/// # Example
///
/// ```no_run
/// use mcp_rs::handlers::database::engines::postgres::{PostgresConfig, create_optimized_pool};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = PostgresConfig::builder()
///     .host("localhost")
///     .database("mydb")
///     .username("postgres")
///     .password("password")
///     .min_connections(10)
///     .max_connections(50)
///     .build()?;
///
/// let pool = create_optimized_pool(&config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_optimized_pool(config: &PostgresConfig) -> Result<PgPool> {
    // Parse connection options from connection string
    let connect_opts = PgConnectOptions::from_str(&config.connection_string())
        .map_err(|e| PostgresError::Configuration(format!("Invalid connection string: {}", e)))?
        .application_name(&config.application_name)
        .statement_cache_capacity(config.statement_cache_capacity);

    // Build pool with optimized settings
    let pool = PgPoolOptions::new()
        .min_connections(config.min_connections)
        .max_connections(config.max_connections)
        .acquire_timeout(config.connection_timeout())
        .idle_timeout(Some(config.idle_timeout()))
        .max_lifetime(Some(config.max_lifetime()))
        .test_before_acquire(true)
        .connect_with(connect_opts)
        .await
        .map_err(|e| PostgresError::Connection(format!("Failed to create pool: {}", e)))?;

    // Verify pool health
    sqlx::query("SELECT 1")
        .fetch_one(&pool)
        .await
        .map_err(|e| PostgresError::Connection(format!("Pool health check failed: {}", e)))?;

    tracing::info!(
        "Created optimized PostgreSQL connection pool: min={}, max={}, cache={}",
        config.min_connections,
        config.max_connections,
        config.statement_cache_capacity
    );

    Ok(pool)
}

/// Health check for a connection pool
///
/// # Arguments
///
/// * `pool` - The connection pool to check
///
/// # Returns
///
/// `true` if the pool is healthy, `false` otherwise
pub async fn health_check(pool: &PgPool) -> bool {
    match sqlx::query("SELECT 1").fetch_one(pool).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("PostgreSQL pool health check failed: {}", e);
            false
        }
    }
}

/// Get current pool metrics
///
/// # Arguments
///
/// * `pool` - The connection pool to analyze
///
/// # Returns
///
/// Current pool metrics
pub fn get_pool_metrics(pool: &PgPool) -> PoolMetrics {
    let mut metrics = PoolMetrics::default();
    metrics.update_from_pool(pool);
    metrics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimized_pool_config_default() {
        let config = OptimizedPoolConfig::default();
        assert_eq!(config.min_connections, 5);
        assert_eq!(config.max_connections, 100);
        assert_eq!(config.statement_cache_capacity, 1000);
        assert!(config.test_on_acquire);
    }

    #[test]
    fn test_pool_metrics_efficiency() {
        let mut metrics = PoolMetrics {
            connections: 100,
            idle_connections: 10,
            ..PoolMetrics::default()
        };

        // Manually calculate efficiency
        let active = metrics.connections - metrics.idle_connections;
        metrics.efficiency = active as f64 / metrics.connections as f64;

        assert_eq!(metrics.efficiency, 0.9);
        assert!(metrics.meets_target_efficiency());
    }

    #[test]
    fn test_pool_metrics_low_efficiency() {
        let mut metrics = PoolMetrics {
            connections: 100,
            idle_connections: 50,
            ..PoolMetrics::default()
        };

        let active = metrics.connections - metrics.idle_connections;
        metrics.efficiency = active as f64 / metrics.connections as f64;

        assert_eq!(metrics.efficiency, 0.5);
        assert!(!metrics.meets_target_efficiency());
    }
}
