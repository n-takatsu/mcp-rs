//! Parameterized Query Performance Tests
//!
//! Comprehensive performance testing for parameterized queries under high load
//! Tests prepared statement efficiency and parameter conversion performance

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
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{self, sync::Semaphore};

/// Parameterized query performance test configuration
#[derive(Debug, Clone)]
pub struct ParamQueryConfig {
    pub test_duration_seconds: u64,
    pub max_concurrent_queries: usize,
    pub parameter_variations: usize,
    pub query_complexity_levels: usize,
    pub large_parameter_size_kb: usize,
    pub batch_sizes: Vec<usize>,
    pub warmup_duration_seconds: u64,
}

impl Default for ParamQueryConfig {
    fn default() -> Self {
        Self {
            test_duration_seconds: 60,
            max_concurrent_queries: 50,
            parameter_variations: 1000,
            query_complexity_levels: 5,
            large_parameter_size_kb: 64,
            batch_sizes: vec![1, 10, 50, 100],
            warmup_duration_seconds: 10,
        }
    }
}

/// Parameter performance metrics
#[derive(Debug, Clone)]
pub struct ParamPerformanceMetrics {
    pub test_name: String,
    pub total_queries: usize,
    pub successful_queries: usize,
    pub failed_queries: usize,
    pub total_duration_seconds: f64,
    pub queries_per_second: f64,
    pub avg_query_time_ms: f64,
    pub avg_param_conversion_time_us: f64,
    pub p95_query_time_ms: f64,
    pub p99_query_time_ms: f64,
    pub parameter_sizes: HashMap<String, usize>,
    pub memory_usage_mb: f64,
    pub error_rate_percentage: f64,
}

impl ParamPerformanceMetrics {
    pub fn new(test_name: String) -> Self {
        Self {
            test_name,
            total_queries: 0,
            successful_queries: 0,
            failed_queries: 0,
            total_duration_seconds: 0.0,
            queries_per_second: 0.0,
            avg_query_time_ms: 0.0,
            avg_param_conversion_time_us: 0.0,
            p95_query_time_ms: 0.0,
            p99_query_time_ms: 0.0,
            parameter_sizes: HashMap::new(),
            memory_usage_mb: 0.0,
            error_rate_percentage: 0.0,
        }
    }

    pub fn calculate_statistics(&mut self, query_times: &mut [u64]) {
        if query_times.is_empty() {
            return;
        }

        query_times.sort();

        let total_time: u64 = query_times.iter().sum();
        self.avg_query_time_ms = total_time as f64 / query_times.len() as f64;

        // Calculate percentiles
        let p95_index = (query_times.len() as f64 * 0.95) as usize;
        let p99_index = (query_times.len() as f64 * 0.99) as usize;

        self.p95_query_time_ms = query_times[p95_index.min(query_times.len() - 1)] as f64;
        self.p99_query_time_ms = query_times[p99_index.min(query_times.len() - 1)] as f64;

        // Calculate QPS and error rate
        if self.total_duration_seconds > 0.0 {
            self.queries_per_second = self.successful_queries as f64 / self.total_duration_seconds;
        }

        if self.total_queries > 0 {
            self.error_rate_percentage =
                (self.failed_queries as f64 / self.total_queries as f64) * 100.0;
        }
    }
}

/// Test query types with different parameter complexities
#[derive(Debug, Clone)]
pub enum QueryType {
    Simple,         // Single parameter
    Multiple,       // Multiple parameters
    Complex,        // Complex nested parameters
    LargeParameter, // Large parameter values
    BatchInsert,    // Batch operations
}

/// Query template with parameter information
#[derive(Debug, Clone)]
pub struct QueryTemplate {
    pub query_type: QueryType,
    pub sql: String,
    pub param_count: usize,
    pub complexity_score: usize,
    pub description: String,
}

/// Parameterized Query Performance Test Suite
pub struct ParamQueryPerformanceTest {
    config: ParamQueryConfig,
    engine: Option<MySqlEngine>,
    query_templates: Vec<QueryTemplate>,
    metrics: Vec<ParamPerformanceMetrics>,
}

impl ParamQueryPerformanceTest {
    pub fn new(config: ParamQueryConfig) -> Self {
        let mut test = Self {
            config,
            engine: None,
            query_templates: Vec::new(),
            metrics: Vec::new(),
        };

        test.initialize_query_templates();
        test
    }

