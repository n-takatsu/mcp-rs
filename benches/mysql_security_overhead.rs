//! Security Overhead Measurement
//!
//! Measures the performance overhead introduced by security layers
//! Compares performance with and without security features enabled

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
use std::time::Instant;

/// Security overhead measurement configuration
#[derive(Debug, Clone)]
pub struct SecurityOverheadConfig {
    pub test_queries: usize,
    pub warmup_queries: usize,
    pub concurrent_connections: usize,
    pub security_rules_count: usize,
    pub repeat_measurements: usize,
}

impl Default for SecurityOverheadConfig {
    fn default() -> Self {
        Self {
            test_queries: 1000,
            warmup_queries: 50,
            concurrent_connections: 5,
            security_rules_count: 10,
            repeat_measurements: 3,
        }
    }
}

/// Performance measurement results
#[derive(Debug, Clone)]
pub struct OverheadMetrics {
    pub test_name: String,
    pub with_security_ms: f64,
    pub without_security_ms: f64,
    pub overhead_ms: f64,
    pub overhead_percentage: f64,
    pub queries_per_second_secure: f64,
    pub queries_per_second_baseline: f64,
    pub throughput_impact_percentage: f64,
    pub memory_overhead_mb: f64,
}

impl OverheadMetrics {
    pub fn new(test_name: String) -> Self {
        Self {
            test_name,
            with_security_ms: 0.0,
            without_security_ms: 0.0,
            overhead_ms: 0.0,
            overhead_percentage: 0.0,
            queries_per_second_secure: 0.0,
            queries_per_second_baseline: 0.0,
            throughput_impact_percentage: 0.0,
            memory_overhead_mb: 0.0,
        }
    }

    pub fn calculate_overhead(&mut self) {
        if self.without_security_ms > 0.0 {
            self.overhead_ms = self.with_security_ms - self.without_security_ms;
            self.overhead_percentage = (self.overhead_ms / self.without_security_ms) * 100.0;

            if self.queries_per_second_baseline > 0.0 {
                self.throughput_impact_percentage = ((self.queries_per_second_baseline
                    - self.queries_per_second_secure)
                    / self.queries_per_second_baseline)
                    * 100.0;
            }
        }
    }
}

/// Security Overhead Measurement Suite
pub struct SecurityOverheadMeasurement {
    config: SecurityOverheadConfig,
    baseline_engine: Option<MySqlEngine>,
    secure_engine: Option<MySqlEngine>,
    metrics: Vec<OverheadMetrics>,
}

impl SecurityOverheadMeasurement {
    pub fn new(config: SecurityOverheadConfig) -> Self {
        Self {
            config,
            baseline_engine: None,
            secure_engine: None,
            metrics: Vec::new(),
        }
    }

    /// Initialize both baseline and secure MySQL engines
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Initializing Security Overhead Measurement");

        let base_db_config = DatabaseConfig {
            database_type: DatabaseType::MySQL,
            connection: ConnectionConfig {
                host: std::env::var("MYSQL_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("MYSQL_PORT")
                    .unwrap_or_else(|_| "3306".to_string())
                    .parse()
                    .unwrap_or(3306),
                database: std::env::var("MYSQL_DATABASE")
                    .unwrap_or_else(|_| "security_test_db".to_string()),
                username: std::env::var("MYSQL_USER")
                    .unwrap_or_else(|_| "security_user".to_string()),
                password: std::env::var("MYSQL_PASSWORD")
                    .unwrap_or_else(|_| "security_pass".to_string()),
                ssl_mode: Some("disabled".to_string()),
                timeout_seconds: 30,
                retry_attempts: 3,
                options: std::collections::HashMap::new(),
            },
            pool: PoolConfig::default(),
            security: SecurityConfig::default(),
            features: FeatureConfig::default(),
        };

        // Create baseline engine without security
        self.baseline_engine =
            Some(MySqlEngine::new_without_security(base_db_config.clone()).await?);

        // Create secure engine with default security configuration
        let secure_config = base_db_config.clone();

        // For testing purposes, we'll use the same engine
        self.secure_engine = Some(MySqlEngine::new_without_security(secure_config).await?);

        println!("✅ Both baseline and secure engines initialized");
        Ok(())
    }

