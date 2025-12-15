//! Integration tests for performance optimization engine

use mcp_rs::ai::performance::{
    analyzer::{AnalysisSeverity, DefaultPerformanceAnalyzer, PerformanceAnalyzer},
    bottleneck::{BottleneckCategory, BottleneckDetector, DefaultBottleneckDetector, Severity},
    metrics::{MetricsHistory, SystemMetrics},
    optimizer::{DefaultOptimizationAdvisor, OptimizationAdvisor},
    recommendation::{DefaultRecommendationEngine, RecommendationEngine},
};

#[test]
fn test_end_to_end_performance_analysis() {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.88)
        .with_memory_usage(0.88)
        .with_cache_hit_rate(0.40);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics).unwrap();

    assert!(result.health_score < 0.8);
    assert!(!result.bottlenecks.is_empty());
    assert!(result.severity >= AnalysisSeverity::Warning);
}

#[test]
fn test_bottleneck_detection_pipeline() {
    let detector = DefaultBottleneckDetector::new();

    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.95)
        .with_memory_usage(0.90)
        .with_cache_hit_rate(0.25);

    let bottlenecks = detector.detect(&metrics);
    assert!(bottlenecks.len() >= 2); // CPU and Memory at minimum

    let prioritized = detector.prioritize(&bottlenecks);
    assert_eq!(prioritized.len(), bottlenecks.len());

    // Verify prioritization
    for i in 1..prioritized.len() {
        assert!(
            prioritized[i - 1].priority_score() >= prioritized[i].priority_score(),
            "Bottlenecks should be sorted by priority"
        );
    }
}

#[test]
fn test_optimization_report_generation() {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.92)
        .with_avg_query_time(300.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics).unwrap();

    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result).unwrap();

    assert!(!report.suggestions.is_empty());

    // Verify suggestions have priorities calculated
    for suggestion in &report.suggestions {
        assert!(suggestion.priority > 0);
    }
}

#[test]
fn test_recommendation_generation() {
    let metrics = SystemMetrics::new().with_avg_query_time(400.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics).unwrap();

    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result).unwrap();

    let engine = DefaultRecommendationEngine::new();
    let recommendations = engine.generate_recommendations(&report).unwrap();

    assert!(!recommendations.is_empty());

    for rec in &recommendations {
        assert!(!rec.title.is_empty());
        assert!(!rec.description.is_empty());
        assert!(!rec.steps.is_empty());
        assert!(rec.priority > 0);
    }
}

#[test]
fn test_quick_wins_identification() {
    let metrics = SystemMetrics::new()
        .with_cache_hit_rate(0.30)
        .with_avg_query_time(250.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics).unwrap();

    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result).unwrap();

    // Quick wins should exist for cache and query optimization
    if !report.quick_wins.is_empty() {
        for qw in &report.quick_wins {
            assert!(qw.expected_impact > 0.6);
            assert!(qw.difficulty < 0.4);
        }
    }
}

#[test]
fn test_historical_trend_detection() {
    let mut history = MetricsHistory::new(3600);

    // Add metrics showing increasing CPU usage trend
    for i in 0..10 {
        let cpu = 0.50 + (i as f64 * 0.05);
        let metrics = SystemMetrics::new().with_cpu_usage(cpu);
        history.add(metrics);
    }

    let cpu_trend = history.cpu_trend();
    assert!(cpu_trend > 0.0, "CPU trend should be positive");

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze_history(&history).unwrap();

    // Should detect the upward trend
    let has_trend_finding = result
        .findings
        .iter()
        .any(|f| f.contains("trending") || f.contains("trend"));
    assert!(has_trend_finding, "Should detect CPU trend");
}

#[test]
fn test_anomaly_detection() {
    let analyzer = DefaultPerformanceAnalyzer::new();

    // Normal metrics - fewer or no anomalies
    let normal_metrics = SystemMetrics::new()
        .with_cpu_usage(0.60)
        .with_memory_usage(0.70)
        .with_cache_hit_rate(0.70);

    let normal_anomalies = analyzer.detect_anomalies(&normal_metrics);

    // Extreme metrics - should detect more anomalies
    let extreme_metrics = SystemMetrics::new()
        .with_cpu_usage(0.98)
        .with_memory_usage(0.97);

    let extreme_anomalies = analyzer.detect_anomalies(&extreme_metrics);
    assert!(
        !extreme_anomalies.is_empty(),
        "Should detect anomalies in extreme metrics"
    );
    assert!(
        extreme_anomalies.len() > normal_anomalies.len(),
        "Extreme metrics should have more anomalies than normal metrics"
    );
}

