//! Concurrent Connection Performance Tests
//!
//! Comprehensive testing for connection pool efficiency, resource contention,
//! and performance degradation under increasing concurrent load

#![cfg(feature = "database")]

use mcp_rs::handlers::database::{
    engine::DatabaseEngine,
    engines::mysql::MySqlEngine,
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig, SecurityConfig,
        Value,
    },
};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{self, sync::Semaphore, time::sleep};

/// Concurrent connection test configuration
#[derive(Debug, Clone)]
pub struct ConcurrentConnectionConfig {
    pub min_connections: usize,
    pub max_connections: usize,
    pub connection_step: usize,
    pub test_duration_per_level_seconds: u64,
    pub queries_per_connection: usize,
    pub connection_pool_sizes: Vec<usize>,
    pub query_timeout_seconds: u64,
    pub ramp_up_duration_seconds: u64,
    pub sustained_load_duration_seconds: u64,
}

impl Default for ConcurrentConnectionConfig {
    fn default() -> Self {
        Self {
            min_connections: 1,
            max_connections: 100,
            connection_step: 10,
            test_duration_per_level_seconds: 30,
            queries_per_connection: 50,
            connection_pool_sizes: vec![10, 20, 50, 100],
            query_timeout_seconds: 30,
            ramp_up_duration_seconds: 60,
            sustained_load_duration_seconds: 120,
        }
    }
}

/// Connection performance metrics
#[derive(Debug, Clone)]
pub struct ConcurrentMetrics {
    pub test_name: String,
    pub concurrent_connections: usize,
    pub connection_pool_size: usize,
    pub total_queries: usize,
    pub successful_queries: usize,
    pub failed_queries: usize,
    pub connection_failures: usize,
    pub query_timeouts: usize,
    pub total_duration_seconds: f64,
    pub queries_per_second: f64,
    pub avg_query_time_ms: f64,
    pub avg_connection_time_ms: f64,
    pub p95_query_time_ms: f64,
    pub p99_query_time_ms: f64,
    pub error_rate_percentage: f64,
    pub connection_efficiency_percentage: f64,
    pub resource_utilization: HashMap<String, f64>,
    pub performance_degradation_percentage: f64,
}

impl ConcurrentMetrics {
    pub fn new(test_name: String, concurrent_connections: usize, pool_size: usize) -> Self {
        Self {
            test_name,
            concurrent_connections,
            connection_pool_size: pool_size,
            total_queries: 0,
            successful_queries: 0,
            failed_queries: 0,
            connection_failures: 0,
            query_timeouts: 0,
            total_duration_seconds: 0.0,
            queries_per_second: 0.0,
            avg_query_time_ms: 0.0,
            avg_connection_time_ms: 0.0,
            p95_query_time_ms: 0.0,
            p99_query_time_ms: 0.0,
            error_rate_percentage: 0.0,
            connection_efficiency_percentage: 0.0,
            resource_utilization: HashMap::new(),
            performance_degradation_percentage: 0.0,
        }
    }

    pub fn calculate_statistics(&mut self, query_times: &mut [u64], connection_times: &mut [u64]) {
        // Query time statistics
        if !query_times.is_empty() {
            query_times.sort();

            let total_query_time: u64 = query_times.iter().sum();
            self.avg_query_time_ms = total_query_time as f64 / query_times.len() as f64;

            let p95_index = (query_times.len() as f64 * 0.95) as usize;
            let p99_index = (query_times.len() as f64 * 0.99) as usize;

            self.p95_query_time_ms = query_times[p95_index.min(query_times.len() - 1)] as f64;
            self.p99_query_time_ms = query_times[p99_index.min(query_times.len() - 1)] as f64;
        }

        // Connection time statistics
        if !connection_times.is_empty() {
            let total_connection_time: u64 = connection_times.iter().sum();
            self.avg_connection_time_ms =
                total_connection_time as f64 / connection_times.len() as f64;
        }

        // Calculate derived metrics
        if self.total_duration_seconds > 0.0 {
            self.queries_per_second = self.successful_queries as f64 / self.total_duration_seconds;
        }

        if self.total_queries > 0 {
            self.error_rate_percentage =
                (self.failed_queries as f64 / self.total_queries as f64) * 100.0;
        }

        // Connection efficiency (successful connections vs attempted)
        let attempted_connections = self.concurrent_connections;
        let successful_connections = attempted_connections - self.connection_failures;
        if attempted_connections > 0 {
            self.connection_efficiency_percentage =
                (successful_connections as f64 / attempted_connections as f64) * 100.0;
        }
    }
}

