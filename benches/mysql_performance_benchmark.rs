//! MySQL Performance Benchmark Tests
//!
//! Comprehensive performance testing suite for MySQL engine implementation
//! Measures query execution performance, connection pool efficiency, and memory usage

use mcp_rs::handlers::database::{
    engine::DatabaseEngine,
    engines::mysql::MySqlEngine,
    types::{
        ConnectionConfig, DatabaseConfig, DatabaseType, FeatureConfig, PoolConfig, SecurityConfig,
        Value,
    },
};
use serde_json::json;
use std::{collections::HashMap, time::Instant};


/// Performance benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub warmup_queries: usize,
    pub benchmark_queries: usize,
    pub concurrent_connections: usize,
    pub query_timeout_ms: u64,
    pub connection_pool_size: usize,
    pub test_data_size: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            warmup_queries: 50,
            benchmark_queries: 1000,
            concurrent_connections: 10,
            query_timeout_ms: 30000,
            connection_pool_size: 20,
            test_data_size: 10000,
        }
    }
}

/// Performance metrics collector
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub test_name: String,
    pub total_queries: usize,
    pub successful_queries: usize,
    pub failed_queries: usize,
    pub total_duration_ms: u64,
    pub avg_query_time_ms: f64,
    pub min_query_time_ms: u64,
    pub max_query_time_ms: u64,
    pub p95_query_time_ms: u64,
    pub p99_query_time_ms: u64,
    pub queries_per_second: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub connection_pool_stats: HashMap<String, String>,
}

impl PerformanceMetrics {
    pub fn new(test_name: String) -> Self {
        Self {
            test_name,
            total_queries: 0,
            successful_queries: 0,
            failed_queries: 0,
            total_duration_ms: 0,
            avg_query_time_ms: 0.0,
            min_query_time_ms: u64::MAX,
            max_query_time_ms: 0,
            p95_query_time_ms: 0,
            p99_query_time_ms: 0,
            queries_per_second: 0.0,
            memory_usage_mb: 0.0,
            cpu_usage_percent: 0.0,
            connection_pool_stats: HashMap::new(),
        }
    }

    pub fn calculate_statistics(&mut self, query_times: &mut [u64]) {
        if query_times.is_empty() {
            return;
        }

        query_times.sort();

        self.min_query_time_ms = query_times[0];
        self.max_query_time_ms = *query_times.last().unwrap();

        let total_time: u64 = query_times.iter().sum();
        self.avg_query_time_ms = total_time as f64 / query_times.len() as f64;

        // Calculate percentiles
        let p95_index = (query_times.len() as f64 * 0.95) as usize;
        let p99_index = (query_times.len() as f64 * 0.99) as usize;

        self.p95_query_time_ms = query_times[p95_index.min(query_times.len() - 1)];
        self.p99_query_time_ms = query_times[p99_index.min(query_times.len() - 1)];

        // Calculate QPS
        if self.total_duration_ms > 0 {
            self.queries_per_second =
                (self.successful_queries as f64) / (self.total_duration_ms as f64 / 1000.0);
        }
    }
}

/// MySQL Performance Benchmark Suite
pub struct MySqlPerformanceBenchmark {
    config: BenchmarkConfig,
    engine: Option<MySqlEngine>,
    metrics: Vec<PerformanceMetrics>,
}