    /// Initialize various query templates for testing
    fn initialize_query_templates(&mut self) {
        self.query_templates = vec![
            QueryTemplate {
                query_type: QueryType::Simple,
                sql: "SELECT ? as simple_param".to_string(),
                param_count: 1,
                complexity_score: 1,
                description: "Simple single parameter query".to_string(),
            },
            QueryTemplate {
                query_type: QueryType::Multiple,
                sql: "SELECT ?, ?, ?, ?, ? as multi_params".to_string(),
                param_count: 5,
                complexity_score: 2,
                description: "Multiple parameter query".to_string(),
            },
            QueryTemplate {
                query_type: QueryType::Complex,
                sql: "SELECT * FROM (SELECT ?, ?, ? as data) t WHERE t.data IN (?, ?, ?) AND t.data NOT IN (?, ?)".to_string(),
                param_count: 8,
                complexity_score: 4,
                description: "Complex query with multiple parameter conditions".to_string(),
            },
            QueryTemplate {
                query_type: QueryType::LargeParameter,
                sql: "SELECT LENGTH(?) as param_length, ? as large_data".to_string(),
                param_count: 2,
                complexity_score: 3,
                description: "Query with large parameter values".to_string(),
            },
            QueryTemplate {
                query_type: QueryType::BatchInsert,
                sql: "SELECT * FROM (SELECT ? as id, ? as name, ? as email, ? as data UNION ALL SELECT ?, ?, ?, ?) as batch_data".to_string(),
                param_count: 8,
                complexity_score: 5,
                description: "Batch-like operation with multiple parameter sets".to_string(),
            },
        ];
    }

    /// Initialize MySQL engine for parameterized query testing
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Initializing Parameterized Query Performance Test");

        let db_config = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: ConnectionConfig {
                host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("MYSQL_PORT")
                    .unwrap_or_else(|_| "3306".to_string())
                    .parse()
                    .unwrap_or(3306),
                database: std::env::var("MYSQL_DATABASE")
                    .unwrap_or_else(|_| "param_test_db".to_string()),
                username: std::env::var("MYSQL_USER").unwrap_or_else(|_| "param_user".to_string()),
                password: std::env::var("MYSQL_PASSWORD")
                    .unwrap_or_else(|_| "param_pass".to_string()),
                ssl_mode: Some("disabled".to_string()),
                timeout_seconds: 30,
                retry_attempts: 3,
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig {
                max_connections: (self.config.max_concurrent_queries + 10) as u32,
                min_connections: 5,
                connection_timeout: 30,
                idle_timeout: 300,
                max_lifetime: 3600,
            },
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        };

        self.engine = Some(MySqlEngine::new_without_security(db_config).await?);