/// Connection pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    pub active_connections: usize,
    pub idle_connections: usize,
    pub pool_utilization_percentage: f64,
    pub wait_time_ms: f64,
    pub connection_recycling_rate: f64,
}

/// Concurrent Connection Performance Test Suite
pub struct ConcurrentConnectionTest {
    config: ConcurrentConnectionConfig,
    engine: Option<MySqlEngine>,
    baseline_qps: f64, // For performance degradation calculation
    metrics: Vec<ConcurrentMetrics>,
}

impl ConcurrentConnectionTest {
    pub fn new(config: ConcurrentConnectionConfig) -> Self {
        Self {
            config,
            engine: None,
            baseline_qps: 0.0,
            metrics: Vec::new(),
        }
    }

    /// Initialize MySQL engine for concurrent connection testing
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Initializing Concurrent Connection Performance Test");

        let db_config = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: ConnectionConfig {
                host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("MYSQL_PORT")
                    .unwrap_or_else(|_| "3306".to_string())
                    .parse()
                    .unwrap_or(3306),
                database: std::env::var("MYSQL_DATABASE")
                    .unwrap_or_else(|_| "concurrent_test_db".to_string()),
                username: std::env::var("MYSQL_USER")
                    .unwrap_or_else(|_| "concurrent_user".to_string()),
                password: std::env::var("MYSQL_PASSWORD")
                    .unwrap_or_else(|_| "concurrent_pass".to_string()),
                ssl_mode: Some("disabled".to_string()),
                timeout_seconds: 30,
                retry_attempts: 3,
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig {
                max_connections: (self.config.max_connections + 20) as u32, // Extra headroom
                min_connections: 5,
                connection_timeout: 30,
                idle_timeout: 300,
                max_lifetime: 3600,
            },
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        };

        self.engine = Some(MySqlEngine::new_without_security(db_config).await?);

        // Establish baseline performance with single connection
        self.baseline_qps = self.measure_baseline_performance().await?;

        println!("✅ MySQL engine initialized for concurrent testing");
        println!("📊 Baseline performance: {:.2} QPS", self.baseline_qps);