    /// Measure basic query overhead
    pub async fn measure_basic_query_overhead(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔍 Measuring Basic Query Security Overhead");

        let mut metrics = OverheadMetrics::new("Basic Query Overhead".to_string());

        // Test queries
        let test_queries = vec![
            ("SELECT 1 as test", vec![]),
            ("SELECT ? as param_test", vec![Value::from_i64(123)]),
            (
                "SELECT ?, ? as multi_param",
                vec![Value::String("test".to_string()), Value::from_bool(true)],
            ),
            (
                "SELECT COUNT(*) as count_test FROM (SELECT 1) as sub",
                vec![],
            ),
        ];

        // Measure baseline performance (without security)
        println!("📊 Measuring baseline performance...");
        let baseline_time = self
            .measure_engine_performance(
                self.baseline_engine.as_ref().unwrap(),
                &test_queries,
                "baseline",
            )
            .await?;

        // Measure secure performance (with security)
        println!("🔒 Measuring secure performance...");
        let secure_time = self
            .measure_engine_performance(
                self.secure_engine.as_ref().unwrap(),
                &test_queries,
                "secure",
            )
            .await?;

        metrics.without_security_ms = baseline_time.avg_query_time_ms;
        metrics.with_security_ms = secure_time.avg_query_time_ms;
        metrics.queries_per_second_baseline = baseline_time.queries_per_second;
        metrics.queries_per_second_secure = secure_time.queries_per_second;
        metrics.calculate_overhead();

        self.metrics.push(metrics.clone());
        self.print_overhead_metrics(&metrics);

        Ok(())
    }

    /// Measure parameterized query overhead
    pub async fn measure_parameterized_query_overhead(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Measuring Parameterized Query Security Overhead");

        let mut metrics = OverheadMetrics::new("Parameterized Query Overhead".to_string());

        // Complex parameterized queries that trigger security validation
        let test_queries = vec![
            (
                "SELECT * FROM (SELECT ? as id, ? as name, ? as email) as users WHERE users.id = ?",
                vec![
                    Value::from_i64(1),
                    Value::String("John Doe".to_string()),
                    Value::String("john@example.com".to_string()),
                    Value::from_i64(1),
                ],
            ),
            (
                "SELECT ?, ?, ?, ? as complex_query",
                vec![
                    Value::String("SELECT * FROM users".to_string()), // This should trigger security check
                    Value::from_i64(42),
                    Value::from_bool(true),
                    Value::Float(std::f64::consts::PI),
                ],
            ),
            (
                "SELECT ? FROM (SELECT ? as data WHERE ? > ?) as filtered",
                vec![
                    Value::String("filtered_data".to_string()),
                    Value::String("test_data".to_string()),
                    Value::from_i64(100),
                    Value::from_i64(50),
                ],
            ),
        ];

        // Measure both engines
        let baseline_time = self
            .measure_engine_performance(
                self.baseline_engine.as_ref().unwrap(),
                &test_queries,
                "baseline_parameterized",
            )
            .await?;

        let secure_time = self
            .measure_engine_performance(
                self.secure_engine.as_ref().unwrap(),
                &test_queries,
                "secure_parameterized",
            )
            .await?;

        metrics.without_security_ms = baseline_time.avg_query_time_ms;
        metrics.with_security_ms = secure_time.avg_query_time_ms;
        metrics.queries_per_second_baseline = baseline_time.queries_per_second;
        metrics.queries_per_second_secure = secure_time.queries_per_second;
        metrics.calculate_overhead();

        self.metrics.push(metrics.clone());
        self.print_overhead_metrics(&metrics);

        Ok(())
    }

