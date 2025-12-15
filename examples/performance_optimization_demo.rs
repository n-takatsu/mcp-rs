//! Performance optimization engine demo
//!
//! Demonstrates the performance optimization features including:
//! - Metrics collection
//! - Performance analysis
//! - Bottleneck detection
//! - Optimization recommendations

use mcp_rs::ai::performance::{
    analyzer::{DefaultPerformanceAnalyzer, PerformanceAnalyzer},
    bottleneck::{BottleneckDetector, DefaultBottleneckDetector},
    metrics::{MetricsHistory, SystemMetrics},
    optimizer::{DefaultOptimizationAdvisor, OptimizationAdvisor},
    recommendation::{DefaultRecommendationEngine, RecommendationEngine},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Performance Optimization Engine Demo ===\n");

    // Scenario 1: Healthy system
    println!("📊 Scenario 1: Healthy System");
    println!("--------------------------------");
    demo_healthy_system()?;
    println!();

    // Scenario 2: CPU bottleneck
    println!("📊 Scenario 2: CPU Bottleneck");
    println!("--------------------------------");
    demo_cpu_bottleneck()?;
    println!();

    // Scenario 3: Database performance issue
    println!("📊 Scenario 3: Database Performance Issue");
    println!("------------------------------------------");
    demo_database_issue()?;
    println!();

    // Scenario 4: Multiple bottlenecks
    println!("📊 Scenario 4: Multiple Bottlenecks");
    println!("------------------------------------");
    demo_multiple_bottlenecks()?;
    println!();

    // Scenario 5: Historical analysis
    println!("📊 Scenario 5: Historical Trend Analysis");
    println!("------------------------------------------");
    demo_historical_analysis()?;

    Ok(())
}

fn demo_healthy_system() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.45)
        .with_memory_usage(0.60)
        .with_cache_hit_rate(0.85)
        .with_avg_query_time(50.0);

    println!("Current Metrics:");
    println!("  CPU Usage: {:.1}%", metrics.cpu_usage * 100.0);
    println!("  Memory Usage: {:.1}%", metrics.memory_usage * 100.0);
    println!("  Cache Hit Rate: {:.1}%", metrics.cache_hit_rate * 100.0);
    println!("  Avg Query Time: {:.1}ms", metrics.avg_query_time);
    println!("  Health Score: {:.1}%", metrics.health_score() * 100.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics)?;

    println!("\nAnalysis Result:");
    println!("  Severity: {:?}", result.severity);
    println!("  Summary: {}", result.summary);
    println!("  Bottlenecks: {}", result.bottlenecks.len());

    Ok(())
}

fn demo_cpu_bottleneck() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.92)
        .with_memory_usage(0.65)
        .with_cache_hit_rate(0.75);

    println!("Current Metrics:");
    println!("  CPU Usage: {:.1}%", metrics.cpu_usage * 100.0);
    println!("  Memory Usage: {:.1}%", metrics.memory_usage * 100.0);
    println!("  Health Score: {:.1}%", metrics.health_score() * 100.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics)?;

    println!("\nAnalysis Result:");
    println!("  Severity: {:?}", result.severity);
    println!("  Summary: {}", result.summary);
    println!("  Bottlenecks detected: {}", result.bottlenecks.len());

    for bottleneck in &result.bottlenecks {
        println!("\n  Bottleneck: {:?}", bottleneck.category);
        println!("    Severity: {:?}", bottleneck.severity);
        println!("    Description: {}", bottleneck.description);
        println!("    Confidence: {:.0}%", bottleneck.confidence * 100.0);
    }

    // Generate optimization report
    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result)?;

    println!("\nOptimization Suggestions:");
    for (i, suggestion) in report.suggestions.iter().enumerate() {
        println!(
            "\n  {}. {} (Priority: {})",
            i + 1,
            suggestion.strategy.description(),
            suggestion.priority
        );
        println!("     {}", suggestion.description);
        println!(
            "     Impact: {:.0}%, Difficulty: {:.0}%, Hours: {:.1}",
            suggestion.expected_impact * 100.0,
            suggestion.difficulty * 100.0,
            suggestion.estimated_hours
        );
    }

    // Generate recommendations
    let engine = DefaultRecommendationEngine::new();
    let recommendations = engine.generate_recommendations(&report)?;

    println!("\nDetailed Recommendations:");
    for (i, rec) in recommendations.iter().take(2).enumerate() {
        println!("\n  {}. {}", i + 1, rec.title);
        println!("     Impact Level: {:?}", rec.impact_level);
        println!("     Steps:");
        for step in &rec.steps {
            println!("       {}", step);
        }
    }

    Ok(())
}

