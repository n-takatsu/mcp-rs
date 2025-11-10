#!/usr/bin/env cargo-script

//! 🔄 Production Monitoring Example
//!
//! This example demonstrates production-ready monitoring for WordPress integration.
//! It includes automated health checks, authentication validation, and alert generation.
//!
//! Run with: cargo run --example production_monitoring

use mcp_rs::config::McpConfig;
use mcp_rs::handlers::wordpress::WordPressHandler;
use serde_json::json;
use std::time::{Duration, Instant};
use tokio::time::interval;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 MCP-RS Production Monitoring");
    println!("================================");

    // Load configuration
    let config = McpConfig::load()?;

    // Initialize WordPress handler
    let wp_config = config
        .handlers
        .wordpress
        .clone()
        .ok_or("WordPress configuration not found")?;
    let handler = WordPressHandler::new(wp_config);

    println!("📋 Starting production monitoring suite...\n");

    // Run monitoring checks
    run_monitoring_suite(&handler).await?;

    Ok(())
}

async fn run_monitoring_suite(
    handler: &WordPressHandler,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut check_count = 0;
    let mut success_count = 0;
    let mut warning_count = 0;
    let mut error_count = 0;

    println!("🎯 1. Basic Connectivity Check");
    println!("------------------------------");
    let start = Instant::now();
    match basic_connectivity_check(handler).await {
        Ok(()) => {
            println!(
                "✅ Basic connectivity: PASS ({:.2}ms)",
                start.elapsed().as_millis()
            );
            success_count += 1;
        }
        Err(e) => {
            println!("❌ Basic connectivity: FAIL - {}", e);
            error_count += 1;
        }
    }
    check_count += 1;

    println!("\n🔐 2. Authentication Validation");
    println!("-------------------------------");
    let start = Instant::now();
    match authentication_check(handler).await {
        Ok(username) => {
            println!(
                "✅ Authentication: PASS - User: {} ({:.2}ms)",
                username,
                start.elapsed().as_millis()
            );
            success_count += 1;
        }
        Err(e) => {
            println!("❌ Authentication: FAIL - {}", e);
            println!("💡 Resolution: Generate new application password in WordPress Admin");
            error_count += 1;
        }
    }
    check_count += 1;

    println!("\n🛠️ 3. API Endpoint Validation");
    println!("-----------------------------");
    let start = Instant::now();
    match api_endpoint_check(handler).await {
        Ok(endpoints) => {
            println!(
                "✅ API Endpoints: PASS - {} endpoints accessible ({:.2}ms)",
                endpoints.len(),
                start.elapsed().as_millis()
            );
            for endpoint in endpoints.iter().take(5) {
                println!("   ├── {}", endpoint);
            }
            if endpoints.len() > 5 {
                println!("   └── ... and {} more", endpoints.len() - 5);
            }
            success_count += 1;
        }
        Err(e) => {
            println!("⚠️ API Endpoints: WARNING - {}", e);
            println!("💡 Some endpoints may be restricted or unavailable");
            warning_count += 1;
        }
    }
    check_count += 1;

    println!("\n🏥 4. WordPress Health Assessment");
    println!("--------------------------------");
    let start = Instant::now();
    match health_assessment(handler).await {
        Ok(report) => {
            println!(
                "✅ Health Assessment: PASS ({:.2}ms)",
                start.elapsed().as_millis()
            );
            println!(
                "   ├── WordPress Version: {}",
                report.get("wordpress_version").unwrap_or(&json!("Unknown"))
            );
            println!(
                "   ├── PHP Version: {}",
                report.get("php_version").unwrap_or(&json!("Unknown"))
            );
            println!(
                "   ├── Active Plugins: {}",
                report.get("active_plugins").unwrap_or(&json!(0))
            );
            println!(
                "   └── Memory Usage: {}",
                report.get("memory_usage").unwrap_or(&json!("Unknown"))
            );
            success_count += 1;
        }
        Err(e) => {
            println!("⚠️ Health Assessment: WARNING - {}", e);
            println!("💡 Health data may be limited or unavailable");
            warning_count += 1;
        }
    }
    check_count += 1;

    println!("\n🚦 5. Performance Metrics");
    println!("-------------------------");
    let start = Instant::now();
    match performance_check(handler).await {
        Ok(metrics) => {
            println!(
                "✅ Performance: PASS ({:.2}ms)",
                start.elapsed().as_millis()
            );
            println!("   ├── Response Time: {:.2}ms", metrics.response_time);
            println!("   ├── Throughput: {:.1} req/s", metrics.throughput);
            println!("   └── Status: {}", metrics.status);

            if metrics.response_time > 2000.0 {
                println!("⚠️ High response time detected (>2s)");
                warning_count += 1;
            } else {
                success_count += 1;
            }
        }
        Err(e) => {
            println!("❌ Performance: FAIL - {}", e);
            error_count += 1;
        }
    }
    check_count += 1;

    // Summary Report
    println!("\n📊 Monitoring Summary");
    println!("=====================");
    println!("Total Checks: {}", check_count);
    println!("✅ Passed: {}", success_count);
    println!("⚠️ Warnings: {}", warning_count);
    println!("❌ Errors: {}", error_count);

    let success_rate = (success_count as f64 / check_count as f64) * 100.0;
    println!("Success Rate: {:.1}%", success_rate);

    if error_count > 0 {
        println!("\n🚨 Critical Issues Detected!");
        println!("Immediate action required:");
        println!("1. Check WordPress application password");
        println!("2. Verify WordPress site accessibility");
        println!("3. Review network connectivity");
        println!("4. Run diagnostic: cargo run --example settings_api_deep_diagnosis");
    } else if warning_count > 0 {
        println!("\n⚠️ Minor Issues Detected");
        println!("Recommended actions:");
        println!("1. Monitor performance trends");
        println!("2. Consider WordPress optimization");
        println!("3. Review plugin compatibility");
    } else {
        println!("\n🎉 All systems operational!");
        println!("WordPress integration is healthy and ready for production use.");
    }

    Ok(())
}