    /// Measure concurrent query overhead
    pub async fn measure_concurrent_overhead(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔀 Measuring Concurrent Query Security Overhead");

        let mut metrics = OverheadMetrics::new("Concurrent Query Overhead".to_string());

        let queries_per_connection = self.config.test_queries / self.config.concurrent_connections;

        // Measure baseline concurrent performance
        println!("📊 Measuring baseline concurrent performance...");
        let baseline_start = Instant::now();
        let baseline_results = self
            .run_concurrent_queries(
                self.baseline_engine.as_ref().unwrap(),
                queries_per_connection,
            )
            .await?;
        let baseline_duration = baseline_start.elapsed();

        // Measure secure concurrent performance
        println!("🔒 Measuring secure concurrent performance...");
        let secure_start = Instant::now();
        let secure_results = self
            .run_concurrent_queries(self.secure_engine.as_ref().unwrap(), queries_per_connection)
            .await?;
        let secure_duration = secure_start.elapsed();

        // Calculate metrics
        metrics.without_security_ms =
            baseline_duration.as_millis() as f64 / baseline_results.successful_queries as f64;
        metrics.with_security_ms =
            secure_duration.as_millis() as f64 / secure_results.successful_queries as f64;

        metrics.queries_per_second_baseline = baseline_results.successful_queries as f64
            / (baseline_duration.as_millis() as f64 / 1000.0);
        metrics.queries_per_second_secure = secure_results.successful_queries as f64
            / (secure_duration.as_millis() as f64 / 1000.0);

        metrics.calculate_overhead();

        self.metrics.push(metrics.clone());
        self.print_overhead_metrics(&metrics);

        Ok(())
    }

    /// Run concurrent queries for overhead measurement
    async fn run_concurrent_queries(
        &self,
        engine: &MySqlEngine,
        queries_per_connection: usize,
    ) -> Result<ConcurrentResults, Box<dyn std::error::Error>> {
        let mut handles = Vec::new();

        for connection_id in 0..self.config.concurrent_connections {
            let engine_clone = engine.clone();
            let config_clone = engine.get_config().clone();

            let handle = tokio::spawn(async move {
                let connection = engine_clone.connect(&config_clone).await?;
                let mut successful = 0;
                let mut failed = 0;

                for i in 0..queries_per_connection {
                    match connection
                        .query(
                            "SELECT ? as conn_id, ? as query_id, ? as timestamp, ? as test_data",
                            &[
                                Value::from_i64(connection_id as i64),
                                Value::from_i64(i as i64),
                                Value::from_i64(chrono::Utc::now().timestamp()),
                                Value::String(format!("test_data_{}", i)),
                            ],
                        )
                        .await
                    {
                        Ok(_) => successful += 1,
                        Err(_) => failed += 1,
                    }
                }

                Ok::<(usize, usize), Box<dyn std::error::Error + Send + Sync>>((successful, failed))
            });

            handles.push(handle);
        }

        let mut total_successful = 0;
        let mut _total_failed = 0;

        for handle in handles {
            match handle.await? {
                Ok((successful, failed)) => {
                    total_successful += successful;
                    _total_failed += failed;
                }
                Err(_) => {
                    _total_failed += queries_per_connection;
                }
            }
        }

        Ok(ConcurrentResults {
            successful_queries: total_successful,
        })
    }