fn demo_database_issue() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.55)
        .with_memory_usage(0.70)
        .with_avg_query_time(350.0)
        .with_cache_hit_rate(0.35);

    println!("Current Metrics:");
    println!("  CPU Usage: {:.1}%", metrics.cpu_usage * 100.0);
    println!("  Avg Query Time: {:.1}ms", metrics.avg_query_time);
    println!("  Cache Hit Rate: {:.1}%", metrics.cache_hit_rate * 100.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics)?;

    println!("\nAnalysis Result:");
    println!("  Severity: {:?}", result.severity);
    println!("  Bottlenecks: {}", result.bottlenecks.len());

    for bottleneck in &result.bottlenecks {
        println!("\n  {:?}: {}", bottleneck.category, bottleneck.description);
    }

    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result)?;

    println!("\nQuick Wins:");
    for suggestion in &report.quick_wins {
        println!(
            "  • {} ({:.1}h, Impact: {:.0}%)",
            suggestion.description,
            suggestion.estimated_hours,
            suggestion.expected_impact * 100.0
        );
    }

    Ok(())
}

fn demo_multiple_bottlenecks() -> Result<(), Box<dyn std::error::Error>> {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.88)
        .with_memory_usage(0.92)
        .with_avg_query_time(280.0)
        .with_cache_hit_rate(0.28);

    println!("Current Metrics:");
    println!("  CPU Usage: {:.1}%", metrics.cpu_usage * 100.0);
    println!("  Memory Usage: {:.1}%", metrics.memory_usage * 100.0);
    println!("  Avg Query Time: {:.1}ms", metrics.avg_query_time);
    println!("  Cache Hit Rate: {:.1}%", metrics.cache_hit_rate * 100.0);
    println!("  Health Score: {:.1}%", metrics.health_score() * 100.0);

    let detector = DefaultBottleneckDetector::new();
    let bottlenecks = detector.detect(&metrics);
    let prioritized = detector.prioritize(&bottlenecks);

    println!("\nPrioritized Bottlenecks:");
    for (i, bottleneck) in prioritized.iter().enumerate() {
        println!(
            "\n  {}. {:?} (Priority Score: {})",
            i + 1,
            bottleneck.category,
            bottleneck.priority_score()
        );
        println!("     Severity: {:?}", bottleneck.severity);
        println!("     {}", bottleneck.description);
    }

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics)?;
    let anomalies = analyzer.detect_anomalies(&metrics);

    if !anomalies.is_empty() {
        println!("\n⚠️  Anomalies Detected:");
        for anomaly in &anomalies {
            println!("  • {}", anomaly);
        }
    }

    Ok(())
}

fn demo_historical_analysis() -> Result<(), Box<dyn std::error::Error>> {
    let mut history = MetricsHistory::new(3600); // 1 hour window

    // Simulate metrics over time with increasing CPU usage
    println!("Collecting metrics over time...");
    for i in 0..10 {
        let cpu = 0.50 + (i as f64 * 0.05);
        let memory = 0.60 + (i as f64 * 0.03);

        let metrics = SystemMetrics::new()
            .with_cpu_usage(cpu)
            .with_memory_usage(memory)
            .with_cache_hit_rate(0.70);

        history.add(metrics);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("Metrics collected: {}", history.metrics.len());

    println!("\nHistorical Statistics:");
    println!("  Avg CPU: {:.1}%", history.avg_cpu_usage() * 100.0);
    println!("  Peak CPU: {:.1}%", history.peak_cpu_usage() * 100.0);
    println!("  Avg Memory: {:.1}%", history.avg_memory_usage() * 100.0);
    println!("  Peak Memory: {:.1}%", history.peak_memory_usage() * 100.0);
    println!("  CPU Trend: {:+.3}", history.cpu_trend());

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze_history(&history)?;

    println!("\nHistorical Analysis:");
    println!("  Severity: {:?}", result.severity);
    println!("  Summary: {}", result.summary);
    println!("\n  Findings:");
    for finding in &result.findings {
        println!("    • {}", finding);
    }

    Ok(())
}