        println!("✅ MySQL engine initialized for parameterized query testing");
        Ok(())
    }

    /// Test simple parameter performance
    pub async fn test_simple_parameters(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Testing Simple Parameter Performance");

        let template = &self.query_templates[0]; // Simple query template
        let mut metrics = ParamPerformanceMetrics::new("Simple Parameters".to_string());
        let mut query_times = Vec::new();

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        // Warmup
        println!(
            "🔥 Warming up for {} seconds",
            self.config.warmup_duration_seconds
        );
        let warmup_end = Instant::now() + Duration::from_secs(self.config.warmup_duration_seconds);
        while Instant::now() < warmup_end {
            let _ = connection.query(&template.sql, &[Value::from_i64(1)]).await;
        }

        // Performance test
        println!(
            "📊 Running simple parameter test for {} seconds",
            self.config.test_duration_seconds
        );
        let test_start = Instant::now();
        let test_end = test_start + Duration::from_secs(self.config.test_duration_seconds);

        let mut iteration = 0;
        while Instant::now() < test_end {
            let query_start = Instant::now();

            // Generate varied parameter values
            let param_value = match iteration % 5 {
                0 => Value::from_i64(iteration as i64),
                1 => Value::String(format!("test_string_{}", iteration)),
                2 => Value::from_bool(iteration % 2 == 0),
                3 => Value::Float((iteration as f64) * std::f64::consts::PI),
                _ => Value::String("NULL".to_string()),
            };

            match connection.query(&template.sql, &[param_value]).await {
                Ok(_) => {
                    metrics.successful_queries += 1;
                    query_times.push(query_start.elapsed().as_millis() as u64);
                }
                Err(_) => metrics.failed_queries += 1,
            }

            metrics.total_queries += 1;
            iteration += 1;
        }

        metrics.total_duration_seconds = test_start.elapsed().as_secs_f64();
        metrics.calculate_statistics(&mut query_times);

        self.metrics.push(metrics.clone());
        self.print_param_metrics(&metrics);

        Ok(())
    }

    /// Test multiple parameter performance
    pub async fn test_multiple_parameters(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Testing Multiple Parameter Performance");

        let template = &self.query_templates[1]; // Multiple parameter template
        let mut metrics = ParamPerformanceMetrics::new("Multiple Parameters".to_string());
        let mut query_times = Vec::new();

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        let test_start = Instant::now();
        let test_end = test_start + Duration::from_secs(self.config.test_duration_seconds);

        let mut iteration = 0;
        while Instant::now() < test_end {
            let query_start = Instant::now();

            // Generate multiple varied parameters
            let params = vec![
                Value::from_i64(iteration as i64),
                Value::String(format!("multi_test_{}", iteration)),
                Value::from_bool(true),
                Value::Float(iteration as f64 * std::f64::consts::E),
                Value::String(format!("param_5_{}", iteration % 100)),
            ];

            match connection.query(&template.sql, &params).await {
                Ok(_) => {
                    metrics.successful_queries += 1;
                    query_times.push(query_start.elapsed().as_millis() as u64);
                }
                Err(_) => metrics.failed_queries += 1,
            }

            metrics.total_queries += 1;
            iteration += 1;
        }

        metrics.total_duration_seconds = test_start.elapsed().as_secs_f64();
        metrics.calculate_statistics(&mut query_times);

        // Track parameter sizes
        metrics
            .parameter_sizes
            .insert("param_count".to_string(), template.param_count);

        self.metrics.push(metrics.clone());
        self.print_param_metrics(&metrics);

        Ok(())
    }

    /// Test large parameter performance
    pub async fn test_large_parameters(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📈 Testing Large Parameter Performance");

        let template = &self.query_templates[3]; // Large parameter template
        let mut metrics = ParamPerformanceMetrics::new("Large Parameters".to_string());
        let mut query_times = Vec::new();

        let engine = self.engine.as_ref().unwrap();
        let connection = engine.connect(engine.get_config()).await?;

        // Generate large parameter data
        let large_data_sizes = [1, 4, 16, 64]; // KB sizes

        let test_start = Instant::now();
        let test_end = test_start + Duration::from_secs(self.config.test_duration_seconds);

        let mut iteration = 0;
        while Instant::now() < test_end {
            let query_start = Instant::now();

            let size_kb = large_data_sizes[iteration % large_data_sizes.len()];
            let large_data = "x".repeat(size_kb * 1024);

            let params = vec![
                Value::String(large_data.clone()),
                Value::String(format!("metadata_{}", iteration)),
            ];

            match connection.query(&template.sql, &params).await {
                Ok(_) => {
                    metrics.successful_queries += 1;
                    query_times.push(query_start.elapsed().as_millis() as u64);
                }
                Err(_) => metrics.failed_queries += 1,
            }

            metrics.total_queries += 1;
            iteration += 1;
        }

        metrics.total_duration_seconds = test_start.elapsed().as_secs_f64();
        metrics.calculate_statistics(&mut query_times);

        // Track parameter sizes
        metrics.parameter_sizes.insert(
            "max_param_size_kb".to_string(),
            self.config.large_parameter_size_kb,
        );

        self.metrics.push(metrics.clone());
        self.print_param_metrics(&metrics);

        Ok(())
    }

    /// Test concurrent parameterized queries
    pub async fn test_concurrent_parameters(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔀 Testing Concurrent Parameterized Query Performance");

        let mut metrics = ParamPerformanceMetrics::new("Concurrent Parameters".to_string());
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_queries));

        let test_start = Instant::now();
        let test_end = test_start + Duration::from_secs(self.config.test_duration_seconds);

        let mut handles = Vec::new();
        let mut task_id = 0;

        while Instant::now() < test_end {
            let semaphore_clone = semaphore.clone();
            let engine_clone = self.engine.as_ref().unwrap().clone();
            let config_clone = self.engine.as_ref().unwrap().get_config().clone();
            let template_clone = self.query_templates[1].clone(); // Multiple parameter template
            let current_task_id = task_id;

            let handle = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await.unwrap();

                let connection = match engine_clone.connect(&config_clone).await {
                    Ok(conn) => conn,
                    Err(_) => return (0, 1, Vec::new()), // Failed connection
                };

                let mut local_query_times = Vec::new();
                let mut local_successful = 0;
                let mut local_failed = 0;

                // Each task performs multiple queries
                for i in 0..10 {
                    let query_start = Instant::now();

                    let params = vec![
                        Value::from_i64((current_task_id * 10 + i) as i64),
                        Value::String(format!("concurrent_test_{}_{}", current_task_id, i)),
                        Value::from_bool(i % 2 == 0),
                        Value::Float(i as f64 * 1.414),
                        Value::String(format!("task_{}_iteration_{}", current_task_id, i)),
                    ];

                    match connection.query(&template_clone.sql, &params).await {
                        Ok(_) => {
                            local_successful += 1;
                            local_query_times.push(query_start.elapsed().as_millis() as u64);
                        }
                        Err(_) => local_failed += 1,
                    }
                }

                (local_successful, local_failed, local_query_times)
            });

            handles.push(handle);
            task_id += 1;

            // Limit the number of pending tasks
            if handles.len() >= self.config.max_concurrent_queries * 2 {
                break;
            }
        }

        // Collect results from all tasks
        let mut all_query_times = Vec::new();

        for handle in handles {
            match handle.await {
                Ok((successful, failed, times)) => {
                    metrics.successful_queries += successful;
                    metrics.failed_queries += failed;
                    all_query_times.extend(times);
                }
                Err(_) => {
                    metrics.failed_queries += 10; // Assume 10 queries per task failed
                }
            }
        }

        metrics.total_queries = metrics.successful_queries + metrics.failed_queries;
        metrics.total_duration_seconds = test_start.elapsed().as_secs_f64();
        metrics.calculate_statistics(&mut all_query_times);

        // Add concurrency information
        metrics.parameter_sizes.insert(
            "max_concurrent_queries".to_string(),
            self.config.max_concurrent_queries,
        );

        self.metrics.push(metrics.clone());
        self.print_param_metrics(&metrics);

        Ok(())
    }

    /// Test batch parameter performance
    pub async fn test_batch_parameters(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📦 Testing Batch Parameter Performance");

        for &batch_size in &self.config.batch_sizes {
            println!("  Testing batch size: {}", batch_size);

            let mut metrics =
                ParamPerformanceMetrics::new(format!("Batch Parameters (size: {})", batch_size));
            let mut query_times = Vec::new();

            let engine = self.engine.as_ref().unwrap();
            let connection = engine.connect(engine.get_config()).await?;

            let test_start = Instant::now();
            let test_end = test_start
                + Duration::from_secs(
                    self.config.test_duration_seconds / self.config.batch_sizes.len() as u64,
                );

            let mut batch_iteration = 0;
            while Instant::now() < test_end {
                let query_start = Instant::now();

                // Create batch query with multiple parameter sets
                let mut batch_sql = String::new();
                let mut batch_params = Vec::new();

                for i in 0..batch_size {
                    if i > 0 {
                        batch_sql.push_str(" UNION ALL ");
                    }
                    batch_sql.push_str("SELECT ? as id, ? as batch_id, ? as item_id, ? as data");

                    batch_params.extend(vec![
                        Value::from_i64((batch_iteration * batch_size + i) as i64),
                        Value::from_i64(batch_iteration as i64),
                        Value::from_i64(i as i64),
                        Value::String(format!("batch_data_{}_{}", batch_iteration, i)),
                    ]);
                }

                match connection.query(&batch_sql, &batch_params).await {
                    Ok(_) => {
                        metrics.successful_queries += 1;
                        query_times.push(query_start.elapsed().as_millis() as u64);
                    }
                    Err(_) => metrics.failed_queries += 1,
                }

                metrics.total_queries += 1;
                batch_iteration += 1;
            }

            metrics.total_duration_seconds = test_start.elapsed().as_secs_f64();
            metrics.calculate_statistics(&mut query_times);

            // Track batch information
            metrics
                .parameter_sizes
                .insert("batch_size".to_string(), batch_size);
            metrics
                .parameter_sizes
                .insert("params_per_batch".to_string(), batch_size * 4);

            self.metrics.push(metrics.clone());
            self.print_param_metrics(&metrics);
        }

        Ok(())
    }

    /// Print parameter performance metrics
    fn print_param_metrics(&self, metrics: &ParamPerformanceMetrics) {
        println!("\n📊 {} Results:", metrics.test_name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📈 Total Queries: {}", metrics.total_queries);
        println!(
            "✅ Successful: {} ({:.1}%)",
            metrics.successful_queries,
            100.0 - metrics.error_rate_percentage
        );
        println!(
            "❌ Failed: {} ({:.1}%)",
            metrics.failed_queries, metrics.error_rate_percentage
        );
        println!("⏱️  Test Duration: {:.1}s", metrics.total_duration_seconds);
        println!("⚡ Queries per Second: {:.2}", metrics.queries_per_second);
        println!("🕐 Average Query Time: {:.2}ms", metrics.avg_query_time_ms);
        println!("📊 95th Percentile: {:.2}ms", metrics.p95_query_time_ms);
        println!("📈 99th Percentile: {:.2}ms", metrics.p99_query_time_ms);

        if !metrics.parameter_sizes.is_empty() {
            println!("🔧 Parameter Info:");
            for (key, value) in &metrics.parameter_sizes {
                println!("   {}: {}", key, value);
            }
        }

        // Performance assessment
        if metrics.queries_per_second > 500.0 && metrics.error_rate_percentage < 1.0 {
            println!("✅ EXCELLENT: High throughput with low error rate");
        } else if metrics.queries_per_second > 200.0 && metrics.error_rate_percentage < 5.0 {
            println!("✅ GOOD: Acceptable performance");
        } else if metrics.error_rate_percentage > 10.0 {
            println!("❌ HIGH ERROR RATE: Check parameter handling");
        } else {
            println!("⚠️  MODERATE: Performance could be improved");
        }

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }

    /// Generate comprehensive parameter performance report
    pub fn generate_param_report(&self) -> Result<String, Box<dyn std::error::Error>> {
        let report = json!({
            "parameterized_query_performance": {
                "test_configuration": {
                    "test_duration_seconds": self.config.test_duration_seconds,
                    "max_concurrent_queries": self.config.max_concurrent_queries,
                    "parameter_variations": self.config.parameter_variations,
                    "large_parameter_size_kb": self.config.large_parameter_size_kb,
                    "batch_sizes": self.config.batch_sizes,
                },
                "query_templates": self.query_templates.iter().map(|t| {
                    json!({
                        "query_type": format!("{:?}", t.query_type),
                        "param_count": t.param_count,
                        "complexity_score": t.complexity_score,
                        "description": t.description,
                    })
                }).collect::<Vec<_>>(),
                "test_results": self.metrics.iter().map(|m| {
                    json!({
                        "test_name": m.test_name,
                        "performance_metrics": {
                            "total_queries": m.total_queries,
                            "successful_queries": m.successful_queries,
                            "error_rate_percentage": m.error_rate_percentage,
                            "queries_per_second": m.queries_per_second,
                            "avg_query_time_ms": m.avg_query_time_ms,
                            "p95_query_time_ms": m.p95_query_time_ms,
                            "p99_query_time_ms": m.p99_query_time_ms,
                        },
                        "parameter_info": m.parameter_sizes,
                        "performance_grade": self.assess_param_performance(m),
                    })
                }).collect::<Vec<_>>(),
                "performance_summary": {
                    "total_tests": self.metrics.len(),
                    "avg_queries_per_second": self.metrics.iter()
                        .map(|m| m.queries_per_second)
                        .sum::<f64>() / self.metrics.len() as f64,
                    "avg_error_rate": self.metrics.iter()
                        .map(|m| m.error_rate_percentage)
                        .sum::<f64>() / self.metrics.len() as f64,
                    "best_performance_test": self.find_best_performance_test(),
                    "worst_performance_test": self.find_worst_performance_test(),
                },
                "optimization_recommendations": self.generate_param_recommendations(),
            }
        });

        Ok(serde_json::to_string_pretty(&report)?)
    }

    /// Assess parameter performance
    fn assess_param_performance(&self, metrics: &ParamPerformanceMetrics) -> String {
        match (
            metrics.queries_per_second,
            metrics.error_rate_percentage,
            metrics.avg_query_time_ms,
        ) {
            (qps, err, avg) if qps > 1000.0 && err < 1.0 && avg < 10.0 => "EXCELLENT".to_string(),
            (qps, err, avg) if qps > 500.0 && err < 2.0 && avg < 20.0 => "VERY_GOOD".to_string(),
            (qps, err, avg) if qps > 200.0 && err < 5.0 && avg < 50.0 => "GOOD".to_string(),
            (qps, err, avg) if qps > 100.0 && err < 10.0 && avg < 100.0 => "AVERAGE".to_string(),
            _ => "NEEDS_IMPROVEMENT".to_string(),
        }
    }

    /// Find best performing test
    fn find_best_performance_test(&self) -> String {
        self.metrics
            .iter()
            .max_by(|a, b| {
                a.queries_per_second
                    .partial_cmp(&b.queries_per_second)
                    .unwrap()
            })
            .map(|m| m.test_name.clone())
            .unwrap_or_else(|| "None".to_string())
    }

    /// Find worst performing test
    fn find_worst_performance_test(&self) -> String {
        self.metrics
            .iter()
            .min_by(|a, b| {
                a.queries_per_second
                    .partial_cmp(&b.queries_per_second)
                    .unwrap()
            })
            .map(|m| m.test_name.clone())
            .unwrap_or_else(|| "None".to_string())
    }

    /// Generate parameter optimization recommendations
    fn generate_param_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        let avg_qps = self
            .metrics
            .iter()
            .map(|m| m.queries_per_second)
            .sum::<f64>()
            / self.metrics.len() as f64;

        let avg_error_rate = self
            .metrics
            .iter()
            .map(|m| m.error_rate_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_error_rate > 5.0 {
            recommendations.push(
                "HIGH PRIORITY: Error rate is high. Check parameter validation and type handling."
                    .to_string(),
            );
        }

        if avg_qps < 200.0 {
            recommendations.push(
                "Consider implementing prepared statement caching for better performance."
                    .to_string(),
            );
            recommendations.push("Optimize parameter conversion and validation logic.".to_string());
        }

        // Find performance bottlenecks
        let large_param_metrics = self
            .metrics
            .iter()
            .find(|m| m.test_name.contains("Large Parameters"));

        if let Some(large_metrics) = large_param_metrics {
            if large_metrics.avg_query_time_ms > 100.0 {
                recommendations.push("Large parameter handling is slow. Consider streaming or chunking large parameters.".to_string());
            }
        }

        let concurrent_metrics = self
            .metrics
            .iter()
            .find(|m| m.test_name.contains("Concurrent"));

        if let Some(concurrent) = concurrent_metrics {
            if concurrent.queries_per_second < avg_qps * 0.5 {
                recommendations.push("Concurrent performance is significantly lower. Check for parameter serialization bottlenecks.".to_string());
            }
        }

        if recommendations.is_empty() {
            recommendations
                .push("Parameterized query performance is within acceptable ranges.".to_string());
            recommendations
                .push("Consider monitoring parameter performance in production.".to_string());
        }

        recommendations
    }

    /// Run all parameterized query performance tests
    pub async fn run_all_tests(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🚀 Starting Parameterized Query Performance Test Suite");
        println!("Configuration: {:?}", self.config);

        self.initialize().await?;

        self.test_simple_parameters().await?;
        self.test_multiple_parameters().await?;
        self.test_large_parameters().await?;
        self.test_concurrent_parameters().await?;
        self.test_batch_parameters().await?;

        // Generate and save report
        let report = self.generate_param_report()?;
        std::fs::write("mysql_parameterized_query_performance_report.json", &report)?;

        println!("📄 Parameterized query performance report saved");

        // Print overall summary
        let avg_qps = self
            .metrics
            .iter()
            .map(|m| m.queries_per_second)
            .sum::<f64>()
            / self.metrics.len() as f64;

        let avg_error_rate = self
            .metrics
            .iter()
            .map(|m| m.error_rate_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        println!("🎯 Overall Performance: {:.2} QPS", avg_qps);
        println!("📊 Overall Error Rate: {:.1}%", avg_error_rate);

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ParamQueryConfig {
        test_duration_seconds: 30,
        max_concurrent_queries: 25,
        parameter_variations: 500,
        query_complexity_levels: 5,
        large_parameter_size_kb: 32,
        batch_sizes: vec![1, 5, 10, 25],
        warmup_duration_seconds: 5,
    };

    let mut test = ParamQueryPerformanceTest::new(config);
    test.run_all_tests().await?;

    Ok(())
}
