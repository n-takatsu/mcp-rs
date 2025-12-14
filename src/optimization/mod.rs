//! Optimization Module
//!
//! パフォーマンス最適化システム

pub mod advisor;

pub use advisor::{Bottleneck, BottleneckType, OptimizationAdvisor, OptimizationRecommendation};
