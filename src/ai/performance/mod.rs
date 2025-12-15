//! Performance optimization and analysis module
//!
//! This module provides automatic performance analysis, bottleneck detection,
//! and optimization recommendations for systems and applications.

pub mod analyzer;
pub mod bottleneck;
pub mod metrics;
pub mod optimizer;
pub mod recommendation;

pub use analyzer::{DefaultPerformanceAnalyzer, PerformanceAnalyzer};
pub use bottleneck::{Bottleneck, BottleneckCategory, BottleneckDetector, Severity};
pub use metrics::{MetricValue, MetricsHistory, SystemMetrics};
pub use optimizer::{DefaultOptimizationAdvisor, OptimizationAdvisor, OptimizationReport};
pub use recommendation::{
    DefaultRecommendationEngine, ImpactEstimate, Recommendation, RecommendationEngine,
};