#[test]
fn test_recommendation_prioritization() {
    let engine = DefaultRecommendationEngine::new();

    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.90)
        .with_memory_usage(0.88)
        .with_avg_query_time(300.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics).unwrap();

    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result).unwrap();

    let recommendations = engine.generate_recommendations(&report).unwrap();
    let prioritized = engine.prioritize_recommendations(&recommendations);

    // Verify prioritization
    for i in 1..prioritized.len() {
        assert!(
            prioritized[i - 1].priority >= prioritized[i].priority,
            "Recommendations should be sorted by priority"
        );
    }
}

#[test]
fn test_impact_estimate_calculation() {
    let metrics = SystemMetrics::new()
        .with_cache_hit_rate(0.30)
        .with_avg_query_time(350.0);

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result = analyzer.analyze(&metrics).unwrap();

    let advisor = DefaultOptimizationAdvisor::new();
    let report = advisor.generate_report(&result).unwrap();

    let engine = DefaultRecommendationEngine::new();
    let recommendations = engine.generate_recommendations(&report).unwrap();

    for rec in &recommendations {
        // Verify impact estimate is calculated
        let overall_score = rec.impact_estimate.overall_score();
        assert!(
            (0.0..=1.0).contains(&overall_score),
            "Overall score should be between 0 and 1"
        );

        // Verify ROI is calculated
        let roi = rec.impact_estimate.roi_score();
        assert!(roi >= 0.0, "ROI should be non-negative");
    }
}

#[test]
fn test_multiple_bottleneck_categories() {
    let metrics = SystemMetrics::new()
        .with_cpu_usage(0.85)
        .with_memory_usage(0.88)
        .with_cache_hit_rate(0.35)
        .with_avg_query_time(250.0);

    let detector = DefaultBottleneckDetector::new();
    let bottlenecks = detector.detect(&metrics);

    // Should detect multiple categories
    let categories: std::collections::HashSet<_> = bottlenecks.iter().map(|b| b.category).collect();

    assert!(
        categories.len() >= 2,
        "Should detect multiple bottleneck categories"
    );
}

#[test]
fn test_severity_escalation() {
    let detector = DefaultBottleneckDetector::new();

    // Medium severity
    let medium_metrics = SystemMetrics::new().with_cpu_usage(0.85);
    let medium_bottlenecks = detector.detect(&medium_metrics);
    let medium_severity = medium_bottlenecks
        .iter()
        .find(|b| b.category == BottleneckCategory::Cpu)
        .map(|b| b.severity);
    assert_eq!(medium_severity, Some(Severity::Medium));

    // High severity
    let high_metrics = SystemMetrics::new().with_cpu_usage(0.92);
    let high_bottlenecks = detector.detect(&high_metrics);
    let high_severity = high_bottlenecks
        .iter()
        .find(|b| b.category == BottleneckCategory::Cpu)
        .map(|b| b.severity);
    assert_eq!(high_severity, Some(Severity::High));

    // Critical severity
    let critical_metrics = SystemMetrics::new().with_cpu_usage(0.98);
    let critical_bottlenecks = detector.detect(&critical_metrics);
    let critical_severity = critical_bottlenecks
        .iter()
        .find(|b| b.category == BottleneckCategory::Cpu)
        .map(|b| b.severity);
    assert_eq!(critical_severity, Some(Severity::Critical));
}

#[test]
fn test_health_score_correlation() {
    let metrics_good = SystemMetrics::new()
        .with_cpu_usage(0.50)
        .with_memory_usage(0.60)
        .with_cache_hit_rate(0.80);

    let metrics_bad = SystemMetrics::new()
        .with_cpu_usage(0.90)
        .with_memory_usage(0.85)
        .with_cache_hit_rate(0.30);

    let score_good = metrics_good.health_score();
    let score_bad = metrics_bad.health_score();

    assert!(
        score_good > score_bad,
        "Good metrics should have higher health score"
    );

    let analyzer = DefaultPerformanceAnalyzer::new();
    let result_good = analyzer.analyze(&metrics_good).unwrap();
    let result_bad = analyzer.analyze(&metrics_bad).unwrap();

    assert!(
        result_good.bottlenecks.len() < result_bad.bottlenecks.len(),
        "Bad metrics should have more bottlenecks"
    );
}