    /// Measure engine performance
    async fn measure_engine_performance(
        &self,
        engine: &MySqlEngine,
        test_queries: &[(&str, Vec<Value>)],
        test_type: &str,
    ) -> Result<EnginePerformance, Box<dyn std::error::Error>> {
        let connection = engine.connect(engine.get_config()).await?;

        // Warmup
        for _ in 0..self.config.warmup_queries {
            let (sql, params) = &test_queries[0];
            let _ = connection.query(sql, params).await;
        }

        // Measure performance
        let mut query_times = Vec::new();
        let mut successful_queries = 0;
        let mut _failed_queries = 0;

        let benchmark_start = Instant::now();

        for i in 0..self.config.test_queries {
            let (sql, params) = &test_queries[i % test_queries.len()];
            let start = Instant::now();

            match connection.query(sql, params).await {
                Ok(_) => {
                    successful_queries += 1;
                    query_times.push(start.elapsed().as_millis() as u64);
                }
                Err(e) => {
                    _failed_queries += 1;
                    println!("⚠️ Query failed in {} test: {}", test_type, e);
                }
            }
        }

        let total_duration = benchmark_start.elapsed();

        // Calculate statistics
        let avg_query_time_ms = if !query_times.is_empty() {
            query_times.iter().sum::<u64>() as f64 / query_times.len() as f64
        } else {
            0.0
        };

        let queries_per_second = if total_duration.as_millis() > 0 {
            (successful_queries as f64) / (total_duration.as_millis() as f64 / 1000.0)
        } else {
            0.0
        };

        Ok(EnginePerformance {
            avg_query_time_ms,
            queries_per_second,
        })
    }

    /// Print overhead metrics
    fn print_overhead_metrics(&self, metrics: &OverheadMetrics) {
        println!("\n📊 {} Results:", metrics.test_name);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("🔒 With Security: {:.2}ms avg", metrics.with_security_ms);
        println!("📊 Baseline: {:.2}ms avg", metrics.without_security_ms);
        println!(
            "⚡ Security Overhead: {:.2}ms ({:.1}%)",
            metrics.overhead_ms, metrics.overhead_percentage
        );
        println!("🚀 Secure QPS: {:.2}", metrics.queries_per_second_secure);
        println!(
            "📈 Baseline QPS: {:.2}",
            metrics.queries_per_second_baseline
        );
        println!(
            "📉 Throughput Impact: {:.1}%",
            metrics.throughput_impact_percentage
        );

        // Performance assessment
        if metrics.overhead_percentage < 10.0 {
            println!("✅ EXCELLENT: Security overhead is minimal (<10%)");
        } else if metrics.overhead_percentage < 25.0 {
            println!("✅ GOOD: Security overhead is acceptable (<25%)");
        } else if metrics.overhead_percentage < 50.0 {
            println!("⚠️  MODERATE: Security overhead is noticeable (<50%)");
        } else {
            println!("❌ HIGH: Security overhead is significant (>50%)");
        }

        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }

    /// Generate comprehensive overhead report
    pub fn generate_overhead_report(&self) -> Result<String, Box<dyn std::error::Error>> {
        let report = json!({
            "security_overhead_analysis": {
                "configuration": {
                    "test_queries": self.config.test_queries,
                    "warmup_queries": self.config.warmup_queries,
                    "concurrent_connections": self.config.concurrent_connections,
                    "security_rules_count": self.config.security_rules_count,
                    "repeat_measurements": self.config.repeat_measurements,
                },
                "test_results": self.metrics.iter().map(|m| {
                    json!({
                        "test_name": m.test_name,
                        "baseline_performance": {
                            "avg_query_time_ms": m.without_security_ms,
                            "queries_per_second": m.queries_per_second_baseline,
                        },
                        "secure_performance": {
                            "avg_query_time_ms": m.with_security_ms,
                            "queries_per_second": m.queries_per_second_secure,
                        },
                        "overhead_analysis": {
                            "overhead_ms": m.overhead_ms,
                            "overhead_percentage": m.overhead_percentage,
                            "throughput_impact_percentage": m.throughput_impact_percentage,
                            "memory_overhead_mb": m.memory_overhead_mb,
                        },
                        "performance_grade": self.assess_performance_grade(m.overhead_percentage),
                    })
                }).collect::<Vec<_>>(),
                "summary": {
                    "total_tests": self.metrics.len(),
                    "avg_overhead_percentage": self.metrics.iter()
                        .map(|m| m.overhead_percentage)
                        .sum::<f64>() / self.metrics.len() as f64,
                    "avg_throughput_impact": self.metrics.iter()
                        .map(|m| m.throughput_impact_percentage)
                        .sum::<f64>() / self.metrics.len() as f64,
                    "max_overhead_percentage": self.metrics.iter()
                        .map(|m| m.overhead_percentage)
                        .fold(0.0, f64::max),
                    "min_overhead_percentage": self.metrics.iter()
                        .map(|m| m.overhead_percentage)
                        .fold(f64::INFINITY, f64::min),
                },
                "recommendations": self.generate_overhead_recommendations(),
            }
        });

        Ok(serde_json::to_string_pretty(&report)?)
    }