        Ok(())
    }

    /// Measure baseline performance with single connection
    async fn measure_baseline_performance(&self) -> Result<f64, Box<dyn std::error::Error>> {
        println!("📊 Measuring baseline performance...");

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        let test_start = Instant::now();
        let mut successful_queries = 0;

        for i in 0..100 {
            if (connection
                .query("SELECT ? as baseline_test", &[Value::from_i64(i)])
                .await)
                .is_ok()
            {
                successful_queries += 1;
            }
        }

        let duration = test_start.elapsed().as_secs_f64();
        Ok(successful_queries as f64 / duration)
    }

    /// Test connection scaling performance
    pub async fn test_connection_scaling(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📈 Testing Connection Scaling Performance");

        let mut connection_levels = Vec::new();
        let mut current = self.config.min_connections;
        while current <= self.config.max_connections {
            connection_levels.push(current);
            current += self.config.connection_step;
        }

        for &connection_count in &connection_levels {
            println!("  Testing {} concurrent connections", connection_count);

            let mut metrics = ConcurrentMetrics::new(
                format!("Connection Scaling ({})", connection_count),
                connection_count,
                self.config.connection_pool_sizes[0], // Use first pool size
            );

            let test_results = self
                .run_concurrent_test(
                    connection_count,
                    self.config.connection_pool_sizes[0],
                    self.config.test_duration_per_level_seconds,
                )
                .await?;

            metrics.total_queries = test_results.total_queries;
            metrics.successful_queries = test_results.successful_queries;
            metrics.failed_queries = test_results.failed_queries;
            metrics.connection_failures = test_results.connection_failures;
            metrics.query_timeouts = test_results.query_timeouts;
            metrics.total_duration_seconds = test_results.duration_seconds;

            // Calculate performance degradation from baseline
            if self.baseline_qps > 0.0 && test_results.queries_per_second > 0.0 {
                metrics.performance_degradation_percentage =
                    ((self.baseline_qps - test_results.queries_per_second) / self.baseline_qps)
                        * 100.0;
            }

            let mut query_times = test_results.query_times;
            let mut connection_times = test_results.connection_times;
            metrics.calculate_statistics(&mut query_times, &mut connection_times);

            self.metrics.push(metrics.clone());
            self.print_concurrent_metrics(&metrics);
        }

        Ok(())
    }

    /// Test different connection pool sizes
    pub async fn test_pool_efficiency(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🏊 Testing Connection Pool Efficiency");

        let fixed_connections = 50; // Fixed concurrent connection count

        for &pool_size in &self.config.connection_pool_sizes {
            println!(
                "  Testing pool size: {} (with {} concurrent connections)",
                pool_size, fixed_connections
            );

            let mut metrics = ConcurrentMetrics::new(
                format!("Pool Efficiency (size: {})", pool_size),
                fixed_connections,
                pool_size,
            );

            // Reconfigure engine with specific pool size
            let mut db_config = self.engine.as_ref().unwrap().get_config().clone();
            db_config.pool.max_connections = pool_size as u32;

            let pool_engine = Arc::new(MySqlEngine::new_without_security(db_config).await?);

            let test_results = self
                .run_concurrent_test_with_engine(
                    pool_engine,
                    fixed_connections,
                    pool_size,
                    self.config.test_duration_per_level_seconds,
                )
                .await?;

            metrics.total_queries = test_results.total_queries;
            metrics.successful_queries = test_results.successful_queries;
            metrics.failed_queries = test_results.failed_queries;
            metrics.connection_failures = test_results.connection_failures;
            metrics.query_timeouts = test_results.query_timeouts;
            metrics.total_duration_seconds = test_results.duration_seconds;

            let mut query_times = test_results.query_times;
            let mut connection_times = test_results.connection_times;
            metrics.calculate_statistics(&mut query_times, &mut connection_times);

            // Add pool-specific metrics
            metrics.resource_utilization.insert(
                "pool_utilization".to_string(),
                (fixed_connections as f64 / pool_size as f64) * 100.0,
            );

            self.metrics.push(metrics.clone());
            self.print_concurrent_metrics(&metrics);
        }

        Ok(())
    }

    /// Test sustained load performance
    pub async fn test_sustained_load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("⏱️ Testing Sustained Load Performance");

        let connection_count = 50;
        let pool_size = 75;

        println!(
            "  Ramp-up phase: {} seconds",
            self.config.ramp_up_duration_seconds
        );
        let ramp_up_results = self.run_ramp_up_test(connection_count, pool_size).await?;

        println!(
            "  Sustained load phase: {} seconds",
            self.config.sustained_load_duration_seconds
        );
        let sustained_results = self
            .run_concurrent_test(
                connection_count,
                pool_size,
                self.config.sustained_load_duration_seconds,
            )
            .await?;

        // Create combined metrics
        let mut metrics =
            ConcurrentMetrics::new("Sustained Load".to_string(), connection_count, pool_size);

        metrics.total_queries = ramp_up_results.total_queries + sustained_results.total_queries;
        metrics.successful_queries =
            ramp_up_results.successful_queries + sustained_results.successful_queries;
        metrics.failed_queries = ramp_up_results.failed_queries + sustained_results.failed_queries;
        metrics.connection_failures =
            ramp_up_results.connection_failures + sustained_results.connection_failures;
        metrics.total_duration_seconds =
            ramp_up_results.duration_seconds + sustained_results.duration_seconds;

        // Use sustained phase metrics for performance calculations
        let mut query_times = sustained_results.query_times;
        let mut connection_times = sustained_results.connection_times;
        metrics.calculate_statistics(&mut query_times, &mut connection_times);

        // Add sustained load specific metrics
        metrics.resource_utilization.insert(
            "ramp_up_success_rate".to_string(),
            (ramp_up_results.successful_queries as f64 / ramp_up_results.total_queries as f64)
                * 100.0,
        );
        metrics.resource_utilization.insert(
            "sustained_success_rate".to_string(),
            (sustained_results.successful_queries as f64 / sustained_results.total_queries as f64)
                * 100.0,
        );

        self.metrics.push(metrics.clone());
        self.print_concurrent_metrics(&metrics);

        Ok(())
    }

    /// Test resource contention scenarios
    pub async fn test_resource_contention(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("⚔️ Testing Resource Contention Scenarios");

        // Test scenario: Many connections with small pool
        let high_contention_metrics = self
            .test_contention_scenario(
                "High Contention",
                80, // connections
                10, // small pool
            )
            .await?;

        // Test scenario: Moderate connections with adequate pool
        let moderate_contention_metrics = self
            .test_contention_scenario(
                "Moderate Contention",
                40, // connections
                30, // adequate pool
            )
            .await?;

        // Test scenario: Few connections with large pool
        let low_contention_metrics = self
            .test_contention_scenario(
                "Low Contention",
                20, // connections
                50, // large pool
            )
            .await?;

        self.metrics.push(high_contention_metrics);
        self.metrics.push(moderate_contention_metrics);
        self.metrics.push(low_contention_metrics);

        Ok(())
    }

    /// Test specific contention scenario
    async fn test_contention_scenario(
        &self,
        scenario_name: &str,
        connection_count: usize,
        pool_size: usize,
    ) -> Result<ConcurrentMetrics, Box<dyn std::error::Error>> {
        println!(
            "  Testing {}: {} connections, {} pool size",
            scenario_name, connection_count, pool_size
        );

        let mut metrics =
            ConcurrentMetrics::new(scenario_name.to_string(), connection_count, pool_size);

        let test_results = self
            .run_concurrent_test(
                connection_count,
                pool_size,
                self.config.test_duration_per_level_seconds,
            )
            .await?;

        metrics.total_queries = test_results.total_queries;
        metrics.successful_queries = test_results.successful_queries;
        metrics.failed_queries = test_results.failed_queries;
        metrics.connection_failures = test_results.connection_failures;
        metrics.query_timeouts = test_results.query_timeouts;
        metrics.total_duration_seconds = test_results.duration_seconds;

        let mut query_times = test_results.query_times;
        let mut connection_times = test_results.connection_times;
        metrics.calculate_statistics(&mut query_times, &mut connection_times);

        // Calculate contention metrics
        let contention_ratio = connection_count as f64 / pool_size as f64;
        metrics
            .resource_utilization
            .insert("contention_ratio".to_string(), contention_ratio);
        metrics.resource_utilization.insert(
            "expected_wait_factor".to_string(),
            contention_ratio.max(1.0),
        );

        self.print_concurrent_metrics(&metrics);

        Ok(metrics)
    }

    /// Run ramp-up test (gradually increasing load)
    async fn run_ramp_up_test(
        &self,
        target_connections: usize,
        pool_size: usize,
    ) -> Result<TestResults, Box<dyn std::error::Error>> {
        let mut db_config = self.engine.as_ref().unwrap().get_config().clone();
        db_config.pool.max_connections = pool_size as u32;

        let engine = Arc::new(MySqlEngine::new_without_security(db_config).await?);

        let mut all_results = TestResults::new();
        let test_start = Instant::now();

        let ramp_step_duration = self.config.ramp_up_duration_seconds / 10; // 10 steps
        let connection_step = target_connections / 10;

        for step in 1..=10 {
            let current_connections = connection_step * step;
            println!(
                "    Ramp-up step {}: {} connections",
                step, current_connections
            );

            let step_results = self
                .run_concurrent_test_with_engine(
                    engine.clone(),
                    current_connections,
                    pool_size,
                    ramp_step_duration,
                )
                .await?;

            all_results.merge(step_results);
        }

        all_results.duration_seconds = test_start.elapsed().as_secs_f64();
        all_results.queries_per_second =
            all_results.successful_queries as f64 / all_results.duration_seconds;

        Ok(all_results)
    }

    /// Run concurrent test with default engine
    async fn run_concurrent_test(
        &self,
        connection_count: usize,
        pool_size: usize,
        duration_seconds: u64,
    ) -> Result<TestResults, Box<dyn std::error::Error>> {
        let engine = Arc::new(self.engine.as_ref().unwrap().clone());
        self.run_concurrent_test_with_engine(engine, connection_count, pool_size, duration_seconds)
            .await
    }

    /// Run concurrent test with specific engine
    async fn run_concurrent_test_with_engine(
        &self,
        engine: Arc<MySqlEngine>,
        connection_count: usize,
        _pool_size: usize,
        duration_seconds: u64,
    ) -> Result<TestResults, Box<dyn std::error::Error>> {
        let semaphore = Arc::new(Semaphore::new(connection_count));
        let active_tasks = Arc::new(AtomicUsize::new(0));

        let test_start = Instant::now();
        let test_end = test_start + Duration::from_secs(duration_seconds);

        let mut handles = Vec::new();
        let mut task_id = 0;

        while Instant::now() < test_end {
            // Check if we can start more tasks
            if active_tasks.load(Ordering::Relaxed) >= connection_count {
                sleep(Duration::from_millis(10)).await;
                continue;
            }

            let semaphore_clone = semaphore.clone();
            let active_tasks_clone = active_tasks.clone();
            let engine_clone = engine.clone();
            let config_clone = engine.get_config().clone();
            let current_task_id = task_id;
            let queries_per_connection = self.config.queries_per_connection;

            let handle = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();
                active_tasks_clone.fetch_add(1, Ordering::Relaxed);

                let connection_start = Instant::now();
                let connection_result = engine_clone.connect(&config_clone).await;
                let connection_time = connection_start.elapsed().as_millis() as u64;

                match connection_result {
                    Ok(connection) => {
                        let mut local_query_times = Vec::new();
                        let mut local_successful = 0;
                        let mut local_failed = 0;
                        let mut local_timeouts = 0;

                        for i in 0..queries_per_connection {
                            let query_start = Instant::now();

                            let params = vec![
                                Value::from_i64(current_task_id as i64),
                                Value::from_i64(i as i64),
                                Value::String(format!("concurrent_test_{}_{}", current_task_id, i)),
                                Value::from_i64(chrono::Utc::now().timestamp()),
                            ];

                            match tokio::time::timeout(
                                Duration::from_secs(30),
                                connection.query("SELECT ?, ?, ?, ? as concurrent_query", &params),
                            )
                            .await
                            {
                                Ok(Ok(_)) => {
                                    local_successful += 1;
                                    local_query_times
                                        .push(query_start.elapsed().as_millis() as u64);
                                }
                                Ok(Err(_)) => local_failed += 1,
                                Err(_) => local_timeouts += 1, // Timeout
                            }
                        }

                        active_tasks_clone.fetch_sub(1, Ordering::Relaxed);
                        (
                            local_successful,
                            local_failed,
                            local_timeouts,
                            0,
                            local_query_times,
                            vec![connection_time],
                        )
                    }
                    Err(_) => {
                        active_tasks_clone.fetch_sub(1, Ordering::Relaxed);
                        (
                            0,
                            queries_per_connection,
                            0,
                            1,
                            Vec::new(),
                            vec![connection_time],
                        )
                    }
                }
            });

            handles.push(handle);
            task_id += 1;

            // Prevent unlimited task creation
            if handles.len() >= connection_count * 3 {
                break;
            }
        }

        // Collect results from all tasks
        let mut results = TestResults::new();

        for handle in handles {
            match handle.await {
                Ok((successful, failed, timeouts, conn_failures, query_times, conn_times)) => {
                    results.successful_queries += successful;
                    results.failed_queries += failed;
                    results.query_timeouts += timeouts;
                    results.connection_failures += conn_failures;
                    results.query_times.extend(query_times);
                    results.connection_times.extend(conn_times);
                }
                Err(_) => {
                    results.failed_queries += self.config.queries_per_connection;
                    results.connection_failures += 1;
                }
            }
        }

        results.total_queries = results.successful_queries + results.failed_queries;
        results.duration_seconds = test_start.elapsed().as_secs_f64();

        if results.duration_seconds > 0.0 {
            results.queries_per_second =
                results.successful_queries as f64 / results.duration_seconds;
        }

        Ok(results)
    }

    /// Print concurrent connection metrics
    fn print_concurrent_metrics(&self, metrics: &ConcurrentMetrics) {
        println!("\n📊 {} Results:", metrics.test_name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!(
            "🔗 Concurrent Connections: {}",
            metrics.concurrent_connections
        );
        println!("🏊 Pool Size: {}", metrics.connection_pool_size);
        println!("📈 Total Queries: {}", metrics.total_queries);
        println!(
            "✅ Successful: {} ({:.1}%)",
            metrics.successful_queries,
            100.0 - metrics.error_rate_percentage
        );
        println!("❌ Failed Queries: {}", metrics.failed_queries);
        println!("🔌 Connection Failures: {}", metrics.connection_failures);
        println!("⏰ Query Timeouts: {}", metrics.query_timeouts);
        println!("⚡ Queries per Second: {:.2}", metrics.queries_per_second);
        println!("🕐 Avg Query Time: {:.2}ms", metrics.avg_query_time_ms);
        println!(
            "🔗 Avg Connection Time: {:.2}ms",
            metrics.avg_connection_time_ms
        );
        println!("📊 95th Percentile: {:.2}ms", metrics.p95_query_time_ms);
        println!("📈 99th Percentile: {:.2}ms", metrics.p99_query_time_ms);
        println!(
            "🎯 Connection Efficiency: {:.1}%",
            metrics.connection_efficiency_percentage
        );

        if metrics.performance_degradation_percentage > 0.0 {
            println!(
                "📉 Performance Degradation: {:.1}%",
                metrics.performance_degradation_percentage
            );
        }

        if !metrics.resource_utilization.is_empty() {
            println!("📊 Resource Utilization:");
            for (key, value) in &metrics.resource_utilization {
                println!("   {}: {:.2}", key, value);
            }
        }

        // Performance assessment
        let performance_score = self.assess_concurrent_performance(metrics);
        println!("🎯 Performance Grade: {}", performance_score);

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }

    /// Assess concurrent performance
    fn assess_concurrent_performance(&self, metrics: &ConcurrentMetrics) -> String {
        let efficiency = metrics.connection_efficiency_percentage;
        let error_rate = metrics.error_rate_percentage;
        let degradation = metrics.performance_degradation_percentage;

        match (efficiency, error_rate, degradation) {
            (eff, err, deg) if eff > 95.0 && err < 1.0 && deg < 10.0 => "EXCELLENT".to_string(),
            (eff, err, deg) if eff > 90.0 && err < 5.0 && deg < 25.0 => "VERY_GOOD".to_string(),
            (eff, err, deg) if eff > 80.0 && err < 10.0 && deg < 50.0 => "GOOD".to_string(),
            (eff, err, deg) if eff > 70.0 && err < 20.0 && deg < 75.0 => "AVERAGE".to_string(),
            _ => "NEEDS_IMPROVEMENT".to_string(),
        }
    }

    /// Generate concurrent connection performance report
    pub fn generate_concurrent_report(&self) -> Result<String, Box<dyn std::error::Error>> {
        let report = json!({
            "concurrent_connection_performance": {
                "test_configuration": {
                    "min_connections": self.config.min_connections,
                    "max_connections": self.config.max_connections,
                    "connection_step": self.config.connection_step,
                    "test_duration_per_level_seconds": self.config.test_duration_per_level_seconds,
                    "queries_per_connection": self.config.queries_per_connection,
                    "connection_pool_sizes": self.config.connection_pool_sizes,
                    "baseline_qps": self.baseline_qps,
                },
                "test_results": self.metrics.iter().map(|m| {
                    json!({
                        "test_name": m.test_name,
                        "concurrent_connections": m.concurrent_connections,
                        "connection_pool_size": m.connection_pool_size,
                        "performance_metrics": {
                            "queries_per_second": m.queries_per_second,
                            "avg_query_time_ms": m.avg_query_time_ms,
                            "avg_connection_time_ms": m.avg_connection_time_ms,
                            "p95_query_time_ms": m.p95_query_time_ms,
                            "p99_query_time_ms": m.p99_query_time_ms,
                            "error_rate_percentage": m.error_rate_percentage,
                            "connection_efficiency_percentage": m.connection_efficiency_percentage,
                            "performance_degradation_percentage": m.performance_degradation_percentage,
                        },
                        "resource_utilization": m.resource_utilization,
                        "performance_grade": self.assess_concurrent_performance(m),
                    })
                }).collect::<Vec<_>>(),
                "analysis": {
                    "optimal_connection_count": self.find_optimal_connection_count(),
                    "optimal_pool_size": self.find_optimal_pool_size(),
                    "max_sustainable_load": self.find_max_sustainable_load(),
                    "performance_bottlenecks": self.identify_bottlenecks(),
                },
                "recommendations": self.generate_concurrent_recommendations(),
            }
        });

        Ok(serde_json::to_string_pretty(&report)?)
    }

    /// Find optimal connection count
    fn find_optimal_connection_count(&self) -> usize {
        self.metrics
            .iter()
            .filter(|m| m.test_name.contains("Connection Scaling"))
            .max_by(|a, b| {
                a.queries_per_second
                    .partial_cmp(&b.queries_per_second)
                    .unwrap()
            })
            .map(|m| m.concurrent_connections)
            .unwrap_or(0)
    }

    /// Find optimal pool size
    fn find_optimal_pool_size(&self) -> usize {
        self.metrics
            .iter()
            .filter(|m| m.test_name.contains("Pool Efficiency"))
            .max_by(|a, b| {
                a.queries_per_second
                    .partial_cmp(&b.queries_per_second)
                    .unwrap()
            })
            .map(|m| m.connection_pool_size)
            .unwrap_or(0)
    }

    /// Find maximum sustainable load
    fn find_max_sustainable_load(&self) -> usize {
        self.metrics
            .iter()
            .filter(|m| m.error_rate_percentage < 5.0 && m.connection_efficiency_percentage > 90.0)
            .max_by(|a, b| a.concurrent_connections.cmp(&b.concurrent_connections))
            .map(|m| m.concurrent_connections)
            .unwrap_or(0)
    }

    /// Identify performance bottlenecks
    fn identify_bottlenecks(&self) -> Vec<String> {
        let mut bottlenecks = Vec::new();

        // Check for connection pool bottlenecks
        let high_contention = self
            .metrics
            .iter()
            .find(|m| m.test_name == "High Contention");

        if let Some(hc) = high_contention {
            if hc.connection_efficiency_percentage < 80.0 {
                bottlenecks.push(
                    "Connection pool size is insufficient for high concurrent loads".to_string(),
                );
            }
        }

        // Check for query timeout issues
        let avg_timeouts = self
            .metrics
            .iter()
            .map(|m| m.query_timeouts as f64 / m.total_queries as f64)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_timeouts > 0.05 {
            bottlenecks.push(
                "High query timeout rate indicates potential database or network bottlenecks"
                    .to_string(),
            );
        }

        // Check for linear performance degradation
        let scaling_metrics: Vec<_> = self
            .metrics
            .iter()
            .filter(|m| m.test_name.contains("Connection Scaling"))
            .collect();

        if scaling_metrics.len() > 2 {
            let first_qps = scaling_metrics[0].queries_per_second;
            let last_qps = scaling_metrics.last().unwrap().queries_per_second;

            if last_qps < first_qps * 0.5 {
                bottlenecks
                    .push("Severe performance degradation under high concurrent load".to_string());
            }
        }

        bottlenecks
    }

    /// Generate optimization recommendations
    fn generate_concurrent_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        let optimal_connections = self.find_optimal_connection_count();
        let optimal_pool_size = self.find_optimal_pool_size();

        if optimal_connections > 0 {
            recommendations.push(format!(
                "Optimal concurrent connection count: {}",
                optimal_connections
            ));
        }

        if optimal_pool_size > 0 {
            recommendations.push(format!(
                "Recommended connection pool size: {}",
                optimal_pool_size
            ));
        }

        // Check for pool size optimization
        if optimal_pool_size > 0 && optimal_connections > 0 {
            let ratio = optimal_pool_size as f64 / optimal_connections as f64;
            if ratio < 1.2 {
                recommendations.push(
                    "Consider increasing connection pool size for better connection availability"
                        .to_string(),
                );
            } else if ratio > 2.0 {
                recommendations.push("Connection pool may be oversized - consider reducing for better resource utilization".to_string());
            }
        }

        // Check error rates
        let avg_error_rate = self
            .metrics
            .iter()
            .map(|m| m.error_rate_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_error_rate > 10.0 {
            recommendations.push(
                "High error rate detected - investigate database capacity and network stability"
                    .to_string(),
            );
        }

        // Check connection efficiency
        let avg_efficiency = self
            .metrics
            .iter()
            .map(|m| m.connection_efficiency_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_efficiency < 90.0 {
            recommendations.push(
                "Low connection efficiency - consider connection retry logic and timeout tuning"
                    .to_string(),
            );
        }

        recommendations
            .push("Monitor connection pool metrics in production environment".to_string());
        recommendations.push("Implement connection pool monitoring and alerting".to_string());

        recommendations
    }

    /// Run all concurrent connection tests
    pub async fn run_all_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting Concurrent Connection Performance Test Suite");
        println!("Configuration: {:?}", self.config);

        self.initialize().await?;

        self.test_connection_scaling().await?;
        self.test_pool_efficiency().await?;
        self.test_sustained_load().await?;
        self.test_resource_contention().await?;

        // Generate and save report
        let report = self.generate_concurrent_report()?;
        std::fs::write(
            "mysql_concurrent_connection_performance_report.json",
            &report,
        )?;

        println!("📄 Concurrent connection performance report saved");

        // Print overall summary
        let optimal_connections = self.find_optimal_connection_count();
        let optimal_pool_size = self.find_optimal_pool_size();
        let max_sustainable = self.find_max_sustainable_load();

        println!("🎯 Optimal Connection Count: {}", optimal_connections);
        println!("🏊 Optimal Pool Size: {}", optimal_pool_size);
        println!("📈 Max Sustainable Load: {} connections", max_sustainable);

        Ok(())
    }
}