async fn basic_connectivity_check(
    handler: &WordPressHandler,
) -> Result<(), Box<dyn std::error::Error>> {
    // Test basic WordPress connectivity using health check
    let health_check = handler.health_check().await;

    if health_check.site_accessible {
        Ok(())
    } else {
        Err("WordPress site not accessible".into())
    }
}

async fn authentication_check(
    handler: &WordPressHandler,
) -> Result<String, Box<dyn std::error::Error>> {
    // Test authentication by running health check
    let health_check = handler.health_check().await;

    if health_check.authentication_valid {
        Ok("Authenticated".to_string())
    } else {
        Err("Authentication failed".into())
    }
}

async fn api_endpoint_check(
    handler: &WordPressHandler,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Test API endpoints through health check
    let health_check = handler.health_check().await;

    let mut accessible_endpoints = Vec::new();

    if health_check.rest_api_available {
        accessible_endpoints.push("WordPress REST API".to_string());
    }
    if health_check.authentication_valid {
        accessible_endpoints.push("Authentication".to_string());
    }
    if health_check.permissions_adequate {
        accessible_endpoints.push("Permissions".to_string());
    }
    if health_check.media_upload_possible {
        accessible_endpoints.push("Media Upload".to_string());
    }

    if accessible_endpoints.is_empty() {
        Err("No API endpoints accessible".into())
    } else {
        Ok(accessible_endpoints)
    }
}

async fn health_assessment(
    handler: &WordPressHandler,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    // Get comprehensive health information
    let health_check = handler.health_check().await;

    let report = serde_json::json!({
        "site_accessible": health_check.site_accessible,
        "rest_api_available": health_check.rest_api_available,
        "authentication_valid": health_check.authentication_valid,
        "permissions_adequate": health_check.permissions_adequate,
        "media_upload_possible": health_check.media_upload_possible,
        "error_details": health_check.error_details,
        "site_info": health_check.site_info,
        "wordpress_version": health_check.site_info.as_ref()
            .map(|_info| "Available".to_string())
            .unwrap_or_else(|| "Unknown".to_string()),
        "php_version": "Unknown", // Not available in current health check
        "active_plugins": 0, // Not available in current health check
        "memory_usage": "Unknown" // Not available in current health check
    });

    Ok(report)
}

#[derive(Debug)]
struct PerformanceMetrics {
    response_time: f64,
    throughput: f64,
    status: String,
}

async fn performance_check(
    handler: &WordPressHandler,
) -> Result<PerformanceMetrics, Box<dyn std::error::Error>> {
    // Measure response time for health checks
    let start = Instant::now();
    let request_count = 3;

    for _ in 0..request_count {
        let _health_check = handler.health_check().await;
    }

    let total_time = start.elapsed().as_millis() as f64;
    let avg_response_time = total_time / request_count as f64;
    let throughput = (request_count as f64 / total_time) * 1000.0; // req/s

    let status = if avg_response_time < 500.0 {
        "Excellent"
    } else if avg_response_time < 1000.0 {
        "Good"
    } else if avg_response_time < 2000.0 {
        "Fair"
    } else {
        "Poor"
    };

    Ok(PerformanceMetrics {
        response_time: avg_response_time,
        throughput,
        status: status.to_string(),
    })
}

/// Example of continuous monitoring (commented out for demo)
#[allow(dead_code)]
async fn continuous_monitoring(
    handler: &WordPressHandler,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut interval = interval(Duration::from_secs(300)); // Check every 5 minutes

    loop {
        interval.tick().await;

        println!("🔄 Running scheduled health check...");

        match basic_connectivity_check(handler).await {
            Ok(()) => println!("✅ Scheduled check: PASS"),
            Err(e) => {
                println!("❌ Scheduled check: FAIL - {}", e);
                // In production, send alerts here
            }
        }
    }
}