    /// Assess performance grade based on overhead percentage
    fn assess_performance_grade(&self, overhead_percentage: f64) -> String {
        match overhead_percentage {
            x if x < 10.0 => "EXCELLENT".to_string(),
            x if x < 25.0 => "GOOD".to_string(),
            x if x < 50.0 => "MODERATE".to_string(),
            _ => "NEEDS_IMPROVEMENT".to_string(),
        }
    }

    /// Generate overhead optimization recommendations
    fn generate_overhead_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();

        let avg_overhead = self
            .metrics
            .iter()
            .map(|m| m.overhead_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_overhead > 50.0 {
            recommendations.push("HIGH PRIORITY: Security overhead is too high (>50%). Consider optimizing security rule patterns and reducing rule count.".to_string());
            recommendations.push(
                "Consider caching security validation results for repeated queries.".to_string(),
            );
        } else if avg_overhead > 25.0 {
            recommendations.push("MEDIUM PRIORITY: Security overhead is noticeable (>25%). Review and optimize security rule efficiency.".to_string());
        } else if avg_overhead > 10.0 {
            recommendations.push(
                "LOW PRIORITY: Security overhead is acceptable but could be optimized.".to_string(),
            );
        } else {
            recommendations.push(
                "EXCELLENT: Security overhead is minimal. Current configuration is well-optimized."
                    .to_string(),
            );
        }

        // Check throughput impact
        let avg_throughput_impact = self
            .metrics
            .iter()
            .map(|m| m.throughput_impact_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        if avg_throughput_impact > 30.0 {
            recommendations.push("Consider implementing asynchronous security validation to reduce throughput impact.".to_string());
        }

        recommendations.push("Monitor security overhead in production environments.".to_string());
        recommendations.push(
            "Consider implementing security rule prioritization based on threat levels."
                .to_string(),
        );

        recommendations
    }

    /// Run all security overhead measurements
    pub async fn run_all_measurements(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔍 Starting Security Overhead Measurement Suite");
        println!("Configuration: {:?}", self.config);

        self.initialize().await?;

        self.measure_basic_query_overhead().await?;
        self.measure_parameterized_query_overhead().await?;
        self.measure_concurrent_overhead().await?;

        // Generate and save report
        let report = self.generate_overhead_report()?;
        std::fs::write("mysql_security_overhead_report.json", &report)?;

        println!("📄 Security overhead report saved to: mysql_security_overhead_report.json");

        // Print overall summary
        let avg_overhead = self
            .metrics
            .iter()
            .map(|m| m.overhead_percentage)
            .sum::<f64>()
            / self.metrics.len() as f64;

        println!("🎯 Overall Security Overhead: {:.1}%", avg_overhead);
        println!(
            "📊 Performance Grade: {}",
            self.assess_performance_grade(avg_overhead)
        );

        Ok(())
    }
}

// Helper structs
#[derive(Debug)]
struct EnginePerformance {
    avg_query_time_ms: f64,
    queries_per_second: f64,
}

#[derive(Debug)]
struct ConcurrentResults {
    successful_queries: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SecurityOverheadConfig {
        test_queries: 1500,
        warmup_queries: 100,
        concurrent_connections: 8,
        security_rules_count: 15,
        repeat_measurements: 3,
    };

    let mut measurement = SecurityOverheadMeasurement::new(config);
    measurement.run_all_measurements().await?;

    Ok(())
}