// Helper struct for test results
#[derive(Debug, Clone)]
struct TestResults {
    pub total_queries: usize,
    pub successful_queries: usize,
    pub failed_queries: usize,
    pub connection_failures: usize,
    pub query_timeouts: usize,
    pub duration_seconds: f64,
    pub queries_per_second: f64,
    pub query_times: Vec<u64>,
    pub connection_times: Vec<u64>,
}

impl TestResults {
    pub fn new() -> Self {
        Self {
            total_queries: 0,
            successful_queries: 0,
            failed_queries: 0,
            connection_failures: 0,
            query_timeouts: 0,
            duration_seconds: 0.0,
            queries_per_second: 0.0,
            query_times: Vec::new(),
            connection_times: Vec::new(),
        }
    }

    pub fn merge(&mut self, other: TestResults) {
        self.total_queries += other.total_queries;
        self.successful_queries += other.successful_queries;
        self.failed_queries += other.failed_queries;
        self.connection_failures += other.connection_failures;
        self.query_timeouts += other.query_timeouts;
        self.query_times.extend(other.query_times);
        self.connection_times.extend(other.connection_times);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConcurrentConnectionConfig {
        min_connections: 5,
        max_connections: 75,
        connection_step: 10,
        test_duration_per_level_seconds: 20,
        queries_per_connection: 25,
        connection_pool_sizes: vec![15, 30, 50, 75],
        query_timeout_seconds: 30,
        ramp_up_duration_seconds: 40,
        sustained_load_duration_seconds: 60,
    };

    let mut test = ConcurrentConnectionTest::new(config);
    test.run_all_tests().await?;

    Ok(())
}