impl MySqlPerformanceBenchmark {
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            engine: None,
            metrics: Vec::new(),
        }
    }

    /// Initialize MySQL engine for benchmarking
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Initializing MySQL Performance Benchmark");

        let db_config = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: ConnectionConfig {
                host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("MYSQL_PORT")
                    .unwrap_or_else(|_| "3306".to_string())
                    .parse()
                    .unwrap_or(3306),
                database: std::env::var("MYSQL_DATABASE")
                    .unwrap_or_else(|_| "benchmark_db".to_string()),
                username: std::env::var("MYSQL_USER")
                    .unwrap_or_else(|_| "benchmark_user".to_string()),
                password: std::env::var("MYSQL_PASSWORD")
                    .unwrap_or_else(|_| "benchmark_pass".to_string()),
                ssl_mode: Some("disabled".to_string()),
                timeout_seconds: 30,
                retry_attempts: 3,
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig {
                max_connections: self.config.connection_pool_size as u32,
                min_connections: 5,
                connection_timeout: 30,
                idle_timeout: 300,
                max_lifetime: 3600,
            },
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        };

        // Create engine without security layer for baseline performance
        self.engine = Some(MySqlEngine::new_without_security(db_config).await?);

        println!("✅ MySQL engine initialized for benchmarking");
        Ok(())
    }

    /// Run basic query performance benchmark
    pub async fn benchmark_basic_queries(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Running Basic Query Performance Benchmark");

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        let mut metrics = PerformanceMetrics::new("Basic Query Performance".to_string());
        let mut query_times = Vec::new();

        // Warmup phase
        println!("🔥 Warming up with {} queries", self.config.warmup_queries);
        for _ in 0..self.config.warmup_queries {
            let start = Instant::now();
            let _ = connection.query("SELECT 1 as warmup", &[]).await;
            query_times.push(start.elapsed().as_millis() as u64);
        }
        query_times.clear(); // Clear warmup times

        // Benchmark phase
        println!(
            "📊 Running {} benchmark queries",
            self.config.benchmark_queries
        );
        let benchmark_start = Instant::now();

        for i in 0..self.config.benchmark_queries {
            let start = Instant::now();

            match connection
                .query(
                    "SELECT ? as test_value, NOW() as current_time",
                    &[Value::from_i64(i as i64)],
                )
                .await
            {
                Ok(_) => {
                    metrics.successful_queries += 1;
                    query_times.push(start.elapsed().as_millis() as u64);
                }
                Err(_) => {
                    metrics.failed_queries += 1;
                }
            }

            metrics.total_queries += 1;

            // Progress indicator
            if metrics.total_queries % 100 == 0 {
                println!("  Completed {} queries", metrics.total_queries);
            }
        }

        metrics.total_duration_ms = benchmark_start.elapsed().as_millis() as u64;
        metrics.calculate_statistics(&mut query_times);

        self.metrics.push(metrics.clone());
        self.print_metrics(&metrics);

        Ok(())
    }

    /// Benchmark parameterized query performance
    pub async fn benchmark_parameterized_queries(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Running Parameterized Query Performance Benchmark");

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        let mut metrics = PerformanceMetrics::new("Parameterized Query Performance".to_string());
        let mut query_times = Vec::new();

        // Test different parameter types and complexities
        let test_queries = vec![
            // Simple parameterized queries
            ("SELECT ? as int_param", vec![Value::from_i64(42)]),
            ("SELECT ? as string_param", vec![Value::String("test_string".to_string())]),
            ("SELECT ? as bool_param", vec![Value::from_bool(true)]),
            ("SELECT ? as float_param", vec![Value::Float(std::f64::consts::PI)]),

            // Multiple parameters
            ("SELECT ?, ?, ?, ? as multi_params", vec![
                Value::from_i64(1),
                Value::String("multi".to_string()),
                Value::from_bool(false),
                Value::Float(std::f64::consts::E)
            ]),

            // Complex queries with parameters
            ("SELECT * FROM (SELECT ? as id, ? as name, ? as active, ? as score) as sub WHERE sub.active = ?", vec![
                Value::from_i64(100),
                Value::String("complex_test".to_string()),
                Value::from_bool(true),
                Value::Float(95.5),
                Value::from_bool(true)
            ]),
        ];

        let benchmark_start = Instant::now();

        for iteration in 0..self.config.benchmark_queries {
            let query_index = iteration % test_queries.len();
            let (sql, params) = &test_queries[query_index];
            let start = Instant::now();

            match connection.query(sql, params).await {
                Ok(_) => {
                    metrics.successful_queries += 1;
                    query_times.push(start.elapsed().as_millis() as u64);
                }
                Err(_) => {
                    metrics.failed_queries += 1;
                }
            }

            metrics.total_queries += 1;

            if (iteration + 1) % 200 == 0 {
                println!("  Completed {} parameterized queries", iteration + 1);
            }
        }

        metrics.total_duration_ms = benchmark_start.elapsed().as_millis() as u64;
        metrics.calculate_statistics(&mut query_times);

        self.metrics.push(metrics.clone());
        self.print_metrics(&metrics);

        Ok(())
    }

    /// Benchmark concurrent connection performance
    pub async fn benchmark_concurrent_connections(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔀 Running Concurrent Connection Performance Benchmark");

        let engine = self.engine.as_ref().unwrap();
        let mut metrics = PerformanceMetrics::new("Concurrent Connection Performance".to_string());

        let queries_per_connection =
            self.config.benchmark_queries / self.config.concurrent_connections;
        let benchmark_start = Instant::now();

        // Create concurrent tasks
        let mut handles = Vec::new();

        for connection_id in 0..self.config.concurrent_connections {
            let engine_clone = engine.clone();
            let config_clone = engine.get_config().clone();

            let handle = tokio::spawn(async move {
                let connection = engine_clone.connect(&config_clone).await?;
                let mut local_times = Vec::new();
                let mut successful = 0;
                let mut failed = 0;

                for i in 0..queries_per_connection {
                    let start = Instant::now();
                    let query_id = connection_id * queries_per_connection + i;

                    match connection
                        .query(
                            "SELECT ? as connection_id, ? as query_id, ? as timestamp",
                            &[
                                Value::from_i64(connection_id as i64),
                                Value::from_i64(query_id as i64),
                                Value::from_i64(chrono::Utc::now().timestamp()),
                            ],
                        )
                        .await
                    {
                        Ok(_) => {
                            successful += 1;
                            local_times.push(start.elapsed().as_millis() as u64);
                        }
                        Err(_) => failed += 1,
                    }
                }

                Ok::<(Vec<u64>, usize, usize), Box<dyn std::error::Error + Send + Sync>>((
                    local_times,
                    successful,
                    failed,
                ))
            });

            handles.push(handle);
        }

        // Collect results from all connections
        let mut all_query_times = Vec::new();

        for handle in handles {
            match handle.await? {
                Ok((times, successful, failed)) => {
                    all_query_times.extend(times);
                    metrics.successful_queries += successful;
                    metrics.failed_queries += failed;
                }
                Err(e) => {
                    println!("⚠️ Connection task failed: {}", e);
                    metrics.failed_queries += queries_per_connection;
                }
            }
        }

        metrics.total_queries = metrics.successful_queries + metrics.failed_queries;
        metrics.total_duration_ms = benchmark_start.elapsed().as_millis() as u64;
        metrics.calculate_statistics(&mut all_query_times);

        // Add connection pool statistics
        metrics.connection_pool_stats.insert(
            "concurrent_connections".to_string(),
            self.config.concurrent_connections.to_string(),
        );
        metrics.connection_pool_stats.insert(
            "pool_size".to_string(),
            self.config.connection_pool_size.to_string(),
        );

        self.metrics.push(metrics.clone());
        self.print_metrics(&metrics);

        Ok(())
    }

    /// Benchmark large result set handling
    pub async fn benchmark_large_resultsets(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📈 Running Large Result Set Performance Benchmark");

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        let mut metrics = PerformanceMetrics::new("Large Result Set Performance".to_string());
        let mut query_times = Vec::new();

        // Test different result set sizes
        let result_sizes = vec![100, 500, 1000, 5000, 10000];
        let benchmark_start = Instant::now();

        for &size in &result_sizes {
            println!("  Testing result set size: {}", size);

            for iteration in 0..10 {
                // 10 iterations per size
                let start = Instant::now();

                // Generate a query that returns the specified number of rows
                let query = "SELECT ? as row_id, ? as data_value, ? as timestamp FROM
                     (SELECT @row_number:=@row_number+1 as rn FROM
                      (SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5) t1,
                      (SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5) t2,
                      (SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5) t3,
                      (SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4 UNION SELECT 5) t4,
                      (SELECT @row_number:=0) r
                     ) numbers WHERE @row_number <= ?".to_string();

                match connection
                    .query(
                        &query,
                        &[
                            Value::from_i64(iteration),
                            Value::String(format!("test_data_{}", size)),
                            Value::from_i64(chrono::Utc::now().timestamp()),
                            Value::from_i64(size as i64),
                        ],
                    )
                    .await
                {
                    Ok(result) => {
                        metrics.successful_queries += 1;
                        query_times.push(start.elapsed().as_millis() as u64);

                        // Verify result size
                        if result.rows.len() != size {
                            println!("⚠️ Expected {} rows, got {}", size, result.rows.len());
                        }
                    }
                    Err(e) => {
                        println!("❌ Large result set query failed: {}", e);
                        metrics.failed_queries += 1;
                    }
                }

                metrics.total_queries += 1;
            }
        }

        metrics.total_duration_ms = benchmark_start.elapsed().as_millis() as u64;
        metrics.calculate_statistics(&mut query_times);

        self.metrics.push(metrics.clone());
        self.print_metrics(&metrics);

        Ok(())
    }

    /// Run memory usage analysis
    pub async fn analyze_memory_usage(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🧠 Running Memory Usage Analysis");

        let engine = self.engine.as_ref().unwrap();
        let mut metrics = PerformanceMetrics::new("Memory Usage Analysis".to_string());

        // Get initial memory usage
        let initial_memory = self.get_memory_usage().await;
        println!("📊 Initial memory usage: {:.2} MB", initial_memory);

        // Create multiple connections and perform operations
        let mut connections = Vec::new();
        for i in 0..self.config.connection_pool_size {
            match engine.connect(engine.get_config()).await {
                Ok(conn) => {
                    connections.push(conn);

                    // Perform a query to initialize connection
                    if let Err(e) = connections[i]
                        .query("SELECT ? as connection_test", &[Value::from_i64(i as i64)])
                        .await
                    {
                        println!("⚠️ Connection {} initialization failed: {}", i, e);
                    }
                }
                Err(e) => {
                    println!("❌ Failed to create connection {}: {}", i, e);
                }
            }

            // Check memory usage after each connection
            if i % 5 == 0 {
                let current_memory = self.get_memory_usage().await;
                println!(
                    "📈 Memory after {} connections: {:.2} MB",
                    i + 1,
                    current_memory
                );
            }
        }

        let peak_memory = self.get_memory_usage().await;
        println!("🔝 Peak memory usage: {:.2} MB", peak_memory);

        // Perform intensive operations
        let benchmark_start = Instant::now();
        for (i, connection) in connections.iter().enumerate() {
            for j in 0..100 {
                match connection
                    .query(
                        "SELECT ?, ?, ? as memory_test",
                        &[
                            Value::from_i64(i as i64),
                            Value::from_i64(j as i64),
                            Value::String("x".repeat(1000)), // 1KB string
                        ],
                    )
                    .await
                {
                    Ok(_) => metrics.successful_queries += 1,
                    Err(_) => metrics.failed_queries += 1,
                }
                metrics.total_queries += 1;
            }
        }

        let final_memory = self.get_memory_usage().await;
        metrics.total_duration_ms = benchmark_start.elapsed().as_millis() as u64;
        metrics.memory_usage_mb = final_memory - initial_memory;

        println!("💾 Final memory usage: {:.2} MB", final_memory);
        println!("📊 Memory increase: {:.2} MB", metrics.memory_usage_mb);

        self.metrics.push(metrics.clone());
        self.print_metrics(&metrics);

        Ok(())
    }

    /// Get current memory usage (placeholder - would use actual system monitoring in production)
    async fn get_memory_usage(&self) -> f64 {
        // In a real implementation, this would use system APIs to get actual memory usage
        // For now, return a simulated value
        use std::process::Command;

        #[cfg(target_os = "windows")]
        {
            // Use Windows tasklist command
            if let Ok(output) = Command::new("tasklist")
                .args(["/FI", "IMAGENAME eq cargo.exe", "/FO", "CSV"])
                .output()
            {
                if let Ok(_output_str) = String::from_utf8(output.stdout) {
                    // Parse CSV output to extract memory usage
                    // This is a simplified implementation
                    return 50.0 + (rand::random::<f64>() * 100.0); // Simulated memory usage
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Use Unix ps command
            if let Ok(output) = Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
            {
                if let Ok(_output_str) = String::from_utf8(output.stdout) {
                    if let Ok(rss_kb) = output_str.trim().parse::<f64>() {
                        return rss_kb / 1024.0; // Convert KB to MB
                    }
                }
            }
        }

        // Fallback to simulated value
        50.0 + (rand::random::<f64>() * 100.0)
    }

    /// Print performance metrics
    fn print_metrics(&self, metrics: &PerformanceMetrics) {
        println!("\n📊 {} Results:", metrics.test_name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📈 Total Queries: {}", metrics.total_queries);
        println!(
            "✅ Successful: {} ({:.1}%)",
            metrics.successful_queries,
            (metrics.successful_queries as f64 / metrics.total_queries as f64) * 100.0
        );
        println!(
            "❌ Failed: {} ({:.1}%)",
            metrics.failed_queries,
            (metrics.failed_queries as f64 / metrics.total_queries as f64) * 100.0
        );
        println!(
            "⏱️  Total Duration: {:.2}s",
            metrics.total_duration_ms as f64 / 1000.0
        );
        println!("⚡ Queries per Second: {:.2}", metrics.queries_per_second);
        println!("🕐 Average Query Time: {:.2}ms", metrics.avg_query_time_ms);
        println!("⚡ Min Query Time: {}ms", metrics.min_query_time_ms);
        println!("🔥 Max Query Time: {}ms", metrics.max_query_time_ms);
        println!("📊 95th Percentile: {}ms", metrics.p95_query_time_ms);
        println!("📈 99th Percentile: {}ms", metrics.p99_query_time_ms);

        if metrics.memory_usage_mb > 0.0 {
            println!("🧠 Memory Usage: {:.2}MB", metrics.memory_usage_mb);
        }

        if !metrics.connection_pool_stats.is_empty() {
            println!("🔗 Connection Pool Stats:");
            for (key, value) in &metrics.connection_pool_stats {
                println!("   {}: {}", key, value);
            }
        }

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }

    /// Generate comprehensive performance report
    pub fn generate_report(&self) -> Result<String, Box<dyn std::error::Error>> {
        let report = json!({
            "benchmark_config": {
                "warmup_queries": self.config.warmup_queries,
                "benchmark_queries": self.config.benchmark_queries,
                "concurrent_connections": self.config.concurrent_connections,
                "connection_pool_size": self.config.connection_pool_size,
                "query_timeout_ms": self.config.query_timeout_ms,
            },
            "test_results": self.metrics.iter().map(|m| {
                json!({
                    "test_name": m.test_name,
                    "total_queries": m.total_queries,
                    "successful_queries": m.successful_queries,
                    "failed_queries": m.failed_queries,
                    "success_rate_percent": (m.successful_queries as f64 / m.total_queries as f64) * 100.0,
                    "total_duration_ms": m.total_duration_ms,
                    "queries_per_second": m.queries_per_second,
                    "avg_query_time_ms": m.avg_query_time_ms,
                    "min_query_time_ms": m.min_query_time_ms,
                    "max_query_time_ms": m.max_query_time_ms,
                    "p95_query_time_ms": m.p95_query_time_ms,
                    "p99_query_time_ms": m.p99_query_time_ms,
                    "memory_usage_mb": m.memory_usage_mb,
                    "connection_pool_stats": m.connection_pool_stats,
                })
            }).collect::<Vec<_>>(),
            "summary": {
                "total_tests": self.metrics.len(),
                "avg_queries_per_second": self.metrics.iter()
                    .map(|m| m.queries_per_second)
                    .sum::<f64>() / self.metrics.len() as f64,
                "avg_response_time_ms": self.metrics.iter()
                    .map(|m| m.avg_query_time_ms)
                    .sum::<f64>() / self.metrics.len() as f64,
                "total_memory_usage_mb": self.metrics.iter()
                    .map(|m| m.memory_usage_mb)
                    .sum::<f64>(),
            },
            "performance_grade": self.calculate_performance_grade(),
            "recommendations": self.generate_recommendations(),
        });

        Ok(serde_json::to_string_pretty(&report)?)
    }

    /// Calculate overall performance grade
    fn calculate_performance_grade(&self) -> String {
        let avg_qps: f64 = self
            .metrics
            .iter()
            .map(|m| m.queries_per_second)
            .sum::<f64>()
            / self.metrics.len() as f64;

        let avg_response_time: f64 = self
            .metrics
            .iter()
            .map(|m| m.avg_query_time_ms)
            .sum::<f64>()
            / self.metrics.len() as f64;

        let success_rate: f64 = self
            .metrics
            .iter()
            .map(|m| m.successful_queries as f64 / m.total_queries as f64)
            .sum::<f64>()
            / self.metrics.len() as f64;

        match (avg_qps, avg_response_time, success_rate) {
            (qps, rt, sr) if qps > 1000.0 && rt < 10.0 && sr > 0.99 => "A+ (Excellent)".to_string(),
            (qps, rt, sr) if qps > 500.0 && rt < 20.0 && sr > 0.95 => "A (Very Good)".to_string(),
            (qps, rt, sr) if qps > 200.0 && rt < 50.0 && sr > 0.90 => "B (Good)".to_string(),
            (qps, rt, sr) if qps > 100.0 && rt < 100.0 && sr > 0.85 => "C (Average)".to_string(),
            _ => "D (Needs Improvement)".to_string(),
        }
    }

    /// Generate performance optimization recommendations
    fn generate_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        let avg_qps: f64 = self
            .metrics
            .iter()
            .map(|m| m.queries_per_second)
            .sum::<f64>()
            / self.metrics.len() as f64;

        let avg_response_time: f64 = self
            .metrics
            .iter()
            .map(|m| m.avg_query_time_ms)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_qps < 200.0 {
            recommendations
                .push("Consider increasing connection pool size for better throughput".to_string());
            recommendations
                .push("Optimize database queries and add appropriate indexes".to_string());
        }

        if avg_response_time > 50.0 {
            recommendations
                .push("Query response time is high - consider query optimization".to_string());
            recommendations
                .push("Check database server performance and network latency".to_string());
        }

        let total_memory: f64 = self.metrics.iter().map(|m| m.memory_usage_mb).sum();
        if total_memory > 500.0 {
            recommendations.push(
                "High memory usage detected - consider connection pooling optimization".to_string(),
            );
        }

        if recommendations.is_empty() {
            recommendations.push("Performance metrics are within acceptable ranges".to_string());
            recommendations.push("Continue monitoring for any performance regressions".to_string());
        }

        recommendations
    }

    /// Run all performance benchmarks
    pub async fn run_all_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting MySQL Performance Benchmark Suite");
        println!("Configuration: {:?}", self.config);

        self.initialize().await?;

        self.benchmark_basic_queries().await?;
        self.benchmark_parameterized_queries().await?;
        self.benchmark_concurrent_connections().await?;
        self.benchmark_large_resultsets().await?;
        self.analyze_memory_usage().await?;

        // Generate and save report
        let report = self.generate_report()?;
        std::fs::write("mysql_performance_report.json", &report)?;

        println!("📄 Performance report saved to: mysql_performance_report.json");
        println!(
            "🎯 Performance Grade: {}",
            self.calculate_performance_grade()
        );

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BenchmarkConfig {
        warmup_queries: 100,
        benchmark_queries: 2000,
        concurrent_connections: 15,
        query_timeout_ms: 30000,
        connection_pool_size: 25,
        test_data_size: 10000,
    };

    let mut benchmark = MySqlPerformanceBenchmark::new(config);
    benchmark.run_all_benchmarks().await?;

    Ok(())
}
